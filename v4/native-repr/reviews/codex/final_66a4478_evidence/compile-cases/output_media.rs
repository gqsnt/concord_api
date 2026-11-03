
wire::api! { pub client Api { base "http://127.0.0.1";  GET p() -> json("text/plain") String; } }
