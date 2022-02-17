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

// FIXME: This is lazy, just for a quick and dirty compile.
type Error = Box<dyn std::error::Error>;

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
}

impl Connection {
    pub async fn connect(addr: impl ToSocketAddrs) -> Result<Connection, io::Error> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;

        let (read_half, write_half) = stream.into_split();
        let writer = BufWriter::new(write_half);
        let reader = BufReader::new(read_half);

        Ok(Connection { reader, writer })
    }

    pub async fn read_op(&mut self) -> Result<Option<ServerOp>, io::Error> {
        // Quick and dirty port from nats.rs
        // Lots of room for improvement here.
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }

        let keyword = line
            .split_ascii_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();

        if keyword == "PING" {
            return Ok(Some(ServerOp::Ping));
        }

        if keyword == "PONG" {
            return Ok(Some(ServerOp::Pong));
        }

        if keyword == "MSG" {
            // FIXME: Copied from nats.rs but this is pretty bad with the extra allocations.
            // Extract whitespace-delimited arguments that come after "MSG".
            let args = line["MSG".len()..]
                .split_whitespace()
                .filter(|s| !s.is_empty());
            let args = args.collect::<Vec<_>>();

            // Parse the operation syntax: MSG <subject> <sid> [reply-to] <#bytes>
            let (subject, sid, reply_to, num_bytes) = match args[..] {
                [subject, sid, num_bytes] => (subject, sid, None, num_bytes),
                [subject, sid, reply_to, num_bytes] => (subject, sid, Some(reply_to), num_bytes),
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "invalid number of arguments after MSG",
                    ));
                }
            };

            // Convert the slice into an owned string.
            let subject = subject.to_owned();

            // Parse the subject ID.
            let sid = u64::from_str(sid).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot parse sid argument after MSG",
                )
            })?;

            // Convert the slice into an owned string.
            let reply_to = reply_to.map(String::from);

            // Parse the number of payload bytes.
            let num_bytes = u32::from_str(num_bytes).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot parse the number of bytes argument after MSG",
                )
            })?;

            // Read the payload.
            let mut payload = Vec::new();
            payload.resize(num_bytes as usize, 0_u8);
            self.reader.read_exact(&mut payload[..]).await?;
            // Read "\r\n".
            self.reader.read_exact(&mut [0_u8; 2]).await?;

            return Ok(Some(ServerOp::Message {
                subject,
                sid,
                reply_to,
                payload: Bytes::from(payload),
            }));
        }

        Ok(None)
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
