use concord_macros::api;

pub struct WebSocket<Out, In>(std::marker::PhantomData<(Out, In)>);

api! {
    client WebSocketRemovedApi {
        base "https://example.com"
    }

    GET Connect
        path ["connect"]
        -> WebSocket<String, String>
}

fn main() {}
