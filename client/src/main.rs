use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixStream,
    thread::sleep,
    time::Duration,
};

use clap::Parser;
use lib::{format, server::Command, Client, MprisClient, Server};
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

fn send_command(command: Server, buf: &mut Vec<u8>, socket: &mut UnixStream) {
    command.encode(buf).unwrap();

    socket.write_all(buf).unwrap();
}

#[tokio::main]
async fn main() {
    let mut server = std::os::unix::net::UnixStream::connect("/tmp/mpris-controller.sock").unwrap();

    let mut bytes = vec![];

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
    let mut client = MprisClient::new().await.unwrap();
    client.get_all().await.unwrap();
    println!("playing {:?}", client.currently_playing().unwrap().name());
    let data = "org.mpris.MediaPlayer2.YoutubeMusic";
    let message = Server {
        command: Some(Command::SetPlayer(data.to_string())),
    };

    send_command(message, &mut bytes, &mut server);

    let message = Server {
        command: Some(Command::GetPlayer(true)),
    };

    send_command(message, &mut bytes, &mut server);

    let mut buff = [0; 512];

    loop {
        match server.read(&mut buff) {
            Ok(amt) => {
                let msg = Client::decode(&buff[0..amt]).unwrap();
                println!("{msg:?}");
                break;
            }
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    panic!()
                }
            }
        }
    }

    let message = Server {
        command: Some(Command::PlayerStopped(true)),
    };

    send_command(message, &mut bytes, &mut server);

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
