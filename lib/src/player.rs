use anyhow::{anyhow, bail};
use futures::StreamExt;
use tracing::instrument;
use zbus::{
    names::{BusName, WellKnownName},
    proxy::SignalStream,
    zvariant::{ObjectPath, Str, Value},
    Connection, Message, Proxy,
};

use std::{
    collections::HashMap,
    task::{Context, Poll},
};

use crate::{DbusMethods, DbusSignals, DBUS_PROPERTIES, MPRIS_PATH, MPRIS_PLAYER_PREFIX, WAKER};

#[derive(Debug)]
pub enum NameOwnerChanged {
    NewPlayer,
    RemovedPlayer,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Paused,
    Playing,
}

impl<'a> TryFrom<&Str<'a>> for PlaybackStatus {
    type Error = anyhow::Error;

    fn try_from(value: &Str) -> Result<Self, Self::Error> {
        match &**value {
            "Stopped" => Ok(Self::Stopped),
            "Paused" => Ok(Self::Paused),
            "Playing" => Ok(Self::Playing),
            _ => bail!("incorrect playback status: {value}"),
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

impl TryFrom<&str> for LoopStatus {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "None" => Ok(Self::None),
            "Playlist" => Ok(Self::Playlist),
            "Track" => Ok(Self::Track),
            _ => Err(anyhow!("invalid loop status {value}")),
        }
    }
}

impl<'a> TryFrom<&Value<'a>> for LoopStatus {
    type Error = anyhow::Error;

    fn try_from(value: &Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Str(s) => match &**s {
                "None" => Ok(Self::None),
                "Playlist" => Ok(Self::Playlist),
                "Track" => Ok(Self::Track),
                _ => Err(anyhow!("")),
            },
            _ => Err(anyhow!("")),
        }
    }
}

impl From<LoopStatus> for Value<'_> {
    fn from(value: LoopStatus) -> Self {
        let status = match value {
            LoopStatus::None => "None",
            LoopStatus::Playlist => "Playlist",
            LoopStatus::Track => "Track",
        };

        Value::Str(status.into())
    }
}

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct Metadata {
    art_url: Option<String>,
    length: Option<u64>,
    trackid: Option<String>,
    album: Option<String>,
    artists: Option<Vec<String>>,
    title: Option<String>,
    url: Option<String>,
    track_number: Option<i32>,
    disc_number: Option<i32>,
    auto_rating: Option<f64>,
    album_artists: Option<Vec<String>>,
}

impl Metadata {
    pub fn art_url(&self) -> Option<&str> {
        match &self.art_url {
            Some(url) => Some(url),
            None => None,
        }
    }

    pub fn length(&self) -> Option<u64> {
        self.length
    }

    pub fn track_id(&self) -> Option<&str> {
        match &self.trackid {
            Some(id) => Some(id),
            None => None,
        }
    }

    pub fn album(&self) -> Option<&str> {
        match &self.album {
            Some(title) => Some(title),
            None => None,
        }
    }

    pub fn artists(&self) -> Option<&[String]> {
        match &self.artists {
            Some(artists) => Some(artists.as_slice()),
            None => None,
        }
    }

    pub fn title(&self) -> Option<&str> {
        match &self.title {
            Some(title) => Some(title),
            None => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match &self.url {
            Some(url) => Some(url),
            None => None,
        }
    }

    pub fn track_number(&self) -> Option<i32> {
        self.track_number
    }

    pub fn disc_number(&self) -> Option<i32> {
        self.disc_number
    }

    pub fn auto_rating(&self) -> Option<f64> {
        self.auto_rating
    }

    pub fn album_artists(&self) -> Option<&[String]> {
        match &self.album_artists {
            Some(artists) => Some(artists.as_slice()),
            None => None,
        }
    }
}

impl<'a> TryFrom<&Value<'a>> for Metadata {
    type Error = anyhow::Error;

    #[instrument(skip_all)]
    fn try_from(value: &Value<'a>) -> Result<Self, Self::Error> {
        let value: HashMap<String, Value> = value.try_clone()?.try_into()?;

        let art_url: Option<String> = match value.get("mpris:artUrl") {
            Some(url) => match url {
                Value::Str(s) => Some(s.to_string()),
                _ => bail!("can not find mpris:artUrl"),
            },
            None => None,
        };

        // optional because players like browsers can not include the length when we request its
        // metadata but might give us the length later
        let length = match value.get("mpris:length") {
            Some(Value::I64(s)) => Some(s.cast_unsigned()),
            Some(Value::U64(s)) => Some(*s),
            None => None,
            _ => bail!("can not find mpris:length"),
        };
        let trackid: Option<String> = match value.get("mpris:trackid") {
            Some(Value::ObjectPath(s)) => Some(s.to_string()),
            Some(Value::Str(s)) => Some(s.to_string()),
            _ => None,
        };

        let album: Option<String> = match value.get("xesam:album") {
            Some(Value::Str(s)) => Some(s.to_string()),
            None => None,

            _ => bail!("can not find xesam:album"),
        };

        let artists: Option<Vec<String>> = match value.get("xesam:artist") {
            Some(v) => Some(v.try_clone()?.try_into()?),
            None => None,
        };

        let title: Option<String> = match value.get("xesam:title") {
            Some(v) => Some(v.try_into()?),
            None => None,
        };

        let url: Option<String> = match value.get("xesam:url") {
            Some(v) => Some(v.try_into()?),
            None => None,
        };

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
            _ => None,
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

        Ok(Self {
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
        })
    }
}

impl<'a> TryFrom<HashMap<String, Value<'a>>> for Metadata {
    type Error = anyhow::Error;

    #[instrument]
    fn try_from(value: HashMap<String, Value<'a>>) -> anyhow::Result<Self> {
        let art_url: Option<String> = match value.get("mpris:artUrl") {
            Some(url) => match url {
                Value::Str(s) => Some(s.to_string()),
                _ => bail!("failed to find mpris:artUrl"),
            },
            None => None,
        };
        // optional because players like browsers can not include the length when we request its
        // metadata but might give us the length later
        let length = match value.get("mpris:length") {
            Some(Value::I64(s)) => Some(s.cast_unsigned()),
            Some(Value::U64(s)) => Some(*s),
            None => None,
            _ => bail!("failed to find mpris:length"),
        };

        let trackid: Option<String> = match value.get("mpris:trackid") {
            Some(Value::ObjectPath(s)) => Some(s.to_string()),
            Some(Value::Str(s)) => Some(s.to_string()),
            _ => None,
        };

        let album: Option<String> = match value.get("xesam:album") {
            Some(Value::Str(s)) => Some(s.to_string()),
            None => None,

            _ => bail!("failed to find xesam:album"),
        };
        let artists: Option<Vec<String>> = match value.get("xesam:artist") {
            Some(v) => Some(v.try_clone()?.try_into()?),
            None => None,
        };

        let title: Option<String> = match value.get("xesam:title") {
            Some(v) => Some(v.try_into()?),
            None => None,
        };

        let url: Option<String> = match value.get("xesam:url") {
            Some(v) => Some(v.try_into()?),
            None => None,
        };

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
            _ => None,
        };

        let track_number = {
            match value.get("xesam:trackNumber") {
                Some(Value::I32(s)) => Some(*s),
                None => None,
                _ => bail!("failed to find xesam:trackNumber"),
            }
        };

        let disc_number = {
            match value.get("xesam:discNumber") {
                Some(Value::I32(s)) => Some(*s),
                None => None,

                _ => bail!("failed to find xesam:discNumber"),
            }
        };

        let auto_rating = {
            match value.get("xesam:autoRating") {
                Some(Value::F64(v)) => Some(*v),
                None => None,

                _ => bail!("failed to find xesam:autoRating"),
            }
        };

        Ok(Self {
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
        })
    }
}

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct Capabilities {
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
    pub position: u64,
    pub rate: f64,
    pub shuffle: Option<bool>,
    pub volume: Option<f64>,
}

impl<'a> TryFrom<HashMap<&str, Value<'a>>> for Capabilities {
    type Error = anyhow::Error;

    #[instrument(skip_all)]
    fn try_from(value: HashMap<&str, Value<'a>>) -> anyhow::Result<Self> {
        let can_control: bool = value
            .get("CanControl")
            .unwrap_or(&Value::Bool(false))
            .try_into()?;
        let can_next: bool = value
            .get("CanGoNext")
            .unwrap_or(&Value::Bool(false))
            .try_into()?;
        let can_previous: bool = value
            .get("CanGoPrevious")
            .unwrap_or(&Value::Bool(false))
            .try_into()?;
        let can_pause: bool = value
            .get("CanPause")
            .unwrap_or(&Value::Bool(false))
            .try_into()?;
        let can_play: bool = value
            .get("CanPlay")
            .unwrap_or(&Value::Bool(false))
            .try_into()?;
        let can_seek: bool = value
            .get("CanSeek")
            .unwrap_or(&Value::Bool(false))
            .try_into()?;

        let shuffle: Option<bool> = value.get("Shuffle").map(TryInto::try_into).transpose()?;
        let loop_status: Option<LoopStatus> =
            value.get("LoopStatus").map(TryInto::try_into).transpose()?;

        let max_rate: Option<f64> = value
            .get("MaximumRate")
            .map(TryInto::try_into)
            .transpose()?;

        let min_rate: Option<f64> = value
            .get("MinimumRate")
            .map(TryInto::try_into)
            .transpose()?;

        let metadata: Metadata = TryInto::<HashMap<String, Value>>::try_into(
            value
                .get("Metadata")
                .ok_or(anyhow!("can not find Metadata"))?
                .try_clone()?,
        )?
        .try_into()?;

        let rate: f64 = value
            .get("Rate")
            .ok_or(anyhow!("can not find Rate"))?
            .try_into()?;
        let playback_status: PlaybackStatus = value
            .get("PlaybackStatus")
            .ok_or(anyhow!("can not find PlaybackStatus"))
            .map(|f| match f {
                Value::Str(s) => PlaybackStatus::try_from(s),
                _ => bail!("unsupported type"),
            })??;
        let position = value
            .get("Position")
            .ok_or(anyhow!("can not find Position"))
            .map(|f| match f {
                Value::U64(f) => Ok(*f),
                Value::I64(f) => Ok(f.cast_unsigned()),
                _ => Err(anyhow!("incorrect or unsupported type for Position")),
            })??;

        let volume: Option<f64> = value.get("Volume").map(TryInto::try_into).transpose()?;

        Ok(Self {
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
        })
    }
}

#[derive(Debug)]
pub enum PlayerUpdated {
    PlaybackStatus(PlaybackStatus),
    Metadata(Box<Metadata>),
    CanGoPrevious(bool),
}

#[derive(Debug)]
pub enum MprisEvent {
    PlayerAdded,
    PlayerRemoved,
    PlayerUpdated(PlayerUpdated),
}

pub struct Player<'a> {
    pub capabilities: Capabilities,
    pub stream: SignalStream<'a>,
    name: String,
}

impl<'a> Player<'a> {
    // #[tracing::instrument(skip(conn), ret, err)]
    pub async fn new(conn: &Connection, name: String) -> anyhow::Result<Self> {
        let properties = conn
            .call_method(
                Some(&*name),
                MPRIS_PATH,
                Some(DBUS_PROPERTIES),
                DbusMethods::GetAll,
                &("org.mpris.MediaPlayer2.Player"),
            )
            .await?;

        let body = properties.body();
        let properties: Capabilities = body.deserialize::<HashMap<&str, Value>>()?.try_into()?;

        let proxy = Proxy::new(
            conn,
            BusName::WellKnown(WellKnownName::from_str_unchecked(&name)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await?;

        let stream = proxy.receive_signal(DbusSignals::PropertiesChanged).await?;

        Ok(Self {
            capabilities: properties,
            stream,
            name,
        })
    }

    pub fn stream_mut(&mut self) -> &mut SignalStream<'a> {
        &mut self.stream
    }

    pub fn stream(&self) -> &SignalStream<'a> {
        &self.stream
    }

    #[must_use]
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn capabilities_mut(&mut self) -> &mut Capabilities {
        &mut self.capabilities
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn play(&self, conn: &Connection) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "Play",
            &(),
        )
        .await
        .unwrap();
    }

    pub async fn stop(&self, conn: &Connection) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "Stop",
            &(),
        )
        .await
        .unwrap();
    }

    pub async fn next(&self, conn: &Connection) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "Next",
            &(),
        )
        .await
        .unwrap();
    }

    pub async fn prev(&self, conn: &Connection) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "Previous",
            &(),
        )
        .await
        .unwrap();
    }

    pub async fn pause(&self, conn: &Connection) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "Pause",
            &(),
        )
        .await
        .unwrap();
    }

    pub async fn pause_play(&self, conn: &Connection) -> zbus::Result<Message> {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "PausePlay",
            &(),
        )
        .await
    }

    pub async fn seek(&self, conn: &Connection, nanos: u64) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "SetPosition",
            &(nanos),
        )
        .await
        .unwrap();
    }

    pub async fn set_position(&self, conn: &Connection, track_id: ObjectPath<'_>, nanos: u64) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "SetPosition",
            &(track_id, nanos),
        )
        .await
        .unwrap();
    }

    pub async fn open_uri(&self, conn: &Connection, uri: &str) {
        conn.call_method(
            Some(&*self.name),
            "/org/mpris/MediaPlayer2",
            Some("org.mpris.MediaPlayer2.Player"),
            "OpenUri",
            &(uri),
        )
        .await
        .unwrap();
    }

    pub fn volume(&self) -> Option<f64> {
        self.capabilities.volume
    }

    pub async fn set_volume(&mut self, conn: &Connection, volume: f64) {
        conn.call_method(
            Some(self.name()),
            MPRIS_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(MPRIS_PLAYER_PREFIX, "Volume", &Value::F64(volume)),
        )
        .await
        .unwrap();

        self.capabilities.volume = Some(volume);
    }

    pub async fn toggle_shuffle(&self, conn: &Connection, shuffle: bool) {
        conn.call_method(
            Some(self.name()),
            MPRIS_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(MPRIS_PLAYER_PREFIX, "Shuffle", &Value::from(shuffle)),
        )
        .await
        .unwrap();
    }

    pub async fn set_loop_status(
        &self,
        conn: &Connection,
        status: LoopStatus,
    ) -> anyhow::Result<()> {
        conn.call_method(
            Some(self.name()),
            MPRIS_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(MPRIS_PLAYER_PREFIX, "LoopStatus", Value::from(status)),
        )
        .await?;

        Ok(())
    }
}

impl std::fmt::Debug for Player<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.capabilities)
    }
}

pub async fn poll_player<'a>(stream: &mut SignalStream<'a>) -> anyhow::Result<Poll<PlayerUpdated>> {
    let waker = WAKER;
    let mut cx = Context::from_waker(&waker);
    if let Poll::Ready(Some(msg)) = stream.poll_next_unpin(&mut cx) {
        let body = msg.body();
        // returns interface (str), changed (vec), invalidated (vec), invalidated seems to always
        // be empty
        let structure: zbus::zvariant::Structure = body.deserialize().unwrap();

        let iface: zbus::zvariant::Str = structure.fields()[0].clone().try_into()?;
        let changed: HashMap<String, zbus::zvariant::OwnedValue> =
            structure.fields()[1].clone().try_into()?;

        println!("iface {iface} changed {changed:?}]");

        if let Some(status) = changed.get("PlaybackStatus") {
            let val = &**status;

            let val = match val {
                Value::Str(s) => PlaybackStatus::try_from(s),
                _ => bail!("incorrect type {val}"),
            }?;

            return Ok(Poll::Ready(PlayerUpdated::PlaybackStatus(val)));
        }
        if let Some(status) = changed.get("Metadata") {
            let val = &**status;
            if let Value::Dict(dict) = val {
                let map: HashMap<String, Value> = dict.try_clone()?.try_into()?;
                let metadata: Metadata = map.try_into()?;
                println!("{metadata:?}");
                return Ok(Poll::Ready(PlayerUpdated::Metadata(Box::new(metadata))));
            }
        }
        if let Some(status) = changed.get("CanGoPrevious") {
            return Ok(Poll::Ready(PlayerUpdated::CanGoPrevious(bool::try_from(
                status,
            )?)));
        }
    };

    Ok(Poll::Pending)
}
