// Seeded mistake (M11): `base x;` names a value whose type is not a base URL.
concord::api! {
    pub client Api(instance: String) {
        base instance;
        GET a();
    }
}

fn main() {}
