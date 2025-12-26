use signalrs_client::{subject::StreamSubject, SignalRClient};
use tracing::*;
use tracing_subscriber::{self, filter, prelude::*};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    set_tracing_subscriber();

    let client = SignalRClient::builder("localhost")
        .use_port(5261)
        .use_hub("upload")
        .use_unencrypted_connection()
        .build()
        .await?;

    // Create a subject
    let subject = StreamSubject::new();

    // Clone for pushing items
    let pusher = subject.clone();

    // Start the invocation with the stream
    client
        .method("StreamEcho")
        .arg(subject.stream())?
        .send()
        .await?;

    info!("Started upload stream");

    // Spawn a task to push items incrementally
    tokio::spawn(async move {
        for i in 1..=5 {
            info!("Pushing item {}", i);
            if let Err(e) = pusher.push(format!("item-{}", i)).await {
                error!("Failed to push: {}", e);
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        info!("Completing stream");
        pusher.complete();
    });

    // Keep alive for a bit to let the stream complete
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    info!("Example finished");

    Ok(())
}

fn set_tracing_subscriber() {
    let targets_filter = filter::Targets::new()
        .with_target("signalrs", Level::TRACE)
        .with_default(Level::DEBUG);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_line_number(false)
        .with_file(false)
        .without_time()
        .compact();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(targets_filter)
        .init();
}
