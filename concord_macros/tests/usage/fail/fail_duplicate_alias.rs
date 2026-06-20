use concord_macros::api;

api! {
    client UsageDuplicateAliasApi { base "https://example.com" }

    scope users {
        path ["users"]

        GET GetById(id: u64)
            as get
            path [id]
            -> Json<String>

        GET GetByName(name: String)
            as get
            path ["by-name", name]
            -> Json<String>
    }
}

fn main() {}
