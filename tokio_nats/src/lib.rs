/// An extremely minimal barebones draft of the proposed design.
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;

use bytes::{Buf, Bytes, BytesMut};
use futures_util::future::FutureExt;
use futures_util::select;
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
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
pub enum ServerFrame {
    Ping,
    Pong,
}

#[derive(Clone, Debug)]
pub enum ClientFrame {
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
        stream.set_nodelay(false)?;

        let (read_half, write_half) = stream.into_split();
        let writer = BufWriter::new(write_half);
        let reader = BufReader::new(read_half);

        Ok(Connection { reader, writer })
    }

    pub async fn write_frame(&mut self, item: &ClientFrame) -> Result<(), Error> {
        match item {
            ClientFrame::Publish { subject, payload } => {
                self.writer.write_all(b"PUB ");
                self.writer.write_all(b"PUB ");
                self.writer.write_all(subject.as_bytes());
                self.writer
                    .write_all(format!(" {}\r\n", payload.len()).as_bytes());
                self.writer.write_all(&payload);
                self.writer.write_all(b"\r\n");
            }

            ClientFrame::Subscribe { sid, subject } => {
                self.writer.write_all(b"SUB ");
                self.writer.write_all(subject.as_bytes());
                self.writer.write_all(format!(" {}\r\n", sid).as_bytes());
            }

            ClientFrame::Unsubscribe { sid } => {
                self.writer.write_all(b"UNSUB ");
                self.writer.write_all(format!("{}\r\n", sid).as_bytes());
            }
            ClientFrame::Ping => {
                self.writer.write_all(b"PING\r\n");
            }
            ClientFrame::Pong => {
                self.writer.write_all(b"PONG\r\n");
            }
        }

        Ok(())
    }

    pub async fn publish(&mut self, subject: String, payload: Bytes) -> Result<(), Error> {
        self.write_frame(&ClientFrame::Publish { subject, payload })
            .await?;

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
        mut receiver: mpsc::Receiver<ClientFrame>,
    ) -> Result<(), io::Error> {
        println!("Start processing");

        let mut line = String::new();

        loop {
            select! {
                _ = self.connection.reader.read_line(&mut line).fuse() => {
                    let op = line
                        .split_ascii_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase();

                        if op == "PING" {
                            self.connection.writer.write_all(b"PONG\r\n").await.unwrap();
                        }
                }

                maybe_outgoing = receiver.recv().fuse() => {
                    match maybe_outgoing {
                        Some(outgoing) => {
                            if let Err(err) = self.connection.write_frame(&outgoing).await {
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
            }
            // ...
        }

        println!("Graceful shutdown of processing");
        self.connection.writer.flush().await;

        Ok(())
    }
}

pub struct Client {
    sender: mpsc::Sender<ClientFrame>,
    subscription_context: Arc<Mutex<SubscriptionContext>>,
}

impl Client {
    pub(crate) fn new(
        sender: mpsc::Sender<ClientFrame>,
        subscription_context: Arc<Mutex<SubscriptionContext>>,
    ) -> Client {
        Client {
            sender,
            subscription_context,
        }
    }

    pub async fn publish(&mut self, subject: String, payload: Bytes) -> Result<(), Error> {
        self.sender
            .send(ClientFrame::Publish { subject, payload })
            .await?;

        Ok(())
    }

    pub async fn subscribe(&mut self, _subject: &str) -> Result<Subscriber, io::Error> {
        let (sender, receiver) = mpsc::channel(128);

        // Aiming to make this the only lock (aside from internal locks in channels).
        let mut subscripton_context = self.subscription_context.lock().unwrap();

        let subscription_id = subscripton_context.insert(Subscription { sender });

        Ok(Subscriber::new(subscription_id, receiver))
    }
}

pub async fn connect<T: ToSocketAddrs>(addr: T) -> Result<Client, io::Error> {
    let connection = Connection::connect(addr).await?;
    let subscription_context = Arc::new(Mutex::new(SubscriptionContext::new()));
    let mut connector = Connector::new(connection);

    // TODO unbound?
    let (sender, receiver) = mpsc::channel(128);
    let client = Client::new(sender, subscription_context.clone());

    task::spawn(async move { connector.process(receiver).await });

    Ok(client)
}

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
