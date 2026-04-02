use mpris_controller::MprisClient;

#[tokio::main]
async fn main() {
    let mut client = MprisClient::new().await.unwrap();
    client.get_all().await.unwrap();
}
