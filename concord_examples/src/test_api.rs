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
        vars {
            subdomain: String = "jsonplaceholder".to_string(),
            client_trace: bool
        }

        headers {
            "user-agent" = "ClientApiExample/1.0",
            "x-client-trace" = vars.client_trace
        }
    }

    scope jsonplaceholder {
        host[vars.subdomain]

        scope posts {
            path["posts"]

            GET GetPosts {
                params {
                    user_id?: u32,
                    x_debug: bool = true
                }
                query {
                    "userId" = user_id
                }
                headers {
                    "x-debug" = part["test:", x_debug]
                }
                -> Json<Vec<models::Post>>;
            }

            GET GetPost {
                params {
                    id: i32
                }
                path[id]
                -> Json<models::Post>;
            }

            GET GetPostComments {
                params {
                    post_id: i32
                }
                path[post_id, "comments"]
                -> Json<Vec<models::Comment>>;
            }

            POST CreatePost {
                body Json<models::NewPost>
                -> Json<models::Post>;
            }
        }

        scope users {
            path["users"]

            GET GetUser {
                params {
                    id: i32
                }
                path[id]
                -> Json<models::User>;
            }

            GET GetUserPosts {
                params {
                    id: i32,
                    user_id?: u32
                }
                path[id, "posts"]
                query {
                    "userId" = user_id
                }
                -> Json<Vec<models::Post>> | Vec<String> => {
                    IntoIterator::into_iter(r).map(|p| p.title).collect()
                };
            }
        }
    }
}

pub async fn test_api() -> Result<(), ApiClientError> {
    let client = client::Client::new(true);

    let posts = client
        .clone()
        .request(client::endpoints::GetPosts::new().user_id(1).x_debug(true))
        .debug_level(DebugLevel::VV)
        .execute()
        .await?;
    println!("GET /posts?userId=1 => {} posts", posts.len());

    let post = client
        .clone()
        .request(client::endpoints::GetPost::new(1))
        .debug_level(DebugLevel::V)
        .execute()
        .await?;
    println!("GET /posts/1 => title={:?}", post.title);

    let comments = client
        .request(client::endpoints::GetPostComments::new(1))
        .execute()
        .await?;
    println!("GET /posts/1/comments => {} comments", comments.len());

    let created = client
        .request(client::endpoints::CreatePost::new(models::NewPost {
            title: "foo".to_string(),
            body: "bar".to_string(),
            user_id: 10,
        }))
        .execute()
        .await?;
    println!(
        "POST /posts => id={} user_id={}",
        created.id, created.user_id
    );

    let user = client
        .request(client::endpoints::GetUser::new(1))
        .execute()
        .await?;
    println!("GET /users/1 => username={}", user.username);

    let titles = client
        .request(client::endpoints::GetUserPosts::new(1))
        .execute()
        .await?;
    println!("GET /users/1/posts => {} titles (mapped)", titles.len());

    Ok(())
}
