//! C2.1, C2.2, C2.3, C2.10: the base, the host, the path (docs/reference/contracts.md, group C2;
//! docs/internals/engine.md, "How a value is written"; docs/reference/rules.md rules 5 and 6).

#[macro_use]
mod common;

use common::*;
use concord::__private::{AdaptValue as _, Core, Draft, Options};
use concord::error::Kind;
use concord::Base;

const HOST: &str = "build error: input region is not one label of a host, in tasks.get";
const BASE: &str = "build error: the base is not a URL that can be used, in tasks.get";
const WITH_BASE: &str =
    "build error: the base is not a URL that can be used (with_base), in tasks.get";
const SEGMENT: &str = "build error: input id gives an empty or dot path segment, in tasks.get";

/// `base "https://{region}.api.example.com";` then `path "ids";`.
fn host_hole(region: &str) -> Built {
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[
        lit!("https://"),
        val!("region", region),
        lit!(".api.example.com"),
    ]);
    draft.segment(&[lit!("ids")]);
    finish(draft, "tasks.get")
}

/// `base "https://api-{region}.example";` then `path "ids";`.
fn label_piece(region: &str) -> Built {
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[
        lit!("https://api-"),
        val!("region", region),
        lit!(".example"),
    ]);
    draft.segment(&[lit!("ids")]);
    finish(draft, "tasks.get")
}

/// `path "{id}";` under `base "https://api.example.com/v1";`.
fn path_hole(id: &str) -> Built {
    let mut draft = get();
    draft.segment(&[val!("id", id)]);
    finish(draft, "tasks.get")
}

/// `path "files/{*id}";` under `base "https://api.example.com/v1";`.
fn rest_hole(id: &str) -> Built {
    let mut draft = get();
    draft.segment(&[lit!("files")]);
    draft.segments("id", &(&concord::__private::ValueRef(&id)).adapt());
    finish(draft, "tasks.get")
}

/// `base url;` with an input of the type `Base`, then these fixed segments.
fn from_base(base: &str, segments: &[&'static str]) -> Built {
    let base: Base = base.parse().expect("a legal base");
    let mut draft = Draft::new(http::Method::GET);
    draft.base_from(&base);
    for segment in segments {
        draft.segment(&[lit!(segment)]);
    }
    finish(draft, "tasks.get")
}

// C1.2 -------------------------------------------------------------------------------

#[test]
fn c1_2_the_request_has_the_method_of_its_endpoint_and_a_custom_token_is_sent_as_written() {
    let token = http::Method::from_bytes(b"VERSION-CONTROL").expect("an HTTP token");
    for method in [
        http::Method::GET,
        http::Method::HEAD,
        http::Method::POST,
        http::Method::PUT,
        http::Method::PATCH,
        http::Method::DELETE,
        http::Method::OPTIONS,
        token,
    ] {
        let make = || {
            let mut draft = draft(method.clone(), "https://api.example.com/v1");
            draft.segment(&[lit!("items")]);
            draft
        };
        let request = ok(finish(make(), "tasks.get"));
        assert_eq!(request.method(), &method);
        assert_eq!(request.url().as_str(), "https://api.example.com/v1/items");
        assert_eq!(ok(finish_for_send(make(), "tasks.get")).method(), &method);
    }
}

// C2.1 -------------------------------------------------------------------------------

#[test]
fn c2_1_a_fixed_base_gives_the_scheme_the_host_and_the_prefix() {
    let mut draft = draft(http::Method::GET, "https://api.example.com:8443/v1");
    draft.segment(&[lit!("items")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com:8443/v1/items",
    );
}

#[test]
fn c2_1_a_host_hole_gives_one_label() {
    let request = ok(host_hole("europe"));
    assert_url(&request, "https://europe.api.example.com/ids");
    let longest = "a".repeat(63);
    let request = ok(host_hole(&longest));
    assert_url(&request, &format!("https://{longest}.api.example.com/ids"));
    for label in ["a", "eu-west-1", "x1", "1x"] {
        let request = ok(host_hole(label));
        assert_eq!(
            request.url().host_str(),
            Some(format!("{label}.api.example.com").as_str())
        );
    }
}

#[test]
fn c2_1_a_label_is_compared_without_case() {
    let request = ok(host_hole("EUrope"));
    assert_eq!(request.url().host_str(), Some("europe.api.example.com"));
}

#[test]
fn c2_1_a_hole_may_be_a_piece_of_a_label_and_the_finished_label_is_checked() {
    let request = ok(label_piece("eu"));
    assert_url(&request, "https://api-eu.example/ids");
    // "api-" and 59 more: 63 in all.
    assert!(label_piece(&"a".repeat(59)).is_ok());
    // "api-" and 60 more: 64 in all.
    assert_eq!(build_error(label_piece(&"a".repeat(60))), HOST);
    // The finished label would end with `-`.
    assert_eq!(build_error(label_piece("eu-")), HOST);
    assert_eq!(build_error(label_piece("")), HOST);
}

#[test]
fn c2_1_a_value_that_is_not_one_label_of_1_to_63_letters_digits_and_hyphens_is_refused() {
    let too_long = "a".repeat(64);
    for label in [
        "",
        too_long.as_str(),
        "-eu",
        "eu-",
        "-",
        "a_b",
        "a.b",
        "europe.evil.example",
        "a b",
        "a/b",
        "a:1",
        "a@b",
        "a%b",
        "a?b",
        "a#b",
        "a\\b",
        "a\tb",
        // a label is ASCII; an international name is given in its ASCII form.
        "é",
        "europé",
    ] {
        assert_eq!(build_error(host_hole(label)), HOST, "{label:?}");
    }
    // Its ASCII form is a label.
    assert_eq!(
        ok(host_hole("xn--europ-fsa")).url().host_str(),
        Some("xn--europ-fsa.api.example.com")
    );
}

#[test]
fn c2_1_a_port_hole_gives_digits() {
    let port_of = |port: &str| {
        let mut draft = Draft::new(http::Method::GET);
        draft.base(&[lit!("https://api.example.com:"), val!("port", port)]);
        draft.segment(&[lit!("ids")]);
        finish(draft, "tasks.get")
    };
    assert_url(&ok(port_of("8443")), "https://api.example.com:8443/ids");
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[lit!("https://api.example.com:"), val!("port", 8443u16)]);
    draft.segment(&[lit!("ids")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com:8443/ids",
    );
    // an empty port is the Build error of the base.
    for port in ["84a3", "-1", "+80", "99999", "80/x", "80 ", ""] {
        assert_eq!(build_error(port_of(port)), BASE, "{port:?}");
    }
}

#[test]
fn c2_1_a_number_that_a_parser_reads_as_an_address_is_refused() {
    let whole_host = |host: &str| {
        let mut draft = Draft::new(http::Method::GET);
        draft.base(&[lit!("https://"), val!("host", host)]);
        draft.segment(&[lit!("ids")]);
        finish(draft, "tasks.get")
    };
    // Each is a label of letters and digits; the URL library reads each as an address,
    // so the host after parsing is not the host that was written.
    for host in ["2130706433", "0x7f000001", "127", "0177"] {
        assert_eq!(build_error(whole_host(host)), BASE, "{host:?}");
    }
    // A host whose last label is a number does not parse as a host at all.
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[lit!("https://example."), val!("n", "1")]);
    draft.segment(&[lit!("ids")]);
    assert_eq!(build_error(finish(draft, "tasks.get")), BASE);
}

// C2.2 -------------------------------------------------------------------------------

#[test]
fn c2_2_a_base_is_a_scheme_a_host_maybe_a_port_and_maybe_a_prefix() {
    for text in [
        "https://api.example.com",
        "https://api.example.com/",
        "http://api.example.com",
        "https://api.example.com:8443",
        "https://api.example.com/gitlab",
        "https://api.example.com:8443/gitlab/api/",
        "http://[::1]:8080/prefix",
        "http://127.0.0.1:8080",
    ] {
        assert!(text.parse::<Base>().is_ok(), "{text:?} is a base");
    }
}

#[test]
fn c2_2_no_user_no_query_no_fragment() {
    for text in [
        "https://user@api.example.com",
        "https://user:pass@api.example.com/prefix",
        "https://@api.example.com",
        "https://api.example.com/prefix?q=1",
        "https://api.example.com/prefix?",
        "https://api.example.com?q=1",
        "https://api.example.com/prefix#part",
        "https://api.example.com#",
    ] {
        assert!(text.parse::<Base>().is_err(), "{text:?} is refused");
    }
}

#[test]
fn c2_2_only_http_and_https_with_a_host() {
    for text in [
        "ftp://api.example.com",
        "file:///etc",
        "api.example.com",
        "//api.example.com",
        "https://",
        "https:///prefix",
        "",
    ] {
        assert!(text.parse::<Base>().is_err(), "{text:?} is refused");
    }
}

#[test]
fn c2_2_the_same_host_port_and_path_in_another_spelling_are_taken() {
    // The scheme, case aside; the host as the library reads it on its own; the default port
    // written; the path once both are decoded.
    for text in [
        "HTTPS://api.example.com/prefix",
        "https://API.Example.COM/prefix",
        "https://[2001:0DB8:0:0:0:0:0:1]/prefix",
        "https://éxample.test/prefix",
        "https://api.example.com:443/prefix",
        "http://api.example.com:80/prefix",
        "https://api.example.com/caf%C3%A9",
        "https://api.example.com/café",
        "https://api.example.com/v1.0~x_y-z",
    ] {
        assert!(text.parse::<Base>().is_ok(), "{text:?} is a base");
    }
}

#[test]
fn c2_2_a_base_that_is_taken_gives_back_what_was_written_piece_by_piece() {
    // Each base of the two tests above, then `path "items";`: the URL that is sent.
    for (text, sent) in [
        ("https://api.example.com", "https://api.example.com/items"),
        ("https://api.example.com/", "https://api.example.com/items"),
        ("http://api.example.com", "http://api.example.com/items"),
        (
            "https://api.example.com:8443",
            "https://api.example.com:8443/items",
        ),
        (
            "https://api.example.com/gitlab",
            "https://api.example.com/gitlab/items",
        ),
        (
            "https://api.example.com:8443/gitlab/api/",
            "https://api.example.com:8443/gitlab/api/items",
        ),
        ("http://[::1]:8080/prefix", "http://[::1]:8080/prefix/items"),
        ("http://127.0.0.1:8080", "http://127.0.0.1:8080/items"),
        (
            "HTTPS://api.example.com/prefix",
            "https://api.example.com/prefix/items",
        ),
        (
            "https://API.Example.COM/prefix",
            "https://api.example.com/prefix/items",
        ),
        (
            "https://[2001:0DB8:0:0:0:0:0:1]/prefix",
            "https://[2001:db8::1]/prefix/items",
        ),
        (
            "https://éxample.test/prefix",
            "https://xn--xample-9ua.test/prefix/items",
        ),
        (
            "https://api.example.com:443/prefix",
            "https://api.example.com/prefix/items",
        ),
        (
            "http://api.example.com:80/prefix",
            "http://api.example.com/prefix/items",
        ),
        (
            "https://api.example.com/caf%C3%A9",
            "https://api.example.com/caf%C3%A9/items",
        ),
        (
            "https://api.example.com/café",
            "https://api.example.com/caf%C3%A9/items",
        ),
        (
            "https://api.example.com/v1.0~x_y-z",
            "https://api.example.com/v1.0~x_y-z/items",
        ),
    ] {
        let request = ok(from_base(text, &["items"]));
        assert_eq!(url(&request), sent, "{text:?}");
    }
}

#[test]
fn c2_2_what_the_library_would_change_with_no_word_is_refused() {
    for text in [
        // a `\`, which the URL would read as `/`
        "https://api.example.com\\prefix",
        "https://api.example.com/pre\\fix",
        // a control character, which the URL would lose
        "https://api.example.com/pre\tfix",
        "https://api.example.com/prefix\n",
        "https://api.\rexample.com/prefix",
        // a user found in what looked like a path
        "https:///user@api.example.com/prefix",
        "https:///api.example.com/prefix",
        // a dot piece
        "https://api.example.com/./prefix",
        "https://api.example.com/prefix/.",
        "https://api.example.com/a/../b",
        "https://api.example.com/..",
        // a port that is not one
        "https://api.example.com:99999",
        "https://api.example.com:8x",
    ] {
        assert!(text.parse::<Base>().is_err(), "{text:?} is refused");
    }
}

#[test]
fn c2_2_each_segment_of_the_prefix_is_held_to_c1_15_once_decoded() {
    for text in [
        "https://api.example.com/a//b",
        "https://api.example.com//a",
        "https://api.example.com/a b",
        "https://api.example.com/a%20b",
        "https://api.example.com/100%25",
        "https://api.example.com/%2e",
        "https://api.example.com/%2E%2e",
        "https://api.example.com/a%5Cb",
        "https://api.example.com/a%3Fb",
        "https://api.example.com/a%23b",
        "https://api.example.com/a%09b",
        "https://api.example.com/a%0Ab",
    ] {
        assert!(text.parse::<Base>().is_err(), "{text:?} is refused");
    }
}

#[test]
fn c2_2_the_prefix_comes_first_in_the_path() {
    assert_url(
        &ok(from_base("https://git.example.com/gitlab", &["api", "v4"])),
        "https://git.example.com/gitlab/api/v4",
    );
    assert_url(
        &ok(from_base(
            "https://git.example.com:8443/gitlab/",
            &["api", "v4"],
        )),
        "https://git.example.com:8443/gitlab/api/v4",
    );
    assert_url(
        &ok(from_base("https://api.example.com", &["v1", "items"])),
        "https://api.example.com/v1/items",
    );
}

#[test]
fn c2_2_a_trailing_slash_of_a_base_is_kept_only_when_no_path_follows() {
    assert_url(
        &ok(from_base("https://git.example.com/gitlab/", &[])),
        "https://git.example.com/gitlab/",
    );
    assert_url(
        &ok(from_base("https://git.example.com/gitlab", &[])),
        "https://git.example.com/gitlab",
    );
    assert_url(
        &ok(from_base("https://git.example.com/gitlab/", &["api"])),
        "https://git.example.com/gitlab/api",
    );
}

#[test]
fn c2_2_the_url_is_what_the_library_reads_a_base_to_be() {
    // Upper case, a default port and a longer spelling of an address are the same host.
    assert_url(
        &ok(from_base("https://API.Example.COM:443/Prefix", &["items"])),
        "https://api.example.com/Prefix/items",
    );
    assert_url(
        &ok(from_base(
            "https://[2001:0DB8:0:0:0:0:0:1]/prefix",
            &["items"],
        )),
        "https://[2001:db8::1]/prefix/items",
    );
}

#[test]
fn c2_2_with_base_takes_the_same_text_and_a_text_that_is_not_a_base_is_a_build_error() {
    // "A `with_base` that is no base: `build error: the base is not a URL that can be used (with_base), in <endpoint>`."
    for text in [
        "not a base",
        "https://user@api.example.com",
        "https://api.example.com/a//b",
        "https://api.example.com/?q=1",
    ] {
        let core = Core::<()>::default().with_base(text);
        let mut draft = get();
        draft.segment(&[lit!("items")]);
        let built = draft.finish(&core, &Options::<()>::default(), "tasks.get");
        assert_eq!(build_error(built), WITH_BASE, "{text:?}");
    }
}

// C2.3 -------------------------------------------------------------------------------

#[test]
fn c2_3_the_path_is_the_prefix_then_every_path_root_first_joined_with_one_slash() {
    let mut draft = get();
    draft.segment(&[lit!("users")]);
    draft.segment(&[val!("id", 42u64)]);
    draft.segment(&[lit!("repos")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/users/42/repos",
    );
}

#[test]
fn c2_3_a_path_hole_encodes_every_byte_outside_the_unreserved_set() {
    for c in (0u8..=127).map(char::from).chain(['é', '€', '😀']) {
        let value = format!("a{c}b");
        let request = ok(path_hole(&value));
        let expected = format!("https://api.example.com/v1/a{}b", encoded(c));
        assert_eq!(url(&request), expected, "the character {c:?}");
    }
    // `%` too: a value that looks encoded is encoded again.
    assert_url(
        &ok(path_hole("%2F%2e")),
        "https://api.example.com/v1/%252F%252e",
    );
    // So no value adds a `/`, a `?` or a `#`.
    let request = ok(path_hole("a/b?c=d#e"));
    assert_eq!(request.url().path(), "/v1/a%2Fb%3Fc%3Dd%23e");
    assert_eq!(request.url().query(), None);
    assert_eq!(request.url().fragment(), None);
}

#[test]
fn c2_3_a_hole_may_be_a_piece_of_a_segment() {
    let mut draft = get();
    draft.segment(&[lit!("v"), val!("version", "1 2"), lit!(".json")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/v1%202.json",
    );
}

#[test]
fn c2_3_an_empty_value_of_a_hole_is_a_build_error() {
    assert_eq!(build_error(path_hole("")), SEGMENT);
}

#[test]
fn c2_3_a_finished_segment_that_is_a_dot_or_two_is_a_build_error() {
    assert_eq!(build_error(path_hole(".")), SEGMENT);
    assert_eq!(build_error(path_hole("..")), SEGMENT);
    // `{x}.` with `x = "."` finishes as `..`.
    let mut draft = get();
    draft.segment(&[val!("id", "."), lit!(".")]);
    assert_eq!(build_error(finish(draft, "tasks.get")), SEGMENT);
    // `v{x}` with `x = ".."` gives `v..`, which is sent.
    let mut draft = get();
    draft.segment(&[lit!("v"), val!("id", "..")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/v..",
    );
    // Three dots is no dot segment.
    assert_url(&ok(path_hole("...")), "https://api.example.com/v1/...");
}

#[test]
fn c2_3_a_rest_hole_is_cut_at_each_slash_and_each_piece_is_encoded() {
    assert_url(
        &ok(rest_hole("src/lib.rs")),
        "https://api.example.com/v1/files/src/lib.rs",
    );
    assert_url(
        &ok(rest_hole("my docs/a?b#c%d")),
        "https://api.example.com/v1/files/my%20docs/a%3Fb%23c%25d",
    );
    let request = ok(rest_hole("a/b/c"));
    assert_eq!(request.url().path(), "/v1/files/a/b/c");
}

#[test]
fn c2_3_a_piece_of_a_rest_hole_may_be_empty() {
    // A final `/` and `//` are sent as they are.
    for (value, path) in [
        ("folder/", "/v1/files/folder/"),
        ("a//b", "/v1/files/a//b"),
        ("/object", "/v1/files//object"),
        ("a///", "/v1/files/a///"),
        ("/", "/v1/files//"),
    ] {
        let request = ok(rest_hole(value));
        assert_eq!(request.url().path(), path, "{value:?}");
    }
}

#[test]
fn c2_3_a_rest_hole_whose_whole_value_is_empty_is_a_build_error() {
    assert_eq!(build_error(rest_hole("")), SEGMENT);
}

#[test]
fn c2_3_a_piece_of_a_rest_hole_that_is_a_dot_or_two_is_a_build_error() {
    for value in [".", "..", "./a", "a/.", "a/../b", "a/./b", "a//.."] {
        assert_eq!(build_error(rest_hole(value)), SEGMENT, "{value:?}");
    }
    // Three dots, and a dot inside a piece, are no dot pieces.
    assert_eq!(
        ok(rest_hole(".../a.b/.c")).url().path(),
        "/v1/files/.../a.b/.c"
    );
}

#[test]
fn c2_3_the_url_ends_with_a_slash_when_the_last_piece_on_the_way_down_does() {
    // The base, when no path follows.
    let draft = draft(http::Method::GET, "https://api.example.com/v1/");
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/",
    );
    let draft = draft_of("https://api.example.com/v1");
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1",
    );
    // A path follows: the base's `/` is not kept.
    let mut draft = draft_of("https://api.example.com/v1/");
    draft.segment(&[lit!("items")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/items",
    );
    // The last path, a `{*x}` whose value ends with `/`.
    assert_eq!(ok(rest_hole("folder/")).url().path(), "/v1/files/folder/");
    // A `{*x}` whose value does not end with `/`, under a base that does.
    let mut draft = draft_of("https://api.example.com/v1/");
    draft.segments("id", &(&concord::__private::ValueRef(&"a/b")).adapt());
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/a/b",
    );
}

#[test]
fn c2_3_a_fixed_path_that_ends_with_a_slash_ends_the_url_with_one_when_it_is_the_last() {
    // `path "items/";` (the generated code gives the slash with the text).
    let mut draft = get();
    draft.segment(&[lit!("items/")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/items/",
    );
    // Under it, `path "{id}";`: the last path decides, and no `/` is doubled.
    let mut draft = get();
    draft.segment(&[lit!("items/")]);
    draft.segment(&[val!("id", 7u64)]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/items/7",
    );
    // A later path that ends with `/`, under one that does not.
    let mut draft = get();
    draft.segment(&[lit!("items")]);
    draft.segment(&[lit!("v"), val!("id", 7u64), lit!("/")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/items/v7/",
    );
    // Under a base that ends with `/`.
    let mut draft = draft_of("https://api.example.com/v1/");
    draft.segment(&[lit!("items/")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/items/",
    );
}

fn draft_of(base: &'static str) -> Draft {
    draft(http::Method::GET, base)
}

#[test]
fn c2_3_fixed_text_is_sent_as_written() {
    let mut draft = get();
    draft.segment(&[lit!("v1.0~x_y-z")]);
    draft.segment(&[lit!("ids")]);
    assert_url(
        &ok(finish(draft, "tasks.get")),
        "https://api.example.com/v1/v1.0~x_y-z/ids",
    );
}

#[test]
fn c2_5_a_path_hole_takes_one_text_and_nothing_or_several_is_a_build_error() {
    let with = |value: &dyn concord::__private::Value| {
        let mut draft = get();
        draft.segment(&[concord::__private::Piece::Val("id", value)]);
        finish(draft, "tasks.get")
    };
    // one pattern for every place.
    let text = cannot_be_written("id", "path", "tasks.get");
    let none = with(&(&concord::__private::ValueRef(&None::<u64>)).adapt());
    assert_eq!(build_error(none), text);
    let unit = with(&(&concord::__private::ValueRef(&())).adapt());
    assert_eq!(build_error(unit), text);
    let several = with(&(&concord::__private::ValueRef(&vec![1u64, 2])).adapt());
    assert_eq!(build_error(several), text);
    let empty = with(&(&concord::__private::ValueRef(&Vec::<u64>::new())).adapt());
    assert_eq!(build_error(empty), text);
    // A list of one is several, not one: a sequence or a tuple is several (C2.4).
    let one_item = with(&(&concord::__private::ValueRef(&vec![7u64])).adapt());
    assert_eq!(build_error(one_item), text);
    let one_tuple = with(&(&concord::__private::ValueRef(&(7u64,))).adapt());
    assert_eq!(build_error(one_tuple), text);
}

#[test]
fn c2_5_a_rest_hole_and_a_host_hole_take_one_text_and_a_list_of_one_is_a_build_error() {
    let rest = |value: &dyn concord::__private::Value| {
        let mut draft = get();
        draft.segments("id", value);
        finish(draft, "tasks.get")
    };
    let host = |value: &dyn concord::__private::Value| {
        let mut draft = Draft::new(http::Method::GET);
        draft.base(&[
            lit!("https://"),
            concord::__private::Piece::Val("region", value),
            lit!(".api.example.com"),
        ]);
        finish(draft, "tasks.get")
    };
    let path_text = cannot_be_written("id", "path", "tasks.get");
    let host_text = cannot_be_written("region", "host", "tasks.get");
    for value in [
        &(&concord::__private::ValueRef(&vec!["eu"])).adapt() as &dyn concord::__private::Value,
        &(&concord::__private::ValueRef(&("eu",))).adapt(),
        &(&concord::__private::ValueRef(&["eu", "us"])).adapt(),
        &(&concord::__private::ValueRef(&None::<String>)).adapt(),
    ] {
        assert_eq!(build_error(rest(value)), path_text);
        assert_eq!(build_error(host(value)), host_text);
    }
}

// C2.10 ------------------------------------------------------------------------------

#[test]
fn c2_10_with_base_replaces_the_scheme_the_host_the_port_and_the_prefix() {
    let core = Core::<()>::default().with_base("http://other.example.test:8080/gitlab");
    let mut draft = draft_of("https://api.example.com/v1");
    draft.segment(&[lit!("items")]);
    draft.segment(&[val!("id", 7u64)]);
    let request = ok(draft.finish(&core, &Options::<()>::default(), "tasks.get"));
    assert_url(&request, "http://other.example.test:8080/gitlab/items/7");
}

#[test]
fn c2_10_with_base_with_no_prefix_keeps_the_path_lines() {
    let base: Base = "https://other.example.test".parse().expect("a base");
    let core = Core::<()>::default().with_base(&base);
    let mut draft = draft_of("https://api.example.com/v1");
    draft.segment(&[lit!("items")]);
    let request = ok(draft.finish(&core, &Options::<()>::default(), "tasks.get"));
    assert_url(&request, "https://other.example.test/items");
}

#[test]
fn c2_10_with_base_replaces_a_base_with_a_host_hole() {
    let core = Core::<()>::default().with_base("https://other.example.test/p");
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[
        lit!("https://"),
        val!("region", "europe"),
        lit!(".api.example.com"),
    ]);
    draft.segment(&[lit!("ids")]);
    let request = ok(draft.finish(&core, &Options::<()>::default(), "tasks.get"));
    assert_url(&request, "https://other.example.test/p/ids");
}

#[test]
fn c2_10_with_base_discards_a_base_of_the_description_that_could_not_be_built() {
    // The host hole of the description would be a Build error; the base of the caller
    // replaces the whole base, so it is never built.
    let core = Core::<()>::default().with_base("https://other.example.test/p");
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[
        lit!("https://"),
        val!("region", "a.b"),
        lit!(".api.example.com"),
    ]);
    draft.segment(&[lit!("ids")]);
    let request = ok(draft.finish(&core, &Options::<()>::default(), "tasks.get"));
    assert_url(&request, "https://other.example.test/p/ids");
}

// -------------------------------------------------------------------------------

/// The longest URL that the HTTP layer can hold.
const LONGEST_URL: usize = 65_534;

const TOO_LONG: &str = "build error: the URL is too long to be sent, in files.readme";

/// `files.readme(repo)`, whose URL is the base, `repo` and `/readme`, with a middleware of the
/// caller that notes each try, over a script that answers once.
fn readme_of_length(length: usize) -> (tour::files::Readme, concord_testkit::Script, engine::Log) {
    let fixed = url(&ok(tour::Files::new("key").readme("a").request())).len() - 1;
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .header("content-type", "text/plain")
        .body("readme");
    let log = engine::Log::default();
    let files = engine::files(&script).with_middleware(engine::Logs {
        log: log.clone(),
        name: "caller",
    });
    (files.readme("a".repeat(length - fixed)), script, log)
}

#[tokio::test(start_paused = true)]
async fn c2_3_c2_11_a_url_of_65_534_bytes_is_sent_and_one_byte_more_is_a_build_error() {
    // "A request whose URL the HTTP layer cannot hold (longer than 65,534
    // bytes) is refused before any middleware sees it: `build error: the URL is too long to
    // be sent`, class Build, zero tries, nothing sent."
    for length in [LONGEST_URL - 1, LONGEST_URL] {
        let (call, script, log) = readme_of_length(length);
        assert_eq!(url(&ok(call.request())).len(), length);
        let text = engine::success(call.await);
        assert_eq!(text, "readme", "{length} bytes");
        let requests = script.requests();
        assert_eq!(requests.len(), 1, "{length} bytes");
        assert_eq!(requests[0].url().len(), length);
        assert_eq!(
            log.events().first().map(String::as_str),
            Some("caller out 1 files.readme")
        );
    }

    let (call, script, log) = readme_of_length(LONGEST_URL + 1);
    assert_eq!(build_error(call.request()), TOO_LONG);
    let error = engine::failure(call.await);
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.class(), None);
    assert_eq!(error.tries(), 0);
    assert_eq!(error.to_string(), TOO_LONG);
    assert!(!format!("{error:?}").contains("aaaa"));
    assert!(
        log.events().is_empty(),
        "a middleware ran: {:?}",
        log.events()
    );
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c2_6_a_query_that_makes_the_url_too_long_is_a_build_error() {
    // The same edge when the length is in the query: `feed.list` writes
    // `?starting_after=<value>`.
    let fixed = url(&ok(tour::Feed::new().list().starting_after("").request())).len();
    for (length, sent) in [(LONGEST_URL, true), (LONGEST_URL + 1, false)] {
        let script = concord_testkit::Script::new();
        script
            .answer(200)
            .header("content-type", "application/json")
            .body(r#"{"data":[],"next_cursor":null}"#);
        let feed = tour::Feed::new().with_transport(script.clone());
        let call = feed.list().starting_after("q".repeat(length - fixed));
        let result = call.send().await;
        if sent {
            engine::success(result);
            assert_eq!(script.requests()[0].url().len(), length);
        } else {
            let error = engine::failure(result);
            assert_eq!(
                error.to_string(),
                "build error: the URL is too long to be sent, in feed.list"
            );
            assert_eq!(error.tries(), 0);
            assert!(script.requests().is_empty());
        }
    }
}
