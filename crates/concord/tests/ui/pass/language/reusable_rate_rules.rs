use concord::api;

api! {
    client ReusableRateApi {
        base "https://example.test";

        rate_limits {
            rule account_reads {
                id "service.account.read";
                by [host, account];
                cost 1;
                100 / 1m;
                1_000 / 1h;
            }

            rule service_reads {
                10_000 / 1m;
            }
        }

        rate_limit use service_reads;
    }

    scope accounts(account: String) {
        rate_limit use account_reads {
            account = scope.account;
        }

        GET details() -> Bytes;
    }

    GET lookup(account: String) -> Bytes {
        rate_limit use account_reads {
            account = ep.account;
            cost 2;
        }
    }
}

fn main() {
    let builder = ReusableRateApi::builder();
    let _ = builder;
}
