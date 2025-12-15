use std::{
    collections::HashMap,
    fmt::Debug,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::{anyhow, bail};
use futures::StreamExt;
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
    NameHasOwner,
}

impl TryFrom<DbusMethods> for MemberName<'_> {
    type Error = zbus::names::Error;

    fn try_from(value: DbusMethods) -> Result<Self, Self::Error> {
        let s = match value {
            DbusMethods::ListNames => "ListNames",
            DbusMethods::GetAll => "GetAll",
            DbusMethods::NameHasOwner => "NameHasOwner",
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
    type Error = anyhow::Error;

    fn try_from(value: &Value<'a>) -> Result<Self, Self::Error> {
        match value {
            Value::Str(s) => match &**s {
                "Stopped" => Ok(Self::Stopped),
                "Paused" => Ok(Self::Paused),
                "Playing" => Ok(Self::Playing),
                _ => Err(anyhow!("")),
            },
            _ => Err(anyhow!("")),
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

#[derive(Debug, Default)]
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
            Some(Value::I64(s)) => Some(*s as u64),
            Some(Value::U64(s)) => Some(*s),
            None => None,
            _ => bail!("can not find mpris:length"),
        };
        let trackid: String = match value.get("mpris:trackid") {
            Some(Value::ObjectPath(s)) => s.to_string(),
            Some(Value::Str(s)) => s.to_string(),
            _ => bail!("can not find mpris:trackid"),
        };

        let album: Option<String> = match value.get("xesam:album") {
            Some(Value::Str(s)) => Some(s.to_string()),
            None => None,

            _ => bail!("can not find xesam:album"),
        };
        let artists: Vec<String> = value
            .get("xesam:artist")
            .ok_or(anyhow!("failed to find artists"))?
            .try_clone()?
            .try_into()?;
        let title: String = value
            .get("xesam:title")
            .ok_or(anyhow!("can not find xesam:title"))?
            .try_into()?;
        let url: String = value
            .get("xesam:url")
            .ok_or(anyhow!("can not find xesam:url"))?
            .try_into()?;

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
            Some(Value::I64(s)) => Some(*s as u64),
            Some(Value::U64(s)) => Some(*s),
            None => None,
            _ => bail!("failed to find mpris:length"),
        };
        let trackid: String = match value.get("mpris:trackid") {
            Some(Value::ObjectPath(s)) => s.to_string(),
            Some(Value::Str(s)) => s.to_string(),
            _ => bail!("failed to find mpris:trackid"),
        };

        let album: Option<String> = match value.get("xesam:album") {
            Some(Value::Str(s)) => Some(s.to_string()),
            None => None,

            _ => bail!("failed to find xesam:album"),
        };
        let artists: Vec<String> = value
            .get("xesam:artist")
            .ok_or(anyhow!("failed to find xesam:artist"))?
            .try_clone()?
            .try_into()?;
        let title: String = value
            .get("xesam:title")
            .ok_or(anyhow!("failed to find xesam:title"))?
            .try_into()?;
        let url: String = value
            .get("xesam:url")
            .ok_or(anyhow!("failed to find xesam:url"))?
            .try_into()?;

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

#[derive(Debug, Default)]
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
    pub position: u64,
    pub rate: f64,
    pub shuffle: Option<bool>,
    pub volume: Option<f64>,
}

impl<'a> TryFrom<HashMap<&str, Value<'a>>> for PlayerCapabilities {
    type Error = anyhow::Error;

    #[instrument(skip_all)]
    fn try_from(value: HashMap<&str, Value<'a>>) -> anyhow::Result<Self> {
        let can_control: bool = value
            .get("CanControl")
            .ok_or(anyhow::anyhow!("can not find CanControl"))
            .map(TryInto::try_into)??;
        let can_next: bool = value
            .get("CanGoNext")
            .ok_or(anyhow::anyhow!("can not find CanGoNext"))
            .map(TryInto::try_into)??;
        let can_previous: bool = value
            .get("CanGoPrevious")
            .ok_or(anyhow!("can not find CanGoPrevious"))
            .map(TryInto::try_into)??;
        let can_pause: bool = value
            .get("CanPause")
            .ok_or(anyhow!("can not find CanPause"))
            .map(TryInto::try_into)??;
        let can_play: bool = value
            .get("CanPlay")
            .ok_or(anyhow!("can not find CanPlay"))
            .map(TryInto::try_into)??;
        let can_seek: bool = value
            .get("CanSeek")
            .ok_or(anyhow!("can not find CanSeek"))
            .map(TryInto::try_into)??;
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
            .map(TryInto::try_into)??;
        let position = value
            .get("Position")
            .ok_or(anyhow!("can not find Position"))
            .map(|f| match f {
                Value::U64(f) => Ok(*f),
                Value::I64(f) => Ok(*f as u64),
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

#[derive(Default)]
pub struct PlayerBuilder<'a> {
    capabilities: PlayerCapabilities,
    stream: Option<SignalStream<'a>>,
}

impl<'a> PlayerBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    #[instrument(skip_all, err)]
    pub async fn stream(mut self, conn: &Connection, name: &str) -> anyhow::Result<Self> {
        let proxy = Proxy::new(
            conn,
            BusName::WellKnown(WellKnownName::from_str_unchecked(name)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await?;

        let stream = proxy.receive_signal(DbusSignals::PropertiesChanged).await?;

        self.stream = Some(stream);

        Ok(self)
    }

    #[instrument(skip_all, err)]
    pub async fn capabilities(mut self, conn: &Connection, name: &str) -> anyhow::Result<Self> {
        let properties = conn
            .call_method(
                Some(name),
                MPRIS_PATH,
                Some(DBUS_PROPERTIES),
                DbusMethods::GetAll,
                &(MPRIS_PLAYER_PREFIX),
            )
            .await?;

        let body = properties.body();
        let properties: PlayerCapabilities =
            body.deserialize::<HashMap<&str, Value>>()?.try_into()?;

        self.capabilities = properties;

        Ok(self)
    }

    pub fn build(self) -> Player<'a> {
        Player {
            stream: self.stream.unwrap(),
            capabilities: self.capabilities,
        }
    }
}

pub struct Player<'a> {
    pub capabilities: PlayerCapabilities,
    pub stream: SignalStream<'a>,
}

impl Debug for Player<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.capabilities)
    }
}

impl<'a> Player<'a> {
    #[instrument(skip(conn), ret, err)]
    pub async fn new(conn: &Connection, name: &'a str) -> anyhow::Result<Self> {
        println!("name {name:?}");
        let properties = conn
            .call_method(
                Some(name),
                MPRIS_PATH,
                Some(DBUS_PROPERTIES),
                DbusMethods::GetAll,
                &(name),
            )
            .await?;

        let body = properties.body();
        let properties: PlayerCapabilities =
            body.deserialize::<HashMap<&str, Value>>()?.try_into()?;

        let proxy = Proxy::new(
            conn,
            BusName::WellKnown(WellKnownName::from_str_unchecked(name)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await?;

        let stream = proxy.receive_signal(DbusSignals::PropertiesChanged).await?;

        Ok(Self {
            capabilities: properties,
            stream,
        })
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

#[derive(Debug)]
pub enum NameOwnerChanged {
    NewPlayer,
    RemovedPlayer,
}

#[derive(Debug)]
pub enum PlayerUpdated {
    PlaybackStatus,
    Metadata,
    CanGoPrevious,
}

#[derive(Debug)]
pub enum MprisEvent {
    NameOwnerChanged(NameOwnerChanged),
    PlayerUpdated(PlayerUpdated),
}

#[derive(Debug)]
pub struct MprisClient<'a> {
    pub players: HashMap<String, Option<Player<'a>>>,
    owner_changed_signal: SignalStream<'a>,
}

impl<'a> MprisClient<'a> {
    pub async fn new(conn: &Connection) -> anyhow::Result<Self> {
        let name_changed = Proxy::new(conn, DBUS_NAME, DBUS_PATH, DBUS_NAME).await?;

        let stream = name_changed
            .receive_signal(DbusSignals::NameOwnerChanged)
            .await?;

        Ok(Self {
            players: HashMap::default(),
            owner_changed_signal: stream,
        })
    }

    pub async fn add(&mut self, name: &str, conn: &Connection) -> anyhow::Result<()> {
        let player = PlayerBuilder::default()
            .stream(conn, name)
            .await?
            .capabilities(conn, name)
            .await?
            .build();

        self.players.insert(name.to_string(), Some(player));

        Ok(())
    }

    pub async fn get(&self, name: &str) -> anyhow::Result<Option<&Player<'a>>> {
        match self.players.get(name) {
            Some(p) => Ok(p.as_ref()),
            None => anyhow::bail!("value did not exist"),
        }
    }

    #[instrument(skip_all, ret)]
    pub async fn get_all(&mut self, conn: &Connection) -> anyhow::Result<()> {
        let msg = conn
            .call_method(
                Some(DBUS_NAME),
                DBUS_PATH,
                Some(DBUS_NAME),
                DbusMethods::ListNames,
                &(),
            )
            .await?;

        let body = msg.body();
        let iter = body.deserialize::<Vec<&str>>()?.into_iter();

        for item in iter {
            if item.starts_with(MPRIS_PREFIX) {
                let player = PlayerBuilder::default()
                    .stream(conn, item)
                    .await?
                    .capabilities(conn, item)
                    .await?
                    .build();

                self.players.insert(item.to_string(), Some(player));
            }
        }

        Ok(())
    }

    /// handles signal changed signal
    pub async fn handle_owner_changed(
        &mut self,
        cx: &mut Context<'a>,
        conn: &Connection,
    ) -> anyhow::Result<Poll<NameOwnerChanged>> {
        if let Poll::Ready(Some(msg)) = self.owner_changed_signal.poll_next_unpin(cx) {
            let (name, old_owner, new_owner): (String, String, String) =
                msg.body().deserialize()?;

            if name.starts_with(MPRIS_PREFIX) {
                match (old_owner.is_empty(), new_owner.is_empty()) {
                    (true, false) => {
                        let p = PlayerBuilder::default()
                            .stream(conn, &name)
                            .await?
                            .capabilities(conn, &name)
                            .await?
                            .build();
                        println!("added {name:?}");
                        self.players.insert(name, Some(p));
                        return Ok(Poll::Ready(NameOwnerChanged::NewPlayer));
                    }
                    // removed player
                    (false, true) => {
                        match self.players.remove(&name) {
                            Some(_) => println!("removed player {name:?}"),
                            None => println!("key {name:?} does not exist in list of players"),
                        };

                        return Ok(Poll::Ready(NameOwnerChanged::RemovedPlayer));
                    }

                    _ => {}
                }
            }
        }

        Ok(Poll::Pending)
    }

    pub async fn handle_player_changed(
        &mut self,
        player: &mut Player<'a>,
        cx: &mut Context<'a>,
    ) -> Option<MprisEvent> {
        if let Poll::Ready(Some(msg)) = Pin::new(&mut player.stream).poll_next_unpin(cx) {
            let body = msg.body();
            // returns interface (str), changed (vec), invalidated (vec), invalidated seems to always
            // be empty
            let structure: zbus::zvariant::Structure = body.deserialize().unwrap();

            let iface: zbus::zvariant::Str = structure.fields()[0].clone().try_into().unwrap();
            let changed: HashMap<String, zbus::zvariant::OwnedValue> =
                structure.fields()[1].clone().try_into().unwrap();

            println!("iface {iface} changed {changed:?}]");

            if let Some(status) = changed.get("PlaybackStatus") {
                let val = &**status;
                player.capabilities_mut().playback_status = val.try_into().unwrap();

                return Some(MprisEvent::PlayerUpdated(PlayerUpdated::PlaybackStatus));
            }
            if let Some(status) = changed.get("Metadata") {
                let val = &**status;
                if let Value::Dict(dict) = val {
                    let map: HashMap<String, Value> = dict.try_clone().unwrap().try_into().unwrap();
                    let metadata: Metadata = map.try_into().ok()?;
                    println!("{metadata:?}");
                    return Some(MprisEvent::PlayerUpdated(PlayerUpdated::Metadata));
                }
            }
            if let Some(status) = changed.get("CanGoPrevious") {
                player.capabilities_mut().can_previous = status.try_into().unwrap();

                return Some(MprisEvent::PlayerUpdated(PlayerUpdated::CanGoPrevious));
            }
        }
        None
    }

    pub async fn handle_players_changed(&mut self, cx: &mut Context<'a>) -> Option<MprisEvent> {
        for (name, player) in self.players.iter_mut() {
            if player.is_none() {
                continue;
            }
            let player = player.as_mut().unwrap();
            if let Poll::Ready(Some(msg)) = Pin::new(&mut player.stream).poll_next_unpin(cx) {
                let body = msg.body();
                // returns interface (str), changed (vec), invalidated (vec), invalidated seems to always
                // be empty
                let structure: zbus::zvariant::Structure = body.deserialize().unwrap();

                let iface: zbus::zvariant::Str = structure.fields()[0].clone().try_into().unwrap();
                let changed: HashMap<String, zbus::zvariant::OwnedValue> =
                    structure.fields()[1].clone().try_into().unwrap();

                println!("name {name} iface {iface} changed {changed:?}]");

                if let Some(status) = changed.get("PlaybackStatus") {
                    let val = &**status;
                    player.capabilities_mut().playback_status = val.try_into().unwrap();

                    return Some(MprisEvent::PlayerUpdated(PlayerUpdated::PlaybackStatus));
                }
                if let Some(status) = changed.get("Metadata") {
                    let val = &**status;
                    if let Value::Dict(dict) = val {
                        let map: HashMap<String, Value> = dict.try_clone().ok()?.try_into().ok()?;
                        let metadata: Metadata = map.try_into().ok()?;
                        println!("{metadata:?}");
                    }

                    return Some(MprisEvent::PlayerUpdated(PlayerUpdated::Metadata));
                }
                if let Some(status) = changed.get("CanGoPrevious") {
                    player.capabilities_mut().can_previous = status.try_into().unwrap();

                    return Some(MprisEvent::PlayerUpdated(PlayerUpdated::CanGoPrevious));
                }
            }
        }

        None
    }

    pub async fn event(&mut self, ctx: &mut Context<'a>, conn: &Connection) -> Option<MprisEvent> {
        if let Some(event) = self.handle_players_changed(ctx).await {
            info!(?event);
            return Some(event);
        }
        if let Ok(Poll::Ready(changed)) = self.handle_owner_changed(ctx, conn).await {
            info!(?changed);
            return Some(MprisEvent::NameOwnerChanged(changed));
        }

        None
    }
}
