# 6. Bodies, Responses, and Mapping

An endpoint can declare a request body and must declare a response codec.

```rust
POST CreatePost(body: Json<NewPost>) -> Json<Post>;
```

The body codec controls how the request body is encoded. The response codec controls how response bytes are decoded.

## JSON request bodies

`body Json<T>` serializes a Rust value into JSON and sets `Content-Type: application/json` unless overridden.

```rust
mod models {
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct NewPost {
        pub title: String,
        pub body: String,
        pub user_id: u32,
    }
}

api! {
    client Client {
        scheme: https,
        host: "example.com",
    }

    POST CreatePost(body: Json<models::NewPost>) -> Json<Post> {
        path["posts"]
    }
}
```

The generated constructor includes the body value.

```rust
let created = api
    .request(endpoints::CreatePost::new(models::NewPost {
        title: "foo".to_string(),
        body: "bar".to_string(),
        user_id: 10,
    }))
    .execute()
    .await?;
```

## Response codecs

Common response codecs exported by the prelude are:

- `Json<T>` for JSON bodies.
- `Text<T>` for text bodies where the codec implementation supports the target type.
- `NoContent<()>` for responses that must not have a body.

A response declaration is required. The canonical form puts the response in the endpoint header.

```rust
GET GetPost(id: i32) -> Json<Post> {
    path["posts", id]
}
```

## Decoded response metadata

`execute()` returns only the decoded value.

```rust
let post: Post = api.request(endpoints::GetPost::new(1))
    .execute()
    .await?;
```

`execute_decoded()` returns `DecodedResponse<T>` with metadata.

```rust
let response = api.request(endpoints::GetPost::new(1))
    .execute_decoded()
    .await?;

println!("status = {}", response.status);
println!("url = {}", response.url);
println!("value = {:?}", response.value);
```

Use `execute_decoded()` when the status, headers, URL, or request metadata matters.

## Response mapping

Mapping transforms a decoded response into another output type.

```rust
GET GetUserPosts(id: i32) -> Json<Vec<Post>> | Vec<String> => {
    IntoIterator::into_iter(r).map(|p| p.title).collect()
} {
    path["users", id, "posts"]
}
```

The value named `r` is the decoded response body before mapping. The type after `|` is the endpoint output type returned by `execute()`.

```rust
let titles: Vec<String> = api
    .request(endpoints::GetUserPosts::new(1))
    .execute()
    .await?;
```

Use mapping for small transformations that are part of the API contract, such as extracting IDs or flattening wrapper objects. Keep complex business logic outside the DSL.

## No-content responses

`HEAD` endpoints must use `NoContent<()>`.

```rust
HEAD CheckUser -> NoContent<()> {
    path["users", "42"]
}
```

HTTP 204 and 205 success statuses also require `NoContent<()>`. If an endpoint expects `Json<T>` and the server returns 204 or 205, Concord returns `ApiClientError::NoContentStatusRequiresNoContent`.

This is intentional. It prevents accidental successful decoding of an empty body into a meaningful value.

## Status errors

Concord classifies successful and unsuccessful HTTP statuses before decoding. Non-success statuses become `ApiClientError::HttpStatus` unless retry or auth handling changes the flow.

```rust
match api.request(endpoints::GetPost::new(1)).execute().await {
    Ok(post) => println!("{}", post.title),
    Err(ApiClientError::HttpStatus { status, headers, .. }) => {
        eprintln!("server returned {status} with {headers:?}");
    }
    Err(err) => return Err(err),
}
```

## Body and policy interaction

The body is available after route and policy construction. Headers and query values can use endpoint parameters and body constructor arguments if they are declared as endpoint fields.

For JSON bodies, let Concord set `Content-Type` unless the upstream API explicitly requires something else.

## Codec feature notes

`Json<T>` requires the `json` feature on `concord_core`.

The public DSL currently uses the codecs exported by `concord_core`, such as `Json<T>`, `Text<T>`, and `NoContent<()>`. Custom external codecs are not yet a stable extension point because the lower-level codec traits are not exported as public API.
