type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and a reply in one response. The model is the caller's choice.
pub async fn ask() -> Result<(), BoxError> {
    let api = Anthropic::new(std::env::var("ANTHROPIC_API_KEY")?);
    let model = std::env::var("ANTHROPIC_MODEL")?;
    let messages = vec![Turn::user("Name three rivers.")];
    let tokens = api.count_tokens(&model, &messages).await?;
    println!("{} input tokens", tokens.input_tokens);
    let reply = api.create(&model, 1024, messages).system("Answer briefly.").await?;
    for block in reply.content {
        println!("{}", block.text.unwrap_or_default());
    }
    Ok(())
}

/// The text of a streamed reply, printed as it arrives.
pub async fn stream(api: &Anthropic, model: &str, prompt: &str) -> Result<String, ReplyError> {
    let mut events = api.stream(model, 1024, vec![Turn::user(prompt)]).await
        .map_err(ReplyError::Call)?;
    let mut text = String::new();
    while let Some(event) = events.try_next().await.map_err(ReplyError::Record)? {
        match streamed(&event).map_err(ReplyError::Record)? {
            Streamed::Text(piece) => {
                print!("{piece}");
                text.push_str(&piece);
            }
            Streamed::Failed(error) => return Err(ReplyError::Api(error)),
            Streamed::Done => return Ok(text),
            Streamed::Stopped { .. } | Streamed::Other => {}
        }
    }
    Err(ReplyError::Unfinished)
}

/// Every model, page after page.
pub async fn models(api: &Anthropic) -> anthropic::Result<()> {
    let mut models = api.models().limit(100).pages().items();
    while let Some(model) = models.try_next().await? {
        println!("{} {}", model.id, model.display_name);
    }
    Ok(())
}
