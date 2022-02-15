use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut client = tokio_nats::connect("localhost:4222").await?;

    for i in 0..1_000_000 {
        client.publish("foo", i.to_string().into()).await?;
    }

    // client
    //    .subscribe("MyOtherSubject")
    //    .await?
    //     .for_each(move |message| {
    //        println!("Received message {:?}", message);
    //   })
    //  .await;

    Ok(())
}
