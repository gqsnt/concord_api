concord::api! {
    pub client Telegram(token: Secret) {
        base "https://api.telegram.org/bot{token}";
        read Json;
        problem TgProblem read Json;
        status { 2xx { body } => telegram_result(body)? }
        middleware AttemptTrace;

        part formatted(parse_mode?: ParseMode, entities?: Vec<MessageEntity>) {
            body json { parse_mode; entities; }
        }

        GET get_me() -> User { path "getMe"; }
        POST get_me_post() -> User { path "getMe"; idempotent; }

        /// One batch; repeated polling and acknowledgement belong to the caller.
        GET get_updates(
            offset?: i64, limit?: u8, timeout?: u32,
            allowed_updates?: Vec<String>,
        ) -> Vec<Update> {
            path "getUpdates";
            query {
                offset; limit; timeout;
                "allowed_updates" = allowed_updates.as_ref()
                    .map(serde_json::to_string).transpose()?;
            }
            idle_timeout Duration::from_secs(u64::from(timeout.unwrap_or(0)) + 10);
            idempotent false;
        }

        POST send_message(
            chat_id: ChatId, text: String, disable_notification?: bool,
            protect_content?: bool, reply_markup?: ReplyMarkup, ..formatted,
        ) -> Message {
            path "sendMessage";
            body json { chat_id; text; disable_notification; protect_content; reply_markup; }
        }

        /// Supply chat_id + message_id, or inline_message_id, exclusively.
        POST edit_message_text(
            text: String, chat_id?: ChatId, message_id?: i64,
            inline_message_id?: String, reply_markup?: InlineKeyboardMarkup, ..formatted,
        ) -> EditedMessage {
            path "editMessageText";
            body json { text; chat_id; message_id; inline_message_id; reply_markup; }
        }

        POST delete_message(chat_id: ChatId, message_id: i64) -> bool {
            path "deleteMessage";
            body json { chat_id; message_id; }
        }

        POST send_photo(
            chat_id: ChatId, photo: MediaInput, caption?: String,
            parse_mode?: ParseMode, disable_notification?: bool,
        ) -> Message {
            path "sendPhoto";
            body media_body("photo", chat_id, photo, caption, parse_mode, disable_notification);
        }

        POST send_document(
            chat_id: ChatId, document: MediaInput, caption?: String,
            parse_mode?: ParseMode, disable_notification?: bool,
        ) -> Message {
            path "sendDocument";
            body media_body("document", chat_id, document, caption, parse_mode, disable_notification);
        }

        GET get_file(file_id: String) -> File {
            path "getFile";
            query { file_id; }
        }

        /// Same host, different URL prefix; this success is bytes, not an envelope.
        GET download_file(file_path: String) -> concord::ByteStream {
            base "https://api.telegram.org/file/bot{client.token}";
            path "{*file_path}";
            read Stream;
            status { remove 2xx }
        }

        POST set_webhook(
            url: String, secret_token?: Secret, allowed_updates?: Vec<String>,
            drop_pending_updates?: bool,
        ) -> bool {
            path "setWebhook";
            body webhook_body(url, secret_token, allowed_updates, drop_pending_updates);
        }

        POST delete_webhook(drop_pending_updates?: bool) -> bool {
            path "deleteWebhook";
            body json { drop_pending_updates; }
        }

        POST answer_callback_query(
            callback_query_id: String, text?: String, show_alert?: bool,
            url?: String, cache_time?: u32,
        ) -> bool {
            path "answerCallbackQuery";
            body json { callback_query_id; text; show_alert; url; cache_time; }
        }

        GET get_chat(chat_id: ChatId) -> Option<ChatFullInfo> {
            path "getChat";
            status { 400 => None }
            query { chat_id; }
        }
    }
}
fn main() {}

include!("../../support/models.rs");
