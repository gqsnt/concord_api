//! Generated-surface parity guard (R3-01, extended for R4-01): every member
//! the v2.1 and v3 generators add, remove or re-sign on a generated type must
//! have a migration disposition in the tool, and the tool's tables must match
//! what the generators actually emit.
//!
//! The migrator describes the generated public surface in its own tables
//! (`DeclIndex::generated_method` for v2.1, `v3_core_names`/`v3_value_names`
//! and the v3-only builder names for v3, `v21_removed_core_names` and the
//! removed builder names for the v2.1 members v3 no longer emits). Those
//! tables drifted from the generator once already: the v3 bound request's
//! `description(&self)` was missing, so an application trait `description`
//! on the request was silently displaced. This test compares, for every
//! generated type of a representative declaration
//! (`fixtures/generated_surface/surface_v2_1.rs.txt`, covering every
//! capability condition under which the member set differs), the members the
//! real generators emit:
//!
//! - `v2_1_members.txt`: frozen, derived from the v2.1 baseline generator
//!   (derivation in its header);
//! - `v3_members.txt`: produced from the migrated fixture by the real v3
//!   generator and kept current by the `concord-macros` test
//!   `generated_surface::generated_public_members_match_the_migrator_snapshot`
//!   (which also checks that the fixture reaches every member emission site);
//!
//! against the tool's per-type inventories. Each delta row is `(role, owner,
//! member, receiver, arity, old_present, new_present) -> disposition`:
//!
//! - added (`old_present=false`): the member must be in the v3-only
//!   inventory (`DeclIndex::v3_only`) with its arity and receiver, whose
//!   callers are checked for shadowing (`AddedShadowCheck`), or be listed in
//!   [`NOT_SHADOWING`] with a concrete `NoResolutionRisk` reason; every
//!   inventory entry must be such a member;
//! - removed (`new_present=false`, R4-01): the member must be in the
//!   removed-member inventory (`DeclIndex::v3_removed`) with its
//!   [`Disposition`] (`RemovedManualDiagnostic`: callers get
//!   `C3-MIGRATE-GENERATED-MEMBER-REMOVED`; `AutomaticRename` or
//!   `DedicatedMigrationRule`: must name a `#[test]` of the migration tests
//!   exercising that transformation, checked here; `NoResolutionRisk`: a
//!   concrete reason), and every inventory entry must be such a member;
//! - re-signed (both present, receiver or arity differ): must be listed in
//!   [`SIGNATURE_CHANGES`];
//! - every member of a generated type outside the role model: the type's
//!   [`UNMODELLED`] reason.
//!
//! A newly generated method, and a method the generator stops emitting, fail
//! this test until the tool accounts for it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::super::callers::{Disposition, SurfaceRow};
use super::super::resolve::{Layout, Role, to_snake_case};
use super::*;

/// One generated member: `(name, receiver, arity)`.
type Member = (String, String, usize);

fn surface_fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/generated_surface")
            .join(name),
    )
    .expect("generated-surface fixture")
}

/// `owner -> members` of a member snapshot (`owner name receiver arity`).
fn snapshot(name: &str) -> BTreeMap<String, Vec<Member>> {
    let mut owners = BTreeMap::<String, Vec<Member>>::new();
    for line in surface_fixture(name).lines() {
        if line.starts_with('#') || line.starts_with("export ") || line.trim().is_empty() {
            continue;
        }
        let fields = line.split(' ').collect::<Vec<_>>();
        assert!(fields.len() >= 4, "malformed snapshot line `{line}`");
        assert!(
            fields.get(4).is_none(),
            "`{line}`: a hidden generated member; decide whether it can shadow"
        );
        owners.entry(fields[0].to_owned()).or_default().push((
            fields[1].to_owned(),
            fields[2].replace('_', " "),
            fields[3].parse().expect("arity"),
        ));
    }
    owners
}

/// The v3 path of a generated type the tool models under its v2.1 `path`:
/// v3 moved the builder into the client module and renamed the unbound
/// intent `<Endpoint>Endpoint`; every other generated type kept its path.
fn v3_path(path: &str, role: Role, client: &str) -> String {
    match role {
        Role::Builder => format!("{}::{path}", to_snake_case(client)),
        Role::Endpoint => format!("{path}Endpoint"),
        _ => path.to_owned(),
    }
}

/// Generated types outside the tool's role model, with why none of their
/// members (all added or all removed with the type) can silently change the
/// method a v2.1 call reached.
const UNMODELLED: &[(&str, &str)] = &[
    (
        "surface::credentials::",
        "v2.1 credential facade and handle types (`SurfaceCredentials`, `*CredentialHandle`): removed in v3, so an application impl for them no longer compiles",
    ),
    (
        "surface::Credentials",
        "the v3 credential facade is a new type (v2.1 had `credentials::SurfaceCredentials`): no v2.1 application impl names it, and every v2.1 call chain through `credentials()` (`.session()`, `.application_key()`) no longer compiles",
    ),
];

/// v3-only members that are deliberately not in the tool's inventory, with
/// the reason they cannot silently displace an application method. Kept
/// empty unless an entry is proved unable to shadow.
const NOT_SHADOWING: &[(&str, &str, &str)] = &[];

/// Members both generators emit on a type with a different receiver or
/// arity, with their disposition. Empty: no such member exists today, and a
/// new one fails the guard until it is classified here.
const SIGNATURE_CHANGES: &[(&str, &str, Disposition)] = &[];

/// A `NoResolutionRisk` reason must say concretely why no call can change
/// meaning (a placeholder is rejected).
const MINIMUM_REASON: usize = 40;

/// The `#[test]` functions of the migration test sources.
fn migration_tests() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/migration");
    let mut files = vec![root.join("tests.rs")];
    for entry in fs::read_dir(root.join("tests")).expect("migration tests") {
        files.push(entry.expect("test file").path());
    }
    let mut names = BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("test source");
        let lines = text.lines().map(str::trim).collect::<Vec<_>>();
        for pair in lines.windows(2) {
            if pair[0] != "#[test]" {
                continue;
            }
            if let Some(name) = pair[1]
                .strip_prefix("fn ")
                .and_then(|rest| rest.split('(').next())
            {
                names.insert(name.to_owned());
            }
        }
    }
    names
}

/// Why a disposition is not acceptable, if it is not.
fn disposition_problem(disposition: &Disposition, tests: &BTreeSet<String>) -> Option<String> {
    match disposition {
        Disposition::AddedShadowCheck | Disposition::RemovedManualDiagnostic => None,
        Disposition::AutomaticRename { to, test } => (!tests.contains(*test)).then(|| {
            format!("the automatic rename to `{to}` names no migration `#[test]` `{test}`")
        }),
        Disposition::DedicatedMigrationRule { rule, test } => (!tests.contains(*test))
            .then(|| format!("the dedicated rule `{rule}` names no migration `#[test]` `{test}`")),
        Disposition::NoResolutionRisk(reason) => (reason.trim().len() < MINIMUM_REASON)
            .then(|| format!("`NoResolutionRisk({reason:?})` needs a concrete reason")),
    }
}

/// The tool's removed-member inventory of one modelled type:
/// `(name, arity, by_ref, disposition)`.
fn removed_inventory(row: &SurfaceRow) -> Vec<(String, usize, bool, Disposition)> {
    row.v3_removed.clone()
}

/// Whether `(receiver, arity)` of a snapshot member matches an inventory
/// entry's `(by_ref, arity)`.
fn same_shape(receiver: &str, arity: usize, by_ref: bool, known_arity: usize) -> bool {
    arity == known_arity && (receiver == "&self") == by_ref
}

#[test]
fn migrator_v3_only_inventory_matches_the_generated_surface() {
    // The fixture the v3 snapshot expands is the tool's own migration of the
    // v2.1 fixture.
    let v2_source = surface_fixture("surface_v2_1.rs.txt");
    let migrated = convert(&v2_source);
    assert_eq!(
        migrated.converted,
        surface_fixture("surface_v3.rs.txt"),
        "surface_v3.rs.txt must be the unedited migrate-v3 output of surface_v2_1.rs.txt (regenerate it, then the concord-macros snapshot)"
    );

    let v2 = snapshot("v2_1_members.txt");
    let v3 = snapshot("v3_members.txt");
    let layout = Layout {
        standalone: true,
        ..Layout::default()
    };
    let index = DeclIndex::build(&layout, &[(PathBuf::from("surface.rs"), v2_source)]);
    let rows = index.surface_inventory();

    let tests = migration_tests();
    let mut failures = Vec::new();
    let mut seen_v2 = BTreeSet::new();
    let mut seen_v3 = BTreeSet::new();
    let mut table = String::new();
    let mut deltas = String::new();
    for row in &rows {
        let v2_owner = row.path.clone();
        let v3_owner = v3_path(&row.path, row.role, &row.client);
        let (Some(old), Some(new)) = (v2.get(&v2_owner), v3.get(&v3_owner)) else {
            failures.push(format!(
                "the tool models `{v2_owner}` ({:?}) but a generator emits no members for it (v2.1 `{v2_owner}`: {}, v3 `{v3_owner}`: {})",
                row.role,
                v2.contains_key(&v2_owner),
                v3.contains_key(&v3_owner)
            ));
            continue;
        };
        seen_v2.insert(v2_owner.clone());
        seen_v3.insert(v3_owner.clone());
        let old_names = old
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<BTreeSet<_>>();
        let new_names = new
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<BTreeSet<_>>();

        // ---- added: v3 generates the name for this type, v2.1 did not.
        let added = new
            .iter()
            .filter(|(name, _, _)| !old_names.contains(name.as_str()))
            .collect::<Vec<_>>();
        for (name, receiver, arity) in &added {
            let _ = writeln!(table, "{:?} {v3_owner} {name} {receiver} {arity}", row.role);
            if let Some((_, _, reason)) = NOT_SHADOWING
                .iter()
                .find(|(owner, member, _)| *owner == v3_owner && member == name)
            {
                let disposition = Disposition::NoResolutionRisk(reason);
                let _ = writeln!(
                    deltas,
                    "{:?} {v3_owner} {name} {receiver} {arity} old_present=false new_present=true -> {disposition:?}",
                    row.role
                );
                if let Some(problem) = disposition_problem(&disposition, &tests) {
                    failures.push(format!("`{v3_owner}::{name}`: {problem}"));
                }
                continue;
            }
            let by_ref = match receiver.as_str() {
                "&self" => true,
                "self" => false,
                other => {
                    failures.push(format!(
                        "`{v3_owner}::{name}` (receiver `{other}`) is new in v3 and the tool's inventory cannot describe it: decide whether it can shadow and list it in NOT_SHADOWING or extend V3Name"
                    ));
                    continue;
                }
            };
            let listed = row.v3_only.iter().any(|(known, known_arity, known_ref)| {
                known == name && known_arity == arity && *known_ref == by_ref
            });
            let _ = writeln!(
                deltas,
                "{:?} {v3_owner} {name} {receiver} {arity} old_present=false new_present=true -> {}",
                row.role,
                if listed {
                    "AddedShadowCheck"
                } else {
                    "MISSING"
                }
            );
            if !listed {
                failures.push(format!(
                    "`{v3_owner}::{name}({receiver}, {arity} argument(s))` ({:?}) is generated by v3 but not v2.1, and is missing from the tool's v3-only inventory (callers.rs `v3_core_names`/`v3_value_names`/builder names): an application trait method `{name}` on this type would be silently displaced; inventory: {:?}",
                    row.role, row.v3_only
                ));
            }
        }
        for (known, known_arity, known_ref) in &row.v3_only {
            let generated = added.iter().any(|(name, receiver, arity)| {
                name == known && same_shape(receiver, *arity, *known_ref, *known_arity)
            });
            if !generated {
                failures.push(format!(
                    "the tool lists `{known}` ({known_arity} argument(s), by_ref {known_ref}) as v3-only on `{v3_owner}` ({:?}), but v3 does not generate it there as a new member",
                    row.role
                ));
            }
        }

        // ---- removed (R4-01): v2.1 generated the name for this type, v3
        // does not.
        let removed = old
            .iter()
            .filter(|(name, _, _)| !new_names.contains(name.as_str()))
            .collect::<Vec<_>>();
        let inventory = removed_inventory(row);
        for (name, receiver, arity) in &removed {
            let found = inventory.iter().find(|(known, known_arity, known_ref, _)| {
                known == name && same_shape(receiver, *arity, *known_ref, *known_arity)
            });
            let Some((_, _, _, disposition)) = found else {
                let _ = writeln!(
                    deltas,
                    "{:?} {v2_owner} {name} {receiver} {arity} old_present=true new_present=false -> MISSING",
                    row.role
                );
                failures.push(format!(
                    "`{v2_owner}::{name}({receiver}, {arity} argument(s))` ({:?}) is generated by v2.1 but not by v3 on `{v3_owner}`, and has no disposition in the tool's removed-member inventory (callers.rs `v21_removed_core_names`/removed builder names): an unchanged v2.1 call may now reach an application trait method `{name}` silently; inventory: {inventory:?}",
                    row.role
                ));
                continue;
            };
            let _ = writeln!(
                deltas,
                "{:?} {v2_owner} {name} {receiver} {arity} old_present=true new_present=false -> {disposition:?}",
                row.role
            );
            if *disposition == Disposition::AddedShadowCheck {
                failures.push(format!(
                    "`{v2_owner}::{name}` is removed in v3; `AddedShadowCheck` does not apply to a removal"
                ));
            }
            if let Some(problem) = disposition_problem(disposition, &tests) {
                failures.push(format!("`{v2_owner}::{name}`: {problem}"));
            }
        }
        for (known, known_arity, known_ref, _) in &inventory {
            let is_removal = removed.iter().any(|(name, receiver, arity)| {
                name == known && same_shape(receiver, *arity, *known_ref, *known_arity)
            });
            if !is_removal {
                failures.push(format!(
                    "the tool lists `{known}` ({known_arity} argument(s), by_ref {known_ref}) as removed from `{v2_owner}` ({:?}), but v2.1 does not generate it there or v3 still does",
                    row.role
                ));
            }
        }

        // ---- re-signed: both generate the name, with another receiver or
        // arity.
        for (name, receiver, arity) in old {
            let Some((_, new_receiver, new_arity)) =
                new.iter().find(|(candidate, _, _)| candidate == name)
            else {
                continue;
            };
            if new_receiver == receiver && new_arity == arity {
                continue;
            }
            let listed = SIGNATURE_CHANGES
                .iter()
                .find(|(owner, member, _)| *owner == v3_owner && member == name);
            let _ = writeln!(
                deltas,
                "{:?} {v3_owner} {name} {receiver}/{arity} -> {new_receiver}/{new_arity} old_present=true new_present=true -> {}",
                row.role,
                listed.map_or_else(|| "MISSING".to_owned(), |(_, _, d)| format!("{d:?}"))
            );
            match listed {
                Some((_, _, disposition)) => {
                    if let Some(problem) = disposition_problem(disposition, &tests) {
                        failures.push(format!("`{v3_owner}::{name}`: {problem}"));
                    }
                }
                None => failures.push(format!(
                    "`{v3_owner}::{name}` changes from ({receiver}, {arity}) in v2.1 to ({new_receiver}, {new_arity}) in v3 and has no disposition in SIGNATURE_CHANGES"
                )),
            }
        }
    }

    // ---- types outside the role model: every member of theirs is a delta
    // whose disposition is the type's UNMODELLED reason.
    for (owners, seen, old_present, version) in
        [(&v2, &seen_v2, true, "v2.1"), (&v3, &seen_v3, false, "v3")]
    {
        for (owner, members) in owners.iter().filter(|(owner, _)| !seen.contains(*owner)) {
            let Some((_, reason)) = UNMODELLED
                .iter()
                .find(|(prefix, _)| owner == prefix || owner.starts_with(prefix))
            else {
                failures.push(format!(
                    "{version} generates `{owner}`, a type outside the tool's role model: model it or justify it in UNMODELLED"
                ));
                continue;
            };
            if let Some(problem) =
                disposition_problem(&Disposition::NoResolutionRisk(reason), &tests)
            {
                failures.push(format!("`{owner}`: {problem}"));
            }
            for (name, receiver, arity) in members {
                let _ = writeln!(
                    deltas,
                    "unmodelled {owner} {name} {receiver} {arity} old_present={old_present} new_present={} -> NoResolutionRisk(UNMODELLED)",
                    !old_present
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "generated-surface parity failures:\n{}\n\ngenerated-member deltas (role owner member receiver arity old_present new_present -> disposition):\n{deltas}",
        failures.join("\n")
    );
    // Print the tables (`--nocapture`) for the evidence record.
    println!("v3 members new relative to v2.1 per type (role owner name receiver arity):\n{table}");
    println!(
        "generated-member deltas (role owner member receiver arity old_present new_present -> disposition):\n{deltas}"
    );
}
