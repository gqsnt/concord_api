concord::api! {
    pub client Slack(token: Secret) {
        base "https://slack.com/api";
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem SlackProblem;
        status { 200 { body } => slack_response(body)? }
        /// Shares a method's observed 429 cooldown within this client/workspace.
        middleware Cooldown::default();

        part page(limit?: u16, cursor?: String) {
            query { limit; cursor; }
            paginate by_cursor(cursor, SlackPage::next_cursor);
        }
        part workspace(team_id?: String) {
            query { team_id; }
        }
        part content(text?: String, blocks?: Vec<Block>, attachments?: Vec<Attachment>) {
            body json { text; blocks; attachments; }
        }

        POST auth_test() -> AuthIdentity {
            path "auth.test";
            body form { }
        }
        scope conversations {
            GET list(types?: Vec<String>, exclude_archived?: bool, ..page, ..workspace) -> SlackPage<Conversation> {
                path "conversations.list";
                query { "types" = types.as_ref().map(|v| v.join(",")); exclude_archived; }
            }
            GET history(channel: String, oldest?: String, latest?: String, inclusive?: bool, include_all_metadata?: bool, ..page) -> SlackPage<Message> {
                path "conversations.history";
                query { channel; oldest; latest; inclusive; include_all_metadata; }
            }
            GET info(channel: String, include_locale?: bool, include_num_members?: bool) -> ChannelInfo {
                path "conversations.info";
                query { channel; include_locale; include_num_members; }
            }
            POST join(channel: String) -> ChannelInfo {
                path "conversations.join";
                body form { channel; }
            }
        }
        scope chat {
            POST post_message(channel: String, thread_ts?: String, reply_broadcast?: bool, unfurl_links?: bool, unfurl_media?: bool, ..content) -> MessageReceipt {
                path "chat.postMessage";
                body json { channel; thread_ts; reply_broadcast; unfurl_links; unfurl_media; }
            }
            POST update(channel: String, ts: String, ..content) -> MessageReceipt {
                path "chat.update";
                body json { channel; ts; }
            }
            POST delete(channel: String, ts: String) -> DeletedMessage {
                path "chat.delete";
                body form { channel; ts; }
            }
        }
        POST reactions_add(channel: String, timestamp: String, name: String) -> Acknowledged {
            path "reactions.add";
            body form { channel; timestamp; name; }
        }
        scope users {
            GET list(include_locale?: bool, ..page, ..workspace) -> SlackPage<User> {
                path "users.list";
                query { include_locale; }
            }
            GET info(user?: String, include_locale?: bool) -> UserInfo {
                path "users.info";
                query { user; include_locale; }
            }
        }
        scope files {
            POST get_upload_url_external(filename: String, length: u64, alt_txt?: String, snippet_type?: String) -> UploadTicket {
                path "files.getUploadURLExternal";
                body form { filename; length; alt_txt; snippet_type; }
            }
            /// Completion waits for asynchronous processing; 300s is this crate's allowance, not Slack's SLA.
            POST complete_upload_external(uploads: Vec<UploadedFile>, channel_id?: String, initial_comment?: String, thread_ts?: String) -> CompletedFiles {
                path "files.completeUploadExternal";
                body json { "files" = uploads; channel_id; initial_comment; thread_ts; }
                idle_timeout Duration::from_secs(300);
            }
            /// Narrow form: a returned address representable as Base, including its path, without a query.
            POST upload(upload_base: Base, file: Body) {
                base upload_base;
                headers { remove "authorization"; }
                body file;
                status { override 200 => (), 2xx => Err(concord::mismatch("byte upload did not return HTTP 200"))? }
            }
        }
        POST views_open(view: Modal, trigger_id?: String, interactivity_pointer?: String) -> OpenedView {
            path "views.open";
            body json { view; trigger_id; interactivity_pointer; }
        }
    }
}
