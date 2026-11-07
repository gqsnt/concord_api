//! Caller inputs with provider rules: property dot-paths, response-field lists, property filters
//! and typed country lookup values. Every constructor is synchronous and sends nothing; a
//! refused value never creates a request.

use std::borrow::Cow;
use std::fmt;

use concord::{QueryPairs, QueryPairsWriter};

/// A refused caller input. Messages are static and never repeat the refused value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputError {
    /// A property path is empty, has an empty segment, or uses a character outside
    /// `a-z`, `0-9` and `_` (segments are joined by `.`, as in `memberships.eu`).
    Property,
    /// A response-field list is empty: omit `response_fields` to receive every field.
    EmptyFields,
    /// A filter names a query parameter the provider reserves (`q`, `limit`, `offset`,
    /// `response_fields`, `response_fields_omit`, `pretty`) instead of a country property.
    ReservedFilter,
    /// A country code is not two or three ASCII letters (ISO 3166-1 alpha-2 / alpha-3).
    CountryCode,
    /// A region is not a non-empty, unpadded, plain-text lookup value.
    Region,
    /// A subregion is not a non-empty, unpadded, plain-text lookup value.
    Subregion,
    /// A language is not a non-empty, unpadded, plain-text lookup value.
    Language,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Property => "a property path must be dot-separated segments of a-z, 0-9 and _",
            Self::EmptyFields => "a response-field list needs at least one property",
            Self::ReservedFilter => "a filter cannot use a reserved query parameter name",
            Self::CountryCode => "a country code must be two or three ASCII letters",
            Self::Region => "a region must be a non-empty, unpadded, plain-text lookup value",
            Self::Subregion => "a subregion must be a non-empty, unpadded, plain-text lookup value",
            Self::Language => "a language must be a non-empty, unpadded, plain-text lookup value",
        })
    }
}

impl std::error::Error for InputError {}

/// A country property as the provider names it: a dot-path such as `region`, `names.common` or
/// `memberships.eu` (Countries API field reference).
///
/// It is used three ways: as the `{property}` path segment of a read or search by property, as a
/// filter key on the list endpoint ([`Filters`]), and as an entry of a response-field allowlist
/// or blocklist ([`Fields`]). The constants are documented paths; [`Property::new`] accepts any
/// other path of the same grammar. Whether a property is *readable* (path lookup) or
/// *searchable* (filter, `?q=`) is the provider's decision: a property that is not answers
/// `404`, a premium property on a free plan `403`, both as [`Error::Provider`](crate::Error).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Property(Cow<'static, str>);

macro_rules! properties {
    ($($(#[$doc:meta])* $name:ident = $path:literal;)*) => {
        impl Property {
            $($(#[$doc])* pub const $name: Property = Property(Cow::Borrowed($path));)*

            /// Every documented constant, in declaration order (used by the crate's tests).
            pub const DOCUMENTED: &'static [Property] = &[$(Self::$name),*];
        }
    };
}

properties! {
    /// `names`: the whole names branch (common, official, alternates, native, translations).
    NAMES = "names";
    /// `names.common`: the common name (part of the `name` aggregate).
    NAMES_COMMON = "names.common";
    /// `names.official`: the official name (part of the `name` aggregate).
    NAMES_OFFICIAL = "names.official";
    /// `names.alternates`: alternate names (part of the `name` aggregate).
    NAMES_ALTERNATES = "names.alternates";
    /// `names.native`: native names keyed by ISO 639-3 code (part of the `name` aggregate).
    NAMES_NATIVE = "names.native";
    /// `names.translations`: translations keyed by ISO 639-3 code (not in the `name` aggregate).
    NAMES_TRANSLATIONS = "names.translations";
    /// `codes`: the whole codes branch.
    CODES = "codes";
    /// `codes.alpha_2`: ISO 3166-1 alpha-2.
    CODES_ALPHA_2 = "codes.alpha_2";
    /// `codes.alpha_3`: ISO 3166-1 alpha-3.
    CODES_ALPHA_3 = "codes.alpha_3";
    /// `codes.ccn3`: ISO 3166-1 numeric.
    CODES_CCN3 = "codes.ccn3";
    /// `flag.emoji` (not readable, not searchable).
    FLAG_EMOJI = "flag.emoji";
    /// `flag.url_svg` (not readable, not searchable).
    FLAG_URL_SVG = "flag.url_svg";
    /// `capitals`: capital objects (readable and searchable by name).
    CAPITALS = "capitals";
    /// `region` (e.g. `Europe`).
    REGION = "region";
    /// `subregion` (e.g. `Northern Europe`).
    SUBREGION = "subregion";
    /// `continents`.
    CONTINENTS = "continents";
    /// `landlocked` (boolean).
    LANDLOCKED = "landlocked";
    /// `borders`: alpha-3 codes of land neighbours (reverse lookup: `borders/CAN`).
    BORDERS = "borders";
    /// `population` (dynamic; not readable, not searchable).
    POPULATION = "population";
    /// `currencies`.
    CURRENCIES = "currencies";
    /// `languages`.
    LANGUAGES = "languages";
    /// `memberships`: the whole memberships branch.
    MEMBERSHIPS = "memberships";
    /// `memberships.eu` (boolean).
    MEMBERSHIPS_EU = "memberships.eu";
    /// `memberships.un` (boolean).
    MEMBERSHIPS_UN = "memberships.un";
    /// `memberships.nato` (boolean).
    MEMBERSHIPS_NATO = "memberships.nato";
    /// `leaders` (premium: a notice in place on free plans, `403` as a filter).
    LEADERS = "leaders";
}

impl Property {
    /// A property path: one or more segments of `a-z`, `0-9` and `_`, joined by `.`.
    ///
    /// # Errors
    ///
    /// [`InputError::Property`] for any other text (empty, `names..common`, `Region`,
    /// `region=x`, a space).
    pub fn new(path: impl Into<String>) -> Result<Self, InputError> {
        let path = path.into();
        let valid = path.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        if valid {
            Ok(Self(Cow::Owned(path)))
        } else {
            Err(InputError::Property)
        }
    }

    /// The dot-path as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Property {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A non-empty list of properties for `response_fields` (allowlist) or `response_fields_omit`
/// (blocklist), sent as one comma-separated value. Order and duplicates are the caller's; the
/// provider documents that ordering does not matter.
///
/// With an allowlist, each returned object carries only those properties, so every field of
/// [`Country`](crate::Country) that was not selected is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields(Vec<Property>);

impl Fields {
    /// The listed properties.
    ///
    /// # Errors
    ///
    /// [`InputError::EmptyFields`] for an empty list (omit the parameter instead).
    pub fn new(properties: impl IntoIterator<Item = Property>) -> Result<Self, InputError> {
        let properties: Vec<Property> = properties.into_iter().collect();
        if properties.is_empty() {
            return Err(InputError::EmptyFields);
        }
        Ok(Self(properties))
    }

    /// The properties, in the order given.
    pub fn properties(&self) -> &[Property] {
        &self.0
    }
}

impl fmt::Display for Fields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, property) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            f.write_str(property.as_str())?;
        }
        Ok(())
    }
}

/// Query parameters a filter may not use: the list endpoint's own parameters.
const RESERVED: &[&str] = &[
    "q",
    "limit",
    "offset",
    "response_fields",
    "response_fields_omit",
    "pretty",
];

/// Property filters for the list endpoint (`GET /countries/v5?region=Europe&memberships.eu=1`):
/// each one a `{property}={value}` query pair, all combined with AND by the provider, which
/// documents them as substring filters on searchable properties.
///
/// Values are sent as given (the provider's examples spell a boolean filter `1`). Setting a
/// property twice keeps one pair with the latest value, at the first position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters(Vec<(Property, String)>);

impl Filters {
    /// No filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) the filter on `property`.
    ///
    /// # Errors
    ///
    /// [`InputError::ReservedFilter`] when `property` is one of the endpoint's own parameters
    /// (`q`, `limit`, `offset`, `response_fields`, `response_fields_omit`, `pretty`).
    pub fn with(
        mut self,
        property: Property,
        value: impl Into<String>,
    ) -> Result<Self, InputError> {
        if RESERVED.contains(&property.as_str()) {
            return Err(InputError::ReservedFilter);
        }
        let value = value.into();
        match self.0.iter_mut().find(|(p, _)| *p == property) {
            Some(existing) => existing.1 = value,
            None => self.0.push((property, value)),
        }
        Ok(self)
    }

    /// The filters, in wire order.
    pub fn pairs(&self) -> &[(Property, String)] {
        &self.0
    }
}

impl QueryPairs for Filters {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        for (property, value) in &self.0 {
            out.pair(property.as_str(), value);
        }
    }
}

/// Which ISO 3166-1 code a [`CountryCode`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodeKind {
    /// Two letters, read through `codes.alpha_2/{code}`.
    Alpha2,
    /// Three letters, read through `codes.alpha_3/{code}`.
    Alpha3,
}

/// An ISO 3166-1 alpha-2 or alpha-3 code, uppercased, for
/// [`RestCountries::country`](crate::RestCountries::country).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CountryCode {
    kind: CodeKind,
    code: String,
}

impl CountryCode {
    /// Two ASCII letters are alpha-2, three are alpha-3 (case-insensitive; stored uppercase,
    /// the provider's matching is case-insensitive).
    ///
    /// # Errors
    ///
    /// [`InputError::CountryCode`] for anything else.
    pub fn parse(code: &str) -> Result<Self, InputError> {
        let kind = match code.len() {
            2 => CodeKind::Alpha2,
            3 => CodeKind::Alpha3,
            _ => return Err(InputError::CountryCode),
        };
        if !code.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err(InputError::CountryCode);
        }
        Ok(Self {
            kind,
            code: code.to_ascii_uppercase(),
        })
    }

    /// Alpha-2 or alpha-3.
    pub fn kind(&self) -> CodeKind {
        self.kind
    }

    /// The uppercase code.
    pub fn as_str(&self) -> &str {
        &self.code
    }

    /// The property the code is read through: `codes.alpha_2` or `codes.alpha_3`.
    pub fn property(&self) -> Property {
        match self.kind {
            CodeKind::Alpha2 => Property::CODES_ALPHA_2,
            CodeKind::Alpha3 => Property::CODES_ALPHA_3,
        }
    }
}

impl fmt::Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)
    }
}

// This is SDK input hygiene, not a closed provider vocabulary. In particular, an unknown
// well-formed value must reach the provider and may produce an empty page. URL syntax is
// refused so callers pass plain text (spaces included), never pre-encoded path fragments.
fn valid_lookup_value(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && !matches!(value, "." | "..")
        && !value.chars().any(|c| {
            c.is_control()
                || (c.is_whitespace() && c != ' ')
                || matches!(c, '/' | '\\' | '?' | '#' | '%' | '+')
        })
}

macro_rules! lookup_values {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            ///
            /// Values retain spelling and case; the provider matches case-insensitively.
            /// No local vocabulary is assumed: a well-formed unknown value can yield no match.
            #[derive(Clone, Debug, PartialEq, Eq, Hash)]
            pub struct $name(String);

            impl $name {
                /// Validate plain text before constructing a lookup.
                ///
                /// Internal spaces, Unicode text, and language identifier hyphens are accepted.
                /// This SDK refuses empty text, surrounding whitespace, controls, whitespace
                /// other than ASCII spaces, `.` / `..`, and URL syntax (`/`, `\\`, `?`, `#`,
                /// `%`, `+`). Pass spaces as spaces, not `+` or `%20`.
                ///
                /// # Errors
                ///
                /// The corresponding [`InputError`] variant for refused text; sends nothing.
                pub fn parse(value: &str) -> Result<Self, InputError> {
                    if !valid_lookup_value(value) {
                        return Err(InputError::$name);
                    }
                    Ok(Self(value.to_owned()))
                }

                /// The original plain-text value, before URL encoding.
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str(&self.0)
                }
            }
        )*
    };
}

lookup_values! {
    /// A geographic `region` value, such as `Europe`, for
    /// [`RestCountries::region`](crate::RestCountries::region).
    Region;
    /// A geographic `subregion` value, such as `Northern Europe`, for
    /// [`RestCountries::subregion`](crate::RestCountries::subregion).
    Subregion;
    /// A `languages` lookup value for [`RestCountries::language`](crate::RestCountries::language).
    /// The documented array contains ISO identifiers, BCP 47 tags, English and native names;
    /// which values match remains the provider's decision.
    Language;
}
