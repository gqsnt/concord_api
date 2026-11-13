concord::api! {
    /// The Slack Web API: one method is one URL, `https://slack.com/api/<family>.<method>`.
    /// Every answer is a `200`; `{ "ok": false, "error": "…" }` is a refusal.
    pub client Slack(
        /// A bot (`xoxb-…`) or a user (`xoxp-…`) token.
        token: Secret,
    ) {
        base "https://slack.com/api";                 // `/api` here: a `path` would follow the upload host
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem SlackProblem;
        status { 2xx { body } => Reply::into_result(body)? }     // `ok: false` is a Refused error

        /// Slack's cursor: the next one is `response_metadata.next_cursor`, empty at the end.
        part page(cursor?: String, limit?: u32) {
            query { cursor; limit; }
            paginate by_cursor(cursor, next_cursor);
        }

        scope auth {
            /// Who this token is: the team, the user, the bot.
            GET test() -> Identity { path "auth.test"; status { remove 200 } }
        }

        scope conversations {
            /// The conversations the token can see. `types` goes as one comma list.
            GET list(
                exclude_archived?: bool,
                types?: Vec<ConversationType>,
                team_id?: TeamId,
                ..page
            ) -> ChannelList {
                path "conversations.list";
                query { exclude_archived; "types" = types.as_ref().map(|t| comma(t)); team_id; }
            }

            /// The messages of a conversation, newest first.
            GET history(
                channel: ChannelId,
                oldest?: Ts,
                latest?: Ts,
                inclusive?: bool,
                include_all_metadata?: bool,
                ..page
            ) -> MessageList {
                path "conversations.history";
                query { channel; oldest; latest; inclusive; include_all_metadata; }
            }

            GET info(channel: ChannelId, include_locale?: bool, include_num_members?: bool) -> ChannelInfo {
                path "conversations.info";
                query { channel; include_locale; include_num_members; }
            }

            /// Joins a public channel. Joining again changes nothing (Slack adds a warning).
            POST join(channel: ChannelId) -> ChannelInfo {
                path "conversations.join";
                body slack_json(json!({ "channel": channel }));
                idempotent;
            }
        }

        scope chat {
            /// Slack wants `text` or `blocks` (else `no_text`).
            POST post_message(
                channel: ChannelId,
                text?: String,
                blocks?: Vec<Block>,
                thread_ts?: Ts,
                reply_broadcast?: bool,
                unfurl_links?: bool,
                unfurl_media?: bool,
                metadata?: Metadata,
            ) -> Posted {
                path "chat.postMessage";
                body slack_json(json!({
                    "channel": channel, "text": text, "blocks": blocks, "thread_ts": thread_ts,
                    "reply_broadcast": reply_broadcast, "unfurl_links": unfurl_links,
                    "unfurl_media": unfurl_media, "metadata": metadata,
                }));
            }

            /// The same update twice gives the same message.
            POST update(channel: ChannelId, ts: Ts, text?: String, blocks?: Vec<Block>) -> Updated {
                path "chat.update";
                body slack_json(json!({ "channel": channel, "ts": ts, "text": text, "blocks": blocks }));
                idempotent;
            }

            POST delete(channel: ChannelId, ts: Ts) -> Deleted {
                path "chat.delete";
                body slack_json(json!({ "channel": channel, "ts": ts }));
            }
        }

        scope reactions {
            /// `name` is the emoji without colons; `timestamp` is the `ts` of the message.
            POST add(channel: ChannelId, timestamp: Ts, name: String) -> Done {
                path "reactions.add";
                body slack_json(json!({ "channel": channel, "timestamp": timestamp, "name": name }));
            }
        }

        scope users {
            GET list(include_locale?: bool, team_id?: TeamId, ..page) -> UserList {
                path "users.list";
                query { include_locale; team_id; }
            }

            GET info(user: UserId, include_locale?: bool) -> UserInfo {
                path "users.info";
                query { user; include_locale; }
            }
        }

        scope files {
            /// Upload, step 1: where to send `length` bytes. A form, not JSON (unsure).
            POST get_upload_url_external(
                filename: String,
                length: u64,
                alt_txt?: String,
                snippet_type?: String,
            ) -> UploadTicket {
                path "files.getUploadURLExternal";
                body slack_form(json!({
                    "filename": filename, "length": length, "alt_txt": alt_txt, "snippet_type": snippet_type,
                }));
            }

            /// Upload, step 2: the bytes, to the `upload_url` of step 1. Another host, no token,
            /// and the answer is text (`OK - 1234`), not Slack's JSON (unsure).
            POST upload(upload_url: concord::Base, file: concord::Body) {
                base upload_url;
                headers { remove "authorization"; }
                status { remove 2xx }
                body file;
            }

            /// Upload, step 3: the files are shared, in a channel when `channel_id` is given.
            POST complete_upload_external(
                uploaded: Vec<FileRef>,                   // Slack's `files`: the name of this scope
                channel_id?: ChannelId,
                initial_comment?: String,
                thread_ts?: Ts,
            ) -> Completed {
                path "files.completeUploadExternal";
                body slack_json(json!({
                    "files": uploaded, "channel_id": channel_id,
                    "initial_comment": initial_comment, "thread_ts": thread_ts,
                }));
            }
        }

        scope views {
            /// A modal. `trigger_id` comes from an interaction, lives 3 seconds, and is used once.
            POST open(trigger_id: String, view: View) -> Opened {
                path "views.open";
                body slack_json(json!({ "trigger_id": trigger_id, "view": view }));
            }
        }
    }
}
fn main() {}

include!("../support/real.rs");
