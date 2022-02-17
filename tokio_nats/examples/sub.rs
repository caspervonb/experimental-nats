use bytes::Bytes;
use futures_util::StreamExt;
use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut client = tokio_nats::connect("localhost:4222").await?;

    client.publish("foo".into(), "bar".into()).await?;

    let now = Instant::now();
    let mut subscriber = client.subscribe("foo".into()).await.unwrap();
    client.publish("foo".into(), "hello".into()).await?;
    while let Some(message) = subscriber.next().await {
        println!("Received message {:?}", message);
    }

    println!("subscriber received in {:?}", now.elapsed());

    Ok(())
}
