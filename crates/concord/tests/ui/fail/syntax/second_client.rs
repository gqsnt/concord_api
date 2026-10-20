// §3.2: `api!` contains exactly one client; a second client is neither a
// child nor a clause.
#![allow(unused_imports)]

use concord::api;

api! {
    client FirstApi {
        base "https://one.example.test";

        client SecondApi {
            base "https://two.example.test";
        }
    }
}

api! {
    client ThirdApi {
        base "https://three.example.test";
    }
    client FourthApi {
        base "https://four.example.test";
    }
}

fn main() {}
