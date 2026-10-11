//! Fourth post-fix review of F03 (round 5, V3-O04): migrate-v3 input.
//!
//! Every call marked `unrelated` returns 1 in v2.1, and its unrelated target
//! also offers `replace_x` returning 2, so a wrong rename would still compile
//! and silently change the result.
//! `tools/xtask/src/migration/tests/round5.rs` migrates this file as the module
//! `round5` of a crate (with `round5_files/shared.rs` beside it, mounted twice)
//! and asserts that the result is byte-for-byte
//! `tools/xtask/tests/migrated_v3_identity/round5.rs`, which
//! `tools/xtask/tests/migrated_v3.rs` compiles through the real v3
//! `concord::api!` and executes.

use concord::endpoint::RequestPlan;
use concord::{ApiError, AtomEncoder, AtomError, BoundedAtomWriter, Empty, api};

api! {
    client Api {
        base "https://review.example.test/";
        timeout 5s;
        GET item(query x?: u32) -> () via Empty {
            path "item";
        }
        GET nested(query x?: Option<u32> via crate::round5::NestedAtom) -> () via Empty {
            path "nested";
        }
    }
}

/// Encodes the nested optional `x` of `nested`: an inner `None` is `none`.
pub struct NestedAtom;

impl AtomEncoder<Option<u32>> for NestedAtom {
    fn write(value: &Option<u32>, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
        match value {
            Some(number) => output.write(number),
            None => output.push_str("none"),
        }
    }
}

/// Unrelated items whose names collide with the generated ones.
pub mod other {
    /// The generated request's leaf name, with both setter names.
    #[derive(Clone, Copy)]
    pub struct ItemRequest;

    impl ItemRequest {
        /// The original call's result.
        pub fn set_x(self, _: Option<u32>) -> u32 {
            1
        }

        /// What a wrong rename would return instead.
        pub fn replace_x(self, _: Option<u32>) -> u32 {
            2
        }
    }
}

/// Constants of the unrelated type, imported into inner blocks below.
#[allow(non_upper_case_globals)]
pub mod consts {
    pub const r: super::other::ItemRequest = super::other::ItemRequest;
    pub const REQ: super::other::ItemRequest = super::other::ItemRequest;
}

// ----- (N1) an item of an inner block shadows an outer local binding -----------

#[allow(non_upper_case_globals)]
pub fn inner_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        const r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // unrelated: inner const
    }
}

#[allow(non_upper_case_globals)]
pub fn inner_static(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        static r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // unrelated: inner static
    }
}

#[allow(non_camel_case_types)]
pub fn inner_unit_struct(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        #[derive(Clone, Copy)]
        struct r;
        impl r {
            fn set_x(self, _: Option<u32>) -> u32 {
                1
            }
            #[allow(dead_code)]
            fn replace_x(self, _: Option<u32>) -> u32 {
                2
            }
        }
        r.set_x(None) // unrelated: inner unit struct
    }
}

pub fn inner_use_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        use self::consts::r;
        r.set_x(None) // unrelated: inner use of a const
    }
}

pub fn inner_glob_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    {
        #[allow(unused_imports)]
        use self::consts::*;
        r.set_x(None) // unrelated: inner glob of a const
    }
}

#[allow(non_upper_case_globals)]
pub fn closure_body_const(client: &Api) -> u32 {
    let r = client.item();
    let _ = &r;
    let f = || {
        const r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // unrelated: closure body const
    };
    f()
}

#[allow(non_snake_case)]
pub fn upper_case(client: &Api) -> u32 {
    let REQ = client.item();
    let _ = &REQ;
    {
        use self::consts::REQ;
        REQ.set_x(None) // unrelated: upper case local and const
    }
}

#[allow(non_upper_case_globals)]
pub fn parameter(r: api::ItemRequest) -> u32 {
    let _ = &r;
    {
        const r: other::ItemRequest = other::ItemRequest;
        r.set_x(None) // unrelated: const shadows a parameter
    }
}

// ----- (N2) one module file mounted twice ---------------------------------------

/// `round5_files/shared.rs` with `super::Req` a generated request (whose
/// `set_x(None)` also compiles in v3, where it sets a present inner `None`).
#[path = "round5_files"]
pub mod first {
    pub type Req = super::api::NestedRequest;
    pub type Out = super::api::NestedRequest;
    pub mod shared;
}

/// The same file with `super::Req` the unrelated type.
#[path = "round5_files"]
pub mod second {
    pub type Req = super::other::ItemRequest;
    pub type Out = u32;
    pub mod shared;
}

// ----- genuine generated calls ------------------------------------------------

/// Genuine generated calls in v2.1 exact outer-slot replacement: a binding no
/// block item shadows, a binding beside an inner block's differently named
/// item, a binding whose name an item of a finished sibling block takes, and a
/// binding that shadows a function item of its own block.
#[allow(non_upper_case_globals)]
pub fn genuine_plans(client: &Api) -> Vec<Result<RequestPlan, ApiError>> {
    let plain = client.item();
    let plain = plain.replace_x(Some(31)); // genuine: plain binding
    let beside = client.item();
    let beside = {
        const Q: u32 = 32;
        beside.replace_x(Some(Q)) // genuine: inner block item of another name
    };
    let sibling = client.item();
    {
        const sibling: u32 = 0;
        let _ = sibling;
    }
    let sibling = sibling.replace_x(Some(33)); // genuine: sibling block item
    let own = client.item();
    #[allow(dead_code)]
    fn own() {}
    let own = own.replace_x(Some(34)); // genuine: binding shadows its block's fn
    vec![plain.plan(), beside.plan(), sibling.plan(), own.plan()]
}
