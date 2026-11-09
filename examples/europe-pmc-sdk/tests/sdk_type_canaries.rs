//! Direct secret-type canaries, including values used beneath sensitive request placements.

use concord_check_canary::Canaries;
use concord_sdk_europe_pmc as sdk;

#[test]
fn sdk_owned_secrets_are_withheld_in_both_debug_modes() {
    let mut c = Canaries::new("T");
    let secret = c.issue();
    let source = sdk::Source::new(&secret).unwrap();
    let id = sdk::PublicationId::new(&secret).unwrap();
    c.assert_debug_absent("Source", &source);
    c.assert_debug_absent("PublicationId", &id);
    assert_eq!(source.to_string(), secret);
    assert_eq!(id.to_string(), secret);
}
