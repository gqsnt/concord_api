//! Six pagers. Each is a function that returns a function from the call and its response to the
//! next call.
//!
//! In a `paginate` statement, an input of the endpoint names its slot on the call
//! ([`Slot`]). So `paginate by_offset_limit(start, count);` hands two slots to
//! [`by_offset_limit`]. Nothing here is known to the macro.

use concord_core::error::{mismatch, Fail};
use concord_core::page::{Call, IntoItems, Slot};
use concord_core::Answer;

fn advance<C: Call, S>(start: S, call: C, got: usize) -> Result<Option<C>, Fail<C::Problem>>
where
    S: Slot<C>,
    S::Value: Copy + Default + TryFrom<u64> + TryInto<u64>,
{
    let at = start.get(&call).copied().unwrap_or_default();
    // counted in `u64`, with a check: an offset that cannot grow is an error, never a wrap
    let next = at
        .try_into()
        .ok()
        // PANIC-SAFETY: usize is at most 64 bits on supported targets; offset addition is checked.
        .and_then(|at: u64| at.checked_add(got as u64))
        .and_then(|next| S::Value::try_from(next).ok());
    match next {
        Some(next) => Ok(Some(start.set(call, next))),
        None => Err(mismatch("the offset cannot grow any further")),
    }
}

/// `paginate by_offset(start);`: the next call starts after the items that arrived. The pages end
/// with an empty page.
// Keep the public pager signature's explicit call, answer and problem types.
// PANIC-SAFETY: The closure preserves the call, answer and problem types without boxing; no panic lint is relaxed.
#[allow(clippy::type_complexity)]
pub fn by_offset<C, S>(
    start: S,
) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + Default + TryFrom<u64> + TryInto<u64>,
    C::Value: IntoItems,
{
    move |call, answer| match answer.value().count() {
        0 => Ok(None),
        got => advance(start, call, got),
    }
}

/// `paginate by_offset_limit(start, count);`: like [`by_offset`], and the pages also end with a
/// page that is shorter than the `count` that the caller set. Use this only for an API
/// that promises full pages until the last one.
///
/// The first request carries what the caller set and nothing more: with no `start` it has
/// no offset pair (it counts as 0), and with no `count` no limit pair. Each next request has
/// `start` plus the number of items that arrived, and the same `count`. When the caller set
/// no `count`, only an empty page ends the pages, as with [`by_offset`].
// Keep the public pager signature's explicit call, answer and problem types.
// PANIC-SAFETY: The closure preserves the call, answer and problem types without boxing; no panic lint is relaxed.
#[allow(clippy::type_complexity)]
pub fn by_offset_limit<C, S, K>(
    start: S,
    limit: K,
) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + Default + TryFrom<u64> + TryInto<u64>,
    K: Slot<C>,
    K::Value: Copy + TryInto<usize>,
    C::Value: IntoItems,
{
    move |call, answer| {
        let got = answer.value().count();
        if got == 0 {
            return Ok(None);
        }
        if let Some(asked) = limit
            .get(&call)
            .copied()
            .and_then(|asked| asked.try_into().ok())
        {
            if got < asked {
                return Ok(None);
            }
        }
        advance(start, call, got)
    }
}

/// `paginate by_page(page, 1);`: the next call requests the following page number.
/// When no number was supplied, the page just received is `first`. An empty page ends
/// the sequence; a number that cannot be incremented is a Mismatch.
// Keep the public pager signature's explicit call, answer and problem types.
// PANIC-SAFETY: The closure preserves the call, answer and problem types without boxing; no panic lint is relaxed.
#[allow(clippy::type_complexity)]
pub fn by_page<C, S>(
    page: S,
    first: S::Value,
) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + TryFrom<u64> + TryInto<u64>,
    C::Value: IntoItems,
{
    let first = first.try_into().ok();
    move |call, answer| {
        if answer.value().count() == 0 {
            return Ok(None);
        }
        let at = match page.get(&call) {
            Some(at) => (*at).try_into().ok(),
            None => first,
        };
        let next = at
            .and_then(|at: u64| at.checked_add(1))
            .and_then(|next| S::Value::try_from(next).ok());
        match next {
            Some(next) => Ok(Some(page.set(call, next))),
            None => Err(mismatch("the page number cannot grow any further")),
        }
    }
}

/// `paginate by_cursor(after, Page::next);`: the input takes what your function finds in the
/// page. The pages end when it finds nothing. The length of a page is never checked.
pub fn by_cursor<C, S, F>(
    input: S,
    next: F,
) -> impl Fn(C, &Answer<C::Value>) -> Option<C> + Send + 'static
where
    C: Call,
    S: Slot<C>,
    F: Fn(&C::Value) -> Option<S::Value> + Send + 'static,
{
    move |call, answer| next(answer.value()).map(|value| input.set(call, value))
}

/// `paginate by_next_url(Page::next);`: the same call, sent to the URL that your function reads
/// from the page. The pages end when it finds none.
pub fn by_next_url<C, F, U>(next: F) -> impl Fn(C, &Answer<C::Value>) -> Option<C> + Send + 'static
where
    C: Call,
    F: Fn(&C::Value) -> Option<U> + Send + 'static,
    U: AsRef<str>,
{
    move |call, answer| {
        next(answer.value()).map(|url| call.at_url(concord_core::page::fragmentless(url.as_ref())))
    }
}

/// `paginate by_link();`: the same call, sent to the URL of `Link: <…>; rel="next"` (RFC 8288).
/// The pages end when the response has no such link. A link with `anchor` is ignored.
/// A field with invalid text or malformed quotes is a Mismatch.
/// The target is parsed as `reqwest` parses a URL (using the `url` crate, like a browser),
/// so `</a|b>` and bad percent escapes are accepted. This pager checks the field's
/// brackets, parameters and quotes; the core checks every page's origin.
// Keep the public pager signature's explicit call, answer and problem types.
// PANIC-SAFETY: The closure preserves the call, answer and problem types without boxing; no panic lint is relaxed.
#[allow(clippy::type_complexity)]
pub fn by_link<C: Call>(
) -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static {
    move |call, answer| {
        let mut next = None;
        for header in answer.headers().as_map().get_all("link").iter() {
            // HeaderValue::to_str rejects non-ASCII, including legal UTF-8 quoted text.
            let found = next_link(header.as_bytes())
                .map_err(|()| mismatch("the Link field could not be read"))?;
            if next.is_none() {
                next = found;
            }
        }
        Ok(next.map(|url| call.at_url(concord_core::page::fragmentless(url))))
    }
}

/// Read quoted text as bytes, including RFC quoted-pairs and UTF-8 octets.
fn parameter_value(value: &[u8]) -> Result<Vec<u8>, ()> {
    if value.first() != Some(&b'"') {
        return if !value.iter().all(|&b| token(b)) {
            Err(())
        } else {
            Ok(value.to_vec())
        };
    }
    let mut result = Vec::new();
    let mut index = 1;
    while let Some(&byte) = value.get(index) {
        match byte {
            b'"' => {
                // PANIC-SAFETY: The parser offsets come from byte iteration or a found ASCII delimiter, strictly below the slice length.
                return if index + 1 == value.len() {
                    Ok(result)
                } else {
                    Err(())
                };
            }
            b'\\' => {
                // PANIC-SAFETY: The parser offsets come from byte iteration or a found ASCII delimiter, strictly below the slice length.
                index += 1;
                let escaped = *value.get(index).ok_or(())?;
                if !(escaped == b'\t' || escaped >= 0x20 && escaped != 0x7f) {
                    return Err(());
                }
                result.push(escaped);
            }
            _ => {
                if !(byte == b'\t' || byte >= 0x20 && byte != 0x7f) {
                    return Err(());
                }
                result.push(byte);
            }
        }
        // PANIC-SAFETY: The parser offsets come from byte iteration or a found ASCII delimiter, strictly below the slice length.
        index += 1;
    }
    Err(())
}

/// The target of the first unanchored link whose relation is `next`.
fn next_link(header: &[u8]) -> Result<Option<&str>, ()> {
    let mut next = None;
    for link in split_link_parts(header, b',')? {
        let link = trim(link);
        if link.is_empty() {
            continue;
        } // Empty list members are permitted by HTTP.
        let link = link.strip_prefix(b"<").ok_or(())?;
        let end = link.iter().position(|&byte| byte == b'>').ok_or(())?;
        let target = std::str::from_utf8(link.get(..end).ok_or(())?).map_err(|_| ())?;
        if target
            .bytes()
            .any(|b| b <= 0x20 || b == 0x7f || matches!(b, b'<' | b'"' | b'\\'))
        {
            return Err(());
        }
        // PANIC-SAFETY: The parser offsets come from byte iteration or a found ASCII delimiter, strictly below the slice length.
        let mut parameters = split_link_parts(link.get(end + 1..).ok_or(())?, b';')?.into_iter();
        if !trim(parameters.next().unwrap_or(b"")).is_empty() {
            return Err(());
        }
        let (mut relation, mut anchored) = (None, false);
        for parameter in parameters {
            let parameter = trim(parameter);
            let split = parameter.iter().position(|&byte| byte == b'=');
            let (name, value) = match split {
                Some(at) => (
                    parameter.get(..at).ok_or(())?,
                    // PANIC-SAFETY: The parser offsets come from byte iteration or a found ASCII delimiter, strictly below the slice length.
                    parameter.get(at + 1..).ok_or(())?,
                ),
                None => (parameter, b"".as_slice()),
            };
            if trim(name).is_empty() || !trim(name).iter().all(|&b| token(b)) {
                return Err(());
            }
            if split.is_some() && trim(value).is_empty() {
                return Err(());
            }
            let value = parameter_value(trim(value))?;
            if trim(name).eq_ignore_ascii_case(b"anchor") {
                anchored = true;
            }
            if trim(name).eq_ignore_ascii_case(b"rel") && relation.is_none() {
                relation = Some(value);
            }
        }
        if !anchored
            && next.is_none()
            && relation.is_some_and(|value| {
                value
                    .split(|&byte| byte == b' ')
                    .any(|part| part.eq_ignore_ascii_case(b"next"))
            })
        {
            next = Some(target);
        }
    }
    Ok(next)
}

fn trim(value: &[u8]) -> &[u8] {
    value.trim_ascii_start().trim_ascii_end()
}
fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

// Delimiters inside quoted strings and target URIs are data. Validate the whole
// field even after finding a next link, so bad quotes never look like completion.
fn split_link_parts(text: &[u8], delimiter: u8) -> Result<Vec<&[u8]>, ()> {
    let (mut quoted, mut escaped, mut target) = (false, false, false);
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, &byte) in text.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if quoted => escaped = true,
            b'"' if !target => quoted = !quoted,
            b'<' if !quoted => target = true,
            b'>' if !quoted => target = false,
            _ => {}
        }
        if byte == delimiter && !quoted && !target {
            parts.push(text.get(start..index).ok_or(())?);
            // PANIC-SAFETY: The parser offsets come from byte iteration or a found ASCII delimiter, strictly below the slice length.
            start = index + 1;
        }
    }
    if quoted || escaped || target {
        return Err(());
    }
    parts.push(text.get(start..).ok_or(())?);
    Ok(parts)
}

#[cfg(test)]
mod tests {
    //! These tests run: a pager is a plain function, so it needs no network and no engine.
    use super::*;
    use concord_core::http::{HeaderMap, HeaderValue, StatusCode};
    use concord_core::Headers;

    /// A call written by hand, as the macro would write one: two optional inputs, and a URL.
    #[derive(Clone, Debug, PartialEq)]
    struct List {
        start: Option<u32>,
        count: Option<u32>,
        after: Option<String>,
        url: Option<String>,
    }
    impl Call for List {
        type Value = Page;
        type Problem = ();
        fn at_url(mut self, url: &str) -> List {
            self.url = Some(url.to_owned());
            self
        }
    }
    #[derive(Clone, Copy)]
    struct Start;
    impl Slot<List> for Start {
        type Value = u32;
        fn get<'a>(&self, call: &'a List) -> Option<&'a u32> {
            call.start.as_ref()
        }
        fn set(&self, mut call: List, value: u32) -> List {
            call.start = Some(value);
            call
        }
    }
    #[derive(Clone, Copy)]
    struct Count;
    impl Slot<List> for Count {
        type Value = u32;
        fn get<'a>(&self, call: &'a List) -> Option<&'a u32> {
            call.count.as_ref()
        }
        fn set(&self, mut call: List, value: u32) -> List {
            call.count = Some(value);
            call
        }
    }
    #[derive(Clone, Copy)]
    struct After;
    impl Slot<List> for After {
        type Value = String;
        fn get<'a>(&self, call: &'a List) -> Option<&'a String> {
            call.after.as_ref()
        }
        fn set(&self, mut call: List, value: String) -> List {
            call.after = Some(value);
            call
        }
    }

    /// A list that comes wrapped, as most do.
    struct Page {
        items: Vec<u32>,
        next: Option<String>,
    }
    impl Page {
        fn next(&self) -> Option<String> {
            self.next.clone()
        }
    }
    impl IntoItems for Page {
        type Item = u32;
        fn into_items(self) -> Vec<u32> {
            self.items
        }
        fn count(&self) -> usize {
            self.items.len()
        }
    }

    fn list() -> List {
        List {
            start: None,
            count: None,
            after: None,
            url: None,
        }
    }
    fn answer(items: &[u32], next: Option<&str>, link: Option<&'static str>) -> Answer<Page> {
        let mut headers = HeaderMap::new();
        if let Some(link) = link {
            headers.insert("link", HeaderValue::from_static(link));
        }
        Answer::new(
            StatusCode::OK,
            Headers::from(headers),
            Page {
                items: items.to_vec(),
                next: next.map(String::from),
            },
        )
    }

    #[test]
    fn c8_6_by_offset_moves_by_what_came_and_ends_on_an_empty_page() {
        let pager = by_offset(Start);
        let second = pager(list(), &answer(&[1, 2, 3], None, None))
            .unwrap()
            .unwrap();
        assert_eq!(second.start, Some(3));
        let third = pager(second, &answer(&[4], None, None)).unwrap().unwrap();
        assert_eq!(third.start, Some(4)); // a short page is not the end
        assert!(pager(third, &answer(&[], None, None)).unwrap().is_none());
    }

    #[test]
    fn c8_6_an_offset_that_cannot_grow_is_an_error_and_not_a_wrap() {
        let pager = by_offset(Start);
        let at_the_end = List {
            start: Some(u32::MAX - 1),
            ..list()
        };
        assert!(pager(at_the_end, &answer(&[1, 2, 3], None, None)).is_err());
    }

    #[test]
    fn c8_6_by_offset_limit_also_ends_on_a_short_page() {
        let pager = by_offset_limit(Start, Count);
        let first = List {
            count: Some(3),
            ..list()
        };
        let second = pager(first, &answer(&[1, 2, 3], None, None))
            .unwrap()
            .unwrap();
        assert_eq!((second.start, second.count), (Some(3), Some(3))); // the other inputs are kept
        assert!(pager(second, &answer(&[4], None, None)).unwrap().is_none());
        // with no count given by the caller, only an empty page ends
        assert!(pager(list(), &answer(&[1], None, None)).unwrap().is_some());
    }

    #[test]
    fn c8_6_by_cursor_never_looks_at_the_length() {
        let pager = by_cursor(After, Page::next);
        let second = pager(list(), &answer(&[], Some("c2"), None)).unwrap(); // an empty page in the middle
        assert_eq!(second.after.as_deref(), Some("c2"));
        assert!(pager(second, &answer(&[7, 8], None, None)).is_none());
    }

    #[test]
    fn c8_6_by_next_url_sends_the_same_call_elsewhere() {
        let pager = by_next_url(Page::next);
        let first = List {
            count: Some(50),
            ..list()
        };
        let second = pager(first, &answer(&[1], Some("/v2/items?page=2"), None)).unwrap();
        assert_eq!(
            (second.url.as_deref(), second.count),
            (Some("/v2/items?page=2"), Some(50))
        );
    }

    #[test]
    fn c8_6_by_link_follows_rel_next_only() {
        let pager = by_link();
        let github = r#"<https://api.github.com/repos?page=3>; rel="next", <https://api.github.com/repos?page=9>; rel="last""#;
        assert_eq!(
            pager(list(), &answer(&[1], None, Some(github)))
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("https://api.github.com/repos?page=3")
        );
        let last = r#"<https://api.github.com/repos?page=1>; rel="first", <https://api.github.com/repos?page=8>; rel="prev""#;
        assert!(pager(list(), &answer(&[1], None, Some(last)))
            .unwrap()
            .is_none());
        assert!(pager(list(), &answer(&[1], None, None)).unwrap().is_none());
    }

    #[test]
    fn c8_6_by_link_accepts_browser_url_targets() {
        for (target, field) in [
            ("/a|b", "</a|b>; rel=\"next\""),
            ("/bad%GG", "</bad%GG>; rel=\"next\""),
        ] {
            let next = by_link()(list(), &answer(&[1], None, Some(field)))
                .unwrap()
                .unwrap();
            assert_eq!(next.url.as_deref(), Some(target));
            let base = concord_core::reqwest::Url::parse("https://x.test/").unwrap();
            assert_eq!(
                base.join(target).unwrap().as_str(),
                format!("https://x.test{target}")
            );
        }
    }

    #[test]
    fn c8_6_by_link_reads_next_in_a_second_header_field() {
        let mut headers = HeaderMap::new();
        headers.append(
            "link",
            HeaderValue::from_static(r#"</items?page=1>; rel="prev""#),
        );
        headers.append(
            "link",
            HeaderValue::from_static(r#"</items?page=3>; rel="next""#),
        );
        let answer = Answer::new(
            StatusCode::OK,
            Headers::from(headers),
            Page {
                items: vec![1],
                next: None,
            },
        );
        assert_eq!(
            by_link()(list(), &answer).unwrap().unwrap().url.as_deref(),
            Some("/items?page=3")
        );
    }

    #[test]
    fn c8_6_by_link_keeps_quoted_semicolons_and_commas_inside_their_parameter() {
        let pager = by_link();
        assert!(pager(
            list(),
            &answer(
                &[1],
                None,
                Some(r#"</prev>; rel=prev; title="x; rel=next""#)
            )
        )
        .unwrap()
        .is_none());
        assert!(pager(
            list(),
            &answer(&[1], None, Some(r#"</prev>; title="x; rel=next""#))
        )
        .unwrap()
        .is_none());
        let next = pager(
            list(),
            &answer(&[1], None, Some(r#"</a>; rel="next"; title="a, b""#)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(next.url.as_deref(), Some("/a"));
        let next = pager(list(), &answer(&[1], None,
            Some(r#"</prev>; title="a, </fake>; rel=next"; rel=prev, </a>; title="a; b, c"; rel=next"#))).unwrap().unwrap();
        assert_eq!(next.url.as_deref(), Some("/a"));
    }

    #[test]
    fn c8_6_by_link_handles_quoted_pairs_and_delimiters_in_targets() {
        for header in [
            r#"</prev>; title="a\"; rel=next, </fake>"; rel=prev, </a>; rel=next"#,
            r#"</prev>; title="a\\"; rel=prev, </a>; rel="n\ext""#,
            r#"</prev>; title="a<"; rel=prev, </a>; rel=next"#,
        ] {
            assert_eq!(
                next_link(header.as_bytes()).unwrap(),
                Some("/a"),
                "{header}"
            );
        }
        assert_eq!(
            next_link(r#"</a,b;c>; rel="next""#.as_bytes()).unwrap(),
            Some("/a,b;c")
        );
        assert_eq!(
            next_link(r#"</prev>; title="a\"; rel=next""#.as_bytes()).unwrap(),
            None
        );
    }

    #[test]
    fn c8_6_by_link_uses_only_the_first_rel_parameter() {
        assert_eq!(
            next_link(r#"</prev>; rel=prev; rel=next, </a>; rel=next"#.as_bytes()).unwrap(),
            Some("/a")
        );
        assert_eq!(
            next_link(r#"</a>; rel=next; rel=prev"#.as_bytes()).unwrap(),
            Some("/a")
        );
    }

    #[test]
    fn c8_6_the_link_header_is_read_as_the_rfc_writes_it() {
        assert_eq!(next_link(r#"<a>; rel=next"#.as_bytes()).unwrap(), Some("a")); // a token, not a quoted string
        assert_eq!(
            next_link(r#"<a>; title="x"; rel="prev next""#.as_bytes()).unwrap(),
            Some("a")
        ); // several relations in one value
        assert_eq!(
            next_link(r#"<a>; rel="prev", <b>; REL="Next""#.as_bytes()).unwrap(),
            Some("b")
        );
        assert_eq!(next_link(r#"<a>; rel="nextish""#.as_bytes()).unwrap(), None);
        assert!(next_link("nonsense".as_bytes()).is_err());
    }

    #[test]
    fn c8_6_by_link_rejects_bad_quotes_instead_of_ending_pages() {
        for field in [
            r#"</a>; rel="next"#,
            r#"</a>; title="bad\"#,
            r#"</a>; rel="next"garbage"#,
        ] {
            assert!(
                by_link()(list(), &answer(&[1], None, Some(field))).is_err(),
                "{field}"
            );
        }
        let mut headers = HeaderMap::new();
        headers.append("link", HeaderValue::from_static("</a>; rel=next"));
        headers.append("link", HeaderValue::from_static("</b>; title=\"bad"));
        let response = Answer::new(
            StatusCode::OK,
            Headers::from(headers),
            Page {
                items: vec![1],
                next: None,
            },
        );
        assert!(by_link()(list(), &response).is_err());
    }

    #[test]
    fn c8_6_by_link_accepts_obs_text_in_quoted_parameters() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "link",
            HeaderValue::from_bytes(b"</a>; rel=next; title=\"\xff\"").unwrap(),
        );
        let response = Answer::new(
            StatusCode::OK,
            Headers::from(headers),
            Page {
                items: vec![1],
                next: None,
            },
        );
        assert_eq!(
            by_link()(list(), &response)
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("/a")
        );
    }

    #[test]
    fn c8_6_by_link_ignores_anchors_even_after_rel() {
        for field in [
            r#"</a>; rel=next; anchor="/elsewhere""#,
            r#"</a>; anchor="/"; rel=next"#,
        ] {
            assert!(by_link()(list(), &answer(&[1], None, Some(field)))
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn c8_6_by_link_reads_utf8_quoted_text_as_bytes() {
        let field = r#"</a>; title="café, 日本; \"next\""; rel=next"#;
        let mut headers = HeaderMap::new();
        headers.insert("link", HeaderValue::from_bytes(field.as_bytes()).unwrap());
        let response = Answer::new(
            StatusCode::OK,
            Headers::from(headers),
            Page {
                items: vec![1],
                next: None,
            },
        );
        assert_eq!(
            by_link()(list(), &response)
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("/a")
        );
    }

    #[test]
    fn c8_6_next_urls_drop_fragments() {
        assert_eq!(
            by_next_url(Page::next)(list(), &answer(&[1], Some("/a?x=1#part"), None))
                .unwrap()
                .url
                .as_deref(),
            Some("/a?x=1")
        );
        assert_eq!(
            by_link()(list(), &answer(&[1], None, Some("</a?x=1#part>; rel=next")))
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("/a?x=1")
        );
    }

    #[test]
    fn c8_6_numbered_pages_advance_the_number_that_was_answered() {
        let pager = by_page(Start, 1);
        let next = pager(
            List {
                start: Some(7),
                count: Some(20),
                ..list()
            },
            &answer(&[1], None, None),
        )
        .unwrap()
        .unwrap();
        assert_eq!((next.start, next.count), (Some(8), Some(20)));
    }

    #[test]
    fn c8_6_numbered_pages_use_first_when_no_number_was_given() {
        let pager = by_page(Start, 3);
        assert_eq!(
            pager(list(), &answer(&[1], None, None))
                .unwrap()
                .unwrap()
                .start,
            Some(4)
        );
    }

    #[test]
    fn c8_6_numbered_pages_end_on_an_empty_page() {
        let pager = by_page(Start, 1);
        assert!(pager(list(), &answer(&[], None, None)).unwrap().is_none());
        assert!(pager(
            List {
                start: Some(u32::MAX),
                ..list()
            },
            &answer(&[], None, None)
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn c8_6_a_page_number_that_cannot_grow_is_a_mismatch() {
        let pager = by_page(Start, 1);
        let error = pager(
            List {
                start: Some(u32::MAX),
                ..list()
            },
            &answer(&[1], None, None),
        )
        .unwrap_err();
        assert_eq!(format!("{error:?}"), "Fail(mismatch)");
    }

    #[test]
    fn c8_6_a_pager_of_your_own_is_written_the_same_way() {
        // by page number: the next page is one further, until a page comes back empty
        fn by_page<C: Call, S: Slot<C, Value = u32>>(
            page: S,
        ) -> impl Fn(C, &Answer<C::Value>) -> Option<C>
        where
            C::Value: IntoItems,
        {
            move |call, answer| {
                if answer.value().count() == 0 {
                    return None;
                }
                let at = page.get(&call).copied().unwrap_or(1);
                Some(page.set(call, at + 1))
            }
        }
        let pager = by_page(Start);
        assert_eq!(
            pager(list(), &answer(&[1], None, None)).unwrap().start,
            Some(2)
        );
        assert!(pager(list(), &answer(&[], None, None)).is_none());
    }
}
