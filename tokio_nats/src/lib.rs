/// An extremely minimal barebones draft of the proposed design.
use std::collections::HashMap;
use std::sync::Arc;

use bytes::{Buf, Bytes, BytesMut};
use futures_util::future::FutureExt;
use futures_util::select;
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use std::sync::Mutex;
use tokio::io;
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tokio::sync::mpsc;
use tokio::task;
use tokio_util::codec::{Decoder, Encoder, Framed};

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

/// A codec for encoding and decoding the protocol.
pub struct Codec {}

impl Codec {
    fn new() -> Codec {
        Codec {}
    }
}

impl Decoder for Codec {
    type Item = ServerFrame;
    type Error = tokio::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.starts_with(b"PING\r\n") {
            src.advance(6);
            return Ok(Some(ServerFrame::Ping));
        }

        if src.starts_with(b"PONG\r\n") {
            src.advance(6);
            return Ok(Some(ServerFrame::Pong));
        }

        Ok(None)
    }
}

impl Encoder<ClientFrame> for Codec {
    type Error = std::io::Error;

    fn encode(&mut self, item: ClientFrame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            ClientFrame::Publish { subject, payload } => {
                dst.extend_from_slice(b"PUB ");
                dst.extend_from_slice(subject.as_bytes());
                dst.extend_from_slice(format!(" {}\r\n", payload.len()).as_bytes());
                dst.extend_from_slice(&payload);
                dst.extend_from_slice(b"\r\n");
            }

            ClientFrame::Subscribe { sid, subject } => {
                dst.extend_from_slice(b"SUB ");
                dst.extend_from_slice(subject.as_bytes());
                dst.extend_from_slice(format!(" {}\r\n", sid).as_bytes());
            }

            ClientFrame::Unsubscribe { sid } => {
                dst.extend_from_slice(b"UNSUB ");
                dst.extend_from_slice(format!("{}\r\n", sid).as_bytes());
            }
            ClientFrame::Ping => dst.extend_from_slice(b"PING\r\n"),
            ClientFrame::Pong => dst.extend_from_slice(b"PONG\r\n"),
        }

        Ok(())
    }
}

/// A framed connection
///
/// The type will probably not be public.
pub struct Connection {
    framed: Framed<TcpStream, Codec>,
}

impl Connection {
    pub async fn connect(addr: impl ToSocketAddrs) -> Result<Connection, io::Error> {
        let stream = TcpStream::connect(addr).await?;
        let framed = Framed::new(stream, Codec::new());

        Ok(Connection { framed })
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
        loop {
            select! {
                maybe_incoming = self.connection.framed.next().fuse() => {
                    match maybe_incoming {
                        Some(frame) => {
                            match frame {
                                Ok(ServerFrame::Ping) => {
                                   self.connection.framed.send(ClientFrame::Pong).await?;
                                }
                                Ok(ServerFrame::Pong) => {
                                    // TODO track last pong
                                },
                                Err(_) => {
                                    // TODO
                                },
                            }
                        }
                        None => {
                            // ...
                        }
                    }
                }

                maybe_outgoing = receiver.recv().fuse() => {
                    match maybe_outgoing {
                        Some(outgoing) => {
                            self.connection.framed.send(outgoing).await?
                        }
                        None => {
                            // Sender dropped, return.
                            break
                        }
                    }
                }
            }
            // ...
        }

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
