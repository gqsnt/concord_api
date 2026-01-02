use client_api_lib::prelude::*;

pub enum Region {
    US,
    EU,
    ASIA,
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Region::US => "us",
            Region::EU => "eu",
            Region::ASIA => "asia",
        };
        write!(f, "{}", s)
    }
}

mod api {
    use super::*;
    use client_api_macros::api_client;
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
    api_client! {
        client Client {
            scheme: https,
            host: "typicode.com",

            params {
                user_agent: String="ClientApiExample/1.0".to_string(),
                client_trace: bool,
            }

            headers {
                "user-agent": "{user_agent}",
                // active/inactive via param (template supports Display; bool => "true"/"false")
                "x-client-trace": "{client_trace}",
            }
        }

        // host prefix "jsonplaceholder"
        prefix "jsonplaceholder" {

            // /posts
            path "posts" {
               GET GetPosts "" query { userId?: u32 } headers { "x-debug": "{debug:bool=false}" } -> JsonEncoding<Vec<models::Post>>;
                GET GetPost "{id:i32}" headers {"x-test":"{name?:Region}"} -> JsonEncoding<models::Post>;
                GET GetPostComments "{post_id:i32}/comments" -> JsonEncoding<Vec<models::Comment>>;
                POST CreatePost "" body JsonEncoding<models::NewPost>  -> JsonEncoding<models::Post>;
            }

            // /users
            path "users" {
                GET GetUser "{id:i32}" -> JsonEncoding<models::User>;
                GET GetUserPosts "{id:i32}/posts" query { userId?: u32 } -> JsonEncoding<Vec<models::Post>> | Vec<String> => {
                    r.into_iter().map(|p| p.title).collect()
                };
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), ApiClientError> {
    let client = api::Client::new(true);

    let posts = client
        .execute(api::endpoints::GetPosts::new().user_id(1).debug(true))
        .await?;
    println!("GET /posts?userId=1 => {} posts", posts.len());

    let post = client
        .execute(api::endpoints::GetPost::new(1).name(Region::ASIA))
        .await?;
    println!("GET /posts/1 => title={:?}", post.title);

    let comments = client
        .execute(api::endpoints::GetPostComments::new(1))
        .await?;
    println!("GET /posts/1/comments => {} comments", comments.len());
    let created = client
        .execute(api::endpoints::CreatePost::new(api::models::NewPost {
            title: "foo".to_string(),
            body: "bar".to_string(),
            user_id: 10,
        }))
        .await?;
    println!(
        "POST /posts => id={} user_id={}",
        created.id, created.user_id
    );

    let user = client.execute(api::endpoints::GetUser::new(1)).await?;
    println!("GET /users/1 => username={}", user.username);

    let titles = client.execute(api::endpoints::GetUserPosts::new(1)).await?;
    println!("GET /users/1/posts => {} titles (mapped)", titles.len());

    Ok(())
}
