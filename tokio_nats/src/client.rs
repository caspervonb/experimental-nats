use crate::connection::Connection;
use crate::Result;
use bytes::Bytes;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

/// A map of subscriptions to broadcast sender
type SubscriptionMap<T> = HashMap<u64, broadcast::Sender<T>>;

/// An established connection with a NATS server.
#[derive(Debug)]
pub struct Client {
    connection: Connection,
    subscriptions: SubscriptionMap<Message>,
    next_subscription_id: u64,
}

/// A NATS message
#[derive(Debug, Clone)]
pub struct Message {
    pub subject: String,
    pub payload: Bytes,
}

/// A subscription to one or more subjects
///
/// TODO(caspervonb): Figure out if a subscription should be a stream or if is should only be convertible into one.
/// Having `into_stream` makes sense in cases where we want to swallow some messages like jetstream
/// subscriptions.
#[derive(Debug)]
pub struct Subscription {
    id: u64,
    receiver: broadcast::Receiver<Message>,
}

// TODO implement drop

impl Subscription {
    pub fn into_stream(self) -> BroadcastStream<Message> {
        BroadcastStream::new(self.receiver)
    }
}

/// Establish a connection with the NATS server located at `addr`.
///
/// # Examples
///
/// ```
/// use tokio_nats::client;
///
/// #[tokio::main]
/// async fn main() {
///     let client = match client::connect("localhost:4222").await {
///         Ok(client) => client,
///         Err(_) => panic!("failed to establish connection"),
///     };
/// # drop(client);
/// }
/// ```
///
pub async fn connect<T: ToSocketAddrs>(addr: T) -> Result<Client> {
    let socket = TcpStream::connect(addr).await?;
    let connection = Connection::new(socket);

    Ok(Client {
        connection,
        subscriptions: SubscriptionMap::new(),
        next_subscription_id: 1,
    })
}

impl Client {
    /// ```
    /// use tokio_nats::client;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut client = client::connect("localhost:4222").await.unwrap();
    ///     client.publish("foo", "bar".into()).await.unwrap();
    /// }
    /// ```
    ///
    pub async fn publish(&mut self, subject: &str, payload: Bytes) -> Result<()> {
        Ok(())
    }

    /// Subscribe to the given `subject` which may be either a topic or a pattern.
    ///
    /// ```
    /// use tokio_nats::client;
    /// use tokio_stream::{StreamExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut client = client::connect("localhost:4222").await.unwrap();
    ///     # client.publish("foo", "bar");
    ///     let mut subscription = client.subscribe("foo").await.unwrap();
    /// }
    pub async fn subscribe(&mut self, subject: &str) -> Result<Subscription> {
        let id = self.next_subscription_id;
        self.next_subscription_id += 1;

        let (sender, receiver) = broadcast::channel(32);
        self.subscriptions.insert(id, sender);

        Ok(Subscription { id, receiver })
    }

    /// Process all incoming messages until the socket hangs up.
    pub async fn process(&mut self) -> Result<()> {
        // TODO
        Ok(())
    }
}
