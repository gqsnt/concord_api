//! Current V3 Riot contract recordings and semantic edit regressions.
//!
//! The generated client is exercised for all 46 endpoints in two origins.
//! A recording limiter captures each complete protected rate plan and denies
//! admission before native handoff. The records retain endpoint identity,
//! method, ordered public query/header occurrences, timeout, logical origin,
//! policy IDs, costs, windows and canonical key bytes. Schema, capability and
//! authentication descriptors are captured alongside those request facts.
//!
//! Fresh output must match `fixtures/contracts/v3_riot_recording.json` byte
//! for byte. Tests separately verify policy membership, bucket sharing through
//! the real `LocalRateLimiter`, endpoint identities and private auth placement.
//! The edit battery checks which observable axes change when a V3 declaration
//! is edited. Set `CONCORD_BASELINE_RECORDING_OUT` to write a fresh recording.
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
// Current V3 contract recording
// ---------------------------------------------------------------------------

const V3_RIOT: &str = include_str!("../../../fixtures/contracts/v3_riot_recording.json");

/// Capability of the recording limiter. It is a process-local test double.
const RECORDING_CAPABILITY: LimiterCapability =
    LimiterCapability::new(Enforcement::Local, "concord.baseline.recording", 1);

// ---------------------------------------------------------------------------
// Comparable recording model
// ---------------------------------------------------------------------------

/// One recorded endpoint context in the committed V3 contract fixture.
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
        // `replace_count` performs an exact outer-slot replacement.
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
// V3 recording rendering (committed contract fixture)
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

/// Renders request and rate-plan facts with their V3 schema, capability,
/// identity and authentication descriptors.
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
// Current contract assertions
// ---------------------------------------------------------------------------

/// Asserts the current schema contract: reserved built-in labels, explicit
/// custom labels, UTF-8 encoding IDs, canonical window order and framing
/// version. The bound plan summary must agree with the admitted plan.
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
            assert_eq!(
                RATE_KEY_FRAMING_VERSION, 1,
                "the current key framing version is 1"
            );
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

/// Per-endpoint attachments: each ID covers exactly one endpoint in
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

/// Every recorded request and rate-plan fact matches the current V3 oracle.
#[tokio::test]
async fn riot_recorded_facts_match_the_current_contract() {
    let expected = parse_recording(V3_RIOT);
    assert_eq!(expected.len(), 92);
    let captures = record_riot().await;
    let actual = captures
        .iter()
        .map(|capture| capture.record.clone())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

/// Schema labels, kinds, encoding IDs, canonical windows and framing version
/// agree with the current key contract and admitted plan.
#[tokio::test]
async fn riot_policy_schemas_match_the_current_key_contract() {
    let captures = record_riot().await;
    assert_schemas(&captures);
}

/// The current sharing relation spans two origins, scope-shared and
/// application buckets, and independent per-endpoint policy identities.
/// The real limiter must enforce that relation on the actual generated keys.
#[tokio::test(start_paused = true)]
async fn riot_sharing_relation_matches_the_current_contract_and_is_enforced() {
    let committed = parse_recording(V3_RIOT);
    let captures = record_riot().await;
    let expected_records = committed.iter().collect::<Vec<_>>();
    let actual_records = captures
        .iter()
        .map(|capture| &capture.record)
        .collect::<Vec<_>>();
    assert_uniform_windows(&expected_records);
    assert_uniform_windows(&actual_records);
    let expected = sharing(&expected_records);
    let actual = sharing(&actual_records);
    assert_eq!(actual, expected, "sharing relation");
    assert_riot_groups(&expected);
    assert_riot_groups(&actual);
    assert_riot_policy_membership(&expected_records);
    assert_riot_policy_membership(&actual_records);
    assert_dynamic_sharing(&captures, &actual);
}

/// Every Riot endpoint has its navigation-derived identity and one private
/// client-level API-key header, with no challenge or displaced placement.
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

/// The fresh V3 Riot recording equals the committed contract fixture
/// byte for byte (detects any later state change, including in cleanup).
#[tokio::test]
async fn riot_v3_recording_matches_committed_v3_fixture() {
    let captures = record_riot().await;
    assert_eq!(render_recording(&captures), V3_RIOT);
    assert_eq!(parse_recording(V3_RIOT).len(), 92);
}

// ---------------------------------------------------------------------------
// V3 edit battery: dynamic key/dispatch recordings of declaration edits
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

/// RATE-02: merging two equal-number definitions under one ID is a
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
