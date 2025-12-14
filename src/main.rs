use std::task::Context;

use mpris_controller::PlayerFinder;
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
    let mut cx = Context::from_waker(&waker);

    let mut finder = PlayerFinder::new(&conn).await;
    finder.get_all(&conn).await.unwrap();

    let p = finder
        .get("org.mpris.MediaPlayer2.YoutubeMusic", &conn)
        .await
        .unwrap();

    println!("{p:?}");

    loop {
        finder.handle_players_changed(&mut cx).await;
        _ = finder.handle_owner_changed(&mut cx, &conn).await.unwrap();
    }
}
