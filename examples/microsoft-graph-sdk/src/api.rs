//! Private wire declaration; the public client enforces credential and continuation policy.
use concord::Json;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};

use crate::model::{DirectoryObject, Envelope, Group, UserDelta};

concord::api! {
    pub client GraphApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://graph.microsoft.com/v1.0";
        format Json;

        GET users_delta(query("$select") select?: String) -> Envelope<UserDelta> {
            path "users/delta";
        }
        GET groups(query("$select") select?: String) -> Envelope<Group> {
            path "groups";
        }
        GET direct_members(
            group: String,
            query("$select") select?: String,
            query("$count") count?: bool,
            header("consistencylevel") consistency?: String,
        ) -> Envelope<DirectoryObject> {
            path "groups/{group}/members";
        }
    }
}
