#[tokio::main]
async fn main() {
    let url = "https://cht.sh/rust/read+file?T";
    let resp = reqwest::Client::new().get(url).send().await.unwrap();
    println!("Status: {}", resp.status());
}
