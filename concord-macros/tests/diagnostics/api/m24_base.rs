concord::api! {
    pub client Github(user_agent: String, quota: QuotaLog, token?: Secret) {
        base "https://api.github.com";
        headers {
            "user-agent" = user_agent;
            "accept" = "application/vnd.github+json";
            "x-github-api-version" = "2026-03-10";
            "authorization" = "Bearer {token}";
        }
        read Json;
        problem GithubProblem;
        /// Records rate-limit headers on every response, including each page and try.
        middleware ObserveQuota(quota.clone());

        part page(per_page?: u32, page?: u32) {
            query { per_page; page; }
            paginate by_link();
        }
        part repo_order(sort?: RepoSort, direction?: Direction) {
            query { sort; direction; }
        }
        part at_ref(reference?: String) {
            query { "ref" = reference; }
        }

        GET rate_limit() -> RateLimits { path "rate_limit"; }
        GET repository(id: u64) -> Repository { path "repositories/{id}"; }

        scope user {
            path "user";
            GET get() -> User;
            /// kind cannot accompany visibility or affiliation.
            GET repos(
                visibility?: Visibility, affiliation?: Vec<String>,
                kind?: UserRepoType, since?: String, before?: String,
                ..repo_order, ..page
            ) -> Vec<Repository> {
                path "repos";
                query {
                    visibility;
                    "affiliation" = affiliation.as_ref().map(|v| v.join(","));
                    "type" = checked_repo_kind(kind, visibility, affiliation)?;
                    since; before;
                }
            }
            POST create_repo(repo: NewRepository) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        scope users(username: String) {
            path "users/{username}";
            GET get() -> User;
            GET repos(kind?: PublicRepoType, ..repo_order, ..page) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; }
            }
        }

        scope orgs(org: String) {
            path "orgs/{org}";
            GET repos(kind?: OrgRepoType, ..repo_order, ..page) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; }
            }
            POST create_repo(repo: NewRepository) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        scope repo(owner: String, name: String) {
            path "repos/{owner}/{name}";
            /// A renamed repository's 301 remains an error with Location; see get_with_moves.
            GET get() -> Repository;
            /// Compatibility alias for this inventory; get().send_if_none_match(etag) also works.
            GET get_if_changed(etag: String) -> Option<Repository> {
                headers { "if-none-match" = etag; }
                status { 304 => None }
            }
            /// None means calculation pending, not an absent repository.
            GET contributor_stats() -> Option<Vec<ContributorStats>> {
                path "stats/contributors";
                status { 202 => None, 204 => Some(Vec::new()) }
            }
            /// A root directory is not a paginated list.
            GET contents_root(..at_ref) -> Vec<ContentEntry> { path "contents"; }

            scope issues {
                path "issues";
                /// Includes pull requests. milestone/assignee/kind accept "*" and "none".
                /// since is UTC ISO 8601; labels is an AND filter.
                GET list(
                    milestone?: String, state?: IssueState, assignee?: String,
                    kind?: String, creator?: String, mentioned?: String,
                    issue_field_values?: String, labels?: Vec<String>,
                    sort?: IssueSort, direction?: Direction, since?: String,
                    ..page
                ) -> Vec<Issue> {
                    query {
                        milestone; state; assignee; "type" = kind; creator; mentioned;
                        issue_field_values;
                        "labels" = labels.as_ref().map(|v| v.join(","));
                        sort; direction; since;
                    }
                }
                POST create(issue: NewIssue) -> Issue { body json(issue); }

                scope comment(id: u64) {
                    path "comments/{id}";
                    GET get() -> Comment;
                    PATCH update(body: String) -> Comment {
                        body json(serde_json::json!({ "body": body }));
                    }
                    DELETE delete();
                }

                scope issue(number: u64) {
                    path "{number}";
                    GET get() -> Issue;
                    PATCH update(changes: EditIssue) -> Issue { body json(changes); }
                    GET comments(since?: String, ..page) -> Vec<Comment> {
                        path "comments";
                        query { since; }
                    }
                    POST create_comment(body: String) -> Comment {
                        path "comments";
                        body json(serde_json::json!({ "body": body }));
                    }
                }
            }

            scope pulls {
                path "pulls";
                GET list(
                    state?: IssueState, head?: String, base?: String,
                    sort?: PullSort, direction?: Direction, ..page
                ) -> Vec<Pull> {
                    query { state; head; base; sort; direction; }
                }
                POST create(pull: NewPull) -> Pull { body json(pull); }

                scope pull(number: u64) {
                    path "{number}";
                    GET get() -> Pull;
                    GET diff() -> Bytes {
                        headers { override "accept" = "application/vnd.github.diff"; }
                        read Binary;
                    }
                    GET files(..page) -> Vec<PullFile> { path "files"; }
                    /// 404 here is GitHub's negative answer; inaccessible resources can also hide.
                    GET is_merged() -> bool {
                        path "merge";
                        status { 204 => true, 404 => false }
                    }
                    PUT merge(options: MergePull) -> MergeResult {
                        path "merge";
                        body json(options);
                        idempotent false;
                    }
                }
            }

            scope contents(path: String) {
                path "contents/{*path}";
                GET get(..at_ref) -> Contents;
                /// Files, including symlinks resolved to files; bytes need not be UTF-8.
                GET raw(..at_ref) -> Bytes {
                    headers { override "accept" = "application/vnd.github.raw+json"; }
                    read Binary.limit(100 * concord::MIB);
                }
                /// content is base64; sha is required when replacing an existing file.
                PUT create_or_update(change: PutContent) -> ContentWrite {
                    body json(change);
                    idempotent false;
                }
            }

            scope releases {
                path "releases";
                GET list(..page) -> Vec<Release>;
                POST create(release: NewRelease) -> Release { body json(release); }
                GET latest() -> Release { path "latest"; }
                GET by_tag(tag: String) -> Release { path "tags/{tag}"; }

                scope release(id: u64) {
                    path "{id}";
                    GET get() -> Release;
                    DELETE delete();
                    GET assets(..page) -> Vec<Asset> { path "assets"; }
                    /// Body carries its media type; size also permits a known-length stream.
                    POST asset(
                        name: String, file: concord::Body, size: u64, label?: String
                    ) -> Asset {
                        override base "https://uploads.github.com";
                        path "assets";
                        headers {
                            override "authorization" = "Bearer {client.token}";
                            "content-length" = size;
                        }
                        query { name; label; }
                        body file;
                    }
                }
            }
        }

        scope search {
            path "search";
            /// q is unencoded GitHub search text. Sorting words belong to GitHub.
            GET repositories(q: String, sort?: String, order?: Direction, ..page)
                -> SearchPage<Repository> {
                path "repositories";
                query { q; sort; order; }
            }
            GET issues(q: String, sort?: String, order?: Direction, ..page)
                -> SearchPage<Issue> {
                path "issues";
                query { q; sort; order; }
            }
        }
    }
}
fn main() {}

include!("../../support/models.rs");
