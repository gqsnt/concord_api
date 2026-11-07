//! The declaration. The generated client stays private to the crate: its `with_base_url` keeps
//! every client value, the access token included, so a caller holding it could point a token at
//! another host. [`Mastodon`](crate::Mastodon) holds it, binds it to one instance and hands out
//! the generated endpoint values, whose setters, `prepare()` and `send()` are the ordinary ones.

use concord::Json;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};

use crate::error::{Error, MastodonPolicy};
use crate::model::{InstanceInfo, Status, StatusId, TimelineLimit};
use crate::streaming::{PublicStreamFormat, StreamEvent};

concord::api! {
    /// One Mastodon instance's REST API, or its streaming host (the same declaration, another
    /// base). Every `send()` returns `Result<_, Error>`.
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

        /// The public (federated) timeline: `GET /api/v1/timelines/public`, newest first. The
        /// flags and the id bounds are sent only when set; the server's defaults apply otherwise
        /// (`false` for the flags, 20 statuses). An empty page can also mean that the instance
        /// disabled this feed.
        GET public_timeline(
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
        ) -> Vec<Status> {
            path "api/v1/timelines/public";
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
    }
}
