use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct User;
#[derive(Debug, Serialize, Deserialize)]
pub struct NewPost;
#[derive(Debug, Serialize, Deserialize)]
pub struct Post;
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse;
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest;
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse { access_token: String }

api! {
    client V5EndpointStanzaApi {
        base https "example.com"
        secret upstream_key: String
        credential upstream = api_key(secret.upstream_key)
    }

    GET Me
        as me
        path ["me"]
        -> Json<User>

    POST CreatePost(body: Json<NewPost>)
        as create
        path ["posts"]
        -> Json<Post>

    GET Search(q: String, page?: u32)
        as search
        path ["search"]
        query {
            q
            page
        }
        -> Json<SearchResponse>

    POST LoginForSession(body: Json<LoginRequest>)
        as login_for_session
        path ["login"]
        auth header "X-Upstream-Key" = upstream
        -> Json<LoginResponse>
        map AccessToken {
            AccessToken::new(r.access_token)
        }
}

fn main() {}
