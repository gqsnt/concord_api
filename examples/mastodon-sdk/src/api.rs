//! The declaration. The generated client [`Api`] is public, but its constructor and
//! `with_base_url` are this crate's (`controls`): `with_base_url` keeps every client value, the
//! access token included, so a caller allowed to call it could point a token at another host.
//! [`Mastodon`](crate::Mastodon) creates it, binds it to one instance and hands out the generated
//! endpoint values, whose setters, `prepare()` and `send()` are the ordinary ones.

use concord::Json;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};

use crate::error::{Error, MastodonPolicy};
use crate::model::{Hashtag, InstanceInfo, Status, StatusId, TimelineLimit};
use crate::streaming::{PublicStreamFormat, StreamEvent};

concord::api! {
    /// One Mastodon instance's REST API, or its streaming host (the same declaration, another
    /// base). Every `send()` returns `Result<_, Error>`.
    ///
    /// Bound to one instance: create it with [`Mastodon::new`](crate::Mastodon::new) and reach
    /// it with [`Mastodon::api`](crate::Mastodon::api). Only this crate creates it and chooses
    /// its host (the instance, or the streaming host that instance advertises), so its token is
    /// sent nowhere else.
    pub client Api(
        /// `Authorization: Bearer <token>`, set by [`Mastodon::with_token`](crate::Mastodon::with_token);
        /// absent for anonymous calls.
        #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
    ) {
        // A placeholder: the instance is chosen at run time and `Mastodon::new` always replaces
        // it (`.invalid` is reserved and never resolves).
        base "https://mastodon.invalid";
        send MastodonPolicy -> Result<_, Error>;
        format Json;
        // Every finite document below (a timeline page of at most 40 statuses, one status, the
        // instance document) is read into memory: at most 8 MiB.
        limit 8 * 1024 * 1024;
        // The token is bound to the instance: only this crate creates the client and moves it.
        controls { pub(crate) new; pub(crate) with_base_url; }

        /// Selectors and paging shared by public and hashtag timelines. Unset values are not
        /// sent: server defaults apply (false flags, 20 statuses).
        params TimelineOptions(
            /// `local`: only statuses of this instance.
            query local?: bool,
            /// `remote`: only statuses of other instances.
            query remote?: bool,
            /// `only_media`: only statuses with media attached.
            query only_media?: bool,
            /// `max_id`: statuses older than this one (an upper bound).
            query max_id?: StatusId,
            /// `since_id`: statuses newer than this one (a lower bound).
            query since_id?: StatusId,
            /// `min_id`: the statuses immediately newer than this one (forward paging).
            query min_id?: StatusId,
            /// `limit`: at most this many statuses (1 to 40; the server's default is 20).
            query limit?: TimelineLimit,
        );

        /// The public (federated) timeline: `GET /api/v1/timelines/public`, newest first.
        /// An empty page can also mean that the instance disabled this feed.
        GET public_timeline(..TimelineOptions) -> Vec<Status> {
            path "api/v1/timelines/public";
        }

        /// Public statuses containing `hashtag`, encoded as one path segment. The instance
        /// controls access and the number of additional tags applied (default total: four).
        GET hashtag_timeline(
            hashtag: Hashtag,
            /// `any[]`: match any of these additional tags (one query pair per tag).
            query("any[]") each any?: Vec<Hashtag>,
            /// `all[]`: match all of these additional tags.
            query("all[]") each all?: Vec<Hashtag>,
            /// `none[]`: exclude these additional tags.
            query("none[]") each none?: Vec<Hashtag>,
            ..TimelineOptions
        ) -> Vec<Status> {
            path "api/v1/timelines/tag/{hashtag}";
        }

        /// One status: `GET /api/v1/statuses/{id}`.
        GET status(id: StatusId) -> Status {
            path "api/v1/statuses/{id}";
        }

        /// The instance document: `GET /api/v2/instance`. Only what this slice needs is decoded
        /// (`configuration.urls.streaming`).
        GET instance() -> InstanceInfo {
            path "api/v2/instance";
        }

        /// The public (federated) timeline as server-sent events:
        /// `GET /api/v1/streaming/public` on the streaming host.
        GET public_stream(
            /// `only_media`: only statuses with media attachments.
            query only_media?: bool,
        ) -> stream StreamEvent {
            path "api/v1/streaming/public";
            response PublicStreamFormat;
        }

        /// The `hashtag` category as HTTP SSE (user token + `read:statuses` required).
        GET hashtag_stream(query tag: Hashtag) -> stream StreamEvent {
            path "api/v1/streaming/hashtag";
            response PublicStreamFormat;
        }

        /// The `hashtag:local` category as HTTP SSE: only statuses originating here.
        GET local_hashtag_stream(query tag: Hashtag) -> stream StreamEvent {
            path "api/v1/streaming/hashtag/local";
            response PublicStreamFormat;
        }
    }
}
