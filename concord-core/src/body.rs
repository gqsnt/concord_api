//! The body of a request, as a value.

mod depth;

use futures_util::StreamExt;
use std::{
    fmt,
    pin::Pin,
    sync::{Arc, Mutex},
};

/// A chunk stream used by body writers.
pub type BodyStream = Pin<
    Box<
        dyn futures_core::Stream<
                Item = Result<bytes::Bytes, Box<dyn std::error::Error + Send + Sync>>,
            > + Send,
    >,
>;

#[derive(Clone)]
enum Data {
    Bytes(bytes::Bytes),
    Stream(Arc<Mutex<Option<BodyStream>>>),
    Concat(Vec<Body>),
    Map(
        Box<Body>,
        Arc<dyn Fn(BodyStream) -> BodyStream + Send + Sync>,
    ),
}

/// The bytes of a request body, with the label that goes with them.
///
/// Bytes can be sent again. A stream is sent once.
/// Make a body from bytes with [`Body::new(bytes, "application/octet-stream")`](Body::new),
/// or from a stream of byte chunks with [`Body::stream(stream)`](Body::stream).
/// Use [`content_type`](Body::content_type) to label a stream.
///
/// A clone of a one-shot stream shares it: sending through any clone uses up every clone,
/// and a later send of one of them is a Build error.
#[derive(Clone)]
pub struct Body {
    content_type: Option<std::borrow::Cow<'static, str>>,
    data: Data,
}

impl Body {
    /// These bytes, with a label: what a writer of yours gives.
    ///
    /// ```
    /// # use concord_core as concord;
    /// fn csv(rows: &[(u32, &str)]) -> concord::Body {
    ///     let text: String = rows.iter().map(|(id, name)| format!("{id},{name}\n")).collect();
    ///     concord::Body::new(text, "text/csv")
    /// }
    /// ```
    pub fn new(
        bytes: impl Into<bytes::Bytes>,
        content_type: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Body {
        Body {
            content_type: Some(content_type.into()),
            data: Data::Bytes(bytes.into()),
        }
    }

    /// A body that is sent once, as it comes.
    pub fn stream<S, E>(stream: S) -> Body
    where
        S: futures_core::Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Body {
            content_type: None,
            data: Data::Stream(Arc::new(Mutex::new(Some(Box::pin(
                Box::pin(stream).map(|chunk| chunk.map_err(Into::into)),
            ))))),
        }
    }

    /// Join bodies in order. If all parts are in memory, the result holds bytes.
    /// Otherwise all one-shot parts are claimed together by [`into_stream`](Self::into_stream),
    /// before any bytes can be read. Set the combined label with [`content_type`](Self::content_type).
    pub fn concat(parts: impl IntoIterator<Item = Body>) -> Body {
        let parts: Vec<_> = parts.into_iter().collect();
        if parts.iter().all(|part| part.as_bytes().is_some()) {
            let bytes: Vec<u8> = parts
                .iter()
                .filter_map(Body::as_bytes)
                .flatten()
                .copied()
                .collect();
            bytes.into()
        } else {
            Body {
                content_type: None,
                data: Data::Concat(parts),
            }
        }
    }

    /// Bytes held in memory, or `None` if any part is a stream.
    /// Inspecting a body never claims or polls any of its streams.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match &self.data {
            Data::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Compatibility name for generated code.
    #[doc(hidden)]
    pub fn __bytes(&self) -> Option<&[u8]> {
        self.as_bytes()
    }

    /// The content type held by this writer.
    #[doc(hidden)]
    pub fn __content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Take the body as a chunk stream. Clones share each one-shot source.
    ///
    /// Returns an error before yielding any byte if a source was already taken,
    /// or occurs twice in the body. On failure no remaining source is taken.
    pub fn into_stream(self) -> Result<BodyStream, std::io::Error> {
        fn sources(body: &Body, out: &mut Vec<Arc<Mutex<Option<BodyStream>>>>) {
            match &body.data {
                Data::Bytes(_) => (),
                Data::Stream(source) => out.push(source.clone()),
                Data::Concat(parts) => parts.iter().for_each(|part| sources(part, out)),
                Data::Map(body, _) => sources(body, out),
            }
        }
        fn prepared(
            body: Body,
            claimed: &mut std::collections::HashMap<usize, BodyStream>,
        ) -> Result<BodyStream, std::io::Error> {
            match body.data {
                Data::Bytes(bytes) => Ok(Box::pin(futures_util::stream::once(async { Ok(bytes) }))),
                Data::Stream(source) => claimed
                    // PANIC-SAFETY: This thin Arc pointer fits usize; it is an identity key, never dereferenced after casting.
                    .remove(&(Arc::as_ptr(&source) as usize))
                    .ok_or_else(|| std::io::Error::other("the body stream was already used")),
                Data::Concat(parts) => {
                    let streams: Vec<_> = parts
                        .into_iter()
                        .map(|part| prepared(part, claimed))
                        .collect::<Result<_, _>>()?;
                    Ok(Box::pin(futures_util::stream::iter(streams).flatten()))
                }
                Data::Map(body, map) => Ok(map(prepared(*body, claimed)?)),
            }
        }
        let mut sources_to_claim = Vec::new();
        sources(&self, &mut sources_to_claim);
        // Stable lock ordering also makes overlapping compositions safe to take concurrently.
        sources_to_claim.sort_by_key(Arc::as_ptr);
        let reused = || std::io::Error::other("the body stream was already used");
        if sources_to_claim
            // PANIC-SAFETY: The window size is the nonzero constant two.
            .windows(2)
            .any(|pair| matches!(pair, [left, right] if Arc::ptr_eq(left, right)))
        {
            return Err(reused());
        }
        let mut guards = sources_to_claim
            .iter()
            .map(|source| {
                source
                    .lock()
                    .map_err(|_| std::io::Error::other("the body stream lock was poisoned"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if guards.iter().any(|guard| guard.is_none()) {
            return Err(reused());
        }
        let mut claimed = sources_to_claim
            .iter()
            .zip(&mut guards)
            .map(|(source, guard)| {
                guard
                    .take()
                    // PANIC-SAFETY: This thin Arc pointer fits usize; it is an identity key, never dereferenced after casting.
                    .map(|stream| (Arc::as_ptr(source) as usize, stream))
                    .ok_or_else(reused)
            })
            .collect::<Result<_, _>>()?;
        drop(guards);
        prepared(self, &mut claimed)
    }

    /// Wrap the chunk stream after all of this body's sources have been claimed.
    /// Body writers can validate chunks without hiding their one-shot sources from
    /// composition. The wrapper is never called during inspection or a failed claim.
    /// The result is one-shot even when the source holds only bytes.
    #[doc(hidden)]
    pub fn map_stream(
        mut self,
        map: impl Fn(BodyStream) -> BodyStream + Send + Sync + 'static,
    ) -> Body {
        // Give buffered input a shared claim too. All other variants already
        // contain one-shot sources, which the map keeps visible to composition.
        if let Data::Bytes(bytes) = &self.data {
            self.data = Body::stream(futures_util::stream::iter([Ok::<_, std::io::Error>(
                bytes.clone(),
            )]))
            .data;
        }
        Body {
            content_type: self.content_type.clone(),
            data: Data::Map(Box::new(self), Arc::new(map)),
        }
    }

    /// Compatibility name for generated code.
    #[doc(hidden)]
    pub fn __into_stream(self) -> Result<BodyStream, std::io::Error> {
        self.into_stream()
    }

    /// Says what the bytes are. A `content-type` header set by the client's author wins over it.
    pub fn content_type(mut self, value: impl Into<std::borrow::Cow<'static, str>>) -> Body {
        self.content_type = Some(value.into());
        self
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            Data::Bytes(bytes) => f
                .debug_struct("Body::Bytes")
                .field("len", &bytes.len())
                .finish(),
            _ => f.write_str("Body::Stream"),
        }
    }
}

impl From<Vec<u8>> for Body {
    fn from(bytes: Vec<u8>) -> Body {
        Body {
            content_type: None,
            data: Data::Bytes(bytes.into()),
        }
    }
}

impl From<String> for Body {
    fn from(text: String) -> Body {
        Body {
            content_type: None,
            data: Data::Bytes(text.into()),
        }
    }
}

impl From<&'static str> for Body {
    fn from(text: &'static str) -> Body {
        Body {
            content_type: None,
            data: Data::Bytes(text.into()),
        }
    }
}

impl From<bytes::Bytes> for Body {
    fn from(bytes: bytes::Bytes) -> Body {
        Body {
            content_type: None,
            data: Data::Bytes(bytes),
        }
    }
}

impl From<&Body> for Body {
    /// Clone the body, sharing any one-shot sources with the original.
    fn from(body: &Body) -> Body {
        body.clone()
    }
}

/// What the expression of a `body` statement may give.
///
/// A [`Body`], a `&Body`, or a `Result` of one of these with any error: an `Err` is a Build
/// error, nothing is sent, and the error of your writer is the `source()` of that error.
/// A way to write a body is a function that gives a [`Body`]: `json` and `form` of
/// `concord` are two, and one of yours is used in the same way.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a body",
    note = "a body is what json(x), form(x) or a function of yours gives: a concord::Body"
)]
pub trait IntoBody: sealed::Sealed {
    /// The body, or why there is none.
    fn into_body(self) -> Result<Body, Box<dyn std::error::Error + Send + Sync>>;
}

mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for Body {}
impl IntoBody for Body {
    fn into_body(self) -> Result<Body, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self)
    }
}

impl sealed::Sealed for &Body {}
impl IntoBody for &Body {
    fn into_body(self) -> Result<Body, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.clone())
    }
}

impl<B: IntoBody, E: Into<Box<dyn std::error::Error + Send + Sync>>> sealed::Sealed
    for Result<B, E>
{
}
impl<B: IntoBody, E: Into<Box<dyn std::error::Error + Send + Sync>>> IntoBody for Result<B, E> {
    fn into_body(self) -> Result<Body, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Ok(body) => body.into_body(),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_debug_shows_only_the_kind_and_length() {
        let body =
            Body::new("private-body", "private/content-type").content_type("another/private-type");
        assert_eq!(format!("{body:?}"), "Body::Bytes { len: 12 }");
        assert_eq!(format!("{:?}", body.clone()), "Body::Bytes { len: 12 }");
        assert_eq!(
            format!("{:?}", Body::new("", "private/type")),
            "Body::Bytes { len: 0 }"
        );
    }

    #[test]
    fn stream_debug_shows_only_the_kind() {
        let body = Body::stream(futures_util::stream::empty::<
            Result<bytes::Bytes, std::io::Error>,
        >())
        .content_type("private/content-type");
        assert_eq!(format!("{body:?}"), "Body::Stream");
        assert_eq!(format!("{body:#?}"), "Body::Stream");
    }

    fn source(text: &'static str) -> Body {
        Body::stream(futures_util::stream::iter([Ok::<_, std::io::Error>(
            bytes::Bytes::from_static(text.as_bytes()),
        )]))
    }
    fn collect(mut stream: BodyStream) -> Vec<u8> {
        let mut cx = std::task::Context::from_waker(futures_util::task::noop_waker_ref());
        let mut out = Vec::new();
        loop {
            match stream.poll_next_unpin(&mut cx) {
                std::task::Poll::Ready(Some(chunk)) => out.extend_from_slice(&chunk.unwrap()),
                std::task::Poll::Ready(None) => return out,
                std::task::Poll::Pending => panic!("in-memory test source must be ready"),
            }
        }
    }
    #[test]
    fn from_body_reference_clones_bytes_and_shares_stream_claims() {
        let buffered = Body::new("photo", "image/jpeg");
        let copy = Body::from(&buffered);
        assert_eq!(copy.as_bytes(), buffered.as_bytes());
        assert_eq!(copy.__content_type(), Some("image/jpeg"));
        assert_eq!(collect(copy.into_stream().unwrap()), b"photo");
        assert_eq!(collect(buffered.into_stream().unwrap()), b"photo");

        for take_reference in [false, true] {
            let original = source("photo").content_type("image/jpeg");
            let copy: Body = (&original).into();
            let clone = copy.clone();
            assert_eq!(copy.__content_type(), Some("image/jpeg"));
            let (first, second) = if take_reference {
                (copy, original)
            } else {
                (original, copy)
            };
            assert_eq!(collect(first.into_stream().unwrap()), b"photo");
            assert!(second.into_stream().is_err());
            assert!(clone.into_stream().is_err());
        }
    }

    #[test]
    fn mapped_bodies_share_one_shot_claims_with_clones_and_stream_sources() {
        let buffered = Body::new("bytes", "text/plain").map_stream(|stream| stream);
        assert!(buffered.as_bytes().is_none());
        assert_eq!(buffered.__content_type(), Some("text/plain"));
        let copy = buffered.clone();
        assert_eq!(collect(copy.into_stream().unwrap()), b"bytes");
        assert!(buffered.into_stream().is_err());

        for take_mapped in [false, true] {
            let source = source("stream");
            let mapped = source.clone().map_stream(|stream| stream);
            let copy = mapped.clone();
            let (first, second) = if take_mapped {
                (mapped, source)
            } else {
                (source, mapped)
            };
            assert_eq!(collect(first.into_stream().unwrap()), b"stream");
            assert!(second.into_stream().is_err());
            assert!(copy.into_stream().is_err());
        }
    }

    #[test]
    fn concat_buffered_bodies_in_order_and_set_combined_label() {
        let body = Body::concat([
            Body::from("first"),
            Body::concat([Body::from("second"), Body::from("third")]),
        ])
        .content_type("combined/type");
        assert_eq!(body.as_bytes(), Some(b"firstsecondthird".as_slice()));
        assert_eq!(body.__content_type(), Some("combined/type"));
        assert_eq!(collect(body.into_stream().unwrap()), b"firstsecondthird");
        assert_eq!(Body::concat([]).as_bytes(), Some(b"".as_slice()));
    }
    #[test]
    fn concat_streams_are_claimed_together_before_the_first_byte() {
        let first = source("one");
        let second = source("two");
        let body = Body::concat([
            Body::from("prefix"),
            first.clone(),
            second.clone(),
            Body::from("suffix"),
        ]);
        // Inspection refuses the composed stream without claiming either part.
        assert!(body.as_bytes().is_none());
        assert!(body.clone().as_bytes().is_none());
        let stream = body.into_stream().unwrap();
        assert!(first.clone().into_stream().is_err());
        assert!(second.clone().into_stream().is_err());
        assert_eq!(collect(stream), b"prefixonetwosuffix");
        let rebuilt = Body::concat([Body::from("prefix"), first, second]);
        assert!(rebuilt.into_stream().is_err());
    }
    #[test]
    fn failed_concat_claim_does_not_touch_a_part_or_run_its_wrapper() {
        let used = source("used");
        drop(used.clone().into_stream().unwrap());
        let fresh = source("fresh");
        let wrapped = fresh
            .clone()
            .map_stream(|_| panic!("a failed claim cannot run wrappers"));
        let body = Body::concat([Body::from("prefix"), Body::concat([wrapped, used])]);
        assert!(body.as_bytes().is_none());
        assert!(body.into_stream().is_err());
        assert_eq!(collect(fresh.into_stream().unwrap()), b"fresh");
    }
    #[test]
    fn duplicate_source_in_a_composition_fails_without_claiming_it() {
        let body = source("only once");
        assert!(Body::concat([body.clone(), body.clone()])
            .into_stream()
            .is_err());
        assert_eq!(collect(body.into_stream().unwrap()), b"only once");
    }
}

/// An ordered list of body entries, serialized as a map by any Serde writer.
///
/// A top-level Serde `None` is omitted, including through references. `Some(None)`
/// is kept: JSON writes `null`, while a form omits that pair. Generated code adapts
/// every value first, allowing a `Secret` entry without making `Secret` serializable.
/// Presence is checked when the body is written, in that writer's
/// `is_human_readable` mode, without iterating, measuring or formatting collections.
/// Each value's `Serialize` is called once to check for a top-level `None`, then
/// again to write it if retained. Its outer Serde operation must stay the same
/// across both calls; changing it through side effects can defeat `None` omission.
/// The unit (`()`) is not a `None`: the format writes it (`null` in JSON).
///
/// ```compile_fail
/// # use concord_core as concord;
/// let mut entries = concord::body::Entries::new();
/// entries.__push("token", concord::Secret::new("key"));
/// ```
#[derive(Default)]
pub struct Entries<'a> {
    entries: Vec<(&'static str, Box<dyn StoredEntry + 'a>)>,
}

// Probe the concrete value: erased-serde's adapter implements collect_seq/map/str
// itself, so probing through that adapter could iterate or format before Probe
// sees the operation. Erasure is used only for the real writer.
trait StoredEntry: erased_serde::Serialize {
    fn is_none(&self, human_readable: bool) -> bool;
}
impl<T: serde::Serialize> StoredEntry for T {
    fn is_none(&self, human_readable: bool) -> bool {
        self.serialize(none_probe::Probe {
            human_readable,
            depth: 0,
        })
        .unwrap_or(false)
    }
}
erased_serde::serialize_trait_object!(StoredEntry);
impl fmt::Debug for Entries<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entries")
            .field("len", &self.entries.len())
            .finish()
    }
}
impl<'a> Entries<'a> {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }
    /// Add an adapted value. A top-level `None` is omitted when the body is written.
    #[doc(hidden)]
    pub fn __push<T: EntryValue>(&mut self, key: &'static str, value: T)
    where
        T::Serialized: 'a,
    {
        self.entries.push((key, Box::new(value.__body_value())));
    }
    /// Compatibility name for the same top-level None rule as [`Self::__push`].
    #[doc(hidden)]
    pub fn __optional<T: serde::Serialize + 'a>(&mut self, key: &'static str, value: Option<T>) {
        self.__push(key, value);
    }
}

/// Convert an adapted placement to its body representation.
#[doc(hidden)]
pub trait EntryValue {
    /// The serializable representation, confined to this placement.
    type Serialized: serde::Serialize;
    /// Adapt the placement without making its underlying input serializable.
    fn __body_value(self) -> Self::Serialized;
}
impl<T: serde::Serialize> EntryValue for T {
    type Serialized = T;
    fn __body_value(self) -> T {
        self
    }
}
impl serde::Serialize for Entries<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EntryMap(self).serialize(serializer)
    }
}
struct EntryMap<'a, 'b>(&'a Entries<'b>);
impl serde::Serialize for EntryMap<'_, '_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let human_readable = serializer.is_human_readable();
        // Probe once per entry, before giving the writer the exact map length.
        let present: Vec<_> = self
            .0
            .entries
            .iter()
            .filter(|(_, value)| !value.as_ref().is_none(human_readable))
            .collect();
        let mut map = serializer.serialize_map(Some(present.len()))?;
        for (key, value) in present {
            depth::with_input(key, || {
                map.serialize_entry(key, &depth::Value(value, 0, None, Some(key)))
            })?;
        }
        map.end()
    }
}

// This serializer observes only the outer Serde operation. In particular it never
// descends into `Some`, sequences or structs, and never formats a value.
mod none_probe {
    use serde::ser::{self, Impossible};
    use std::fmt;
    pub struct Probe {
        pub human_readable: bool,
        pub depth: u8,
    }
    #[derive(Debug)]
    pub struct NotNone;
    impl fmt::Display for NotNone {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("not a top-level none")
        }
    }
    impl std::error::Error for NotNone {}
    impl ser::Error for NotNone {
        fn custom<T: fmt::Display>(_: T) -> Self {
            Self
        }
    }
    macro_rules! scalar {
        ($($method:ident($ty:ty);)*) => {$(
            fn $method(self, _: $ty) -> Result<bool, NotNone> { Ok(false) }
        )*};
    }
    impl ser::Serializer for Probe {
        type Ok = bool;
        type Error = NotNone;
        type SerializeSeq = Impossible<bool, NotNone>;
        type SerializeTuple = Impossible<bool, NotNone>;
        type SerializeTupleStruct = Impossible<bool, NotNone>;
        type SerializeTupleVariant = Impossible<bool, NotNone>;
        type SerializeMap = Impossible<bool, NotNone>;
        type SerializeStruct = Impossible<bool, NotNone>;
        type SerializeStructVariant = Impossible<bool, NotNone>;
        scalar! {
            serialize_bool(bool); serialize_i8(i8); serialize_i16(i16); serialize_i32(i32);
            serialize_i64(i64); serialize_i128(i128); serialize_u8(u8); serialize_u16(u16);
            serialize_u32(u32); serialize_u64(u64); serialize_u128(u128);
            serialize_f32(f32); serialize_f64(f64); serialize_char(char); serialize_str(&str);
            serialize_bytes(&[u8]);
        }
        fn collect_str<T: ?Sized + fmt::Display>(self, _: &T) -> Result<bool, NotNone> {
            Ok(false)
        }
        fn collect_seq<I>(self, _: I) -> Result<bool, NotNone>
        where
            I: IntoIterator,
            I::Item: serde::Serialize,
        {
            Ok(false)
        }
        fn collect_map<K, V, I>(self, _: I) -> Result<bool, NotNone>
        where
            K: serde::Serialize,
            V: serde::Serialize,
            I: IntoIterator<Item = (K, V)>,
        {
            Ok(false)
        }
        fn is_human_readable(&self) -> bool {
            self.human_readable
        }
        fn serialize_none(self) -> Result<bool, NotNone> {
            Ok(true)
        }
        fn serialize_some<T: ?Sized + serde::Serialize>(self, _: &T) -> Result<bool, NotNone> {
            Ok(false)
        }
        fn serialize_unit(self) -> Result<bool, NotNone> {
            Ok(false)
        }
        fn serialize_unit_struct(self, _: &'static str) -> Result<bool, NotNone> {
            Ok(false)
        }
        fn serialize_unit_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
        ) -> Result<bool, NotNone> {
            Ok(false)
        }
        fn serialize_newtype_struct<T: ?Sized + serde::Serialize>(
            self,
            _: &'static str,
            value: &T,
        ) -> Result<bool, NotNone> {
            if self.depth == 128 {
                return Err(NotNone);
            }
            value.serialize(Self {
                // PANIC-SAFETY: This serializer rejects depth 128 before incrementing its private counter.
                depth: self.depth + 1,
                ..self
            })
        }
        fn serialize_newtype_variant<T: ?Sized + serde::Serialize>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: &T,
        ) -> Result<bool, NotNone> {
            Ok(false)
        }
        fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, NotNone> {
            Err(NotNone)
        }
        fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, NotNone> {
            Err(NotNone)
        }
        fn serialize_tuple_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleStruct, NotNone> {
            Err(NotNone)
        }
        fn serialize_tuple_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleVariant, NotNone> {
            Err(NotNone)
        }
        fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, NotNone> {
            Err(NotNone)
        }
        fn serialize_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStruct, NotNone> {
            Err(NotNone)
        }
        fn serialize_struct_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStructVariant, NotNone> {
            Err(NotNone)
        }
    }
}

#[cfg(test)]
mod presence_probe_tests {
    use super::none_probe::Probe;
    use serde::{Serialize, Serializer};
    use std::{
        fmt,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    struct Inner(Arc<AtomicUsize>);
    impl Serialize for Inner {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0.fetch_add(1, Ordering::SeqCst);
            s.serialize_none()
        }
    }
    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fetch_add(1, Ordering::SeqCst);
            f.write_str("formatted")
        }
    }
    struct First {
        op: &'static str,
        inner: Inner,
    }
    impl Serialize for First {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::{
                SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
                SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
            };
            match self.op {
                "bool" => s.serialize_bool(true),
                "i8" => s.serialize_i8(-1),
                "i16" => s.serialize_i16(-1),
                "i32" => s.serialize_i32(-1),
                "i64" => s.serialize_i64(-1),
                "i128" => s.serialize_i128(-1),
                "u8" => s.serialize_u8(1),
                "u16" => s.serialize_u16(1),
                "u32" => s.serialize_u32(1),
                "u64" => s.serialize_u64(1),
                "u128" => s.serialize_u128(1),
                "f32" => s.serialize_f32(1.0),
                "f64" => s.serialize_f64(1.0),
                "char" => s.serialize_char('x'),
                "str" => s.serialize_str("x"),
                "bytes" => s.serialize_bytes(b"x"),
                "none" => s.serialize_none(),
                "some" => s.serialize_some(&self.inner),
                "unit" => s.serialize_unit(),
                "unit_struct" => s.serialize_unit_struct("Unit"),
                "unit_variant" => s.serialize_unit_variant("Enum", 0, "Unit"),
                "newtype_struct" => s.serialize_newtype_struct("Newtype", &self.inner),
                "newtype_variant" => s.serialize_newtype_variant("Enum", 0, "Newtype", &self.inner),
                "seq" => {
                    let mut c = s.serialize_seq(Some(1))?;
                    c.serialize_element(&self.inner)?;
                    c.end()
                }
                "tuple" => {
                    let mut c = s.serialize_tuple(1)?;
                    c.serialize_element(&self.inner)?;
                    c.end()
                }
                "tuple_struct" => {
                    let mut c = s.serialize_tuple_struct("Tuple", 1)?;
                    c.serialize_field(&self.inner)?;
                    c.end()
                }
                "tuple_variant" => {
                    let mut c = s.serialize_tuple_variant("Enum", 0, "Tuple", 1)?;
                    c.serialize_field(&self.inner)?;
                    c.end()
                }
                "map" => {
                    let mut c = s.serialize_map(Some(1))?;
                    c.serialize_entry("x", &self.inner)?;
                    c.end()
                }
                "struct" => {
                    let mut c = s.serialize_struct("Struct", 1)?;
                    c.serialize_field("x", &self.inner)?;
                    c.end()
                }
                "struct_variant" => {
                    let mut c = s.serialize_struct_variant("Enum", 0, "Struct", 1)?;
                    c.serialize_field("x", &self.inner)?;
                    c.end()
                }
                "collect_str" => s.collect_str(&self.inner),
                "is_human_readable" => {
                    let human = s.is_human_readable();
                    s.serialize_bool(human)
                }
                "custom_error" => Err(serde::ser::Error::custom(&self.inner)),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn all_other_serializer_entry_methods_do_not_format_or_descend_into_collections() {
        for op in [
            "bool",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "f32",
            "f64",
            "char",
            "str",
            "bytes",
            "none",
            "some",
            "unit",
            "unit_struct",
            "unit_variant",
            "newtype_struct",
            "newtype_variant",
            "seq",
            "tuple",
            "tuple_struct",
            "tuple_variant",
            "map",
            "struct",
            "struct_variant",
            "collect_str",
            "is_human_readable",
            "custom_error",
        ] {
            let calls = Arc::new(AtomicUsize::new(0));
            let value = First {
                op,
                inner: Inner(calls.clone()),
            };
            let result = value.serialize(Probe {
                human_readable: true,
                depth: 0,
            });
            // Newtypes deliberately unwrap to preserve the documented None-through-newtype behavior.
            assert_eq!(
                calls.load(Ordering::SeqCst),
                usize::from(op == "newtype_struct"),
                "{op}"
            );
            assert_eq!(
                result.unwrap_or(false),
                op == "none" || op == "newtype_struct",
                "{op}"
            );
        }
    }
}

/// A body writer's failure, with fixed text and the original format error as its source.
///
/// [`Bounded::error`] constructs it after serialization fails, retaining any input
/// name supplied by [`Entries`]. `Display` and `Debug` never show the serialized value.
/// A call uses the retained name for its Build error and exposes the format error
/// directly through `source()`, so the writer's fixed sentence is not repeated.
/// JSON, form and writers supplied by an author all use this same error type.
///
/// ```
/// # use concord_core as concord;
/// use concord::body::{Bounded, WriteError};
/// use std::error::Error;
/// let value = Bounded::new(&42u8);
/// let written: Result<(), std::io::Error> = Err(std::io::Error::other("format refused the value"));
/// let error: WriteError = written.map_err(|source| value.error(source)).unwrap_err();
/// assert_eq!(error.to_string(), "the body could not be written");
/// assert_eq!(error.source().unwrap().to_string(), "format refused the value");
/// ```
pub struct WriteError {
    input: Option<String>,
    source: Box<dyn std::error::Error + Send + Sync>,
}
impl WriteError {
    pub(crate) fn depth_input(&self) -> Option<&str> {
        self.input.as_deref()
    }
    pub(crate) fn into_source(self) -> Box<dyn std::error::Error + Send + Sync> {
        self.source
    }
}
impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the body could not be written")
    }
}
impl fmt::Debug for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WriteError { .. }")
    }
}
impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.source)
    }
}

/// A bounded serialization view for body writers, retaining named entry failures.
///
/// Wrap a value before passing it to a Serde serializer, then map that format's
/// error with [`Bounded::error`]. Each body entry may nest at most 128 levels.
/// Input names come directly from [`Entries`]; ordinary map keys and Serde type
/// names never become diagnostic input names. `Entries` keeps its map shape through this view.
///
/// ```
/// # use concord_core as concord;
/// use concord::body::{Bounded, WriteError};
/// use serde::{Serialize, Serializer};
/// fn write<T: Serialize, S: Serializer>(value: &T, serializer: S)
///     -> Result<S::Ok, WriteError>
/// where S::Error: std::error::Error + Send + Sync + 'static {
///     let value = Bounded::new(value);
///     value.serialize(serializer).map_err(|error| value.error(error))
/// }
/// ```
pub struct Bounded<'a, T: ?Sized> {
    value: &'a T,
    context: depth::Context,
}
impl<T: ?Sized> fmt::Debug for Bounded<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Bounded { .. }")
    }
}
impl<'a, T: serde::Serialize + ?Sized> Bounded<'a, T> {
    /// Bound each input to at most 128 nested Serde operations.
    pub fn new(value: &'a T) -> Self {
        Self {
            value,
            context: depth::Context::default(),
        }
    }
    /// Consume this view, transferring its recorded input name and the format's error.
    /// The input name belongs to the returned error and cannot be taken a second time.
    ///
    /// ```compile_fail
    /// # use concord_core as concord;
    /// use concord::body::Bounded;
    /// let value = Bounded::new(&1u8);
    /// let first = value.error(std::io::Error::other("format failed"));
    /// let second = value.error(std::io::Error::other("failed again"));
    /// ```
    pub fn error(self, source: impl std::error::Error + Send + Sync + 'static) -> WriteError {
        WriteError {
            input: self.context.input.into_inner(),
            source: Box::new(source),
        }
    }
}
impl<T: serde::Serialize + ?Sized> serde::Serialize for Bounded<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        depth::Value(self.value, 0, Some(&self.context), None).serialize(serializer)
    }
}

#[cfg(test)]
mod map_shape_tests {
    use super::*;
    use serde::ser::{self, Serialize, SerializeMap, Serializer};
    struct MapOnly;
    struct Map(usize);
    type E = serde::de::value::Error;
    fn no<T>() -> Result<T, E> {
        Err(ser::Error::custom("expected a plain map"))
    }
    macro_rules! scalar {
        ($($method:ident($ty:ty)),* $(,)?) => { $(
            fn $method(self, _: $ty) -> Result<usize, E> { no() }
        )* };
    }
    impl Serializer for MapOnly {
        type Ok = usize;
        type Error = E;
        type SerializeSeq = ser::Impossible<usize, E>;
        type SerializeTuple = ser::Impossible<usize, E>;
        type SerializeTupleStruct = ser::Impossible<usize, E>;
        type SerializeTupleVariant = ser::Impossible<usize, E>;
        type SerializeMap = Map;
        type SerializeStruct = ser::Impossible<usize, E>;
        type SerializeStructVariant = ser::Impossible<usize, E>;
        scalar!(
            serialize_bool(bool),
            serialize_i8(i8),
            serialize_i16(i16),
            serialize_i32(i32),
            serialize_i64(i64),
            serialize_u8(u8),
            serialize_u16(u16),
            serialize_u32(u32),
            serialize_u64(u64),
            serialize_f32(f32),
            serialize_f64(f64),
            serialize_char(char),
            serialize_str(&str),
            serialize_bytes(&[u8])
        );
        fn serialize_none(self) -> Result<usize, E> {
            no()
        }
        fn serialize_unit(self) -> Result<usize, E> {
            no()
        }
        fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<usize, E> {
            no()
        }
        fn serialize_unit_struct(self, _: &'static str) -> Result<usize, E> {
            no()
        }
        fn serialize_unit_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
        ) -> Result<usize, E> {
            no()
        }
        fn serialize_newtype_struct<T: Serialize + ?Sized>(
            self,
            _: &'static str,
            _: &T,
        ) -> Result<usize, E> {
            no()
        }
        fn serialize_newtype_variant<T: Serialize + ?Sized>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: &T,
        ) -> Result<usize, E> {
            no()
        }
        fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, E> {
            no()
        }
        fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, E> {
            no()
        }
        fn serialize_tuple_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleStruct, E> {
            no()
        }
        fn serialize_tuple_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleVariant, E> {
            no()
        }
        fn serialize_map(self, len: Option<usize>) -> Result<Map, E> {
            assert_eq!(len, Some(2));
            Ok(Map(0))
        }
        fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, E> {
            no()
        }
        fn serialize_struct_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStructVariant, E> {
            no()
        }
    }
    impl SerializeMap for Map {
        type Ok = usize;
        type Error = E;
        fn serialize_key<T: Serialize + ?Sized>(&mut self, _: &T) -> Result<(), E> {
            self.0 += 1;
            Ok(())
        }
        fn serialize_value<T: Serialize + ?Sized>(&mut self, _: &T) -> Result<(), E> {
            Ok(())
        }
        fn end(self) -> Result<usize, E> {
            Ok(self.0)
        }
    }
    #[test]
    fn c2_14_every_writer_sees_a_plain_map_even_through_bounded() {
        let mut entries = Entries::new();
        entries.__push("a", 1u32);
        entries.__push("b", "x");
        assert_eq!(entries.serialize(MapOnly).unwrap(), 2);
        assert_eq!(Bounded::new(&entries).serialize(MapOnly).unwrap(), 2);
    }
}

#[cfg(test)]
mod chain_tests {
    use super::*;
    use std::error::Error as _;
    #[test]
    fn c6_2_a_build_error_exposes_the_format_cause_once() {
        let writer = Bounded::new(&1u8).error(std::io::Error::other("format rejected the value"));
        let error: crate::Error = crate::Error::build_source("body.post", Box::new(writer));
        let mut messages = Vec::new();
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        while let Some(error) = cause {
            messages.push(error.to_string());
            cause = error.source();
        }
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert_eq!(messages[1], "format rejected the value");
        assert!(error.source().unwrap().is::<std::io::Error>());
        assert_eq!(
            messages
                .iter()
                .filter(|m| m.contains("the body could not be written"))
                .count(),
            1
        );
    }
}
