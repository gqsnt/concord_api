// Rate registry spellings stay within the referenced namespace and policy.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client PolicySpelling {
        base "https://example.test";
        rate_limits {
            windows AccountReads { allow 1 per 1s; }
            policy account_reads() {
                id "suggest.policy";
                key [origin];
                allow 1 per 1s;
            }
        }
        rate_limit AccountReads();
    }
}

api! {
    client WindowSpelling {
        base "https://example.test";
        rate_limits {
            windows standard_window { allow 1 per 1s; }
            policy reads() {
                id "suggest.window.policy";
                key [origin];
                using StandardWindow;
            }
        }
    }
}

api! {
    client AttachedWindowSpelling {
        base "https://example.test";
        rate_limits {
            windows standard { allow 1 per 1s; }
        }
        rate_limit "suggest.window.inline" key [origin] using standrad;
    }
}

api! {
    client WindowNamespace {
        base "https://example.test";
        rate_limits {
            windows standard { allow 1 per 1s; }
            policy reads() {
                id "suggest.window.namespace";
                key [origin];
                allow 1 per 1s;
            }
        }
        rate_limit "suggest.window.wrongkind" key [origin] using readz;
    }
}

api! {
    client FormalBindingSpelling {
        base "https://example.test";
        values { query account: String; }
        rate_limits {
            policy reads(account_id) {
                id "suggest.formal.binding";
                key ["account" = account_id];
                allow 1 per 1s;
            }
        }
        rate_limit reads(AccountId = account);
    }
}

api! {
    client BoundFormalCandidates {
        base "https://example.test";
        values { query account: String; }
        rate_limits {
            policy reads(account_id, account_ix) {
                id "suggest.formal.unbound";
                key ["account" = account_id, "index" = account_ix];
                allow 1 per 1s;
            }
        }
        rate_limit reads(account_i = account, account_id = account);
    }
}

api! {
    client PolicyKeyFormalSpelling {
        base "https://example.test";
        rate_limits {
            policy reads(account_id) {
                id "suggest.formal.key";
                key ["account" = account_di];
                allow 1 per 1s;
            }
        }
    }
}

api! {
    client PolicyFormalNamespace {
        base "https://example.test";
        values { query account_id: String; }
        rate_limits {
            policy reads(tenant) {
                id "suggest.formal.namespace";
                key ["account" = account_id];
                allow 1 per 1s;
            }
        }
    }
}

// `off` is a declared policy here, and needs escaping in an attachment.
api! {
    client ReservedPolicySpelling {
        base "https://example.test";
        rate_limits {
            policy off() {
                id "suggest.policy.reserved";
                key [origin];
                allow 1 per 1s;
            }
        }
        rate_limit of();
    }
}

fn main() {}
