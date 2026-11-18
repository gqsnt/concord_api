concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; scope outer { GET get(outer: String, outer_value: String, outer_input: String, outer_param: String) { query { outer; outer_value; outer_input; outer_param; } } } }
}
fn main() {}
