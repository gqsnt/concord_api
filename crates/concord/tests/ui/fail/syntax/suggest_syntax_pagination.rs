use concord::api;

api! { client Via { base "https://example.test/"; GET read(cursor: u32) -> () { paginate vai Controller {} } } }
api! { client Init { base "https://example.test/"; GET read(cursor: u32) -> () { paginate via Controller { inti Init {}; cursor -> cursor; } } } }
api! { client Cursor { base "https://example.test/"; GET read(cursor: u32) -> () { paginate via Controller { init Init {}; crusor -> cursor; } } } }

fn main() {}
