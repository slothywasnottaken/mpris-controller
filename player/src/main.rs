use std::{collections::HashMap, pin::Pin};

use lib::player::{Capabilities, LoopStatus, MetadataBuilder};
use zbus::{
    Connection, ObjectServer, Result, fdo,
    message::{self, Header, Type},
    names::{InterfaceName, MemberName, WellKnownName},
    object_server::{DispatchResult, Interface, SignalEmitter},
    zvariant::{ObjectPath, OwnedValue, Value},
};

#[derive(Debug)]
struct Controller {
    p: Capabilities,
}

unsafe impl Send for Controller {}

type ReturnType = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

impl Controller {
    fn n(&self) -> ReturnType {
        let fut: ReturnType = Box::pin(async { Ok(()) });

        fut
    }
}

#[allow(unused)]
#[async_trait::async_trait]
impl Interface for Controller {
    #[doc = " Return the name of the interface. Ex: \"org.foo.MyInterface\""]
    fn name() -> InterfaceName<'static>
    where
        Self: Sized,
    {
        InterfaceName::from_static_str_unchecked("org.mpris.MediaPlayer2.Player")
    }

    #[doc = " Get a property value. Returns `None` if the property doesn\'t exist."]
    #[doc = ""]
    #[doc = " Note: The header parameter will be None when the getter is not being called as part"]
    #[doc = " of D-Bus communication (for example, when it is called as part of initial object setup,"]
    #[doc = " before it is registered on the bus, or when we manually send out property changed"]
    #[doc = " notifications)."]
    #[must_use]
    #[allow(
        mismatched_lifetime_syntaxes,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds
    )]
    async fn get(
        &self,
        property_name: &str,
        server: &ObjectServer,
        connection: &Connection,
        header: Option<&message::Header<'_>>,
        emitter: &SignalEmitter<'_>,
    ) -> Option<zbus::fdo::Result<OwnedValue>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
        'life4: 'async_trait,
        'life5: 'async_trait,
        'life6: 'async_trait,
        'life7: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    #[doc = " Return all the properties."]
    #[must_use]
    #[allow(
        mismatched_lifetime_syntaxes,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds
    )]
    async fn get_all(
        &self,
        object_server: &ObjectServer,
        connection: &Connection,
        header: Option<&message::Header<'_>>,
        emitter: &SignalEmitter<'_>,
    ) -> fdo::Result<HashMap<String, OwnedValue>> {
        let map: HashMap<String, OwnedValue> = self.p.clone().into();

        return Ok(map);
    }

    #[doc = " Set a property value."]
    #[doc = ""]
    #[doc = " Returns `None` if the property doesn\'t exist."]
    #[doc = ""]
    #[doc = " This will only be invoked if `set` returned `RequiresMut`."]
    #[must_use]
    #[allow(
        mismatched_lifetime_syntaxes,
        clippy::type_complexity,
        clippy::type_repetition_in_bounds
    )]
    fn set_mut<
        'life0,
        'life1,
        'life2,
        'life3,
        'life4,
        'life5,
        'life6,
        'life7,
        'life8,
        'life9,
        'async_trait,
    >(
        &'life0 mut self,
        property_name: &'life1 str,
        value: &'life2 Value<'life3>,
        object_server: &'life4 ObjectServer,
        connection: &'life5 Connection,
        header: Option<&'life6 Header<'life7>>,
        emitter: &'life8 SignalEmitter<'life9>,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Option<fdo::Result<()>>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
        'life4: 'async_trait,
        'life5: 'async_trait,
        'life6: 'async_trait,
        'life7: 'async_trait,
        'life8: 'async_trait,
        'life9: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    #[doc = " Call a method."]
    #[doc = ""]
    #[doc = " Return [`DispatchResult::NotFound`] if the method doesn\'t exist, or"]
    #[doc = " [`DispatchResult::RequiresMut`] if `call_mut` should be used instead."]
    #[doc = ""]
    #[doc = " It is valid, though inefficient, for this to always return `RequiresMut`."]
    fn call<'call>(
        &'call self,
        server: &'call ObjectServer,
        connection: &'call Connection,
        msg: &'call zbus::Message,
        name: MemberName<'call>,
    ) -> DispatchResult<'call> {
        let header = msg.header();
        let t = header.message_type();
        if let (Some(path), Some(iface), Some(member)) =
            (header.path(), header.interface(), header.member())
            && header.message_type() == Type::MethodCall
            && path == &ObjectPath::from_static_str_unchecked("/org/mpris/MediaPlayer2")
            && iface == &InterfaceName::from_static_str_unchecked("org.mpris.MediaPlayer2.Player")
        {
            match member.as_str() {
                "Next" => {}
                "Previous" => {}
                "Pause" => {}
                "PlayPause" => {}
                "Stop" => {}
                "Play" => {}
                "Seek" => {}
                "SetPosition" => {}
                "OpenUri" => {}

                _ => todo!("unsupported type {msg} {name}"),
            }
        }
        println!("{msg:?} {name:?}");

        let fut: ReturnType = self.n();

        DispatchResult::Async(fut)
    }

    #[doc = " Call a `&mut self` method."]
    #[doc = ""]
    #[doc = " This will only be invoked if `call` returned `RequiresMut`."]
    fn call_mut<'call>(
        &'call mut self,
        server: &'call ObjectServer,
        connection: &'call Connection,
        msg: &'call zbus::Message,
        name: MemberName<'call>,
    ) -> DispatchResult<'call> {
        todo!()
    }

    #[doc = " Write introspection XML to the writer, with the given indentation level."]
    fn introspect_to_writer(&self, writer: &mut dyn std::fmt::Write, level: usize) {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let conn = Connection::session().await.unwrap();

    let controller = Controller {
        p: Capabilities {
            can_control: true,
            can_next: true,
            can_previous: true,
            can_pause: true,
            can_play: true,
            can_seek: true,
            loop_status: Some(LoopStatus::None),
            max_rate: Some(1.0),
            min_rate: Some(0.0),
            metadata: MetadataBuilder::default()
                .artists(vec!["Hello".to_string(), "World".to_string()])
                .length(10000)
                .title(String::from("sailor"))
                .finish(),
            playback_status: lib::player::PlaybackStatus::Playing,
            position: 0,
            rate: 1.0,
            shuffle: Some(false),
            volume: Some(1.0),
        },
    };

    conn.object_server()
        .at("/org/mpris/MediaPlayer2", controller)
        .await
        .unwrap();
    let name = WellKnownName::from_static_str_unchecked("org.mpris.MediaPlayer2.controller");
    conn.request_name(&name).await.unwrap();

    loop {}
}
