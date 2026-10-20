//! Representation selection by direction, nearest default, `explicit`
//! fences and inline `via` (§9.1), as public contract facts.

use concord::body::RequestRepresentation;
use concord::endpoint::ResponseAcquisition;
use concord::inspection::RepresentationSource;
use concord::response::{
    Buffered, DiscardDriver, ResponseBodyError, ResponseDriver, ResponseHead,
    ResponseRepresentation,
};
use concord::{Text, api};

pub struct ParentBuffered;

impl ResponseRepresentation<String> for ParentBuffered {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-parent");

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<String, ResponseBodyError> {
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}

pub struct ParentDiscard;

impl ResponseRepresentation<String> for ParentDiscard {
    type Driver = DiscardDriver;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-discard");

    fn from_body(
        (): <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<String, ResponseBodyError> {
        Ok("discarded".to_owned())
    }
}

// ANCHOR: representation_inheritance_contract
api! {
    client RepresentationInheritanceApi {
        base "https://inheritance.example.test/";
        timeout 30s;
        request Text;
        response ParentBuffered;

        POST inherited(body input: String) -> String;

        scope overrides() {
            request Text;
            response Text;

            POST local(body input: String) -> String;
        }

        scope fenced() {
            request explicit;
            response explicit;

            POST explicit(body input: String via Text) -> String via Text;
        }
    }
}
// ANCHOR_END: representation_inheritance_contract

api! {
    client BeforeParentRepresentationApi {
        base "https://diff.example.test/";
        timeout 30s;
        response ParentBuffered;

        GET first() -> String;
        GET second() -> String;
    }
}

api! {
    client AfterParentRepresentationApi {
        base "https://diff.example.test/";
        timeout 30s;
        response ParentDiscard;

        GET first() -> String;
        GET second() -> String;
    }
}

#[test]
fn inheritance_override_and_fence_provenance_are_public_contract_facts() {
    fn assert_request_representation<R: RequestRepresentation<String>>() {}
    assert_request_representation::<Text>();

    let contract = RepresentationInheritanceApi::contract();
    let inherited_description = RepresentationInheritanceApi::unbound()
        .inherited("body".to_owned())
        .description();
    let inherited = inherited_description.contract().expect("inherited details");
    let inherited_body = inherited.body().expect("inherited body");
    assert_eq!(inherited_body.representation().type_name(), "Text");
    assert_eq!(
        inherited_body.representation().source(),
        RepresentationSource::Inherited("RepresentationInheritanceApi")
    );
    assert_eq!(
        inherited.result().representation().type_name(),
        "ParentBuffered"
    );
    assert_eq!(
        inherited.result().representation().source(),
        RepresentationSource::Inherited("RepresentationInheritanceApi")
    );
    assert_eq!(
        inherited.result().accept(),
        Some("application/x-concord-parent")
    );
    assert_eq!(
        inherited_description.response_accept(),
        Some("application/x-concord-parent")
    );

    let local = RepresentationInheritanceApi::unbound()
        .overrides()
        .local("body".to_owned())
        .description()
        .contract()
        .expect("local details");
    assert_eq!(
        local.body().expect("local body").representation().source(),
        RepresentationSource::Inherited("RepresentationInheritanceApi::overrides")
    );
    assert_eq!(
        local.result().representation().source(),
        RepresentationSource::Inherited("RepresentationInheritanceApi::overrides")
    );
    assert_eq!(local.result().representation().type_name(), "Text");

    let explicit = RepresentationInheritanceApi::unbound()
        .fenced()
        .explicit("body".to_owned())
        .description()
        .contract()
        .expect("explicit details");
    let explicit_body = explicit.body().expect("explicit body").representation();
    assert_eq!(explicit_body.source(), RepresentationSource::Inline);
    assert_eq!(explicit_body.type_name(), "Text");
    assert_eq!(
        explicit.result().representation().source(),
        RepresentationSource::Inline
    );
    // The scope fence is the provenance of the explicit selection.
    let fenced = contract
        .owner("RepresentationInheritanceApi::fenced")
        .expect("fenced scope")
        .effective()
        .expect("fenced view");
    assert!(fenced.request_default().fenced());
    assert!(fenced.response_default().fenced());
    assert_eq!(
        fenced.request_default().owner(),
        Some("RepresentationInheritanceApi::fenced")
    );
    assert_eq!(fenced.response_default().type_name(), None);
    let overrides = contract
        .owner("RepresentationInheritanceApi::overrides")
        .expect("overrides scope")
        .effective()
        .expect("overrides view");
    assert!(!overrides.response_default().fenced());
    assert_eq!(overrides.response_default().type_name(), Some("Text"));
}

#[test]
fn parent_representation_diff_reports_caller_media_and_acquisition_changes() {
    use concord_inspect::{Axis, diff_documents, export_string, export_value, parse_document};

    let before = export_value(BeforeParentRepresentationApi::contract());
    let after = parse_document(
        &export_string(AfterParentRepresentationApi::contract())
            .replace(
                "AfterParentRepresentationApi",
                "BeforeParentRepresentationApi",
            )
            .replace(
                "after_parent_representation_api",
                "before_parent_representation_api",
            ),
    )
    .expect("renamed after document");
    let report = diff_documents(&before, &after).expect("semantic diff");
    assert!(!report.is_structurally_equal());
    // §18.3 axes: the negotiated media is a wire change, Buffered -> Discard
    // is a capability (driver) change, the inherited default is provenance
    // and the representation type is an opaque Rust dependency. v2 also
    // reported a caller-API change; in v3 the generated signatures (output
    // type `String`, terminals) are identical, so `CallerApi` must NOT be
    // claimed for this edit.
    for axis in [
        Axis::Wire,
        Axis::Capability,
        Axis::Provenance,
        Axis::OpaqueDependency,
    ] {
        assert!(report.has_axis(axis), "{axis:?}: {}", report.to_text());
    }
    assert!(!report.has_axis(Axis::CallerApi), "{}", report.to_text());
    let changed = report
        .changed_endpoints()
        .into_iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        changed,
        [
            "before_parent_representation_api.first",
            "before_parent_representation_api.second"
        ],
        "{}",
        report.to_text()
    );
    assert!(
        report
            .changes_on(Axis::Capability)
            .iter()
            .any(|change| { change.field() == "result.driver" })
    );

    let first_before = BeforeParentRepresentationApi::unbound()
        .first()
        .description();
    let first_after = AfterParentRepresentationApi::unbound()
        .first()
        .description();
    assert_eq!(
        first_before.response_acquisition(),
        ResponseAcquisition::Buffered
    );
    assert_eq!(
        first_after.response_acquisition(),
        ResponseAcquisition::Discard
    );
    let before_media = first_before
        .contract()
        .expect("before details")
        .result()
        .accept();
    let after_media = first_after
        .contract()
        .expect("after details")
        .result()
        .accept();
    assert_ne!(before_media, after_media);
}
