concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; part answer() { status { 201 { body } => true } } HEAD fetch(..answer) -> bool; }
}
fn main() {}
