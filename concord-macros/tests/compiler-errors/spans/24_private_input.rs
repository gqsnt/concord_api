#[derive(Clone)]
struct Private;

concord::api! {
    pub client Api {
        base "https://example.invalid";
        GET narrow(id: u64, hidden: Private) {
            path "{id}";
            query { "hidden" = { let _ = hidden; 1 }; }
        }
        GET wide(
            a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8,
            i: u8, j: u8, k: u8, l: u8, m: u8, n: u8, o: u8, p: u8,
            hidden: Private,
        ) {
            query { a; b; c; d; e; f; g; h; i; j; k; l; m; n; o; p;
                    "hidden" = { let _ = hidden; 1 }; }
        }
    }
}

fn main() {}
