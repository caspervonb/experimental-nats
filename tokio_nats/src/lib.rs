pub mod client;
pub mod connection;

/// Default port that a nats server listens on.
///
/// Used if no port is specified.
pub const DEFAULT_PORT: &str = "4222";

pub type Error = std::io::Error;

/// A specialized `Result` type for nats operations.
///
/// This is defined as a convenience.
pub type Result<T> = std::io::Result<T>;
