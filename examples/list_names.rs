use mpris_controller::MprisClient;
use zbus::Connection;

#[tokio::main]
async fn main() {
    let conn = Connection::session().await.unwrap();
    let mut client = MprisClient::new(&conn).await.unwrap();
    client.get_all(&conn).await.unwrap();
}
