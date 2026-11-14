concord::api! {
    /// The Telegram Bot API: twelve of its methods, and the download of a file.
    pub client Telegram(
        /// The bot's token from @BotFather (`123456:ABC-…`). Telegram wants it in the path.
        token: Secret,
    ) {
        base "https://api.telegram.org/bot{token}";
        read Json;
        problem TgProblem;
        status { 2xx { body } => Reply::into_result(body)? }        // `{ "ok": true, "result": … }`

        /// How a text is formatted: one of the two.
        part formatted(parse_mode?: ParseMode, entities?: Vec<MessageEntity>) {}

        /// What a new message may say about how it is delivered.
        part delivery(
            message_thread_id?: i64,
            disable_notification?: bool,
            protect_content?: bool,
            reply_parameters?: ReplyParameters,
            reply_markup?: ReplyMarkup,
        ) {}

        /// The bot itself.
        GET get_me() -> User { path "getMe"; }

        /// New updates, oldest first. While there is none, the server holds the request up to
        /// `timeout` seconds: send it with `.long_poll(seconds)`, which sets Concord's clock too.
        GET get_updates(offset?: i64, limit?: u32, timeout?: u32, allowed_updates?: Vec<UpdateKind>) -> Vec<Update> {
            path "getUpdates";
            query { offset; limit; timeout; "allowed_updates" = allowed_updates.as_ref().map(AsJson); }
        }

        /// `sendMessage` (`send_message` is a name that Concord keeps).
        POST post_message(chat_id: ChatId, text: String, link_preview_options?: LinkPreviewOptions,
                          ..formatted, ..delivery) -> Message {
            path "sendMessage";
            body params(json!({
                "chat_id": chat_id, "text": text, "link_preview_options": link_preview_options,
                "parse_mode": parse_mode, "entities": entities,
                "message_thread_id": message_thread_id, "disable_notification": disable_notification,
                "protect_content": protect_content, "reply_parameters": reply_parameters,
                "reply_markup": reply_markup
            }));
        }

        /// `sendPhoto`: a `file_id`, a URL that Telegram fetches, or bytes.
        POST post_photo(chat_id: ChatId, photo: InputFile, caption?: String, ..formatted, ..delivery) -> Message {
            path "sendPhoto";
            body upload("photo", photo, json!({
                "chat_id": chat_id, "caption": caption,
                "parse_mode": parse_mode, "caption_entities": entities,
                "message_thread_id": message_thread_id, "disable_notification": disable_notification,
                "protect_content": protect_content, "reply_parameters": reply_parameters,
                "reply_markup": reply_markup
            }));
        }

        /// `sendDocument`: the same three ways. (`thumbnail` is left out.)
        POST post_document(chat_id: ChatId, document: InputFile, caption?: String,
                           disable_content_type_detection?: bool, ..formatted, ..delivery) -> Message {
            path "sendDocument";
            body upload("document", document, json!({
                "chat_id": chat_id, "caption": caption,
                "disable_content_type_detection": disable_content_type_detection,
                "parse_mode": parse_mode, "caption_entities": entities,
                "message_thread_id": message_thread_id, "disable_notification": disable_notification,
                "protect_content": protect_content, "reply_parameters": reply_parameters,
                "reply_markup": reply_markup
            }));
        }

        /// `editMessageText` for a message in a chat: the message as it is now.
        POST edit_message_text(chat_id: ChatId, message_id: i64, text: String,
                               reply_markup?: InlineKeyboardMarkup, ..formatted) -> Message {
            path "editMessageText";
            body params(json!({
                "chat_id": chat_id, "message_id": message_id, "text": text, "reply_markup": reply_markup,
                "parse_mode": parse_mode, "entities": entities
            }));
        }

        /// `editMessageText` for a message that was sent inline: Telegram answers `true`.
        POST edit_inline_message_text(inline_message_id: String, text: String,
                                      reply_markup?: InlineKeyboardMarkup, ..formatted) -> bool {
            path "editMessageText";
            body params(json!({
                "inline_message_id": inline_message_id, "text": text, "reply_markup": reply_markup,
                "parse_mode": parse_mode, "entities": entities
            }));
        }

        /// `true`. Most messages can be deleted only in their first 48 hours (unsure of every case).
        POST delete_message(chat_id: ChatId, message_id: i64) -> bool {
            path "deleteMessage";
            body params(json!({ "chat_id": chat_id, "message_id": message_id }));
        }

        /// Ends the wait on an inline button; `text` shows as a notice, or as an alert.
        POST answer_callback_query(callback_query_id: String, text?: String, show_alert?: bool,
                                   url?: String, cache_time?: u32) -> bool {
            path "answerCallbackQuery";
            body params(json!({
                "callback_query_id": callback_query_id, "text": text, "show_alert": show_alert,
                "url": url, "cache_time": cache_time
            }));
        }

        /// A chat, with everything that Telegram tells a bot about it.
        GET get_chat(chat_id: ChatId) -> ChatFullInfo {
            path "getChat";
            query { chat_id; }
        }

        /// Where a file is: its `file_path`, good for at least an hour. 20 MB at most.
        GET get_file(file_id: String) -> File {
            path "getFile";
            query { file_id; }
        }

        /// The bytes of a file, from the `file_path` of `get_file`: another path on the same host.
        GET download(file_path: String) -> ByteStream {
            base "https://api.telegram.org/file/bot{client.token}";
            path "{*file_path}";
            read Stream;
            status { remove 2xx }                                    // the bytes are not `{ "ok": … }`
        }

        /// Updates go to `url`, and `get_updates` answers `409`. (`certificate`, `ip_address` left out.)
        POST set_webhook(url: String, secret_token?: Secret, max_connections?: u32,
                         allowed_updates?: Vec<UpdateKind>, drop_pending_updates?: bool) -> bool {
            path "setWebhook";
            body params(json!({
                "url": url, "secret_token": secret_token, "max_connections": max_connections,
                "allowed_updates": allowed_updates, "drop_pending_updates": drop_pending_updates
            }));
            idempotent drop_pending_updates != &Some(true);          // a second drop loses what came between
        }

        /// Back to `get_updates`.
        POST delete_webhook(drop_pending_updates?: bool) -> bool {
            path "deleteWebhook";
            body params(json!({ "drop_pending_updates": drop_pending_updates }));
            idempotent drop_pending_updates != &Some(true);
        }
    }
}
