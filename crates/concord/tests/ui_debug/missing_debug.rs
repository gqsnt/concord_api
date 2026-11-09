// An unmarked field is formatted, so its type needs `Debug`: the error names the field's type.

struct Opaque;

#[derive(concord::Debug)]
struct Record {
    id: u32,
    payload: Opaque,
}

#[derive(concord::Debug)]
struct Generic<T> {
    value: T,
}

fn main() {
    let _ = format!("{:?}", Generic { value: Opaque });
}
