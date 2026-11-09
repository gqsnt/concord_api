//! Private wire declaration; the public client enforces credential and continuation policy.
use concord::Json;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};

use crate::model::{DirectoryObject, Envelope, Group, UserDelta};

concord::api! {
    pub client GraphApi(origin: concord::BaseUrl, #[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base origin;
        error crate::ErrorEnvelope in concord::IgnoreContentType<concord::Json>;
        format Json;

        GET users_delta(query("$select") select?: String) -> Envelope<UserDelta> {
            path "users/delta";
            page next url next;
        }
        GET groups(query("$select") select?: String) -> Envelope<Group> {
            path "groups";
            page next url next;
        }
        GET direct_members(
            group: String,
            query("$select") select?: String,
            query("$count") count?: bool,
            header("consistencylevel") consistency?: String,
        ) -> Envelope<DirectoryObject> {
            path "groups/{group}/members";
            page next url next;
        }
    }
}
