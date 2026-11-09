// Compile the production Datadog classification against a declaration with one new write.
// A method/effect shortcut would accept this write without choosing its HTTP-202 contract.
mod api {
    concord::api! {
        client DatadogApi {
            base "http://127.0.0.1";
            scope events {
                GET list();
                #[read] POST search();
                POST post();
                POST future_write();
            }
        }
    }
    pub(crate) use __DatadogApiOperation as DatadogApiOperation;
}
#[path = "../../../../examples/datadog-events-sdk/src/posting.rs"]
mod posting;
fn main() {}
