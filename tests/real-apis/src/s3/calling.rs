use concord::{Base, Body, ByteStream};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client for one region, and a bucket by its address: the bucket in the host, or in the path.
pub fn connect(credentials: Credentials) -> Result<(ObjectStore, object_store::Bucket), BoxError> {
    let store = ObjectStore::new("eu-west-1", credentials);
    let photos: Base = "https://photos.s3.eu-west-1.amazonaws.com".parse()?;
    // Or, with the bucket in the path: "https://s3.eu-west-1.amazonaws.com/photos".
    let bucket = store.bucket(photos);
    Ok((store, bucket))
}

/// Every object under a prefix, all pages, with the "folders" that the delimiter makes.
pub async fn list(bucket: &object_store::Bucket) -> object_store::Result<()> {
    let mut pages = bucket.list_objects().prefix("reports/").delimiter("/").max_keys(500).pages();
    while let Some(page) = pages.try_next().await? {
        for object in page.objects {
            println!("{}: {} bytes", object.key, object.size);
        }
        for folder in page.common_prefixes {
            println!("folder: {}", folder.prefix);
        }
    }
    Ok(())
}

/// Write an object with your own metadata, read its headers, copy it to another key.
pub async fn put_and_copy(bucket: &object_store::Bucket) -> Result<(), BoxError> {
    let object = bucket.object("reports/2026/november.txt");
    let etag = object.put(Body::new("hello", "text/plain"), 5)
        .edit_request(metadata([("owner", "finance")])?)
        .await?;
    let head = object.head().await?;
    println!("{:?}", head.get("x-amz-meta-owner"));
    bucket.object("reports/latest.txt")
        .copy_from("photos", "reports/2026/november.txt")
        .source_if_match(etag)
        .await?;
    Ok(())
}

/// The object again, only if it changed since its ETag. `None`: it did not (a `304`).
pub async fn if_changed(bucket: &object_store::Bucket, etag: &str) -> object_store::Result<Option<ByteStream>> {
    let answer = bucket.object("large.bin").get().send_if_none_match(etag).await?;
    Ok(answer.into_value())
}

/// The first kilobyte of an object, as a stream of chunks.
pub async fn first_kilobyte(bucket: &object_store::Bucket) -> object_store::Result<Vec<u8>> {
    let answer = bucket.object("large.bin").get().range("bytes=0-1023").send_full().await?;
    println!("{}", answer.status());                       // 206: a part of the object
    let mut stream = answer.into_value();
    let mut bytes = Vec::new();
    while let Some(piece) = stream.try_next().await? {
        bytes.extend_from_slice(&piece);
    }
    Ok(bytes)
}

/// An upload in parts (one part here; the last part may be small), aborted when a part fails.
pub async fn upload(bucket: &object_store::Bucket, data: Vec<u8>) -> object_store::Result<String> {
    let object = bucket.object("backup.bin");
    let started = object.create_multipart_upload().content_type("application/octet-stream").await?;
    let upload = object.upload(started.upload_id);
    let size = data.len() as u64;
    let etag = match upload.upload_part(1, Body::new(data, "application/octet-stream"), size).await {
        Ok(etag) => etag,
        Err(error) => {
            upload.abort().await?;
            return Err(error);
        }
    };
    let done = upload.complete(CompleteUpload::from_etags([(1, etag)])).await?;
    Ok(done.etag)
}
