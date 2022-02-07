# Experimental NATS

This is an experimental playground for a new asynchronous NATS implementation with heavy
focus on staying true to Rust borrow semantics and reducing locks.

In other words, lets try to not make everything
`Arc<Mutex<SharedGlobalState>>`.
