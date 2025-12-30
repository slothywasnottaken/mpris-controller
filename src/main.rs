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

    let mut client = MprisClient::new(&conn).await.unwrap();
    client.get_all(&conn).await.unwrap();

    info!(?client);

    loop {
        let ev = client.blocking_event(&conn).await;
        println!("event: {ev:?}");
    }
}
