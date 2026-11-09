//! The five numeric priorities keep their scalar wire spelling.
use concord_sdk_ntfy::Priority;

#[test]
fn priorities_keep_the_five_wire_numbers() {
    for (priority, text) in [
        (Priority::Min, "1"),
        (Priority::Low, "2"),
        (Priority::Default, "3"),
        (Priority::High, "4"),
        (Priority::Max, "5"),
    ] {
        assert_eq!(priority.to_string(), text);
        assert_eq!(serde_json::to_string(&priority).unwrap(), text);
    }
}
