use wire::reqwest::header::HeaderValue as HV; type Alias=HV;
wire::api! { pub client Api(header("content-type") ct?: HV) { base "http://127.0.0.1"; request json("application/custom"); POST p(json b: String); } }
