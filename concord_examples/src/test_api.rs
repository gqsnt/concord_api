use concord_core::prelude::*;
use concord_macros::api;
pub mod models {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Post {
        #[serde(rename = "userId")]
        pub user_id: u32,
        pub id: u32,
        pub title: String,
        pub body: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Comment {
        #[serde(rename = "postId")]
        pub post_id: u32,
        pub id: u32,
        pub name: String,
        pub email: String,
        pub body: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct User {
        pub id: u32,
        pub name: String,
        pub username: String,
        pub email: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct NewPost {
        pub title: String,
        pub body: String,
        #[serde(rename = "userId")]
        pub user_id: u32,
    }
}
api! {
    client Client {
      scheme: https,
      host: "typicode.com",
      params {
        user_agent: String="ClientApiExample/1.0".to_string(),
        client_trace: bool,
      }
      headers {
        "user-agent": user_agent,
        "x-client-trace": client_trace,
      }
    }

    prefix "jsonplaceholder" {
      path "posts" {

        GET GetPosts ""
          query { "userId" => user_id?: u32 }
          headers { "x-debug" => ["test:", {x_debug: bool = true}] }
          -> JsonEncoding<Vec<models::Post>>;

        GET GetPost {id: i32} -> JsonEncoding<models::Post>;
        GET GetPostComments {post_id: i32}/"comments" -> JsonEncoding<Vec<models::Comment>>;
        POST CreatePost "" body JsonEncoding<models::NewPost> -> JsonEncoding<models::Post>;
      }

      path "users" {
        GET GetUser {id: i32} -> JsonEncoding<models::User>;

        GET GetUserPosts {id: i32}/"posts"
          query { "userId" => user_id?: u32 }
          -> JsonEncoding<Vec<models::Post>> | Vec<String> => {
            IntoIterator::into_iter(r).map(|p| p.title).collect()
          };
      }
    }
}
