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
    Stop,
    TogglePause,
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

    let message = Server {
        command: Some(Command::GetPlayer(true)),
    };

    send_command(message, &mut bytes, &mut server);

    let mut buff = [0; 512];

    let player_name;

    loop {
        match server.read(&mut buff) {
            Ok(amt) => {
                let msg = Client::decode(&buff[0..amt]).unwrap();
                player_name = msg.current_player;
                break;
            }
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    panic!()
                }
            }
        }
    }

    match cli {
        Cli::Prev => {
            let playing = client.get(&player_name).unwrap().unwrap();
            let conn = zbus::Connection::session().await.unwrap();
            playing.prev(&conn).await;
        }
        Cli::After => {
            let playing = client.get(&player_name).unwrap().unwrap();
            let conn = zbus::Connection::session().await.unwrap();
            playing.next(&conn).await;
        }
        Cli::Stop => {
            let playing = client.get(&player_name).unwrap().unwrap();
            let conn = zbus::Connection::session().await.unwrap();
            playing.stop(&conn).await;
        }
        Cli::TogglePause => {
            println!("player name {player_name:?}");
            let playing = client.get(&player_name).unwrap().unwrap();
            let conn = zbus::Connection::session().await.unwrap();

            if let Err(e) = playing.pause_play(&conn).await {
                if e.description().unwrap().contains("PausePlay") {
                    match playing.capabilities.playback_status {
                        lib::player::PlaybackStatus::Stopped => playing.play(&conn).await,
                        lib::player::PlaybackStatus::Paused => playing.play(&conn).await,
                        lib::player::PlaybackStatus::Playing => playing.pause(&conn).await,
                    }
                }
            }
        }
        Cli::Players => {
            for player in client.player_names() {
                print!("{} ", player.name())
            }
            println!();
        }
        Cli::Playing => {
            let playing = client.get(&player_name).unwrap().unwrap();
            let title = playing.capabilities.metadata.title().unwrap_or("");
            let artists = playing.capabilities.metadata.artists();

            let url = playing.capabilities.metadata.url().unwrap_or("");
            print!("{} - ", title);
            if let Some(a) = artists {
                for a in a {
                    print!("{a} ");
                }
            }
            println!("{url}");
        }
        Cli::Url => {
            let playing = client.get(&player_name).unwrap().unwrap();
            let url = playing.capabilities.metadata.url().unwrap_or("");
            println!("{url}");
        }
    }
    //
    // info!(?client);
}
