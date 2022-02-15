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
use tokio_util::codec::{Decoder, Encoder, Framed};

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

impl Encoder<String> for Codec {
    type Error = std::io::Error;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// A framed connection
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

        self.subscription_map.insert(id, subscription);
        self.next_id = self.next_id + 1;

        id
    }
}

/// A connector which facilitates communication from channels to a single shared connection.
/// The connector takes ownership of the channel.
pub struct Connector {
    connection: Connection,
    // Note: use of std mutex is
    subscription_context: Mutex<SubscriptionContext>,
    // Locks on individual fields, share the Connector as an Arc rather than an Arc<Mutex>
}

impl Connector {
    pub fn new(connection: Connection) -> Connector {
        Connector {
            connection,
            subscription_context: Mutex::new(SubscriptionContext::new()),
        }
    }

    pub async fn run(&self) -> Result<(), io::Error> {
        loop {
            // ...
        }

        Ok(())
    }
}

pub struct Client {
    connector: Arc<Connector>,
}

impl Client {
    pub fn new(connector: Arc<Connector>) -> Client {
        Client { connector }
    }

    pub async fn publish(&mut self, _subject: &str, _bytes: Bytes) -> Result<(), io::Error> {
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

    task::spawn({
        let connector = connector.clone();
        async move { connector.run().await }
    });

    let client = Client::new(connector);

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
        // TODO
    }
}
