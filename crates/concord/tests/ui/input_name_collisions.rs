// G1 (experiment): an endpoint's input record `{Endpoint}Input` and its `{endpoint}_with` method
// enter the generated-name checks.
concord::api! {
    pub client Shop {
        base "http://x.test";
        // `ShopList` + `ShopListInput`; the scope `list_input` generates `ShopListInput` too.
        GET list() { path "a"; }
        scope list_input { GET ping() { path "p"; } }
        // `item` generates `item_with` on `Shop`.
        GET item() { path "b"; }
        GET item_with() { path "c"; }
    }
}
fn main() {}
