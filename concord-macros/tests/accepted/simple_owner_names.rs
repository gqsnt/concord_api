// Exercise parenthesized paths in both the callee and its argument.
#![allow(unused_parens)]

#[allow(non_upper_case_globals)]
const zone: () = ();

#[allow(non_camel_case_types)]
struct client;

fn view_width<T>(_: T) -> usize {
    std::mem::size_of::<T>()
}

fn pick<T>(owner: T) -> concord::Json {
    assert_eq!(view_width(owner), std::mem::size_of::<&u8>());
    concord::Json
}

concord::api! {
    client Service(key: u8) {
        base "https://example.invalid";
        query { key; }

        scope zone(id: u16) {
            path "{id}";
            POST scoped() -> usize {
                body concord::json(view_width(r#zone));
                read pick(zone);
                status { 201 => view_width(zone) }
            }
            POST root() -> usize {
                body concord::json((view_width)((((client)))));
                read pick(r#client);
                status { 201 => view_width(client) }
            }
            POST plain(value: usize) -> usize {
                body concord::json(value);
                read concord::Json;
                status { 201 => 42 }
            }
        }
    }
}

fn assert_body(request: concord::reqwest::Request, expected: usize) {
    assert_eq!(
        request.body().unwrap().as_bytes().unwrap(),
        expected.to_string().as_bytes()
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let calls = Service::new(5u8).zone(3u16);
    let width = std::mem::size_of::<&u8>();
    assert_body(calls.scoped().request().unwrap(), width);
    assert_body(calls.root().request().unwrap(), width);
    assert_body(calls.plain(13usize).request().unwrap(), 13);

    assert_eq!(
        calls
            .scoped()
            .read_response(concord::reply(200, "22"))
            .await
            .unwrap(),
        22
    );
    assert_eq!(
        calls
            .root()
            .read_response(concord::reply(200, "23"))
            .await
            .unwrap(),
        23
    );
    assert_eq!(
        calls
            .plain(13usize)
            .read_response(concord::reply(200, "24"))
            .await
            .unwrap(),
        24
    );

    assert_eq!(
        calls
            .scoped()
            .read_response(concord::reply(201, ""))
            .await
            .unwrap(),
        width
    );
    assert_eq!(
        calls
            .root()
            .read_response(concord::reply(201, ""))
            .await
            .unwrap(),
        width
    );
    assert_eq!(
        calls
            .plain(13usize)
            .read_response(concord::reply(201, ""))
            .await
            .unwrap(),
        42
    );
}
