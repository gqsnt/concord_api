// §3.1: parameter types are parsed with a real Rust type parser, so a
// function-pointer type's own `->` is not mistaken for the endpoint result
// arrow, even when a placement adapter (`via`) follows it. VALUE-06 requires
// the value to be used, so it is a directly placed query value whose atom is
// supplied by an explicit AtomEncoder (foreign fn types cannot implement
// RequestAtom).
use concord::{AtomEncoder, AtomError, Binary, BoundedAtomWriter, Bytes, api};

pub struct CallbackProbe;

impl AtomEncoder<fn(u32) -> bool> for CallbackProbe {
    fn write(value: &fn(u32) -> bool, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        output.push_str(if value(1) { "accepts" } else { "rejects" })
    }
}

api! {
    client FunctionPointerApi {
        timeout 30s;
        base "http://example.test";

        GET process(query callback: fn(u32) -> bool via crate::CallbackProbe) -> Bytes via Binary;
    }
}

fn accepts_nonzero(value: u32) -> bool {
    value != 0
}

fn main() {
    let endpoint = FunctionPointerApi::unbound().process(accepts_nonzero as fn(u32) -> bool);
    let client = FunctionPointerApi::new().expect("function-pointer client");
    let _: function_pointer_api::ProcessRequest = client.request(endpoint);
    let _: function_pointer_api::ProcessRequest = client.process(accepts_nonzero as fn(u32) -> bool);
}
