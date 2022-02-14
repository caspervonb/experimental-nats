# Experimental NATS client for Rust

This is an experimental playground for a new and improved client
implementation.

Before we implement this lets get down some design goals, implementing
it is easy but implementing it right the first time can be tricky.

## Problems

The implementation has few problems I'd like to solve, primarily the lock
contention and lack of parallelism but also including but not limited to the
following ones:

### No future

As of today, the `nats` crate has no support for futures and therefore no
support for async/await.

This leads to quite a few caveats, especially surrounding `select` which has
been reqested quite a bit.

This also means that a jetstream publish, which is effectivly a request/response
set blocks the entire program.

#### Proposal

First class support for futures, streams and sinks.

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


### No clear ownership semantics 

More or less everything in the crate is `Arc<Mutex>` which leads to the awkward
situation of no one owns anything.

This is an escape hatch, we shouldn't make it the default thing for all types
to have interior mutability.

##### Proposal

Be **very** deliberate with ownership semantics.


### No clear error semantics

We're a bit all over the place with errors in the current crate, it's not bad
but it could perhaps be better.

##### Proposal

Careful consideration when adding new error conditions.

## Design

The design is up for debate but a few notable runtime agnostic semantics and
types include the following:

### `nats::Connection`

A framed connection to a server (otherwise known as a transport), allows for direct unaltered access at the protocol level.

#### Examples

### `nats::Client`

An indirect handle to a connection which allows for the subscription and publication of messages.

This type communicates with connection via channels and does not hold an direct
reference to the connection but has one indirectly via an internal `Context`
type which owns the connection.

This context type would also hold a map of channels to subscription `sid`
as-well.

This type would probably be clonable.
#### Examples

### `nats::Subscription`

A stream of messages, implements the `Stream` trait.

This types communicates via channels and does not hold a direct reference to
the connection nor client.

#### Examples

```rust
let mut client = nats::connect("localhost:4222").await?;
let mut subscription = client.subscribe("foo").await?;

subscription.for_each(async move |message| {
  println!("Received message {:?}", message);
}).await;
```

## Questions

Some things to consider that have not yet been ironed out:

### Should we continue to use unbounded channels?

Unbound channels can grow forever and are most often the slowest kind of
channel in any implementation.

### Implicit vs explicit into

Open question on how magical we want conversion to be.
