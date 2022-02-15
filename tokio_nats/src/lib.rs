/// An extremely minimal barebones draft of the proposed design.
use std::collections::HashMap;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use tokio::io;
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

/// A connector which facilitates communication from channels to a single shared connection.
/// The connector takes ownership of the channel.
pub struct Connector {
    connection: Connection,
}

impl Connector {
    pub fn new(connection: Connection) -> Connector {
        Connector { connection }
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

pub struct Subscription {
    message_rx: mpsc::Receiver<Message>,
}

// impl Stream for Subscription { }
