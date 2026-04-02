use std::{
    io::{ErrorKind, Read},
    os::unix::net::UnixListener,
};

use lib::{Command, Format, MprisClient};
use prost::Message;

#[tokio::main]
async fn main() {
    let server = UnixListener::bind("/tmp/mpris-controller.sock").unwrap();
    let mut sockets: Vec<std::os::unix::net::UnixStream> = vec![];
    server.set_nonblocking(true).unwrap();

    let mut bytes = [0; 512];
    let mut index = 0;

    let mut client = MprisClient::new().await.unwrap();
    client.get_all().await.unwrap();

    loop {
        if let Some(stream) = sockets.get_mut(index) {
            match stream.read(&mut bytes) {
                Ok(amount) => {
                    if amount == 0 {
                        sockets.remove(index);
                    }
                    if amount > 0 {
                        println!("{sockets:?}");
                        let command = Format::decode(&bytes[0..amount]).unwrap();
                        if let Some(cmd) = command.command {
                            match cmd {
                                Command::GetPlayer(_) => todo!(),
                                Command::SetPlayer(_) => todo!(),
                            }
                        }
                        println!("{:?}", command);
                    }
                }

                Err(err) => {
                    if err.kind() != ErrorKind::WouldBlock {
                        panic!("{err:?}");
                    }
                }
            }
        } else {
            index = 0;
        }
        match server.accept() {
            Ok((sock, _addr)) => {
                sockets.push(sock);
            }
            Err(err) => {
                if err.kind() != ErrorKind::WouldBlock {
                    panic!("{err:?}");
                }
            }
        }
    }
}
