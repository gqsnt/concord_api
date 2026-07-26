use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            by [
                vars.k00, vars.k01, vars.k02, vars.k03,
                vars.k04, vars.k05, vars.k06, vars.k07,
                vars.k08, vars.k09, vars.k10, vars.k11,
                vars.k12, vars.k13, vars.k14, vars.k15,
                vars.k16, vars.k17, vars.k18, vars.k19,
                vars.k20, vars.k21, vars.k22, vars.k23,
                vars.k24, vars.k25, vars.k26, vars.k27,
                vars.k28, vars.k29, vars.k30, vars.k31,
                vars.first_excess,
            ];
            1 / 1s;
        }
    }
}

fn main() {}
