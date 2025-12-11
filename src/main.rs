use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    task::{Context, Poll},
};

use futures::StreamExt;
use tracing::{info, instrument};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use zbus::{
    Connection, Proxy,
    names::{BusName, MemberName, WellKnownName},
    proxy::SignalStream,
    zvariant::{OwnedValue, Str, Structure, Value},
};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_PLAYER_PREFIX: &str = "org.mpris.MediaPlayer2.Player";

const DBUS_NAME: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";

#[derive(Debug)]
enum DbusMethods {
    ListNames,
    GetAll,
}

impl TryFrom<DbusMethods> for MemberName<'_> {
    type Error = zbus::names::Error;

    fn try_from(value: DbusMethods) -> Result<Self, Self::Error> {
        let s = match value {
            DbusMethods::ListNames => "ListNames",
            DbusMethods::GetAll => "GetAll",
        };

        Ok(MemberName::from_str_unchecked(s))
    }
}

#[derive(Debug)]
enum DbusSignals {
    PropertiesChanged,
    NameOwnerChanged,
}

impl TryFrom<DbusSignals> for MemberName<'_> {
    // so that things that require TryFrom<MemberName> work for this type
    type Error = zbus::names::Error;
    fn try_from(value: DbusSignals) -> Result<Self, Self::Error> {
        let s = match value {
            DbusSignals::PropertiesChanged => "PropertiesChanged",
            DbusSignals::NameOwnerChanged => "NameOwnerChanged",
        };

        Ok(MemberName::from_str_unchecked(s))
    }
}

#[derive(Debug, Default, Clone, Copy)]
enum PlaybackStatus {
    #[default]
    Stopped,
    Paused,
    Playing,
}

impl<'a> TryFrom<&Value<'a>> for PlaybackStatus {
    type Error = ();
    fn try_from(value: &Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Str(s) => match &**s {
                "Stopped" => Ok(Self::Stopped),
                "Paused" => Ok(Self::Paused),
                "Playing" => Ok(Self::Playing),
                _ => Err(()),
            },
            _ => Err(()),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
enum LoopStatus {
    #[default]
    None,
    Playlist,
    Track,
}

impl<'a> TryFrom<&Value<'a>> for LoopStatus {
    type Error = ();
    fn try_from(value: &Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Str(s) => match &**s {
                "None" => Ok(Self::None),
                "Playlist" => Ok(Self::Playlist),
                "Track" => Ok(Self::Track),
                _ => Err(()),
            },
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct Metadata {
    art_url: Option<String>,
    length: Option<u64>,
    trackid: String,
    album: Option<String>,
    artists: Vec<String>,
    title: String,
    url: String,
    track_number: Option<i32>,
    disc_number: Option<i32>,
    auto_rating: Option<f64>,
    album_artists: Option<Vec<String>>,
}

impl<'a> TryFrom<&Value<'a>> for Metadata {
    type Error = ();

    fn try_from(value: &Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Dict(dict) => {
                let map: HashMap<String, Value> = dict
                    .iter()
                    .filter_map(|f| {
                        let s = match f.0 {
                            Value::Str(s) => s.to_string(),
                            _ => return None,
                        };

                        Some((s, f.1.try_clone().unwrap()))
                    })
                    .collect();
                let metadata: Metadata = map.into();

                Ok(metadata)
            }
            _ => Err(()),
        }
    }
}

impl<'a> From<HashMap<String, Value<'a>>> for Metadata {
    #[instrument]
    fn from(value: HashMap<String, Value<'a>>) -> Self {
        let art_url: Option<String> = match value.get("mpris:artUrl") {
            Some(url) => match url {
                Value::Str(s) => Some(s.to_string()),
                _ => unimplemented!(),
            },
            None => None,
        };
        // optional because players like browsers can not include the length when we request its
        // metadata but might give us the length later
        let length = match value.get("mpris:length") {
            Some(Value::I64(s)) => Some(*s as u64),
            Some(Value::U64(s)) => Some(*s),
            None => None,
            _ => unimplemented!(),
        };
        let trackid: String = match value.get("mpris:trackid") {
            Some(Value::ObjectPath(s)) => s.to_string(),
            Some(Value::Str(s)) => s.to_string(),
            _ => unimplemented!(),
        };

        let album: Option<String> = match value.get("xesam:album") {
            Some(Value::Str(s)) => Some(s.to_string()),
            None => None,

            _ => unimplemented!(),
        };
        let artists: Vec<String> = value
            .get("xesam:artist")
            .unwrap()
            .try_clone()
            .unwrap()
            .try_into()
            .unwrap();
        let title: String = value.get("xesam:title").unwrap().try_into().unwrap();
        let url: String = value.get("xesam:url").unwrap().try_into().unwrap();

        // optional (basically only spotify implements this)
        let album_artist = match value.get("xesam:albumArtist") {
            Some(Value::Array(s)) => Some(
                s.iter()
                    .filter_map(|f| {
                        if let Value::Str(s) = f {
                            Some(s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
            s => {
                info!(?s);
                None
            }
        };

        let track_number = {
            match value.get("xesam:trackNumber") {
                Some(Value::I32(s)) => Some(*s),
                None => None,
                _ => unreachable!(),
            }
        };

        let disc_number = {
            match value.get("xesam:discNumber") {
                Some(Value::I32(s)) => Some(*s),
                None => None,

                _ => unreachable!(),
            }
        };

        let auto_rating = {
            match value.get("xesam:autoRating") {
                Some(Value::F64(v)) => Some(*v),
                None => None,

                _ => unreachable!(),
            }
        };

        Self {
            album_artists: album_artist,
            art_url,
            length,
            trackid,
            album,
            artists,
            title,
            url,
            track_number,
            disc_number,
            auto_rating,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct PlayerCapabilities {
    can_control: bool,
    can_next: bool,
    can_previous: bool,
    can_pause: bool,
    can_play: bool,
    can_seek: bool,
    loop_status: Option<LoopStatus>,
    max_rate: Option<f64>,
    min_rate: Option<f64>,
    metadata: Metadata,
    playback_status: PlaybackStatus,
    position: i64,
    rate: f64,
    shuffle: Option<bool>,
    volume: Option<f64>,
}

impl<'a> From<HashMap<&str, Value<'a>>> for PlayerCapabilities {
    fn from(value: HashMap<&str, Value<'a>>) -> Self {
        let can_control: bool = value.get("CanControl").unwrap().try_into().unwrap();
        let can_next: bool = value.get("CanGoNext").unwrap().try_into().unwrap();
        let can_previous: bool = value.get("CanGoPrevious").unwrap().try_into().unwrap();
        let can_pause: bool = value.get("CanPause").unwrap().try_into().unwrap();
        let can_play: bool = value.get("CanPlay").unwrap().try_into().unwrap();
        let can_seek: bool = value.get("CanSeek").unwrap().try_into().unwrap();
        let shuffle: Option<bool> = value.get("Shuffle").map(|f| f.try_into().unwrap());
        let loop_status: Option<LoopStatus> =
            value.get("LoopStatus").map(|f| f.try_into().unwrap());
        let max_rate: Option<f64> = value.get("MaximumRate").map(|v| v.try_into().unwrap());
        let min_rate: Option<f64> = value.get("MinimumRate").map(|v| v.try_into().unwrap());

        let metadata: Metadata = TryInto::<HashMap<String, Value>>::try_into(
            value.get("Metadata").unwrap().try_clone().unwrap(),
        )
        .unwrap()
        .into();
        let rate: f64 = value.get("Rate").unwrap().try_into().unwrap();
        let playback_status: PlaybackStatus =
            value.get("PlaybackStatus").unwrap().try_into().unwrap();
        let position: i64 = value.get("Position").unwrap().try_into().unwrap();
        let volume: Option<f64> = value.get("Volume").map(|f| f.try_into().unwrap());

        Self {
            can_control,
            can_next,
            can_previous,
            can_pause,
            can_play,
            can_seek,
            loop_status,
            max_rate,
            min_rate,
            metadata,
            playback_status,
            position,
            rate,
            shuffle,
            volume,
        }
    }
}

#[derive(Debug)]
struct Player<'a> {
    capabilities: PlayerCapabilities,
    stream: SignalStream<'a>,
}

impl<'a> Player<'a> {
    #[instrument]
    async fn new(conn: &Connection, name: String) -> Self {
        println!("name {name:?}");
        let properties = conn
            .call_method(
                Some(name.clone()),
                MPRIS_PATH,
                Some(DBUS_PROPERTIES),
                DbusMethods::GetAll,
                &(MPRIS_PLAYER_PREFIX),
            )
            .await
            .unwrap();

        let body = properties.body();
        let properties: PlayerCapabilities =
            body.deserialize::<HashMap<&str, Value>>().unwrap().into();

        let cloned = name.clone();
        let proxy = Proxy::new(
            conn,
            BusName::WellKnown(WellKnownName::from_str_unchecked(&cloned)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await
        .unwrap();

        let stream = proxy
            .receive_signal(DbusSignals::PropertiesChanged)
            .await
            .unwrap();

        Self {
            capabilities: properties,
            stream,
        }
    }
}

#[tokio::main]
async fn main() {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("log")
        .expect("truncating log file failed");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("mpris_controller=trace"))
        .with_span_events(FmtSpan::FULL)
        .with_writer(file)
        .with_ansi(false)
        .init();

    let conn = Connection::session().await.unwrap();
    let msg = conn
        .call_method(
            Some(DBUS_NAME),
            DBUS_PATH,
            Some(DBUS_NAME),
            DbusMethods::ListNames,
            &(),
        )
        .await
        .unwrap();

    let body = msg.body();
    let data: Vec<Box<_>> = body
        .deserialize::<Vec<&str>>()
        .unwrap()
        .iter()
        .filter_map(|f| {
            if f.starts_with(MPRIS_PREFIX) {
                Some(Box::new(*f))
            } else {
                None
            }
        })
        .collect();

    println!("{data:?}");

    let mut players: HashMap<&str, Player> = HashMap::new();

    for player_name in &data {
        let player = Player::new(&conn, player_name.to_string()).await;
        players.insert(player_name, player);
    }

    let name_changed = Proxy::new(&conn, DBUS_NAME, DBUS_PATH, DBUS_NAME)
        .await
        .unwrap();

    let mut stream = name_changed
        .receive_signal(DbusSignals::NameOwnerChanged)
        .await
        .unwrap();

    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut pollable_stream = Pin::new(&mut stream);
    while let Some(player) = players.iter_mut().next() {
        match Pin::new(&mut player.1.stream).poll_next_unpin(&mut cx) {
            Poll::Ready(Some(msg)) => {
                let body = msg.body();
                // returns interface, changed (vec), invalidated (vec), invalidated seems to always
                // be empty
                let s: Structure = body.deserialize().unwrap();

                let iface: Str = s.fields()[0].clone().try_into().unwrap();
                let changed: HashMap<String, OwnedValue> =
                    s.fields()[1].clone().try_into().unwrap();

                println!("iface {iface}; changed {changed:?}");
                if let Some(status) = changed.get("PlaybackStatus") {
                    let val = &**status;
                    player.1.capabilities.playback_status = val.try_into().unwrap();
                }
                if let Some(status) = changed.get("Metadata") {
                    let val = &**status;
                    if let Value::Dict(dict) = val {
                        let map: HashMap<String, Value> =
                            dict.try_clone().unwrap().try_into().unwrap();
                        let metadata: Metadata = map.into();
                        println!("{metadata:?}");
                    }
                }
                if let Some(status) = changed.get("CanGoPrevious") {
                    player.1.capabilities.can_previous = status.try_into().unwrap();
                }
            }
            Poll::Ready(None) => {
                // stream ended
            }
            Poll::Pending => {
                // no message available (non-blocking)
            }
        }
        if let Poll::Ready(Some(msg)) = pollable_stream.poll_next_unpin(&mut cx) {
            let (name, old_owner, new_owner): (String, String, String) =
                msg.body().deserialize().unwrap();

            if name.starts_with(MPRIS_PREFIX) {
                println!("name {name}; old_owner {old_owner}; new_owner {new_owner}");

                match (old_owner.is_empty(), new_owner.is_empty()) {
                    // added player
                    // needs to call ListNames to convert the name (unique name) to well known name
                    (true, false) => {
                        let msg = conn
                            .call_method(
                                Some(DBUS_NAME),
                                DBUS_PATH,
                                Some(DBUS_NAME),
                                DbusMethods::ListNames,
                                &(),
                            )
                            .await
                            .unwrap();

                        let body = msg.body();
                        let data: HashSet<&str> = body
                            .deserialize::<Vec<&str>>()
                            .unwrap()
                            .iter()
                            .filter_map(|f| {
                                if f.starts_with(MPRIS_PREFIX) {
                                    Some(*f)
                                } else {
                                    None
                                }
                            })
                            .collect();

                        let mut idx = "";
                        for player_name in players.keys() {
                            if data.contains(player_name) {
                                idx = player_name;
                                break;
                            }
                        }

                        if idx.is_empty() {
                            unimplemented!()
                        }
                        let p = Player::new(&conn, idx.to_string()).await;
                        println!("added player {p:?}");
                        players.insert(idx, p);
                    }
                    // removed player
                    (false, true) => {
                        let msg = conn
                            .call_method(
                                Some(DBUS_NAME),
                                DBUS_PATH,
                                Some(DBUS_NAME),
                                DbusMethods::ListNames,
                                &(),
                            )
                            .await
                            .unwrap();

                        let body = msg.body();
                        let data: HashSet<&str> = body
                            .deserialize::<Vec<&str>>()
                            .unwrap()
                            .iter()
                            .filter_map(|f| {
                                if f.starts_with(MPRIS_PREFIX) {
                                    Some(*f)
                                } else {
                                    None
                                }
                            })
                            .collect();

                        let mut idx = "";
                        for player_name in players.keys() {
                            if data.contains(player_name) {
                                idx = *player_name;
                            }
                        }

                        println!("removing player {idx:?}");
                        _ = players.remove(idx);
                    }

                    _ => {}
                };
            }
        }
    }
}
