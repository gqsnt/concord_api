use concord::prelude::*;

api! {
    client RateRuleLimitApi {
        base "https://example.test";
        rate_limit { 1 / 1s; }
    }
    scope s() { // 01
    rate_limit { 1 / 1s; }
    scope s() { // 02
    rate_limit { 1 / 1s; }
    scope s() { // 03
    rate_limit { 1 / 1s; }
    scope s() { // 04
    rate_limit { 1 / 1s; }
    scope s() { // 05
    rate_limit { 1 / 1s; }
    scope s() { // 06
    rate_limit { 1 / 1s; }
    scope s() { // 07
    rate_limit { 1 / 1s; }
    scope s() { // 08
    rate_limit { 1 / 1s; }
    scope s() { // 09
    rate_limit { 1 / 1s; }
    scope s() { // 10
    rate_limit { 1 / 1s; }
    scope s() { // 11
    rate_limit { 1 / 1s; }
    scope s() { // 12
    rate_limit { 1 / 1s; }
    scope s() { // 13
    rate_limit { 1 / 1s; }
    scope s() { // 14
    rate_limit { 1 / 1s; }
    scope s() { // 15
    rate_limit { 1 / 1s; }
    scope s() { // 16
    rate_limit { 1 / 1s; }
    scope s() { // 17
    rate_limit { 1 / 1s; }
    scope s() { // 18
    rate_limit { 1 / 1s; }
    scope s() { // 19
    rate_limit { 1 / 1s; }
    scope s() { // 20
    rate_limit { 1 / 1s; }
    scope s() { // 21
    rate_limit { 1 / 1s; }
    scope s() { // 22
    rate_limit { 1 / 1s; }
    scope s() { // 23
    rate_limit { 1 / 1s; }
    scope s() { // 24
    rate_limit { 1 / 1s; }
    scope s() { // 25
    rate_limit { 1 / 1s; }
    scope s() { // 26
    rate_limit { 1 / 1s; }
    scope s() { // 27
    rate_limit { 1 / 1s; }
    scope s() { // 28
    rate_limit { 1 / 1s; }
    scope s() { // 29
    rate_limit { 1 / 1s; }
    scope s() { // 30
    rate_limit { 1 / 1s; }
    scope s() { // 31
    rate_limit { 1 / 1s; }
    scope s() { // 32
    rate_limit { 1 / 1s; }
    scope s() { // 33
    rate_limit { 1 / 1s; }
    scope s() { // 34
    rate_limit { 1 / 1s; }
    scope s() { // 35
    rate_limit { 1 / 1s; }
    scope s() { // 36
    rate_limit { 1 / 1s; }
    scope s() { // 37
    rate_limit { 1 / 1s; }
    scope s() { // 38
    rate_limit { 1 / 1s; }
    scope s() { // 39
    rate_limit { 1 / 1s; }
    scope s() { // 40
    rate_limit { 1 / 1s; }
    scope s() { // 41
    rate_limit { 1 / 1s; }
    scope s() { // 42
    rate_limit { 1 / 1s; }
    scope s() { // 43
    rate_limit { 1 / 1s; }
    scope s() { // 44
    rate_limit { 1 / 1s; }
    scope s() { // 45
    rate_limit { 1 / 1s; }
    scope s() { // 46
    rate_limit { 1 / 1s; }
    scope s() { // 47
    rate_limit { 1 / 1s; }
    scope s() { // 48
    rate_limit { 1 / 1s; }
    scope s() { // 49
    rate_limit { 1 / 1s; }
    scope s() { // 50
    rate_limit { 1 / 1s; }
    scope s() { // 51
    rate_limit { 1 / 1s; }
    scope s() { // 52
    rate_limit { 1 / 1s; }
    scope s() { // 53
    rate_limit { 1 / 1s; }
    scope s() { // 54
    rate_limit { 1 / 1s; }
    scope s() { // 55
    rate_limit { 1 / 1s; }
    scope s() { // 56
    rate_limit { 1 / 1s; }
    scope s() { // 57
    rate_limit { 1 / 1s; }
    scope s() { // 58
    rate_limit { 1 / 1s; }
    scope s() { // 59
    rate_limit { 1 / 1s; }
    scope s() { // 60
    rate_limit { 1 / 1s; }
    scope s() { // 61
    rate_limit { 1 / 1s; }
    scope s() { // 62
    rate_limit { 1 / 1s; }
    scope s() { // 63
    rate_limit { 1 / 1s; }
    GET read() -> Bytes {
        rate_limit { 1 / 1s; }
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
}

fn main() {}
