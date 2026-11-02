//! `concord` depended on as `concord4` (ADV-4): expansions refer to the crate through `$crate`.

concord4::api! {
    /// A client declared through the renamed crate.
    pub client Renamed(query("v") version: u32 = 4) {
        base "http://renamed.invalid";
        GET ping() -> String via concord4::Text { path "ping"; }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_a_request() {
        let p = super::Renamed::new(concord4::reqwest::Client::new())
            .ping()
            .prepare()
            .unwrap();
        assert_eq!(
            p.request().url().as_str(),
            "http://renamed.invalid/ping?v=4"
        );
    }
}
