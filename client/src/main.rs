use std::io::{ErrorKind, Read, Write};

use clap::Parser;
use lib::{format, Format, MprisClient};
use prost::Message;
use tracing::info;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[derive(Debug, clap::Parser)]
enum Cli {
    Players,
    Playing,
    Prev,
    After,
    Url,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let user = std::env::home_dir().unwrap();
    let user = user.to_str().unwrap();
    let path = format!("{user}/.local/share/log");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("truncating log file failed");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("mpris_controller=trace"))
        .with_span_events(FmtSpan::FULL)
        .with_writer(file)
        .with_ansi(false)
        .init();

    let mut bytes = vec![];

    let format = Format {
        command: Some(lib::Command::SetPlayer(String::from(
            "org.mpris.MediaPlayer2.YoutubeMusic",
        ))),
    };

    format.encode(&mut bytes).unwrap();

    let m = Format::decode(&*bytes).unwrap();

    println!("{m:?}");

    let mut server = std::os::unix::net::UnixStream::connect("/tmp/mpris-controller.sock").unwrap();
    server.set_nonblocking(true).unwrap();
    _ = server.write(&bytes);

    // let mut client = MprisClient::new().await.unwrap();
    // client.get_all().await.unwrap();
    //
    // match cli {
    //     Cli::Prev => {}
    //     Cli::After => {}
    //     Cli::Players => {
    //         for player in client.player_names() {
    //             print!("{} ", player.name())
    //         }
    //         println!();
    //     }
    //     Cli::Playing => {
    //         for playing in client.currently_playing() {
    //             let title = playing.capabilities.metadata.title().unwrap_or("");
    //             let artists = playing.capabilities.metadata.artists();
    //
    //             let url = playing.capabilities.metadata.url().unwrap_or("");
    //             print!("{} - ", title);
    //             if let Some(a) = artists {
    //                 for a in a {
    //                     print!("{a} ");
    //                 }
    //             }
    //             println!("{url}");
    //         }
    //     }
    //     Cli::Url => {
    //         for playing in client.currently_playing() {
    //             let url = playing.capabilities.metadata.url().unwrap_or("");
    //             println!("{url}");
    //         }
    //     }
    // }
    //
    // info!(?client);
}
