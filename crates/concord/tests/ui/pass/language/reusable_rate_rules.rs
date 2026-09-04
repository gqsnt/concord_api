use concord::api;

api! {
    client ReusableRateApi {
        timeout 30s;
        base "https://example.test";

        rate_limits {
            policy account_reads(account) {
                id "service.account.read";
                key [origin, account];
                cost 1;
                allow 100 per 1m;
                allow 1_000 per 1h;
            }

            policy service_reads() { id "concord.reusable_rate_rules.2"; key [origin]; cost 1;
                allow 10_000 per 1m;
            }
        }

        rate_limit service_reads();


    scope accounts(account: String) {
        rate_limit account_reads(account = accounts.account);

        GET details() -> Bytes via Binary;
    }

    GET lookup(account: String) -> Bytes via Binary {
        rate_limit account_reads(account = endpoint.account) { cost 2; }
    }
}}

fn main() {
    let builder = ReusableRateApi::builder();
    let _ = builder;
}
