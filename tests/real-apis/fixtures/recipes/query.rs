concord::api! {
    pub client Search {
        base "https://search.example.com";
        GET find() { query { "page" = 1; } }
    }
}

async fn search(filters: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>)
    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let pairs = concord::bracket_pairs(&filters)?;
    Search::new().find().edit_request(move |request| {
        request.url_mut().query_pairs_mut().extend_pairs(&pairs);
    }).await?;
    Ok(())
}
