/// A body made of several inputs (`serde_json`'s `json!`). A `None` is written as `null`.
POST search(text: String, cursor?: String) -> Page {
    path "search";
    body json(json!({ "query": text, "cursor": cursor }));
}
