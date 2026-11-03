use wire::reqwest::header::HeaderValue as HV; type Alias=HV;
wire::api! { pub client Api { base "http://127.0.0.1";  GET p(v: Alias) { headers { "x" = "{v}"; } } } }
