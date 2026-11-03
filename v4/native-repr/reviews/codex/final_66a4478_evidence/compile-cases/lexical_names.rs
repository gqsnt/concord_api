
wire::api! { pub client Api { base "http://127.0.0.1";  POST p(json: String, json query: String) { query { "q" = json; } } GET q(header("x") json?: String); } }
