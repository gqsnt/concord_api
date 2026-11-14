use concord::Secret;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Send a message, change it, delete it.
pub async fn hello(bot: &Telegram, chat: ChatId) -> telegram::Result<()> {
    let me = bot.get_me().await?;
    let sent = bot.send_message(&chat, format!("{} is working…", me.first_name)).await?;
    bot.edit_message_text("Done").chat_id(&chat).message_id(sent.message_id).await?;
    bot.delete_message(&chat, sent.message_id).await?;
    Ok(())
}

/// Updates by long poll, for ever. An empty batch is not the end: poll again.
pub async fn poll(bot: &Telegram, mut handle: impl FnMut(Update)) -> telegram::Result<()> {
    bot.delete_webhook().drop_pending_updates(false).await?;
    let mut offset = None::<i64>;
    loop {
        let batch = bot.get_updates().offset(offset).timeout(50).with_retry(retry()).await?;
        for update in batch {
            offset = Some(update.update_id + 1);
            handle(update);
        }
    }
}

/// Send a file from its bytes, then read it back from Telegram.
pub async fn round_trip(bot: &Telegram, chat: &ChatId, file: InputFile) -> Result<Vec<u8>, BoxError> {
    let sent = bot.send_document(chat, file).caption("The report").await?;
    let document = sent.document.ok_or("no document in the message")?;
    let file = bot.get_file(document.file_id).await?;
    let path = file.file_path.ok_or("no file_path")?;
    let mut stream = bot.download_file(path).with_timeout(Duration::from_secs(600)).await?;
    let mut bytes = Vec::new();
    while let Some(piece) = stream.try_next().await? {
        bytes.extend_from_slice(&piece);
    }
    Ok(bytes)
}

/// Updates by webhook: Telegram sends them to `url`, with the secret in a header.
pub async fn use_webhook(bot: &Telegram, url: &str, secret: Secret) -> telegram::Result<bool> {
    bot.set_webhook(url)
        .secret_token(secret)
        .allowed_updates(["message", "callback_query"])
        .await
}
