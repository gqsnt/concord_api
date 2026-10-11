//! The exact migrate-v3 output, compiled through the real v3 `concord::api!`
//! and executed (V3-O04).
//!
//! Every module under `tests/migrated_v3/` is byte-for-byte the output of
//! `concord-xtask migrate-v3` for one v2.1 crate made of the captured
//! fixtures in `tools/xtask/fixtures/migrate_v3/` plus unchanged support
//! modules; the unit test
//! `migration::tests::golden_outputs_are_the_exact_tool_output` pins that
//! equality. Compiling this target therefore proves the tool's output is valid
//! for the implemented v3 grammar and generated surface, and the tests below
//! prove it keeps the v2.1 meaning:
//!
//! * nested `Option` setters keep the v2.1 exact outer-slot law (and a naive
//!   rename would not);
//! * direct placement never reorders mixed operations, and inherited keys,
//!   `when_present` and multi-use values keep their wire programs;
//! * Rust constant paths keep their qualification beside same-named scopes;
//! * the migrated Riot and Catalog clients reproduce the captured v2.1
//!   limiter recordings: endpoint IDs, methods, logical origins, routes, query
//!   key order and every rate policy (ID, cost, windows, key labels and
//!   canonical key bytes), so the original sharing relations are unchanged;
//! * the one deliberate timing change the migration cannot express in source,
//!   root `#[default]` evaluation at builder creation, is classified against
//!   the documented v2.1 `build()`-time evaluation (MIGRATE-04).
//!
//! `tests/migrated_v3_identity/receiver_identity.rs` is likewise the exact
//! tool output of `fixtures/migrate_v3/receiver_identity_v2_1.rs.txt`
//! (pinned by `migration::tests::receiver_identity_golden_output_is_the_exact_tool_output`):
//! receivers that are not proved to be generated types keep their original
//! setter call and result (independent review F03).
//!
//! `tests/migrated_v3_identity/default_deny.rs` is the exact tool output of
//! `fixtures/migrate_v3/default_deny_v2_1.rs.txt` (pinned by
//! `migration::tests::default_deny::round3_f03_default_deny_golden_output_is_the_exact_tool_output`):
//! calls under `#[cfg]`-gated declarations, inside macros that do not evaluate
//! their arguments in place, intercepted by another crate's trait or by a
//! macro-generated trait implementation keep their original call and result
//! (second post-fix review of F03). The crate root below stands in for that
//! other crate, `extpatch`.
//!
//! `tests/migrated_v3_identity/round4.rs` is the exact tool output of
//! `fixtures/migrate_v3/round4_v2_1.rs.txt` (pinned by
//! `migration::tests::round4::round4_f03_golden_output_is_the_exact_tool_output`),
//! with its `#[path]` module file `round4_files/local_data.rs`: a `let`
//! annotation that coerces to a trait object, an `async fn` call, fields that
//! are not visible where they are accessed, traits implemented under an import
//! rename or a re-export alias, and a type read through an inline module's
//! `#[path]` keep their original call and result (third post-fix review of
//! F03).
//!
//! `tests/migrated_v3_identity/round5.rs` is the exact tool output of
//! `fixtures/migrate_v3/round5_v2_1.rs.txt` (pinned by
//! `migration::tests::round5::round5_f03_golden_output_is_the_exact_tool_output`),
//! with its module file `round5_files/shared.rs` mounted twice: an item of an
//! inner block (a `const`, `static`, unit struct, block `use` or glob) that
//! shadows an outer local binding or parameter, and a call in a module file
//! whose `super::Req` names a different type in each mount keep their original
//! call and result (fourth post-fix review of F03).
//!
//! No request reaches a network: the recording limiter denies admission after
//! capturing the complete protected plan, before any transport handoff.

#![forbid(unsafe_code)]

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "migrated_v3/riot/mod.rs"]
mod riot;

#[allow(dead_code)]
#[rustfmt::skip]
#[path = "migrated_v3/representative.rs"]
mod representative;

// The review's pattern-macro probe is deliberately a single-arm `match`, and
// the genuine `Box` receiver is deliberately boxed.
#[allow(dead_code, clippy::match_single_binding, clippy::boxed_local)]
#[rustfmt::skip]
#[path = "migrated_v3_identity/receiver_identity.rs"]
mod receiver_identity;

// The second post-fix review's classes. The unrelated `Box` receivers are
// deliberately boxed, the fixture's macros are deliberately local, and its
// `#[cfg(all())]` is deliberately the trivially true predicate the tool
// accepts.
#[allow(dead_code, clippy::boxed_local, clippy::non_minimal_cfg)]
#[rustfmt::skip]
#[path = "migrated_v3_identity/default_deny.rs"]
mod default_deny;

// The third post-fix review's classes. The unrelated `Box` receivers are
// deliberately boxed, and the plain `_` annotation is deliberately the one the
// tool treats as no annotation.
#[allow(dead_code, clippy::boxed_local, clippy::let_with_type_underscore)]
#[rustfmt::skip]
#[path = "migrated_v3_identity/round4.rs"]
mod round4;

// The fourth post-fix review's classes. The inner items deliberately reuse
// the names of outer bindings, and the fixture deliberately mounts one module
// file twice.
#[allow(dead_code, clippy::duplicate_mod)]
#[rustfmt::skip]
#[path = "migrated_v3_identity/round5.rs"]
mod round5;

// The focused review of R3 B01 (D1): an application trait method that v3
// shadows with a generated inherent method of the same name.
#[allow(dead_code)]
#[rustfmt::skip]
#[path = "migrated_v3_identity/name_shadow.rs"]
mod name_shadow;

// The stand-in for a third-party crate named `extpatch`: this test crate under
// an extern-prelude alias, so migrate-v3 (which migrated `default_deny.rs` as
// part of another crate) sees `extpatch::Patch` as another crate's item.
extern crate self as extpatch;

/// Another crate's extension trait: a blanket implementation for every
/// `Box<T>` whose provided methods take the generated setter names.
pub trait Patch {
    /// The original call's result.
    fn set_x(self, _: Option<u32>) -> u32
    where
        Self: Sized,
    {
        1
    }

    /// What a wrong rename would return instead.
    fn replace_x(self, _: Option<u32>) -> u32
    where
        Self: Sized,
    {
        2
    }
}

impl<T> Patch for Box<T> {}

/// Another crate's trait whose methods are all provided by default.
pub trait Defaulted {
    /// The original call's result.
    fn set_x(self, _: Option<u32>) -> u32
    where
        Self: Sized,
    {
        1
    }

    /// What a wrong rename would return instead.
    fn replace_x(self, _: Option<u32>) -> u32
    where
        Self: Sized,
    {
        2
    }
}

/// Another crate's function.
pub fn touch() {}

use std::collections::BTreeMap;
use std::future::{Future, IntoFuture};
use std::path::Path;
use std::sync::{Arc, Mutex};

use concord::endpoint::RequestPlan;
use concord::observation::SanitizedHeaderValue;
use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, NoopRateLimiter, RateLimitError, RateLimitErrorKind,
    RateLimitPermit, RateLimitRequest, RateLimiter,
};
use concord::{ApiError, SecretString};

use riot::api::RiotApi;
use riot::domain::*;

// ----- recording limiter -------------------------------------------------------

/// One captured admission request: the complete protected rate plan.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Admission {
    endpoint: String,
    method: String,
    logical_origin: Option<String>,
    policies: Vec<RecordedPolicy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedPolicy {
    id: String,
    cost: u32,
    /// `(capacity, duration in milliseconds)` in plan order.
    windows: Vec<(u64, u128)>,
    /// `(label, lowercase hex canonical key bytes)` in key order.
    key: Vec<(String, String)>,
}

#[derive(Default)]
struct Recorder {
    admissions: Mutex<Vec<Admission>>,
    /// The comparison label of each admission, in admission order.
    labels: Mutex<Vec<String>>,
}

/// Captures the plan, then denies admission so nothing is dispatched.
struct RecordingLimiter(Arc<Recorder>);

impl RateLimiter for RecordingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.migration.recording", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let policies = request
            .plan
            .policies()
            .iter()
            .map(|policy| RecordedPolicy {
                id: policy.id().as_str().to_owned(),
                cost: policy.cost().get(),
                windows: policy
                    .windows()
                    .iter()
                    .map(|window| {
                        (
                            u64::from(window.capacity().get()),
                            window.duration().as_millis(),
                        )
                    })
                    .collect(),
                key: policy
                    .key()
                    .parts()
                    .iter()
                    .map(|part| (part.name().to_owned(), hex(part.value())))
                    .collect(),
            })
            .collect();
        self.0
            .admissions
            .lock()
            .expect("recorder lock")
            .push(Admission {
                endpoint: request.endpoint.to_owned(),
                method: request.method.as_str().to_owned(),
                logical_origin: request.logical_origin.map(str::to_owned),
                policies,
            });
        Box::pin(async { Err(RateLimitError::new(RateLimitErrorKind::Acquire)) })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ----- the captured v2.1 recording ----------------------------------------------

/// One observation of the v2.1 baseline recording harness.
#[derive(Debug)]
struct Observation {
    endpoint_id: String,
    method: String,
    timeout: String,
    /// `(name, rendered value)` public plan headers in plan order.
    headers: Vec<(String, String)>,
    path_segments: Vec<String>,
    query_keys: Vec<String>,
    admission: Admission,
}

fn recording(name: &str) -> BTreeMap<String, Observation> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/migration")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("captured v2.1 recording");
    let entries: serde_json::Value = serde_json::from_str(&text).expect("recording JSON");
    let mut map = BTreeMap::new();
    for entry in entries.as_array().expect("recording array") {
        let text = |value: &serde_json::Value| value.as_str().expect("string").to_owned();
        let url = entry["observed_url"].as_str().expect("observed URL");
        let (path_segments, query_keys) = url_shape(url);
        let limiter = &entry["limiter"];
        let admission = Admission {
            endpoint: text(&limiter["endpoint"]),
            method: text(&limiter["method"]),
            logical_origin: limiter["logical_origin"].as_str().map(str::to_owned),
            policies: limiter["policies"]
                .as_array()
                .expect("policies")
                .iter()
                .map(|policy| RecordedPolicy {
                    id: text(&policy["id"]),
                    cost: u32::try_from(policy["cost"].as_u64().expect("cost")).expect("cost"),
                    windows: policy["windows"]
                        .as_array()
                        .expect("windows")
                        .iter()
                        .map(|window| {
                            (
                                window[0].as_u64().expect("capacity"),
                                u128::from(window[1].as_u64().expect("duration")),
                            )
                        })
                        .collect(),
                    key: policy["key"]
                        .as_array()
                        .expect("key")
                        .iter()
                        .map(|part| (text(&part[0]), text(&part[1])))
                        .collect(),
                })
                .collect(),
        };
        let previous = map.insert(
            text(&entry["label"]),
            Observation {
                endpoint_id: text(&entry["endpoint_id"]),
                method: text(&entry["method"]),
                timeout: text(&entry["timeout"]),
                headers: entry["headers"]
                    .as_array()
                    .expect("headers")
                    .iter()
                    .map(|pair| (text(&pair[0]), text(&pair[1])))
                    .collect(),
                path_segments,
                query_keys,
                admission,
            },
        );
        assert!(previous.is_none(), "duplicate recording label");
    }
    map
}

/// Percent-decoded path segments and the ordered query keys (values are
/// omitted: v2.1 and v3 redact different public values in observed URLs).
fn url_shape(url: &str) -> (Vec<String>, Vec<String>) {
    let after_scheme = url.split_once("://").expect("absolute URL").1;
    let path_and_query = after_scheme.split_once('/').map_or("", |(_, rest)| rest);
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(percent_decode)
        .collect();
    let keys = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| percent_decode(pair.split_once('=').map_or(pair, |(key, _)| key)))
        .collect();
    (segments, keys)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let digits = std::str::from_utf8(&bytes[index + 1..index + 3]).expect("ASCII escape");
            output.push(u8::from_str_radix(digits, 16).expect("hex escape"));
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).expect("UTF-8 URL component")
}

/// Plans and executes one migrated request with the recording limiter, then
/// compares the result with the v2.1 observation under the same label.
async fn compare<F, T>(
    label: &str,
    recorder: &Arc<Recorder>,
    expected: &BTreeMap<String, Observation>,
    plan: Result<RequestPlan, ApiError>,
    execution: F,
) where
    F: Future<Output = Result<T, ApiError>>,
{
    let observation = expected
        .get(label)
        .unwrap_or_else(|| panic!("the v2.1 recording has no `{label}`"));
    let plan = plan.unwrap_or_else(|error| panic!("{label}: v3 plan failed: {error}"));
    assert_eq!(
        plan.endpoint_id(),
        observation.endpoint_id,
        "{label}: endpoint ID"
    );
    assert_eq!(
        plan.method().as_str(),
        observation.method,
        "{label}: method"
    );
    assert_eq!(
        format!("{:?}", plan.timeout_selection()),
        observation.timeout,
        "{label}: timeout selection"
    );
    assert_eq!(
        rendered_headers(&plan),
        observation.headers,
        "{label}: public headers"
    );
    let (segments, keys) = url_shape(plan.observed_url());
    assert_eq!(segments, observation.path_segments, "{label}: route");
    assert_eq!(keys, observation.query_keys, "{label}: ordered query keys");
    let before = recorder.admissions.lock().expect("recorder lock").len();
    let result = execution.await;
    assert!(
        result.is_err(),
        "{label}: the recording limiter denies admission"
    );
    let admissions = recorder.admissions.lock().expect("recorder lock");
    assert_eq!(
        admissions.len(),
        before + 1,
        "{label}: exactly one admission"
    );
    assert_eq!(
        admissions[before], observation.admission,
        "{label}: complete rate plan (IDs, costs, windows, labels, canonical key bytes)"
    );
    recorder
        .labels
        .lock()
        .expect("recorder lock")
        .push(label.to_owned());
}

/// Renders public plan headers exactly like the v2.1 baseline harness.
fn rendered_headers(plan: &RequestPlan) -> Vec<(String, String)> {
    plan.public_headers()
        .iter()
        .map(|(name, value)| {
            let value = match value {
                SanitizedHeaderValue::Text(text) => text.to_owned(),
                SanitizedHeaderValue::Redacted => "<redacted>".to_owned(),
                SanitizedHeaderValue::NonUtf8 => "<non-utf8>".to_owned(),
            };
            (name.as_str().to_owned(), value)
        })
        .collect()
}

/// A rate bucket (policy ID and canonical key bytes) shared by more than one
/// recorded request, mapped to the labels that share it.
type SharingRelation = BTreeMap<(String, Vec<(String, String)>), Vec<String>>;

fn sharing<'a>(admissions: impl Iterator<Item = (&'a str, &'a Admission)>) -> SharingRelation {
    let mut buckets = SharingRelation::new();
    for (label, admission) in admissions {
        for policy in &admission.policies {
            buckets
                .entry((policy.id.clone(), policy.key.clone()))
                .or_default()
                .push(label.to_owned());
        }
    }
    buckets.retain(|_, labels| {
        labels.sort();
        labels.len() > 1
    });
    buckets
}

/// The migrated client's sharing relation equals the v2.1 one: every bucket
/// is shared by exactly the same requests.
fn assert_sharing_unchanged(
    recorder: &Recorder,
    expected: &BTreeMap<String, Observation>,
) -> usize {
    let admissions = recorder.admissions.lock().expect("recorder lock");
    let labels = recorder.labels.lock().expect("recorder lock");
    assert_eq!(admissions.len(), labels.len());
    let migrated = sharing(labels.iter().map(String::as_str).zip(admissions.iter()));
    let original = sharing(
        expected
            .iter()
            .map(|(label, observation)| (label.as_str(), &observation.admission)),
    );
    assert_eq!(migrated, original, "sharing relation");
    migrated.len()
}

macro_rules! compare {
    ($label:expr, $recorder:ident, $expected:ident, $client:expr, $unbound:expr, $bound:expr) => {{
        let endpoint = $unbound;
        let plan = $client.plan(&endpoint);
        compare($label, &$recorder, &$expected, plan, $bound.into_future()).await;
    }};
}

fn riot_client(recorder: &Arc<Recorder>) -> RiotApi {
    RiotApi::builder()
        .api_key(SecretString::try_new("riot-migration-sentinel").expect("bounded key"))
        .configure_runtime(|runtime| {
            runtime.rate_limiter(Arc::new(RecordingLimiter(Arc::clone(recorder))))
        })
        .build()
        .expect("migrated Riot client")
}

// ----- Riot ------------------------------------------------------------------

/// V3-O04 / V3-R01 (via migration): all 46 migrated Riot endpoints at two
/// origins reproduce the captured v2.1 recording, including canonical key
/// bytes, so every original sharing group (application bucket, method
/// buckets and the scope-shared `champion_mastery_v4`, `challenges_v1` and
/// `tournament_stub_v5` groups) keeps its state partition. Inputs mirror the
/// baseline harness; its v2.1 optional setters are written with v3
/// `replace_x`.
#[tokio::test]
async fn migrated_riot_reproduces_the_v2_1_recording() {
    let expected = recording("v2_1_riot_recording.json");
    let recorder = Arc::new(Recorder::default());
    let client = riot_client(&recorder);
    let mut compared = 0;
    for platform in [PlatformRoute::Na1, PlatformRoute::Euw1] {
        let p = format!("{platform:?}");
        let u = || RiotApi::unbound().platform(platform);
        let b = || client.platform(platform);
        macro_rules! both {
            ($name:literal, $($path:tt)+) => {{
                compare!(&format!("{p}:{}", $name), recorder, expected, client, u().$($path)+, b().$($path)+);
                compared += 1;
            }};
        }
        both!(
            "masteries_by_puuid",
            champion_mastery_v4().masteries_by_puuid("puuid-a".to_owned())
        );
        both!(
            "mastery_by_champion",
            champion_mastery_v4().mastery_by_champion("puuid-a".to_owned(), 7)
        );
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
            ($name:literal, $($path:tt)+) => {{
                compare!(&format!("{p}:{}", $name), recorder, expected, client, u().$($path)+, b().$($path)+);
                compared += 1;
            }};
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
    assert_eq!(compared, 92, "46 endpoints x 2 origins");
    assert_eq!(
        expected.len(),
        92,
        "every recorded observation was compared"
    );

    // Every original sharing group keeps exactly its members: the
    // application bucket per origin, and the scope-shared
    // `champion_mastery_v4`, `challenges_v1`, `account/match/tournament`
    // method and `tournament_stub_v5` buckets.
    assert_eq!(
        assert_sharing_unchanged(&recorder, &expected),
        10,
        "shared buckets"
    );
}

// ----- Catalog ---------------------------------------------------------------

/// V3-O04 / V3-R02 (via migration): the verbatim v2.1 Catalog declaration,
/// migrated, reproduces the captured v2.1 recording for two tenants and two
/// regions; the preserved legacy `"client.tenant"` label keeps the baseline
/// key bytes, distinct from the named policy's `"tenant"` label.
#[tokio::test]
async fn migrated_catalog_reproduces_the_v2_1_recording() {
    use representative::catalog::{CatalogApi, SessionToken, UpdateInput};

    let expected = recording("v2_1_catalog_recording.json");
    let recorder = Arc::new(Recorder::default());
    let mut compared = 0;
    for tenant in ["tenant-a", "tenant-b"] {
        let client = representative::catalog::client(
            tenant,
            Arc::new(RecordingLimiter(Arc::clone(&recorder))),
        )
        .expect("migrated Catalog client");
        let token: SessionToken =
            serde_json::from_str("\"session-sentinel\"").expect("session material");
        client
            .credentials()
            .replace_session(token)
            .expect("install session");
        for region in ["eu", "us"] {
            let label = format!("{tenant}:{region}");
            compare!(
                &format!("{label}:item_summaries"),
                recorder,
                expected,
                client,
                CatalogApi::unbound()
                    .regional(region.to_owned())
                    .item_summaries(vec!["t1".to_owned(), "t2".to_owned()]),
                client
                    .regional(region.to_owned())
                    .item_summaries(vec!["t1".to_owned(), "t2".to_owned()])
            );
            compare!(
                &format!("{label}:item"),
                recorder,
                expected,
                client,
                CatalogApi::unbound()
                    .regional(region.to_owned())
                    .item("item-1".to_owned()),
                client.regional(region.to_owned()).item("item-1".to_owned())
            );
            compare!(
                &format!("{label}:update_item"),
                recorder,
                expected,
                client,
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
            compared += 3;
        }
    }
    assert_eq!(compared, 12);
    assert_eq!(
        expected.len(),
        12,
        "every recorded observation was compared"
    );
    // Per tenant and origin, the named read policy is shared by all three
    // endpoints (4 buckets); the inline write policy includes the endpoint.
    assert_eq!(
        assert_sharing_unchanged(&recorder, &expected),
        4,
        "shared buckets"
    );
    let admissions = recorder.admissions.lock().expect("recorder lock");
    let write = admissions
        .iter()
        .flat_map(|admission| admission.policies.iter())
        .find(|policy| policy.id == "catalog.item.write")
        .expect("write policy");
    assert_eq!(
        write.key[2].0, "client.tenant",
        "legacy inline label preserved"
    );
    let read = admissions
        .iter()
        .flat_map(|admission| admission.policies.iter())
        .find(|policy| policy.id == "catalog.tenant.read")
        .expect("read policy");
    assert_eq!(read.key[1].0, "tenant", "named-policy formal label");
}

// ----- representative declarations ------------------------------------------------

fn noop() -> Arc<dyn RateLimiter> {
    Arc::new(NoopRateLimiter)
}

fn url(plan: &Result<RequestPlan, ApiError>) -> &str {
    plan.as_ref().expect("plan").observed_url()
}

fn header<'a>(
    plan: &'a Result<RequestPlan, ApiError>,
    name: &str,
) -> Vec<SanitizedHeaderValue<'a>> {
    plan.as_ref()
        .expect("plan")
        .public_headers()
        .iter()
        .filter(|(candidate, _)| candidate.as_str() == name)
        .map(|(_, value)| value)
        .collect()
}

/// V3-O04 (nested Option): the migrated `replace_milestone(None)` clears the
/// outer slot exactly like v2.1 `set_milestone(None)`, `Some(None)` selects a
/// present inner `None`, and a naive rename-free reading of the old call
/// (`set_milestone(None)` under v3) would instead send a present inner `None`.
#[test]
fn nested_option_setters_keep_the_v2_1_outer_slot_law() {
    let client = representative::goals("tenant-a", noop()).expect("client");
    let plans = representative::milestone_plans(&client);
    assert_eq!(
        url(&plans[0]),
        "https://goals.example.test/projects/p1/milestones?locale=fr&v=2&limit=10"
    );
    assert_eq!(
        url(&plans[1]),
        "https://goals.example.test/projects/p1/milestones?locale=fr&v=2&milestone=none&limit=10"
    );
    assert_eq!(
        url(&plans[2]),
        "https://goals.example.test/projects/p1/milestones?locale=fr&v=2&milestone=7&limit=3"
    );
    let unmigrated = client
        .project("p1".to_owned())
        .milestones()
        .set_milestone(None)
        .plan();
    assert_eq!(
        url(&unmigrated),
        url(&plans[1]),
        "unmigrated `set_milestone(None)` means a present inner None in v3"
    );
    assert_ne!(url(&unmigrated), url(&plans[0]));
}

/// Review F03 / V3-O04 (receiver identity): every unrelated receiver in the
/// migrated fixture offers both `set_x` (result 1) and `replace_x` (result 2),
/// so a rename decided by a same leaf type name, a same field name, a
/// same-named function or constructor, or an unqualified UFCS type would
/// still compile and return 2. The migration left each of them unchanged
/// (and reported them); the genuine generated calls were renamed and keep
/// the v2.1 exact outer-slot replacement (`None` clears, `Some(n)` sets).
#[test]
fn unrelated_receivers_keep_their_original_setter_results() {
    use receiver_identity::other::ItemRequest;
    assert_eq!(receiver_identity::qualified(ItemRequest), 1);
    assert_eq!(receiver_identity::alias(ItemRequest), 1);
    assert_eq!(
        receiver_identity::field(receiver_identity::Other {
            request: ItemRequest
        }),
        1
    );
    assert_eq!(receiver_identity::ufcs(ItemRequest), 1);
    assert_eq!(receiver_identity::constructor(), 1);
    assert_eq!(receiver_identity::imported_function(), 1);
    assert_eq!(receiver_identity::local_function(), 1);

    let client = receiver_identity::Api::new().expect("client");
    let plans = receiver_identity::genuine_plans(&client);
    let urls = plans.iter().map(url).collect::<Vec<_>>();
    assert_eq!(
        urls,
        [
            "https://review.example.test/item",
            "https://review.example.test/item?x=4",
            "https://review.example.test/item?x=5",
            "https://review.example.test/item",
            "https://review.example.test/item?x=6",
            "https://review.example.test/item?x=7",
        ]
    );
}

/// Post-fix review of F03 (V3-O04): in the v2.1 input each of these calls is
/// unrelated to the generated setter and returns 1, and each unrelated type
/// also offers `replace_*` (returning 2), so a wrong rename would compile. The
/// migration left them unchanged and reported them: an extension trait on
/// `Option`/`Arc`/a reference of the generated request, a chain through a
/// user helper on the generated builder or request, a name rebound by a
/// pattern macro, and a name rebound by a crate-local statement macro called
/// `debug!`. The helper `impl ApiBuilder` was migrated to the v3 support
/// module path `impl api::ApiBuilder` (this target compiles it), and the
/// genuine calls through a `Box`, an explicit `unwrap()` and the builder
/// chain keep the v2.1 exact outer-slot replacement.
#[test]
fn postfix_unrelated_receivers_keep_their_original_results() {
    let client = receiver_identity::Api::new().expect("client");
    let results = [
        (
            "option_ext",
            receiver_identity::option_ext(Some(client.item())),
        ),
        (
            "arc_ext",
            receiver_identity::arc_ext(Arc::new(client.item())),
        ),
        ("ref_ext", receiver_identity::ref_ext(&client.item())),
        ("builder_chain", receiver_identity::builder_chain()),
        (
            "request_chain",
            receiver_identity::request_chain(client.item()),
        ),
        (
            "pattern_macro_let",
            receiver_identity::pattern_macro_let(client.item()),
        ),
        (
            "pattern_macro_arm",
            receiver_identity::pattern_macro_arm(client.item()),
        ),
        (
            "listed_name_macro",
            receiver_identity::listed_name_macro(client.item()),
        ),
    ];
    let changed = results
        .iter()
        .filter(|(_, result)| *result != 1)
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "migrated calls changed behavior: {changed:?}"
    );

    let plans =
        receiver_identity::postfix_genuine_plans(Box::new(client.item()), Some(client.item()));
    let urls = plans.iter().map(url).collect::<Vec<_>>();
    assert_eq!(
        urls,
        [
            "https://review.example.test/item?x=8",
            "https://review.example.test/item?x=9",
            "https://review.example.test/item?locale=fr",
        ]
    );
}

/// Second post-fix review of F03 (V3-O04), the four classes: in the v2.1 input
/// each call below is unrelated to the generated setter and returns 1 (or its
/// original text), and each unrelated target also offers `replace_x`
/// (returning 2), so a wrong rename would compile. The migration left them
/// unchanged: a `let` and an import gated by `#[cfg]` that no test
/// configuration compiles, arguments of crate-local macros that rebind or
/// restructure them, `stringify!` (of a setter call and of the v2.1 builder
/// name), another crate's blanket and default-method traits intercepting a
/// `Box` receiver, crate-local macros generating a trait implementation on
/// `Box<Request>` and a by-value navigation on the client, and a raw
/// identifier on an unrelated type. The genuine calls (an exact request,
/// navigation through `&Api`, a binding under a trivially true `#[cfg]`, a raw
/// identifier and arguments of `vec!`, `assert!` and `format!`) migrated and
/// keep the v2.1 exact outer-slot replacement.
#[test]
fn round3_unrelated_calls_keep_their_original_results() {
    use default_deny::other::ItemRequest;
    let client = default_deny::Api::new().expect("client");
    let results = [
        (
            "cfg_let",
            default_deny::cfg_let(ItemRequest, client.item()).to_string(),
        ),
        (
            "cfg_import",
            default_deny::cfg_import::call(ItemRequest).to_string(),
        ),
        (
            "macro_rebinds_argument",
            default_deny::macro_rebinds_argument(client.item()).to_string(),
        ),
        (
            "macro_restructures_tokens",
            default_deny::macro_restructures_tokens(client.item()).to_string(),
        ),
        (
            "ext_blanket",
            default_deny::ext_blanket::call(Box::new(client.item())).to_string(),
        ),
        (
            "ext_default",
            default_deny::ext_default::call(Box::new(client.item())).to_string(),
        ),
        (
            "macro_impl_box",
            default_deny::macro_impl_box(Box::new(client.item())).to_string(),
        ),
        (
            "macro_impl_navigation",
            default_deny::macro_impl_navigation(client.clone()).to_string(),
        ),
        (
            "raw_foreign",
            default_deny::raw_foreign(ItemRequest).to_string(),
        ),
    ];
    let changed = results
        .iter()
        .filter(|(_, result)| result != "1")
        .map(|(name, result)| format!("{name} -> {result}"))
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "migrated calls changed behavior: {changed:?}"
    );
    assert_eq!(
        default_deny::stringify_argument(client.item()),
        "r.set_x(None)"
    );
    assert_eq!(default_deny::stringify_builder(), "ApiBuilder");

    let urls = default_deny::genuine_plans(&client)
        .iter()
        .map(url)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        urls,
        [
            "https://review.example.test/item?x=11",
            "https://review.example.test/item?x=12",
            "https://review.example.test/item?x=13",
            "https://review.example.test/item?x=14",
        ]
    );
}

/// Third post-fix review of F03 (V3-O04), every compilable class: in the v2.1
/// input each call below is unrelated to the generated setter and returns 1,
/// and each unrelated target also offers `replace_x` (returning 2), so a wrong
/// rename would compile. The migration left them unchanged: a `let`
/// annotation that coerces the value to a trait object (a reference, a `Box`
/// and an alias of `dyn`), the call of an `async fn` (a future, extended by a
/// blanket trait), a private and a `pub(super)` field accessed where they are
/// not visible (auto-deref reaches an accessible field of the same name), a
/// crate-local trait implemented under an import rename and through a
/// re-export alias whose written names belong to unrelated traits, and a type
/// read from the file an inline module's `#[path]` selects (a stray file at
/// the default location declares the generated type). The genuine calls (an
/// exact annotation, a plain `_` annotation, a private field inside its
/// module, a synchronous function result and the generated type read through
/// the `#[path]` module) migrated and keep the v2.1 exact outer-slot
/// replacement.
#[test]
fn round4_unrelated_calls_keep_their_original_results() {
    let client = round4::Api::new().expect("client");
    let results = [
        ("dyn_ref_annotation", round4::dyn_ref_annotation(&client)),
        (
            "dyn_box_annotation",
            round4::dyn_box_annotation(Box::new(client.item())),
        ),
        (
            "alias_of_dyn_annotation",
            round4::alias_of_dyn_annotation(&client),
        ),
        ("async_fn_result", round4::async_fn_result(&client)),
        (
            "private_field",
            round4::private_field(round4::holder::S::new(client.item())),
        ),
        (
            "pub_super_field",
            round4::pub_super_field(round4::outer::fields::V::new(client.item())),
        ),
        (
            "renamed_trait_box",
            round4::renamed_trait_box(Box::new(client.item())),
        ),
        (
            "re-exported trait alias",
            round4::through_reexport::call(Box::new(client.item())),
        ),
        (
            "inline_path_module",
            round4::inline_path_module(round4::other::ItemRequest),
        ),
    ];
    let changed = results
        .iter()
        .filter(|(_, result)| *result != 1)
        .map(|(name, result)| format!("{name} -> {result}"))
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "migrated calls changed behavior: {changed:?}"
    );

    let urls = round4::genuine_plans(&client)
        .iter()
        .map(url)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        urls,
        [
            "https://review.example.test/item?x=21",
            "https://review.example.test/item?x=22",
            "https://review.example.test/item?x=23",
            "https://review.example.test/item?x=24",
            "https://review.example.test/item?x=25",
        ]
    );
}

/// Fourth post-fix review of F03 (round 5): an item declared in a block
/// between a call and the local binding it names (a `const`, `static`, unit
/// struct, block `use` or glob of a const, also in a closure body and over a
/// parameter) is what the call's receiver names, and a module file mounted
/// twice (`round5_files/shared.rs` as `first::shared` and `second::shared`)
/// is compiled with a different `super::Req` in each mount: every unrelated
/// call keeps its original result 1 (the migration left it unchanged and
/// reported it). The genuine calls (a binding no block item shadows, beside an
/// inner item of another name, after a sibling block's item and over its own
/// block's function item) migrated and keep the v2.1 exact outer-slot
/// replacement.
#[test]
fn name_shadow_reported_call_is_the_one_v3_changes() {
    // `tests/migrated_v3_identity/name_shadow.rs` is the exact tool output
    // (pinned by
    // `migration::tests::generated_names::generated_name_shadow_golden_output_is_the_exact_tool_output`)
    // of a v2.1 file in which both calls clear `x` (observed under v2.1:
    // `https://review.example.test/item` for each). The trait-qualified call
    // keeps that meaning unedited. The method call is the one site the tool
    // reports with `C3-MIGRATE-GENERATED-NAME-SHADOW` and leaves unchanged:
    // v3's generated inherent `set_x_if_some` now wins method resolution and
    // keeps `x = 3`, which is why that site needs the manual migration the
    // diagnostic asks for.
    let client = name_shadow::client();
    assert_eq!(
        name_shadow::trait_call(&client),
        "https://review.example.test/item"
    );
    assert_eq!(
        name_shadow::method_call(&client),
        "https://review.example.test/item?x=3"
    );
}

#[test]
fn round5_unrelated_calls_keep_their_original_results() {
    let client = round5::Api::new().expect("client");
    let results = [
        ("inner_const", round5::inner_const(&client)),
        ("inner_static", round5::inner_static(&client)),
        ("inner_unit_struct", round5::inner_unit_struct(&client)),
        ("inner_use_const", round5::inner_use_const(&client)),
        ("inner_glob_const", round5::inner_glob_const(&client)),
        ("closure_body_const", round5::closure_body_const(&client)),
        ("upper_case", round5::upper_case(&client)),
        ("parameter", round5::parameter(client.item())),
        (
            "second_mount",
            round5::second::shared::clear(round5::other::ItemRequest),
        ),
    ];
    let changed = results
        .iter()
        .filter(|(_, result)| *result != 1)
        .map(|(name, result)| format!("{name} -> {result}"))
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "migrated calls changed behavior: {changed:?}"
    );

    let urls = round5::genuine_plans(&client)
        .iter()
        .map(url)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        urls,
        [
            "https://review.example.test/item?x=31",
            "https://review.example.test/item?x=32",
            "https://review.example.test/item?x=33",
            "https://review.example.test/item?x=34",
        ]
    );
}

/// Review F03 follow-up / V3-O04: a `paginate` call on an unrelated type with
/// a local `PageLimit` is not Concord's traversal. The unrelated `Repo` also
/// offers `collect_prefix` (1000 + n) and `collect_all` (2000 + n), so a
/// rewrite decided by the `PageLimit::Prefix(..)` spelling alone would still
/// compile; the migration left both calls unchanged.
#[test]
fn local_page_limit_calls_keep_their_original_results() {
    assert_eq!(receiver_identity::local_page_limit(), (2, 103));
}

/// V3-O04 (direct-placement ordering): the partly converted mixed block keeps
/// the v2.1 statement order b, a, z, c after the inherited root program.
#[test]
fn direct_placement_keeps_the_v2_1_wire_order() {
    let client = representative::goals("tenant-a", noop()).expect("client");
    let plan = representative::ordering_plan(&client);
    assert_eq!(
        url(&plan),
        "https://goals.example.test/projects/p1/mixed?locale=fr&v=2&b=B&a=A&z=1&c=C"
    );
}

/// `when_present` keeps the inherited key in place when absent and replaces
/// it at the new position when present; a v2.1 plain assignment over the
/// inherited `X-Trace` header became `replace` (case-insensitive name).
#[test]
fn inherited_keys_keep_their_v2_1_programs() {
    let client = representative::goals("tenant-a", noop()).expect("client");
    let plans = representative::localized_plans(&client);
    assert_eq!(
        url(&plans[0]),
        "https://goals.example.test/projects/p1/localized?locale=fr&v=2"
    );
    assert!(
        header(&plans[0], "x-trace").is_empty(),
        "absent replacement clears"
    );
    assert_eq!(
        url(&plans[1]),
        "https://goals.example.test/projects/p1/localized?v=2&locale=de"
    );
    assert_eq!(
        header(&plans[1], "x-trace"),
        [SanitizedHeaderValue::Text("endpoint")]
    );
    let milestones = client.project("p1".to_owned()).milestones().plan();
    assert_eq!(
        header(&milestones, "x-trace"),
        [SanitizedHeaderValue::Text("root")],
        "the root direct placement still applies elsewhere"
    );
}

/// LANG-01: one optional value directly placed and reused under another query
/// key and a header sets or omits all three emissions together.
#[test]
fn one_optional_value_keeps_its_three_emissions() {
    let client = representative::goals("tenant-a", noop()).expect("client");
    let plan = representative::shared_plan(&client);
    assert_eq!(
        url(&plan),
        "https://goals.example.test/projects/p1/shared?locale=fr&v=2&lang=en&language=en"
    );
    assert_eq!(
        header(&plan, "accept-language"),
        [SanitizedHeaderValue::Text("en")]
    );
    let absent = client.project("p1".to_owned()).shared().plan();
    assert_eq!(
        url(&absent),
        "https://goals.example.test/projects/p1/shared?locale=fr&v=2"
    );
    assert!(header(&absent, "accept-language").is_empty());
}

/// V3-O04 (constant qualification): constants keep their exact Rust paths
/// beside the same-named `defaults` scope, owner references are resolved
/// (`defaults.tier` from the endpoint, bare `tier` in the scope), and the
/// derived v3 ID equals the v2.1 ID.
#[test]
fn constant_paths_and_owner_references_resolve_exactly() {
    let client = representative::consts().expect("client");
    let plan = representative::consts_plan(&client);
    assert_eq!(
        url(&plan),
        "https://consts.example.test/v1/tiers/2/v1/items/2?zone=z1&region=eu&offset=5&count=25&tier=2"
    );
    assert_eq!(
        plan.as_ref().expect("plan").endpoint_id(),
        "consts.defaults.items"
    );
}

/// Path templates keep the v2.1 segments (escaped literal braces, `concat`,
/// adapters, replace/off fences) under the fixed `/root` base prefix, DNS
/// labels stay a list, and IDs are pinned to the v2.1 derivation across the
/// `Api2URL` naming change.
#[test]
fn path_templates_and_pinned_ids_keep_v2_1_identity() {
    let client = representative::edge().expect("client");
    let plans = representative::edge_plans(&client);
    let expected: [(&str, &str, &[&str]); 6] = [
        (
            "api2url.braces.literal",
            "GET",
            &["root", "braces", "a{b}", "c}}d", "v2-true"],
        ),
        (
            "api2url.braces.adapted",
            "GET",
            &["root", "braces", "reports", "0007"],
        ),
        ("api2url.braces.keyed", "GET", &["root", "braces"]),
        ("api2url.braces.replaced", "GET", &["root", "fixed"]),
        ("api2url.braces.cleared", "GET", &["root"]),
        ("api2url.braces.purge", "PURGE", &["root", "braces"]),
    ];
    for (plan, (id, method, segments)) in plans.iter().zip(expected) {
        let plan = plan.as_ref().expect("plan");
        assert_eq!(plan.endpoint_id(), id);
        assert_eq!(plan.method().as_str(), method);
        assert!(
            plan.observed_url()
                .starts_with("https://acme.api.edge.example.test/"),
            "{}",
            plan.observed_url()
        );
        assert_eq!(url_shape(plan.observed_url()).0, segments, "{id}");
        assert_eq!(
            plan.public_headers()
                .iter()
                .filter(|(name, _)| name.as_str() == "x-token")
                .map(|(_, value)| value)
                .collect::<Vec<_>>(),
            [SanitizedHeaderValue::Redacted],
            "{id}: the sensitive direct placement stays redacted"
        );
    }
}

// ----- root default timing (MIGRATE-04) --------------------------------------------

/// One step of a root-builder call sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuilderStep {
    /// `CatalogApi::builder()`.
    Create,
    /// `set_api_version(ApiVersion::new("v9"))`.
    Set,
    /// `reset_api_version()`.
    Reset,
    /// `build()`.
    Build,
}

/// The documented v2.1 timing of a root `#[default]` value, as a count of
/// `Default::default()` evaluations after each step. In baseline-v2.1
/// `crates/concord-macros/src/emit/client.rs`, `builder()` stores
/// `Override::UseDeclaration`, `set_x` stores `Override::Set`, `reset_x`
/// stores `Override::UseDeclaration` again, and
/// `generate_client_value_resolution` evaluates the declaration default inside
/// `build()` only for `UseDeclaration` ("a builder reset discards the override
/// so the next successful `build` evaluates the declaration default once").
fn v2_1_root_default_timing(steps: &[BuilderStep]) -> Vec<usize> {
    let mut evaluations = 0;
    let mut overridden = false;
    steps
        .iter()
        .map(|step| {
            match step {
                BuilderStep::Create | BuilderStep::Reset => overridden = false,
                BuilderStep::Set => overridden = true,
                BuilderStep::Build if !overridden => evaluations += 1,
                BuilderStep::Build => {}
            }
            evaluations
        })
        .collect()
}

/// This thread's `ApiVersion::default()` evaluations so far.
fn api_version_defaults() -> usize {
    representative::catalog::API_VERSION_DEFAULTS.with(std::cell::Cell::get)
}

/// Runs the steps on the migrated Catalog builder, recording this thread's
/// `ApiVersion::default()` evaluations after each step, and returns the
/// counts plus the version segment the built client routes through (when the
/// steps build one).
fn v3_root_default_timing(steps: &[BuilderStep]) -> (Vec<usize>, Option<String>) {
    use representative::catalog::{ApiVersion, CatalogApi, LoginInput};

    let start = api_version_defaults();
    let mut counts = Vec::new();
    let mut builder = None;
    let mut client = None;
    for step in steps {
        match step {
            BuilderStep::Create => {
                builder = Some(
                    CatalogApi::builder()
                        .set_tenant("tenant-a")
                        .bootstrap_key(
                            SecretString::try_new("catalog-bootstrap-sentinel").expect("bounded"),
                        )
                        .configure_runtime(|runtime| runtime.rate_limiter(noop())),
                );
            }
            BuilderStep::Set => {
                builder = builder.map(|builder| builder.set_api_version(ApiVersion::new("v9")));
            }
            BuilderStep::Reset => builder = builder.map(|builder| builder.reset_api_version()),
            BuilderStep::Build => {
                client = Some(builder.take().expect("builder").build().expect("client"));
            }
        }
        counts.push(api_version_defaults() - start);
    }
    let segment = client.map(|client| {
        let plan = client
            .authenticate(LoginInput {
                user: "user".to_owned(),
            })
            .plan();
        url_shape(url(&plan))
            .0
            .get(1)
            .cloned()
            .expect("version segment")
    });
    (counts, segment)
}

/// MIGRATE-04 (§4.2, §25.3; migrate-v3 `C3-MIGRATE-ROOT-DEFAULT-TIMING`):
/// root-default evaluation timing is an explicit behavior change of the
/// migration, classified here with a counting `Default` on the migrated
/// Catalog's root `#[default] api_version`.
///
/// * v3 (observed through the real generated builder) evaluates the default
///   exactly once, when `builder()` runs, whatever happens afterwards;
///   `reset_api_version()` restores that retained payload without evaluating
///   `Default` again, and `build()` never evaluates it.
/// * v2.1 (documented baseline timing) evaluated it lazily in `build()`, only
///   when the value was not overridden, and evaluated it there again after a
///   reset.
///
/// The wire is unchanged: the built client routes through the default `v2`
/// segment unless the value is overridden.
#[test]
fn migrated_root_default_timing_is_classified_against_v2_1() {
    use BuilderStep::{Build, Create, Reset, Set};

    // (steps, observed v3 evaluations, documented v2.1 evaluations, segment)
    type Case = (
        &'static [BuilderStep],
        &'static [usize],
        &'static [usize],
        Option<&'static str>,
    );
    let cases: [Case; 5] = [
        (&[Create], &[1], &[0], None),
        (&[Create, Build], &[1, 1], &[0, 1], Some("v2")),
        (&[Create, Set, Build], &[1, 1, 1], &[0, 0, 0], Some("v9")),
        (&[Create, Reset, Build], &[1, 1, 1], &[0, 0, 1], Some("v2")),
        (
            &[Create, Set, Reset, Build],
            &[1, 1, 1, 1],
            &[0, 0, 0, 1],
            Some("v2"),
        ),
    ];
    for (steps, v3_expected, v2_1_expected, segment) in cases {
        assert_eq!(
            v2_1_root_default_timing(steps),
            v2_1_expected,
            "{steps:?}: documented v2.1 timing"
        );
        let (observed, routed) = v3_root_default_timing(steps);
        assert_eq!(
            observed, v3_expected,
            "{steps:?}: v3 evaluates the root default once, at builder creation"
        );
        assert_eq!(routed.as_deref(), segment, "{steps:?}: selected payload");
        // The classification: every sequence differs from v2.1, because v3
        // has already evaluated the default when `builder()` returns, never
        // evaluates it in `build()` and never re-evaluates it after a reset.
        assert_ne!(
            v3_expected, v2_1_expected,
            "{steps:?} is a behavior change, not a spelling change"
        );
    }
}
