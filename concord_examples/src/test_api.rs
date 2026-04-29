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
        base https "typicode.com"
        var subdomain: String = "jsonplaceholder".to_string()
        var client_trace: bool

        headers {
            "user-agent" = "ClientApiExample/1.0",
            "x-client-trace" = vars.client_trace
        }
        default {
            retry read
        }
        retry read {
                max_attempts 2
                methods [GET, HEAD]
                on [429, 500, 502, 503, 504]
                retry_after
        }
    }

    scope jsonplaceholder {
        host [vars.subdomain]

        scope posts {
            path ["posts"]

            GET GetPosts(user_id?: u32, x_debug: bool = true)
                as list
                query {
                    "userId" = user_id
                }
                headers {
                    "x-debug" = fmt["test:", x_debug]
                }
                -> Json<Vec<models::Post>>

            GET GetPost(id: i32)
                as by_id
                path [id]
                -> Json<models::Post>

            GET GetPostComments(post_id: i32)
                as comments
                path [post_id, "comments"]
                -> Json<Vec<models::Comment>>

            POST CreatePost(body: Json<models::NewPost>)
                as create
                -> Json<models::Post>;
        }

        scope users {
            path ["users"]

            GET GetUser(id: i32)
                as by_id
                path [id]
                -> Json<models::User>

            GET GetUserPosts(id: i32, user_id?: u32)
                as posts
                path [id, "posts"]
                query {
                    "userId" = user_id
                }
                -> Json<Vec<models::Post>>
                map Vec<String> {
                    IntoIterator::into_iter(r).map(|p| p.title).collect()
                }
        }
    }
}

pub async fn test_api() -> Result<(), ApiClientError> {
    let client = client::Client::new(true);

    let posts = client
        .jsonplaceholder()
        .posts()
        .list()
        .debug_level(DebugLevel::VV)
        .await?;
    println!("GET /posts => {} posts", posts.len());

    let post = client
        .jsonplaceholder()
        .posts()
        .by_id(1)
        .debug_level(DebugLevel::V)
        .await?;
    println!("GET /posts/1 => title={:?}", post.title);

    let comments = client.jsonplaceholder().posts().comments(1).await?;
    println!("GET /posts/1/comments => {} comments", comments.len());

    let created = client
        .jsonplaceholder()
        .posts()
        .create(models::NewPost {
            title: "foo".to_string(),
            body: "bar".to_string(),
            user_id: 10,
        })
        .await?;
    println!(
        "POST /posts => id={} user_id={}",
        created.id, created.user_id
    );

    let user = client.jsonplaceholder().users().by_id(1).await?;
    println!("GET /users/1 => username={}", user.username);

    let titles = client.jsonplaceholder().users().posts(1).await?;
    println!("GET /users/1/posts => {} titles (mapped)", titles.len());

    Ok(())
}
