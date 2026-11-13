concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { remove paginate pager; } }
}
fn main() {}
