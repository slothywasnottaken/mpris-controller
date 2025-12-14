use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};

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

#[derive(Default)]
pub struct PlayerBuilder<'a> {
    capabilities: PlayerCapabilities,
    stream: Option<SignalStream<'a>>,
}

impl<'a> PlayerBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn stream(mut self, conn: &Connection, name: &str) -> Self {
        let proxy = Proxy::new(
            conn,
            BusName::WellKnown(WellKnownName::from_str_unchecked(name)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await
        .unwrap();

        let stream = proxy
            .receive_signal(DbusSignals::PropertiesChanged)
            .await
            .unwrap();

        self.stream = Some(stream);

        self
    }

    pub async fn capabilities(mut self, conn: &Connection, name: &str) -> Self {
        let properties = conn
            .call_method(
                Some(name),
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

        self.capabilities = properties;

        self
    }

    pub fn build(self) -> Player<'a> {
        Player {
            stream: self.stream.unwrap(),
            capabilities: self.capabilities,
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
    pub async fn new(conn: &Connection, name: &'a str) -> Self {
        println!("name {name:?}");
        let properties = conn
            .call_method(
                Some(name),
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

        let proxy = Proxy::new(
            conn,
            BusName::WellKnown(WellKnownName::from_str_unchecked(name)),
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

#[derive(Debug)]
pub struct PlayerFinder<'a> {
    players: HashMap<String, Option<Player<'a>>>,
    owner_changed_signal: SignalStream<'a>,
}

impl<'a> PlayerFinder<'a> {
    pub async fn new(conn: &Connection) -> Self {
        let name_changed = Proxy::new(conn, DBUS_NAME, DBUS_PATH, DBUS_NAME)
            .await
            .unwrap();

        let stream = name_changed
            .receive_signal(DbusSignals::NameOwnerChanged)
            .await
            .unwrap();

        Self {
            players: HashMap::default(),
            owner_changed_signal: stream,
        }
    }

    pub async fn get(
        &mut self,
        name: &str,
        conn: &Connection,
    ) -> anyhow::Result<Option<&Player<'a>>> {
        if !self.players.contains_key(name) {
            let msg = conn
                .call_method(
                    Some(DBUS_NAME),
                    DBUS_PATH,
                    Some(DBUS_NAME),
                    DbusMethods::NameHasOwner,
                    &(),
                )
                .await?;

            let body = msg.body();
            let has_owner = body.deserialize::<bool>()?;

            if !has_owner {
                return Ok(None);
            }

            let player = PlayerBuilder::default()
                .stream(conn, name)
                .await
                .capabilities(conn, name)
                .await
                .build();

            self.players.insert(name.to_string(), Some(player));
            let p = self.players.get(name).unwrap();

            return Ok(p.as_ref());
        }

        let p = self.players.get(name).unwrap();
        Ok(p.as_ref())
    }

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
            if item.starts_with(MPRIS_PLAYER_PREFIX) {
                let player = PlayerBuilder::default()
                    .stream(conn, item)
                    .await
                    .capabilities(conn, item)
                    .await
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
    ) -> anyhow::Result<Poll<bool>> {
        if let Poll::Ready(Some(msg)) = self.owner_changed_signal.poll_next_unpin(cx) {
            let (name, old_owner, new_owner): (String, String, String) =
                msg.body().deserialize()?;

            if name.starts_with(MPRIS_PREFIX) {
                match (old_owner.is_empty(), new_owner.is_empty()) {
                    (true, false) => {
                        let p = PlayerBuilder::default()
                            .stream(conn, &name)
                            .await
                            .capabilities(conn, &name)
                            .await
                            .build();
                        println!("added {name:?}");
                        self.players.insert(name, Some(p));
                    }
                    // removed player
                    (false, true) => {
                        match self.players.remove(&name) {
                            Some(_) => println!("removed player {name:?}"),
                            None => println!("key {name:?} does not exist in list of players"),
                        };
                    }

                    _ => {}
                }
            }
            return Ok(Poll::Ready(true));
        }
        Ok(Poll::Ready(false))
    }

    pub async fn handle_player_changed(&mut self, player: &mut Player<'a>, cx: &mut Context<'a>) {
        if let Poll::Ready(Some(msg)) = Pin::new(&mut player.stream).poll_next_unpin(cx) {
            let body = msg.body();
            // returns interface (str), changed (vec), invalidated (vec), invalidated seems to always
            // be empty
            let s: zbus::zvariant::Structure = body.deserialize().unwrap();

            let iface: zbus::zvariant::Str = s.fields()[0].clone().try_into().unwrap();
            let changed: HashMap<String, zbus::zvariant::OwnedValue> =
                s.fields()[1].clone().try_into().unwrap();

            println!("iface {iface} changed {changed:?}]");

            if let Some(status) = changed.get("PlaybackStatus") {
                let val = &**status;
                player.capabilities_mut().playback_status = val.try_into().unwrap();
            }
            if let Some(status) = changed.get("Metadata") {
                let val = &**status;
                if let Value::Dict(dict) = val {
                    let map: HashMap<String, Value> = dict.try_clone().unwrap().try_into().unwrap();
                    let metadata: Metadata = map.into();
                    println!("{metadata:?}");
                }
            }
            if let Some(status) = changed.get("CanGoPrevious") {
                player.capabilities_mut().can_previous = status.try_into().unwrap();
            }
        }
    }

    pub async fn handle_players_changed(&mut self, cx: &mut Context<'a>) {
        for (name, player) in self.players.iter_mut() {
            if player.is_none() {
                continue;
            }
            let player = player.as_mut().unwrap();
            if let Poll::Ready(Some(msg)) = Pin::new(&mut player.stream).poll_next_unpin(cx) {
                let body = msg.body();
                // returns interface (str), changed (vec), invalidated (vec), invalidated seems to always
                // be empty
                let s: zbus::zvariant::Structure = body.deserialize().unwrap();

                let iface: zbus::zvariant::Str = s.fields()[0].clone().try_into().unwrap();
                let changed: HashMap<String, zbus::zvariant::OwnedValue> =
                    s.fields()[1].clone().try_into().unwrap();

                println!("name {name} iface {iface} changed {changed:?}]");

                if let Some(status) = changed.get("PlaybackStatus") {
                    let val = &**status;
                    player.capabilities_mut().playback_status = val.try_into().unwrap();
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
                    player.capabilities_mut().can_previous = status.try_into().unwrap();
                }
            }
        }
    }
}
