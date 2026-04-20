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
use zbus::Connection;

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

// #[derive(Debug)]
// struct P {
//     p: Capabilities,
// }
//
// #[allow(unused)]
// #[async_trait::async_trait]
// impl Interface for P {
//     #[doc = " Return the name of the interface. Ex: \"org.foo.MyInterface\""]
//     fn name() -> InterfaceName<'static>
//     where
//         Self: Sized,
//     {
//         InterfaceName::from_static_str_unchecked("org.mpris.MediaPlayer2.Player")
//     }
//
//     #[doc = " Get a property value. Returns `None` if the property doesn\'t exist."]
//     #[doc = ""]
//     #[doc = " Note: The header parameter will be None when the getter is not being called as part"]
//     #[doc = " of D-Bus communication (for example, when it is called as part of initial object setup,"]
//     #[doc = " before it is registered on the bus, or when we manually send out property changed"]
//     #[doc = " notifications)."]
//     #[must_use]
//     #[allow(
//         mismatched_lifetime_syntaxes,
//         clippy::type_complexity,
//         clippy::type_repetition_in_bounds
//     )]
//     async fn get(
//         &self,
//         property_name: &str,
//         server: &ObjectServer,
//         connection: &Connection,
//         header: Option<&message::Header<'_>>,
//         emitter: &SignalEmitter<'_>,
//     ) -> Option<FdoResult<OwnedValue>>
//     where
//         'life0: 'async_trait,
//         'life1: 'async_trait,
//         'life2: 'async_trait,
//         'life3: 'async_trait,
//         'life4: 'async_trait,
//         'life5: 'async_trait,
//         'life6: 'async_trait,
//         'life7: 'async_trait,
//         Self: 'async_trait,
//     {
//         todo!()
//     }
//
//     #[doc = " Return all the properties."]
//     #[must_use]
//     #[allow(
//         mismatched_lifetime_syntaxes,
//         clippy::type_complexity,
//         clippy::type_repetition_in_bounds
//     )]
//     async fn get_all(
//         &self,
//         object_server: &ObjectServer,
//         connection: &Connection,
//         header: Option<&message::Header<'_>>,
//         emitter: &SignalEmitter<'_>,
//     ) -> fdo::Result<HashMap<String, OwnedValue>> {
//         let map: HashMap<String, OwnedValue> = self.p.clone().into();
//
//         println!("get_all {map:#?}");
//
//         return Ok(map);
//     }
//
//     #[doc = " Set a property value."]
//     #[doc = ""]
//     #[doc = " Returns `None` if the property doesn\'t exist."]
//     #[doc = ""]
//     #[doc = " This will only be invoked if `set` returned `RequiresMut`."]
//     #[must_use]
//     #[allow(
//         mismatched_lifetime_syntaxes,
//         clippy::type_complexity,
//         clippy::type_repetition_in_bounds
//     )]
//     fn set_mut<
//         'life0,
//         'life1,
//         'life2,
//         'life3,
//         'life4,
//         'life5,
//         'life6,
//         'life7,
//         'life8,
//         'life9,
//         'async_trait,
//     >(
//         &'life0 mut self,
//         property_name: &'life1 str,
//         value: &'life2 Value<'life3>,
//         object_server: &'life4 ObjectServer,
//         connection: &'life5 Connection,
//         header: Option<&'life6 Header<'life7>>,
//         emitter: &'life8 SignalEmitter<'life9>,
//     ) -> ::core::pin::Pin<
//         Box<
//             dyn ::core::future::Future<Output = Option<fdo::Result<()>>>
//                 + ::core::marker::Send
//                 + 'async_trait,
//         >,
//     >
//     where
//         'life0: 'async_trait,
//         'life1: 'async_trait,
//         'life2: 'async_trait,
//         'life3: 'async_trait,
//         'life4: 'async_trait,
//         'life5: 'async_trait,
//         'life6: 'async_trait,
//         'life7: 'async_trait,
//         'life8: 'async_trait,
//         'life9: 'async_trait,
//         Self: 'async_trait,
//     {
//         todo!()
//     }
//
//     #[doc = " Call a method."]
//     #[doc = ""]
//     #[doc = " Return [`DispatchResult::NotFound`] if the method doesn\'t exist, or"]
//     #[doc = " [`DispatchResult::RequiresMut`] if `call_mut` should be used instead."]
//     #[doc = ""]
//     #[doc = " It is valid, though inefficient, for this to always return `RequiresMut`."]
//     fn call<'call>(
//         &'call self,
//         server: &'call ObjectServer,
//         connection: &'call Connection,
//         msg: &'call zbus::Message,
//         name: MemberName<'call>,
//     ) -> DispatchResult<'call> {
//         todo!()
//     }
//
//     #[doc = " Call a `&mut self` method."]
//     #[doc = ""]
//     #[doc = " This will only be invoked if `call` returned `RequiresMut`."]
//     fn call_mut<'call>(
//         &'call mut self,
//         server: &'call ObjectServer,
//         connection: &'call Connection,
//         msg: &'call zbus::Message,
//         name: MemberName<'call>,
//     ) -> DispatchResult<'call> {
//         todo!()
//     }
//
//     #[doc = " Write introspection XML to the writer, with the given indentation level."]
//     fn introspect_to_writer(&self, writer: &mut dyn std::fmt::Write, level: usize) {
//         todo!()
//     }
// }

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

fn send_command(command: Server, buf: &mut Vec<u8>, socket: &mut UnixStream) {
    command.encode(buf).unwrap();

    socket.write_all(buf).unwrap();
}

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|panic_info| {
        println!("panic occurred: {panic_info}");
    }));

    let user = std::env::home_dir().unwrap();
    let user = user.to_str().unwrap();
    let path = format!("{user}/.local/share/log");
    let _file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .expect("truncating log file failed");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("client=trace"))
        .with_span_events(FmtSpan::FULL)
        .init();

    let conn = Connection::session().await.unwrap();

    let mut client = MprisClient::new().unwrap();
    client.get_all(&conn).await.unwrap();

    let mut server = std::os::unix::net::UnixStream::connect("/tmp/mpris-controller.sock").unwrap();
    let mut bytes = vec![];

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

    let cli = Cli::parse();

    if let Some(player_name) = player_name {
        info!(?player_name);
        let playing = client.get(&player_name).unwrap();
        match cli {
            Cli::Prev => {
                playing.prev(&conn).await;
            }
            Cli::After => {
                playing.next(&conn).await;
            }
            Cli::Stop => {
                playing.stop(&conn).await;
            }
            Cli::TogglePause => {
                println!("player name {player_name:?}");

                match playing.capabilities().playback_status {
                    lib::player::PlaybackStatus::Stopped => playing.play(&conn).await,
                    lib::player::PlaybackStatus::Paused => playing.play(&conn).await,
                    lib::player::PlaybackStatus::Playing => {
                        playing.pause(&conn).await;
                    }
                }
            }
            Cli::Pause => {
                playing.pause(&conn).await;
            }
            Cli::Play => {
                playing.play(&conn).await;
            }
            Cli::Players => {
                for player in client.player_names() {
                    print!("{} ", player)
                }
                println!();
            }
            Cli::Playing => {
                let metadata = &playing.capabilities().metadata;
                let title = metadata.title().unwrap_or("");
                let artists = metadata.artists();

                let url = metadata.url().unwrap_or("");
                print!("{} - ", title);
                if let Some(a) = artists {
                    for a in a {
                        print!("{a} ");
                    }
                }
                println!("{url}");
            }
            Cli::Url => {
                let url = playing.capabilities().metadata.url().unwrap_or("");
                println!("{url}");
            }
            Cli::Metadata(data) => {
                let mut fmt = String::new();
                let metadata = &playing.capabilities().metadata;
                if data.art_url {
                    fmt.write_fmt(format_args!("{} ", metadata.url().unwrap_or("")))
                        .unwrap();
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
                                fmt.write_fmt(format_args!("0{}:", printable_len[2]))
                                    .unwrap();
                            } else {
                                fmt.write_fmt(format_args!("{}:", printable_len[2]))
                                    .unwrap();
                            }

                            if printable_len[1] < 10 {
                                fmt.write_fmt(format_args!("0{}:", printable_len[1]))
                                    .unwrap();
                            } else {
                                fmt.write_fmt(format_args!("{}:", printable_len[1]))
                                    .unwrap();
                            }

                            if printable_len[0] < 10 {
                                fmt.write_fmt(format_args!("0{}:", printable_len[0]))
                                    .unwrap();
                            } else {
                                fmt.write_fmt(format_args!("{}:", printable_len[0]))
                                    .unwrap();
                            }
                        }
                    }
                }

                if data.trackid {
                    fmt.write_fmt(format_args!("{} ", metadata.track_id().unwrap_or("")))
                        .unwrap();
                }
                if data.album {
                    fmt.write_fmt(format_args!("{} ", metadata.album().unwrap_or("")))
                        .unwrap();
                }
                if data.artists
                    && let Some(artists) = metadata.artists()
                {
                    for (i, art) in artists.iter().enumerate() {
                        if i >= artists.len() {
                            fmt.write_fmt(format_args!("{art} ")).unwrap();
                        } else {
                            fmt.write_fmt(format_args!("{art}, ")).unwrap();
                        }
                    }
                }
                if data.title {
                    fmt.write_fmt(format_args!("{}, ", metadata.title().unwrap_or("")))
                        .unwrap();
                }
                if data.url {
                    fmt.write_fmt(format_args!("{}, ", metadata.url().unwrap_or("")))
                        .unwrap();
                }
                if data.track_number {
                    match metadata.track_number() {
                        Some(n) => {
                            fmt.write_fmt(format_args!("{}, ", n)).unwrap();
                        }
                        None => {
                            fmt.write_fmt(format_args!("Track number unsupported, "))
                                .unwrap();
                        }
                    }
                }
                if data.auto_rating {
                    match metadata.auto_rating() {
                        Some(n) => {
                            fmt.write_fmt(format_args!("{}, ", n)).unwrap();
                        }
                        None => {
                            fmt.write_fmt(format_args!("Auto rating unsupported, "))
                                .unwrap();
                        }
                    }
                }
                if data.album_artists
                    && let Some(artists) = metadata.album_artists()
                {
                    if artists.len() == 1 {
                        fmt.write_fmt(format_args!("{} ", artists[0])).unwrap();
                    } else {
                        for (i, art) in artists.iter().enumerate() {
                            if i >= artists.len() {
                                fmt.write_fmt(format_args!("{} ", art)).unwrap();
                            } else {
                                fmt.write_fmt(format_args!("{}, ", art)).unwrap();
                            }
                        }
                    }
                }
                println!("{fmt}");
            }
        }
    }
}
