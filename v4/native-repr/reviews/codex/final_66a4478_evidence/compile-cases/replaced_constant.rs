pub struct Bad; impl wire::Encode<String> for Bad { const CONTENT_TYPE:Option<&'static str>=Some("bad\nheader"); fn encode(v:String)->Result<wire::reqwest::Body,wire::BodyError>{Ok(v.into())} }
wire::api! { pub client Api { base "http://127.0.0.1";  POST p(Bad("text/plain") b: String); } }
