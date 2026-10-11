// V3-L10 / §4.3 mutation matrix: controls exist only where the state allows
// the transition. A required uninitialized positional input has no controls;
// an initialized required input has no `omit_x`/`replace_x`; an uninitialized
// optional has no `reset_x`; a required root value has no omission/reset; a
// required body has no controls; an optional body has exact-payload controls
// (no conversion) and no default/reset.
use concord::{Binary, Bytes, Text, api};

const DEFAULT_REGION: &str = "eu";

api! {
    client ControlMatrixApi {
        base "https://matrix.example.test";
        values {
            query tenant: String;
            query region: &'static str = crate::DEFAULT_REGION;
            query locale?: String;
        }

        GET read(query id: String, query limit: u32 = 25, query verbose?: bool) -> Bytes via Binary;
        POST write(body payload: String via Text) -> Bytes via Binary;
        PUT put(body payload?: String via Text) -> Bytes via Binary;
    }
}

fn main() {
    // Required root value: builder `set_`/`set_*_if_some` only.
    let _ = ControlMatrixApi::builder().set_tenant("t").set_tenant_if_some(None);
    let _ = ControlMatrixApi::builder().omit_tenant();
    let _ = ControlMatrixApi::builder().reset_tenant();
    let _ = ControlMatrixApi::builder().replace_tenant(Some("t".to_owned()));
    // Initialized required root value: no omission or exact replacement.
    let _ = ControlMatrixApi::builder().omit_region();
    let _ = ControlMatrixApi::builder().replace_region(Some("us"));
    // Uninitialized optional root value: no reset.
    let _ = ControlMatrixApi::builder().reset_locale();

    let client = ControlMatrixApi::builder().set_tenant("t").build().unwrap();
    // Required positional endpoint input: no controls at all.
    let _ = client.read("1").set_id("2");
    let _ = client.read("1").set_id_if_some(None);
    let _ = client.read("1").replace_id(None);
    let _ = client.read("1").omit_id();
    let _ = client.read("1").reset_id();
    // Initialized required endpoint input.
    let _ = client.read("1").set_limit(5).reset_limit();
    let _ = client.read("1").omit_limit();
    let _ = client.read("1").replace_limit(Some(5));
    // Uninitialized optional endpoint input.
    let _ = client.read("1").reset_verbose();
    // Required body: positional only.
    let _ = client.write("x".to_owned()).set_payload("y".to_owned());
    let _ = client.write("x".to_owned()).omit_payload();
    // Optional body: exact payload, no conversion, no default to reset to.
    let _ = client.put().set_payload("y".to_owned()).omit_payload();
    let _ = client.put().set_payload("y");
    let _ = client.put().reset_payload();
}
