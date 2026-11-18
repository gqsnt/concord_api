concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope accounts { POST update(accounts?: String) { body json { accounts; } } } }
}
fn main() {}
