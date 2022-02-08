# Experimental NATS

This is an experimental playground for a new asynchronous NATS implementation.

Things to consider:

- Lets not throw away borrow semantics and make everything `Arc<Mutex>` by
  default. We should also try to reduce the number of mutex locks used as much
as possible.

- Lets not implicitly convert by default, the `Into` trait provides conversions
  but in Rust conversions tend to be explicit by design. 

- Subscriptions should not take a mutable reference to the connection but
  instead communicate via channels. This also applies to `Drop`
implementations.

- Subscriptions should implement the `Stream` trait.
