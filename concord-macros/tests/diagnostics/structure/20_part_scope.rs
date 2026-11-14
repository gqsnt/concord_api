concord::api! {
    client Tasks {
        scope jobs {
            part own(limit?: u32) { query { limit; } }
        }
    }
}
fn main() {}
