# Experimental NATS client for Rust

This is an experimental playground for a new and improved client
implementation.

## Problems

The implementation has few problems I'd like to solve, including but not
limited to the following ones:

## No future

As of today, the `nats` crate has no support for futures and therefore no
support for async/await.

This leads to quite a few caveats, especially surrounding `select` which has
been reqested quite a bit.

This also means that a jetstream publish, which is effectivly a request/response
set blocks the entire program.

#### Proposal

First class support for streams and sinks.

### Lock contention

In the current crate we have quite a few locks and nearly everything is
represented as `Arc<Mutex>` which in my opinion is quite nasty.

The most agregious is a read/write lock protocol which stalls everything.
This is especially noticable and slow if you do publishing and subscription in
the same program.

We also run into quite a few issues around `drop` implementations.

#### Proposal

We should respect borrow semantics where it makes sense, reduce locks to the
bare minimum and communicate over channels instead of sharing data.

`Drop` could be implemented as shutdown signals (one shot channels).

## Design

The design is up for debate but a few notable semantics and types include the
following:

### `nats::Connection`

A framed connection to a server, allows for direct unaltered access at the protocol level.

#### Examples

### `nats::Client`

An indirect handle to a connection which allows for the subscription and publication of messages.

This type communicates with connection via channels and does not hold an direct
reference to the connection.

#### Examples

### Subscription

A stream of messages, implements the `Stream` trait.

This types communicates via channels and does not hold a direct reference to
the connection nor client.

#### Examples

```rust
let client = nats::connect("localhost:4222").await?;
let subscription = client.subscribe("foo").await?;
```
