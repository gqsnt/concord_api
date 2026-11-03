use wire::reqwest::header::HeaderValue as HV; type Alias=HV; static V:HV=HV::from_static("v"); pub fn run(){let a=Api::new(wire::reqwest::Client::new());let _=a.p(&V).set_v(&V).prepare();}
wire::api! { pub client Api(header("x-root") v: Alias = &V) { base "http://127.0.0.1";  GET p(header("x") v: Alias); } }
