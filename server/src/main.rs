use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixListener,
    time::Duration,
};

use lib::{Client, MprisClient, player::PlaybackStatus};
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
                                    player = client.get_id(&name)
                                }
                                lib::server::Command::GetPlayer(_) => {
                                    client.event().await;
                                    println!("{}", player.is_some());
                                    match player {
                                        Some(p) => {
                                            if let Some(curent) = client.currently_playing()
                                                && let Some(p) = player
                                            {
                                                let real = client.get_from_id(p).unwrap();
                                                if curent.name() != real.name() {
                                                    player = client.get_id(curent.name());
                                                }
                                            }
                                            let client_message = Client {
                                                message: Some(lib::client::Message::FocusedPlayer(
                                                    client
                                                        .get_from_id(p)
                                                        .unwrap()
                                                        .name()
                                                        .to_string(),
                                                )),
                                            };

                                            client_message.encode(&mut send).unwrap();

                                            _ = sock.write(&send);
                                            send.clear();
                                        }

                                        None => {
                                            match client.currently_playing() {
                                                Some(p) => {
                                                    player = client.get_id(p.name());
                                                }
                                                None => {
                                                    let client_message = Client {
                                            message: Some(lib::client::Message::CouldNotFindPlayer(
                                                true,
                                            ))
                                                };

                                                    client_message.encode(&mut send).unwrap();

                                                    info!("sent {:?}", send.len());
                                                    _ = sock.write(&send);
                                                    send.clear();
                                                }
                                            }
                                        }
                                    }
                                }
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
                        socket = None;
                    }
                }
            },
        }
    }
}
