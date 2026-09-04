use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_dimension_limit.1"; cost 1;
            key [
                client.k00, client.k01, client.k02, client.k03,
                client.k04, client.k05, client.k06, client.k07,
                client.k08, client.k09, client.k10, client.k11,
                client.k12, client.k13, client.k14, client.k15,
                client.k16, client.k17, client.k18, client.k19,
                client.k20, client.k21, client.k22, client.k23,
                client.k24, client.k25, client.k26, client.k27,
                client.k28, client.k29, client.k30, client.k31,
                client.first_excess,
            ];
            allow 1 per 1s;
        }
    }
}

fn main() {}
