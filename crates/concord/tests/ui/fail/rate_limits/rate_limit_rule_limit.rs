// §20.1: at most 64 effective rate policies per plan; the 65th, attached
// below 63 nested scopes, is rejected.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client RateRuleLimitApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_rule_limit.1"; key [origin]; allow 1 per 1s; }

        scope s01() {
            rate_limit { id "concord.rate_limit_rule_limit.2"; key [origin]; allow 1 per 1s; }
        scope s02() {
            rate_limit { id "concord.rate_limit_rule_limit.3"; key [origin]; allow 1 per 1s; }
        scope s03() {
            rate_limit { id "concord.rate_limit_rule_limit.4"; key [origin]; allow 1 per 1s; }
        scope s04() {
            rate_limit { id "concord.rate_limit_rule_limit.5"; key [origin]; allow 1 per 1s; }
        scope s05() {
            rate_limit { id "concord.rate_limit_rule_limit.6"; key [origin]; allow 1 per 1s; }
        scope s06() {
            rate_limit { id "concord.rate_limit_rule_limit.7"; key [origin]; allow 1 per 1s; }
        scope s07() {
            rate_limit { id "concord.rate_limit_rule_limit.8"; key [origin]; allow 1 per 1s; }
        scope s08() {
            rate_limit { id "concord.rate_limit_rule_limit.9"; key [origin]; allow 1 per 1s; }
        scope s09() {
            rate_limit { id "concord.rate_limit_rule_limit.10"; key [origin]; allow 1 per 1s; }
        scope s10() {
            rate_limit { id "concord.rate_limit_rule_limit.11"; key [origin]; allow 1 per 1s; }
        scope s11() {
            rate_limit { id "concord.rate_limit_rule_limit.12"; key [origin]; allow 1 per 1s; }
        scope s12() {
            rate_limit { id "concord.rate_limit_rule_limit.13"; key [origin]; allow 1 per 1s; }
        scope s13() {
            rate_limit { id "concord.rate_limit_rule_limit.14"; key [origin]; allow 1 per 1s; }
        scope s14() {
            rate_limit { id "concord.rate_limit_rule_limit.15"; key [origin]; allow 1 per 1s; }
        scope s15() {
            rate_limit { id "concord.rate_limit_rule_limit.16"; key [origin]; allow 1 per 1s; }
        scope s16() {
            rate_limit { id "concord.rate_limit_rule_limit.17"; key [origin]; allow 1 per 1s; }
        scope s17() {
            rate_limit { id "concord.rate_limit_rule_limit.18"; key [origin]; allow 1 per 1s; }
        scope s18() {
            rate_limit { id "concord.rate_limit_rule_limit.19"; key [origin]; allow 1 per 1s; }
        scope s19() {
            rate_limit { id "concord.rate_limit_rule_limit.20"; key [origin]; allow 1 per 1s; }
        scope s20() {
            rate_limit { id "concord.rate_limit_rule_limit.21"; key [origin]; allow 1 per 1s; }
        scope s21() {
            rate_limit { id "concord.rate_limit_rule_limit.22"; key [origin]; allow 1 per 1s; }
        scope s22() {
            rate_limit { id "concord.rate_limit_rule_limit.23"; key [origin]; allow 1 per 1s; }
        scope s23() {
            rate_limit { id "concord.rate_limit_rule_limit.24"; key [origin]; allow 1 per 1s; }
        scope s24() {
            rate_limit { id "concord.rate_limit_rule_limit.25"; key [origin]; allow 1 per 1s; }
        scope s25() {
            rate_limit { id "concord.rate_limit_rule_limit.26"; key [origin]; allow 1 per 1s; }
        scope s26() {
            rate_limit { id "concord.rate_limit_rule_limit.27"; key [origin]; allow 1 per 1s; }
        scope s27() {
            rate_limit { id "concord.rate_limit_rule_limit.28"; key [origin]; allow 1 per 1s; }
        scope s28() {
            rate_limit { id "concord.rate_limit_rule_limit.29"; key [origin]; allow 1 per 1s; }
        scope s29() {
            rate_limit { id "concord.rate_limit_rule_limit.30"; key [origin]; allow 1 per 1s; }
        scope s30() {
            rate_limit { id "concord.rate_limit_rule_limit.31"; key [origin]; allow 1 per 1s; }
        scope s31() {
            rate_limit { id "concord.rate_limit_rule_limit.32"; key [origin]; allow 1 per 1s; }
        scope s32() {
            rate_limit { id "concord.rate_limit_rule_limit.33"; key [origin]; allow 1 per 1s; }
        scope s33() {
            rate_limit { id "concord.rate_limit_rule_limit.34"; key [origin]; allow 1 per 1s; }
        scope s34() {
            rate_limit { id "concord.rate_limit_rule_limit.35"; key [origin]; allow 1 per 1s; }
        scope s35() {
            rate_limit { id "concord.rate_limit_rule_limit.36"; key [origin]; allow 1 per 1s; }
        scope s36() {
            rate_limit { id "concord.rate_limit_rule_limit.37"; key [origin]; allow 1 per 1s; }
        scope s37() {
            rate_limit { id "concord.rate_limit_rule_limit.38"; key [origin]; allow 1 per 1s; }
        scope s38() {
            rate_limit { id "concord.rate_limit_rule_limit.39"; key [origin]; allow 1 per 1s; }
        scope s39() {
            rate_limit { id "concord.rate_limit_rule_limit.40"; key [origin]; allow 1 per 1s; }
        scope s40() {
            rate_limit { id "concord.rate_limit_rule_limit.41"; key [origin]; allow 1 per 1s; }
        scope s41() {
            rate_limit { id "concord.rate_limit_rule_limit.42"; key [origin]; allow 1 per 1s; }
        scope s42() {
            rate_limit { id "concord.rate_limit_rule_limit.43"; key [origin]; allow 1 per 1s; }
        scope s43() {
            rate_limit { id "concord.rate_limit_rule_limit.44"; key [origin]; allow 1 per 1s; }
        scope s44() {
            rate_limit { id "concord.rate_limit_rule_limit.45"; key [origin]; allow 1 per 1s; }
        scope s45() {
            rate_limit { id "concord.rate_limit_rule_limit.46"; key [origin]; allow 1 per 1s; }
        scope s46() {
            rate_limit { id "concord.rate_limit_rule_limit.47"; key [origin]; allow 1 per 1s; }
        scope s47() {
            rate_limit { id "concord.rate_limit_rule_limit.48"; key [origin]; allow 1 per 1s; }
        scope s48() {
            rate_limit { id "concord.rate_limit_rule_limit.49"; key [origin]; allow 1 per 1s; }
        scope s49() {
            rate_limit { id "concord.rate_limit_rule_limit.50"; key [origin]; allow 1 per 1s; }
        scope s50() {
            rate_limit { id "concord.rate_limit_rule_limit.51"; key [origin]; allow 1 per 1s; }
        scope s51() {
            rate_limit { id "concord.rate_limit_rule_limit.52"; key [origin]; allow 1 per 1s; }
        scope s52() {
            rate_limit { id "concord.rate_limit_rule_limit.53"; key [origin]; allow 1 per 1s; }
        scope s53() {
            rate_limit { id "concord.rate_limit_rule_limit.54"; key [origin]; allow 1 per 1s; }
        scope s54() {
            rate_limit { id "concord.rate_limit_rule_limit.55"; key [origin]; allow 1 per 1s; }
        scope s55() {
            rate_limit { id "concord.rate_limit_rule_limit.56"; key [origin]; allow 1 per 1s; }
        scope s56() {
            rate_limit { id "concord.rate_limit_rule_limit.57"; key [origin]; allow 1 per 1s; }
        scope s57() {
            rate_limit { id "concord.rate_limit_rule_limit.58"; key [origin]; allow 1 per 1s; }
        scope s58() {
            rate_limit { id "concord.rate_limit_rule_limit.59"; key [origin]; allow 1 per 1s; }
        scope s59() {
            rate_limit { id "concord.rate_limit_rule_limit.60"; key [origin]; allow 1 per 1s; }
        scope s60() {
            rate_limit { id "concord.rate_limit_rule_limit.61"; key [origin]; allow 1 per 1s; }
        scope s61() {
            rate_limit { id "concord.rate_limit_rule_limit.62"; key [origin]; allow 1 per 1s; }
        scope s62() {
            rate_limit { id "concord.rate_limit_rule_limit.63"; key [origin]; allow 1 per 1s; }
        scope s63() {
            rate_limit { id "concord.rate_limit_rule_limit.64"; key [origin]; allow 1 per 1s; }
            GET read() -> Bytes via Binary {
                rate_limit { id "concord.rate_limit_rule_limit.65"; key [origin]; allow 1 per 1s; }
            }
        } // s63
        } // s62
        } // s61
        } // s60
        } // s59
        } // s58
        } // s57
        } // s56
        } // s55
        } // s54
        } // s53
        } // s52
        } // s51
        } // s50
        } // s49
        } // s48
        } // s47
        } // s46
        } // s45
        } // s44
        } // s43
        } // s42
        } // s41
        } // s40
        } // s39
        } // s38
        } // s37
        } // s36
        } // s35
        } // s34
        } // s33
        } // s32
        } // s31
        } // s30
        } // s29
        } // s28
        } // s27
        } // s26
        } // s25
        } // s24
        } // s23
        } // s22
        } // s21
        } // s20
        } // s19
        } // s18
        } // s17
        } // s16
        } // s15
        } // s14
        } // s13
        } // s12
        } // s11
        } // s10
        } // s09
        } // s08
        } // s07
        } // s06
        } // s05
        } // s04
        } // s03
        } // s02
        } // s01
    }
}

fn main() {}
