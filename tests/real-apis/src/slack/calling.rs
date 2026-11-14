use concord::bytes::Bytes;
use concord::Secret;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and who it is. A refusal comes with the status `200`; `e.problem()` has its code.
pub async fn connect(token: Secret) -> slack::Result<Slack> {
    let slack = Slack::new(token).with_retry(retry()).with_timeout(Duration::from_secs(90));
    match slack.auth_test().await {
        Ok(me) => println!("team {}, user {}", me.team_id, me.user_id),
        Err(e) => {
            if let Some(problem) = e.problem() {
                eprintln!("Slack said no: {}", problem.error);
            }
            return Err(e);
        }
    }
    Ok(slack)
}

/// Every public channel, all pages; then one page of a channel's history, on purpose.
pub async fn channels(slack: &Slack) -> slack::Result<()> {
    let mut channels = slack.conversations().list()
        .types(["public_channel"]).exclude_archived(true).limit(200)
        .pages().items();
    while let Some(channel) = channels.try_next().await? {
        println!("{}", channel.id);
    }
    let recent = slack.conversations().history("C123").limit(15).send().await?;
    println!("{} messages", recent.items.len());
    Ok(())
}

/// Post, edit, react, delete.
pub async fn post(slack: &Slack) -> slack::Result<()> {
    let sent = slack.chat().post_message("C123").text("Deploying").unfurl_links(false).await?;
    slack.chat().update(&sent.channel, &sent.ts).text("Deployed").await?;
    slack.reactions_add(&sent.channel, &sent.ts, "white_check_mark").await?;
    slack.chat().delete(&sent.channel, &sent.ts).await?;
    Ok(())
}

/// An upload in three steps: ask for a URL, send the bytes there, then share the file.
pub async fn upload(slack: &Slack, name: &str, bytes: Bytes) -> Result<(), BoxError> {
    let ticket = slack.files().get_upload_url_external(name, bytes.len() as u64).await?;
    upload_bytes(&ticket.upload_url, bytes).await?;
    let file = UploadedFile { id: ticket.file_id, title: Some(name.to_owned()) };
    slack.files().complete_upload_external([file]).channel_id("C123").await?;
    Ok(())
}
