//! Six pagers, and what they combine with. Each returns a [`Paging`]: a pager, which
//! `paginate` takes as it is, with methods for what the page states.
//!
//! In a `paginate` statement, an input of the endpoint names its slot on the call
//! ([`Slot`]). So `paginate by_offset_limit(start, count);` hands two slots to
//! [`by_offset_limit`]. Nothing here is known to the macro.
//!
//! A [`Paging`] decides in one fixed order, whatever the order in which its methods are written:
//! the gate ([`only_when_set`](Paging::only_when_set)), what the page states
//! ([`has_more`](Paging::has_more), [`by_offset_total`]), the move (with
//! [`or_when_set`](Paging::or_when_set)), the short-page rule
//! ([`stop_on_short_page`](Paging::stop_on_short_page)) when nothing is stated, and the edits
//! ([`map_next`](Paging::map_next)):
//! `paginate by_cursor(after, |page| page.next.clone()).has_more(|page| page.has_more);`.

mod paging;
pub use paging::{by_offset_total, Paging};

use concord_core::error::{mismatch, Fail};
use concord_core::page::{Call, IntoItems, Slot};

/// The text of the Mismatch when the offset of the call cannot be read as a number of items.
const UNREADABLE: &str = "the offset of the call is not a number of items";
/// The text of the Mismatch when a valid offset has no successor that its type can hold.
const OVERFLOW: &str = "the offset cannot grow any further";

/// Why the offset of the next call could not be computed.
enum Stuck {
    /// The offset of the call is not a number of items (a negative one).
    Unreadable,
    /// A valid offset, whose successor its type cannot hold.
    Overflow,
}

impl Stuck {
    fn fail<P>(self) -> Fail<P> {
        mismatch(match self {
            Stuck::Unreadable => UNREADABLE,
            Stuck::Overflow => OVERFLOW,
        })
    }
}

fn advance<C: Call, S>(start: S, call: C, got: usize) -> Result<C, Stuck>
where
    S: Slot<C>,
    S::Value: Copy + TryFrom<u64> + TryInto<u64>,
{
    // An offset that is not set is zero.
    let at: u64 = match start.get(&call) {
        Some(at) => (*at).try_into().map_err(|_| Stuck::Unreadable)?,
        None => 0,
    };
    // counted in `u64`, with a check: an offset that cannot grow is an error, never a wrap
    // PANIC-SAFETY: usize is at most 64 bits on supported targets; offset addition is checked.
    let sum = at.checked_add(got as u64);
    let next = sum
        .and_then(|next| S::Value::try_from(next).ok())
        .ok_or(Stuck::Overflow)?;
    Ok(start.set(call, next))
}

/// `paginate by_offset(start);`: the next call starts after the items that arrived. The pages end
/// with an empty page.
pub fn by_offset<C, S>(start: S) -> Paging<C>
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + TryFrom<u64> + TryInto<u64>,
    C::Value: IntoItems,
{
    Paging::<C>::from_fn(move |call, answer| match answer.value().count() {
        0 => Ok(None),
        got => advance(start, call, got).map(Some).map_err(Stuck::fail),
    })
}

/// `paginate by_offset_limit(start, count);`: like [`by_offset`], and the pages also end with a
/// page that is shorter than the `count` that the caller set. Use this only for an API
/// that promises full pages until the last one.
///
/// The first request carries what the caller set and nothing more: with no `start` it has
/// no offset pair (it counts as 0), and with no `count` no limit pair. Each next request has
/// `start` plus the number of items that arrived, and the same `count`. When the caller set
/// no `count`, only an empty page ends the pages, as with [`by_offset`].
pub fn by_offset_limit<C, S, K>(start: S, limit: K) -> Paging<C>
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + TryFrom<u64> + TryInto<u64>,
    K: Slot<C>,
    K::Value: Copy + TryInto<usize>,
    C::Value: IntoItems,
{
    // The short page is a rule of the short-page stage, so that a statement of the page has
    // authority over it. The move still ends quietly when a valid offset could not grow past a
    // short page, which needs no next call.
    let short = move |call: &C, got: usize| {
        limit
            .get(call)
            .copied()
            .and_then(|asked| asked.try_into().ok())
            .is_some_and(|asked: usize| got < asked)
    };
    Paging::<C>::from_fn(move |call, answer| {
        let got = answer.value().count();
        if got == 0 {
            return Ok(None);
        }
        let ended = short(&call, got);
        // A valid offset whose successor cannot be held ends a short page quietly: no next call
        // is needed. An offset that cannot be read is a Mismatch, short page or not.
        match advance(start, call, got) {
            Ok(next) => Ok(Some(next)),
            Err(Stuck::Overflow) if ended => Ok(None),
            Err(stuck) => Err(stuck.fail()),
        }
    })
    .short_page_rule(move |call| {
        limit
            .get(call)
            .copied()
            .and_then(|asked| asked.try_into().ok())
    })
}

/// `paginate by_page(page, 1);`: the next call requests the following page number.
/// When no number was supplied, the page just received is `first`. An empty page ends
/// the sequence; a number that cannot be incremented is a Mismatch.
pub fn by_page<C, S>(page: S, first: S::Value) -> Paging<C>
where
    C: Call,
    S: Slot<C>,
    S::Value: Copy + TryFrom<u64> + TryInto<u64>,
    C::Value: IntoItems,
{
    let first = first.try_into().ok();
    Paging::<C>::from_fn(move |call, answer| {
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
    })
}

/// `paginate by_cursor(after, Page::next);`: the input takes what your function finds in the
/// page. The pages end when it finds nothing. The length of a page is never checked.
pub fn by_cursor<C, S, F>(input: S, next: F) -> Paging<C>
where
    C: Call,
    S: Slot<C>,
    F: Fn(&C::Value) -> Option<S::Value> + Send + 'static,
{
    Paging::<C>::from_fn(move |call, answer| {
        next(answer.value()).map(|value| input.set(call, value))
    })
}

/// `paginate by_next_url(Page::next);`: the same call, sent to the URL that your function reads
/// from the page. The pages end when it finds none.
pub fn by_next_url<C, F, U>(next: F) -> Paging<C>
where
    C: Call,
    F: Fn(&C::Value) -> Option<U> + Send + 'static,
    U: AsRef<str>,
{
    Paging::<C>::from_fn(move |call, answer| {
        next(answer.value()).map(|url| call.at_url(concord_core::page::fragmentless(url.as_ref())))
    })
}

/// `paginate by_link();`: the same call, sent to the URL of `Link: <…>; rel="next"` (RFC 8288).
/// The pages end when the response has no such link. A link with `anchor` is ignored.
/// A field with invalid text or malformed quotes is a Mismatch.
/// The target is parsed as `reqwest` parses a URL (using the `url` crate, like a browser),
/// so `</a|b>` and bad percent escapes are accepted. This pager checks the field's
/// brackets, parameters and quotes; the core checks every page's origin.
pub fn by_link<C: Call>() -> Paging<C> {
    Paging::<C>::from_fn(move |call, answer| {
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
    })
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
    use concord_core::page::Pager;
    use concord_core::Answer;
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
        let mut pager = by_offset(Start);
        let second = pager
            .next_call(list(), &answer(&[1, 2, 3], None, None))
            .unwrap()
            .unwrap();
        assert_eq!(second.start, Some(3));
        let third = pager
            .next_call(second, &answer(&[4], None, None))
            .unwrap()
            .unwrap();
        assert_eq!(third.start, Some(4)); // a short page is not the end
        assert!(pager
            .next_call(third, &answer(&[], None, None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn c8_6_an_offset_that_cannot_grow_is_an_error_and_not_a_wrap() {
        let mut pager = by_offset(Start);
        let at_the_end = List {
            start: Some(u32::MAX - 1),
            ..list()
        };
        assert!(pager
            .next_call(at_the_end, &answer(&[1, 2, 3], None, None))
            .is_err());
    }

    #[test]
    fn c8_6_by_offset_limit_also_ends_on_a_short_page() {
        let mut pager = by_offset_limit(Start, Count);
        let first = List {
            count: Some(3),
            ..list()
        };
        let second = pager
            .next_call(first, &answer(&[1, 2, 3], None, None))
            .unwrap()
            .unwrap();
        assert_eq!((second.start, second.count), (Some(3), Some(3))); // the other inputs are kept
        assert!(pager
            .next_call(second, &answer(&[4], None, None))
            .unwrap()
            .is_none());
        // with no count given by the caller, only an empty page ends
        assert!(pager
            .next_call(list(), &answer(&[1], None, None))
            .unwrap()
            .is_some());
    }

    #[test]
    fn c8_6_by_cursor_never_looks_at_the_length() {
        let mut pager = by_cursor(After, Page::next);
        let second = pager
            .next_call(list(), &answer(&[], Some("c2"), None))
            .unwrap()
            .unwrap(); // an empty page in the middle
        assert_eq!(second.after.as_deref(), Some("c2"));
        assert!(pager
            .next_call(second, &answer(&[7, 8], None, None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn c8_6_by_next_url_sends_the_same_call_elsewhere() {
        let mut pager = by_next_url(Page::next);
        let first = List {
            count: Some(50),
            ..list()
        };
        let second = pager
            .next_call(first, &answer(&[1], Some("/v2/items?page=2"), None))
            .unwrap()
            .unwrap();
        assert_eq!(
            (second.url.as_deref(), second.count),
            (Some("/v2/items?page=2"), Some(50))
        );
    }

    #[test]
    fn c8_6_by_link_follows_rel_next_only() {
        let mut pager = by_link();
        let github = r#"<https://api.github.com/repos?page=3>; rel="next", <https://api.github.com/repos?page=9>; rel="last""#;
        assert_eq!(
            pager
                .next_call(list(), &answer(&[1], None, Some(github)))
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("https://api.github.com/repos?page=3")
        );
        let last = r#"<https://api.github.com/repos?page=1>; rel="first", <https://api.github.com/repos?page=8>; rel="prev""#;
        assert!(pager
            .next_call(list(), &answer(&[1], None, Some(last)))
            .unwrap()
            .is_none());
        assert!(pager
            .next_call(list(), &answer(&[1], None, None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn c8_6_by_link_accepts_browser_url_targets() {
        for (target, field) in [
            ("/a|b", "</a|b>; rel=\"next\""),
            ("/bad%GG", "</bad%GG>; rel=\"next\""),
        ] {
            let next = by_link()
                .next_call(list(), &answer(&[1], None, Some(field)))
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
            by_link()
                .next_call(list(), &answer)
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("/items?page=3")
        );
    }

    #[test]
    fn c8_6_by_link_keeps_quoted_semicolons_and_commas_inside_their_parameter() {
        let mut pager = by_link();
        assert!(pager
            .next_call(
                list(),
                &answer(
                    &[1],
                    None,
                    Some(r#"</prev>; rel=prev; title="x; rel=next""#)
                )
            )
            .unwrap()
            .is_none());
        assert!(pager
            .next_call(
                list(),
                &answer(&[1], None, Some(r#"</prev>; title="x; rel=next""#))
            )
            .unwrap()
            .is_none());
        let next = pager
            .next_call(
                list(),
                &answer(&[1], None, Some(r#"</a>; rel="next"; title="a, b""#)),
            )
            .unwrap()
            .unwrap();
        assert_eq!(next.url.as_deref(), Some("/a"));
        let next = pager.next_call(list(), &answer(&[1], None,
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
                by_link()
                    .next_call(list(), &answer(&[1], None, Some(field)))
                    .is_err(),
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
        assert!(by_link().next_call(list(), &response).is_err());
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
            by_link()
                .next_call(list(), &response)
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
            assert!(by_link()
                .next_call(list(), &answer(&[1], None, Some(field)))
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
            by_link()
                .next_call(list(), &response)
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
            by_next_url(Page::next)
                .next_call(list(), &answer(&[1], Some("/a?x=1#part"), None))
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("/a?x=1")
        );
        assert_eq!(
            by_link()
                .next_call(list(), &answer(&[1], None, Some("</a?x=1#part>; rel=next")))
                .unwrap()
                .unwrap()
                .url
                .as_deref(),
            Some("/a?x=1")
        );
    }

    #[test]
    fn c8_6_numbered_pages_advance_the_number_that_was_answered() {
        let mut pager = by_page(Start, 1);
        let next = pager
            .next_call(
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
        let mut pager = by_page(Start, 3);
        assert_eq!(
            pager
                .next_call(list(), &answer(&[1], None, None))
                .unwrap()
                .unwrap()
                .start,
            Some(4)
        );
    }

    #[test]
    fn c8_6_numbered_pages_end_on_an_empty_page() {
        let mut pager = by_page(Start, 1);
        assert!(pager
            .next_call(list(), &answer(&[], None, None))
            .unwrap()
            .is_none());
        assert!(pager
            .next_call(
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
        let mut pager = by_page(Start, 1);
        let error = pager
            .next_call(
                List {
                    start: Some(u32::MAX),
                    ..list()
                },
                &answer(&[1], None, None),
            )
            .unwrap_err();
        assert_eq!(format!("{error:?}"), "Fail(mismatch)");
    }

    /// A short page ends a list only after the move: the move's failure is the result.
    fn short(count: u32) -> List {
        List {
            count: Some(count),
            ..list()
        }
    }

    #[test]
    fn c8_6_an_unreadable_link_is_a_mismatch_also_on_a_short_page() {
        let mut pager = by_link().stop_on_short_page(Count, 10);
        let page = answer(&[1], None, Some(r#"</a>; rel="next"#));
        assert_eq!(
            format!("{:?}", pager.next_call(short(5), &page).unwrap_err()),
            "Fail(mismatch)"
        );
    }

    #[test]
    fn c8_6_a_failing_pager_of_your_own_fails_also_on_a_short_page() {
        let mut pager = Paging::from_fn(|_: List, _: &Answer<Page>| {
            Err::<Option<List>, _>(mismatch("the pager of the author failed"))
        })
        .stop_on_short_page(Count, 10);
        assert!(pager
            .next_call(short(5), &answer(&[1], None, None))
            .is_err());
    }

    #[test]
    fn c8_6_two_inputs_that_choose_the_move_fail_also_on_a_short_page() {
        let mut pager = by_cursor(After, Page::next)
            .or_when_set(Start, by_offset(Start))
            .or_when_set(Count, by_page(Count, 1))
            .stop_on_short_page(Count, 10);
        let call = List {
            start: Some(0),
            ..short(5)
        };
        assert!(pager.next_call(call, &answer(&[1], None, None)).is_err());
    }

    #[test]
    fn c8_6_a_statement_has_authority_over_the_count_of_by_offset_limit() {
        let mut pager = by_offset_limit(Start, Count).has_more(|_| true);
        let next = pager
            .next_call(short(5), &answer(&[1, 2], None, None))
            .unwrap()
            .unwrap();
        assert_eq!((next.start, next.count), (Some(2), Some(5)));
        // without a statement, the short page ends the list, as before
        let mut bare = by_offset_limit(Start, Count);
        assert!(bare
            .next_call(short(5), &answer(&[1, 2], None, None))
            .unwrap()
            .is_none());
    }

    #[test]
    fn c8_6_every_statement_is_read_once_also_after_a_disagreement() {
        let read = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let third = read.clone();
        let mut pager = by_cursor(After, Page::next)
            .has_more(|_| true)
            .has_more(|_| false)
            .has_more(move |_| {
                third.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                true
            });
        let error = pager
            .next_call(list(), &answer(&[1], Some("c2"), None))
            .unwrap_err();
        assert_eq!(format!("{error:?}"), "Fail(mismatch)");
        assert_eq!(read.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// An offset whose type says that its default is seven.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Seven(u64);
    impl Default for Seven {
        fn default() -> Self {
            Seven(7)
        }
    }
    impl From<u64> for Seven {
        fn from(at: u64) -> Self {
            Seven(at)
        }
    }
    impl From<Seven> for u64 {
        fn from(at: Seven) -> u64 {
            at.0
        }
    }
    #[derive(Clone, Debug)]
    struct Positioned(Option<Seven>);
    impl Call for Positioned {
        type Value = Page;
        type Problem = ();
        fn at_url(self, _: &str) -> Positioned {
            self
        }
    }
    #[derive(Clone, Copy)]
    struct At;
    impl Slot<Positioned> for At {
        type Value = Seven;
        fn get<'a>(&self, call: &'a Positioned) -> Option<&'a Seven> {
            call.0.as_ref()
        }
        fn set(&self, _: Positioned, value: Seven) -> Positioned {
            Positioned(Some(value))
        }
    }
    fn page(items: &[u32]) -> Answer<Page> {
        Answer::new(
            StatusCode::OK,
            Headers::from(HeaderMap::new()),
            Page {
                items: items.to_vec(),
                next: None,
            },
        )
    }

    #[test]
    fn c8_6_an_offset_that_is_not_set_is_zero_whatever_its_default() {
        let next = by_offset(At)
            .next_call(Positioned(None), &page(&[1, 2]))
            .unwrap()
            .unwrap();
        assert_eq!(next.0, Some(Seven(2)));
        // one of three received: the total is not reached, and the walk goes on from 1
        let next = by_offset_total(At, |_: &Page| 3u32)
            .next_call(Positioned(None), &page(&[1]))
            .unwrap()
            .unwrap();
        assert_eq!(next.0, Some(Seven(1)));
    }

    #[test]
    fn c8_6_what_each_shipped_pager_returns_is_send_and_sync() {
        fn send_and_sync<T: Send + Sync>(_: &T) {}
        send_and_sync(&by_offset(Start));
        send_and_sync(&by_offset_limit(Start, Count));
        send_and_sync(&by_page(Start, 1));
        send_and_sync(&by_cursor(After, Page::next));
        send_and_sync(&by_next_url::<List, _, _>(Page::next));
        send_and_sync(&by_link::<List>());
        send_and_sync(&by_offset_total(Start, |_: &Page| 3u32));
        // also when a function it holds is not `Sync`
        let seen = std::cell::Cell::new(0);
        let counted = by_cursor(After, move |page: &Page| {
            seen.set(seen.get() + 1);
            page.next()
        })
        .has_more(|_| true)
        .stop_on_short_page_of(10);
        send_and_sync(&counted);
    }

    #[test]
    fn c8_6_a_fixed_page_size_ends_a_list_that_states_nothing() {
        let mut pager = by_cursor(After, Page::next).stop_on_short_page_of(3);
        assert!(pager
            .next_call(list(), &answer(&[1, 2], Some("c2"), None))
            .unwrap()
            .is_none());
        let next = pager
            .next_call(list(), &answer(&[1, 2, 3], Some("c2"), None))
            .unwrap()
            .unwrap();
        assert_eq!(next.after.as_deref(), Some("c2"));
    }

    /// A page whose `count` records each read.
    struct Seen {
        items: Vec<u32>,
        reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl IntoItems for Seen {
        type Item = u32;
        fn into_items(self) -> Vec<u32> {
            self.items
        }
        fn count(&self) -> usize {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.items.len()
        }
    }
    #[derive(Clone)]
    struct Watched;
    impl Call for Watched {
        type Value = Seen;
        type Problem = ();
        fn at_url(self, _: &str) -> Watched {
            self
        }
    }

    #[test]
    fn c8_6_the_short_page_rule_is_read_after_the_move() {
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let page = Answer::new(
            StatusCode::OK,
            Headers::from(HeaderMap::new()),
            Seen {
                items: vec![1],
                reads: reads.clone(),
            },
        );
        // the move continues only if no rule has read the page before it
        let mut pager = Paging::from_fn(|call: Watched, answer: &Answer<Seen>| {
            (answer
                .value()
                .reads
                .load(std::sync::atomic::Ordering::SeqCst)
                == 0)
                .then_some(call)
        })
        .stop_on_short_page_of(1);
        assert!(pager.next_call(Watched, &page).unwrap().is_some());
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// A call whose data the move takes as the only owner.
    struct Owned {
        unique: std::sync::Arc<usize>,
        clones: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl Clone for Owned {
        fn clone(&self) -> Self {
            self.clones
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Owned {
                unique: self.unique.clone(),
                clones: self.clones.clone(),
            }
        }
    }
    impl Call for Owned {
        type Value = Vec<u8>;
        type Problem = ();
        fn at_url(self, _: &str) -> Owned {
            self
        }
    }

    #[test]
    fn c8_6_the_short_page_rule_takes_nothing_from_the_call_but_the_size() {
        let clones = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let call = Owned {
            unique: std::sync::Arc::new(1),
            clones: clones.clone(),
        };
        let page = Answer::new(StatusCode::OK, Headers::from(HeaderMap::new()), vec![1u8]);
        // the move needs to be the only owner of the call's data
        let mut pager = Paging::from_fn(|mut call: Owned, _: &Answer<Vec<u8>>| {
            *std::sync::Arc::get_mut(&mut call.unique)? += 1;
            Some(call)
        })
        .stop_on_short_page_of(1);
        assert!(pager.next_call(call, &page).unwrap().is_some());
        assert_eq!(clones.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn c8_6_an_outer_no_more_skips_the_statements_of_a_converted_pager() {
        let read = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let inner = read.clone();
        let converted = by_cursor(After, Page::next)
            .has_more(move |_| {
                inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .into_fn();
        let mut pager = Paging::from_fn(converted).has_more(|_| false);
        assert!(pager
            .next_call(list(), &answer(&[1], Some("c2"), None))
            .unwrap()
            .is_none());
        assert_eq!(read.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn c8_6_an_outer_short_page_rule_ends_a_list_that_a_converted_pager_says_goes_on() {
        let converted = by_cursor(After, Page::next).has_more(|_| true).into_fn();
        let mut pager = Paging::from_fn(converted).stop_on_short_page_of(10);
        assert!(pager
            .next_call(list(), &answer(&[1], Some("c2"), None))
            .unwrap()
            .is_none());
        // the same pager, not converted: its statement has authority over the rule
        let mut direct = by_cursor(After, Page::next)
            .has_more(|_| true)
            .stop_on_short_page_of(10);
        assert!(direct
            .next_call(list(), &answer(&[1], Some("c2"), None))
            .unwrap()
            .is_some());
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
