use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};

use futures::StreamExt;
use mpris_controller::{
    DBUS_NAME, DBUS_PATH, DbusMethods, DbusSignals, MPRIS_PREFIX, Metadata, Player,
};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedValue, Str, Structure, Value},
};

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

    let mut players: HashMap<String, Player> = HashMap::new();

    for player_name in &data {
        let player = Player::new(&conn, player_name.to_string()).await;
        players.insert(player_name.to_string(), player);
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
                println!("name {:?}", player.0);
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
                    player.1.capabilities_mut().playback_status = val.try_into().unwrap();
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
                    player.1.capabilities_mut().can_previous = status.try_into().unwrap();
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

                println!("removed_players = {data:?}");

                match (old_owner.is_empty(), new_owner.is_empty()) {
                    // added player
                    // needs to call ListNames to convert the name (unique name) to well known name
                    (true, false) => {
                        let p = Player::new(&conn, name.clone()).await;
                        players.insert(name.to_string(), p);
                    }
                    // removed player
                    (false, true) => {
                        data.iter().for_each(|f| {
                            if !players.contains_key(&f.to_string()) {
                                players.remove(&f.to_string());
                            }
                        });
                    }

                    _ => {}
                }
            }
        }
    }
}
