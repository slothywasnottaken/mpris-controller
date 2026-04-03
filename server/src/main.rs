use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixListener,
    task::Poll,
    time::Duration,
};

use lib::{
    Client, MprisClient,
    player::{self, PlaybackStatus, Player, PlayerUpdated},
};
use prost::Message;
use tracing::{info, level_filters::LevelFilter};

#[tokio::main]
async fn main() {
    let _guard = tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(LevelFilter::INFO)
            .finish(),
    );
    let path = "/tmp/mpris-controller.sock";
    if std::fs::exists(path).unwrap() {
        std::fs::remove_file(path).unwrap();
    }
    let server = UnixListener::bind("/tmp/mpris-controller.sock").unwrap();
    server.set_nonblocking(true).unwrap();

    let mut bytes = [0; 512];
    let mut send = vec![];

    let mut client = MprisClient::new().await.unwrap();
    client.get_all().await.unwrap();

    let mut player = None;
    let mut socket = None;

    loop {
        match socket {
            None => match server.accept() {
                Ok((sock, _)) => {
                    sock.set_nonblocking(true).unwrap();
                    sock.set_read_timeout(Some(Duration::from_millis(100)))
                        .unwrap();
                    socket = Some(sock);
                }
                Err(e) => {
                    if e.kind() != ErrorKind::WouldBlock {
                        panic!("{e:?}");
                    }
                    continue;
                }
            },
            Some(ref mut sock) => match sock.read(&mut bytes) {
                Ok(amount) => {
                    if amount > 0 {
                        let message = lib::format::Server::decode(&bytes[0..amount]).unwrap();
                        if let Some(msg) = message.command {
                            info!("{msg:?}");
                            match msg {
                                lib::server::Command::SetFocusedPlayer(name) => {
                                    player = client.get(&name).unwrap()
                                }
                                lib::server::Command::GetPlayer(_) => {
                                    match player {
                                        Some(p) => {
                                            if p.capabilities().playback_status
                                                != PlaybackStatus::Playing
                                            {
                                                match client.currently_playing() {
                                                    Some(p) => {
                                                        player = Some(p);
                                                    }
                                                    None => panic!(),
                                                }
                                            }
                                        }
                                        None => match client.currently_playing() {
                                            Some(p) => {
                                                player = Some(p);
                                            }
                                            None => panic!(),
                                        },
                                    }

                                    if let Some(p) = player {
                                        let client_message = Client {
                                            get_focused_player: p.name().to_string(),
                                        };

                                        client_message.encode(&mut send).unwrap();

                                        _ = sock.write(&send);
                                    }
                                }
                                lib::server::Command::UnfocusPlayer(_) => loop {
                                    client.event().await;
                                    if let Some(p) = client.currently_playing() {
                                        player = Some(p);
                                        break;
                                    }
                                },
                            }
                        }
                    }
                    if amount == 0 {
                        println!("client disconnected");
                        socket = None;
                    }
                }

                Err(err) => {
                    if err.kind() != ErrorKind::WouldBlock {
                        info!("{err:?}");
                    }
                }
            },
        }
    }
}
