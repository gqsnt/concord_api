// Suggestions use the registry or policy formals required by each reference.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client PolicyTypoApi {
        base "https://example.test";
        rate_limits {
            policy reads() { id "policy.typo"; key [origin]; allow 1 per 1s; }
        }
        rate_limit reasd();
        GET read() -> Bytes via Binary;
    }
}

api! {
    client PolicyWindowsTypoApi {
        base "https://example.test";
        rate_limits {
            windows standard { allow 1 per 1s; }
            policy reads() { id "windows.policy.typo"; key [origin]; using standrad; }
        }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client InlineWindowsTypoApi {
        base "https://example.test";
        rate_limits { windows standard { allow 1 per 1s; } }
        rate_limit "windows.inline.typo" key [origin] using standrad;
        GET read() -> Bytes via Binary;
    }
}

api! {
    client BindingTypoApi {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy reads(account) {
                id "binding.typo";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit reads(accoutn = tenant);
        GET read() -> Bytes via Binary;
    }
}

api! {
    client KeyFormalTypoApi {
        base "https://example.test";
        rate_limits {
            policy reads(account) {
                id "key.formal.typo";
                key ["account" = accoutn];
                allow 1 per 1s;
            }
        }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client PolicyUnrelatedApi {
        base "https://example.test";
        rate_limits {
            policy reads() { id "policy.unrelated"; key [origin]; allow 1 per 1s; }
        }
        rate_limit unrelated();
        GET read() -> Bytes via Binary;
    }
}

api! {
    client WindowsRoleApi {
        base "https://example.test";
        rate_limits {
            windows standard { allow 1 per 1s; }
            policy reads() { id "windows.role"; key [origin]; allow 1 per 1s; }
        }
        rate_limit "windows.role.inline" key [origin] using reasd;
        GET read() -> Bytes via Binary;
    }
}

api! {
    client PolicyRoleApi {
        base "https://example.test";
        rate_limits { windows reads { allow 1 per 1s; } }
        rate_limit reasd();
        GET read() -> Bytes via Binary;
    }
}

api! {
    client BindingRoleApi {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy reads(account) {
                id "binding.role";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit reads(tenatn = tenant);
        GET read() -> Bytes via Binary;
    }
}

api! {
    client KeyFormalRoleApi {
        base "https://example.test";
        rate_limits {
            policy other(tenant) {
                id "key.formal.other";
                key ["tenant" = tenant];
                allow 1 per 1s;
            }
            policy reads(account) {
                id "key.formal.role";
                key ["tenant" = tenatn];
                allow 1 per 1s;
            }
        }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client WindowsUnrelatedApi {
        base "https://example.test";
        rate_limits { windows standard { allow 1 per 1s; } }
        rate_limit "windows.unrelated" key [origin] using unrelated;
        GET read() -> Bytes via Binary;
    }
}

api! {
    client RawPolicyTypoApi {
        base "https://example.test";
        rate_limits {
            policy r#type() { id "policy.raw"; key [origin]; allow 1 per 1s; }
        }
        rate_limit tpye();
        GET read() -> Bytes via Binary;
    }
}

api! {
    client RawWindowsTypoApi {
        base "https://example.test";
        rate_limits { windows r#type { allow 1 per 1s; } }
        rate_limit "windows.raw" key [origin] using tpye;
        GET read() -> Bytes via Binary;
    }
}

api! {
    client RawBindingTypoApi {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy reads(r#type) {
                id "binding.raw";
                key ["type" = r#type];
                allow 1 per 1s;
            }
        }
        rate_limit reads(tpye = tenant);
        GET read() -> Bytes via Binary;
    }
}

api! {
    client RawKeyFormalTypoApi {
        base "https://example.test";
        rate_limits {
            policy reads(r#type) {
                id "key.formal.raw";
                key ["type" = tpye];
                allow 1 per 1s;
            }
        }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client BoundFormalRoleApi {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy reads(account) {
                id "binding.already.bound";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit reads(acount = tenant, account = tenant);
        GET read() -> Bytes via Binary;
    }
}

fn main() {}
