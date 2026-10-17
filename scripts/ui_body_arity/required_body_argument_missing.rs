use concord::{Text, api};

api! {
    client RequiredBodyApi {
        base "http://body.example.test/";

        POST upload(body payload: String via Text) -> String via Text;
    }
}

fn main() {
    let client = RequiredBodyApi::new().expect("client");
    let _ = client.upload();
}
