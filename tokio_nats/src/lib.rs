use std::collections::HashMap;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
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

impl Encoder<String> for Decoder {
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
    pub async fn connect(addr: impl ToSocketAddrs) -> Result<Connection> {
        let stream = TcpStream::connect(addr).await?;
        let framed = Framed::new(stream, Codec::new());

        Ok(Connection { framed })
    }
}

/// A connector which facilitates communication from channels to a single shared connection.
/// The connector takes ownership of the channel.
pub struct Connector {
    connection: Connection,
    subscription_senders: Mutex<HashMap<u64, Sender>>,
}

impl Connector {
    pub fn new(connection: Connection) -> Connector {
        Connector { connection }
    }

    pub async fn run() -> Result<()> {
        loop {
            // ...
        }
    }
}

pub struct Client {
    connector: Arc<Connector>,
}

impl Client {
    pub fn new(connector: Arc<Connector>) -> Client {
        Client { connector }
    }
}

pub async fn connect<T: ToSocketAddrs>(addr: T) -> Result<Client> {
    let mut connection = Connection::connect(addr).await?;

    let mut connector = Arc::new(Connector::new(connection));
    task::spawn(async move { connector.run().await });

    let client = Client::new(connector);

    Ok(client)
}
