//! Direct Debug coverage for cursor carriers that otherwise sit behind traversal wrappers.
use concord_check_canary::Canaries;
use concord_example_stripe_sdk::pages::CursorList;
use concord_example_stripe_sdk::{CustomerId, SecretKey, StripeContext};

#[test]
fn cursor_and_list_template_withhold_their_stored_values() {
    let mut c = Canaries::new("stripe-cursor");
    let secret = c.issue();
    let key = SecretKey::new(format!("sk_test_{secret}")).unwrap();
    let context = StripeContext::new(concord::reqwest::Client::new(), &key);
    let list =
        CursorList::forward(context.api().customers().list().set_email(secret.clone())).unwrap();
    let prepared = list.request(None).unwrap().prepare().unwrap();
    assert!(prepared.request().url().as_str().contains(&secret));
    c.assert_debug_absent("CursorList", &list);
    let id = CustomerId::new(format!("cus_{secret}")).unwrap();
    let cursor = list.continue_from(id.clone());
    assert_eq!(cursor.id(), &id);
    c.assert_debug_absent("Cursor", &cursor);
}
