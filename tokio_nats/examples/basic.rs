use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut client = tokio_nats::connect("localhost:4222").await?;

    let now = Instant::now();
    for i in 0..10_000_000 {
        client.publish("foo".into(), i.to_string().into()).await?;
    }

    println!("published in {:?}", now.elapsed());

    // client
    //    .subscribe("MyOtherSubject")
    //    .await?
    //     .for_each(move |message| {
    //        println!("Received message {:?}", message);
    //   })
    //  .await;

    Ok(())
}
