concord::api! {
    /// The GitHub REST API on github.com, version `2022-11-28`: repositories, issues,
    /// pull requests, contents, releases, search, the rate limit.
    ///
    /// GitHub refuses a request with no `User-Agent`, so `new` asks for your program's name.
    /// With no token GitHub allows 60 requests an hour, with one 5 000.
    /// For a program that runs for long, build the client with [`Github::watched`] (it keeps
    /// the quotas) and `.with_retry(retry())` (GitHub's own "later").
    pub client Github(
        /// Sent as `User-Agent` on every request: `"my-tool/1.0"`.
        user_agent: String,
        /// A personal access token, or an app's installation token. Sent as a bearer.
        token?: Secret,
    ) {
        base "https://api.github.com";
        headers {
            "accept" = "application/vnd.github+json";
            "x-github-api-version" = "2022-11-28";
            "user-agent" = user_agent;
            "authorization" = "Bearer {token}";
        }
        read Json;
        problem GitHubProblem;

        /// The quotas of the token (or of your address, with no token). It spends none.
        GET rate_limit() -> RateLimits { path "rate_limit"; }

        /// A repository by its id. This is where the `301` of a renamed repository points
        /// (see [`Github::find_repo`]). (unsure: GitHub's reference does not list it)
        GET repository(id: u64) -> Repository { path "repositories/{id}"; }

        /// The user that the token belongs to.
        scope user {
            path "user";

            GET get() -> User;

            /// Repositories that the user can reach. GitHub refuses `kind` together with
            /// `visibility` (`422`).
            GET repos(
                visibility?: Visibility,
                /// Sent as `type`. `all`, `owner`, `public`, `private`, `member`.
                kind?: RepoType,
                sort?: RepoSort,
                direction?: Direction,
                /// At most 100.
                per_page?: u8,
                page?: u32,
            ) -> Vec<Repository> {
                path "repos";
                query { visibility; "type" = kind; sort; direction; per_page; page; }
                paginate link();
            }

            /// A new repository, owned by the user.
            POST create_repo(repo: NewRepo) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        /// Another user, by login.
        scope users(username: String) {
            path "users/{username}";

            GET get() -> User;

            /// The user's public repositories. `kind` is sent as `type`: `all`, `owner`, `member`.
            GET repos(kind?: RepoType, sort?: RepoSort, direction?: Direction,
                      per_page?: u8, page?: u32) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; sort; direction; per_page; page; }
                paginate link();
            }
        }

        /// An organisation, by login.
        scope orgs(org: String) {
            path "orgs/{org}";

            /// The organisation's repositories that the token may see. `kind` is sent as
            /// `type`: `all`, `public`, `private`, `forks`, `sources`, `member`.
            GET repos(kind?: RepoType, sort?: RepoSort, direction?: Direction,
                      per_page?: u8, page?: u32) -> Vec<Repository> {
                path "repos";
                query { "type" = kind; sort; direction; per_page; page; }
                paginate link();
            }

            /// A new repository in the organisation.
            POST create_repo(repo: NewRepo) -> Repository {
                path "repos";
                body json(repo);
            }
        }

        /// One repository. A `404` here means "no such repository" or "not one the token may
        /// see": GitHub does not say which. Under an old name, every endpoint below answers
        /// `301`: the error's `problem().moved_to()` has the new id (see [`Github::find_repo`]).
        scope repo(owner: String, name: String) {
            path "repos/{owner}/{name}";

            GET get() -> Repository;

            /// The repository, or `None` when it has not changed since `etag` (the `etag` of an
            /// earlier answer). A `304` spends no quota (unsure: only with a token).
            GET get_if_changed(etag?: String) -> Option<Tagged<Repository>> {
                headers { "if-none-match" = etag; }
                status {
                    304 => None,
                    200 { body, headers } => Some(Tagged::new(headers.get("etag"), body)),
                }
            }

            /// Commits, additions and deletions of each contributor, week by week. GitHub
            /// computes them on the first ask and answers `202` meanwhile: `Stats::Computing`.
            /// [`github::Repo::contributor_stats_ready`] asks again for you.
            GET contributor_stats() -> Stats<Vec<ContributorStats>> {
                path "stats/contributors";
                status {
                    200 { body } => Stats::Ready(body),
                    202 => Stats::Computing,
                    204 => Stats::Ready(Vec::new()),      // an empty repository (unsure)
                }
            }

            /// The entries at the root of the repository. (`contents` takes a path, and an
            /// empty path cannot be sent.)
            GET contents_root(
                /// A branch, a tag or a commit. The default branch when not given.
                reference?: String,
            ) -> Vec<Entry> {
                path "contents";
                query { "ref" = reference; }
            }

            /// The issues of the repository, and its issue comments.
            scope issues {
                path "issues";

                /// Issues, newest first. GitHub counts a pull request as an issue: those come
                /// too, with `pull_request` set.
                GET list(
                    /// A milestone number, `Filter::Any` or `Filter::Without`.
                    milestone?: Filter<u64>,
                    /// GitHub gives `open` when this is not sent.
                    state?: StateFilter,
                    /// A login, `Filter::Any` or `Filter::Without`.
                    assignee?: Filter<String>,
                    /// The login of who opened it.
                    creator?: String,
                    /// A login that is mentioned in it.
                    mentioned?: String,
                    /// Issues that have every one of these labels.
                    labels?: Vec<String>,
                    /// Issues updated at or after this time.
                    since?: DateTime<Utc>,
                    sort?: IssueSort,
                    direction?: Direction,
                    per_page?: u8,
                    page?: u32,
                ) -> Vec<Issue> {
                    query {
                        milestone; state; assignee; creator; mentioned;
                        "labels" = labels.as_ref().map(|l| l.join(","));
                        since; sort; direction; per_page; page;
                    }
                    paginate link();
                }

                /// A new issue. Labels, assignees and a milestone need push access; without it
                /// GitHub leaves them out and still creates the issue (unsure).
                POST create(title: String, body?: String, labels?: Vec<String>,
                            assignees?: Vec<String>, milestone?: u64) -> Issue {
                    body compact(json!({
                        "title": title, "body": body, "labels": labels,
                        "assignees": assignees, "milestone": milestone
                    }));
                }

                /// One comment, by its id (the ids run across the whole repository).
                scope comment(id: u64) {
                    path "comments/{id}";

                    GET get() -> Comment;

                    PATCH update(body: String) -> Comment {
                        body json(json!({ "body": body }));
                        idempotent;                    // the text is set, not added to
                    }

                    DELETE delete();
                }
            }

            /// One issue (or pull request), by its number.
            scope issue(number: u64) {
                path "issues/{number}";

                GET get() -> Issue;

                /// Sets the fields that `changes` holds; the others stay as they are.
                PATCH update(changes: IssueUpdate) -> Issue {
                    body json(changes);
                    idempotent;                        // every field is set, none is added to
                }

                GET comments(
                    /// Comments updated at or after this time.
                    since?: DateTime<Utc>,
                    per_page?: u8,
                    page?: u32,
                ) -> Vec<Comment> {
                    path "comments";
                    query { since; per_page; page; }
                    paginate link();
                }

                POST create_comment(body: String) -> Comment {
                    path "comments";
                    body json(json!({ "body": body }));
                }
            }

            /// The pull requests of the repository.
            scope pulls {
                path "pulls";

                /// `head` is `user:branch`; `base` is the branch that they want to merge into.
                GET list(state?: StateFilter, head?: String, base?: String,
                         sort?: PullSort, direction?: Direction,
                         per_page?: u8, page?: u32) -> Vec<PullRequest> {
                    query { state; head; base; sort; direction; per_page; page; }
                    paginate link();
                }

                /// A new pull request: `head` into `base`.
                POST create(title: String, head: String, base: String,
                            body?: String, draft?: bool) -> PullRequest {
                    body compact(json!({
                        "title": title, "head": head, "base": base, "body": body, "draft": draft
                    }));
                }
            }

            /// One pull request, by its number.
            scope pull(number: u64) {
                path "pulls/{number}";

                GET get() -> PullRequest;

                /// The diff as GitHub writes it. Bytes, not text: a changed file that is not
                /// UTF-8 is in the diff as it is.
                GET diff() -> Bytes {
                    headers { override "accept" = "application/vnd.github.diff"; }
                    read Binary;
                }

                /// The changed files, at most 3 000.
                GET files(per_page?: u8, page?: u32) -> Vec<DiffEntry> {
                    path "files";
                    query { per_page; page; }
                    paginate link();
                }

                /// `false` also when there is no such pull request: GitHub answers `404` to both.
                GET is_merged() -> bool {
                    path "merge";
                    status { 2xx => true, 404 => false }
                }

                /// Merge it. With `sha`, only if the head is still that commit.
                PUT merge(commit_title?: String, commit_message?: String,
                          sha?: String, merge_method?: MergeMethod)
                    -> Result<Merged, MergeRefused> {
                    path "merge";
                    body compact(json!({
                        "commit_title": commit_title, "commit_message": commit_message,
                        "sha": sha, "merge_method": merge_method
                    }));
                    idempotent false;      // sent again after a merge that happened, it answers 405
                    status {
                        200 { body } => Ok(body),
                        405 { body } => Err(MergeRefused::NotMergeable(body)),
                        409 { body } => Err(MergeRefused::HeadMoved(body)),
                    }
                }
            }

            /// A file or a directory, by its path in the repository (`src/lib.rs`).
            scope contents(path: String) {
                path "contents/{*path}";

                /// The file (its bytes in base64) or the entries of a directory.
                /// `None`: no such path on that reference.
                GET get(
                    /// A branch, a tag or a commit. The default branch when not given.
                    reference?: String,
                ) -> Option<Contents> {
                    query { "ref" = reference; }
                    status { 404 => None }
                }

                /// The bytes of the file, as they are. Up to 100 MB, GitHub's limit for this
                /// form (unsure); past 1 MB the JSON form above has no content.
                GET raw(reference?: String) -> Bytes {
                    query { "ref" = reference; }
                    headers { override "accept" = "application/vnd.github.raw+json"; }
                    read Binary.limit(100 * concord::MIB);
                }

                /// Create the file, or replace it. Replacing needs `sha`, the blob that it
                /// replaces: when someone wrote first, GitHub answers `409`.
                PUT create_or_update(
                    message: String,
                    /// The new bytes. Sent in base64, as GitHub wants.
                    content: Vec<u8>,
                    sha?: String,
                    /// The default branch when not given.
                    branch?: String,
                ) -> FileCommit {
                    body compact(json!({
                        "message": message, "content": STANDARD.encode(content),
                        "sha": sha, "branch": branch
                    }));
                    idempotent false;      // sent again after a write that happened: 409 or 422
                }
            }

            /// The releases of the repository.
            scope releases {
                path "releases";

                /// Published releases, and drafts for a token with push access.
                GET list(per_page?: u8, page?: u32) -> Vec<Release> {
                    query { per_page; page; }
                    paginate link();
                }

                POST create(release: NewRelease) -> Release { body json(release); }

                /// The latest published release that is not a pre-release.
                GET latest() -> Release { path "latest"; }

                /// A tag may hold `/` (`release/1.0`). (unsure: that GitHub reads it unencoded)
                GET by_tag(tag: String) -> Release { path "tags/{*tag}"; }
            }

            /// One release, by its id.
            scope release(id: u64) {
                path "releases/{id}";

                GET get() -> Release;

                DELETE delete();

                GET assets(per_page?: u8, page?: u32) -> Vec<Asset> {
                    path "assets";
                    query { per_page; page; }
                    paginate link();
                }

                /// GitHub's upload host.
                scope upload {
                    base "https://uploads.github.com";

                    /// Attach a file to the release. `file` carries its media type
                    /// (`Body::new(bytes, "application/zip")`) and needs a known length (unsure).
                    /// The time to send it counts as silence: give a large file
                    /// `.with_idle_timeout(…)`. A name that is taken: `422`.
                    POST asset(name: String, label?: String, file: concord::Body) -> Asset {
                        path "assets";
                        query { name; label; }
                        body file;
                    }
                }
            }
        }

        /// Search. It has its own quota: 30 requests a minute with a token.
        scope search {
            path "search";

            /// Repositories that match `q`, in GitHub's search syntax
            /// (`"language:rust stars:>1000"`). At most 1 000 results in all.
            GET repositories(q: String, sort?: RepoSearchSort, order?: Direction,
                             per_page?: u8, page?: u32) -> SearchResults<Repository> {
                path "repositories";
                query { q; sort; order; per_page; page; }
                paginate link();
            }

            /// Issues and pull requests that match `q` (`"repo:rust-lang/rust is:open label:C-bug"`).
            GET issues(q: String, sort?: IssueSearchSort, order?: Direction,
                       per_page?: u8, page?: u32) -> SearchResults<Issue> {
                path "issues";
                query { q; sort; order; per_page; page; }
                paginate link();
            }
        }
    }
}
