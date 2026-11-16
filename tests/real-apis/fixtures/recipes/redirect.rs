concord::api! {
    pub client Archives(token: concord::Secret) {
        base "https://api.example.com";
        headers { "authorization" = "Bearer {token}"; }
        GET download(id: u64) { path "archives/{id}"; }
    }
}
concord::api! {
    pub client Downloads(origin: concord::Base) {
        base origin;
        GET fetch() -> concord::ByteStream { read concord::read::Stream; }
    }
}

async fn download(api: &Archives, id: u64)
    -> Result<concord::ByteStream, Box<dyn std::error::Error + Send + Sync>>
{
    let response = api.download(id).send_raw().await?;
    if response.status() != concord::http::StatusCode::FOUND {
        return Err("the archive did not return its documented 302".into());
    }
    let location = response.headers().get("location")
        .ok_or("the download Location is missing")?.to_str()?.to_owned();
    let url = concord::reqwest::Url::parse(&location)?;
    if url.scheme() != "https" || url.port_or_known_default() != Some(443)
        || !url.host_str().is_some_and(|host| host.ends_with(".downloads.example.com"))
    {
        return Err("the download host is not allowed".into());
    }
    let origin: concord::Base = url.origin().ascii_serialization().parse()?;
    drop(response);
    let downloads = Downloads::new(origin);
    Ok(concord::transfer::at(downloads.fetch(), &location)?.await?)
}
