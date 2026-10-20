// §12.3 / §20.1: every key has 1-32 dimensions.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        values {
            k00: String;
            k01: String;
            k02: String;
            k03: String;
            k04: String;
            k05: String;
            k06: String;
            k07: String;
            k08: String;
            k09: String;
            k10: String;
            k11: String;
            k12: String;
            k13: String;
            k14: String;
            k15: String;
            k16: String;
            k17: String;
            k18: String;
            k19: String;
            k20: String;
            k21: String;
            k22: String;
            k23: String;
            k24: String;
            k25: String;
            k26: String;
            k27: String;
            k28: String;
            k29: String;
            k30: String;
            k31: String;
            k32: String;
        }
        rate_limit {
            id "concord.rate_limit_dimension_limit.1";
            key [
                "k00" = k00,
                "k01" = k01,
                "k02" = k02,
                "k03" = k03,
                "k04" = k04,
                "k05" = k05,
                "k06" = k06,
                "k07" = k07,
                "k08" = k08,
                "k09" = k09,
                "k10" = k10,
                "k11" = k11,
                "k12" = k12,
                "k13" = k13,
                "k14" = k14,
                "k15" = k15,
                "k16" = k16,
                "k17" = k17,
                "k18" = k18,
                "k19" = k19,
                "k20" = k20,
                "k21" = k21,
                "k22" = k22,
                "k23" = k23,
                "k24" = k24,
                "k25" = k25,
                "k26" = k26,
                "k27" = k27,
                "k28" = k28,
                "k29" = k29,
                "k30" = k30,
                "k31" = k31,
                "k32" = k32,
            ];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}
