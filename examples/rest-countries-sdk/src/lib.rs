#![doc = include_str!("../README.md")]

mod error;
mod input;
mod model;

use std::num::NonZeroU32;

use concord::reqwest::header::{AUTHORIZATION, HeaderValue};
use concord::{Endpoint, Json, SendWith, Sends};
use concord_pagination::{Page, Pages, Resume};

pub use concord;
pub use concord_auth::CredentialError;
pub use concord_pagination;
pub use error::{
    ContractError, Error, ErrorBodyError, ErrorDocument, ErrorEntry, ProviderClass, ProviderError,
    ResponseMeta,
};
pub use input::{
    CodeKind, CountryCode, Fields, Filters, InputError, Language, Property, Region, Subregion,
};
pub use model::{Capital, Codes, Country, CountryPage, Flag, NameForms, Names, PageMeta, Premium};

/// The largest success body read before decoding (16 MiB): a paid plan's largest page (500
/// complete records) fits; beyond it `send()` fails with `ErrorKind::Limit` without decoding.
pub const RESPONSE_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// The largest error document read for a non-2xx status (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// The `Authorization: Bearer <key>` value for [`RestCountriesConfig`], flagged sensitive.
///
/// The provider prefers this header over its `?api-key=` query fallback (meant for browser
/// experiments), which this crate does not send.
///
/// # Errors
///
/// [`CredentialError::Token`] when the key is empty or contains whitespace, a control
/// character or non-ASCII bytes (`concord_auth::bearer`). Nothing is sent.
pub fn bearer(api_key: &str) -> Result<HeaderValue, CredentialError> {
    concord_auth::bearer(api_key)
}

concord::api! {
    /// The REST Countries Countries API, version 5 (read-only, JSON).
    ///
    /// Create it with [`bearer`]:
    /// `RestCountries::new(http, RestCountriesConfig { authorization: bearer(key)? })`.
    /// Transport settings (timeouts, proxy, redirects) belong to the `reqwest::Client`.
    /// `with_base_url(origin)` replaces the origin; the `countries/v5` prefix is kept.
    ///
    /// Every `send()` performs one request and returns `Result<`[`CountryPage`]`, `[`Error`]`>`:
    /// a non-2xx status is [`Error::Provider`] with its bounded error document. Nothing is
    /// retried, paginated or re-authenticated by `send()`; [`pages`] walks offsets explicitly.
    pub client RestCountries(
        /// `Authorization: Bearer <key>`, built with [`bearer`].
        #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
    ) {
        base "https://api.restcountries.com";
        path "countries/v5";
        response Json; // every answer is JSON; no endpoint has a body
        limit RESPONSE_BODY_LIMIT;
        send SendPolicy -> Result<_, Error>;

        /// The options every endpoint accepts: the response shape and the page window.
        params ListOptions(
            /// `response_fields`: return only these properties.
            query response_fields?: Fields,
            /// `response_fields_omit`: drop these properties (applied after `response_fields`).
            query response_fields_omit?: Fields,
            /// `limit`: page size; the provider allows 1–100 on a free plan, up to 500 on a paid
            /// plan, and defaults to 25.
            query limit?: NonZeroU32,
            /// `offset`: records to skip (default 0).
            query offset?: u64,
        );

        /// `GET /countries/v5`: every country, narrowed by a free-text `q` (substring of any
        /// searchable property) and by property filters, all combined with AND.
        GET countries(query q?: String, query(..) filters?: Filters, ..ListOptions) -> CountryPage;

        /// `GET /countries/v5/name?q=`: substring search over `names.common`, `names.official`,
        /// `names.alternates` and `names.native` (not translations). An empty `q` is a `400`.
        GET search_names(query q: String, ..ListOptions) -> CountryPage { path "name"; }

        /// `GET /countries/v5/code?q=`: substring search over every `codes.*` variant.
        GET search_codes(query q: String, ..ListOptions) -> CountryPage { path "code"; }

        /// One property, `/countries/v5/{property}`.
        scope property(property: Property) {
            path "{property}";

            /// `GET /countries/v5/{property}/{value}`: records whose property equals `value`
            /// (case-insensitive, in full); for an array property, records containing it.
            /// `q` narrows the matched set further.
            GET read(value: String, query q?: String, ..ListOptions) -> CountryPage {
                path "{value}";
            }

            /// `GET /countries/v5/{property}?q=`: substring search within one property.
            GET search(query q: String, ..ListOptions) -> CountryPage;
        }
    }
}

impl RestCountries {
    /// `GET /countries/v5/codes.alpha_2/{code}` or `codes.alpha_3/{code}`: the country with
    /// this ISO code, as a page; [`CountryPage::single`] gives the record or `None`.
    pub fn country(&self, code: &CountryCode) -> RestCountriesPropertyRead {
        self.property(code.property()).read(code.as_str())
    }

    /// `GET /countries/v5/region/{region}`: exact, case-insensitive region lookup.
    /// Accepts the same [`ListOptions`] and [`pages`] walk as [`Self::countries`].
    /// No match is an empty [`CountryPage`]. Validate the value with [`Region::parse`].
    pub fn region(&self, region: &Region) -> RestCountriesPropertyRead {
        self.property(Property::REGION).read(region.as_str())
    }

    /// `GET /countries/v5/subregion/{subregion}`: exact, case-insensitive subregion lookup.
    /// Accepts the same [`ListOptions`] and [`pages`] walk as [`Self::countries`].
    /// No match is an empty [`CountryPage`]. Validate the value with [`Subregion::parse`].
    pub fn subregion(&self, subregion: &Subregion) -> RestCountriesPropertyRead {
        self.property(Property::SUBREGION).read(subregion.as_str())
    }

    /// `GET /countries/v5/languages/{language}`: reverse lookup on the languages array.
    /// Accepts the same [`ListOptions`] and [`pages`] walk as [`Self::countries`].
    /// No match is an empty [`CountryPage`]. Validate the value with [`Language::parse`].
    pub fn language(&self, language: &Language) -> RestCountriesPropertyRead {
        self.property(Property::LANGUAGES).read(language.as_str())
    }
}

/// The send policy of [`RestCountries`]: what every generated `send()` does. Callers never
/// name it.
///
/// One request. A 2xx is decoded as a [`CountryPage`] (an empty page included); any other
/// status is [`Error::Provider`] with the error document read up to [`ERROR_BODY_LIMIT`].
#[derive(Debug, Default)]
pub struct SendPolicy;

impl SendWith<CountryPage> for SendPolicy {
    type Output = Result<CountryPage, Error>;

    async fn send<E: Endpoint<CountryPage> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            // The status is authoritative even when the document is absent or unreadable.
            let (meta, document) = received
                .document::<Json, ErrorDocument>(ERROR_BODY_LIMIT)
                .await;
            let document = document.map_err(ErrorBodyError::from);
            return Err(Error::Provider(Box::new(ProviderError { meta, document })));
        }
        Ok(received.decode().await?)
    }
}

/// Every record of a listing, page by page from `first` (any endpoint of this client with its
/// filters, fields and `limit` set), following `meta.offset + count` while `meta.more` holds.
///
/// One request per page, none repeated: a failed page ends the walk with its [`Error`] in
/// [`concord_pagination::PagesError::Provider`]; a contradictory page ends it with
/// [`Error::Contract`]. An empty first page (no match) is the complete, empty result after one
/// request. Cap the walk with `max_pages` (none by default).
pub fn pages<E>(first: E) -> Pages<Country, u64, (), Error>
where
    E: Sends<Result<CountryPage, Error>> + ListOptions + Clone + Send + 'static,
{
    Pages::new(Resume::Start, move |offset: Option<u64>| {
        let request = match offset {
            None => first.clone(),
            Some(offset) => first.clone().set_offset(offset),
        };
        async move {
            let page = request.send().await?;
            let next = page.next_offset()?;
            Ok(Page::new(page.countries, next))
        }
    })
}
