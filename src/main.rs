use std::collections::HashMap;

use tracing::info;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use zbus::{
    Connection,
    zvariant::{ObjectPath, Value},
};

const MPRIS_PLAYER: &str = "org.mpris.MediaPlayer2";

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
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
struct Metadata {
    art_url: String,
    length: u64,
    trackid: String,
    album: String,
    artist: Vec<String>,
    title: String,
    url: String,
    track_number: Option<i32>,
    disc_number: Option<i32>,
    auto_rating: Option<f64>,
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
    fn from(value: HashMap<String, Value<'a>>) -> Self {
        let art_url: String = value.get("mpris:artUrl").unwrap().try_into().unwrap();
        let length: u64 = match value.get("mpris:length") {
            Some(f) => match f {
                Value::I64(s) => *s as u64,
                Value::U64(s) => *s,
                _ => unimplemented!(),
            },
            None => {
                unimplemented!()
            }
        };
        let trackid: String = match value.get("mpris:trackid") {
            Some(f) => match f {
                Value::ObjectPath(s) => s.to_string(),
                Value::Str(s) => s.to_string(),
                _ => unimplemented!(),
            },
            None => {
                unimplemented!()
            }
        };

        let album: String = value.get("xesam:album").unwrap().try_into().unwrap();
        let artist: Vec<String> = value
            .get("xesam:artist")
            .unwrap()
            .try_clone()
            .unwrap()
            .try_into()
            .unwrap();
        let title: String = value.get("xesam:title").unwrap().try_into().unwrap();
        let url: String = value.get("xesam:url").unwrap().try_into().unwrap();

        // optional (basically only spotify implements this)
        value.get("xesam:albumArtist");
        value.get("xesam:autoRating");
        value.get("xesam:discNumber");
        value.get("xesam:trackNumber");

        let track_number = {
            match value.get("xesam:trackNumber") {
                Some(val) => match val {
                    Value::I32(v) => Some(*v),
                    _ => unreachable!(),
                },
                None => None,
            }
        };

        let disc_number = {
            match value.get("xesam:discNumber") {
                Some(val) => match val {
                    Value::I32(v) => Some(*v),
                    _ => unreachable!(),
                },
                None => None,
            }
        };

        let auto_rating = {
            match value.get("xesam:autoRating") {
                Some(val) => match val {
                    Value::F64(v) => Some(*v),
                    _ => unreachable!(),
                },
                None => None,
            }
        };

        Self {
            art_url,
            length,
            trackid: trackid.to_string(),
            album,
            artist,
            title,
            url,
            track_number,
            disc_number,
            auto_rating,
        }
    }
}

#[derive(Debug)]
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
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "ListNames",
            &(),
        )
        .await
        .unwrap();

    let body = msg.body();
    let data: Vec<&str> = body
        .deserialize::<Vec<&str>>()
        .unwrap()
        .iter()
        .filter_map(|f| {
            if f.starts_with(MPRIS_PLAYER) {
                Some(*f)
            } else {
                None
            }
        })
        .collect();

    println!("{data:?}");

    for player in data.iter() {
        let properties = conn
            .call_method(
                Some(*player),
                "/org/mpris/MediaPlayer2",
                Some("org.freedesktop.DBus.Properties"),
                "GetAll",
                &("org.mpris.MediaPlayer2.Player"),
            )
            .await
            .unwrap();

        let body = properties.body();
        let map = body.deserialize::<HashMap<&str, Value>>().unwrap();
        info!(?map);
        let capabilities: PlayerCapabilities = map.into();
        // info!(?capabilities);

        // let mut msg: Vec<_> = map
        //     .iter()
        //     .filter_map(|f| {
        //         if *f.0 == "Metadata" { Some(f.1) } else { None }
        //         // if f.1.signature() != Signature::dict(Signature::Str, Signature::Variant) {
        //         //     let (name, val) = (*f.0, f.1.clone());
        //         //     Some(name)
        //         // } else {
        //         //     None
        //         // }
        //     })
        //     .collect();
        //
        // msg.sort();
        // println!("{capabilities:#?}");
    }
}
