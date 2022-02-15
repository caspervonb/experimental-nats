/// An extremely minimal barebones draft of the proposed design.
use std::collections::HashMap;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use std::sync::Mutex;
use tokio::io;
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, Framed};

// FIXME: This is lazy, just for a quick and dirty compile.
type Error = Box<dyn std::error::Error>;

#[derive(Clone, Debug)]
pub enum ServerFrame {}

#[derive(Clone, Debug)]
pub enum ClientFrame {
    Publish { subject: String, payload: Bytes },
    Subscribe { sid: u64, subject: String },
    Unsubscribe { sid: u64 },
}

/// A codec for encoding and decoding the protocol.
pub struct Codec {}

impl Codec {
    fn new() -> Codec {
        Codec {}
    }
}

impl Decoder for Codec {
    type Item = String;
    type Error = tokio::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
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
    // Note: use of std mutex is
    subscription_context: Mutex<SubscriptionContext>,
}

impl Connector {
    pub fn new(connection: Connection) -> Connector {
        Connector {
            connection,
            subscription_context: Mutex::new(SubscriptionContext::new()),
        }
    }

    pub async fn run(&self, mut receiver: mpsc::Receiver<ClientFrame>) -> Result<(), io::Error> {
        loop {
            tokio::select! {
                frame = receiver.recv() => {
                    self.connection.framed.write(frame).await?
                }
            }
            // ...
        }

        Ok(())
    }
}

pub struct Client {
    // TODO; Probably won't have to the connector here (we don't have mut access to it anyway) but
    // instead just share the subscription context.
    connector: Arc<Connector>,
    sender: mpsc::Sender<ClientFrame>,
}

impl Client {
    pub fn new(connector: Arc<Connector>, sender: mpsc::Sender<ClientFrame>) -> Client {
        Client { connector, sender }
    }

    pub async fn publish(&mut self, subject: String, payload: Bytes) -> Result<(), Error> {
        self.sender
            .send(ClientFrame::Publish { subject, payload })
            .await?;

        Ok(())
    }

    pub async fn subscribe(&mut self, _subject: &str) -> Result<Subscriber, io::Error> {
        let (sender, receiver) = mpsc::channel(32);
        let mut subscripton_context = self.connector.subscription_context.lock().unwrap();

        let subscription_id = subscripton_context.insert(Subscription { sender });

        Ok(Subscriber::new(subscription_id, receiver))
    }
}

pub async fn connect<T: ToSocketAddrs>(addr: T) -> Result<Client, io::Error> {
    let connection = Connection::connect(addr).await?;
    let connector = Arc::new(Connector::new(connection));

    // TODO unbound?
    let (sender, receiver) = mpsc::channel(32);
    task::spawn({
        let connector = connector.clone();
        async move { connector.run(receiver).await }
    });

    let client = Client::new(connector, sender);

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
        // TODO send a unsub over a outgoing channell
    }
}
