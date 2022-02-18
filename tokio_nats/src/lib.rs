use futures_util::stream::Stream;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::pin::Pin;
use std::str::{self, FromStr};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;
use tokio::sync::mpsc::Receiver;

use bytes::{Buf, Bytes, BytesMut};
use futures_util::future::FutureExt;
use futures_util::select;
use std::sync::Mutex;
use tokio::io;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tokio::sync::mpsc;
use tokio::task;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug)]
pub enum ServerOp {
    Ping,
    Pong,
    Message {
        sid: u64,
        subject: String,
        reply_to: Option<String>,
        payload: Bytes,
    },
}

#[derive(Clone, Debug)]
pub enum ClientOp {
    Publish { subject: String, payload: Bytes },
    Subscribe { sid: u64, subject: String },
    Unsubscribe { sid: u64 },
    Ping,
    Pong,
}

/// A framed connection
///
/// The type will probably not be public.
pub struct Connection {
    writer: BufWriter<OwnedWriteHalf>,
    reader: BufReader<OwnedReadHalf>,
    buffer: BytesMut,
}

impl Connection {
    pub async fn connect(addr: impl ToSocketAddrs) -> Result<Connection, io::Error> {
        let tcp_stream = TcpStream::connect(addr).await?;
        tcp_stream.set_nodelay(true)?;

        let (read_half, write_half) = tcp_stream.into_split();
        let writer = BufWriter::new(write_half);
        let reader = BufReader::new(read_half);

        Ok(Connection {
            reader,
            writer,
            buffer: BytesMut::new(),
        })
    }

    pub async fn parse_op(&mut self) -> Result<Option<ServerOp>, io::Error> {
        Ok(None)
    }

    pub async fn read_op(&mut self) -> Result<Option<ServerOp>, Error> {
        loop {
            if let Some(frame) = self.parse_op().await? {
                return Ok(Some(frame));
            }

            if 0 == self.reader.read_buf(&mut self.buffer).await? {
                if self.buffer.is_empty() {
                    return Ok(None);
                } else {
                    return Err("connection reset by peer".into());
                }
            }
        }
    }

    pub async fn write_op(&mut self, item: &ClientOp) -> Result<(), Error> {
        match item {
            ClientOp::Publish { subject, payload } => {
                self.writer.write_all(b"PUB ").await?;
                self.writer.write_all(subject.as_bytes()).await?;
                self.writer
                    .write_all(format!(" {}\r\n", payload.len()).as_bytes())
                    .await?;
                self.writer.write_all(payload).await?;
                self.writer.write_all(b"\r\n").await?;
            }

            ClientOp::Subscribe { sid, subject } => {
                self.writer.write_all(b"SUB ").await?;
                self.writer.write_all(subject.as_bytes()).await?;
                self.writer
                    .write_all(format!(" {}\r\n", sid).as_bytes())
                    .await?;
            }

            ClientOp::Unsubscribe { sid } => {
                self.writer.write_all(b"UNSUB ").await?;
                self.writer
                    .write_all(format!("{}\r\n", sid).as_bytes())
                    .await?;
            }
            ClientOp::Ping => {
                self.writer.write_all(b"PING\r\n").await?;
            }
            ClientOp::Pong => {
                self.writer.write_all(b"PONG\r\n").await?;
            }
        }

        Ok(())
    }
}

struct Subscription {
    sender: mpsc::Sender<Message>,
}

struct SubscriptionContext {
    next_id: u64,
    subscription_map: HashMap<u64, Subscription>,
}

impl SubscriptionContext {
    pub fn new() -> SubscriptionContext {
        SubscriptionContext {
            next_id: 0,
            subscription_map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, subscription: Subscription) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.subscription_map.insert(id, subscription);

        id
    }
}

/// A connector which facilitates communication from channels to a single shared connection.
/// The connector takes ownership of the channel.
///
/// The type will probably not be public.
pub struct Connector {
    connection: Connection,
    // Note: use of std mutex is intentional, we never hold this across boundaries.
    subscription_context: Mutex<SubscriptionContext>,
}

impl Connector {
    pub(crate) fn new(connection: Connection) -> Connector {
        Connector {
            connection,
            subscription_context: Mutex::new(SubscriptionContext::new()),
        }
    }

    pub async fn process(
        &mut self,
        mut receiver: mpsc::Receiver<ClientOp>,
    ) -> Result<(), io::Error> {
        loop {
            select! {
                maybe_outgoing = receiver.recv().fuse() => {
                    match maybe_outgoing {
                        Some(outgoing) => {
                            if let Err(err) = self.connection.write_op(&outgoing).await {
                                println!("Send failed with {:?}", err);
                            }
                        }
                        None => {
                            println!("Sender closed");
                            // Sender dropped, return.
                            break
                        }
                    }
                }

                // TODO: make this internally stateful.
                maybe_client_op = self.connection.read_op().fuse() => {
                    println!("{:?}", maybe_client_op);
                }
            }
            // ...
        }

        println!("Graceful shutdown of processing");
        self.connection.writer.flush().await;

        Ok(())
    }
}

pub struct Client {
    sender: mpsc::Sender<ClientOp>,
    subscription_context: Arc<Mutex<SubscriptionContext>>,
}

impl Client {
    pub(crate) fn new(
        sender: mpsc::Sender<ClientOp>,
        subscription_context: Arc<Mutex<SubscriptionContext>>,
    ) -> Client {
        Client {
            sender,
            subscription_context,
        }
    }

    pub async fn publish(&mut self, subject: String, payload: Bytes) -> Result<(), Error> {
        self.sender
            .send(ClientOp::Publish { subject, payload })
            .await?;

        Ok(())
    }

    pub async fn subscribe(&mut self, subject: String) -> Result<Subscriber, io::Error> {
        let (sender, receiver) = mpsc::channel(16);

        // Aiming to make this the only lock (aside from internal locks in channels).
        let mut context = self.subscription_context.lock().unwrap();
        let sid = context.insert(Subscription { sender });

        self.sender
            .send(ClientOp::Subscribe { sid, subject })
            .await
            .unwrap();

        Ok(Subscriber::new(sid, receiver))
    }
}

pub async fn connect<T: ToSocketAddrs>(addr: T) -> Result<Client, io::Error> {
    let mut connection = Connection::connect(addr).await?;
    connection.writer.write_all(b"CONNECT { \"no_responders\": true, \"headers\": true, \"verbose\": false, \"pedantic\": false }\r\n").await?;

    let subscription_context = Arc::new(Mutex::new(SubscriptionContext::new()));
    let mut connector = Connector::new(connection);

    // TODO unbound?
    let (sender, receiver) = mpsc::channel(128);
    let client = Client::new(sender, subscription_context.clone());

    task::spawn(async move { connector.process(receiver).await });

    Ok(client)
}

#[derive(Debug)]
pub struct Message {
    subject: String,
    payload: Bytes,
}

pub struct Subscriber {
    sid: u64,
    receiver: mpsc::Receiver<Message>,
}

impl Subscriber {
    fn new(sid: u64, receiver: mpsc::Receiver<Message>) -> Subscriber {
        Subscriber { sid, receiver }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        // Can we get away with just closing, and then handling that on the sender side?
        self.receiver.close();
    }
}

impl Stream for Subscriber {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}
