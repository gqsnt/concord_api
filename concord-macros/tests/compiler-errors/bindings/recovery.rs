concord::api! {
    pub client Three {
        base "https://three.test";
        read concord::Json;
        GET first() -> u32;
        GET refused() { path "{missing}"; }
        GET last() -> u32;
    }
}
fn main() {
    let _ = async {
        let client = Three::new();
        let _: u32 = client.first().await.unwrap();
        let _: u32 = client.last().await.unwrap();
    };
}
