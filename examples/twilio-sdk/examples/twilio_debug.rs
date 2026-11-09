//! Ordinary caller diagnostics and native request preparation, without sending an SMS.
use concord_sdk_twilio::{BasicAuth, CreateMessage, Message, MessageStatus, Twilio};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let auth = BasicAuth::new("synthetic-key", "synthetic-password")?;
    println!("{auth:?}");
    let api = Twilio::new(concord::reqwest::Client::new(), "AC_synthetic", auth);
    let input = CreateMessage::new("+15005550001", "+15005550006", "Private message");
    println!("{input:?}");
    println!("{:?}", api.prepare_create_message(input)?);

    // The fields a fetch would return remain directly readable, with the same public types.
    let message = Message {
        sid: "SM_synthetic".into(),
        account_sid: "AC_synthetic".into(),
        status: MessageStatus::Undelivered,
        body: Some("Private message".into()),
        from: Some("+15005550006".into()),
        to: Some("+15005550001".into()),
        error_code: Some(30007),
        error_message: Some("Private provider explanation".into()),
    };
    // Both D1 and the D4 variant show sid, status and error_code.
    println!("{message:?}");
    println!("{message:#?}");
    println!("{:?}", api.prepare_get_message(message.sid.clone())?);
    Ok(())
}
