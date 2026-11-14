concord::api! {
    client Workshop { part fragment() { read First; read Second; } GET fetch(..fragment); }
}
fn main() {}
