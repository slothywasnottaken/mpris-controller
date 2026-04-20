use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixListener,
    time::Duration,
};

#[cfg(feature = "owner_changed")]
use lib::init_owner_changed_signal;

use lib::{Client, MprisClient, client::Message};
use prost::Message as _;
use tracing::{info, level_filters::LevelFilter};
use zbus::Connection;

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

    let mut client = MprisClient::new().unwrap();
    let conn = Connection::session().await.unwrap();
    client.get_all(&conn).await.unwrap();

    #[cfg(feature = "owner_changed")]
    init_owner_changed_signal().await;

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
                                    client.event(&conn).await;

                                    if player.is_none()
                                        && let Some(p) = client.currently_playing()
                                    {
                                        player = Some(client.get_id(p.name()).unwrap())
                                    }

                                    let msg = match player {
                                        None => Client {
                                            message: Some(Message::CouldNotFindPlayer(true)),
                                        },
                                        Some(p) => match client.get_from_id(p) {
                                            None => Client {
                                                message: Some(Message::CouldNotFindPlayer(true)),
                                            },
                                            Some(player) => Client {
                                                message: Some(Message::FocusedPlayer(
                                                    player.name().to_string(),
                                                )),
                                            },
                                        },
                                    };
                                    msg.encode(&mut send).unwrap();

                                    _ = sock.write(&send);
                                    send.clear();
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
