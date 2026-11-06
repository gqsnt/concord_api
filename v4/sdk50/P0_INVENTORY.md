# P0 inventory — obligations and residue across the existing corpus

Source analysis by an independent read-only session (Codex `gpt-6.1-sol`, requested xhigh; observed `gpt-6.1-sol`/`xhigh` in the rollout turn context; thread `01a1b2ec-21e0-7502-813d-b7a358cfeb20`), 2026-11-06 23:13–23:23 CET, on checkout `5c3ef5f6` (SDK20 head `90b69011` + records + the registry fix). Brief: `v4/sdk50/roles/p0-inventory.md`. Nothing was compiled or run by that session. The coordinator's reconciliation is in `DECISIONS.md`; the stale-documentation list (§5) was corrected by the P0 docs lane. Kept verbatim below (paths were relative to `examples/<sdk>/src/` at that checkout).

**SDK20 removed substantial execution and representation plumbing, but SDK authors still assemble response policies, pagination adapters, continuation bindings, replay factories, and completion wrappers.** Much of that surviving control flow is generic mechanics; provider schemas, continuation meaning, signing rules, and terminal-state interpretation remain provider semantics.

This is a read-only inventory of checkout `5c3ef5f6`: SDK20 head `90b69011`, its records successor, and the registry lifetime fix. I read the applicable `AGENTS.md`/`CLAUDE.md`, current source, then the records. No files were changed; nothing was compiled, built, tested, or called against a provider.

**1. Obligation × SDK matrix**

Notation:

| Mark | Meaning |
|---|---|
| `D:key` | Direct declaration/generated surface; source resolves through the SDK index below. |
| `H:Name` | Existing shared helper, named explicitly. SDK adaptation around it is inventoried separately. |
| `A:key` | Handwritten SDK adapter; source resolves through the index. |
| `N:key` | Actual native escape in implementation source. |
| `U` | Relevant surface is absent, excluded, or deliberately not adopted; does **not** imply Concord lacks the underlying capability. |
| `-` | Not applicable to the bounded slice. |
| `†` | Partial retention/binding/coverage; qualifications follow the tables. |
| `ᴮ` | Raw binary streaming, rather than SSE/NDJSON/CSV/watch. |

Each `SDK:key` reference below expands to the concrete `file:symbol` in the source index. Shared helpers are `concord::ResponseMeta` (`RM`), `concord_pagination::Pages` (`Pages`), `concord_pagination::independent` (`independent`), and the named formats/readers in `concord-extras`. Pagination rows classify the handwritten provider adaptation; the separate scheduler row records shared-helper adoption.

“Completion beyond EOF” includes provider page/job completion as well as stream completion. “Cursor/token” includes opaque fields named `offset`; numeric offsets have their own row. Native escapes count implementation source, not a caller recovering an unread status response in a test.

| Obligation | SDK01 NH | SDK02 PK | SDK03 TW | SDK04 S3 | SDK05 FF |
|---|---|---|---|---|---|
| Typed provider error body | U | U | A:e | A:e | A:e |
| Metadata on success | A:m | A:m† | A:m | A:m† | A:m |
| Metadata on failure | A:m | A:m† | A:m | A:m† | A:m |
| HTTP-200 application errors | - | - | A:f | A:f | - |
| Status-selected success documents | - | - | - | - | - |
| Link header parsing | - | - | - | - | - |
| Body next-URL pagination | - | A:p | U | - | - |
| Cursor/token pagination | - | - | U | A:p | - |
| Numeric offset pagination | - | D:d | - | - | - |
| Page-number/index pagination | - | - | - | - | - |
| Traversal scheduler | - | A:p, caller loop | - | H:Pages | - |
| Continuation context binding | - | A:p† | - | A:p† | - |
| Cycle/did-not-advance guards | - | U | - | A:p† | - |
| Retry-After/rate headers | H:RM, raw | A:m, raw | H:RM, raw | U | H:RM, raw |
| Static credential header | - | - | D:d | - | - |
| Basic auth encoding | - | - | A:a | - | - |
| Bearer acquisition/refresh/cache | - | - | - | - | - |
| Request signing | - | - | - | A:a | - |
| Replay/idempotency keys | - | - | U | U | - |
| Challenge-and-replay | - | - | - | - | - |
| Runtime request media | - | - | - | D:d | - |
| Lazy/one-shot uploads | - | - | - | U | - |
| Finite non-JSON formats | - | - | H:Form, request | A:f | - |
| Incremental body streams | - | - | - | A:sᴮ | - |
| Completion beyond EOF | - | A:p | A:f | A:s/p | - |
| Head-only/head-derived outputs | - | - | - | A:h | - |
| Native escape use | - | - | - | N:n | - |
| `send X;` / `SendWith` | D:y | D:y | U | D:y | D:y |
| Ordinary builder/wrapper surface | D:d | D:d | A:w | D:d | D:d |

| Obligation | SDK06 GL | SDK07 SL | SDK08 OA | SDK09 NT | SDK10 LI |
|---|---|---|---|---|---|
| Typed provider error body | A:e | A:e | A:e | A:e | A:e |
| Metadata on success | A:m | A:m | A:m | U | A:m |
| Metadata on failure | A:m | A:m | A:m | A:m† | A:m |
| HTTP-200 application errors | - | A:e | A:s/f | - | A:e |
| Status-selected success documents | - | - | - | - | - |
| Link header parsing | A:p | - | - | - | - |
| Body next-URL pagination | - | - | - | - | - |
| Cursor/token pagination | - | A:p | - | - | A:p |
| Numeric offset pagination | - | - | - | - | - |
| Page-number/index pagination | - | - | - | - | - |
| Traversal scheduler | H:Pages | H:Pages | - | - | H:Pages |
| Continuation context binding | A:p† | A:p† | - | A:p† | A:p† |
| Cycle/did-not-advance guards | A:p† | U | - | - | A:p† |
| Retry-After/rate headers | A:r | A:r | H:RM, raw | A:r | H:RM, raw |
| Static credential header | D:d | D:d | D:d | D:d | D:d |
| Basic auth encoding | - | - | - | U | - |
| Bearer acquisition/refresh/cache | U | U | U | U | U |
| Request signing | - | - | - | - | - |
| Replay/idempotency keys | - | - | U | U | - |
| Challenge-and-replay | - | - | - | - | - |
| Runtime request media | - | - | H:Multipart | - | - |
| Lazy/one-shot uploads | - | - | U | U | - |
| Finite non-JSON formats | - | - | A:f | A:f, request | - |
| Incremental body streams | - | - | A:s | A:s | - |
| Completion beyond EOF | A:p | A:p | A:s/f | A:s | A:p/e |
| Head-only/head-derived outputs | - | - | - | - | - |
| Native escape use | - | - | - | - | - |
| `send X;` / `SendWith` | D:y | U | D:y | D:y | U |
| Ordinary builder/wrapper surface | D:d | A:w | A:w | D:d | A:w |

GitLab follows authoritative Link URLs containing the provider’s pagination parameters; it does not expose a numeric-page construction strategy.

| Obligation | SDK11 DB | SDK12 GD | SDK13 OM | SDK14 AT | SDK15 CH |
|---|---|---|---|---|---|
| Typed provider error body | A:e | A:e | A:e | A:e | A:e, text |
| Metadata on success | A:m | A:m | A:m | A:m | A:m† |
| Metadata on failure | A:m | A:m | A:m | A:m | A:m† |
| HTTP-200 application errors | - | - | - | - | A:s/e |
| Status-selected success documents | - | A:u, 308/200/201 | - | - | - |
| Link header parsing | - | - | - | - | - |
| Body next-URL pagination | - | - | - | - | - |
| Cursor/token pagination | A:p | A:p | - | A:p, opaque offset | - |
| Numeric offset pagination | - | - | - | - | - |
| Page-number/index pagination | - | - | - | - | - |
| Traversal scheduler | H:Pages | H:Pages | - | H:Pages | - |
| Continuation context binding | A:p† | A:p† | - | A:p† | - |
| Cycle/did-not-advance guards | U | A:p† | - | U | - |
| Retry-After/rate headers | A:r | H:RM, raw | H:RM, raw | H:RM, raw | - |
| Static credential header | D:d | D:d | - | D:d | D:d |
| Basic auth encoding | - | - | - | - | A:a |
| Bearer acquisition/refresh/cache | U | U | - | U | - |
| Request signing | - | - | - | - | - |
| Replay/idempotency keys | - | U | - | - | U |
| Challenge-and-replay | - | - | - | - | - |
| Runtime request media | - | H:Multipart | - | - | H:Upload/Encoded |
| Lazy/one-shot uploads | U | U | - | - | H:CsvWriter/JsonLinesWriter |
| Finite non-JSON formats | - | - | A:f, CSV | - | H:Text; A:e |
| Incremental body streams | - | - | U† | - | A:s |
| Completion beyond EOF | A:p | A:p/u | - | A:p | A:s |
| Head-only/head-derived outputs | - | A:u† | - | - | - |
| Native escape use | - | - | - | - | N:n |
| `send X;` / `SendWith` | D:y | U | U | D:y | U |
| Ordinary builder/wrapper surface | A:w | A:w | A:w | A:w | A:w |

Open-Meteo declares bare `-> stream` to obtain `Incoming`, then buffers and validates the entire JSON/CSV forecast. That is runtime response selection, not a public incremental forecast API.

| Obligation | SDK16 SN | SDK17 WB | SDK18 NO | SDK19 MG | SDK20 DR |
|---|---|---|---|---|---|
| Typed provider error body | A:e | A:e | A:e | A:e | A:e |
| Metadata on success | A:m | A:m | A:m | A:m | A:m |
| Metadata on failure | A:m | A:m | A:m | A:m | A:m |
| HTTP-200 application errors | A:e/p | A:e | A:e | - | - |
| Status-selected success documents | A:f, 200/202 | - | - | - | - |
| Link header parsing | A:p | - | - | - | A:p |
| Body next-URL pagination | - | - | - | A:p | - |
| Cursor/token pagination | - | - | A:p | - | - |
| Numeric offset pagination | - | - | - | - | - |
| Page-number/index pagination | A:p, partitions | A:p | - | - | - |
| Traversal scheduler | H:independent | H:Pages | H:Pages | H:Pages | H:Pages |
| Continuation context binding | A:p | A:p† | A:p† | A:p | A:p† |
| Cycle/did-not-advance guards | A:p† | A:p | A:p | A:p† | A:p† |
| Retry-After/rate headers | H:RM, raw | H:RM, raw | H:RM, raw | H:RM, raw | H:RM, raw |
| Static credential header | D:d | - | D:d | D:d | D:d |
| Basic auth encoding | - | - | - | - | A:a |
| Bearer acquisition/refresh/cache | U | - | U | U | A:a† |
| Request signing | U, caller JWT | - | - | - | - |
| Replay/idempotency keys | D:d, requestId† | - | - | - | A:a/u† |
| Challenge-and-replay | - | - | - | - | A:a |
| Runtime request media | - | - | - | - | H:EncodedBody |
| Lazy/one-shot uploads | - | - | - | - | U |
| Finite non-JSON formats | - | U, XML excluded | - | - | H:Binary, exact bytes |
| Incremental body streams | U, finite partitions | - | - | - | A:sᴮ |
| Completion beyond EOF | A:p | A:p | A:p/e | A:p | A:s/u |
| Head-only/head-derived outputs | - | - | - | - | A:h |
| Native escape use | - | - | - | - | - |
| `send X;` / `SendWith` | U | D:y | U | U | U |
| Ordinary builder/wrapper surface | A:w | D:d | A:w | A:w | A:w |

The older controls remain separate:

| Obligation | GH control | ST control | SH control | SF control | KU control |
|---|---|---|---|---|---|
| Typed provider error body | U | A:e | A:e†, GraphQL | A:e | A:e |
| Metadata on success | A:m† | A:m† | U† | A:m† | U† |
| Metadata on failure | A:m† | A:m† | U† | A:e† | A:e† |
| HTTP-200 application errors | - | - | A:e | - | A:s, watch ERROR |
| Status-selected success documents | A:h, includes 304 | - | - | - | - |
| Link header parsing | A:p | - | - | - | - |
| Body next-URL pagination | - | - | - | A:p | - |
| Cursor/token pagination | - | A:p | A:p | A:p† | A:p |
| Numeric offset pagination | - | - | - | - | - |
| Page-number/index pagination | U, Link instead | - | - | - | - |
| Traversal scheduler | H:Pages/sequential | H:Pages | H:sequential | H:Pages/manifest_jobs/independent | H:sequential |
| Continuation context binding | A:p† | A:p† | U | A:p† | A:p† |
| Cycle/did-not-advance guards | A:p† | A:p† | A:p† | A:p† | A:p† |
| Retry-After/rate headers | A:r | A:r | -† | A:r† | -† |
| Static credential header | D:d | D:d | D:d | D:d | D:d |
| Basic auth encoding | - | - | - | - | - |
| Bearer acquisition/refresh/cache | U | U | U | U | U |
| Request signing | - | - | - | - | - |
| Replay/idempotency keys | - | D:d† | - | - | - |
| Challenge-and-replay | - | - | - | - | - |
| Runtime request media/dialect | - | - | - | A:u | - |
| Lazy/one-shot uploads | - | - | - | A:u†, pre-spooled | - |
| Finite non-JSON formats | - | A:f, form request | - | H:CsvWriter/CsvReader | - |
| Incremental body streams | A:sᴮ | - | H:JsonLinesReader | A:s | D:d, JsonStream |
| Completion beyond EOF | - | - | A:s, app | A:s | A:s, app |
| Head-only/head-derived outputs | A:h | - | - | - | - |
| Native escape use | N:n | - | N:n | N:n | N:n |
| `send X;` / `SendWith` | U | U | U | U | U |
| Ordinary builder/wrapper surface | A:w | D:d† | A:w | A:w | A:w |

Concrete source index—paths are `examples/<directory>/src/` unless a test/app path is stated:

| SDK code / directory | Source keys used above |
|---|---|
| **NH** `nager-holidays-sdk` | `d` `lib.rs:NagerHolidays`; `m/y` `lib.rs:KeepHead::send`; error wrapper `error.rs:Error`. |
| **PK** `pokeapi-sdk` | `d` `lib.rs:PokeApi`; `m` `lib.rs:finish`; `y` `lib.rs:KeepHead::send`; `p` `lib.rs:PokeApi::{follow,pokemon_url}`, `checked_url`, `PokemonPage`; caller traversal `tests/first_use.rs:select_named`. |
| **TW** `twilio-sdk` | `d` `lib.rs:wire::TwilioApi`; `e` `error.rs:read_error`; `m` `error.rs:send`; `a` `auth.rs:BasicAuth::new`; `f` `models.rs:{Message,MessageStatus}`; `w` `lib.rs:Twilio::{create_message,get_message,prepare_create_message,prepare_get_message}`. |
| **S3** `amazon-s3-sdk` | `d/y` `api.rs:S3Api`, `send signer`; `a` `client.rs:Signer::sign`, `sigv4.rs:{canonicalize_target,sign_request}`; `e` `error.rs:ApiError::read`; `m` `client.rs:finish`, `error.rs:ResponseMeta`; `p` `client.rs:S3ApiBucketListObjectsV2::pages`; `f` `xml.rs:{ListXml,parse_listing,parse_error_document}`; `s` `object.rs:{Download,ObjectBody}`; `h` `object.rs:{HeadFormat,PutFormat}`; `n` `Signer::sign`, `SignedRequest::receive`, `Download::open`. |
| **FF** `frankfurter-sdk` | `d` `lib.rs:Frankfurter`; `e/m/y` `lib.rs:SendPolicy::send`; provider/error outcomes `error.rs:{Problem,ProviderError,ErrorBodyError}`. |
| **GL** `gitlab-sdk` | `d/y` `api.rs:GitLab`, `send SendPolicy`; `e/m` `send.rs:{finish,decode,api_error}`, `meta.rs:ResponseMeta::capture`; `r` `meta.rs:ResponseMeta::from_head`; `p` `link.rs:{parse,parse_str}`, `pages.rs:{NextPage,fetch,fetch_prepared,check_link,next_link,pages_from}`. |
| **SL** `slack-sdk` | `d` `wire.rs:SlackApi`; `e/m` `lib.rs:execute`, `error.rs:{ErrorBody::new,Envelope::into_provider}`; `r` `error.rs:ResponseMeta::retry_after`; `p` `lib.rs:Client::{users_page,users}`, `models.rs:{Cursor,cursor}`; `w` `lib.rs:Client`. |
| **OA** `openai-sdk` | `d/y` `lib.rs:OpenAiApi`, `SendPolicy::send`; `e/m` `lib.rs:checked`, `SendPolicy::send`, `content`; `f` `lib.rs:{batch_jsonl,parse_batch_jsonl,BatchResults,BatchStatus}`; `s` `lib.rs:{OpenAi::stream_response,EventStream::next}` over `SseReader`; `u` `OpenAi::upload_batch_file`; `w` `OpenAi`. |
| **NT** `ntfy-sdk` | `d/y` `lib.rs:Ntfy`, `error.rs:NtfyPolicy::send`; `e/m/r` `error.rs:ApiError::read`; `f` `model.rs:PlainText`; `s` `events.rs:{SseFormat,NdjsonFormat,Events,SseSource,EventsError,Body,LineEnds}`; `p` `events.rs:ReplayCursor`. |
| **LI** `linear-sdk` | `d` `lib.rs:LinearApi`; `e/m` `lib.rs:{send,Envelope,Response::is_success}`; `p` `lib.rs:Linear::{issue_page,issues}`, `PageMeta`; `w` `Linear`; custom request codec `Graphql::encode`. |
| **DB** `dropbox-sdk` | `d/y` `wire.rs:DropboxApi`, `DropboxPolicy::send`; `e` `error.rs:ErrorBody::new`; `m` `DropboxPolicy::send`, `error.rs:ResponseMeta`; `r` `ResponseMeta::retry_after`; `p` `lib.rs:{Client::list_folder_pages,bind_page}`, `models.rs:Cursor`; `w` `lib.rs:Client`. |
| **GD** `google-drive-sdk` | `d` `api.rs:DriveApi`; `e` `error.rs:Error::provider_response`; `m` `client.rs:DriveClient::{receive,finish}`; `p` `DriveClient::{files,changes,list_changes_page}`, `validate_next`, `model.rs:PageToken`; `u` `upload.rs:{DriveClient::upload_multipart,DriveClient::initiate_upload,UploadSession::send_chunk,UploadSession::probe,UploadSession::accept}`; `w` `DriveClient`, `UploadSession`. |
| **OM** `open-meteo-sdk` | `d` `lib.rs:ForecastApi`; `e/m/w` `lib.rs:OpenMeteo::forecast`, `error.rs:Error::response`; `f` `csv.rs:decode`. |
| **AT** `airtable-sdk` | `d/y` `lib.rs:AirtableApi`, `Policy::send`; `e/m` `Policy::send`; `p` `Client::{list_page,list_pages}`, `present_offset`; `w` `Client`. |
| **CH** `clickhouse-sdk` | `d` `wire.rs:ClickHouseHttp`; `a` `client.rs:Credentials::basic`; `e` `client.rs:{server_exception,head_exception}`; `m/s` `client.rs:open_rows`, `rows.rs:Rows::{failure,outcome,collect}`, `guard.rs:Guarded`, `insert.rs:Insert::send`; `u` `Insert::{json_each_row,csv_with_names}`; `n` `client.rs:execute`; `w` `ClickHouse`, `Select`, `Insert`, `Rows`. |
| **SN** `snowflake-sdk` | `d` `api.rs:SqlApi`; `e/m` `client.rs:Client::execute`, `Reply`, `error.rs:ServerError`; `f` `api.rs:StatementJson`, `compression.rs:{PartitionWire,PartitionBytes::decode}`; `p` `links.rs:{parse_str,partition_links,resolve,validate}`, `partitions.rs:{FirstResult,PartitionPlan,Partitions,PartitionReport}`, `polling.rs:Query::poll_from`; `w` `client.rs:{Client,Query}`. |
| **WB** `world-bank-sdk` | `d/y` `lib.rs:WorldBank`, `SendPolicy::send`; `e/m` `SendPolicy::send`, `WireResponse`, `IndicatorPage`; `p` `WorldBank::range`, `TraversalMeta`. |
| **NO** `notion-sdk` | `d` `api.rs:Notion`; `e/m` `client.rs:receive`, `Response`, `query_data_source`; `p` `client.rs:{query_pages_from,children_from,project,validate}`; `w` `NotionClient`. |
| **MG** `microsoft-graph-sdk` | `d` `api.rs:GraphApi`; `e/m` `paging.rs:Listing::fetch`, `PageMeta`, `error.rs:ErrorBody`; `p` `links.rs:{Binding,StoredLink::check,validate_url}`, `paging.rs:Listing::{new,fetch,pages_from}`; `w` `client.rs:GraphClient`, `Listing`. |
| **DR** `distribution-registry-sdk` | `d` `api.rs:{Registry,Repository,TokenService}`; `e` `error.rs:{api_error,expect_status}`; `m` `meta.rs:ResponseMeta`; `a` `client.rs:Registry::{execute,cached_token,fetch_token}`, `auth.rs:{BasicCredentials::new,base64,Challenge::bearer,TokenAuth,Access,CachedToken}`; `p` `pages.rs:{catalog_page,tag_page,checked_key,next_link,check_link,parse_next}`; `u` `manifest.rs:Repository::put_manifest`, `upload.rs:BlobUpload::{patch,status,complete}`; `s/h` `blob.rs:{Repository::blob_info,BlobReader::next_chunk}`; `w` `Registry`, `Repository`, `BlobUpload`. |
| **GH** `github-sdk` | `d` `api.rs:GitHub`; `e` `error.rs:GitHubError`; `m` `meta.rs:{send_with_meta,send_edited,capture,decode_with_meta}`; `r` `ResponseMeta::from_head`; `p` `link.rs:{parse,parse_str}`, `paging.rs:{Binding,LinkPages,fetch_page}`; `s` `download.rs:stream_to`; `h` `meta.rs:send_conditional`, `download.rs:locate`; `n` `send_edited`, `download.rs:{locate,stream_to}`; `w` `context.rs:{GitHubContext,RepoContext}`. |
| **ST** `stripe-sdk` | `d` `lib.rs:Stripe`, write-endpoint idempotency headers; `e/m/r` `send.rs:{send,read_document,ResponseMeta::from_head}`; `p` `pages.rs:{ListRequest,CursorList,project,fingerprint}`; `f` `form.rs:{StripeForm,to_pairs,encode_pairs}`; ordinary SDK terminal helper `send(prepared)`. |
| **SH** `shopify-sdk` | `d` `api.rs:ShopifyAdmin`; `e` `graphql.rs:{GraphqlResponse,GraphqlError}`; `m/w` `context.rs:ShopContext::send`; `p` `connection.rs:{PageInfo::next,connection_page}`; `s` `bulk_export.rs:{result_reader,open_result}` and `shopify-app/src/bulk_export.rs:{export_catalog,download}`; `n` `context.rs:ShopContext::result_download`. |
| **SF** `salesforce-sdk` | `d` `api.rs:SalesforceApi`; `e/n` `error.rs:{ApiFailure::read,SfError::from_concord,SfError::from_open}`; `m/r` `transport.rs:{send_with_usage,ApiUsage::from_headers}`; `p` `query.rs:{QueryPages::fetch,project}`, `bulk_query.rs:{ResultManifests::fetch,ResultSets::fetch}`, `context.rs:OrgContext` link validators; `u` `csv.rs:SalesforceCsv::{writer,spool,spool_stream}`; `s` `bulk_query.rs:{ResultPartReader,ResultPartRows}`, `ingest.rs:IngestResultRows`; `w` `OrgContext`, `QueryPages`, `BulkQueryJob`, `IngestJob`. |
| **KU** `kubernetes-sdk` | `d` `api.rs:Kube` and six watch declarations; `e/n` `error.rs:{ApiError::read,ApiError::from_response,read_bounded}`, `watch.rs:open`; `p` `list.rs:{ListOperation,Continuation,ListPager::fetch,into_page}`; `s` `watch.rs:{WatchOperation,WatchEvent}` and `kubernetes-app/src/list_watch.rs:list_watch`; `w` `context.rs:{KubeContext,NamespaceContext}`. |

Important qualifications:

| Area | Current truth |
|---|---|
| Metadata completeness | PK keeps status, all headers and final URL, but not version/operation as metadata fields. S3 projects status/request IDs/region and drops arbitrary headers. GH/ST also use provider projections. NT keeps narrow API-error information, not success heads. |
| Metadata losses | CH drops metadata on several `Error::Concord` paths after receiving a head. SF drops captured `ApiUsage` on failed decode/status handling. SH keeps GraphQL cost/errors but not an owned HTTP head; KU keeps list/watch body metadata and typed `Status`, not general HTTP headers. |
| Binding strength | MG binds stored links to explicit context/selection/path. DB binds member/root, not credentials. GH omits filters/page size/credential identity; ST omits origin/credential and uses `DefaultHasher`; KU binds snapshot only. |
| Closure capture versus exported checkpoint | SL, LI, GD, AT, WB and NO hold context inside the current factory, but exported raw keys do not enforce that context when restored. PK and DR constrain URL origin/collection without binding every request dimension. |
| Cycle protection | NO maintains seen-cursor sets. Most other guards reject only immediate repetition. DB/AT/SL rely on caps; PK leaves traversal policy to callers. `Pages` itself has no generic visited-key detection. |
| Replay | S3 conditional headers are supported, but are not a replay engine. SN `requestId` is caller supplied, without an automatic resubmit guarantee. DR replays one authentication challenge using buffered bodies. ST’s stable-key replay ledger lives in `stripe-app`. |
| Upload ownership | OA/GD multipart is buffered. GD/DR resumable uploads copy bounded chunks. S3 rejects streaming signing. SF consumes a one-shot source into a bounded known-length spool before dispatch; lazy Salesforce wire upload remains excluded. |
| Head outputs | S3 head-derived codecs still participate in the finite decoder lifecycle. GD initiation extracts `Location` and explicitly validates an empty body. No-output endpoints ordinarily drain; omission of `->` is not a head-only result contract. |
| Rates | SH throttle information and KU retry advice occur in bodies, not rate headers. Raw retention through `RM` is not parsing, admission, sleeping, or retrying. |

**2. Recurring generic residue**

References such as `TW:e` resolve to the exact source witnesses above. The witness sets below include the controls and relevant callers; they do not count historical implementations that have disappeared.

| Repeated mechanic and every identified witness | Shared control flow | Provider semantics / generic remainder / cost classification |
|---|---|---|
| **Capture head, consume body, attach metadata to both outcomes:** NH:m, PK:m, TW:m, S3:m, FF:m, GL:m, SL:m, OA:m, LI:m, DB:m, GD:m, OM:m, AT:m, CH:m, SN:m, WB:m, NO:m, MG:m, DR:m; partial SH:m, SF:m; GH:m, ST:m; `native/src/metadata.rs:{execute,send_with_meta,MetaError}`; `checks/maintenance-github/src/issue_events.rs:send` | Receive → capture/project head → decode → pair value or failure with captured metadata. | Which fields/results are exposed is **provider semantics**. Pairing, cloning, error propagation and safe formatting are **generic mechanics**. `Received`/`ResponseMeta` supply primitives; they do not supply a common SDK result envelope. |
| **Bounded typed non-success decoding:** TW:e, S3:e, FF:e, GL:e, SL:e, OA:e, NT:e, LI:e, DB:e, GD:e, OM:e, AT:e, CH:e, SN:e, WB:e, NO:e, MG:e, DR:e plus `DR client.rs:Registry::fetch_token`; ST:e, SF:e, KU:e | Status branch → retain head → check error representation → capped read → parse provider document → retain malformed/oversized/unreadable alternatives. | Schemas, authoritative statuses and classification are **provider semantics**. Consumption/outcome orchestration is **generic mechanics**, broader than merely writing schemas. SF/KU still handwrite capped chunk loops: existing-helper non-adoption. |
| **SDK result/error/accessor/redaction envelopes:** all `m/e` SDK entries above; NT `events.rs:EventsError`; `extension/src/error.rs:ExtensionError` | Define value+metadata wrapper, provider error, transport/decode variants, metadata accessors and redacted `Debug`/`Display`. | Provider-approved projections are semantic. Repeated forwarding and safe diagnostic structure are **generic mechanics**. |
| **Owned page factories and projection:** S3:p, GL:p, SL:p, LI:p, DB:p, GD:p, AT:p, WB:p, NO:p, MG:p, DR:p; SN `FirstResult::partitions`; GH `paging.rs:fetch_page`; ST `CursorList::fetch`; SF `QueryPages::fetch`; KU `ListPager::fetch`; SH `connection_page` | Capture client/options → clone into future → build operation → set/follow continuation → project `items,next,meta`. | Token placement/envelope meaning is semantic. Clone/future/boxing/projection scaffolding is **generic mechanics**. Shared schedulers do not remove this adapter. |
| **Continuation binding:** PK `checked_url`; S3 `.pages`; GL `NextPage`; DB `Cursor`/`bind_page`; MG `Binding`/`StoredLink`; DR `checked_key`; SN `Query`/`PartitionPlan`; GH `Binding`; ST `fingerprint`; SF `OrgContext` plus `salesforce-app/src/sf_a.rs:{QueryResume,bind_resume}`; KU `Continuation`; factory-only SL/LI/GD/AT/WB/NO | Freeze identity → carry it with a continuation or outside it → validate before dispatch. | Relevant tenant/query/snapshot dimensions are semantic. Carrier/check scaffolding is **generic mechanics**; unbound exported resumes are a **current API/SDK surface limitation**. |
| **Non-advance/cycle scaffolding:** S3:p, GL:p, LI:p, GD `validate_next`, NO `validate` and both `_from` HashSets, MG `Listing::fetch`, DR `next_link`, WB `range`; GH `fetch_page`, ST `project`, SH `PageInfo::next`, SF query/result projections, KU `into_page`; PK caller `select_named` | Validate terminal flag/key consistency → compare current and next → optionally maintain seen set/cap. | Meaning of `has_more`, next/delta and snapshot is semantic. Comparison/state/bounded-stop plumbing is **generic mechanics**. Caps do not establish cycle detection. |
| **Strict Link tokenization:** GH `link.rs:{parse,parse_str}`; GL same; SN `links.rs:{parse_str,partition_links}`; DR `pages.rs:parse_next` | Scan multiple fields, angle targets, quoted/escaped parameters and relations; resolve relative URLs; reject ambiguity. | Relation/path/index rules are semantic. Most tokenizer/resolution code is **generic mechanics**. SN maintenance explicitly copied/adapted the GH tokenizer. |
| **Server URL validation/following:** PK `checked_url`; GL `check_link`; SN `links.rs:{resolve,validate,partition_links}`, `Query::remember_status`; MG `StoredLink::check`/`validate_url`; DR `check_link`, `Registry::location`, `TokenAuth::check_realm`; SF `OrgContext` link validators; GH `LinkPages`/download validators | Parse/resolve → reject userinfo/fragment/foreign destination → enforce collection/job/path → follow unchanged. | Collection/job/trust rules are **provider semantics**. Repeated validation skeleton is **generic mechanics**. `Prepared::follow` already supplies same-origin replacement and redacted diagnostics; redirect policy remains a separate transport obligation. |
| **Retry-After/rate parsing:** GH `ResponseMeta::from_head`; GL `ResponseMeta::from_head`; SL `ResponseMeta::retry_after`; DB same; NT `ApiError::read`; `riot/src/lib.rs:Stop::from`; ST response-header interpretation; SF `ApiUsage::from_headers` | Read singleton/text/numeric fields → distinguish absent/invalid → produce delay/counters. | Header meanings are semantic. Strict parsing is **generic mechanics**. SL/DB reject repeated/negative/overflow forms; GL/NT are weaker decimal-only projections. No shared HTTP-date parser/admission policy is adopted. |
| **Credential preparation:** TW `BasicAuth::new`; CH `Credentials::basic`; DR `BasicCredentials::new`/handwritten `base64`; GL `PrivateToken`; OA `OpenAi::new`; NT `AccessToken::{new,header_value}`; LI `Linear::{authorization,api_key,oauth}`; GD `DriveClient::with_config`; MG `GraphClient::build`; SN `Client::new`; ST `StripeContext::new`; KU `BearerToken::header_value` | Validate raw value → encode/prefix → native sensitive header → generated constructor/setter. | Credential role/scheme is semantic; Base64/header validation/redaction is **generic mechanics**. Sealed `ToHeaderValue` preserves a deliberate invariant but leaves domain-newtype conversion outside declarations: **current API limitation/tradeoff**. |
| **Generated-operation registration/forwarding:** ST `pages.rs:{ListRequest,list_request!}`, `context.rs:{OnAccount,on_account!}`; KU `list.rs:{ListOperation,list_operation!}`, `watch.rs:{WatchOperation,watch_operation!}`; SF `QueryRecord`; GH `LinkPages::new` prepare function; TW:w | Register types and forward setters/prepare/send into reusable algorithms or public wrappers. | Common prepare forwarding is now **generic mechanics/non-adoption** because `Endpoint<T>` exists. Generic parameter setters/groups remain a **current API limitation**. Domain validation can still justify wrappers. |
| **Repeated declaration facts:** KU’s 13 list and six watch parameter blocks; ST’s account-override blocks; GH filters/context mappings; SF six typed/dynamic query declarations | Repeat one parameter/header fact across declarations, traits and wrapper mappings. | Provider parameter sets are semantic. Repeated placement/mapping is a **current API limitation**, not a reason to replace client → scope → endpoint. |
| **Native target edits/policy-only input:** S3 `Signer::sign`, `canonicalize_target`, `PutObjectWithMetadata::prepare`; CH `client.rs:execute`; DR `verbatim_last_segment`, repository-specific base clients, upload placeholder endpoints/query mutation | Prepare → edit dynamic names/segments/query → execute original decoder; S3 also signs final bytes. | SigV4 canonical rules and metadata prefixes are semantic. Multi-segment paths, dynamic query/header names, policy-only inputs and request read-backs expose **current API limitations**. CH’s appended parameters cannot use declaration sensitivity metadata. |
| **Runtime response decoder configuration:** OM `OpenMeteo::forecast`; CH `open_rows`/`Reader<T>`; SN `PartitionWire` → `PartitionBytes::decode(limit)`; SF `ResultPartReader`/runtime CSV profile | Bind request/job configuration → check head → choose/configure reader → normalize output/error/meta. | Format selector/dialect/inflation policy is semantic. Static codec configuration, union façade and repeated head/error orchestration are **generic mechanics/current API limitations**. |
| **Provider wrappers around shared readers:** OA `EventStream::next`; NT `SseSource::next`, `EventsError`, `Body`, `LineEnds`; CH `Guarded`, `Rows::failure`, `Insert::send`; SF `ResultPartRows::{next,finish}`; DR `BlobReader`; GH `download.rs:stream_to` | Frame/read → project payload → preserve prefix → classify source/parser/provider error → fuse/finish → verify provider completion. | Terminal events, markers, digests and row counts are **provider semantics**. Reader lifecycle/error/completion scaffolding is **generic mechanics**. NT recreates about 140 lines of error vocabulary because constructors/reader traits remain sealed; CH recovers typed errors through source downcasts. |
| **Raw binary lifecycle:** S3 `ObjectBody`; DR `BlobReader::{next_chunk,read_to_end}`; GH `stream_to` | Own body, poll chunks, count/bound bytes, retain operation/progress, terminate after error, validate EOF where required. | Expected digest/range/size policy is semantic. Limits/progress/terminal-state code is **generic mechanics/current API limitation**; no shared raw reader with the standard record-reader report contract is adopted. |
| **Replay factories and stable write identity:** DR `Registry::execute` callers: `ping`, `catalog_page`, `tag_page`, `manifest`, `put_manifest`, `blob_info`, `blob`, `start_upload`, `BlobUpload::{patch,status,complete}`; `stripe-app/src/provisioning.rs:{Run::write,request_identity,KeyLedger,IdempotencyPolicy}`; `native/src/policies.rs:retry_bounded` | Rebuild request per attempt; preserve logical operation identity/key/body; bound attempts; distinguish ambiguity. | Auth scopes/idempotency meanings/replay permission are semantic. Rebuild/cache/attempt scheduling is **generic mechanics**; DR’s inability to route replay through consumed `Endpoint` is a **current API limitation**. |
| **Polling/cancellation loops:** SN `Query::{poll_from,cancel_and_wait}`/`LocalStop`; OA `poll_batch`; SH app bulk-export polling; SF bulk/ingest polling; KU app `Stop::race`; `native::items_admitted` | Attempts/deadline/interval/stop around calls; retain unresolved/partial outcomes; separate remote cancellation from local stop. | Terminal codes and application decisions are semantic. Timers/budgets/races/admission plumbing are **generic mechanics**. |
| **Report/seed/restart stitching:** SN `FirstResult::partitions`, `Partitions`, `PartitionReport`; GH app `activity_audit`; SH app six `catalog_inventory` loops; SF app `sf_a::traverse`, `sf_b::run`; KU app `collect_list` | Drive engine, retain yielded errors, inspect report; merge owned first result or reruns with new discovery. | Nested workflow/durable publication policy is semantic. Repeated engine driving and report merging are **generic mechanics/current API limitations**. |
| **Item metadata side channels:** GH app `requests.rs:{RequestLog,record_page}`, `audit.rs:activity_audit`; GD `ChangesPageMeta`; MG `PageMeta::delta_link`; LI `PageMeta`; SL `UsersPageMeta` | Preserve page evidence/checkpoints separately when flattening yields only items. | Which evidence must be committed is semantic. Losing it at flattening and reconstructing side channels is a **current API limitation**. |
| **Known-length spool:** SF `SalesforceCsv::{spool,spool_stream,Spool}` | Serialize header separately, batch rows with headerless writer, append under whole-body cap, dispatch only after completion. | Provider upload-length requirement is semantic. Bounded collection/framing is **generic mechanics**; shared `Upload` has no corresponding bounded collect/spool helper. |
| **Undeclared-operation context mirrors:** GH `GitHubContext::{build,push_segment}`, `RepoContext::request`; `sdk/src/context.rs:{CatalogContext::build,push_segment}`; `extension/src/lib.rs:export_parts`; maintenance GH extension | Recreate base/path/header composition outside declaration, then execute with public decoder. | Endpoint names and headers are semantic. Duplicated composition is a **current API limitation** at the extension boundary. |
| **Representation scaffolding:** ST bracket serializer; LI `Graphql::encode`; NT `PlainText::encode`; OM two-table CSV adapter; SF CSV profile | Wrap a provider/domain representation around ordinary serialization/framing. | ST clear/omit conventions, GraphQL documents, OM tables/units and SF dialect are **provider semantics**. LI’s JSON wrapper and NT’s text-newtype encoder add **generic mechanics/SDK choice**. |
| **Fixture synchronization/routing:** DB custom generator gates; SF app router; `fixture`/`testserver` | Hold/release replies, route exact targets, synthesize partial transport failures. | **Harness issue**. These are not SDK-author execution obligations. Shared `Gate`, `Reply::after`, `Body::HeadOnly` now exist. |

Source-backed current defects/omissions should not be attributed to missing primitives:

| Finding | Evidence and classification |
|---|---|
| CH loses a definitely received head on an insert receipt failure | `insert.rs:143` captures metadata; `:171–175` returns `Error::Concord(*transport)`; `error.rs:46–53` returns no metadata for that variant. **SDK mistake/incomplete contract.** |
| CH collapses unusable error-text causes | `client.rs:server_exception:396–402` uses `.ok()` for bounded read/UTF-8, collapsing oversized/interrupted/non-UTF-8 into no message. **SDK outcome-design limitation.** |
| SF loses captured usage after failure | `transport.rs:send_with_usage` captures `ApiUsage`, then failed status/decode conversion does not retain it. **SDK mistake/incomplete contract.** |
| OA drops an incomplete-response reason | `Event::Other` and `ResponseObject` do not retain `incomplete_details`; EOF later becomes generic stream failure. `FIRST_USE.md:123–131` remains **CURRENT SDK omission**. |
| DR does not enforce its README’s universal exact-status promise | `pages.rs:{catalog_page,tag_page}` and token fetch accept any success status before decode; other operations use `expect_status`. **Documentation issue**, potentially an SDK enforcement mistake. This conclusion is from branch inspection, not an executed probe. |

**3. Deferrals to reconsider, and historical reconciliation**

Here `D` means `v4/sdk20/DECISIONS.md`. A working workaround is recorded as a path, not evidence that its cost disappeared.

| Record / rationale | Disposition against current source and visible cost |
|---|---|
| `D:98–107` D-A5: collect more cases before parameter groups | **CURRENT.** KU parameter blocks, ST account overrides, GH filter mappings and SF typed-query registrations still repeat. `Endpoint<T>` resolves common preparation, not grouped setters/placements. |
| `D:116` MG-4: unexpected-status constructor, one call site | **CURRENT.** GH `download.rs:locate` still needs its special status path; public status-error construction remains restricted. |
| `D:119` MG-5: optional operation identity, one SDK | **PARTLY HISTORICAL(resolved by `Endpoint::operation`)** for identity available before building. `Incoming::operation()` remains optional; SDK custom-reader paths still handle fallback identity. |
| `D:149` W1-4: signing forwarders deferred for one consumer | **HISTORICAL(resolved by value-form `SendWith`, `send signer;`).** S3 removed 36 forwarding definitions; native signing and metadata decoration remain. |
| `D:150`, `:601`; S3 `FRICTION:49–80`, DR `FRICTION:54–77`: multi-segment workaround | **CURRENT.** S3 rewrites encoded separators; DR builds a generated client per validated repository base. Two consumers now pay different workarounds. |
| `D:194`, `:287`; GL `FRICTION:36–60`: prepare a placeholder to borrow handling/context | **CURRENT.** PK detail URL prepares `"placeholder"`; GL prepares first-page operations for linked pages; MG prepares in `Listing::new` to discover path, then again to fetch; DR runtime upload endpoints use placeholder paths plus `follow`. |
| `D:224–225`: SSE/multipart deferred until another consumer | **HISTORICAL(resolved by `SseReader` and `multipart::MultipartBody`).** OA/NT and OA/GD adopted them. Provider payload/terminal interpretation remains. |
| `D:226` W2-10: “both strict parsers work”; GL `FRICTION:62–69` | **CURRENT.** Four tokenizer implementations now remain: GH, GL, SN, DR. SN maintenance records adapting GH’s parser; DR friction names the GL duplication. |
| `D:227`, `:604`: item metadata deferred because callers can use `Pages` | **CURRENT.** SL/LI page diagnostics, GD final changes token, MG delta checkpoint and GH request logging still require page consumption or side channels. |
| `D:228`, `:318`, `:408–411`: getters deferred; read-backs work | **CURRENT.** GL derives API prefix from a prepared path; S3 reads starting token/range from built request; ST prepares templates to inspect preset cursors/fingerprint. |
| `D:316–317`; S3 dynamic-metadata friction: one policy-only-input consumer | **CURRENT.** `PutObjectWithMetadata::prepare` inserts dynamic headers before signing and uses a separate `.send(&s3)` terminal surface. Decorated requests cannot re-enter the generated policy. |
| `D:337`, `:473`: configured reader workaround works through bare stream | **HISTORICAL(resolved by `check_head`, `read_for`, `Incoming: ByteSource`).** A custom `Incremental` can retain typed `-> stream T` with configured readers. Existing SDKs did not all migrate to that shape. |
| `D:347`, `:536`: cursor context deferred because factory closure works | **CURRENT.** DB/AT are joined by GD/SL/LI/WB/NO raw-resume obligations. MG implements substantial stored-link binding itself. Capturing context in one traversal does not bind a restored key. |
| `D:470` W34-G1: SSE line counter, one consumer | **CURRENT.** NT `Body`/`LineEnds` makes a second pass to report payload-error lines because shared SSE events lack dispatch-line information. |
| `D:471–473`: body adapter/name/configuration workarounds | **HISTORICAL(resolved by D-W45-R5)** for `Incoming` byte source, by-value source extraction, `read_for` and checked-reader composition. NT line counting and custom error vocabulary remain. |
| `D:533`: runtime format choice deferred; native/runtime reader works | **CURRENT.** OM manually dispatches finite JSON/CSV; CH boxes runtime reader variants; SN separately supplies runtime inflated-size limits after `PartitionWire`. These are distinct but repeated configuration pressure. |
| `D:535`, `:607`: error orchestration “already addressed; remaining schemas” | **CURRENT residue exceeds schemas.** The repeated status/check/read/decode/metadata branches in §2 remain in the SDKs. NO `FRICTION:27–40`, MG `:61–72`, WB `:4`, SN `:33–41` explicitly report this work. |
| `D:537`; CH `FRICTION:18–34`: dynamic keys, native recipe works | **CURRENT.** `execute` dismantles every query, appends `param_<name>` pairs, then uses `Decoder::execute`; declaration sensitivity does not cover appended values. |
| `D:602`; DR `FRICTION:79–101`: digest colon, one consumer | **CURRENT.** `verbatim_last_segment` rewrites digest/reference path bytes then calls `follow`. |
| `D:603`; DR `FRICTION:103–119`: challenge replay, one consumer/factory exists | **CURRENT.** Eleven operation sites pass rebuild factories into `Registry::execute`; uploads remain buffered/replayable `Bytes`; no `SendWith` adoption. |
| `D:605`: provider-error accessor deferred to next iteration | **CURRENT.** `PagesError` has `outcome/resume/progress`, no typed provider-error accessor. MG’s consumer boxes it and repeatedly matches nested variants; DB/AT have similar layered access. |
| `D:606`; DR `FRICTION:147–157`: realm query excluded, one consumer | **CURRENT.** `TokenAuth::check_realm` rejects a query because the token declaration uses the realm as a base URL. |
| SH `FRICTION:106–132`: six loops clearer; borrowing factory works | **CURRENT.** Six `catalog_inventory` functions still drive `sequential`/`finish` separately; ordinary `Pages` exists but this control has not adopted it. |
| SF `FRICTION:72–108`: native Upload/spool workaround | **CURRENT.** Bespoke `Spool` remains; lazy wire upload is excluded. Generic source consumption is demonstrated, Salesforce chunked acceptance is not. |
| SF `FRICTION:138–163`: ordinal/restart workaround | **CURRENT.** Descriptor ordinals and app dual-engine report merging remain in `sf_b::run`. |
| GH `FRICTION:146–188`: raw loop/item metadata/checkpoint workarounds | **CURRENT.** `stream_to`, `RequestLog`, and nested `activity_audit` checkpoint handling remain. |

Other material record findings reconciled with current source:

| Historical record | Current classification |
|---|---|
| NH `FRICTION:26–37`; FF `FRICTION:24–35`; PK old `.call()`; GL `FRICTION:82–85` | **HISTORICAL(resolved by their `SendWith` policies).** Metadata/error adapter bodies remain **CURRENT**. |
| TW `FRICTION:8–19`; ST `FRICTION:134–154` chunk-limit/native-read claims | **HISTORICAL(resolved by adopted `Received::bytes_limited`).** TW/ST still parse provider documents themselves. |
| GH `FRICTION:15–66` native execution/impossible transport conversion | **HISTORICAL(resolved by `Prepared::receive`, `Decoder::execute`, `transport_error`).** GH’s claim that URL capture requires `into_parts` is also historical; `capture` uses `Received::url`. |
| GH `FRICTION:208–217`; ST `FRICTION:168–171` unnamed factory-only ordinary lists | **HISTORICAL(resolved by `Pages` and named SDK page/item types).** Engine use remains where item checkpoints are required. |
| OA `FRICTION:7–8`; GD `FRICTION:58–71`; NT `FRICTION:15–35` | **HISTORICAL(resolved by shared Multipart/SSE).** Preserve original observations and later integration pointers. |
| OA `MAINTENANCE:115–117`: JSONL cannot continue malformed rows | **HISTORICAL(resolved by `JsonLinesOptions::record_errors(RecordErrors::Yield)`).** The SDK still uses its buffered parser and provider ID/correlation rules. |
| NT `FRICTION:58–82`, `:84–96` | Media/configuration/cancel-contract absence is **HISTORICAL(resolved by shared checks, R5 reader APIs, and `Incoming::chunk` documentation)**. Fixed SDK limits/custom errors remain **CURRENT**. |
| CH `FRICTION:36–53`, `:63–68` | **HISTORICAL(resolved by `check_head`, `read_for`, `SourceError::into_source_unredacted`).** Provider guard, downcasts and runtime reader enum remain **CURRENT**. |
| OM `FRICTION:64–86`, maintenance claim that quote guard remains | **HISTORICAL(resolved by shared CSV EOF scanner); SDK quote-parity guard removed.** Two-table interpretation remains **CURRENT**. |
| LI `FIRST_USE:44`, `MAINTENANCE:158–163`: null connection loses diagnostics | **HISTORICAL(resolved by required-nullable `IssuesData::issues` and `issue_page` returning `Error::Page`).** Missing member remains a decode failure. |
| SN `FIRST_USE:162–168`, D W4-10: absent partition manifest differs from empty | **HISTORICAL(resolved by defaulted `partition_info` plus `FirstResult::new` rejection).** Missing `resultSetMetaData` still decode-fails. |
| SN discarded returned status/partition links | **HISTORICAL(resolved by `remember_status`, `PartitionPlan::fetch`, link validation).** |
| Registry unchecked token-expiry arithmetic | **HISTORICAL(resolved by `Registry::fetch_token:255–257`, `Instant::checked_add` → `AuthProblem::TokenResponse`).** Token is not cached/sent on that branch. |
| SH `FRICTION:98–104`: no labelled transport conversion | **HISTORICAL(resolved by `Operation::transport_error`).** App/maintenance callers still use manual `without_url`: non-adoption. |
| SH `FRICTION:142–156`: uncertain skipped-line terminal state | **HISTORICAL(resolved by `ReadError::is_terminal`), adopted by app.** |
| KU `FRICTION:80–118`; SF `FRICTION:50–70`: bodyless/unlabelled stream declaration | **HISTORICAL(resolved by typed/bare `-> stream`, `Incoming`).** Provider completion adapters remain **CURRENT**. |
| S3 hidden builders/36 forwarders, duplicate Range storage | **HISTORICAL(resolved by value-form policy and `Signer::sign` request read-back).** Dynamic metadata surface and carried response-validation context remain **CURRENT**. |
| S3 body failure indistinguishable from verified EOF | **HISTORICAL(resolved by `ObjectBody::completion`).** A later `chunk()->None` alone still does not prove success. |
| PK maintenance `follow_with_limit` | **HISTORICAL(trial removed).** Integration restored verbatim server-link following; do not count removed query splicing as current residue. |
| TW empty SID/hidden status/ambiguous public error names | **HISTORICAL(resolved by empty-SID validation, `Message::fmt`, `TwilioError` and docs).** Wrapper synchronization remains **CURRENT**. |
| DB blocking fixture callback/no general gate; S3 no HEAD-only fixture | **HISTORICAL(resolved by nonblocking fixture changes, `Gate`/`Reply::after`, `Body::HeadOnly`).** **Harness issues**, not provider requirements. |
| SN workspace gzip collision; Frankfurter environment/cache failures; AT doubled `/v0`; NO guessed harness APIs | Historical **SDK/harness/environment mistakes**, not current core limitations. SN local decompression and corrected consumer/harness calls are present. |

**4. What became easier, where complexity moved, and what was not tested**

The numerical reductions below are measurements recorded by SDK20; I did not reproduce them.

| Before → current source | Concrete improvement | Remaining/moved work |
|---|---|---|
| Native split/execute/`finish(Err)` recipes → `Received` | Core `prepared.rs:receive`, `response.rs:Received`; GH/ST/SH/SF migrations remove impossible transport-success branches. D-A1 records the comparison. | SDK policies still capture metadata, choose provider outcomes and wrap failures. Some auxiliaries/controls retain older native recipes. |
| Fixed generated terminal method → `send Policy;` / `send value;` | Ten of 25 SDKs select a policy. NH/PK/FF/GL ordinary calls use generated `.send()`; S3 generated builders are signed through client-held state. | DB/AT/OA still expose wrappers despite policy use. TW/SL measured and rejected migration; all five controls remain without policy adoption. |
| S3 private builder façade → generated operations | D `:307–310` records 36 forwarders removed and source reduction. `api.rs:send signer` is current. | `Signer`, specialized signed outputs, request read-backs, metadata decoration and custom binary reader remain. |
| Error chunk loops → bounded reads | TW/ST use `Received::bytes_limited`; SL uses `decode_limited`. | Typed error-body orchestration remains; SF/KU retain manual loops. |
| Bodyless streaming endpoint → truthful stream output | KU six `-> stream WatchEvent<T>`; SF bare `-> stream`; OA raw `Incoming`; core `Incremental`. | SDKs still interpret completion, errors and runtime configuration; default reader streaming does not imply a provider wrapper implements standard `Stream`. |
| Factory-only pagination → named `Pages`/`PageItems` | GH/ST ordinary lists now expose named traversals; caps/failures cannot silently become clean exhaustion; lossless report decomposition exists. | Context/projection factories, item metadata, nested checkpointing, seed/rerun merging remain. |
| Local SSE and multipart → shared formats | OA/NT use `SseReader`; OA/GD use `MultipartBody`. NT integration records 608→417 non-comment production lines. | OA provider event wrapper; NT custom errors/line counting/replay; GD upload session state remain. |
| Repeated whole-head types → `ResponseMeta` | Seven-SDK migration records −197/+64 source lines and fixes derived-header `Debug` leaks. | Value+head and error+head wrappers remain; selective projections and incomplete adoption persist. |
| Wrapped-source readers lose identity/errors → R5 APIs | CH/OA/NT use `Incoming: ByteSource`, `check_head`, `read_for`, by-value source extraction. | Downcasts, provider guards, custom error unions and request-derived configuration remain. |
| SDK quote-count workaround → shared CSV EOF validation | OM guard removed; shared `csv/frame.rs:507–514` rejects EOF inside quote/escape state. | OM’s metadata/data table split, units and selected-column checks remain provider code. |

Auxiliary corpus inventory:

| Example | Concord surface exercised |
|---|---|
| `native` | `metadata::{execute,MetaError}` selected-head wrappers; `errors::rename_item` typed status body; native edits; custom formats; raw chunks/deadlines; non-Clone upload; factory retry; semaphore admission; caller-driven credential snapshots. |
| `extension` | Undeclared operation through public `TenantContext`; native construction plus `Decoder::new::<Csv>()`; finite/incremental CSV. `CatalogExport` profiles shared CSV rather than framing it. |
| `multipart` | Both native reqwest multipart for streamed parts and a custom `Encode::encode_body` runtime-boundary demonstration. Source recommends shared multipart for ordinary in-memory forms. |
| `riot` | Routed scopes/subdomains; sensitive custom credential header; credential-free DataDragon JSON/binary; multi-call workflows; numeric Retry-After projection; redirect safety/control examples. |
| `quickstart` | Small generated Tasks API, query defaults/options, typed JSON bodies, raw status-error document; offline server and documentation-excerpt harness. |
| `app` | Domain conversion into generated operations; snapshot scopes; independent multi-service outcomes; handwritten bounded traversal and partial resumptions. |
| `sdk` | Two unlike generated APIs, hierarchy/inheritance/override/clear behavior, by-value binary encoder; native extension context mirrors declaration composition. |
| `shapes` | Flat, scoped/routed/nested, repeated-query, body and no-output declaration controls. |
| `renamed` | Renamed facade/reqwest dependencies, aliases, qualified formats/native headers, feature-minimal macro hygiene. |
| `compat` | Edition-2021/private clients; metadata escape; HTTP-200 envelope; nested form; multisegment resource key; alternate request media; idempotency initializer placement; snapshot refresh; 200/204 custom decoder. |
| `fixture` | Catalog/Ledger HTTP/1.1 recording/reply/service harness, including chunked requests. |
| `testserver` | Delayed/stalled/chunked/lazy/failing bodies, slow receivers, request accounting, HEAD-only replies, synchronization gates and owned cleanup. |
| `github-app` | Conditional requests, nested item checkpoints, page-metadata side channel, Link traversal, credential-free artifact download, sink bounds/cancellation. |
| `stripe-app` | Cursor reconciliation, embedded pages/expansions; bracket-form writes, account overrides, stable-key ledger and bounded ambiguous-write replay. |
| `shopify-app` | Partial GraphQL data/cost policy; six nested traversal loops; bounded job polling, signed native download, JSONL error/sink policy. |
| `salesforce-app` | Org/SOQL resume binding, explicit locator restart; manifest/independent rerun composition; validated CSV parts and staged publication; bounded-spool ingest. |
| `kubernetes-app` | Snapshot list inventory, expiry restart, list/watch transition, bookmarks/ERROR, reconnect/relist, state map and cancellation lifetime. |

What the exercise did **not** establish:

| Untested/generalized guarantee | Existing boundary |
|---|---|
| Live provider conformance | SDK20 records say local fixtures only; synthetic partition/CSV/provider envelopes are not vendor acceptance. |
| General OAuth acquisition, refresh grants, single-flight/distributed cache | Most SDKs accept supplied credentials. DR is a scoped challenge-token cache; concurrent fetches may duplicate, `issued_at` is ignored, refresh grants are absent. |
| Unified auth/retry/rate admission composition | Native recipes demonstrate individual mechanics; DR has one challenge replay; ST app has keyed replay. There is no corpus-wide composed policy. |
| Strict wire-attempt accounting | One core `execute` is not proof of one physical exchange under arbitrary caller reqwest retry/redirect configuration. GD/SN explicitly disable native retries; many supplied-client surfaces cannot enforce configuration. |
| Complete durable continuation binding | Several keys are raw strings/numbers; stronger bindings still omit some dimensions or trust caller-supplied identity labels. |
| Snapshot consistency/deduplication across ordinary live listings | Most adapters validate local continuation contracts, not a transactionally stable population. |
| Lazy multipart/resumable/signing uploads generally | OA/GD multipart and GD/DR chunks are buffered; S3 streaming signing is refused; SF spools. CH/native are the principal lazy-source witnesses. |
| Universal stream completion | EOF, parser completion, provider terminal success, row-count/digest validation and application commit remain distinct. |
| Admission covering caller-consumed stream lifetime | Native semaphore example covers finite decode; no general stream lease/cooldown composition is demonstrated. |
| Full vendor SDK breadth | General GraphQL schema generation, arbitrary query selection, many mutation/resource families, provider-specific auth alternatives and excluded formats remain outside the slices. |
| Untouched first-pass success after all fixes | Records preserve initial findings and later migrations. LI/SN fixes and shared codec migrations must not be relabelled as original first-pass results. |

**5. Stale current documentation**

Historical `FRICTION`/`FIRST_USE`/`MAINTENANCE` statements above should be preserved, with resolution pointers. The following are current guidance/reference discrepancies.

| Current file:line | Misdescription | Current truth |
|---|---|---|
| `README.md:134`; `crates/concord/src/lib.rs:165`; `docs/CHEATSHEET.md:112`; `docs/RECIPES.md:313–314`, `:408` | Unqualified “one `send()` is one execute/request,” no additional requests/retries. | Applies to `Prepared::send` and default generated send. A policy-selected generated send invokes arbitrary `SendWith<T>::send` (`concord-macros/src/emit.rs:1498`) and can perform subrequests/repetition. |
| `docs/CALLER_GUIDE.md:252` | Generated send is universally `prepare()?.send()`, executing/checking/decoding. | Default only. Policy output can differ; typed stream output opens a reader without consuming the body. |
| `docs/CALLER_GUIDE.md:300` | Every call returns `Result<_, concord::Error>`. | `SendWith<T>::Output` is arbitrary and need not be `Result`; `prepare()` retains Concord’s result contract. |
| `docs/AUTHOR_GUIDE.md:1038` | Incremental consumption is directed to `send_raw` and native reads. | Ordinary routes are `-> stream T`/`Incremental<T>` or bare `-> stream`/`Incoming`, already documented later at `:1151`. |
| `docs/RECIPES.md:107` | Any declared typed output buffers the whole body. | Finite `-> T` does. `-> stream T` opens the declared reader unread. |
| `docs/CALLER_GUIDE.md:378–379` | Untrusted error bodies are directed to a handwritten chunk-limit loop. | Current `Received::bytes_limited` supplies bounded any-status reading with identity; `decode_limited` bounds finite success decode. Native loops remain valid escapes. |
| `crates/concord/src/lib.rs:104–107`, `:120–130` | Runtime/syntax overview omits the main SDK20 execution/streaming/policy types and syntax. | `Received`, `Incoming`, `Endpoint`, `SendWith`, `Incremental`, `-> stream`, and client `send` are public current surface. This is incomplete reference coverage. |
| `docs/MAINTAINERS.md:10–16`, `:37–39` | Runtime map omits response/policy/stream contracts; multipart described solely through reqwest. | `response.rs` owns response state/identity; `prepared.rs` owns `Endpoint`/`SendWith`; `codec.rs` owns runtime media/`Incremental`; multipart has declared/shared and native routes. |
| `examples/native/README.md:23` | `metadata::execute` converts transport failure through `decoder.finish(Err(e))`. | Current `metadata.rs:275` uses synchronous `decoder.transport_error(e)`. |
| `examples/google-drive-sdk/README.md:262–263` | Small related serializer is SDK code. | `upload.rs:91–96` uses shared `MultipartBody::related`/`Part`; `api.rs` selects `Multipart`. |
| `examples/open-meteo-sdk/README.md:126–127` | An SDK guard rejects unmatched-quote EOF. | The guard was removed; shared CSV framing performs the rejection. |
| `examples/clickhouse-sdk/README.md:158` | `Error::Concord` described as pre-head/outbound-upload failure. | `Insert::send:171–175` also returns it for a post-head receipt-body transport failure; its metadata accessor then returns `None`. |
| `examples/distribution-registry-sdk/README.md:233–234` | Every unexpected 2xx becomes `UnexpectedStatus`. | Catalog/tag/token paths only check `is_success`; exact-status checks are operation dependent. |
| `examples/ntfy-sdk/README.md:167–170` | Ending inside a JSON line is described as Format failure without qualifying completeness. | Shared NDJSON accepts a complete final JSON value without LF at true EOF; incomplete values still fail. Existing first-use source explicitly records this discrepancy. |
| `examples/slack-sdk/README.md:118–119` | Checkpoints described as bound to originating listing. | This is a caller constraint: `Cursor(String)` has no context; the current closure fixes options only within that traversal. |
| Generated DB Config docs: `concord-macros/src/emit.rs:1252`; reexport `dropbox-sdk/src/lib.rs:17` | Documentation points at hidden `DropboxApi` constructor/setters while public wrapper lacks those setters. | Generated wording is correct for the hidden generated client, misleading for the reexported wrapper-facing Config. Source-backed documentation issue; rustdoc was not generated here. |
| `v4/sdk20/READOUT.md:117` | Says eight SDKs use `decode_limited`. | Current source has ten: SL, LI, DB, GD, AT, SN, WB, NO, MG, DR. This is stale adoption counting. |
| `v4/sdk20/READOUT.md:446` | Open-Meteo appears in the “Streams/readers” feature table through bare stream syntax. | The declaration is truthful, but the SDK forecast result is buffered. It should not be counted as incremental caller-facing forecast consumption. |

Qualification rather than outright error:

| Location | Assessment |
|---|---|
| `docs/CUSTOM_FORMATS.md:248–252` | A custom `WithVersion<T>` decoder remains valid; add a `Received::head/meta` pointer so retaining metadata does not appear to require changing output type. |
| `docs/RECIPES.md:84–87` | Selected-header/URL `Meta` remains a valid custom projection. `ResponseMeta` now supplies a safe whole-head container; core still does not supply the example’s `WithMeta<T>`/`MetaError`. |
| `docs/MIGRATION.md:38–89` | Old native snippets are explicitly historical and followed by current migration routes. Preserve them. |
| `examples/compat/README.md:11–15` | Accurately describes existing native compatibility tests; it is not a claim those routes are mandatory. |
| `v4/sdk20/READOUT.md:406–410` | Correctly records configured typed-reader composition through `check_head`/`read_for`; the previously stale limitation was already corrected. |
| `v4/sdk20/STATUS.md:30–48` | Earlier “records only” successor wording is superseded by the explicit M0 implementation-fix paragraph. Preserve the pinned gate identity; current checkout should be identified separately. No additional missing-`Received`/`Incoming` API claim was found there. |
| `crates/concord-core/src/lib.rs`, `concord-macros/src/lib.rs`, `concord-extras/src/lib.rs`, `concord-pagination/src/lib.rs` | No corresponding obsolete execution/streaming claim found. Core/extras/pagination current contracts document the new surfaces. |

**6. Proposed fixed scenario denominator: 88 scenarios**

This is a scenario denominator, not a population percentage or pass report. A witness means the corpus contains relevant source/caller evidence; qualifications and gaps remain part of that scenario’s later assessment. `—` means no existing SDK demonstrates the general guarantee. Codes resolve through §1; auxiliary names are literal example directories.

| ID | One-line scenario definition | Existing witnesses / gaps |
|---|---|---|
| R01 | Finite unauthenticated JSON object/array with required path values. | NH, PK, FF, WB |
| R02 | Client/scope/endpoint inheritance, override and removal produce the intended request. | GH, ST, KU; `sdk`, `shapes` |
| R03 | Absent, empty, null and defaulted request values remain distinct. | ST patch/form, NO query; `compat` |
| R04 | Repeated query/filter values are encoded exactly once. | GL, AT; `shapes` |
| R05 | JSON request body and typed response share normal generated preparation. | OA, LI, NO |
| R06 | Flat form request omits only absent fields and preserves Unicode/reserved bytes. | TW |
| R07 | Nested bracket form supports arrays, maps and clear-versus-omit semantics. | ST |
| R08 | HTTP status chooses different successful representations. | SN 200/202; GD 308/completion; GH 304; `compat` 200/204 |
| R09 | Head-derived output preserves metadata without treating no-output drain as head-only. | S3, DR, GH; GD explicit empty-body check |
| R10 | An undeclared downstream endpoint reuses SDK context through public boundaries. | GH maintenance extension; `extension`/`sdk`; KU closed-surface gap |
| E01 | Non-2xx provider document becomes a typed SDK failure with authoritative status. | TW, FF, GL, ST, GD, NO, MG, DR, SF, KU |
| E02 | Malformed/HTML/absent error document does not turn HTTP failure into success. | FF, GL, SL, DB, GD, ST, DR |
| E03 | Error-document media rejection is distinguishable from document decoding failure. | TW, FF, GL |
| E04 | Oversized error body stops at a bound and retains failure identity/head. | TW, FF, SL, ST; SF/KU manual loops |
| E05 | Interrupted error-body reading preserves HTTP failure and read-failure evidence. | TW, DB, GD, AT, DR; CH collapses cause |
| E06 | Failure before any response has no fabricated status/metadata. | NH, PK, ST, GL |
| E07 | Failure after a received head retains that head. | Most metadata adapters; CH/SF gaps in §2 |
| E08 | HTTP-200 application failure is classified separately from transport success. | SL, WB, SN; TW failure status remains data |
| E09 | GraphQL partial data, errors-only and null connection retain diagnostics. | LI, SH; LI missing-member distinction |
| E10 | Missing, explicit null, zero and unknown variants follow provider model rules. | PK, LI, WB, NO, MG, NH |
| P01 | Strict Link parsing rejects malformed/conflicting continuation fields. | GH, GL, SN, DR |
| P02 | Body next URL is followed unchanged with destination/path validation. | PK, MG, SF |
| P03 | Opaque cursor/offset token is transmitted without trimming or reconstruction. | SL, DB, AT, NO, LI |
| P04 | Numeric limit/offset pagination remains directly addressable. | PK |
| P05 | Page/index plan is derived from provider metadata and validated. | WB pages; SN partitions |
| P06 | Empty nonterminal page continues rather than falsely exhausting. | PK, S3, SL, DB, GD, WB, NO, MG, KU |
| P07 | Contradictory terminal flag/missing next key fails explicitly. | S3, LI, NO, GD, SH |
| P08 | Immediate repeated continuation is rejected. | S3, GL, LI, GD, MG, DR; five controls |
| P09 | Nonadjacent continuation cycle is detected or explicitly bounded. | NO seen sets; caps elsewhere; DR/PK caller-policy gaps |
| P10 | Continuation rejects mismatched bound context before dispatch. | MG, DB, GH, ST, SF app; partial dimensions documented |
| P11 | Restored raw continuation cannot silently migrate to another query/account. | Gap in SL, LI, GD, AT, WB, NO; negative scenario |
| P12 | Exact terminal-page cap succeeds; cap with work remaining reports boundedness. | `Pages` adapters S3/GL/SL/DB/GD/AT/WB/NO/MG |
| P13 | Failed page preserves delivered prefix and failed/restart identity. | GH, ST, GL, SL, DB, AT, WB, NO, MG |
| P14 | Flattened items preserve or explicitly lose page completion metadata. | GD changes token, MG delta, LI/SL diagnostics, GH side channel |
| C01 | Supplied static credential is placed and redacted consistently. | TW, GL, SL, OA, NT, LI, DB, GD, AT, SN, NO, MG; controls |
| C02 | Basic credentials are validated and correctly Base64 encoded. | TW, CH, DR |
| C03 | Invalid credential/header text fails locally without secret diagnostics. | GL, OA, NT, LI, GD, MG, TW |
| C04 | Custom credential headers cannot leak through cross-origin redirect policy. | GH, GL; `riot`; DR caller-transport obligation |
| C05 | Challenge token acquisition separates resource scopes/cache entries. | DR `Access` pull/push/catalog |
| C06 | Expired or unrepresentable token lifetime produces safe cache behavior. | DR `cached_token`, checked `fetch_token` |
| C07 | Untrusted/malformed token realm causes no credential-bearing token request. | DR `TokenAuth::check_realm` |
| C08 | Second authentication challenge terminates after one replay. | DR `Registry::execute` |
| C09 | Concurrent acquisition/refresh is single-flight and cache-safe. | — general guarantee; DR explicitly permits duplicate fetches |
| C10 | Signature covers final target, headers and payload using client-specific state. | S3 |
| C11 | Credential rotation preserves defined behavior of queued old handles. | `native::Session`, `compat::Refresh`; SDKs mostly reconstruct clients |
| X01 | Preparing/inspecting a request performs no network I/O. | All generated SDK surfaces; native upload probes |
| X02 | Logical dispatch, policy subrequests and physical transport attempts are counted separately. | DR challenge flow; GD/SN transport configuration; general gap |
| X03 | Ambiguous keyed write rebuilds the same request with the same logical key. | ST app `Run::write`/`KeyLedger` |
| X04 | Changed request under an existing idempotency key is rejected/reported. | ST app `request_identity` |
| X05 | Ambiguous unkeyed write is not automatically treated as safe to repeat. | TW, S3, GD, CH, DR |
| X06 | Retry attempts/backoff are bounded and conditioned on semantic replay permission. | `native::retry_bounded`; ST app; no general SDK policy |
| X07 | Rate advice distinguishes absent, repeated, invalid, overflow and date forms. | SL/DB strict seconds; GH/GL/NT weaker; date gap |
| X08 | Admission permit covers the intended response/stream lifetime. | Native finite decode example; caller-consumed stream lease gap |
| X09 | Auth replay, retry and rate admission compose under one operation budget. | — general guarantee; isolated DR/ST/native mechanisms only |
| S01 | Declared SSE opens after head acceptance without reading the body. | OA bare stream; NT typed stream |
| S02 | NDJSON/consecutive JSON/watch records are read incrementally. | NT, CH, KU |
| S03 | Runtime CSV reader uses provider/job dialect and column contract. | CH, SF |
| S04 | Record framing survives arbitrary delimiter/UTF-8/CRLF chunk splits. | OA, NT, CH, KU, SF |
| S05 | Wrong response media/head is rejected before a stalled body is consumed. | NT, CH, GL; native head-stage example |
| S06 | Record/event/total-byte bounds fail without claiming clean completion. | OA, NT, CH, SF; shared readers |
| S07 | Provider failure arriving after valid records retains the delivered prefix. | CH late exception, OA failure event, KU ERROR |
| S08 | Clean EOF is distinguished from provider terminal success. | OA completed event, CH marker, SF count validation, KU reconnect |
| S09 | Complete final record without LF differs from truncated record/frame. | NT NDJSON, SH JSONL; SSE has stricter boundary rule |
| S10 | Recoverable malformed record can yield an error and continue safely. | SH JSONL policy; OA buffered parser; shared Yield policy |
| S11 | Cancelling a pending read/drop preserves defined partial state and stops work. | OA, NT, CH, SF, KU apps |
| S12 | Binary EOF validates expected length/digest before claiming verification. | DR digest/size; S3 length/range; GH digest not verified |
| S13 | Runtime response-format/configuration choice remains consistent with request/job context. | OM JSON/CSV, CH reader union, SN inflation cap, SF dialect |
| U01 | Multipart boundary/media and exact part contents remain consistent. | OA form-data, GD related |
| U02 | Contradictory runtime media fails before dispatch. | `multipart` declared-body example; no additional SDK-specific negative witness |
| U03 | Lazy one-shot upload transfers ownership without manufacturing replayability. | CH; `native::OneShot` |
| U04 | Upload-source failure reports possible partial remote effects. | CH; native streamed upload |
| U05 | Known-length-only upload consumes a bounded spool before dispatch. | SF |
| U06 | Preparation/Debug/inspection never polls a lazy upload source. | Native upload source; CH writer-based path |
| U07 | Lost resumable-write response blocks further writes until authoritative probe. | GD `needs_probe`; DR `needs_status` |
| U08 | Resume uses acknowledged prefix, including partial/unaligned acknowledgement. | GD maintenance; DR `BlobUpload::status` |
| U09 | Repeated/regressed/overflowing/impossible upload acknowledgement fails closed. | GD Range checks; DR range/location protocol |
| U10 | Upload completion validates provider status/location/digest rather than request EOF. | GD `UploadSession::accept`; DR `BlobUpload::complete` |
| M01 | Additive unknown fields/enum values preserve compatible existing callers. | NH, TW, PK, NO, MG |
| M02 | Selected nullable GraphQL field change remains aligned across documents/models. | LI dueDate maintenance; SH selection/type independent-knob gap |
| M03 | Provider field/column rename does not require unrelated adapter changes. | AT dynamic fields; OM selected-column mapping |
| M04 | CSV ending inside quoted field is rejected on finite and incremental paths. | OM reproduction; shared CSV scanner |
| M05 | Encoded and inflated response limits are independent and checked. | SN local gzip adapter |
| M06 | Already-owned first page/partition is reused rather than fetched again. | SN partition zero; ST embedded invoice lines |
| M07 | Poll attempt/deadline cap retains an explicit unresolved/last-observation result. | SN, OA, SH, SF; OA metadata limitations |
| M08 | Remote cancel acknowledgement and eventual terminal outcome remain distinct. | SN `CancelObservation`; local stop separate |
| M09 | Partial/staged output is committed only after validated completion. | SF app shards; SH bulk sink; MG/GD application checkpoint obligation |
| M10 | Private outputs, renamed dependencies and downstream extension preserve public usability. | `renamed`, `compat`, core private-output fixtures; maintenance extensions |
| M11 | SDK dependency/features work independently without changing sibling transport behavior. | SN gzip incident/local fix; isolated SDK manifests and historical gate records; no fresh verification here |