pub mod client;
pub mod connection;
pub mod message;

/// Default port that a nats server listens on.
///
/// Used if no port is specified.
pub const DEFAULT_PORT: &str = "4222";

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// A specialized `Result` type for nats operations.
///
/// This is defined as a convenience.
pub type Result<T> = std::result::Result<T, Error>;
