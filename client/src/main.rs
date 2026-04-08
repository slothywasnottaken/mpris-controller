use std::{
    fmt::Write as _,
    io::{ErrorKind, Read, Write},
    os::unix::net::UnixStream,
};

use clap::Parser;
use lib::{Client, MprisClient, Server, server::Command};
use prost::Message;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

#[derive(Debug, clap::Parser)]
enum Cli {
    Players,
    Playing,
    Prev,
    After,
    Stop,
    TogglePause,
    Pause,
    Play,
    Url,
    Metadata(MetadataCommand),
}

#[derive(Debug, clap::Parser)]
struct MetadataCommand {
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    art_url: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    length: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    trackid: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    album: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    artists: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    title: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    url: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    track_number: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    disc_number: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    auto_rating: bool,
    // #[arg(long, default_value_t = true)]
    #[arg(long)]
    album_artists: bool,
}

impl Default for MetadataCommand {
    fn default() -> Self {
        Self {
            art_url: true,
            length: true,
            trackid: true,
            album: true,
            artists: true,
            title: true,
            url: true,
            track_number: true,
            disc_number: true,
            auto_rating: true,
            album_artists: true,
        }
    }
}

fn send_command(command: Server, buf: &mut Vec<u8>, socket: &mut UnixStream) {
    command.encode(buf).unwrap();

    socket.write_all(buf).unwrap();
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    // std::panic::set_hook(Box::new(|panic_info| {
    //     println!("panic occurred: {panic_info}");
    // }));
    //
    let mut server = std::os::unix::net::UnixStream::connect("/tmp/mpris-controller.sock").unwrap();
    // server.set_nonblocking(true).unwrap();
    //
    let mut bytes = vec![];

    // let user = std::env::home_dir().unwrap();
    // let user = user.to_str().unwrap();
    // let path = format!("{user}/.local/share/log");
    // let file = std::fs::OpenOptions::new()
    //     .write(true)
    //     .create(true)
    //     .truncate(false)
    //     .open(&path)
    //     .expect("truncating log file failed");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("client=trace"))
        .with_span_events(FmtSpan::FULL)
        // .with_writer(file)
        // .with_ansi(false)
        .init();
    let mut client = MprisClient::new().await.unwrap();
    client.get_all().await.unwrap();

    let mut buff = [0; 512];

    let mut player_name: Option<String> = None;

    let message = Server {
        command: Some(Command::GetPlayer(true)),
    };
    send_command(message, &mut bytes, &mut server);

    loop {
        match server.read(&mut buff) {
            Ok(amt) => {
                let msg = Client::decode(&buff[..amt]).unwrap();
                match msg.message {
                    Some(msg) => match msg {
                        lib::client::Message::FocusedPlayer(focused) => {
                            info!("name {:?}", focused.len());
                            player_name = Some(focused);
                            break;
                        }
                        lib::client::Message::CouldNotFindPlayer(_) => {
                            println!("Could not find player");
                            break;
                        }
                    },
                    None => todo!(),
                }
            }
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    panic!()
                }
            }
        }
    }

    if let Some(player_name) = player_name {
        match cli {
            Cli::Prev => {
                let playing = client.get(&player_name).unwrap();
                let conn = zbus::Connection::session().await.unwrap();
                playing.prev(&conn).await;
            }
            Cli::After => {
                let playing = client.get(&player_name).unwrap();
                let conn = zbus::Connection::session().await.unwrap();
                playing.next(&conn).await;
            }
            Cli::Stop => {
                let playing = client.get(&player_name).unwrap();
                let conn = zbus::Connection::session().await.unwrap();
                playing.stop(&conn).await;
            }
            Cli::TogglePause => {
                println!("player name {player_name:?}");
                let playing = client.get(&player_name).unwrap();
                let conn = zbus::Connection::session().await.unwrap();

                match playing.capabilities.playback_status {
                    lib::player::PlaybackStatus::Stopped => playing.play(&conn).await,
                    lib::player::PlaybackStatus::Paused => playing.play(&conn).await,
                    lib::player::PlaybackStatus::Playing => {
                        playing.pause(&conn).await;

                        let message = Server {
                            command: Some(Command::UnfocusPlayer(true)),
                        };

                        send_command(message, &mut bytes, &mut server);
                    }
                }
            }
            Cli::Pause => {
                println!("player name {player_name:?}");
                let playing = client.get(&player_name).unwrap();
                let conn = zbus::Connection::session().await.unwrap();
                playing.pause(&conn).await;

                let message = Server {
                    command: Some(Command::UnfocusPlayer(true)),
                };
                send_command(message, &mut bytes, &mut server);
            }
            Cli::Play => {
                println!("player name {player_name:?}");
                let playing = client.get(&player_name).unwrap();
                let conn = zbus::Connection::session().await.unwrap();
                playing.play(&conn).await;

                let message = Server {
                    command: Some(Command::UnfocusPlayer(true)),
                };
                send_command(message, &mut bytes, &mut server);
            }
            Cli::Players => {
                for player in client.player_names() {
                    print!("{} ", player.name())
                }
                println!();
            }
            Cli::Playing => {
                let playing = client.get(&player_name).unwrap();
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
                let playing = client.get(&player_name).unwrap();
                let url = playing.capabilities.metadata.url().unwrap_or("");
                println!("{url}");
            }
            Cli::Metadata(data) => {
                let mut fmt = String::new();
                let playing = client.get(&player_name).unwrap();
                let metadata = &playing.capabilities().metadata;
                if data.art_url
                    && let Some(art_url) = metadata.art_url()
                {
                    print!("{art_url} ");
                }
                if data.length {
                    let mut printable_len = [0; 3];
                    match metadata.length() {
                        None => fmt.write_char(' ').unwrap(),
                        Some(len) => {
                            let total_secs = len / 1_000_000;
                            let mut minutes = total_secs / 60;
                            let hours = minutes / 60;
                            if minutes >= 60 {
                                minutes %= 60;
                            }
                            let secs = (total_secs % 60) as u8;

                            // info!(?hours, minutes, ?secs);

                            printable_len[0] = secs;
                            printable_len[1] = minutes as u8;
                            printable_len[2] = hours as u8;

                            if printable_len[2] < 10 {
                                print!("0{}:", printable_len[2]);
                            } else {
                                print!("{}:", printable_len[2]);
                            }

                            if printable_len[1] < 10 {
                                print!("0{}:", printable_len[1]);
                            } else {
                                print!("{}:", printable_len[1]);
                            }

                            if printable_len[0] < 10 {
                                print!("0{} ", printable_len[0]);
                            } else {
                                print!("{} ", printable_len[0]);
                            }
                        }
                    }
                }

                if data.trackid
                    && let Some(id) = metadata.track_id()
                {
                    print!("{id} ");
                }
                if data.album
                    && let Some(album) = metadata.album()
                {
                    print!("{album} ");
                }
                if data.artists
                    && let Some(artists) = metadata.artists()
                {
                    if artists.len() == 1 {
                        print!("{} ", artists[0]);
                    } else {
                        for (i, art) in artists.iter().enumerate() {
                            if i >= artists.len() {
                                print!("{art} ")
                            } else {
                                print!("{art}, ")
                            }
                        }
                    }
                }
                if data.title
                    && let Some(title) = metadata.title()
                {
                    print!("{title} ");
                }
                if data.url
                    && let Some(url) = metadata.url()
                {
                    print!("{url} ");
                }
                if data.track_number
                    && let Some(number) = metadata.track_number()
                {
                    print!("{number} ");
                }
                if data.auto_rating
                    && let Some(rating) = metadata.auto_rating()
                {
                    print!("{rating} ");
                }
                if data.album_artists
                    && let Some(artists) = metadata.album_artists()
                {
                    if artists.len() == 1 {
                        print!("{} ", artists[0]);
                    } else {
                        for (i, art) in artists.iter().enumerate() {
                            if i >= artists.len() {
                                print!("{art} ")
                            } else {
                                print!("{art}, ")
                            }
                        }
                    }
                }
                println!();
            }
        }
    }
}
