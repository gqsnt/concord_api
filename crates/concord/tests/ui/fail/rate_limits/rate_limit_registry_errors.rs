// §12.1: window sets and policies are validated as a closed registry.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client UnknownWindowSetApi {
        base "https://example.test";
        rate_limit "registry.unknown" key [origin] using missing;
    }
}

api! {
    client RepeatedDurationApi {
        base "https://example.test";
        rate_limits {
            windows contradictory { allow 10 per 1s; allow 20 per 1s; }
        }
        rate_limit "registry.repeated" key [origin] using contradictory;
    }
}

api! {
    client MixedWindowSourceApi {
        base "https://example.test";
        rate_limits {
            windows standard { allow 10 per 1s; }
            policy mixed() {
                id "registry.mixed";
                key [origin];
                using standard;
                allow 5 per 1m;
            }
        }
    }
}

api! {
    client DuplicateWindowSetApi {
        base "https://example.test";
        rate_limits {
            windows standard { allow 10 per 1s; }
            windows standard { allow 20 per 1s; }
        }
    }
}

api! {
    client DuplicatePolicyNameApi {
        base "https://example.test";
        rate_limits {
            policy reads() { id "registry.reads.a"; key [origin]; allow 1 per 1s; }
            policy reads() { id "registry.reads.b"; key [origin]; allow 1 per 1s; }
        }
    }
}

api! {
    client DuplicatePolicyIdApi {
        base "https://example.test";
        rate_limits {
            policy first() { id "registry.shared"; key [origin]; allow 1 per 1s; }
            policy second() { id "registry.shared"; key [origin]; allow 1 per 1s; }
        }
    }
}

api! {
    client ZeroCapacityApi {
        base "https://example.test";
        rate_limit { id "registry.zero"; key [origin]; allow 0 per 1s; }
    }
}

api! {
    client LegacyProfileApi {
        base "https://example.test";
        rate_limits {
            reads(account) { key [origin]; allow 1 per 1s; }
        }
    }
}

fn main() {}
