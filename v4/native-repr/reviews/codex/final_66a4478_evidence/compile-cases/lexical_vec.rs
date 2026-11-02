
wire::api! { pub client Api { base "http://127.0.0.1"; response json; GET p() -> Vec<String>; GET q() -> Vec<String> {} } }
