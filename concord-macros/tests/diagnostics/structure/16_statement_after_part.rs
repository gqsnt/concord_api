concord::api! {
    client Tasks {
        part page() {}
        idle_timeout own_timeout();
    }
}
fn main() {}
