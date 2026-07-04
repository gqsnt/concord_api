pub fn assert_event_order(events: &[String], expected: &[&str]) {
    let actual: Vec<&str> = events.iter().map(String::as_str).collect();
    assert_eq!(actual, expected);
}
