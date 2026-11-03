use wire::reqwest::header::HeaderValue as HV; type Alias=HV;
wire::api! { pub client Api(header("content-type") ct?: HV) { base "http://127.0.0.1";  POST p(json("application/custom") b: String) { headers { remove "content-type"; "content-type" = "readded"; } } } }
