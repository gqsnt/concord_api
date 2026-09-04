use concord::prelude::*;

api! {
    client RateRuleLimitApi {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_rule_limit.1"; key [origin]; cost 1;  allow 1 per 1s; }

    scope s01() { // 01
    rate_limit { id "concord.rate_limit_rule_limit.2"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s02() { // 02
    rate_limit { id "concord.rate_limit_rule_limit.3"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s03() { // 03
    rate_limit { id "concord.rate_limit_rule_limit.4"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s04() { // 04
    rate_limit { id "concord.rate_limit_rule_limit.5"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s05() { // 05
    rate_limit { id "concord.rate_limit_rule_limit.6"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s06() { // 06
    rate_limit { id "concord.rate_limit_rule_limit.7"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s07() { // 07
    rate_limit { id "concord.rate_limit_rule_limit.8"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s08() { // 08
    rate_limit { id "concord.rate_limit_rule_limit.9"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s09() { // 09
    rate_limit { id "concord.rate_limit_rule_limit.10"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s10() { // 10
    rate_limit { id "concord.rate_limit_rule_limit.11"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s11() { // 11
    rate_limit { id "concord.rate_limit_rule_limit.12"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s12() { // 12
    rate_limit { id "concord.rate_limit_rule_limit.13"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s13() { // 13
    rate_limit { id "concord.rate_limit_rule_limit.14"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s14() { // 14
    rate_limit { id "concord.rate_limit_rule_limit.15"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s15() { // 15
    rate_limit { id "concord.rate_limit_rule_limit.16"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s16() { // 16
    rate_limit { id "concord.rate_limit_rule_limit.17"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s17() { // 17
    rate_limit { id "concord.rate_limit_rule_limit.18"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s18() { // 18
    rate_limit { id "concord.rate_limit_rule_limit.19"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s19() { // 19
    rate_limit { id "concord.rate_limit_rule_limit.20"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s20() { // 20
    rate_limit { id "concord.rate_limit_rule_limit.21"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s21() { // 21
    rate_limit { id "concord.rate_limit_rule_limit.22"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s22() { // 22
    rate_limit { id "concord.rate_limit_rule_limit.23"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s23() { // 23
    rate_limit { id "concord.rate_limit_rule_limit.24"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s24() { // 24
    rate_limit { id "concord.rate_limit_rule_limit.25"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s25() { // 25
    rate_limit { id "concord.rate_limit_rule_limit.26"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s26() { // 26
    rate_limit { id "concord.rate_limit_rule_limit.27"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s27() { // 27
    rate_limit { id "concord.rate_limit_rule_limit.28"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s28() { // 28
    rate_limit { id "concord.rate_limit_rule_limit.29"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s29() { // 29
    rate_limit { id "concord.rate_limit_rule_limit.30"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s30() { // 30
    rate_limit { id "concord.rate_limit_rule_limit.31"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s31() { // 31
    rate_limit { id "concord.rate_limit_rule_limit.32"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s32() { // 32
    rate_limit { id "concord.rate_limit_rule_limit.33"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s33() { // 33
    rate_limit { id "concord.rate_limit_rule_limit.34"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s34() { // 34
    rate_limit { id "concord.rate_limit_rule_limit.35"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s35() { // 35
    rate_limit { id "concord.rate_limit_rule_limit.36"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s36() { // 36
    rate_limit { id "concord.rate_limit_rule_limit.37"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s37() { // 37
    rate_limit { id "concord.rate_limit_rule_limit.38"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s38() { // 38
    rate_limit { id "concord.rate_limit_rule_limit.39"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s39() { // 39
    rate_limit { id "concord.rate_limit_rule_limit.40"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s40() { // 40
    rate_limit { id "concord.rate_limit_rule_limit.41"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s41() { // 41
    rate_limit { id "concord.rate_limit_rule_limit.42"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s42() { // 42
    rate_limit { id "concord.rate_limit_rule_limit.43"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s43() { // 43
    rate_limit { id "concord.rate_limit_rule_limit.44"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s44() { // 44
    rate_limit { id "concord.rate_limit_rule_limit.45"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s45() { // 45
    rate_limit { id "concord.rate_limit_rule_limit.46"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s46() { // 46
    rate_limit { id "concord.rate_limit_rule_limit.47"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s47() { // 47
    rate_limit { id "concord.rate_limit_rule_limit.48"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s48() { // 48
    rate_limit { id "concord.rate_limit_rule_limit.49"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s49() { // 49
    rate_limit { id "concord.rate_limit_rule_limit.50"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s50() { // 50
    rate_limit { id "concord.rate_limit_rule_limit.51"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s51() { // 51
    rate_limit { id "concord.rate_limit_rule_limit.52"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s52() { // 52
    rate_limit { id "concord.rate_limit_rule_limit.53"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s53() { // 53
    rate_limit { id "concord.rate_limit_rule_limit.54"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s54() { // 54
    rate_limit { id "concord.rate_limit_rule_limit.55"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s55() { // 55
    rate_limit { id "concord.rate_limit_rule_limit.56"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s56() { // 56
    rate_limit { id "concord.rate_limit_rule_limit.57"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s57() { // 57
    rate_limit { id "concord.rate_limit_rule_limit.58"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s58() { // 58
    rate_limit { id "concord.rate_limit_rule_limit.59"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s59() { // 59
    rate_limit { id "concord.rate_limit_rule_limit.60"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s60() { // 60
    rate_limit { id "concord.rate_limit_rule_limit.61"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s61() { // 61
    rate_limit { id "concord.rate_limit_rule_limit.62"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s62() { // 62
    rate_limit { id "concord.rate_limit_rule_limit.63"; key [origin]; cost 1;  allow 1 per 1s; }
    scope s63() { // 63
    rate_limit { id "concord.rate_limit_rule_limit.64"; key [origin]; cost 1;  allow 1 per 1s; }
    GET read() -> Bytes via Binary {
        rate_limit { id "concord.rate_limit_rule_limit.65"; key [origin]; cost 1;  allow 1 per 1s; }
    }
    } // 63
    } // 62
    } // 61
    } // 60
    } // 59
    } // 58
    } // 57
    } // 56
    } // 55
    } // 54
    } // 53
    } // 52
    } // 51
    } // 50
    } // 49
    } // 48
    } // 47
    } // 46
    } // 45
    } // 44
    } // 43
    } // 42
    } // 41
    } // 40
    } // 39
    } // 38
    } // 37
    } // 36
    } // 35
    } // 34
    } // 33
    } // 32
    } // 31
    } // 30
    } // 29
    } // 28
    } // 27
    } // 26
    } // 25
    } // 24
    } // 23
    } // 22
    } // 21
    } // 20
    } // 19
    } // 18
    } // 17
    } // 16
    } // 15
    } // 14
    } // 13
    } // 12
    } // 11
    } // 10
    } // 09
    } // 08
    } // 07
    } // 06
    } // 05
    } // 04
    } // 03
    } // 02
    } // 01
}}

fn main() {}
