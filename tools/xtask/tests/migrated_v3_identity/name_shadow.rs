//! v2.1 input (focused review of R3 B01, D1): an application extension trait
//! on the generated request provides `set_x_if_some`, which v3 generates as
//! an inherent method of the same type. Both calls below clear `x` in v2.1.
use concord::{api, Empty};
api! {
    client XOnly {
        base "https://review.example.test/";
        timeout 5s;
        GET item(query x?: u8) -> () via Empty {
            path "item";
        }
    }
}

pub trait ClearOnNone {
    fn set_x_if_some(self, v: Option<u8>) -> Self;
}

impl ClearOnNone for x_only::ItemRequest {
    // Application semantics: `None` clears (outer absence), `Some` sets.
    fn set_x_if_some(self, v: Option<u8>) -> Self {
        self.replace_x(v)
    }
}

pub fn client() -> XOnly {
    XOnly::builder().build().expect("fixture client")
}

/// A method call: reported by the tool (v3 would call the generated method).
pub fn method_call(client: &XOnly) -> String {
    let plan = client.item().replace_x(Some(3)).set_x_if_some(None).plan().expect("plan");
    plan.observed_url().to_owned()
}

/// A trait-qualified call: names the application's method in both versions.
pub fn trait_call(client: &XOnly) -> String {
    let request = ClearOnNone::set_x_if_some(client.item().replace_x(Some(3)), None);
    let plan = request.plan().expect("plan");
    plan.observed_url().to_owned()
}
