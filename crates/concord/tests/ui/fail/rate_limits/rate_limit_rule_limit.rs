use concord::prelude::*;

api! {
    client RateRuleLimitApi {
        timeout off;
        base "https://example.test";
        rate_limit { allow 1 per 1s; }

    scope s01() { // 01
    rate_limit { allow 1 per 1s; }
    scope s02() { // 02
    rate_limit { allow 1 per 1s; }
    scope s03() { // 03
    rate_limit { allow 1 per 1s; }
    scope s04() { // 04
    rate_limit { allow 1 per 1s; }
    scope s05() { // 05
    rate_limit { allow 1 per 1s; }
    scope s06() { // 06
    rate_limit { allow 1 per 1s; }
    scope s07() { // 07
    rate_limit { allow 1 per 1s; }
    scope s08() { // 08
    rate_limit { allow 1 per 1s; }
    scope s09() { // 09
    rate_limit { allow 1 per 1s; }
    scope s10() { // 10
    rate_limit { allow 1 per 1s; }
    scope s11() { // 11
    rate_limit { allow 1 per 1s; }
    scope s12() { // 12
    rate_limit { allow 1 per 1s; }
    scope s13() { // 13
    rate_limit { allow 1 per 1s; }
    scope s14() { // 14
    rate_limit { allow 1 per 1s; }
    scope s15() { // 15
    rate_limit { allow 1 per 1s; }
    scope s16() { // 16
    rate_limit { allow 1 per 1s; }
    scope s17() { // 17
    rate_limit { allow 1 per 1s; }
    scope s18() { // 18
    rate_limit { allow 1 per 1s; }
    scope s19() { // 19
    rate_limit { allow 1 per 1s; }
    scope s20() { // 20
    rate_limit { allow 1 per 1s; }
    scope s21() { // 21
    rate_limit { allow 1 per 1s; }
    scope s22() { // 22
    rate_limit { allow 1 per 1s; }
    scope s23() { // 23
    rate_limit { allow 1 per 1s; }
    scope s24() { // 24
    rate_limit { allow 1 per 1s; }
    scope s25() { // 25
    rate_limit { allow 1 per 1s; }
    scope s26() { // 26
    rate_limit { allow 1 per 1s; }
    scope s27() { // 27
    rate_limit { allow 1 per 1s; }
    scope s28() { // 28
    rate_limit { allow 1 per 1s; }
    scope s29() { // 29
    rate_limit { allow 1 per 1s; }
    scope s30() { // 30
    rate_limit { allow 1 per 1s; }
    scope s31() { // 31
    rate_limit { allow 1 per 1s; }
    scope s32() { // 32
    rate_limit { allow 1 per 1s; }
    scope s33() { // 33
    rate_limit { allow 1 per 1s; }
    scope s34() { // 34
    rate_limit { allow 1 per 1s; }
    scope s35() { // 35
    rate_limit { allow 1 per 1s; }
    scope s36() { // 36
    rate_limit { allow 1 per 1s; }
    scope s37() { // 37
    rate_limit { allow 1 per 1s; }
    scope s38() { // 38
    rate_limit { allow 1 per 1s; }
    scope s39() { // 39
    rate_limit { allow 1 per 1s; }
    scope s40() { // 40
    rate_limit { allow 1 per 1s; }
    scope s41() { // 41
    rate_limit { allow 1 per 1s; }
    scope s42() { // 42
    rate_limit { allow 1 per 1s; }
    scope s43() { // 43
    rate_limit { allow 1 per 1s; }
    scope s44() { // 44
    rate_limit { allow 1 per 1s; }
    scope s45() { // 45
    rate_limit { allow 1 per 1s; }
    scope s46() { // 46
    rate_limit { allow 1 per 1s; }
    scope s47() { // 47
    rate_limit { allow 1 per 1s; }
    scope s48() { // 48
    rate_limit { allow 1 per 1s; }
    scope s49() { // 49
    rate_limit { allow 1 per 1s; }
    scope s50() { // 50
    rate_limit { allow 1 per 1s; }
    scope s51() { // 51
    rate_limit { allow 1 per 1s; }
    scope s52() { // 52
    rate_limit { allow 1 per 1s; }
    scope s53() { // 53
    rate_limit { allow 1 per 1s; }
    scope s54() { // 54
    rate_limit { allow 1 per 1s; }
    scope s55() { // 55
    rate_limit { allow 1 per 1s; }
    scope s56() { // 56
    rate_limit { allow 1 per 1s; }
    scope s57() { // 57
    rate_limit { allow 1 per 1s; }
    scope s58() { // 58
    rate_limit { allow 1 per 1s; }
    scope s59() { // 59
    rate_limit { allow 1 per 1s; }
    scope s60() { // 60
    rate_limit { allow 1 per 1s; }
    scope s61() { // 61
    rate_limit { allow 1 per 1s; }
    scope s62() { // 62
    rate_limit { allow 1 per 1s; }
    scope s63() { // 63
    rate_limit { allow 1 per 1s; }
    GET read() -> Bytes using Bytes {
        rate_limit { allow 1 per 1s; }
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
