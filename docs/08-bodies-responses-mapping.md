# 8. Bodies, Responses, and Mapping

Endpoints declare request bodies and response codecs.

## JSON body

```rust
POST CreatePost(body: Json<NewPost>)
    as create_post
    path ["posts"]
    -> Json<Post>
```

Usage:

```rust
let post = api.posts()
    .create_post(NewPost {
        title: "foo".to_string(),
        body: "bar".to_string(),
        user_id: 10,
    })
    .await?;
```

## Response codec

Common codecs:

```rust
Json<T>
Text<T>
NoContent<()>
```

Example:

```rust
GET GetPost(id: i32)
    as get_post
    path [id]
    -> Json<Post>
```

## No content

Use `NoContent<()>` for endpoints that must not return a body.

```rust
HEAD CheckUser(id: u64)
    as check
    path ["users", id]
    -> NoContent<()>
```

## Mapping

Mapping transforms the decoded response into the endpoint output type.

```rust
GET GetUserPosts(id: i32, user_id?: u32)
    path [id, "posts"]
    query {
        "userId" = user_id
    }
    -> Json<Vec<Post>>
    map Vec<String> {
        IntoIterator::into_iter(r).map(|p| p.title).collect()
    }
```

The variable `r` is the decoded response body.

Usage:

```rust
let titles: Vec<String> = api
    .jsonplaceholder()
    .users()
    .get_user_posts(1)
    .await?;
```

## Login mapping

Endpoint-backed credentials commonly map login JSON into `AccessToken`.

```rust
POST LoginForSession(body: Json<LoginRequest>)
    path ["login"]
    -> Json<LoginResponse>
    map AccessToken {
        AccessToken::new(r.access_token)
    }
```

The endpoint output type is `AccessToken`.

## `execute_decoded`

Use `execute_decoded()` for metadata.

```rust
let response = api.posts()
    .get_post(1)
    .execute_decoded()
    .await?;

println!("status = {}", response.status);
println!("url = {}", response.url);
println!("value = {:?}", response.value);
```

## Status handling

Non-success statuses produce errors unless retry/auth handling changes the flow.

No-content statuses such as `204` and `205` require `NoContent<()>`. This prevents silently treating an empty body as valid JSON.
