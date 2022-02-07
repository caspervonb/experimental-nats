use tokio_nats::client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = client::connect("localhost:4222").await?;
    client.publish("foo", "bar".into()).await?;

    let mut subscriber = client.subscribe("foo").await?;
    let mut messages = subscriber.into_stream();

    while let Some(message) = messages.next().await {
        println!("{:?}", message);
    }

    Ok(())
}
