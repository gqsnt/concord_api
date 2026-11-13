concord::api! {
    client Tasks {
        scope jobs {
            problem TaskFault read FaultReader;
        }
    }
}
fn main() {}
