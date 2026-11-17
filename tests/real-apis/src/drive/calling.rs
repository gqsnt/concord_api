use concord::Body;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A report uploaded into a folder with its name, in one request; then read back.
pub async fn upload(drive: &Drive, folder: &str, csv: Vec<u8>) -> Result<File, BoxError> {
    let metadata = NewFile {
        name: Some("report.csv".into()),
        parents: vec![folder.into()],
        ..Default::default()
    };
    let file = drive.upload(metadata, Body::new(csv, "text/csv")).await?;
    let mut bytes = drive.download(&file.id).await?;
    while let Some(chunk) = bytes.try_next().await? {
        println!("{} bytes", chunk.len());
    }
    Ok(file)
}

/// Every PDF of the user's Drive, page after page, with only the fields that are needed.
pub async fn pdfs() -> Result<(), BoxError> {
    let drive = Drive::new(std::env::var("GOOGLE_ACCESS_TOKEN")?);
    let mut files = drive.list()
        .q("mimeType = 'application/pdf' and trashed = false")
        .fields("nextPageToken, files(id, name)")
        .page_size(100)
        .pages().items();
    while let Some(file) = files.try_next().await? {
        println!("{} {}", file.id, file.name);
    }
    Ok(())
}

/// A file renamed, then deleted.
pub async fn rename_and_delete(drive: &Drive, file_id: &str) -> drive::Result<()> {
    let name = NewFile { name: Some("old report.csv".into()), ..Default::default() };
    let renamed = drive.update(file_id, name).await?;
    println!("{}", renamed.name);
    drive.delete(file_id).await?;
    Ok(())
}
