//! Regression tests for the V3-O07 owner-snapshot corrections
//! (`tools/perf/measurements/ATTRIBUTION.md`).
//!
//! Scope values are stored inline in the generated scope state and in the
//! unbound scope, a scope-owned request holds its owner state by value
//! (binding moves a consumed intent's snapshot into it), and planning an
//! unbound intent assembles the joined owner on the stack. These tests pin
//! the §4.5 snapshot law across those paths
//! (clones and earlier intents are unaffected by later controls; direct,
//! bound and planned requests agree at every depth) and the allocation
//! counts the corrections established.

#![forbid(unsafe_code)]

use concord::{Binary, Bytes, api};

api! {
    /// Two nested scopes with optional (initialized) and required values.
    client SnapshotApi {
        base "https://snapshots.example.test/";

        scope groups(
            query group_label?: &'static str = "group-default",
        ) {
            path "groups";

            GET inspect(query item: &'static str) -> Bytes via Binary {
                path "inspect";
            }

            scope members(member: String, query member_label?: &'static str = "member-default") {
                path "members/{member}";

                GET show(id: String) -> Bytes via Binary {
                    path "{id}";
                }
            }
        }

        GET root_level() -> Bytes via Binary {
            path "root";
        }
    }
}

fn client() -> SnapshotApi {
    SnapshotApi::new().expect("snapshot client")
}

fn url(plan: &concord::endpoint::RequestPlan) -> String {
    plan.observed_url().to_owned()
}

#[test]
fn unbound_scope_controls_leave_cloned_snapshots_and_earlier_intents_unchanged() {
    let client = client();
    let source = SnapshotApi::unbound().groups().set_group_label("source");
    let earlier = source.inspect("a");
    let changed = source.clone().set_group_label("changed");
    let cleared = source.clone().omit_group_label();

    let source_url = url(&client.plan(&source.inspect("a")).expect("source plan"));
    assert_eq!(
        source_url,
        "https://snapshots.example.test/groups/inspect?group_label=source&item=a"
    );
    assert_eq!(
        url(&client.plan(&earlier).expect("earlier plan")),
        source_url
    );
    assert!(
        url(&client.plan(&changed.inspect("a")).expect("changed plan"))
            .contains("group_label=changed")
    );
    assert!(
        !url(&client.plan(&cleared.inspect("a")).expect("cleared plan")).contains("group_label=")
    );
}

#[test]
fn direct_bound_and_planned_requests_agree_at_every_depth() {
    let client = client();

    let nested = SnapshotApi::unbound()
        .groups()
        .set_group_label("outer")
        .members("m-1".to_owned())
        .set_member_label("inner")
        .show("id-7".to_owned());
    let planned = url(&client.plan(&nested).expect("unbound nested plan"));
    let bound_request = client.request(nested.clone());
    let bound = url(&bound_request.plan().expect("bound nested plan"));
    let cloned = bound_request.clone();
    drop(bound_request);
    assert_eq!(url(&cloned.plan().expect("cloned bound plan")), bound);
    let direct = url(&client
        .groups()
        .set_group_label("outer")
        .members("m-1".to_owned())
        .set_member_label("inner")
        .show("id-7".to_owned())
        .plan()
        .expect("direct nested plan"));
    assert_eq!(
        planned,
        "https://snapshots.example.test/groups/members/m-1/id-7?group_label=outer&member_label=inner"
    );
    assert_eq!(bound, planned);
    assert_eq!(direct, planned);

    let root = SnapshotApi::unbound().root_level();
    assert_eq!(
        url(&client
            .request(root.clone())
            .plan()
            .expect("bound root plan")),
        url(&client.plan(&root).expect("unbound root plan"))
    );

    // A scope handle control after request creation does not reach the
    // earlier request (it retains its snapshot).
    let scope = client.groups().set_group_label("first");
    let earlier = scope.inspect("x");
    let _later = scope.set_group_label("second");
    assert!(url(&earlier.plan().expect("earlier direct plan")).contains("group_label=first"));
}

#[test]
fn owner_projection_allocations_stay_corrected() {
    let client = client();
    let intent = || SnapshotApi::unbound().groups().inspect("a");

    // Warm lazily initialized contract statics outside the counted regions.
    for _ in 0..4 {
        drop(client.plan(&intent()).expect("warm unbound plan"));
        drop(
            client
                .groups()
                .inspect("a")
                .plan()
                .expect("warm direct plan"),
        );
        drop(client.request(intent()));
    }

    // Binding moves the consumed snapshot into the request's owner
    // projection (§16.3): a depth-1 scope intent or a root-level intent
    // allocates nothing; a nested intent allocates only its ancestor scope's
    // shared projection.
    let prepared = intent();
    let binding = allocation_counter::measure(|| drop(client.request(prepared)));
    assert_eq!(binding.count_total, 0, "scope intent binding");
    let root = SnapshotApi::unbound().root_level();
    let binding = allocation_counter::measure(|| drop(client.request(root)));
    assert_eq!(binding.count_total, 0, "root intent binding");
    let nested = SnapshotApi::unbound()
        .groups()
        .members("m".to_owned())
        .show("id".to_owned());
    let binding = allocation_counter::measure(|| drop(client.request(nested)));
    assert_eq!(binding.count_total, 1, "nested scope intent binding");

    // Selecting an unbound scope allocates only the value storage (one
    // `Arc` for the initialized label), never a separate snapshot `Arc`.
    let selection = allocation_counter::measure(|| drop(SnapshotApi::unbound().groups()));
    assert_eq!(selection.count_total, 1, "unbound scope selection");

    // Planning an unbound depth-1 intent costs exactly what planning the
    // equivalent direct request costs: the joined owner is not allocated.
    let unbound = intent();
    let direct = client.groups().inspect("a");
    let unbound_plan = allocation_counter::measure(|| drop(client.plan(&unbound)));
    let direct_plan = allocation_counter::measure(|| drop(direct.plan()));
    assert_eq!(unbound_plan.count_total, direct_plan.count_total);
}
