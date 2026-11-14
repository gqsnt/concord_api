POST send_message(chat_id: ChatId, text: String, ..formatted) -> Message {
    path "sendMessage";
    body json { chat_id; text; "reply_markup" = markup_of(text); }
}
