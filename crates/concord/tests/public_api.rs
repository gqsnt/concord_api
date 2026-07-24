mod public_api {
    mod namespaces;
    pub(crate) mod type_neutrality;
}

pub(crate) use public_api::type_neutrality::{Json, RenamedJson};
