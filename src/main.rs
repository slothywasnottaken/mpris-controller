use std::task::Context;

use mpris_controller::MprisClient;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

#[tokio::main]
async fn main() {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("log")
        .expect("truncating log file failed");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("mpris_controller=trace"))
        .with_span_events(FmtSpan::FULL)
        .with_writer(file)
        .with_ansi(false)
        .init();

    let conn = zbus::Connection::session().await.unwrap();

    let waker = futures::task::noop_waker();
    let mut ctx = Context::from_waker(&waker);

    let mut client = MprisClient::new(&conn).await.unwrap();
    client.get_all(&conn).await.unwrap();

    info!(?client);

    loop {
        if let Some(ev) = client.event(&mut ctx, &conn).await {
            println!("event: {ev:?}");
        }
    }
}
