use std::{
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixListener,
};

use lib::{Client, MprisClient};
use prost::Message;
use tracing::{info, level_filters::LevelFilter};

#[tokio::main]
async fn main() {
    let _guard = tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(LevelFilter::INFO)
            .finish(),
    );
    let server = UnixListener::bind("/tmp/mpris-controller.sock").unwrap();

    let mut bytes = [0; 512];
    let mut send = vec![];

    let mut client = MprisClient::new().await.unwrap();
    client.get_all().await.unwrap();

    let mut player = client.currently_playing();
    let (mut socket, _) = server.accept().unwrap();

    loop {
        match socket.read(&mut bytes) {
            Ok(amount) => {
                if amount > 0 {
                    let message = lib::format::Server::decode(&bytes[0..amount]).unwrap();
                    info!("{message:?}");
                    if let Some(msg) = message.command {
                        info!("{msg:?}");
                        match msg {
                            lib::server::Command::SetPlayer(name) => {
                                player = client.get(&name).unwrap()
                            }
                            lib::server::Command::GetPlayer(_) => {
                                let client_message = Client {
                                    current_player: player.unwrap().name().to_string(),
                                };

                                client_message.encode(&mut send).unwrap();

                                _ = socket.write(&send);
                            }
                            lib::server::Command::PlayerStopped(_) => player = None,
                        }
                    }
                }
                if amount == 0 {
                    (socket, _) = server.accept().unwrap();
                    info!("client disconnected");
                }
            }

            Err(err) => {
                if err.kind() != ErrorKind::WouldBlock {
                    info!("{err:?}");
                }
            }
        }
    }
}
