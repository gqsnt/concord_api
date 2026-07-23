#![deny(deprecated)]

type LegacyEndpoint = concord::endpoint::PreparedEndpoint;

fn main() {
    let _: Option<LegacyEndpoint> = None;
}
