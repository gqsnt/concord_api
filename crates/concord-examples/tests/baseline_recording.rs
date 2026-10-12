//! Baseline recording harness (v3) with full state-equivalence against the
//! captured v2.1 recordings (spec §24.2 MIGRATE-02; V3-R01, V3-R02).
//!
//! The same harness that captured `fixtures/migration/v2_1_riot_recording.json`
//! and `fixtures/migration/v2_1_catalog_recording.json` (its v2.1 source is
//! kept verbatim in `fixtures/migration/baseline_recording_harness_v2_1.rs.txt`)
//! now drives the meaning-preserving v3 Riot client and a v3 migration of the
//! v2.1 Catalog contract through the real generated clients. A recording
//! limiter captures the complete protected rate plan of every attempt and then
//! denies admission, so no request reaches a network: denial happens before
//! native handoff.
//!
//! Equivalence is asserted field by field on exact recorded facts, never on a
//! lossy description: endpoint ID, method, URL scheme/host/route, public query
//! occurrences `(key, value)` in order, public header occurrences in order, the
//! timeout selection, the limiter's endpoint/method/logical origin, and for
//! every policy its ID, cost, source-order windows and complete ordered
//! canonical key `(label, bytes)`. The v3-only schema facts (dimension kinds,
//! encoding IDs, canonical windows, framing version) and the auth placement
//! descriptors are asserted against expectations derived from the v2.1
//! recording and the v2.1 declaration. The sharing relation between endpoint
//! and key contexts is asserted structurally for both recordings and then
//! confirmed dynamically with the real `LocalRateLimiter` on the actual
//! generated v3 keys. The only permitted differences are the named
//! [`EXCEPTIONS`], each tied to a spec rule and pinned to exact before/after
//! values; an exception that no longer matches anything fails the test.
//!
//! The committed v3 recordings `fixtures/migration/v3_riot_recording.json` and
//! `fixtures/migration/v3_catalog_recording.json` must equal the fresh
//! recordings byte for byte, so a later showcase cleanup that changes any
//! recorded state is detected. Set `CONCORD_BASELINE_RECORDING_OUT` /
//! `CONCORD_BASELINE_CATALOG_RECORDING_OUT` to write fresh v3 recordings.
#![forbid(unsafe_code)]
#![cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::future::{Future, IntoFuture};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use concord::endpoint::{RequestPlan, TimeoutSelection};
use concord::error::ErrorCategory;
use concord::inspection::{
    AuthResetMode, ChallengeRule, CredentialSourceKind, EndpointContract, IdSource, PlacementTarget,
};
use concord::observation::{AttemptInfo, SanitizedHeaderValue};
use concord::rate_limit::{
    BoxFuture, CanonicalKeyWriter, Enforcement, LimiterCapability, LocalRateLimiter,
    RATE_KEY_FRAMING_VERSION, RateDimensionKind, RateKeyEncoder, RateLimitError,
    RateLimitErrorKind, RateLimitPermit, RateLimitPlan, RateLimitPolicy, RateLimitRequest,
    RateLimitWindow, RateLimiter, UTF8_ENCODING_ID,
};
use concord::{ApiError, DispatchKnowledge, SecretString};
use concord_examples::riot::api::RiotApi;
use concord_examples::riot::domain::*;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Captured baselines and committed v3 comparison recordings
// ---------------------------------------------------------------------------

const V2_1_RIOT: &str = include_str!("../../../fixtures/migration/v2_1_riot_recording.json");
const V2_1_CATALOG: &str = include_str!("../../../fixtures/migration/v2_1_catalog_recording.json");
const V2_1_HARNESS: &str =
    include_str!("../../../fixtures/migration/baseline_recording_harness_v2_1.rs.txt");
const V2_1_SHA256SUMS: &str = include_str!("../../../fixtures/migration/SHA256SUMS");
const V3_RIOT: &str = include_str!("../../../fixtures/migration/v3_riot_recording.json");
const V3_CATALOG: &str = include_str!("../../../fixtures/migration/v3_catalog_recording.json");

/// Capability of the recording limiter. It is a process-local test double.
const RECORDING_CAPABILITY: LimiterCapability =
    LimiterCapability::new(Enforcement::Local, "concord.baseline.recording", 1);

// ---------------------------------------------------------------------------
// Named, documented intentional differences
// ---------------------------------------------------------------------------

/// One intentional difference between the v2.1 and v3 recordings.
///
/// Every exception names the spec rule that selects the v3 behavior, is
/// restricted to one endpoint ID and one public query key, and pins both the
/// exact v2.1 recorded value and the exact v3 recorded value. Key names,
/// occurrence order and every other recorded fact stay under exact equality.
struct Exception {
    name: &'static str,
    rule: &'static str,
    endpoint_id: &'static str,
    query_key: &'static str,
    v2_1_value: &'static str,
    v3_value: &'static str,
}

/// The v2.1 plan projection replaced every pagination-controlled query value
/// with the redaction marker, although neither value is sensitive. In v3 a
/// public value is withheld from automatic observation only when it is
/// explicitly `#[sensitive]` or matches the mandatory name-based rule
/// (PRIVACY-01, §6.4); a pagination cursor/size is an ordinary public value
/// that the plan shows with its page-controlled provenance (§18.5 shows
/// `offset <- endpoint.offset, initialized 0, page-controlled`). The wire
/// value, key, position and rate state are unchanged; only the plan's
/// observation of these two non-sensitive values differs.
const EXCEPTIONS: &[Exception] = &[
    Exception {
        name: "riot.match_ids.start.plan-observation",
        rule: "PRIVACY-01 (§6.4); §18.5 page-controlled values are ordinary public values",
        endpoint_id: "riot_api.regional.match_v5.match_ids_by_puuid",
        query_key: "start",
        v2_1_value: "%3Credacted%3E",
        v3_value: "0",
    },
    Exception {
        name: "riot.match_ids.count.plan-observation",
        rule: "PRIVACY-01 (§6.4); §18.5 page-controlled values are ordinary public values",
        endpoint_id: "riot_api.regional.match_v5.match_ids_by_puuid",
        query_key: "count",
        v2_1_value: "%3Credacted%3E",
        v3_value: "20",
    },
    Exception {
        name: "catalog.items_list.offset.plan-observation",
        rule: "PRIVACY-01 (§6.4); §18.5 page-controlled values are ordinary public values",
        endpoint_id: "catalog.items.list",
        query_key: "offset",
        v2_1_value: "%3Credacted%3E",
        v3_value: "0",
    },
    Exception {
        name: "catalog.items_list.count.plan-observation",
        rule: "PRIVACY-01 (§6.4); §18.5 page-controlled values are ordinary public values",
        endpoint_id: "catalog.items.list",
        query_key: "count",
        v2_1_value: "%3Credacted%3E",
        v3_value: "25",
    },
];

// ---------------------------------------------------------------------------
// Comparable recording model
// ---------------------------------------------------------------------------

/// One recorded endpoint context, in the exact shape of the v2.1 recording.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Record {
    label: String,
    endpoint_id: String,
    method: String,
    observed_url: String,
    timeout: String,
    headers: Vec<(String, String)>,
    limiter: LimiterRecord,
}

/// The limiter admission request of one context.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LimiterRecord {
    endpoint: String,
    method: String,
    logical_origin: Option<String>,
    policies: Vec<PolicyRecord>,
}

/// One effective policy with its protected canonical key bytes (hex).
#[derive(Clone, Debug, Eq, PartialEq)]
struct PolicyRecord {
    id: String,
    cost: u32,
    /// `(capacity, duration milliseconds)` in source order.
    windows: Vec<(u32, u128)>,
    /// `(label, lowercase hex canonical bytes)` in key order.
    key: Vec<(String, String)>,
}

/// The value-free v3 schema of one recorded policy.
#[derive(Clone, Debug, Eq, PartialEq)]
struct SchemaRecord {
    id: String,
    /// `(label, kind, encoding ID)` in key order.
    dimensions: Vec<(String, RateDimensionKind, &'static str)>,
    /// `(duration milliseconds, capacity)` in canonical schema order.
    canonical_windows: Vec<(u128, u32)>,
    framing_version: u32,
}

/// One effective auth placement descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AuthRecord {
    target: PlacementTarget,
    credential: &'static str,
    challenge: ChallengeRule,
    source: CredentialSourceKind,
    origin: &'static str,
}

/// One displaced inherited auth placement descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
struct DisplacedRecord {
    target: PlacementTarget,
    credential: &'static str,
    origin: &'static str,
    displaced_by: &'static str,
    mode: AuthResetMode,
}

/// One v3 capture: the comparable record plus v3-only facts and the actual
/// protected plan (retained for dynamic limiter probes).
struct Capture {
    record: Record,
    schemas: Vec<SchemaRecord>,
    plan_summary: Vec<(SchemaRecord, u32)>,
    enforcement_label: &'static str,
    capability: LimiterCapability,
    page_index: u32,
    attempt: AttemptInfo,
    id_source: IdSource,
    source_path: String,
    auth: Vec<AuthRecord>,
    displaced: Vec<DisplacedRecord>,
    plan: RateLimitPlan,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").expect("write to string");
        out
    })
}

fn string(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("recording field `{field}` is a string"))
        .to_owned()
}

fn parse_recording(text: &str) -> Vec<Record> {
    let entries: Vec<Value> = serde_json::from_str(text).expect("recording is JSON");
    entries.iter().map(parse_entry).collect()
}

fn parse_entry(entry: &Value) -> Record {
    let limiter = &entry["limiter"];
    Record {
        label: string(entry, "label"),
        endpoint_id: string(entry, "endpoint_id"),
        method: string(entry, "method"),
        observed_url: string(entry, "observed_url"),
        timeout: string(entry, "timeout"),
        headers: entry["headers"]
            .as_array()
            .expect("headers")
            .iter()
            .map(|pair| {
                (
                    pair[0].as_str().expect("header name").to_owned(),
                    pair[1].as_str().expect("header value").to_owned(),
                )
            })
            .collect(),
        limiter: LimiterRecord {
            endpoint: string(limiter, "endpoint"),
            method: string(limiter, "method"),
            logical_origin: limiter["logical_origin"].as_str().map(str::to_owned),
            policies: limiter["policies"]
                .as_array()
                .expect("policies")
                .iter()
                .map(|policy| PolicyRecord {
                    id: string(policy, "id"),
                    cost: u32::try_from(policy["cost"].as_u64().expect("cost")).expect("u32"),
                    windows: policy["windows"]
                        .as_array()
                        .expect("windows")
                        .iter()
                        .map(|window| {
                            (
                                u32::try_from(window[0].as_u64().expect("capacity")).expect("u32"),
                                u128::from(window[1].as_u64().expect("duration")),
                            )
                        })
                        .collect(),
                    key: policy["key"]
                        .as_array()
                        .expect("key")
                        .iter()
                        .map(|part| {
                            (
                                part[0].as_str().expect("label").to_owned(),
                                part[1].as_str().expect("hex").to_owned(),
                            )
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

// ---------------------------------------------------------------------------
// Recording limiter
// ---------------------------------------------------------------------------

struct Admission {
    endpoint: String,
    method: String,
    logical_origin: Option<String>,
    page_index: u32,
    attempt: AttemptInfo,
    capability: LimiterCapability,
    plan: RateLimitPlan,
}

#[derive(Default)]
struct Recorder {
    admissions: Mutex<Vec<Admission>>,
}

impl Recorder {
    fn len(&self) -> usize {
        self.admissions.lock().expect("recorder").len()
    }
}

/// Captures the complete protected plan of an attempt, then denies admission
/// so the attempt ends before native handoff.
struct RecordingLimiter(Arc<Recorder>);

impl RateLimiter for RecordingLimiter {
    fn capability(&self) -> LimiterCapability {
        RECORDING_CAPABILITY
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.0.admissions.lock().expect("recorder").push(Admission {
            endpoint: request.endpoint.to_owned(),
            method: request.method.as_str().to_owned(),
            logical_origin: request.logical_origin.map(str::to_owned),
            page_index: request.page_index,
            attempt: request.attempt,
            capability: request.capability,
            plan: request.plan.clone(),
        });
        Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
    }
}

fn header_pairs(plan: &RequestPlan) -> Vec<(String, String)> {
    plan.public_headers()
        .iter()
        .map(|(name, value)| {
            let value = match value {
                SanitizedHeaderValue::Text(text) => text.to_string(),
                SanitizedHeaderValue::Redacted => "<redacted>".to_owned(),
                SanitizedHeaderValue::NonUtf8 => "<non-utf8>".to_owned(),
            };
            (name.as_str().to_owned(), value)
        })
        .collect()
}

fn timeout_text(selection: TimeoutSelection) -> String {
    format!("{selection:?}")
}

fn policy_record(policy: &RateLimitPolicy) -> PolicyRecord {
    PolicyRecord {
        id: policy.id().as_str().to_owned(),
        cost: policy.cost().get(),
        windows: policy
            .windows()
            .iter()
            .map(|window| (window.capacity().get(), window.duration().as_millis()))
            .collect(),
        key: policy
            .key()
            .parts()
            .iter()
            .map(|part| (part.name().to_owned(), hex(part.value())))
            .collect(),
    }
}

fn schema_record(schema: &concord::rate_limit::RatePolicySchema) -> SchemaRecord {
    SchemaRecord {
        id: schema.id().as_str().to_owned(),
        dimensions: schema
            .dimensions()
            .iter()
            .map(|dimension| {
                (
                    dimension.label().to_owned(),
                    dimension.kind(),
                    dimension.encoding_id(),
                )
            })
            .collect(),
        canonical_windows: schema
            .windows()
            .iter()
            .map(|window| (window.duration().as_millis(), window.capacity().get()))
            .collect(),
        framing_version: schema.framing_version(),
    }
}

fn auth_records(contract: &EndpointContract) -> (Vec<AuthRecord>, Vec<DisplacedRecord>) {
    let placements = contract
        .auth()
        .placements()
        .iter()
        .map(|placement| AuthRecord {
            target: placement.placement().target(),
            credential: placement.placement().credential(),
            challenge: placement.placement().challenge_rule(),
            source: placement.source_kind(),
            origin: placement.origin(),
        })
        .collect();
    let displaced = contract
        .auth()
        .displaced()
        .iter()
        .map(|placement| DisplacedRecord {
            target: placement.placement().target(),
            credential: placement.placement().credential(),
            origin: placement.origin(),
            displaced_by: placement.displaced_by(),
            mode: placement.mode(),
        })
        .collect();
    (placements, displaced)
}

/// Records one context: the unbound intent planned through the client, the
/// directly bound request's own plan, and exactly one denied admission of
/// the bound request's execution.
async fn record<T>(
    label: &str,
    recorder: &Recorder,
    unbound_plan: RequestPlan,
    bound_plan: RequestPlan,
    execution: impl Future<Output = Result<T, ApiError>>,
) -> Capture {
    assert_eq!(
        unbound_plan.observed_url(),
        bound_plan.observed_url(),
        "{label}: unbound and direct plans agree (V3-L12)"
    );
    assert_eq!(
        header_pairs(&unbound_plan),
        header_pairs(&bound_plan),
        "{label}: unbound and direct plan headers agree"
    );
    let before = recorder.len();
    let error = match execution.await {
        Ok(_) => panic!("{label}: the recording limiter must deny admission"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::RateLimit, "{label}");
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched,
        "{label}: limiter denial happens before native handoff"
    );
    let mut admissions = recorder.admissions.lock().expect("recorder");
    assert_eq!(
        admissions.len(),
        before + 1,
        "{label}: exactly one admission"
    );
    let admission = admissions.pop().expect("admission");
    drop(admissions);

    let description = unbound_plan.description();
    let contract = description
        .contract()
        .unwrap_or_else(|| panic!("{label}: generated endpoints carry a contract"));
    let (auth, displaced) = auth_records(contract);
    let summary = unbound_plan.rate_limit();
    Capture {
        record: Record {
            label: label.to_owned(),
            endpoint_id: unbound_plan.endpoint_id().to_owned(),
            method: unbound_plan.method().as_str().to_owned(),
            observed_url: unbound_plan.observed_url().to_owned(),
            timeout: timeout_text(unbound_plan.timeout_selection()),
            headers: header_pairs(&unbound_plan),
            limiter: LimiterRecord {
                endpoint: admission.endpoint,
                method: admission.method,
                logical_origin: admission.logical_origin,
                policies: admission
                    .plan
                    .policies()
                    .iter()
                    .map(policy_record)
                    .collect(),
            },
        },
        schemas: admission
            .plan
            .policies()
            .iter()
            .map(|policy| schema_record(policy.schema()))
            .collect(),
        plan_summary: summary
            .policies()
            .iter()
            .map(|policy| (schema_record(policy.schema()), policy.cost().get()))
            .collect(),
        enforcement_label: summary.enforcement_label(),
        capability: admission.capability,
        page_index: admission.page_index,
        attempt: admission.attempt,
        id_source: contract.identity().source(),
        source_path: description.source_path().to_owned(),
        auth,
        displaced,
        plan: admission.plan,
    }
}

macro_rules! rec {
    ($out:ident, $recorder:ident, $client:ident, $label:expr, $unbound:expr, $bound:expr) => {{
        let endpoint = $unbound;
        let unbound_plan = $client.plan(&endpoint).expect("unbound plan");
        let bound = $bound;
        let bound_plan = bound.plan().expect("direct plan");
        let capture = record(
            &$label,
            &$recorder,
            unbound_plan,
            bound_plan,
            bound.into_future(),
        )
        .await;
        $out.push(capture);
    }};
}

// ---------------------------------------------------------------------------
// v3 Riot recording (46 endpoints x 2 origins)
// ---------------------------------------------------------------------------

fn riot(recorder: &Arc<Recorder>) -> RiotApi {
    let limiter = RecordingLimiter(Arc::clone(recorder));
    RiotApi::builder()
        .api_key(SecretString::try_new("riot-baseline-sentinel").expect("bounded key"))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()
        .expect("baseline Riot client")
}

async fn record_riot() -> Vec<Capture> {
    let recorder = Arc::new(Recorder::default());
    let client = riot(&recorder);
    let mut out = Vec::new();
    for platform in [PlatformRoute::Na1, PlatformRoute::Euw1] {
        let p = format!("{platform:?}");
        let u = || RiotApi::unbound().platform(platform);
        let b = || client.platform(platform);
        macro_rules! both {
            ($name:literal, $($path:tt)+) => {
                rec!(out, recorder, client, format!("{}:{}", p, $name), u().$($path)+, b().$($path)+)
            };
        }
        both!(
            "masteries_by_puuid",
            champion_mastery_v4().masteries_by_puuid("puuid-a".to_owned())
        );
        both!(
            "mastery_by_champion",
            champion_mastery_v4().mastery_by_champion("puuid-a".to_owned(), 7)
        );
        // v2.1 `set_count(Some(3))` was an exact outer-slot replacement; v3
        // spells that `replace_count` (§25.2).
        both!(
            "top_masteries_by_puuid",
            champion_mastery_v4()
                .top_masteries_by_puuid("puuid-a".to_owned())
                .replace_count(Some(3))
        );
        both!(
            "mastery_score_by_puuid",
            champion_mastery_v4().mastery_score_by_puuid("puuid-a".to_owned())
        );
        both!("champion_rotations", champion_v3().champion_rotations());
        both!(
            "clash_players_by_puuid",
            clash_v1().clash_players_by_puuid("puuid-a".to_owned())
        );
        both!("clash_team", clash_v1().clash_team("team-a".to_owned()));
        both!(
            "clash_tournament_by_team",
            clash_v1().clash_tournament_by_team("team-a".to_owned())
        );
        both!("clash_tournament", clash_v1().clash_tournament(11));
        both!("clash_tournaments", clash_v1().clash_tournaments());
        both!(
            "challenger_by_queue",
            league_v4().challenger_by_queue(LeagueQueue::RankedSolo5x5)
        );
        both!(
            "grandmaster_by_queue",
            league_v4().grandmaster_by_queue(LeagueQueue::RankedSolo5x5)
        );
        both!(
            "master_by_queue",
            league_v4().master_by_queue(LeagueQueue::RankedFlexSr)
        );
        both!(
            "league_entries",
            league_v4().league_entries(
                LeagueQueue::RankedSolo5x5,
                LeagueTier::Gold,
                LeagueDivision::Ii
            )
        );
        both!(
            "league_entries_by_puuid",
            league_v4().league_entries_by_puuid("puuid-a".to_owned())
        );
        both!(
            "league_by_id",
            league_v4().league_by_id("league-a".to_owned())
        );
        both!(
            "league_exp_entries",
            league_exp_v4()
                .league_exp_entries(
                    LeagueQueue::RankedSolo5x5,
                    LeagueTier::Gold,
                    LeagueDivision::I
                )
                .set_page(2)
        );
        both!("challenge_configs", challenges_v1().challenge_configs());
        both!(
            "challenge_percentiles",
            challenges_v1().challenge_percentiles()
        );
        both!("challenge_config", challenges_v1().challenge_config(5));
        both!(
            "challenge_leaderboard",
            challenges_v1()
                .challenge_leaderboard(5, ChallengeLevel::Master)
                .replace_limit(Some(10))
        );
        both!(
            "challenge_percentiles_by_id",
            challenges_v1().challenge_percentiles_by_id(5)
        );
        both!(
            "challenge_player_data",
            challenges_v1().challenge_player_data("puuid-a".to_owned())
        );
        both!("platform_data", status_v4().platform_data());
        both!(
            "active_game_by_puuid",
            spectator_v5().active_game_by_puuid("puuid-a".to_owned())
        );
        both!(
            "summoner_by_puuid",
            summoner_v4().summoner_by_puuid("puuid-a".to_owned())
        );
        both!(
            "summoner_by_id",
            summoner_v4().summoner_by_id("summoner-a".to_owned())
        );
    }
    for regional in [RegionalRoute::Americas, RegionalRoute::Europe] {
        let p = format!("{regional:?}");
        let u = || RiotApi::unbound().regional(regional);
        let b = || client.regional(regional);
        macro_rules! both {
            ($name:literal, $($path:tt)+) => {
                rec!(out, recorder, client, format!("{}:{}", p, $name), u().$($path)+, b().$($path)+)
            };
        }
        both!(
            "account_by_riot_id",
            account_v1().account_by_riot_id("game".to_owned(), "tag".to_owned())
        );
        both!(
            "account_by_puuid",
            account_v1().account_by_puuid("puuid-a".to_owned())
        );
        both!(
            "active_region",
            account_v1().active_region(RiotGame::LeagueOfLegends, "puuid-a".to_owned())
        );
        both!(
            "match_ids_by_puuid",
            match_v5()
                .match_ids_by_puuid("puuid-a".to_owned())
                .replace_start_time(Some(100))
                .replace_end_time(Some(200))
                .replace_queue(Some(420))
                .replace_match_type(Some(MatchType::Ranked))
        );
        both!(
            "replays_by_puuid",
            match_v5().replays_by_puuid("puuid-a".to_owned())
        );
        both!("match_by_id", match_v5().match_by_id("match-a".to_owned()));
        both!(
            "timeline_by_id",
            match_v5().timeline_by_id("match-a".to_owned())
        );
        both!(
            "create_stub_codes",
            tournament_stub_v5()
                .create_stub_codes(9, TournamentCodeParameters::default())
                .replace_count(Some(2))
        );
        both!(
            "stub_code",
            tournament_stub_v5().stub_code("code-a".to_owned())
        );
        both!(
            "stub_lobby_events",
            tournament_stub_v5().stub_lobby_events("code-a".to_owned())
        );
        both!(
            "register_stub_provider",
            tournament_stub_v5().register_stub_provider(ProviderRegistrationParameters::default())
        );
        both!(
            "register_stub_tournament",
            tournament_stub_v5()
                .register_stub_tournament(TournamentRegistrationParameters::default())
        );
        both!(
            "create_tournament_codes",
            tournament_v5()
                .create_tournament_codes(9, TournamentCodeParameters::default())
                .replace_count(Some(2))
        );
        both!(
            "tournament_code",
            tournament_v5().tournament_code("code-a".to_owned())
        );
        both!(
            "update_tournament_code",
            tournament_v5().update_tournament_code(
                "code-a".to_owned(),
                TournamentCodeUpdateParameters::default()
            )
        );
        both!(
            "tournament_game_by_code",
            tournament_v5().tournament_game_by_code("code-a".to_owned())
        );
        both!(
            "tournament_lobby_events",
            tournament_v5().tournament_lobby_events("code-a".to_owned())
        );
        both!(
            "register_tournament_provider",
            tournament_v5().register_tournament_provider(ProviderRegistrationParameters::default())
        );
        both!(
            "register_tournament",
            tournament_v5().register_tournament(TournamentRegistrationParameters::default())
        );
    }
    assert_eq!(out.len(), 92, "46 endpoints x 2 origins");
    if let Ok(path) = std::env::var("CONCORD_BASELINE_RECORDING_OUT") {
        std::fs::write(path, render_recording(&out)).expect("write v3 recording");
    }
    out
}

// ---------------------------------------------------------------------------
// v3 migration of the v2.1 Catalog contract (separate baseline fixture)
// ---------------------------------------------------------------------------

/// The v2.1 Catalog contract of `crates/concord-examples/src/contracts/
/// catalog_contract.rs` at the captured baseline, converted to v3 without any
/// meaning change: the policy keeps its formal-derived `"tenant"` label and
/// the write keeps its legacy `"client.tenant"` label (§24.2), explicit query
/// and header blocks keep their order, and `authenticate` keeps its explicit
/// `auth replace` fence.
mod migrated {
    use std::num::NonZeroU32;

    use concord::auth::{CredentialMaterial, SecretCredential};
    use concord::pagination::{OffsetInit, OffsetPagination};
    use concord::{AtomError, BoundedAtomWriter, Json, RequestAtom, SecretString, api};

    pub type TenantId = String;
    pub type Locale = String;
    pub type Region = String;
    pub type Language = String;
    pub type Category = String;
    pub type Tag = String;
    pub type ItemId = String;
    pub type CommandId = String;
    pub type PageSize = NonZeroU32;

    pub const DEFAULT_PAGE_SIZE: PageSize = match NonZeroU32::new(25) {
        Some(value) => value,
        None => unreachable!(),
    };

    /// API version path segment.
    pub struct ApiVersion(&'static str);

    impl Default for ApiVersion {
        fn default() -> Self {
            Self("v2")
        }
    }

    impl RequestAtom for ApiVersion {
        fn write_atom(&self, output: &mut BoundedAtomWriter<'_>) -> Result<(), AtomError> {
            output.push_str(self.0)
        }
    }

    #[derive(serde::Serialize)]
    pub struct LoginInput {
        pub user: String,
    }

    #[derive(serde::Serialize)]
    pub struct UpdateInput {
        pub title: String,
    }

    #[derive(serde::Deserialize)]
    #[expect(
        dead_code,
        reason = "decoded response DTO; the recording limiter denies before any response"
    )]
    pub struct ItemSummary {
        pub id: String,
    }

    #[derive(serde::Deserialize)]
    #[expect(
        dead_code,
        reason = "decoded response DTO; the recording limiter denies before any response"
    )]
    pub struct Item {
        pub id: String,
        pub title: String,
    }

    /// Endpoint-backed session material.
    pub struct SessionToken(pub SecretString);

    impl CredentialMaterial for SessionToken {}

    impl SecretCredential for SessionToken {
        fn expose_secret(&self) -> &str {
            self.0.expose_secret()
        }
    }

    impl<'de> serde::Deserialize<'de> for SessionToken {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            SecretString::try_new(value)
                .map(Self)
                .map_err(serde::de::Error::custom)
        }
    }

    api! {
        /// Typed client for the catalog service.
        client CatalogApi {
            base "https://catalog.example.test/root/";
            timeout 20s;

            values {
                /// Tenant whose catalog is visible.
                #[sensitive]
                tenant: TenantId;

                /// Optional localization preference.
                locale?: Locale;

                /// API version segment selected for this client snapshot.
                #[default]
                api_version: ApiVersion;
            }

            credentials {
                secret bootstrap_key;
                credential application_key = api_key(secret.bootstrap_key);
                credential session = endpoint authenticate;
            }

            subdomain ["api"];
            path "{api_version}";

            query {
                "tenant" = tenant;
                "locale" = locale;
            }

            headers {
                "X-Client-Version" = "2";
            }

            auth {
                header "X-Bootstrap-Key" = application_key;
            }

            rate_limits {
                policy tenant_reads(tenant) {
                    id "catalog.tenant.read";
                    key [origin, "tenant" = tenant];
                    cost 1;
                    allow 120 per 1m;
                    allow 5_000 per 1h;
                }
            }

            /// Creates a session credential.
            POST authenticate(
                body input: LoginInput via Json,
            ) -> SessionToken via Json {
                id "catalog.auth.login";
                path "auth/session";
                auth replace {
                    header "X-Bootstrap-Key" = application_key;
                }
                effect non_idempotent;
            }

            scope regional(
                region: Region,
                language?: Language,
            ) {
                subdomain [region];
                path "regions/{region}";

                headers {
                    "Accept-Language" = language;
                }

                auth replace {
                    bearer session challenge unauthorized;
                }

                rate_limit tenant_reads(tenant = client.tenant);

                /// Returns one page of item summaries.
                GET item_summaries(
                    category?: Category,
                    offset: u64 = 0,
                    count: PageSize = DEFAULT_PAGE_SIZE,
                    tags: Vec<Tag>,
                ) -> Vec<ItemSummary> via Json {
                    id "catalog.items.list";
                    path "items";
                    query {
                        "category" = category;
                        "offset" = offset;
                        "count" = count;
                        "tag" = each tags;
                    }
                    paginate via OffsetPagination {
                        init OffsetInit { offset: offset, page_size: count };
                        cursor -> offset;
                    }
                }

                /// Returns one item.
                GET item(id: ItemId) -> Item via Json {
                    id "catalog.item.get";
                    path "items/{id}";
                    expose accepted_bytes;
                }

                /// Applies an idempotent update identified by `command_id`.
                PUT update_item(
                    id: ItemId,
                    #[sensitive]
                    command_id: CommandId,
                    body update: UpdateInput via Json,
                ) -> Item via Json {
                    id "catalog.item.update";
                    path "items/{id}";
                    headers {
                        "Idempotency-Key" = command_id;
                    }
                    effect idempotent;
                    rate_limit {
                        id "catalog.item.write";
                        key [origin, endpoint, "client.tenant" = client.tenant];
                        cost 5;
                        allow 30 per 1m;
                    }
                }
            }
        }
    }
}

/// The same write with the legacy `"client.tenant"` label renamed to
/// `"tenant"`; every other recorded fact (ID, cost, window, origin, endpoint,
/// tenant value) is identical. Used only to prove that a label is
/// state-affecting (§12.3, V3-R02).
mod relabeled {
    use concord::{Json, api};

    pub use super::migrated::{ItemId, Region, TenantId, UpdateInput};

    api! {
        client CatalogRelabeled {
            base "https://catalog.example.test/root/";
            timeout 20s;

            values {
                #[sensitive]
                tenant: TenantId;
            }

            subdomain ["api"];

            scope regional(region: Region) {
                subdomain [region];
                path "regions/{region}";

                PUT update_item(id: ItemId, body update: UpdateInput via Json) -> () via concord::Discard {
                    id "catalog.item.update";
                    path "items/{id}";
                    effect idempotent;
                    rate_limit {
                        id "catalog.item.write";
                        key [origin, endpoint, "tenant" = client.tenant];
                        cost 5;
                        allow 30 per 1m;
                    }
                }
            }
        }
    }
}

fn catalog_client(tenant: &str, recorder: &Arc<Recorder>) -> migrated::CatalogApi {
    let limiter = RecordingLimiter(Arc::clone(recorder));
    let client = migrated::CatalogApi::builder()
        .set_tenant(tenant)
        .bootstrap_key(SecretString::try_new("catalog-bootstrap-sentinel").expect("secret"))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()
        .expect("baseline Catalog client");
    // v2.1 `credentials().session().set(token).await` is the synchronous,
    // explicit local installation `replace_session` in v3 (§11.3).
    client
        .credentials()
        .replace_session(migrated::SessionToken(
            SecretString::try_new("session-sentinel").expect("session"),
        ))
        .expect("install session");
    client
}

async fn record_catalog() -> Vec<Capture> {
    use migrated::{CatalogApi, UpdateInput};
    let recorder = Arc::new(Recorder::default());
    let mut out = Vec::new();
    for tenant in ["tenant-a", "tenant-b"] {
        let client = catalog_client(tenant, &recorder);
        for region in ["eu", "us"] {
            let label = format!("{tenant}:{region}");
            rec!(
                out,
                recorder,
                client,
                format!("{label}:item_summaries"),
                CatalogApi::unbound()
                    .regional(region.to_owned())
                    .item_summaries(vec!["t1".to_owned(), "t2".to_owned()]),
                client
                    .regional(region.to_owned())
                    .item_summaries(vec!["t1".to_owned(), "t2".to_owned()])
            );
            rec!(
                out,
                recorder,
                client,
                format!("{label}:item"),
                CatalogApi::unbound()
                    .regional(region.to_owned())
                    .item("item-1".to_owned()),
                client.regional(region.to_owned()).item("item-1".to_owned())
            );
            rec!(
                out,
                recorder,
                client,
                format!("{label}:update_item"),
                CatalogApi::unbound()
                    .regional(region.to_owned())
                    .update_item(
                        "item-1".to_owned(),
                        "cmd-1".to_owned(),
                        UpdateInput {
                            title: "t".to_owned()
                        }
                    ),
                client.regional(region.to_owned()).update_item(
                    "item-1".to_owned(),
                    "cmd-1".to_owned(),
                    UpdateInput {
                        title: "t".to_owned()
                    }
                )
            );
        }
    }
    assert_eq!(out.len(), 12);
    if let Ok(path) = std::env::var("CONCORD_BASELINE_CATALOG_RECORDING_OUT") {
        std::fs::write(path, render_recording(&out)).expect("write v3 recording");
    }
    out
}

// ---------------------------------------------------------------------------
// v3 recording rendering (committed comparison fixtures)
// ---------------------------------------------------------------------------

fn json(value: &str) -> String {
    serde_json::to_string(value).expect("JSON string")
}

fn kind_label(kind: RateDimensionKind) -> &'static str {
    match kind {
        RateDimensionKind::Origin => "origin",
        RateDimensionKind::Endpoint => "endpoint",
        RateDimensionKind::Custom => "custom",
    }
}

fn target_text(target: PlacementTarget) -> String {
    match target {
        PlacementTarget::Header(name) | PlacementTarget::Query(name) => {
            format!("{} {name}", target.label())
        }
        PlacementTarget::Authorization {
            scheme: Some(scheme),
        } => format!("authorization {scheme}"),
        other => other.label().to_owned(),
    }
}

/// Renders one capture: the exact v2.1 line shape, followed by a `v3` member
/// holding the v3-only schema, capability, identity and auth facts.
fn render_capture(capture: &Capture) -> String {
    let record = &capture.record;
    let mut line = String::new();
    write!(
        line,
        "{{\"label\":{},\"endpoint_id\":{},\"method\":{},\"observed_url\":{},\"timeout\":{},\"headers\":[",
        json(&record.label),
        json(&record.endpoint_id),
        json(&record.method),
        json(&record.observed_url),
        json(&record.timeout)
    )
    .expect("write");
    for (index, (name, value)) in record.headers.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        write!(line, "[{},{}]", json(name), json(value)).expect("write");
    }
    write!(
        line,
        "],\"limiter\":{{\"endpoint\":{},\"method\":{},\"logical_origin\":{},\"policies\":[",
        json(&record.limiter.endpoint),
        json(&record.limiter.method),
        record
            .limiter
            .logical_origin
            .as_deref()
            .map_or_else(|| "null".to_owned(), json)
    )
    .expect("write");
    for (index, policy) in record.limiter.policies.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        write!(
            line,
            "{{\"id\":{},\"cost\":{},\"windows\":[",
            json(&policy.id),
            policy.cost
        )
        .expect("write");
        for (w, (capacity, millis)) in policy.windows.iter().enumerate() {
            if w > 0 {
                line.push(',');
            }
            write!(line, "[{capacity},{millis}]").expect("write");
        }
        line.push_str("],\"key\":[");
        for (p, (label, bytes)) in policy.key.iter().enumerate() {
            if p > 0 {
                line.push(',');
            }
            write!(line, "[{},{}]", json(label), json(bytes)).expect("write");
        }
        line.push_str("]}");
    }
    write!(
        line,
        "]}},\"v3\":{{\"id_source\":{},\"page_index\":{},\"attempt\":{},\"capability\":[{},{},{}],\"enforcement\":{},\"schemas\":[",
        json(capture.id_source.label()),
        capture.page_index,
        capture.attempt.index,
        json(match capture.capability.enforcement() {
            Enforcement::Local => "local",
            Enforcement::Unenforced => "unenforced",
            Enforcement::SharedPersistent => "shared_persistent",
        }),
        json(capture.capability.namespace()),
        capture.capability.algorithm_version(),
        json(capture.enforcement_label)
    )
    .expect("write");
    for (index, schema) in capture.schemas.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        write!(
            line,
            "{{\"id\":{},\"framing\":{},\"dimensions\":[",
            json(&schema.id),
            schema.framing_version
        )
        .expect("write");
        for (d, (label, kind, encoding)) in schema.dimensions.iter().enumerate() {
            if d > 0 {
                line.push(',');
            }
            write!(
                line,
                "[{},{},{}]",
                json(label),
                json(kind_label(*kind)),
                json(encoding)
            )
            .expect("write");
        }
        line.push_str("],\"canonical_windows\":[");
        for (w, (millis, capacity)) in schema.canonical_windows.iter().enumerate() {
            if w > 0 {
                line.push(',');
            }
            write!(line, "[{millis},{capacity}]").expect("write");
        }
        line.push_str("]}");
    }
    line.push_str("],\"auth\":[");
    for (index, auth) in capture.auth.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        write!(
            line,
            "[{},{},{},{},{}]",
            json(&target_text(auth.target)),
            json(auth.credential),
            json(auth.challenge.label()),
            json(auth.source.label()),
            json(auth.origin)
        )
        .expect("write");
    }
    line.push_str("],\"displaced_auth\":[");
    for (index, displaced) in capture.displaced.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        write!(
            line,
            "[{},{},{},{},{}]",
            json(&target_text(displaced.target)),
            json(displaced.credential),
            json(displaced.origin),
            json(displaced.displaced_by),
            json(displaced.mode.label())
        )
        .expect("write");
    }
    line.push_str("]}}");
    line
}

fn render_recording(captures: &[Capture]) -> String {
    format!(
        "[\n{}\n]\n",
        captures
            .iter()
            .map(render_capture)
            .collect::<Vec<_>>()
            .join(",\n")
    )
}

// ---------------------------------------------------------------------------
// Equivalence assertions
// ---------------------------------------------------------------------------

fn split_url(url: &str) -> (&str, Vec<(&str, &str)>) {
    match url.split_once('?') {
        None => (url, Vec::new()),
        Some((route, query)) => (
            route,
            query
                .split('&')
                .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
                .collect(),
        ),
    }
}

/// Asserts exact state-equivalence of one context, applying only the named
/// exceptions. Returns the names of the exceptions it applied.
fn assert_record_equivalent(v2_1: &Record, v3: &Record) -> Vec<&'static str> {
    let label = &v2_1.label;
    assert_eq!(v3.label, v2_1.label, "context order");
    assert_eq!(v3.endpoint_id, v2_1.endpoint_id, "{label}: endpoint ID");
    assert_eq!(v3.method, v2_1.method, "{label}: method");
    assert_eq!(v3.timeout, v2_1.timeout, "{label}: timeout selection");
    assert_eq!(
        v3.headers, v2_1.headers,
        "{label}: public header occurrences in order"
    );
    assert_eq!(
        v3.limiter, v2_1.limiter,
        "{label}: limiter endpoint/method/origin and every policy ID, cost, window and canonical key"
    );

    let (v2_route, v2_query) = split_url(&v2_1.observed_url);
    let (v3_route, v3_query) = split_url(&v3.observed_url);
    assert_eq!(v3_route, v2_route, "{label}: scheme, host and route shape");
    assert_eq!(
        v3_query.len(),
        v2_query.len(),
        "{label}: public query occurrence count ({} vs {})",
        v2_1.observed_url,
        v3.observed_url
    );
    let mut applied = Vec::new();
    for (position, ((v2_key, v2_value), (v3_key, v3_value))) in
        v2_query.iter().zip(v3_query.iter()).enumerate()
    {
        assert_eq!(
            v3_key, v2_key,
            "{label}: query key at occurrence {position}"
        );
        if v3_value == v2_value {
            continue;
        }
        let exception = EXCEPTIONS
            .iter()
            .find(|exception| {
                exception.endpoint_id == v2_1.endpoint_id && exception.query_key == *v2_key
            })
            .unwrap_or_else(|| {
                panic!(
                    "{label}: query `{v2_key}` value changed from `{v2_value}` to `{v3_value}` without a named exception"
                )
            });
        assert_eq!(
            (*v2_value, *v3_value),
            (exception.v2_1_value, exception.v3_value),
            "{label}: exception {} ({}) pins both values",
            exception.name,
            exception.rule
        );
        applied.push(exception.name);
    }
    applied
}

fn assert_equivalent(baseline: &[Record], captures: &[Capture], exception_prefix: &str) {
    assert_eq!(captures.len(), baseline.len(), "context count");
    let mut applied = BTreeSet::new();
    for (v2_1, capture) in baseline.iter().zip(captures) {
        applied.extend(assert_record_equivalent(v2_1, &capture.record));
    }
    for exception in EXCEPTIONS
        .iter()
        .filter(|exception| exception.name.starts_with(exception_prefix))
    {
        assert!(
            applied.contains(exception.name),
            "exception {} is stale: it no longer matches any recorded difference",
            exception.name
        );
    }
}

/// Asserts the v3-only schema facts against expectations derived from the
/// v2.1 recording: built-in dimensions keep their reserved labels and the
/// `utf8` framing, custom dimensions keep their explicit labels (String
/// values, `utf8`), windows are the v2.1 windows in canonical (duration,
/// capacity) order, the framing version is the unchanged v2.1 framing, and
/// the bound plan's value-free summary agrees with the admitted plan.
fn assert_schemas(captures: &[Capture]) {
    for capture in captures {
        let label = &capture.record.label;
        assert_eq!(capture.capability, RECORDING_CAPABILITY, "{label}");
        assert_eq!(capture.page_index, 0, "{label}: direct execution is page 0");
        assert_eq!(capture.attempt.index, 0, "{label}: first attempt");
        assert!(!capture.attempt.authentication_recovery, "{label}");
        assert_eq!(
            capture.schemas.len(),
            capture.record.limiter.policies.len(),
            "{label}"
        );
        for (schema, policy) in capture.schemas.iter().zip(&capture.record.limiter.policies) {
            assert_eq!(schema.id, policy.id, "{label}");
            assert_eq!(schema.framing_version, RATE_KEY_FRAMING_VERSION, "{label}");
            assert_eq!(RATE_KEY_FRAMING_VERSION, 1, "v2.1 framing is version 1");
            let expected_dimensions = policy
                .key
                .iter()
                .map(|(dimension, _)| {
                    let kind = match dimension.as_str() {
                        "origin" => RateDimensionKind::Origin,
                        "endpoint" => RateDimensionKind::Endpoint,
                        _ => RateDimensionKind::Custom,
                    };
                    (dimension.clone(), kind, UTF8_ENCODING_ID)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                schema.dimensions, expected_dimensions,
                "{label}: {} dimension labels, kinds and encoding IDs",
                policy.id
            );
            let mut canonical = policy
                .windows
                .iter()
                .map(|(capacity, millis)| (*millis, *capacity))
                .collect::<Vec<_>>();
            canonical.sort_unstable();
            assert_eq!(
                schema.canonical_windows, canonical,
                "{label}: {} canonical window schema",
                policy.id
            );
        }
        let admitted = capture
            .schemas
            .iter()
            .cloned()
            .zip(
                capture
                    .record
                    .limiter
                    .policies
                    .iter()
                    .map(|policy| policy.cost),
            )
            .collect::<Vec<_>>();
        assert_eq!(
            capture.plan_summary, admitted,
            "{label}: the bound plan summary agrees with the admitted plan (EXPLAIN-04)"
        );
        assert_eq!(
            capture.enforcement_label,
            Enforcement::Local.plan_label(),
            "{label}"
        );
    }
}

// ---------------------------------------------------------------------------
// Sharing relation (structural, then dynamic with LocalRateLimiter)
// ---------------------------------------------------------------------------

/// `policy ID -> {canonical key -> contexts that resolve to it}`.
type Sharing = BTreeMap<String, BTreeMap<Vec<(String, String)>, BTreeSet<String>>>;

fn sharing(records: &[&Record]) -> Sharing {
    let mut relation = Sharing::new();
    for record in records {
        for policy in &record.limiter.policies {
            relation
                .entry(policy.id.clone())
                .or_default()
                .entry(policy.key.clone())
                .or_default()
                .insert(record.label.clone());
        }
    }
    relation
}

/// The sharing groups of one policy: the sets of contexts that share one
/// bucket per window.
fn groups(relation: &Sharing, id: &str) -> BTreeSet<BTreeSet<String>> {
    relation
        .get(id)
        .unwrap_or_else(|| panic!("policy {id} is recorded"))
        .values()
        .cloned()
        .collect()
}

fn labels(prefix: &str, names: &[&str]) -> BTreeSet<String> {
    names
        .iter()
        .map(|name| format!("{prefix}:{name}"))
        .collect()
}

/// Asserts that every policy ID has identical windows and cost-independent
/// schema in every context, so that sharing under a probe window is sharing
/// under the declared windows.
fn assert_uniform_windows(records: &[&Record]) {
    let mut windows = BTreeMap::<&str, &Vec<(u32, u128)>>::new();
    for record in records {
        for policy in &record.limiter.policies {
            let earlier = windows.entry(&policy.id).or_insert(&policy.windows);
            assert_eq!(
                *earlier, &policy.windows,
                "{}: policy {} keeps one window schema",
                record.label, policy.id
            );
        }
    }
}

fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    future.as_mut().poll(&mut context)
}

/// A single-policy plan with the captured ID and actual generated key and a
/// capacity-one probe window.
fn probe_plan(policy: &RateLimitPolicy) -> RateLimitPlan {
    let probe = RateLimitPolicy::new(
        policy.id().clone(),
        policy.key().clone(),
        NonZeroU32::MIN,
        [RateLimitWindow::new(NonZeroU32::MIN, Duration::from_secs(3_600)).expect("window")],
    )
    .expect("probe policy");
    RateLimitPlan::try_from_policies([probe]).expect("probe plan")
}

fn probe(limiter: &LocalRateLimiter, capture: &Capture, plan: &RateLimitPlan) -> bool {
    let method = concord::http::Method::from_bytes(capture.record.method.as_bytes())
        .expect("recorded method");
    let request = RateLimitRequest {
        cooldown_origin: None,
        endpoint: &capture.record.endpoint_id,
        method: &method,
        logical_origin: capture.record.limiter.logical_origin.as_deref(),
        page_index: 0,
        attempt: capture.attempt,
        plan,
        capability: limiter.capability(),
    };
    match poll_once(limiter.acquire(request)) {
        Poll::Ready(Ok(_)) => true,
        Poll::Ready(Err(error)) => panic!("probe admission failed: {error:?}"),
        Poll::Pending => false,
    }
}

/// Confirms the structural relation dynamically: for every ordered pair of
/// contexts carrying the same policy ID, a fresh `LocalRateLimiter` admits
/// the first context's probe, and the second context's probe is blocked
/// exactly when the structural relation says both resolve to one bucket.
fn assert_dynamic_sharing(captures: &[Capture], relation: &Sharing) {
    let mut contexts = BTreeMap::<&str, Vec<(&Capture, RateLimitPlan)>>::new();
    for capture in captures {
        for policy in capture.plan.policies() {
            contexts
                .entry(policy.id().as_str())
                .or_default()
                .push((capture, probe_plan(policy)));
        }
    }
    let mut pairs = 0_usize;
    for (id, members) in &contexts {
        for (first, first_plan) in members {
            for (second, second_plan) in members {
                if std::ptr::eq(*first, *second) {
                    continue;
                }
                let limiter = LocalRateLimiter::new();
                assert!(
                    probe(&limiter, first, first_plan),
                    "{id}: fresh limiter admits {}",
                    first.record.label
                );
                let shared_dynamically = !probe(&limiter, second, second_plan);
                let shared_structurally = relation[*id].values().any(|members| {
                    members.contains(&first.record.label) && members.contains(&second.record.label)
                });
                assert_eq!(
                    shared_dynamically, shared_structurally,
                    "{id}: {} / {} bucket sharing",
                    first.record.label, second.record.label
                );
                pairs += 1;
            }
        }
    }
    assert!(pairs > 0);
}

const PLATFORM_ENDPOINTS: [&str; 27] = [
    "masteries_by_puuid",
    "mastery_by_champion",
    "top_masteries_by_puuid",
    "mastery_score_by_puuid",
    "champion_rotations",
    "clash_players_by_puuid",
    "clash_team",
    "clash_tournament_by_team",
    "clash_tournament",
    "clash_tournaments",
    "challenger_by_queue",
    "grandmaster_by_queue",
    "master_by_queue",
    "league_entries",
    "league_entries_by_puuid",
    "league_by_id",
    "league_exp_entries",
    "challenge_configs",
    "challenge_percentiles",
    "challenge_config",
    "challenge_leaderboard",
    "challenge_percentiles_by_id",
    "challenge_player_data",
    "platform_data",
    "active_game_by_puuid",
    "summoner_by_puuid",
    "summoner_by_id",
];

const REGIONAL_ENDPOINTS: [&str; 19] = [
    "account_by_riot_id",
    "account_by_puuid",
    "active_region",
    "match_ids_by_puuid",
    "replays_by_puuid",
    "match_by_id",
    "timeline_by_id",
    "create_stub_codes",
    "stub_code",
    "stub_lobby_events",
    "register_stub_provider",
    "register_stub_tournament",
    "create_tournament_codes",
    "tournament_code",
    "update_tournament_code",
    "tournament_game_by_code",
    "tournament_lobby_events",
    "register_tournament_provider",
    "register_tournament",
];

const CHAMPION_MASTERY_V4: [&str; 4] = [
    "masteries_by_puuid",
    "mastery_by_champion",
    "top_masteries_by_puuid",
    "mastery_score_by_puuid",
];

const CHALLENGES_V1: [&str; 6] = [
    "challenge_configs",
    "challenge_percentiles",
    "challenge_config",
    "challenge_leaderboard",
    "challenge_percentiles_by_id",
    "challenge_player_data",
];

const TOURNAMENT_STUB_V5: [&str; 5] = [
    "create_stub_codes",
    "stub_code",
    "stub_lobby_events",
    "register_stub_provider",
    "register_stub_tournament",
];

const TOURNAMENT_V5: [&str; 7] = [
    "create_tournament_codes",
    "tournament_code",
    "update_tournament_code",
    "tournament_game_by_code",
    "tournament_lobby_events",
    "register_tournament_provider",
    "register_tournament",
];

/// Per-endpoint attachments in v2.1: each ID covers exactly one endpoint in
/// each origin, even where windows are numerically equal to another ID's.
const PER_ENDPOINT_POLICIES: [(&str, &str); 22] = [
    ("concord.riot.3", "champion_rotations"),
    ("concord.riot.4", "clash_players_by_puuid"),
    ("concord.riot.5", "clash_team"),
    ("concord.riot.6", "clash_tournament_by_team"),
    ("concord.riot.7", "clash_tournament"),
    ("concord.riot.8", "clash_tournaments"),
    ("concord.riot.9", "challenger_by_queue"),
    ("concord.riot.10", "grandmaster_by_queue"),
    ("concord.riot.11", "master_by_queue"),
    ("concord.riot.12", "league_entries"),
    ("concord.riot.13", "league_entries_by_puuid"),
    ("concord.riot.14", "league_exp_entries"),
    ("concord.riot.16", "platform_data"),
    ("concord.riot.17", "active_game_by_puuid"),
    ("concord.riot.18", "summoner_by_puuid"),
    ("concord.riot.19", "summoner_by_id"),
    ("concord.riot.21", "account_by_riot_id"),
    ("concord.riot.22", "account_by_puuid"),
    ("concord.riot.23", "active_region"),
    ("concord.riot.24", "match_ids_by_puuid"),
    ("concord.riot.25", "replays_by_puuid"),
    ("concord.riot.26", "match_by_id"),
];

fn assert_riot_groups(relation: &Sharing) {
    let per_origin = |prefixes: [&str; 2], names: &[&str]| -> BTreeSet<BTreeSet<String>> {
        prefixes
            .iter()
            .map(|prefix| labels(prefix, names))
            .collect()
    };
    let platforms = ["Na1", "Euw1"];
    let regions = ["Americas", "Europe"];
    // The application buckets: one per routing host, shared by every
    // endpoint of the route tree and never across hosts.
    assert_eq!(
        groups(relation, "concord.riot.1"),
        per_origin(platforms, &PLATFORM_ENDPOINTS)
    );
    assert_eq!(
        groups(relation, "concord.riot.20"),
        per_origin(regions, &REGIONAL_ENDPOINTS)
    );
    // The three originally scope-shared groups keep their shared buckets.
    assert_eq!(
        groups(relation, "concord.riot.2"),
        per_origin(platforms, &CHAMPION_MASTERY_V4)
    );
    assert_eq!(
        groups(relation, "concord.riot.15"),
        per_origin(platforms, &CHALLENGES_V1)
    );
    assert_eq!(
        groups(relation, "concord.riot.28"),
        per_origin(regions, &TOURNAMENT_STUB_V5)
    );
    // Method constraints attached per endpoint are never merged.
    for (id, endpoint) in PER_ENDPOINT_POLICIES {
        let origins = if PLATFORM_ENDPOINTS.contains(&endpoint) {
            platforms
        } else {
            regions
        };
        assert_eq!(
            groups(relation, id),
            per_origin(origins, &[endpoint]),
            "{id}"
        );
    }
    assert_eq!(
        groups(relation, "concord.riot.27"),
        per_origin(regions, &["timeline_by_id"])
    );
    assert_eq!(relation.len(), 28, "28 policy IDs");
}

fn assert_riot_policy_membership(records: &[&Record]) {
    for record in records {
        let (_, name) = record.label.split_once(':').expect("label");
        let ids = record
            .limiter
            .policies
            .iter()
            .map(|policy| policy.id.as_str())
            .collect::<Vec<_>>();
        if TOURNAMENT_V5.contains(&name) {
            assert_eq!(ids, ["concord.riot.20"], "{}", record.label);
        } else if name == "league_by_id" {
            assert_eq!(ids, ["concord.riot.1"], "{}", record.label);
        } else {
            assert_eq!(
                ids.len(),
                2,
                "{}: application plus method bucket",
                record.label
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A minimal SHA-256 (FIPS 180-4) used only to prove the captured baseline
/// bytes this test compares against are the ones listed in `SHA256SUMS`.
fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_length = u64::try_from(data.len()).expect("length") * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (index, word) in block.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.iter().fold(String::new(), |mut out, word| {
        write!(out, "{word:08x}").expect("write");
        out
    })
}

#[test]
fn captured_v2_1_baseline_is_unchanged() {
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let sums = V2_1_SHA256SUMS
        .lines()
        .map(|line| line.split_once("  ").expect("sha256sum line"))
        .map(|(sum, path)| (path.trim(), sum.trim()))
        .collect::<BTreeMap<_, _>>();
    for (path, bytes) in [
        ("fixtures/migration/v2_1_riot_recording.json", V2_1_RIOT),
        (
            "fixtures/migration/v2_1_catalog_recording.json",
            V2_1_CATALOG,
        ),
        (
            "fixtures/migration/baseline_recording_harness_v2_1.rs.txt",
            V2_1_HARNESS,
        ),
    ] {
        assert_eq!(
            sums.get(path).copied(),
            Some(sha256(bytes.as_bytes()).as_str()),
            "{path} is the captured baseline"
        );
    }
    assert_eq!(sums.len(), 3);
}

/// V3-R01 / MIGRATE-02: every Riot context keeps its endpoint ID, method,
/// route, public query/header occurrences, timeout, logical origin and every
/// policy ID, cost, window and canonical key byte; only the named plan
/// observation exceptions differ.
#[tokio::test]
async fn riot_v3_recording_is_state_equivalent_to_v2_1() {
    let baseline = parse_recording(V2_1_RIOT);
    assert_eq!(baseline.len(), 92);
    let captures = record_riot().await;
    assert_equivalent(&baseline, &captures, "riot.");
}

/// The v3-only schema facts of every Riot policy are the v2.1 facts: origin
/// labels/kinds, `utf8` encoding IDs, canonical windows and framing version,
/// and the bound plan summary agrees with the admitted plan.
#[tokio::test]
async fn riot_v3_policy_schemas_match_the_v2_1_key_contract() {
    let captures = record_riot().await;
    assert_schemas(&captures);
}

/// MIGRATE-02: the sharing relation between endpoint/key contexts is
/// identical, with two origins per route tree, all three originally
/// scope-shared groups, the application buckets beside the method buckets,
/// and per-endpoint IDs that are never merged even with equal windows. The
/// relation is confirmed dynamically on the actual generated keys.
#[tokio::test(start_paused = true)]
async fn riot_sharing_relation_is_preserved_and_enforced() {
    let baseline = parse_recording(V2_1_RIOT);
    let captures = record_riot().await;
    let v2_records = baseline.iter().collect::<Vec<_>>();
    let v3_records = captures
        .iter()
        .map(|capture| &capture.record)
        .collect::<Vec<_>>();
    assert_uniform_windows(&v2_records);
    assert_uniform_windows(&v3_records);
    let before = sharing(&v2_records);
    let after = sharing(&v3_records);
    assert_eq!(after, before, "sharing relation");
    assert_riot_groups(&before);
    assert_riot_groups(&after);
    assert_riot_policy_membership(&v2_records);
    assert_riot_policy_membership(&v3_records);
    assert_dynamic_sharing(&captures, &after);
}

/// Identity and auth placement of every Riot endpoint: derived v2.1 IDs are
/// reproduced by v3 derivation (no pin needed; none was moved or renamed),
/// and every endpoint keeps the one client-level private API-key header
/// placement with no challenge attribution and no displaced placement.
#[tokio::test]
async fn riot_identity_and_auth_placement_are_preserved() {
    let captures = record_riot().await;
    for capture in &captures {
        let label = &capture.record.label;
        assert_eq!(capture.id_source, IdSource::Derived, "{label}");
        let (_, name) = label.split_once(':').expect("label");
        assert!(
            capture.record.endpoint_id.starts_with("riot_api.")
                && capture.record.endpoint_id.ends_with(&format!(".{name}")),
            "{label}: navigation-derived ID"
        );
        assert_eq!(
            capture.auth,
            [AuthRecord {
                target: PlacementTarget::Header("X-Riot-Token"),
                credential: "riot_api_key",
                challenge: ChallengeRule::Never,
                source: CredentialSourceKind::ApiKey,
                origin: "RiotApi",
            }],
            "{label}: client-level API-key header placement"
        );
        assert!(capture.displaced.is_empty(), "{label}");
        assert!(
            !capture
                .record
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("x-riot-token")),
            "{label}: private placement is not a public header"
        );
        assert!(
            !capture
                .record
                .observed_url
                .contains("riot-baseline-sentinel"),
            "{label}"
        );
    }
}

/// The fresh v3 Riot recording equals the committed v3 comparison fixture
/// byte for byte (detects any later state change, including in cleanup).
#[tokio::test]
async fn riot_v3_recording_matches_committed_v3_fixture() {
    let captures = record_riot().await;
    assert_eq!(render_recording(&captures), V3_RIOT);
    // The committed v3 fixture is itself state-equivalent to v2.1.
    assert_eq!(parse_recording(V3_RIOT).len(), 92);
    let committed = parse_recording(V3_RIOT);
    for (v2_1, v3) in parse_recording(V2_1_RIOT).iter().zip(&committed) {
        assert_record_equivalent(v2_1, v3);
    }
}

/// V3-R02 / MIGRATE-02: the separate Catalog migration fixture keeps the
/// v2.1 keys byte for byte, including the legacy `"client.tenant"` label, for
/// two tenants and two origins.
#[tokio::test]
async fn catalog_v3_recording_is_state_equivalent_to_v2_1() {
    let baseline = parse_recording(V2_1_CATALOG);
    assert_eq!(baseline.len(), 12);
    let captures = record_catalog().await;
    assert_equivalent(&baseline, &captures, "catalog.");
    assert_schemas(&captures);
    for capture in &captures {
        let label = &capture.record.label;
        assert_eq!(capture.id_source, IdSource::Explicit, "{label}");
        assert_eq!(
            capture.auth,
            [AuthRecord {
                target: PlacementTarget::Bearer,
                credential: "session",
                challenge: ChallengeRule::Unauthorized,
                source: CredentialSourceKind::Endpoint,
                origin: "CatalogApi::regional",
            }],
            "{label}"
        );
        assert_eq!(
            capture.displaced,
            [DisplacedRecord {
                target: PlacementTarget::Header("X-Bootstrap-Key"),
                credential: "application_key",
                origin: "CatalogApi",
                displaced_by: "CatalogApi::regional",
                mode: AuthResetMode::Replace,
            }],
            "{label}"
        );
        if label.ends_with(":update_item") {
            let write = capture
                .record
                .limiter
                .policies
                .iter()
                .find(|policy| policy.id == "catalog.item.write")
                .expect("write policy");
            let dimension_labels = write
                .key
                .iter()
                .map(|(dimension, _)| dimension.as_str())
                .collect::<Vec<_>>();
            assert_eq!(dimension_labels, ["origin", "endpoint", "client.tenant"]);
        }
    }
}

/// Catalog sharing: tenants and origins partition both policies; the read
/// policy is shared by the three regional endpoints of one tenant and origin;
/// the write policy covers only the update. Confirmed dynamically.
#[tokio::test(start_paused = true)]
async fn catalog_sharing_relation_is_preserved_and_enforced() {
    let baseline = parse_recording(V2_1_CATALOG);
    let captures = record_catalog().await;
    let v2_records = baseline.iter().collect::<Vec<_>>();
    let v3_records = captures
        .iter()
        .map(|capture| &capture.record)
        .collect::<Vec<_>>();
    assert_uniform_windows(&v2_records);
    assert_uniform_windows(&v3_records);
    let before = sharing(&v2_records);
    let after = sharing(&v3_records);
    assert_eq!(after, before, "sharing relation");
    let contexts = ["tenant-a:eu", "tenant-a:us", "tenant-b:eu", "tenant-b:us"];
    let expected = |names: &[&str]| -> BTreeSet<BTreeSet<String>> {
        contexts
            .iter()
            .map(|context| labels(context, names))
            .collect()
    };
    for relation in [&before, &after] {
        assert_eq!(
            groups(relation, "catalog.tenant.read"),
            expected(&["item_summaries", "item", "update_item"])
        );
        assert_eq!(
            groups(relation, "catalog.item.write"),
            expected(&["update_item"])
        );
        assert_eq!(relation.len(), 2);
    }
    assert_dynamic_sharing(&captures, &after);
}

/// The fresh v3 Catalog recording equals the committed v3 comparison fixture.
#[tokio::test]
async fn catalog_v3_recording_matches_committed_v3_fixture() {
    let captures = record_catalog().await;
    assert_eq!(render_recording(&captures), V3_CATALOG);
}

/// V3-R02 / §12.3: renaming the legacy `"client.tenant"` label to `"tenant"`
/// with the same policy ID, cost, window and tenant value changes the schema
/// and the canonical key, and selects a different bucket; the preserved
/// label reproduces the v2.1 bytes exactly.
#[tokio::test(start_paused = true)]
async fn catalog_client_tenant_label_is_state_affecting() {
    let recorder = Arc::new(Recorder::default());
    let limiter = RecordingLimiter(Arc::clone(&recorder));
    let relabeled_client = relabeled::CatalogRelabeled::builder()
        .set_tenant("tenant-a")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()
        .expect("relabeled client");
    let relabeled_request = relabeled_client.regional("eu").update_item(
        "item-1",
        migrated::UpdateInput {
            title: "t".to_owned(),
        },
    );
    let relabeled_plan = relabeled_request.plan().expect("plan");
    let relabeled = record(
        "tenant-a:eu:update_item",
        &recorder,
        relabeled_client
            .plan(
                &relabeled::CatalogRelabeled::unbound()
                    .regional("eu")
                    .update_item(
                        "item-1",
                        migrated::UpdateInput {
                            title: "t".to_owned(),
                        },
                    ),
            )
            .expect("unbound plan"),
        relabeled_plan,
        relabeled_request.into_future(),
    )
    .await;

    let preserved = record_catalog()
        .await
        .into_iter()
        .find(|capture| capture.record.label == "tenant-a:eu:update_item")
        .expect("preserved capture");
    let baseline = parse_recording(V2_1_CATALOG)
        .into_iter()
        .find(|record| record.label == "tenant-a:eu:update_item")
        .expect("baseline record");

    let find = |capture: &Capture| -> (PolicyRecord, SchemaRecord, RateLimitPolicy) {
        let index = capture
            .record
            .limiter
            .policies
            .iter()
            .position(|policy| policy.id == "catalog.item.write")
            .expect("write policy");
        (
            capture.record.limiter.policies[index].clone(),
            capture.schemas[index].clone(),
            capture.plan.policies()[index].clone(),
        )
    };
    let (preserved_policy, preserved_schema, preserved_actual) = find(&preserved);
    let (relabeled_policy, relabeled_schema, relabeled_actual) = find(&relabeled);
    let baseline_policy = baseline
        .limiter
        .policies
        .iter()
        .find(|policy| policy.id == "catalog.item.write")
        .expect("baseline write policy");

    // The preserved label reproduces the captured v2.1 key exactly.
    assert_eq!(&preserved_policy, baseline_policy);

    // Same ID, cost, windows, origin/endpoint bytes and tenant value bytes.
    assert_eq!(relabeled_policy.id, preserved_policy.id);
    assert_eq!(relabeled_policy.cost, preserved_policy.cost);
    assert_eq!(relabeled_policy.windows, preserved_policy.windows);
    assert_eq!(relabeled_policy.key.len(), preserved_policy.key.len());
    for (relabeled_part, preserved_part) in relabeled_policy.key.iter().zip(&preserved_policy.key) {
        assert_eq!(relabeled_part.1, preserved_part.1, "value bytes are equal");
    }
    assert_eq!(
        relabeled.record.limiter.logical_origin,
        preserved.record.limiter.logical_origin
    );

    // ... yet the schema and the canonical key differ by the label alone.
    assert_eq!(preserved_policy.key[2].0, "client.tenant");
    assert_eq!(relabeled_policy.key[2].0, "tenant");
    assert_ne!(
        relabeled_policy.key, preserved_policy.key,
        "canonical key changed"
    );
    assert_ne!(relabeled_schema, preserved_schema, "schema changed");
    assert_ne!(
        relabeled_actual.schema(),
        preserved_actual.schema(),
        "value-free schema descriptors differ"
    );
    assert_eq!(relabeled_schema.id, preserved_schema.id);
    assert_eq!(
        relabeled_schema.canonical_windows,
        preserved_schema.canonical_windows
    );

    // One limiter object: exhausting the preserved bucket leaves the
    // relabeled bucket untouched, and vice versa.
    let limiter = LocalRateLimiter::new();
    let preserved_probe = probe_plan(&preserved_actual);
    let relabeled_probe = probe_plan(&relabeled_actual);
    assert!(probe(&limiter, &preserved, &preserved_probe));
    assert!(
        !probe(&limiter, &preserved, &preserved_probe),
        "preserved bucket exhausted"
    );
    assert!(
        probe(&limiter, &relabeled, &relabeled_probe),
        "relabeled bucket is separate"
    );
    assert!(!probe(&limiter, &relabeled, &relabeled_probe));
}

// ---------------------------------------------------------------------------
// §24.2 edit battery: dynamic key/dispatch recordings of declaration edits
// ---------------------------------------------------------------------------
//
// A Riot-shaped base client and eight edited copies are recorded through the
// same harness (two platform origins; two tier values for the custom key).
// Each edit states which axes must stay equal and which are intentionally
// changed. The axes are computed from actual generated plans, actual
// limiter admissions and the static contract coverage view; a semantic
// snapshot is not used as a substitute for these dynamic recordings.

/// Canonical tier-name key codec (a custom `RateKeyEncoder`).
pub struct TierName;

impl RateKeyEncoder<LeagueTier> for TierName {
    const ENCODING_ID: &'static str = "battery.tier-name/1";

    fn write_canonical(
        value: &LeagueTier,
        out: &mut CanonicalKeyWriter,
    ) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("utf8", value.to_string().as_bytes())
    }
}

/// A different codec for the same value: the "changed key codec" edit.
pub struct TierOrdinal;

impl RateKeyEncoder<LeagueTier> for TierOrdinal {
    const ENCODING_ID: &'static str = "battery.tier-ordinal/1";

    fn write_canonical(
        value: &LeagueTier,
        out: &mut CanonicalKeyWriter,
    ) -> Result<(), RateLimitError> {
        let ordinal: u8 = match value {
            LeagueTier::Iron => 0,
            LeagueTier::Bronze => 1,
            LeagueTier::Silver => 2,
            LeagueTier::Gold => 3,
            LeagueTier::Platinum => 4,
            LeagueTier::Emerald => 5,
            LeagueTier::Diamond => 6,
        };
        out.write_tagged_bytes("u8", &[ordinal])
    }
}

/// The unedited base.
mod edit_base {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: rename scope `champion_mastery_v4` to `mastery_v4`. The pinned
/// endpoint keeps its identity; the derived one reports a new ID.
mod edit_scope_rename {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: move `top_masteries_by_puuid` out of its scope to the platform
/// scope with the same effective route.
mod edit_endpoint_move {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                    path "champion-mastery/v4/champion-masteries/by-puuid/{puuid}/top";
                    query { "count" = count; }
                }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: insert an ancestor value with the same name (`puuid`) as the
/// endpoint parameter used in the path. Bare names stay local (LANG-03).
mod edit_ancestor_insert {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4(header("X-Puuid-Hint") puuid?: String) {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: hoist `concord.riot.5` from `clash_team` to the `clash_v1` scope.
/// The source is shorter, but the sibling is newly covered.
mod edit_quota_hoist {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";
                    rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: merge two definitions with equal numbers (`concord.riot.5` and
/// `concord.riot.6`, both 200 per 1m) under one ID.
mod edit_merge_equal_numbers {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: change the per-call cost of `concord.riot.5` from 1 to 2.
mod edit_cost {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 2; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: change the key codec of the custom `"tier"` dimension.
mod edit_key_codec {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierOrdinal;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierOrdinal];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// Edit: add an initializer to the optional `count` (`count?: u32 = 3`).
mod edit_add_default {
    use concord::{Json, api};
    use concord_examples::riot::domain::*;

    use super::TierName;

    api! {
        client RiotApi {
            base "https://riotgames.com/";
            credentials {
                secret api_key;
                credential riot_api_key = api_key(secret.api_key);
            }
            auth { header "X-Riot-Token" = riot_api_key; }
            timeout 20s;
            response Json;

            rate_limits {
                policy by_tier(tier) {
                    id "battery.tier";
                    key [origin, "tier" = tier via TierName];
                    allow 30 per 10s;
                }
            }

            scope platform(platform: PlatformRoute) {
                subdomain replace [platform, "api"];
                path "lol";
                rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; allow 30000 per 10m; }

                scope champion_mastery_v4() {
                    path "champion-mastery/v4";
                    rate_limit { id "concord.riot.2"; key [origin]; cost 1; allow 20000 per 10s; allow 1200000 per 10m; }

                    GET masteries_by_puuid(puuid: String) -> Vec<ChampionMasteryDto> {
                        id "riot_api.platform.champion_mastery_v4.masteries_by_puuid";
                        path "champion-masteries/by-puuid/{puuid}";
                        rate_limit { id "battery.method"; key [origin, endpoint]; allow 10 per 1s; }
                    }

                    GET top_masteries_by_puuid(puuid: String, count?: u32 = 3) -> Vec<ChampionMasteryDto> {
                        path "champion-masteries/by-puuid/{puuid}/top";
                        query { "count" = count; }
                    }
                }

                scope clash_v1() {
                    path "clash/v1";

                    GET clash_team(team_id: String) -> ClashTeamDto {
                        path "teams/{team_id}";
                        rate_limit { id "concord.riot.5"; key [origin]; cost 1; allow 200 per 1m; }
                    }

                    GET clash_tournament_by_team(team_id: String) -> ClashTournamentDto {
                        path "tournaments/by-team/{team_id}";
                        rate_limit { id "concord.riot.6"; key [origin]; cost 1; allow 200 per 1m; }
                    }
                }

                scope league_v4() {
                    path "league/v4";

                    GET league_entries(
                        queue: LeagueQueue,
                        tier: LeagueTier,
                        division: LeagueDivision,
                        page: u32 = 1,
                    ) -> Vec<LeagueEntryDto> {
                        path "entries/{queue}/{tier}/{division}";
                        query { "page" = page; }
                        rate_limit by_tier(tier = tier);
                    }
                }
            }
        }
    }
}

/// `(method, observed URL, ordered public headers)` of one context.
type Wire = (String, String, Vec<(String, String)>);

/// `(label, lowercase hex canonical bytes)` in key order.
type CanonicalKey = Vec<(String, String)>;

/// The recorded axes of one edit variant.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Axes {
    /// Context -> operational endpoint ID.
    identity: BTreeMap<String, String>,
    /// Context -> generated source (navigation) path.
    navigation: BTreeMap<String, String>,
    /// Context -> (method, observed URL, ordered public headers).
    wire: BTreeMap<String, Wire>,
    /// Context -> value-free schemas of the admitted plan.
    schema: BTreeMap<String, Vec<SchemaRecord>>,
    /// Context -> `(policy ID, cost)` of the admitted plan.
    cost: BTreeMap<String, Vec<(String, u32)>>,
    /// Context -> `(policy ID, canonical key)` of the admitted plan.
    keys: BTreeMap<String, Vec<(String, CanonicalKey)>>,
    /// Policy ID -> the groups of contexts that resolve to one bucket.
    sharing: BTreeMap<String, BTreeSet<BTreeSet<String>>>,
    /// Policy ID -> endpoint names covered in the static contract view.
    coverage: BTreeMap<String, BTreeSet<String>>,
}

fn axes(captures: &[Capture], contract: &concord::inspection::ContractDescription) -> Axes {
    let records = captures
        .iter()
        .map(|capture| &capture.record)
        .collect::<Vec<_>>();
    let relation = sharing(&records);
    Axes {
        identity: captures
            .iter()
            .map(|capture| {
                (
                    capture.record.label.clone(),
                    capture.record.endpoint_id.clone(),
                )
            })
            .collect(),
        navigation: captures
            .iter()
            .map(|capture| (capture.record.label.clone(), capture.source_path.clone()))
            .collect(),
        wire: captures
            .iter()
            .map(|capture| {
                (
                    capture.record.label.clone(),
                    (
                        capture.record.method.clone(),
                        capture.record.observed_url.clone(),
                        capture.record.headers.clone(),
                    ),
                )
            })
            .collect(),
        schema: captures
            .iter()
            .map(|capture| (capture.record.label.clone(), capture.schemas.clone()))
            .collect(),
        cost: captures
            .iter()
            .map(|capture| {
                (
                    capture.record.label.clone(),
                    capture
                        .record
                        .limiter
                        .policies
                        .iter()
                        .map(|policy| (policy.id.clone(), policy.cost))
                        .collect(),
                )
            })
            .collect(),
        keys: captures
            .iter()
            .map(|capture| {
                (
                    capture.record.label.clone(),
                    capture
                        .record
                        .limiter
                        .policies
                        .iter()
                        .map(|policy| (policy.id.clone(), policy.key.clone()))
                        .collect(),
                )
            })
            .collect(),
        sharing: relation
            .keys()
            .map(|id| (id.clone(), groups(&relation, id)))
            .collect(),
        coverage: contract
            .policies()
            .iter()
            .map(|policy| {
                (
                    policy.id().to_owned(),
                    policy
                        .covered()
                        .iter()
                        .map(|endpoint| {
                            endpoint
                                .path()
                                .rsplit("::")
                                .next()
                                .expect("endpoint name")
                                .to_owned()
                        })
                        .collect(),
                )
            })
            .collect(),
    }
}

macro_rules! rec_bound {
    ($out:ident, $recorder:ident, $label:expr, $bound:expr) => {{
        let label = $label;
        let bound = $bound;
        let first = bound.plan().expect("plan");
        let second = bound.plan().expect("plan");
        let capture = record(&label, &$recorder, first, second, bound.into_future()).await;
        $out.push(capture);
    }};
}

/// Records one edit variant: two platform origins, every endpoint, and two
/// tier values for the custom key dimension.
macro_rules! record_edit {
    ($module:ident, $scope:ident, masteries: $masteries:expr, top: $top:expr) => {{
        let recorder = Arc::new(Recorder::default());
        let limiter = RecordingLimiter(Arc::clone(&recorder));
        let client = $module::RiotApi::builder()
            .api_key(SecretString::try_new("battery-sentinel").expect("key"))
            .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
            .build()
            .expect("battery client");
        let mut out = Vec::new();
        for platform in [PlatformRoute::Na1, PlatformRoute::Euw1] {
            let p = format!("{platform:?}");
            let $scope = client.platform(platform);
            rec_bound!(out, recorder, format!("{p}:masteries_by_puuid"), $masteries);
            rec_bound!(out, recorder, format!("{p}:top_masteries_by_puuid"), $top);
            rec_bound!(
                out,
                recorder,
                format!("{p}:clash_team"),
                $scope.clash_v1().clash_team("team-a")
            );
            rec_bound!(
                out,
                recorder,
                format!("{p}:clash_tournament_by_team"),
                $scope.clash_v1().clash_tournament_by_team("team-a")
            );
            for tier in [LeagueTier::Gold, LeagueTier::Silver] {
                rec_bound!(
                    out,
                    recorder,
                    format!("{p}:league_entries:{tier}"),
                    $scope.league_v4().league_entries(
                        LeagueQueue::RankedSolo5x5,
                        tier,
                        LeagueDivision::I
                    )
                );
            }
        }
        assert_eq!(out.len(), 12);
        axes(&out, $module::RiotApi::contract())
    }};
}

/// Which axes an edit keeps equal (`true`) or intentionally changes (`false`).
struct Expect {
    identity: bool,
    navigation: bool,
    wire: bool,
    schema: bool,
    cost: bool,
    keys: bool,
    sharing: bool,
    coverage: bool,
}

fn assert_axes(edit: &str, base: &Axes, variant: &Axes, expect: &Expect) {
    fn check<T: PartialEq + std::fmt::Debug>(
        edit: &str,
        axis: &str,
        equal: bool,
        base: &T,
        variant: &T,
    ) {
        if equal {
            assert_eq!(variant, base, "{edit}: axis `{axis}` must stay equal");
        } else {
            assert_ne!(variant, base, "{edit}: axis `{axis}` must change");
        }
    }
    check(
        edit,
        "identity",
        expect.identity,
        &base.identity,
        &variant.identity,
    );
    check(
        edit,
        "navigation",
        expect.navigation,
        &base.navigation,
        &variant.navigation,
    );
    check(edit, "wire", expect.wire, &base.wire, &variant.wire);
    check(edit, "schema", expect.schema, &base.schema, &variant.schema);
    check(edit, "cost", expect.cost, &base.cost, &variant.cost);
    check(edit, "keys", expect.keys, &base.keys, &variant.keys);
    check(
        edit,
        "sharing",
        expect.sharing,
        &base.sharing,
        &variant.sharing,
    );
    check(
        edit,
        "coverage",
        expect.coverage,
        &base.coverage,
        &variant.coverage,
    );
}

/// Returns the contexts whose value differs on one axis.
fn changed<T: PartialEq>(base: &BTreeMap<String, T>, variant: &BTreeMap<String, T>) -> Vec<String> {
    assert_eq!(
        base.keys().collect::<Vec<_>>(),
        variant.keys().collect::<Vec<_>>(),
        "same recorded contexts"
    );
    base.iter()
        .filter(|(context, value)| variant.get(*context) != Some(*value))
        .map(|(context, _)| context.clone())
        .collect()
}

fn both_origins(name: &str) -> Vec<String> {
    vec![format!("Euw1:{name}"), format!("Na1:{name}")]
}

async fn record_edit_base() -> Axes {
    record_edit!(
        edit_base,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    )
}

/// V3-R05: a scope rename keeps the pinned endpoint's identity and its
/// `endpoint`-dimension key bytes (state is preserved) while the derived
/// endpoint reports a new ID; wire, rates and sharing are unchanged.
#[tokio::test]
async fn edit_battery_scope_rename_with_pins_preserves_state() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_scope_rename,
        scope,
        masteries: scope.mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "scope rename",
        &base,
        &variant,
        &Expect {
            identity: false,
            navigation: false,
            wire: true,
            schema: true,
            cost: true,
            keys: true,
            sharing: true,
            coverage: true,
        },
    );
    assert_eq!(
        changed(&base.identity, &variant.identity),
        both_origins("top_masteries_by_puuid"),
        "only the derived ID changes"
    );
    assert_eq!(
        variant.identity["Na1:top_masteries_by_puuid"],
        "riot_api.platform.mastery_v4.top_masteries_by_puuid"
    );
    assert_eq!(
        variant.identity["Na1:masteries_by_puuid"],
        "riot_api.platform.champion_mastery_v4.masteries_by_puuid",
        "the pin keeps the operational identity"
    );
    let method_key = |axes: &Axes| {
        axes.keys["Na1:masteries_by_puuid"]
            .iter()
            .find(|(id, _)| id == "battery.method")
            .expect("endpoint-keyed policy")
            .1
            .clone()
    };
    assert_eq!(method_key(&variant), method_key(&base));
    assert_eq!(method_key(&base)[1].0, "endpoint");
}

/// An endpoint move with the same effective route changes the derived ID
/// and drops the scope-shared bucket the endpoint used to share.
#[tokio::test]
async fn edit_battery_endpoint_move_changes_identity_and_sharing() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_endpoint_move,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "endpoint move",
        &base,
        &variant,
        &Expect {
            identity: false,
            navigation: false,
            wire: true,
            schema: false,
            cost: false,
            keys: false,
            sharing: false,
            coverage: false,
        },
    );
    assert_eq!(
        variant.identity["Euw1:top_masteries_by_puuid"],
        "riot_api.platform.top_masteries_by_puuid"
    );
    assert_eq!(
        changed(&base.keys, &variant.keys),
        both_origins("top_masteries_by_puuid")
    );
    let shared = |axes: &Axes| axes.coverage["concord.riot.2"].clone();
    assert_eq!(
        shared(&base),
        ["masteries_by_puuid", "top_masteries_by_puuid"]
            .map(str::to_owned)
            .into()
    );
    assert_eq!(shared(&variant), ["masteries_by_puuid".to_owned()].into());
}

/// LANG-03: inserting a same-named ancestor value does not capture the
/// endpoint's bare `puuid`; with the ancestor absent nothing changes, and
/// when set it only adds its own header.
#[tokio::test]
async fn edit_battery_same_named_ancestor_does_not_capture_local_names() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_ancestor_insert,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "same-named ancestor insertion",
        &base,
        &variant,
        &Expect {
            identity: true,
            navigation: true,
            wire: true,
            schema: true,
            cost: true,
            keys: true,
            sharing: true,
            coverage: true,
        },
    );
    let client = edit_ancestor_insert::RiotApi::builder()
        .api_key(SecretString::try_new("battery-sentinel").expect("key"))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()
        .expect("client");
    let plan = client
        .platform(PlatformRoute::Na1)
        .champion_mastery_v4()
        .set_puuid("ancestor-value")
        .masteries_by_puuid("puuid-a")
        .plan()
        .expect("plan");
    assert!(
        plan.observed_url()
            .ends_with("/champion-masteries/by-puuid/puuid-a"),
        "{}",
        plan.observed_url()
    );
    assert_eq!(
        header_pairs(&plan)
            .iter()
            .find(|(name, _)| name == "x-puuid-hint")
            .map(|(_, value)| value.as_str()),
        Some("ancestor-value")
    );
}

/// RATE-02: hoisting a policy to its scope is shorter source but newly
/// covers the sibling, which now shares that bucket.
#[tokio::test]
async fn edit_battery_quota_hoist_newly_covers_the_sibling() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_quota_hoist,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "quota hoist",
        &base,
        &variant,
        &Expect {
            identity: true,
            navigation: true,
            wire: true,
            schema: false,
            cost: false,
            keys: false,
            sharing: false,
            coverage: false,
        },
    );
    assert_eq!(
        changed(&base.schema, &variant.schema),
        both_origins("clash_tournament_by_team")
    );
    assert_eq!(
        variant.coverage["concord.riot.5"],
        ["clash_team", "clash_tournament_by_team"]
            .map(str::to_owned)
            .into()
    );
    assert_eq!(
        variant.sharing["concord.riot.5"],
        ["Na1", "Euw1"]
            .iter()
            .map(|origin| labels(origin, &["clash_team", "clash_tournament_by_team"]))
            .collect()
    );
}

/// MIGRATE-01: merging two equal-number definitions under one ID is a
/// sharing change, not a refactor.
#[tokio::test]
async fn edit_battery_merging_equal_numbers_is_a_sharing_change() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_merge_equal_numbers,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "merge equal-number definitions",
        &base,
        &variant,
        &Expect {
            identity: true,
            navigation: true,
            wire: true,
            schema: false,
            cost: false,
            keys: false,
            sharing: false,
            coverage: false,
        },
    );
    assert!(!variant.coverage.contains_key("concord.riot.6"));
    assert_eq!(
        variant.sharing["concord.riot.5"],
        ["Na1", "Euw1"]
            .iter()
            .map(|origin| labels(origin, &["clash_team", "clash_tournament_by_team"]))
            .collect()
    );
    assert_eq!(
        base.sharing["concord.riot.5"],
        ["Na1", "Euw1"]
            .iter()
            .map(|origin| labels(origin, &["clash_team"]))
            .collect()
    );
}

/// RATE-03: cost is charged per call and is not part of bucket identity or
/// schema; only the cost axis changes.
#[tokio::test]
async fn edit_battery_cost_change_keeps_schema_and_sharing() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_cost,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "cost change",
        &base,
        &variant,
        &Expect {
            identity: true,
            navigation: true,
            wire: true,
            schema: true,
            cost: false,
            keys: true,
            sharing: true,
            coverage: true,
        },
    );
    assert_eq!(
        changed(&base.cost, &variant.cost),
        both_origins("clash_team")
    );
    assert!(
        variant.cost["Na1:clash_team"].contains(&("concord.riot.5".to_owned(), 2)),
        "{:?}",
        variant.cost["Na1:clash_team"]
    );
}

/// §12.3: changing a key codec keeps the label and order but changes the
/// encoding ID (schema) and the canonical key bytes, so state is not
/// preserved; the partition between tiers and origins is unchanged.
#[tokio::test]
async fn edit_battery_key_codec_change_changes_schema_and_key_bytes() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_key_codec,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "key codec change",
        &base,
        &variant,
        &Expect {
            identity: true,
            navigation: true,
            wire: true,
            schema: false,
            cost: true,
            keys: false,
            sharing: true,
            coverage: true,
        },
    );
    let tier_contexts = ["Na1", "Euw1"]
        .iter()
        .flat_map(|origin| ["GOLD", "SILVER"].map(|tier| format!("{origin}:league_entries:{tier}")))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        changed(&base.schema, &variant.schema)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        tier_contexts
    );
    let tier_dimension = |axes: &Axes| {
        axes.schema["Na1:league_entries:GOLD"]
            .iter()
            .find(|schema| schema.id == "battery.tier")
            .expect("tier policy")
            .dimensions[1]
            .clone()
    };
    assert_eq!(
        tier_dimension(&base),
        (
            "tier".to_owned(),
            RateDimensionKind::Custom,
            "battery.tier-name/1"
        )
    );
    assert_eq!(
        tier_dimension(&variant),
        (
            "tier".to_owned(),
            RateDimensionKind::Custom,
            "battery.tier-ordinal/1"
        )
    );
}

/// Adding an initializer to an optional changes only the wire of that
/// endpoint (PresenceAndDefaults); identity and rate state are unchanged.
#[tokio::test]
async fn edit_battery_adding_a_default_changes_only_presence() {
    let base = record_edit_base().await;
    let variant = record_edit!(
        edit_add_default,
        scope,
        masteries: scope.champion_mastery_v4().masteries_by_puuid("puuid-a"),
        top: scope.champion_mastery_v4().top_masteries_by_puuid("puuid-a")
    );
    assert_axes(
        "add default",
        &base,
        &variant,
        &Expect {
            identity: true,
            navigation: true,
            wire: false,
            schema: true,
            cost: true,
            keys: true,
            sharing: true,
            coverage: true,
        },
    );
    assert_eq!(
        changed(&base.wire, &variant.wire),
        both_origins("top_masteries_by_puuid")
    );
    assert!(
        base.wire["Na1:top_masteries_by_puuid"].1.ends_with("/top"),
        "{:?}",
        base.wire["Na1:top_masteries_by_puuid"]
    );
    assert!(
        variant.wire["Na1:top_masteries_by_puuid"]
            .1
            .ends_with("/top?count=3"),
        "{:?}",
        variant.wire["Na1:top_masteries_by_puuid"]
    );
}
