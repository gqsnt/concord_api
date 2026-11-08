// What a `controls` block cannot say.
concord::api! {
    pub client Api(key: String, token?: String, region?: String, trace?: String = "on") {
        base "http://x.test";
        controls {
            remove new;
            pub(crate) with_base_url;
            remove with_base_url;
            pub(crate) set_token;
            remove set_region;
            remove maybe_region;
            remove set_trace;
            remove maybe_trace;
        }
        headers { "x-key" = key; "x-token" = token; "x-region" = region; "x-trace" = trace; }
        GET ping(query q?: String) {
            controls { remove set_q; }
        }
    }
}
fn main() {}
