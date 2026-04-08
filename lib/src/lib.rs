use std::{
    collections::HashMap,
    fmt::Debug,
    ptr::null,
    slice::Iter,
    task::{Poll, RawWaker, RawWakerVTable, Waker},
};

pub mod player;

pub mod format {
    include!(concat!(env!("OUT_DIR"), "/format.rs"));
}

pub use format::*;

use zbus::{
    names::{BusName, MemberName, WellKnownName},
    proxy::SignalStream,
    Connection, Proxy,
};

use crate::player::{PlaybackStatus, Player, PlayerUpdated};

const unsafe fn noop_clone(_data: *const ()) -> RawWaker {
    noop_raw_waker()
}

unsafe fn noop(_data: *const ()) {}

const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);

const fn noop_raw_waker() -> RawWaker {
    RawWaker::new(null(), &NOOP_WAKER_VTABLE)
}

/// Create a new [`Waker`] which does
/// nothing when `wake()` is called on it.
///
/// # Examples
///
/// ```
/// use futures::task::noop_waker;
/// let waker = noop_waker();
/// waker.wake();
/// ```
#[inline]
pub const fn noop_waker() -> Waker {
    // FIXME: Since 1.46.0 we can use transmute in consts, allowing this function to be const.
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

pub const WAKER: Waker = noop_waker();

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

static mut SIGNAL_STREAM: Vec<(usize, SignalStream<'static>)> = Vec::new();

pub struct MprisClient {
    player_names: HashMap<String, usize>,
    players: Vec<Player>,
    // owner_changed_signal: SignalStream<'a>,
    next_id: usize,
    connection: Connection,
}

impl MprisClient {
    pub async fn new() -> anyhow::Result<Self> {
        let connection = Connection::session().await?;
        // let name_changed = Proxy::new(&connection, DBUS_NAME, DBUS_PATH, DBUS_NAME).await?;

        // let stream = name_changed
        //     .receive_signal(DbusSignals::NameOwnerChanged)
        //     .await?;

        Ok(Self {
            player_names: HashMap::default(),
            players: Vec::new(),
            // owner_changed_signal: stream,
            next_id: 0,
            connection,
        })
    }

    pub async fn add(&mut self, name: String) -> anyhow::Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            BusName::WellKnown(WellKnownName::from_str_unchecked(&name)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await?;

        let stream = proxy.receive_signal(DbusSignals::PropertiesChanged).await?;

        unsafe {
            SIGNAL_STREAM.push((self.next_id, stream));
        }
        let player = Player::new(&self.connection, name).await?;

        self.players.push(player);

        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Player> {
        match self.player_names.get(name) {
            Some(id) => self.players.get(*id),
            None => None,
        }
    }

    pub fn get_id(&self, name: &str) -> Option<usize> {
        self.player_names.get(name).copied()
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Player> {
        match self.player_names.get_mut(name) {
            Some(id) => self.players.get_mut(*id),
            None => None,
        }
    }

    pub fn get_from_id(&self, id: usize) -> Option<&Player> {
        self.players.get(id)
    }

    pub fn get_from_id_mut(&mut self, id: usize) -> Option<&mut Player> {
        self.players.get_mut(id)
    }

    pub async fn list_names(&mut self) -> anyhow::Result<Vec<String>> {
        let msg = self
            .connection
            .call_method(
                Some(DBUS_NAME),
                DBUS_PATH,
                Some(DBUS_NAME),
                DbusMethods::ListNames,
                &(),
            )
            .await?;

        let body = msg.body();
        let names = body.deserialize::<Vec<String>>()?;

        Ok(names)
    }

    // #[instrument(skip_all, ret)]
    pub async fn get_all(&mut self) -> anyhow::Result<()> {
        if !self.players.is_empty() {
            self.players.clear();
            self.player_names.clear();
            self.next_id = 0;
        }
        let names = self.list_names().await?;
        for item in names {
            if item.starts_with(MPRIS_PREFIX) {
                self.player_names.insert(item.clone(), self.next_id);
                self.add(item).await?;
                self.next_id += 1;
            }
        }

        Ok(())
    }

    // handles signal changed signal
    // pub async fn handle_owner_changed(&mut self) -> anyhow::Result<Poll<NameOwnerChanged>> {
    //     let waker = WAKER;
    //     let mut cx = std::task::Context::from_waker(&waker);
    //     if let Poll::Ready(Some(msg)) = self.owner_changed_signal.poll_next_unpin(&mut cx) {
    //         let body = msg.body();
    //         let (name, old_owner, new_owner): (String, &str, &str) = body.deserialize()?;
    //
    //         if name.starts_with(MPRIS_PREFIX) {
    //             match (old_owner.is_empty(), new_owner.is_empty()) {
    //                 (true, false) => {
    //                     println!("added {name:?}");
    //                     self.player_names.insert(name.clone(), self.next_id);
    //
    //                     self.players
    //                         .insert(self.next_id, Player::new(&self.connection, name).await?);
    //                     self.next_id += 1;
    //                     return Ok(Poll::Ready(NameOwnerChanged::NewPlayer));
    //                 }
    //                 // removed player
    //                 (false, true) => {
    //                     if let Some(id) = self.player_names.get(&name) {
    //                         self.players.remove(*id);
    //
    //                         return Ok(Poll::Ready(NameOwnerChanged::RemovedPlayer));
    //                     }
    //                 }
    //
    //                 _ => {}
    //             }
    //         }
    //     }
    //
    //     Ok(Poll::Pending)
    // }

    pub async fn handle_player_changed(player: &mut Player, index: usize) -> anyhow::Result<()> {
        unsafe {
            if let Poll::Ready(ev) =
                player::poll_player(&mut SIGNAL_STREAM.get_mut(index).unwrap().1).await?
            {
                match ev {
                    PlayerUpdated::PlaybackStatus(playback_status) => {
                        player.capabilities.playback_status = playback_status
                    }
                    PlayerUpdated::Metadata(metadata) => player.capabilities.metadata = *metadata,
                    PlayerUpdated::CanGoPrevious(can_previous) => {
                        player.capabilities.can_previous = can_previous;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn handle_players_changed(&mut self) {
        for id in self.player_names.values() {
            let _ = MprisClient::handle_player_changed(
                self.players.get_mut(*id).expect("invalid player id"),
                *id,
            )
            .await;
        }
    }

    pub async fn event(&mut self) {
        for (i, player) in self.players.iter_mut().enumerate() {
            unsafe {
                if let Poll::Ready(ev) =
                    player::poll_player(&mut SIGNAL_STREAM.get_mut(i).unwrap().1)
                        .await
                        .expect("error polling player")
                {
                    match ev {
                        PlayerUpdated::PlaybackStatus(playback_status) => {
                            player.capabilities.playback_status = playback_status
                        }
                        PlayerUpdated::Metadata(metadata) => {
                            player.capabilities.metadata = *metadata
                        }
                        PlayerUpdated::CanGoPrevious(can_previous) => {
                            player.capabilities.can_previous = can_previous;
                        }
                    };
                }
            }
        }

        // if let Ok(Poll::Ready(changed)) = self.handle_owner_changed().await {
        // tracing::info!(?changed);
        // }
    }

    pub fn player_names(&self) -> Iter<'_, Player> {
        self.players.iter()
    }

    /// returns the first player it finds playing audio
    pub fn currently_playing(&self) -> Option<&Player> {
        self.players
            .iter()
            .find(|&player| player.capabilities.playback_status == PlaybackStatus::Playing)
            .map(|v| v as _)
    }

    pub fn currently_playing_mut(&mut self) -> Option<&mut Player> {
        self.players
            .iter_mut()
            .find(|player| player.capabilities.playback_status == PlaybackStatus::Playing)
            .map(|v| v as _)
    }
}

impl Debug for MprisClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.players)
    }
}
