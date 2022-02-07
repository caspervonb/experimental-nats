use crate::Result;
use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, BufWriter};
use tokio::net::TcpStream;

#[derive(Debug)]
pub struct Connection {
    stream: BufWriter<TcpStream>,
    buffer: BytesMut,
}

impl Connection {
    /// Create a new `Connection` backed by the given `socket`.
    pub fn new(socket: TcpStream) -> Connection {
        Connection {
            stream: BufWriter::new(socket),
            buffer: BytesMut::with_capacity(4 * 1024),
        }
    }

    ///
    pub fn write_publish(&mut self, subject: &str, message: Bytes) -> Result<()> {
        self.stream.write_all(b"PUB ").await?;
        self.stream.write_all(subject.as_bytes()).await?;
        self.stream.write_all(b" ").await?;
    }
}
