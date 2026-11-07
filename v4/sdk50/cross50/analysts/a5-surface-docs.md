The strongest remaining changes are in generated documentation, not additional abstraction. Concord already removes substantial forwarding code, but its docs still make callers learn implementation vocabulary, distinguish identically rendered errors, and infer where a base URL ends.

The sweep covered **all 55 SDKs**. Their declarations contain **63 clients, 91 scopes and 316 endpoints**, across **3,424 physical declaration lines**, including comments and documentation. I read source and notes only; I did not build, execute repository code, contact services, or change files.

**Historical complaints and current code must be kept separate.**

- Discord’s maintenance wrapper was subsequently replaced by `query(..)`; its maintenance record explicitly says so at `examples/discord-rest-sdk/MAINTENANCE.md:6–7`.
- S3’s forwarding layer is already gone. Its integration record reports **36 forwarding definitions / 245 documented lines removed**, while expanding the public builder surface to **82 generated controls**: `examples/amazon-s3-sdk/INTEGRATION.md:267–293,367–370`.
- GitLab, Frankfurter, Nager Holidays and PokéAPI now use generated `send()` rather than their former parallel terminal methods. Their original consumer complaints remain valuable evidence, but are not current deletion opportunities.
- **33 of 41 policy-bearing clients already declare a result template**, covering **140 of 163 policy-backed endpoints**. Do not propose this existing facility again.

### Highest-value changes

| Priority | Smallest change and ownership | Current reach and removable cost | Public API effect | Counterexamples / limits |
|---|---|---|---|---|
| 1 | **Make generated docs distinguish declared decoding from ordinary policy-backed execution.** Macros. Put the concrete declared send result first when available; label the wire output as the handling used by prepared execution. | 39 SDKs use send policies; 163 endpoints. **0 guaranteed SDK code lines removed**; removes misleading explanations and duplicated SDK prose. | None. | Kraken and Cloudflare deliberately unwrap wire envelopes. Saying merely “Output: `Envelope<T>`, decoded from 2xx” misrepresents what ordinary callers receive. |
| 2 | **Print declared base and declared path separately.** Macros. Preserve the composed default URL as a third, explicitly labelled fact. | Generator-wide; six consumers report base/path confusion, another reports the production default being mistaken for the selected environment. **0 code lines removed.** | None. | PayPal and Kraken policies constrain overrides further than Concord does. Generated documentation must identify this boundary rather than promise every syntactically valid override works. |
| 3 | **Move `validate_header_names()` below construction/navigation; hide its documentation when the check is trivial.** Macros. | Generated for all 63 clients. **24 clients across 23 SDKs have no native-name work**; 18 SDKs have no native header names in any declaration. Five consumers explicitly complain about prominence. | Documentation-only hiding preserves compatibility. Removing trivial functions would remove 24 public/generated functions and invalidate 12 checked-in assertion lines. | Native `HeaderName` constants require the structural check; do not remove the useful checks. |
| 4 | **Name `concord::Error` in generated `prepare()` and `with_base_url()` prose.** Macros; SDK re-export guidance in docs. | **12 FIRST_USE reports** describe indistinguishable `Error` names. **0 runtime lines removed.** Twilio has already worked around this by renaming its error and keeping a hidden compatibility alias. | None required. | An SDK alias cannot force rustdoc to print the alias in Concord-generated signatures. Renaming every SDK error transfers the presentation problem to SDK authors. |
| 5 | **Remove redundant explicit format links at their source.** Macros. Prefer implicit intra-doc links over ``[`F`](F)``. | One directly reported rustdoc failure, Europe PMC. Could remove the generator’s three categories of blanket lint allowances. **No established SDK code saving**; removes a documentation-edit trap. | None. | Exact rustdoc behaviour was not reproduced in this read-only sweep. Source inspection supports the maintainer’s diagnosis. |
| 6 | **Keep `maybe_*`, inherent methods, `Sends`, and parameter-group traits. Shorten their presentation.** Macros/docs. | `maybe_*`: **120 invocation lines in 28 SDKs** across source/tests. `Sends`: generic bounds in seven SDKs. Groups: ten declarations in eight SDKs. | No removal recommended. | Removing these would restore forwarding code or require imports for ordinary calls. |

The generator evidence is in [emit.rs](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:16), principally:

- Controls: `741–840`.
- Request summaries and format explanations: `892–1175`.
- Client/config/scope generation: `1287–1490`.
- Policy-backed send and endpoint generation: `1530–1743`.
- Conditional `Clone`: `1827–1870`.
- Documentation/navigation duplication: `1873–2017`.
- Header-name validation: `2370–2437`.

### What Concord actually generates

These are observed generator facts, rather than inferred API requirements.

| Owner | Generated surface | Important qualification |
|---|---|---|
| Client | Client type, hidden state, `Clone`, `Debug`, `new`, `with_base_url`, navigation, controls, `validate_header_names`; a `{Client}Config` when required client values exist | Config fields accept exact types. `new` remains available beside SDK convenience constructors. |
| Scope | Scope type, hidden state, `Clone`, `Debug`, navigation and controls | Scope setters replace values in a new immutable snapshot. They can also expose changes an SDK must restrict. |
| Endpoint | Request type, controls, conditional `Clone`, `Debug`, inherent `operation`, `prepare`, `send`, and `Endpoint<T>` | `prepare()` remains outside the send policy. |
| Policy-backed endpoint | Additionally `Sends<O>` | No `Sends` implementation is generated without a client send policy: `emit.rs:1580–1582,1660–1666`; `concord-core/src/prepared.rs:242–266`. |
| Parameter group | A named setter trait, plus forwarding implementations on including endpoints | Ordinary callers retain inherent setters. Group trait documentation repeats macro terminology: `emit.rs:43–137`. |
| Every optional value/body | `set_x(value)` and `maybe_x(Option<T>)` | `maybe_*` accepts exact `Option<T>`, intentionally unlike `set_*`’s `IntoValue<T>`: `emit.rs:779–805`; `concord-core/src/value.rs:29–36`. |

A request summary is generated on both the endpoint type and its navigation method. With 316 declared endpoints, that means **632 summary placements**. This is useful for discovery, but expensive prose is consequently repeated.

### FIRST_USE complaint census

Counts below are **SDK counts**, once per complaint category. Categories overlap. They count recorded complaints, including ones subsequently addressed.

Paths in this table mean `examples/<name>-sdk/FIRST_USE.md:<lines>`.

| Complaint | Count | Evidence |
|---|---:|---|
| Bare `Error` hides which crate owns the type | **12** | adyen-checkout:209–211; arxiv:184–190; digitalocean:211–218; dropbox:219–224; europe-pmc:224–228; frankfurter:154–160; kraken-spot-rest:182–184; sentry:233–238; square-payments:190–192; strava:214–221; twilio:178–185; xero-accounting:174–178 |
| Policy projection obscures ordinary `send()` result | **7** | cloudflare-zones:117–119; discord-rest:135–140; kraken-spot-rest:165–169; nasa-neows:113–116; postmark:121–126; usgs-earthquake-catalog:170–173; zendesk-support:123–125 |
| `Records<F,T>` obscures SDK reader name | **2** | anthropic-messages:148–156; mastodon:89–94 |
| Header-name maintenance function leads client docs | **5** | arxiv:195–201; europe-pmc:211–214; kraken-spot-rest:170–172; square-payments:186–189; usgs-earthquake-catalog:174–176 |
| Repeated framework prose / boilerplate | **7** | adyen-checkout:215–218; arxiv:195–201; digitalocean:223–228; europe-pmc:221–223; kraken-spot-rest:170–172; paypal:139–145; square-payments:186–189 |
| Base/path boundary unclear | **6** | airtable:152–159; cloudflare-zones:130–134; nasa-neows:117–124; sentry:239–244; spotify-web-api:161–169; zendesk-support:128–132 |
| Production default presented although construction selects another environment | **1** | quickbooks-online:162–167 |
| Generic override documentation conflicts with SDK policy | **2** | kraken-spot-rest:187–193; paypal:139–145 |
| Fixed query values absent from generated endpoint docs | **1** | europe-pmc:215–220 |
| Concrete codec `Accept` value unavailable/unclear | **4** | arxiv:45–46; digitalocean:227–228; europe-pmc:219–220; fred:22 |
| Dynamic `QueryPairs` contributions missing from endpoint documentation | **1** | mailgun:224–228 |
| Original/private generated names leak through aliases, configuration, or identity | **4** | backblaze-b2-native:126–139; dropbox:179–186,219–224; mastodon:89–94; twilio:186–190 |
| Generated names stutter | **1** | strava:222–224: `OAuthOauth`, `OAuthOauthToken`, `oauth.oauth().token(..)` |
| Scope-qualified placeholder surprises | **1** | sentry:245–247: `{issue.issue}` |
| Generated `Clone` bounds expose unused lifetime machinery | **2** | digitalocean:223–226; usgs-earthquake-catalog:182–183 |
| Required query described as “when present” | **1** | frankfurter:176–179 |
| Required argument’s wire role insufficiently explained | **1** | adyen-checkout:215–218: `key` versus `api_key` |
| Policy described as “sent normally” despite never being sent | **1** | arxiv:195–201 |
| `maybe_*` conversion asymmetry requires allocation syntax | **1** | square-payments:222–223 |
| Rustdoc’s non-exhaustive wording suggests valid matching is impossible | **1** | square-payments:196–198 |
| Author-only declaration advice appears in caller error text | **1** | sentry:265–268 |
| SDK re-exported auth documentation still speaks to SDK authors | **1** | square-payments:193–195 |

Two qualifications matter:

1. The result-template mechanism already resolves the projection complaint for ordinary result shapes. Current projection-bearing clients are Airtable, S3, Cloudflare, Frankfurter, Kraken, MediaWiki, Nager Holidays and World Bank—**eight SDKs / 23 endpoints**.
2. Current `owner_docs` already mentions a `query(..)` value’s type at `emit.rs:1897–1905`. It does not enumerate runtime keys, and the navigation method does not receive that same note. Mailgun’s complaint is therefore partly addressed, rather than wholly absent.

### Per-SDK evidence table

All rows use `examples/<SDK>-sdk/` as their directory.

- **L** counts physical lines inside `api!` invocations, including documentation. These are declaration costs, **not claimed removable overhead**.
- **C/Sc/E** means client/scope/endpoint counts.
- **P**: generated client publicly usable; **H**: generated client hidden behind SDK API; **A**: generated scope/request types exported under aliases; **R**: public wire API also exists beside a domain facade.
- **G** is parameter-group count; **S** means handwritten generic code uses `Sends`.
- **Opt** counts `.maybe_*` invocation lines in production source. `+Nt` adds test invocation lines. Zero means absent from that searched use category, not absent from generated public methods.
- **F/U/M** give relevant `FRICTION.md` / `FIRST_USE.md` / `MAINTENANCE.md` line anchors. `—` means no distinct theme complaint found; `∅` means that note file is absent. Positive or historical discussions are included where relevant.

| SDK | Declaration evidence; L; C/Sc/E | Presentation and actual use | F / U / M |
|---|---|---|---|
| adyen-checkout | `src/lib.rs:28–75`; **48**; 1/2/4 | P; policy result explicit; Opt 0 | 41–48 / 204–247 / 231 |
| airtable | `src/lib.rs:184–200`; **17**; 1/0/1 | H; wrapper maps options; Opt **5**, `src/lib.rs:323–327` | 5 / 152–176 / 108 |
| amazon-s3 | `src/api.rs:30–195`; **166**; 1/1/4 | H+A, `src/lib.rs:72–75`; signed generated send; Opt **1**, `src/client.rs:513` | 30–35,90–96 / 93–96 / 50–61 |
| anthropic-messages | `src/lib.rs:69–114`; **46**; 1/1/3 | P; stream reader named manually; Opt 0 | 27–39,80–90 / 148–156 / 130–139 |
| arxiv | `src/lib.rs:27–67`; **41**; 1/1/3 | P; **G1, S**; Opt 0 | 11 / 184–203 / 230–246 |
| backblaze-b2-native | `src/wire.rs:18–75`; **58**; 1/4/6 | H; Opt **5**, `src/storage.rs:183–186`, `src/upload.rs:756` | 52–57 / 121–139 / 108–112 |
| clickhouse | `src/wire.rs:21–76`; **56**; 1/0/4 | H; prepared execution and runtime reader selection; Opt **4**, `src/client.rs:338–340`, `src/insert.rs:140` | 24,87–93 / — / 39 |
| cloudflare-zones | `src/lib.rs:59–140`; **82**; 1/4/5 | P; **G1, S**; heterogeneous policy output; Opt 0 | 8–25,47–62 / 117–134 / 136 |
| datadog-events | `src/api.rs:18–68`; **51**; 1/1/3 | R, `src/lib.rs:12–16`; Opt **2**, `src/client.rs:165`, `src/query.rs:262` | 24–25,92–99 / 149–175 / 88–93 |
| digitalocean | `src/lib.rs:22–54`; **33**; 1/2/5 | P; **G1, S**; Opt 0 | 8–22 / 211–241 / 50–51,83 |
| discord-rest | `src/api.rs:24–106`; **83**; 1/2/4 | P; `query(..) anchor` prevents conflicting cursors; Opt **1+1t**, `src/api.rs:139` | 34–53 / 135–147 / 6–7,32–38 |
| distribution-registry | `src/api.rs:32–126`; **93**; 3/0/12 | H; generated send unused; Opt **18**, principally `src/pages.rs:138–183`, plus upload/blob/manifest/client dispatch | 116–145 / — / 55 |
| dropbox | `src/wire.rs:22–47`; **26**; 1/1/2 | H; generated config aliased at `src/lib.rs:17`; Opt 0 | 23–34 / 179–224 / 104 |
| elasticsearch | `src/lib.rs:55–100`; **46**; 1/1/6 | R; hidden public dispatch trait at `src/lib.rs:122–129`; Opt **1**, `src/lib.rs:238` | 120–127 / 142–158 / 118 |
| europe-pmc | `src/lib.rs:71–128`; **58**; 1/3/6 | P; fixed wire values and URL read-back; Opt 0 | 10 / 211–261 / 158–162,187–202 |
| frankfurter | `src/lib.rs:19–54`; **36**; 1/0/2 | P; former `fetch` removed; Opt **0+1t**, `tests/first_use.rs:216` | 18 / 154–179 / 34 |
| fred | `src/lib.rs:76–138`; **63**; 1/1/7 | P; **G2, S**; Opt **0+3t**, `tests/observation_options.rs:214–216` | 21–25 / 18–22 / 153 |
| github | `src/api.rs:48–311`; **264**; 1/2/22 | P; typed context helpers; Opt **19**, `src/context.rs:342–352,369–376` | 109–144 / ∅ / ∅ |
| gitlab | `src/api.rs:37–93`; **57**; 1/1/3 | P; generated send replaces `Call`; Opt 0 | 71–85 / 124–129,144–148 / 46–47 |
| google-drive | `src/api.rs:9–58`; **50**; 1/3/8 | H; upload/session facade; Opt **3**, `src/client.rs:157–159` | 31–47 / resume discovery in ordered trace / 49–61 |
| hubspot-crm | `src/lib.rs:22–58`; **37**; 2/2/5 | R; public wire clients beside coordinated HubSpot; Opt 0 | 5–16 / 166–178 / 41,77 |
| intercom | `src/lib.rs:70–102`; **33**; 1/1/4 | P; inherent domain extension; Opt **1**, `src/pages.rs:26` | 10,22 / 13–14 / 160 |
| kraken-spot-rest | `src/lib.rs:30–67`; **38**; 1/2/5 | P; envelope-unwrapping policy; Opt **1**, `src/trades.rs:58` | 44 / 165–193,213–214 / 144,178 |
| kubernetes | `src/api.rs:44–243`; **200**; 1/8/27 | P; **G2**; SDK resource traits remain; Opt **14+1t**, `src/list.rs:57–82`, `src/watch.rs:197–223` | 8–70 / ∅ / ∅ |
| linear | `src/lib.rs:47–60`; **14**; 1/0/2 | R; GraphQL facade interprets wire envelopes; Opt 0 | 5–6 / 44 / 135–141 |
| mailgun | `src/lib.rs:59–96`; **38**; 1/1/2 | P; dynamic query range; Opt 0 | 13 / 215–250 / 163 |
| mastodon | `src/api.rs:13–103`; **91**; 1/0/7 | H+A, `src/lib.rs:97–102`; **G1**; Opt 0 | 31–72,104–135 / 89–99 / 125–126 |
| mediawiki-action-api | `src/lib.rs:34–84`; **51**; 1/1/4 | P; envelope policy and dynamic continuation; Opt **2+1t**, `src/pagination.rs:28`, `src/batch.rs:157` | 34–41 / 180–226 / 235–240 |
| microsoft-graph | `src/api.rs:7–27`; **21**; 1/0/3 | H; typed listing facade; Opt **5**, `src/client.rs:198,249,274–276` | — / 224–252 / 108 |
| nager-holidays | `src/lib.rs:15–39`; **25**; 1/0/1 | P; former `fetch` absorbed into send policy; Opt 0 | 26–28 / — / 107–110 |
| nasa-neows | `src/lib.rs:59–77`; **19**; 1/1/3 | P; **S** scopes additive `SendMetered`; Opt 0 | 5 / 113–142 / 162–170 |
| notion | `src/api.rs:9–33`; **25**; 1/0/3 | R; domain client wraps public wire API; Opt 0 | 36,48 / 17 / 26–27 |
| ntfy | `src/lib.rs:101–158`; **58**; 1/1/4 | P; inherent credential helper; Opt **0+5t**, `tests/workflows.rs:65,343,532`, `tests/first_use.rs:192,479` | 113–129 / 137–142 / 11,41 |
| open-library | `src/lib.rs:22–63`; **42**; 1/3/4 | P; policy-backed send; Opt **0+3t**, `tests/maintenance.rs:100`, `tests/protocol.rs:109–110` | 28–44 / — / 148 |
| open-meteo | `src/lib.rs:23–39`; **17**; 1/0/1 | H; runtime-format facade; Opt 0 | 14 / 15 / 72 |
| openai | `src/lib.rs:22–53`; **32**; 1/0/7 | R; domain methods add input/stream rules; Opt 0 | 15 / 150–154 / 122–124 |
| paypal | `src/auth.rs:68–80`; `src/lib.rs:59–171`; **126**; 2/4/9 | P main client; policy retains destination binding; Opt 0 | 77–92 / 139–162 / 63,85 |
| pokeapi | `src/lib.rs:70–88`; **19**; 1/0/2 | P; `Client` names generated client; Opt 0 | — / 139–148,179–184 / 77–84 |
| postmark | `src/lib.rs:288–310`; **23**; 1/1/4 | P; SDK output-rule trait remains private; Opt 0 | 5 / 121–147 / 132–150 |
| quickbooks-online | `src/lib.rs:55–148`; **93**; 2/5/6 | P main client, hidden token endpoint; Opt 0 | 22–32,108 / 145–167 / 117 |
| rest-countries | `src/lib.rs:45–105`; **61**; 1/1/5 | P; **G1, S**; Opt 0 | 34–62 / 143–152 / 146–147 |
| salesforce | `src/api.rs:35–173`; **139**; 1/8/24 | R; SDK `QueryRecord` selects concrete generated endpoints; Opt **4**, `src/query.rs:216`, `src/bulk_query.rs:367–368,628` | 17–48 / ∅ / ∅ |
| sentry | `src/lib.rs:32–77`; **46**; 1/2/5 | P; operation identities enumerated for limiter; Opt **1+1t**, `src/pagination.rs:22` | 11 / 233–288 / 141–171 |
| shopify | `src/api.rs:20–73`; **54**; 1/1/11 | R; generated whole-body setters, SDK GraphQL variable builders; Opt 0 | 29–31,72 / ∅ / ∅ |
| slack | `src/wire.rs:31–55`; **25**; 1/0/2 | H; generated config aliased at `src/lib.rs:16`; Opt **5**, `src/lib.rs:155–157,216–217` | 16–19 / — / 21 |
| snowflake | `src/api.rs:32–57`; **26**; 1/1/4 | H; status/result facade; Opt 0 | 9–28 / 162–184 / — |
| spotify-web-api | `src/auth.rs:165–194`; `src/lib.rs:74–135`; **92**; 2/2/8 | P main client; **G1, S**; Opt **2**, `src/auth.rs:293–294` | 16–20,108–129 / 150–179 / 140–145 |
| square-payments | `src/lib.rs:48–96`; **49**; 1/1/5 | P; policy returns SDK error; Opt **1+1t**, `src/lib.rs:130`, `tests/first_use.rs:512` | 12 / 186–223 / 21 |
| strava | `src/lib.rs:73–131`; **58**; 2/4/6 | P; generated OAuth names stutter; Opt 0 | 16 / 214–263 / — |
| stripe | `src/lib.rs:166–480`; **315**; 1/6/19 | P; handwritten account/list traits over generated controls; Opt **4**, `src/context.rs:104,180`, `src/pages.rs:151,154` | 75–122 / ∅ / ∅ |
| twilio | `src/lib.rs:21–46`; **26**; 1/0/2 | H; four operation/prepare methods retained; hidden old `Error` alias | 23 / 178–203 / 64 |
| usgs-earthquake-catalog | `src/lib.rs:28–73`; **46**; 1/2/3 | P; validated helper beside generated scope entry; Opt **2**, `src/lib.rs:86–87` | 11 / 170–223 / 58–67 |
| world-bank | `src/lib.rs:182–198`; **17**; 1/0/1 | P; fixed wire-envelope policy output; Opt 0 | 3–5 / 151–158 / 139 |
| xero-accounting | `src/auth.rs:39–48`; `src/lib.rs:36–56`; **31**; 2/1/4 | H; tenant-bound wrappers prevent context mutation; Opt **1**, `src/lib.rs:348` | 15–23 / 174–230 / 163 |
| zendesk-support | `src/lib.rs:41–103`; **63**; 1/2/4 | P; subdomain client and policy result; Opt **1**, `src/pagination.rs:29` | 28,120 / 120–148 / — |

No SDK is absent from the overall theme: every SDK uses generated surface. The zeros and dashes above identify absent submechanics or absent complaints.

### Distinct mechanics and their implications

**1. Result templates work; wire-envelope transformations remain a separate problem.**

Observed:

- Existing support substitutes the endpoint output into a declared template and enforces equality with the policy’s actual output: `emit.rs:1635–1666`.
- Cloudflare’s `Single<R>` and `Paged<R>` become different public response wrappers: `examples/cloudflare-zones-sdk/src/policy.rs:156–157,209–210`.
- Kraken unwraps `Envelope<T>`: `examples/kraken-spot-rest-sdk/src/policy.rs:146–147`.
- MediaWiki similarly maps `Envelope<T>`: `examples/mediawiki-action-api-sdk/src/policy.rs:10–11`.

The smallest current cleanup needs **no Concord change** for five SDKs:

| SDK | Existing statement | Result it can explicitly name |
|---|---|---|
| Airtable | `src/lib.rs:190` | `Result<Response<_>, Error>` |
| S3 | `src/api.rs:38` | `Result<_, S3Error>` |
| Frankfurter | `src/lib.rs:34` | `Result<RateResponse, Error>` |
| Nager Holidays | `src/lib.rs:24` | `Result<HolidayResponse, Error>` |
| World Bank | `src/lib.rs:187` | `Result<IndicatorPage, Error>` |

That changes five existing declaration lines, removes **zero** SDK lines, and clarifies nine endpoint signatures.

For Cloudflare, Kraken and MediaWiki—**14 endpoints**—a richer template language would add API and declaration machinery. My inference is that this sweep does **not** justify it yet. First improve the generated distinction between caller result and wire decoder, retaining the policy link.

**2. Reader naming is a documentation projection problem.**

`Records<F,I>` is already an alias for `<F as Incremental<I>>::Records`: `crates/concord-core/src/codec.rs:339`.

The macro cannot resolve another crate’s associated type to discover that the SDK calls it `MessageStream` or `PublicStream`. The smallest fix is generated prose linking the format’s reader implementation, plus the SDK’s existing reader-name comment.

- Anthropic’s manual reader explanation: `src/lib.rs:105–108`, four lines.
- Mastodon’s manual explanations: `src/lib.rs:190–203`.
- Six SDKs have typed stream endpoints that could benefit: S3, Anthropic, Kubernetes, Mastodon, ntfy and USGS.

**Ownership:** macros/docs. **Public API:** none. **Guaranteed removable lines:** zero; the provider-specific stream contract still belongs in SDK documentation. Adding an explicit reader-type annotation would add SDK code merely to improve presentation.

**3. Base replacement, context ownership and validation are different mechanics.**

Observed restrictions cannot be replaced by a universal public generated builder:

- Mastodon hides its generated client to bind a credential to an instance: `src/api.rs:1–4`; `FRICTION.md:31–60`.
- Xero hides setters that could separate tenant identity from its grant: `FRICTION.md:15–23`; `src/lib.rs:59–64`.
- Twilio’s policy prototype saved little code and weakened an input rule. Its integration comparison is **44 documented lines retained versus a 30-line prototype**, not a demonstrated safe deletion: `INTEGRATION.md:257–262`.
- Slack’s evaluated policy version added **14 non-comment lines** and exposed raw argument constraints: `INTEGRATION.md:151–158`.

A runtime-base declaration or opt-in restricted generated view might help Mastodon/Xero. However, neither establishes provider ownership by itself. **No defensible cross-SDK deletion total** can be assigned without designing and checking that boundary.

The immediate change is documentation-only: separate base/path facts and explain that generated `new`, setters and `prepare()` are lower-level construction paths.

**4. Setters and `maybe_*` encode useful distinctions.**

Square’s allocation complaint is real: `set_cursor("&str")` works while `maybe_cursor(Some(...))` needs owned `String`.

But changing `maybe_x` to accept `Option<impl IntoValue<T>>` makes bare `None` lose its concrete input type. Existing clearing calls then require annotations. Stripe also relies on **three states**:

- inherit: `maybe_account_override(None)`;
- explicitly omit: `set_account_override(None::<HeaderValue>)`;
- replace: `set_account_override(Some(...))`.

Evidence: `examples/stripe-sdk/src/context.rs:175–184`.

**Recommendation:** keep the API and explain the asymmetry beside generated methods. **SDK lines removed:** zero. Removing `maybe_*` would add branching or forwarding in the 28 observed users.

**5. Parameter groups remove repetition, but their generated traits are not the entire SDK abstraction.**

Ten group declarations occur in eight SDKs. Eight group traits are used as handwritten generic bounds across seven SDKs:

- arXiv `Paging`: `src/lib.rs:120`.
- Cloudflare `Paging`: `src/pagination.rs:81`.
- DigitalOcean `Paging`: `src/pagination.rs:138`.
- FRED `OffsetOptions`: `src/pagination.rs:38`.
- Kubernetes `ListOptions` / `WatchOptions`: `src/list.rs:51`, `src/watch.rs:191`.
- REST Countries `ListOptions`: `src/lib.rs:170`.
- Spotify `PageParams`: `src/pages.rs:59,69`.

FRED’s `ObservationOptions` and Mastodon’s `TimelineOptions` are used for declaration reuse without a checked-in generic bound. Their generated trait APIs are therefore possible documentation-shrinking targets, **not evidence that the group feature is unused**.

Kubernetes still needs `type Item` and provider resource behaviour. Stripe’s list cursors have different typed IDs. Deleting those SDK traits on the assumption that a parameter-group trait replaces them would lose information.

**Smallest change:** reuse resolved wire-placement documentation on group trait setters and replace “includes `..Paging`” prose with caller language. `arxiv-sdk/MAINTENANCE.md:240–243` identifies the exact gap. **API addition/removal:** none. **SDK lines removed:** zero.

**6. `Sends` and `operation()` have concrete users.**

`Sends` is used generically in arXiv, Cloudflare, DigitalOcean, FRED, NASA, REST Countries and Spotify. NASA uses the result bound to keep its extension trait off unrelated Concord endpoints: `src/lib.rs:216–223`; `MAINTENANCE.md:58–61`.

Associated `operation()` calls appear on **31 source/test lines in 13 SDKs**:

- S3 `src/client.rs:510`;
- ClickHouse `src/client.rs:362`;
- Cloudflare `src/policy.rs:153,161`;
- Datadog `src/api.rs:90`;
- Discord `src/api.rs:156`, `src/limiter.rs:365`;
- Kraken `tests/workflows.rs:60`, `tests/trades.rs:166`;
- PayPal `src/policy.rs:130–134,176`;
- QuickBooks `src/policy.rs:31`;
- Sentry `src/policy.rs:88–92,104,110`;
- Square `src/error.rs:43`;
- Strava `src/auth.rs:264–268,296`, `src/error.rs:115`;
- Twilio `src/lib.rs:144`;
- Zendesk `src/policy.rs:167`.

These select write/retry behaviour, preserve identity in SDK errors, and configure limiter keys.

**Recommendation:** retain both APIs. Move identity documentation into an advanced section rather than removing identity access. A generated operation inventory is requested by Sentry alone (`MAINTENANCE.md:169–171`); generating another item for every client is not yet justified by its five-entry list.

**7. Missing getters cause wire read-back; indiscriminate getters would enlarge the surface.**

The cleanest measured example is Europe PMC:

- `src/lib.rs:61–69`: **nine lines** explaining and reading `pageSize` from the prepared URL.
- `src/pagination.rs:98–101`: **four lines** recovering and revalidating it for traversal.
- The send policy also needs it at `src/lib.rs:47–48`.

A scope getter could reduce the four-line traversal expression to one line: **three lines saved in one SDK**. It would **not** remove the shared nine-line helper, because generic `Endpoint<T>` policies still cannot read the scope value.

Similar requests/read-backs appear in arXiv, S3, Discord, GitLab, Kraken, Spotify, Stripe and Xero’s notes. They are not one interchangeable mechanic: some need typed input, others need trusted base/context or a request fingerprint.

**Candidate:** opt-in access to a specific value, if a broader cross-theme review establishes the required policy interface. **Do not generate getters for every credential and input by default.** The exact wider savings remain undecided.

### Sealed conversion traits

These are two distinct constraints.

| Trait | Observed SDK demand | Actual current cost | Smallest possible change; assessment |
|---|---|---|---|
| `IntoValue` | Cloudflare wants literal/borrowed identifier conversion: `FRICTION.md:47–62`. Anthropic wants its ordinary borrowed conversion accepted: `MAINTENANCE.md:130–135`. | Cloudflare explicitly reports **no SDK implementation workaround**—the cost is caller construction/clone syntax. Anthropic retains an explicit owned snapshot adapter. | Core opt-in conversion support would add a trait or implementations. **No measured SDK LOC saving.** Do not replace the closed set with blanket `Into`: integer inference and explicit validated construction are intentional guarantees. |
| `ToHeaderValue` | Nine SDKs report closed-set friction: S3 `FRICTION.md:88–96`; Anthropic `80–90`; ClickHouse `87–93`; Dropbox `23–34`; GitHub `134–144`; GitLab `71–80`; ntfy `123–129`; REST Countries `34–47`; Stripe `75–107`. | Templates generally cost one placement line each. ntfy reports three lines; ClickHouse three placements. REST Countries’ helper has a three-line function body at `src/lib.rs:41–43`. | Unsealing the existing value-returning trait could preserve typed headers, but each SDK would then add an impl. **No demonstrated net line reduction across these nine SDKs.** |

The guarantees are explicit in `concord-core/src/value.rs:17–36` and `concord-core/src/header.rs:14–40`.

Counterexamples are important:

- Dropbox’s JSON header still needs SDK serialization.
- GitLab and ntfy deliberately avoid implementing `Display` for credentials.
- A native `HeaderValue` preserves exact bytes and sensitivity.
- Accepting arbitrary `Display` headers globally would weaken that distinction.

This evidence supports documenting the extension boundary more clearly. It does not support presenting blanket conversion as a simplification with established savings.

### Naming, hidden items and the rustdoc lint trap

Generated names concatenate the parent type and PascalCase child name; identities concatenate dotted declaration names: `concord-macros/src/model.rs:2129–2142`.

Consequently:

- Strava’s `OAuth` plus `oauth` produces the recorded stutter.
- Re-exporting `ApiPublicTimeline as PublicTimelineRequest` does not rewrite the original implementation heading.
- Private wire identifiers remain in `Operation` identities and generated `Debug`.

The smallest response is **docs and declaration naming**, not automatic deduplication. Registry already renamed private declarations to match its public concepts: `distribution-registry-sdk/FRICTION.md:134–145`. Automatic removal of repeated words could introduce collisions or change stable diagnostic identities.

Dropbox and Slack still re-export generated configuration types while hiding the corresponding generated clients:

- Dropbox: `src/lib.rs:17`.
- Slack: `src/lib.rs:16`.

That is **two one-line re-exports**, but replacing them with proper SDK configuration records adds code. The author guide already advises against this arrangement at `docs/AUTHOR_GUIDE.md:1829–1832`; it is a documentation/API-design trade-off, not an unimplemented core feature.

Europe PMC’s lint report is unusually actionable:

- The maintainer observed warning-denied rustdoc fail after naming `SearchXml` in an endpoint comment: `MAINTENANCE.md:158–162`.
- The generator creates explicit format links at `emit.rs:1131–1132`.
- Generated structs allow the lint at `1398`, `1472`, `1699`.
- Navigation methods carry the same summary without that allowance: `2003–2009`.

**Inference:** emitting implicit links is preferable to spreading suppressions to more generated items. It removes the source of redundant links and permits removal of the broad allowances. No new annotation should be required from SDK authors.

### Other FIRST_USE documentation complaints

These should remain in the report without being converted into macro features. Each entry below is one SDK’s report; several are now historical or have integration addenda.

| SDK(s) | Documentation complaint and evidence | Ownership |
|---|---|---|
| Airtable | README lacks a concrete incomplete-result match, `167–176`; reusable fixture discovery absent, `32` | SDK examples |
| Amazon S3 | Cached ETag reconstruction uses `parse`, unlike surrounding `new` examples, `93–96`; completion wording needs failure qualification, `87–92` | SDK docs |
| Adyen | JSON wire field names/omission absent, `212–214`; endpoint lacks its idempotency explanation, `219–222`; replay/703 revocation wording incomplete, `225–231` | SDK/provider contract |
| arXiv | Undocumented User-Agent, `202–203`; resume-start meaning, `204–206`; boundary error variant and search/fetch strictness insufficiently explained, `207–213,234–239` | SDK docs |
| Backblaze B2 | Filename percent-encoding safe set unspecified, `131–134` | SDK docs |
| ClickHouse | Exception text normalization not specified, `145–148` | SDK docs; minor |
| Datadog | Repeated-cursor failure occurs on requesting the advertised cursor, not on receiving its page, `156–175` | SDK pagination docs |
| DigitalOcean | Whether `Bearer ` is added is unclear, `219–222`; no useful incomplete-result alias, `237–241` | SDK docs/API |
| Discord, FRED, Intercom, QuickBooks, REST Countries, Spotify | Pagination methods, incomplete prefixes, metadata or cap/resume examples require separate discovery: respectively `141–147`; `20–21`; `14`; `148–160`; `143–147`; `224–227` | SDK examples + pagination docs |
| Dropbox | `PagesError::Display` contradicts its documentation, `188–197`; failed fan-out context deserves explanation, `207–211`; advertised asynchronous fixture alias lacks an entry point, `213–217` | Pagination docs / SDK / testserver respectively |
| Elasticsearch, HubSpot, REST Countries | `Items` versus `PageItems` causes navigation ambiguity: `142–158`; `169–170`; `143–147` | Pagination docs, **three SDKs** |
| HubSpot | Required OAuth `token_type` absent from public response information; later uncertain-refresh shape differs from prediction, `171–178` | SDK docs |
| Intercom | Private required contact discriminator invisible in public model docs, `13` | SDK model docs |
| Kraken | Renamed `descr.type` field unexplained, `185–186`; signing diagnostic misleading on public calls, `187–193`; pair-selection case sensitivity unspecified, `198–200` | SDK docs/diagnostics |
| Mailgun | MessageId Display wording ambiguous, `229–232`; rate-limit header names absent, `239–241`; cycle/untrusted-link resume behaviour differs from prose, `251–260`; official timestamp/recipient descriptions conflict with SDK assumptions, `268–275` | SDK/provider evidence |
| MediaWiki | Pagination re-export path absent from README, `185–187`; “empty continuation” ambiguous, `192–199`; missing-ID classification differs from docs, `200–208`; advertised native escape returns an inaccessible envelope, `219–223` | SDK docs/API |
| Microsoft Graph | Missing versus empty required type gets different classification; shortened example omits a required field; ErrorKind naming needs an unre-exported crate, `243–252` | SDK docs/API |
| NASA | README lacks terminal-page/partial-result example, `125–130` | SDK examples |
| Notion | Exact to-do rendering prefix unspecified, ordered trace `15–17` | SDK rendering docs |
| ntfy | Final JSON value without LF accepted despite README prediction, `137–142` | SDK docs |
| Open-Meteo | CSV envelope described without a literal wire example or fixture pointer, `15` | SDK examples |
| OpenAI | Public event variants versus reader-returned errors require clearer variant-level documentation, `150–154` | SDK stream docs |
| PokéAPI | README’s `into_response()` shorthand hides the required outer-error match, `139–148` | SDK examples |
| Postmark | Omission versus null not specified in initial prediction, `21`; diagnostic inspection needs linked Concord documentation, `133–140` | SDK model/diagnostic docs |
| Sentry | Required dependency enum re-exports absent, `262–264`; diagnostic wording conflates origin/path refusals, `269–271` | SDK exports/diagnostics |
| Snowflake | Missing manifest classification differs from documentation, `162–168`; Concord types not re-exported, `169–176`; poll defaults absent, `177–180`; status/poll 422 distinction unstated, `181–185` | SDK docs/API; manifest issue has an integration correction |
| Square | Pagination re-export claim is incomplete, `199–202` | SDK docs/exports |
| Strava | Token refresh timestamp precedence unclear, `229–232`; dependency payload types not re-exported, `233–238`; traversal docs mention unsupported filters, `239–244`; “Ambiguous” names conflate representations, `252–255`; “production” polling wording implies a nonexistent alternative, `260–263` | SDK docs/API |
| Twilio | Debug hides more fields than documentation suggests, `191–195` | SDK docs |
| USGS | CSV missing-column classification depends on data/type and conflicts with the documented table, `206–215` | SDK codec/docs |
| Xero | Credential-error nesting unspecified, `194–200`; reauthorization consequences incomplete, `216–227`; refresh failures use invoice-stage wording, `228–230` | SDK docs/diagnostics |
| Zendesk | Replay advice lacks corroboration in the reviewed provider material, `126–127` | SDK provider evidence |

Separate documentation-tooling reports should not drive Concord API changes:

- **11 SDKs** report unavailable BeautifulSoup/`bs4`: S3, Distribution Registry, Elasticsearch, FRED, Linear, Notion, OpenAI, PayPal, Slack, World Bank and Zendesk.
- **Four** report script/boilerplate pollution in HTML extraction: Anthropic, Datadog, Elasticsearch and PayPal.
- Anthropic additionally reports unavailable `python`; Snowflake reports the absence of plain-text rustdoc output.
- Europe PMC reports redirect-stub navigation at `FIRST_USE.md:258–261`.

### What can shrink, and what remains undecided

**Supported shrinking candidates**

- Hide/reorder trivial `validate_header_names` documentation; preserve callable compatibility initially.
- Remove repeated explanations of macro inheritance, unused clone lifetimes and conversion internals from the normal caller path.
- Replace “Output” with distinct ordinary-send and prepared-decoding descriptions.
- Remove redundant explicit-link generation and its broad lint suppressions.
- Correct “sent normally” for policy values. The model deliberately marks policy values sensitive at `model.rs:572–592`, while `value_notes` unconditionally emits that wording at `emit.rs:626–629`. **Thirteen client-value policies** receive this misleading description; one consumer explicitly reported it.
- Keep private dispatch machinery quiet. Elasticsearch’s `ResponsePolicy` is already public-but-doc-hidden (`src/lib.rs:122–129`); Postmark’s `Reply` lives in a private module (`src/lib.rs:216–230`). Their existence does not establish demand for another universal public trait.

**Not supported by this sweep**

- Removing `prepare`, `operation`, inherent setters, `maybe_*`, `Endpoint`, `Sends`, or parameter groups.
- Replacing SDK ownership wrappers with unrestricted generated handles.
- Automatically deriving runtime query keys or codec constants from arbitrary trait implementations.
- Counting historical forwarding removals as future savings.
- Claiming that absence of checked-in use proves an exported method has no downstream users.

I could not decide the safe shape or net savings of restricted generated views, policy-visible typed inputs, runtime-base construction, or open conversion traits. Those require concrete designs checked against the ownership and inference counterexamples above. The documentation changes can proceed independently and add no public API.