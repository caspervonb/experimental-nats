use crate::connection::Connection;
use crate::Result;
use bytes::Bytes;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_stream::Stream;

/// Established connection with a NATS server.
pub struct Client {
    connection: Connection,
}

/// A subscription to one or more subjects
pub struct Subscription {
    connection: Connection,
}

pub struct Message {
    pub subject: String,
    pub payload: Bytes,
}

impl Subscription {}

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

    Ok(Client { connection })
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

    /// ```
    /// use tokio_nats::client;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut client = client::connect("localhost:4222").await.unwrap();
    ///     let mut subscription = client.subscribe("foo").await.unwrap();
    /// }
    pub async fn subscribe(self, subject: &str) -> Result<Subscription> {
        Ok(Subscription {
            connection: self.connection,
        })
    }
}
