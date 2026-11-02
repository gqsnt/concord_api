//! Classification does not depend on whether a key is spelled as a native constant or a literal:
//! each case declares the same API twice, with `AUTHORIZATION` and with `"authorization"`, and
//! requires the same redaction and outgoing flags. From the final semantic review (R1, P2): a value
//! classified at run time on one chain must be withheld on chains with literal keys only, and
//! stickiness covers overridden contributions.

use concord::reqwest;

// Each case is written out twice: `api!` sees tokens, so a helper macro could not substitute
// the key inside it.
mod native_cross {
    use concord::reqwest::header::AUTHORIZATION;
    concord::api! {
        /// A shared literal slot on a chain without native keys.
        pub client Cross(token: String, other: String) {
            base "https://p.invalid";
            scope a { headers { AUTHORIZATION = client.token; } GET x(); }
            scope b { headers { "x-copy" = client.token; append "x-copy" = client.other; } GET y(); }
        }
    }
}
mod literal_cross {
    concord::api! {
        /// The same with a literal name.
        pub client Cross(token: String, other: String) {
            base "https://p.invalid";
            scope a { headers { "authorization" = client.token; } GET x(); }
            scope b { headers { "x-copy" = client.token; append "x-copy" = client.other; } GET y(); }
        }
    }
}
mod native_sticky {
    use concord::reqwest::header::AUTHORIZATION;
    concord::api! {
        /// An override, by a value or a literal, of a slot whose first value is conditional.
        pub client Sticky(token: String) {
            base "https://p.invalid";
            scope a { headers { AUTHORIZATION = client.token; } GET x(); }
            scope b {
                headers { "x-k" = client.token; }
                GET y(v: String) { headers { override "x-k" = v; } }
                GET fixed() { headers { override "x-k" = "fixed"; } }
            }
        }
    }
}
mod literal_sticky {
    concord::api! {
        /// The same with a literal name.
        pub client Sticky(token: String) {
            base "https://p.invalid";
            scope a { headers { "authorization" = client.token; } GET x(); }
            scope b {
                headers { "x-k" = client.token; }
                GET y(v: String) { headers { override "x-k" = v; } }
                GET fixed() { headers { override "x-k" = "fixed"; } }
            }
        }
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

#[test]
fn a_conditional_value_is_withheld_on_literal_chains() {
    let native = native_cross::Cross::new(
        http(),
        native_cross::CrossConfig {
            token: "TOKEN".into(),
            other: "OTHER".into(),
        },
    );
    let literal = literal_cross::Cross::new(
        http(),
        literal_cross::CrossConfig {
            token: "TOKEN".into(),
            other: "OTHER".into(),
        },
    );
    assert_eq!(format!("{native:?}"), format!("{literal:?}"));
    assert!(!format!("{native:?}").contains("OTHER"), "{native:?}");
    let (n, l) = (
        native.b().y().prepare().unwrap(),
        literal.b().y().prepare().unwrap(),
    );
    assert_eq!(format!("{n:?}"), format!("{l:?}"));
    assert!(
        !format!("{n:?}").contains("OTHER") && !format!("{n:?}").contains("TOKEN"),
        "{n:?}"
    );
    for (a, b) in n
        .request()
        .headers()
        .get_all("x-copy")
        .iter()
        .zip(l.request().headers().get_all("x-copy"))
    {
        assert_eq!(a.is_sensitive(), b.is_sensitive());
        assert!(a.is_sensitive());
    }
}

#[test]
fn stickiness_covers_overridden_conditional_values() {
    let native = native_sticky::Sticky::new(
        http(),
        native_sticky::StickyConfig {
            token: "TOKEN".into(),
        },
    );
    let literal = literal_sticky::Sticky::new(
        http(),
        literal_sticky::StickyConfig {
            token: "TOKEN".into(),
        },
    );
    let (n, l) = (
        native.a().x().prepare().unwrap(),
        literal.a().x().prepare().unwrap(),
    );
    assert_eq!(format!("{n:?}"), format!("{l:?}"));
    assert!(n.request().headers()["authorization"].is_sensitive());
    let (n, l) = (native.b().y("VALUE"), literal.b().y("VALUE"));
    assert_eq!(format!("{n:?}"), format!("{l:?}"));
    assert!(!format!("{n:?}").contains("VALUE"), "{n:?}");
    let (n, l) = (n.prepare().unwrap(), l.prepare().unwrap());
    assert_eq!(format!("{n:?}"), format!("{l:?}"));
    assert!(n.request().headers()["x-k"].is_sensitive());
    assert!(l.request().headers()["x-k"].is_sensitive());
    let (n, l) = (
        native.b().fixed().prepare().unwrap(),
        literal.b().fixed().prepare().unwrap(),
    );
    assert_eq!(format!("{n:?}"), format!("{l:?}"));
    assert_eq!(
        n.request().headers()["x-k"].is_sensitive(),
        l.request().headers()["x-k"].is_sensitive()
    );
}
