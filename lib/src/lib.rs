use std::{
    fmt::Debug,
    ptr::null,
    sync::LazyLock,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

pub mod player;

pub mod format {
    include!(concat!(env!("OUT_DIR"), "/format.rs"));
}

pub use format::*;
use futures::{executor::block_on, StreamExt};

use std::sync::Mutex;
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

#[derive(Debug, Clone)]
pub enum NameOwnerChanged {
    NewPlayer(String),
    RemovedPlayer(String),
}

static mut SIGNAL_STREAM: LazyLock<Mutex<Vec<SignalStream<'static>>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Debug, Default)]
pub struct MprisClient {
    players: Vec<Player>,
    next_id: usize,
}

impl MprisClient {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            players: Vec::new(),
            next_id: 0,
        })
    }

    pub async fn add(&mut self, connection: &Connection, name: String) -> anyhow::Result<()> {
        let proxy = Proxy::new(
            connection,
            BusName::WellKnown(WellKnownName::from_str_unchecked(&name)),
            MPRIS_PATH,
            DBUS_PROPERTIES,
        )
        .await?;

        let stream = proxy.receive_signal(DbusSignals::PropertiesChanged).await?;

        unsafe {
            SIGNAL_STREAM.lock().unwrap().push(stream);
        }
        let player = Player::new(connection, name.clone()).await?;

        self.players.push(player);

        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Player> {
        self.players
            .iter()
            .find(|&p| p.name() == name)
            .map(|v| v as _)
    }

    pub fn get_id(&self, name: &str) -> Option<usize> {
        for (i, p) in self.players.iter().enumerate() {
            if p.name() == name {
                return Some(i);
            }
        }

        None
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Player> {
        self.players
            .iter_mut()
            .find(|p| p.name() == name)
            .map(|v| v as _)
    }

    pub fn get_from_id(&self, id: usize) -> Option<&Player> {
        self.players.get(id)
    }

    pub fn get_from_id_mut(&mut self, id: usize) -> Option<&mut Player> {
        self.players.get_mut(id)
    }

    pub async fn list_names(connection: &Connection) -> anyhow::Result<Vec<String>> {
        let msg = connection
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
    pub async fn get_all(&mut self, connection: &Connection) -> anyhow::Result<()> {
        if !self.players.is_empty() {
            self.players.clear();
            self.next_id = 0;
        }
        let names = Self::list_names(connection).await.unwrap();
        for item in names {
            if item.starts_with(MPRIS_PREFIX) {
                self.add(connection, item).await.unwrap();
                self.next_id += 1;
            }
        }

        Ok(())
    }

    pub async fn handle_player_changed(player: &mut Player, index: usize) {
        unsafe {
            if let Poll::Ready(ev) =
                player::poll_player(&mut SIGNAL_STREAM.lock().unwrap().get_mut(index).unwrap())
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
    }

    pub async fn handle_players_changed(&mut self) {
        for (i, player) in self.players.iter_mut().enumerate() {
            let _ = MprisClient::handle_player_changed(player, i).await;
        }
    }

    pub async fn event(&mut self, connection: &Connection) -> Option<NameOwnerChanged> {
        for (i, player) in self.players.iter_mut().enumerate() {
            unsafe {
                let mut lock = SIGNAL_STREAM.lock().unwrap();
                if let Poll::Ready(ev) = player::poll_player(lock.get_mut(i).unwrap()) {
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

        #[cfg(feature = "owner_changed")]
        return self.handle_owner_changed(connection).await;

        None
    }

    #[cfg(feature = "owner_changed")]
    pub async fn handle_owner_changed(
        &mut self,
        connection: &Connection,
    ) -> Option<NameOwnerChanged> {
        if let Ok(Poll::Ready(changed)) = poll_owner_changed(&self.player_names()).await {
            match changed {
                NameOwnerChanged::NewPlayer(ref name) => {
                    let p = Player::new(connection, name.clone()).await.unwrap();
                    self.players.push(p);
                    return Some(changed);
                }
                NameOwnerChanged::RemovedPlayer(ref name) => {
                    if let Some(idx) = self.get_id(name) {
                        self.players.remove(idx);
                    }
                    return Some(changed);
                }
            }
        }

        None
    }

    pub fn player_names(&self) -> Vec<&str> {
        self.players().iter().map(|f| f.name()).collect::<Vec<_>>()
    }

    pub fn players(&self) -> &[Player] {
        &self.players
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

#[cfg(feature = "owner_changed")]
static mut OWNER_CHANGED_SIGNAL: LazyLock<Mutex<Option<SignalStream<'static>>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

#[cfg(feature = "owner_changed")]
pub async fn init_owner_changed_signal() {
    let connection = zbus::Connection::session().await.unwrap();
    let name_changed = zbus::Proxy::new(&connection, DBUS_NAME, DBUS_PATH, DBUS_NAME)
        .await
        .unwrap();

    let stream = name_changed
        .receive_signal(DbusSignals::NameOwnerChanged)
        .await
        .unwrap();

    unsafe {
        *OWNER_CHANGED_SIGNAL.lock().unwrap() = Some(stream);
    }
}

#[cfg(feature = "owner_changed")]
pub async fn poll_owner_changed(names: &Vec<&str>) -> anyhow::Result<Poll<NameOwnerChanged>> {
    unsafe {
        let waker = WAKER;
        let mut ctx = Context::from_waker(&waker);
        if let Poll::Ready(Some(msg)) = OWNER_CHANGED_SIGNAL
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .poll_next_unpin(&mut ctx)
        {
            let body = msg.body();
            let (name, old_owner, new_owner): (String, &str, &str) = body.deserialize()?;

            if name.starts_with(MPRIS_PREFIX) {
                match (old_owner.is_empty(), new_owner.is_empty()) {
                    (true, false) => {
                        return Ok(Poll::Ready(NameOwnerChanged::NewPlayer(name)));
                    }
                    // removed player
                    (false, true) => {
                        for n_names in names.iter() {
                            if n_names == &name {
                                return Ok(Poll::Ready(NameOwnerChanged::RemovedPlayer(name)));
                            }
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    Ok(Poll::Pending)
}
