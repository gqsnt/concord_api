concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { override paginate pager; } }
}
fn main() {}
