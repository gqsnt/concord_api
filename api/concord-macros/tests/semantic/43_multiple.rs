concord::api! {
    client Workshop { GET fetch(unused: &str, id?: u32) { path "/{id}"; headers { "Host" = "v"; "bad key" = "v"; } status { 601 => () } } GET other(); }
}
fn main() {}
