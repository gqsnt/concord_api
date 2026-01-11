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
        headers {
            "user-agent" as user_agent: String = "ClientApiExample/1.0".to_string(),
            "x-client-trace" as client_trace: bool
        }
    }

    prefix "jsonplaceholder" {
        path "posts" {
            GET GetPosts ""
                query { "userId" as user_id?: u32 }
                headers {
                    x_debug: bool = true,
                    "x-debug" = format!("test:{}", ep.x_debug)
                }
                -> Json<Vec<models::Post>>;

            GET GetPost {id:i32} -> Json<models::Post>;

            GET GetPostComments {post_id:i32}/"comments" -> Json<Vec<models::Comment>>;

            POST CreatePost "" body Json<models::NewPost> -> Json<models::Post>;
        }

        path "users" {
            GET GetUser {id:i32} -> Json<models::User>;

            GET GetUserPosts {id:i32}/"posts"
                query { "userId" as user_id?: u32 }
                -> Json<Vec<models::Post>> | Vec<String> => {
                    IntoIterator::into_iter(r).map(|p| p.title).collect()
                };
        }
    }
}

pub async fn test_api() -> Result<(), ApiClientError> {
    let client = client::Client::new(client::Vars::new(true));

    let posts = client
        .clone()
        .with_debug_level(DebugLevel::VV)
        .execute(client::endpoints::GetPosts::new().user_id(1).x_debug(true))
        .await?;
    println!("GET /posts?userId=1 => {} posts", posts.len());

    let post = client
        .clone()
        .with_debug_level(DebugLevel::V)
        .execute(client::endpoints::GetPost::new(1))
        .await?;
    println!("GET /posts/1 => title={:?}", post.title);

    let comments = client
        .execute(client::endpoints::GetPostComments::new(1))
        .await?;
    println!("GET /posts/1/comments => {} comments", comments.len());

    let created = client
        .execute(client::endpoints::CreatePost::new(
            models::NewPost {
                title: "foo".to_string(),
                body: "bar".to_string(),
                user_id: 10,
            },
        ))
        .await?;
    println!("POST /posts => id={} user_id={}", created.id, created.user_id);

    let user = client
        .execute(client::endpoints::GetUser::new(1))
        .await?;
    println!("GET /users/1 => username={}", user.username);

    let titles = client
        .execute(client::endpoints::GetUserPosts::new(1))
        .await?;
    println!("GET /users/1/posts => {} titles (mapped)", titles.len());

    Ok(())
}
