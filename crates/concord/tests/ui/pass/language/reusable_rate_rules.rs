use concord::api;

api! {
    client ReusableRateApi {
        timeout off;
        base "https://example.test";

        rate_limits {
            account_reads(account) {
                id "service.account.read";
                key [host, account];
                cost 1;
                allow 100 per 1m;
                allow 1_000 per 1h;
            }

            service_reads() {
                allow 10_000 per 1m;
            }
        }

        rate_limit service_reads();


    scope accounts(account: String) {
        rate_limit account_reads(account = accounts.account);

        GET details() -> Bytes using Bytes;
    }

    GET lookup(account: String) -> Bytes using Bytes {
        rate_limit account_reads(account = endpoint.account) { cost 2; }
    }
}}

fn main() {
    let builder = ReusableRateApi::builder();
    let _ = builder;
}
