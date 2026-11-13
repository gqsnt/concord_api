GET get_updates(offset?: i64, timeout?: u32) -> Vec<Update> {
    path "getUpdates";
    query { offset; timeout; }
    idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
}
