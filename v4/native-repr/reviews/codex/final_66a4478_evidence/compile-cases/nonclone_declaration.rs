pub struct NativeBody; impl wire::Encode<wire::reqwest::Body> for NativeBody { const CONTENT_TYPE:Option<&'static str>=None; fn encode(v:wire::reqwest::Body)->Result<wire::reqwest::Body,wire::BodyError>{Ok(v)} }
wire::api! { pub client Api { base "http://127.0.0.1";  POST p(NativeBody b: wire::reqwest::Body); } }
