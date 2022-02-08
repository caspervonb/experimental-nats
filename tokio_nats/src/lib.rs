//! ```
//! use tokio_nats::client;
//! use tokio_stream::StreamExt;
//!
//! async fn main() -> {
//!     let mut client = connect("localhost:4222").await?;
//!
//!     client.publish("MY_SUBJECT", "hello world".as_bytes()).await?;
//!     let my_subject_subscription = client.subscribe("MY_SUBJECT").await?;
//! }
//! ```
pub mod client;
pub mod connection;

/// Default port that a nats server listens on.
pub const DEFAULT_PORT: &str = "4222";

/// Error type used for nats operations.
pub type Error = tokio::io::Error;

/// A specialized `Result` type for nats operations.
///
/// This is defined as a convenience.
pub type Result<T> = std::io::Result<T>;
