
wire::api! { pub client Api { base "http://127.0.0.1"; request json; response form; POST p(body b: String) -> String; POST q(body b: String) -> String; } }
