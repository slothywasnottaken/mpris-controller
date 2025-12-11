use std::collections::HashMap;

use tracing::{info, instrument};
use zbus::{
    Connection, Proxy,
    names::{BusName, MemberName, WellKnownName},
    proxy::SignalStream,
    zvariant::Value,
};

pub const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2";
pub const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
pub const MPRIS_PLAYER_PREFIX: &str = "org.mpris.MediaPlayer2.Player";

pub const DBUS_NAME: &str = "org.freedesktop.DBus";
pub const DBUS_PATH: &str = "/org/freedesktop/DBus";
pub const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";

#[derive(Debug)]
pub enum DbusMethods {
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
pub enum DbusSignals {
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
pub enum PlaybackStatus {
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
pub enum LoopStatus {
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
pub struct Metadata {
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
pub struct PlayerCapabilities {
    pub can_control: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_pause: bool,
    pub can_play: bool,
    pub can_seek: bool,
    pub loop_status: Option<LoopStatus>,
    pub max_rate: Option<f64>,
    pub min_rate: Option<f64>,
    pub metadata: Metadata,
    pub playback_status: PlaybackStatus,
    pub position: i64,
    pub rate: f64,
    pub shuffle: Option<bool>,
    pub volume: Option<f64>,
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
pub struct Player<'a> {
    pub capabilities: PlayerCapabilities,
    pub stream: SignalStream<'a>,
}

impl<'a> Player<'a> {
    #[instrument]
    pub async fn new(conn: &Connection, name: String) -> Self {
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

    pub fn stream_mut(&'a mut self) -> &'a mut SignalStream<'a> {
        &mut self.stream
    }

    pub fn capabilities(&self) -> &PlayerCapabilities {
        &self.capabilities
    }

    pub fn capabilities_mut(&mut self) -> &mut PlayerCapabilities {
        &mut self.capabilities
    }
}
