concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { body { let _: Self = make(); make() }; } }
}
fn main() {}
