//! Concord declarations used only by the benchmarks, in addition to `examples/shapes` (S1–S3).
//!
//! Each client has a hand-written reqwest equivalent in [`crate::manual`] producing the same
//! request (checked by this crate's tests). Benchmarks call only ordinary generated methods
//! (navigation, positional arguments, `set_*`, `prepare`, `send`), so migrating these declarations
//! to a new DSL surface is mechanical.

/// Realistic request: three path values, ten query values (eight optional), a scope chain and six
/// inherited headers (one sensitive).
pub mod tracker {
    use crate::payload::Issue;

    concord::api! {
        /// An issue tracker.
        pub client Tracker(
            #[sensitive] header("authorization") token: String,
            header("x-api-version") api_version: String = "2024-06-01",
        ) {
            base "https://api.example.com/v3";
            headers { "accept" = "application/json"; "user-agent" = "concord-bench/1"; }

            scope org(org: String) {
                path "orgs/{org}";
                headers { "x-org" = org; }

                scope project(project: String) {
                    path "projects/{project}";

                    GET issues(
                        milestone: u32,
                        query state?: String,
                        query("label") each labels?: Vec<String>,
                        query assignee?: String,
                        query since?: u64,
                        query sort?: String,
                        query direction?: String,
                        query include_closed?: bool,
                        query page: u32 = 1,
                        query per_page: u32 = 50,
                    ) -> json Vec<Issue> {
                        path "milestones/{milestone}/issues";
                        headers { "x-request-kind" = "list"; }
                    }
                }
            }
        }
    }
}

/// Header-value scenarios: 1, 4 and 16 endpoint header values, formatted text and numeric.
///
/// A future native-`HeaderValue` variant adds endpoints here (`native_1`, `native_4`,
/// `native_16`) and one function in `scenarios::headers`.
pub mod headers {
    concord::api! {
        /// Endpoints differing only in their header values.
        pub client Headers {
            base "https://api.example.com";

            GET text_1(header("x-h00") h00: String) { path "h"; }
            GET text_4(
                header("x-h00") h00: String, header("x-h01") h01: String,
                header("x-h02") h02: String, header("x-h03") h03: String,
            ) { path "h"; }
            GET text_16(
                header("x-h00") h00: String, header("x-h01") h01: String,
                header("x-h02") h02: String, header("x-h03") h03: String,
                header("x-h04") h04: String, header("x-h05") h05: String,
                header("x-h06") h06: String, header("x-h07") h07: String,
                header("x-h08") h08: String, header("x-h09") h09: String,
                header("x-h10") h10: String, header("x-h11") h11: String,
                header("x-h12") h12: String, header("x-h13") h13: String,
                header("x-h14") h14: String, header("x-h15") h15: String,
            ) { path "h"; }

            GET numeric_1(header("x-h00") h00: u64) { path "h"; }
            GET numeric_4(
                header("x-h00") h00: u64, header("x-h01") h01: u64,
                header("x-h02") h02: u64, header("x-h03") h03: u64,
            ) { path "h"; }
            GET numeric_16(
                header("x-h00") h00: u64, header("x-h01") h01: u64,
                header("x-h02") h02: u64, header("x-h03") h03: u64,
                header("x-h04") h04: u64, header("x-h05") h05: u64,
                header("x-h06") h06: u64, header("x-h07") h07: u64,
                header("x-h08") h08: u64, header("x-h09") h09: u64,
                header("x-h10") h10: u64, header("x-h11") h11: u64,
                header("x-h12") h12: u64, header("x-h13") h13: u64,
                header("x-h14") h14: u64, header("x-h15") h15: u64,
            ) { path "h"; }
        }
    }
}

/// Deep composition: eight nested scopes, each adding a path value and a header.
pub mod deep {
    concord::api! {
        /// Eight nested scopes.
        pub client Deep {
            base "https://api.example.com";

            scope l1(v1: String) {
                path "l1/{v1}";
                headers { "x-l1" = v1; }
                scope l2(v2: String) {
                    path "l2/{v2}";
                    headers { "x-l2" = v2; }
                    scope l3(v3: String) {
                        path "l3/{v3}";
                        headers { "x-l3" = v3; }
                        scope l4(v4: String) {
                            path "l4/{v4}";
                            headers { "x-l4" = v4; }
                            scope l5(v5: String) {
                                path "l5/{v5}";
                                headers { "x-l5" = v5; }
                                scope l6(v6: String) {
                                    path "l6/{v6}";
                                    headers { "x-l6" = v6; }
                                    scope l7(v7: String) {
                                        path "l7/{v7}";
                                        headers { "x-l7" = v7; }
                                        scope l8(v8: String) {
                                            path "l8/{v8}";
                                            headers { "x-l8" = v8; }

                                            GET leaf() { path "leaf"; }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Inherited header operations: plain inheritance, override (value present / absent), append,
/// remove.
pub mod compose {
    concord::api! {
        /// Client headers changed by a scope and by endpoints.
        pub client Compose(
            header("x-a") a: String = "client-a",
            header("x-b") b: String = "client-b",
            header("x-c") c: String = "client-c",
        ) {
            base "https://api.example.com";
            headers { "accept" = "application/json"; }

            scope team(team: String) {
                path "teams/{team}";
                headers { append "x-b" = team; }

                GET inherit() { path "inherit"; }
                GET override_value(v?: String) {
                    path "override";
                    headers { override "x-a" = v else inherit; }
                }
                GET append_value(w: String) {
                    path "append";
                    headers { append "x-b" = w; }
                }
                GET remove_value() {
                    path "remove";
                    headers { remove "x-c"; }
                }
            }
        }
    }
}

/// Request bodies (JSON, text, bytes) and typed responses of the same formats.
pub mod bodies {
    use concord::bytes::Bytes;

    use crate::payload::Payload;

    concord::api! {
        /// Body encoding and response decoding.
        pub client Bodies {
            base "https://api.example.com/v1";

            POST post_json(json b: Payload) { path "json"; }
            POST post_text(text b: String) { path "text"; }
            POST post_bytes(bytes b: Bytes) { path "bytes"; }

            GET get_json() -> json Payload { path "json"; }
            GET get_text() -> text String { path "text"; }
            GET get_bytes() -> bytes Bytes { path "bytes"; }
        }
    }
}
