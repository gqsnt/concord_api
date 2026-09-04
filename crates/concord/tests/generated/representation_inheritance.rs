use concord::body::RequestRepresentation;
use concord::endpoint::{AttachmentFamily, ResponseAcquisition};
use concord::response::{
    Buffered, DiscardDriver, ResponseBodyError, ResponseDriver, ResponseHead,
    ResponseRepresentation,
};
use concord::{Text, api};

pub struct ParentBuffered;

impl ResponseRepresentation<String> for ParentBuffered {
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-parent")
    }

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

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-discard")
    }

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

    let inherited = RepresentationInheritanceApi::unbound()
        .inherited("body".to_owned())
        .description();
    let inherited = inherited.contract().expect("inherited details");
    let inherited_body = inherited.body().expect("inherited body");
    assert_eq!(inherited_body.specification_type(), "Text");
    assert_eq!(inherited_body.representation_origin(), "client");
    assert_eq!(inherited.response().specification_type(), "ParentBuffered");
    assert_eq!(inherited.response().representation_origin(), "client");
    assert_eq!(
        inherited.response().negotiation_media_type(),
        Some("application/x-concord-parent")
    );

    let local = RepresentationInheritanceApi::unbound()
        .overrides()
        .local("body".to_owned())
        .description();
    let local = local.contract().expect("local details");
    assert_eq!(
        local.body().expect("local body").representation_origin(),
        "scope overrides"
    );
    assert_eq!(local.response().representation_origin(), "scope overrides");
    assert_eq!(local.response().specification_type(), "Text");

    let explicit = RepresentationInheritanceApi::unbound()
        .fenced()
        .explicit("body".to_owned())
        .description();
    let explicit = explicit.contract().expect("explicit details");
    assert_eq!(
        explicit
            .body()
            .expect("explicit body")
            .representation_origin(),
        "endpoint fenced::explicit inline body"
    );
    assert_eq!(
        explicit.response().representation_origin(),
        "endpoint fenced::explicit inline result"
    );
    assert!(explicit.effective_attachments().iter().any(|attachment| {
        attachment.family() == AttachmentFamily::RequestRepresentation
            && attachment.origin() == "scope fenced"
    }));
    assert!(explicit.effective_attachments().iter().any(|attachment| {
        attachment.family() == AttachmentFamily::ResponseRepresentation
            && attachment.origin() == "scope fenced"
    }));
}

#[test]
fn parent_representation_diff_reports_caller_media_and_acquisition_changes() {
    use concord::endpoint::{
        SemanticChangeClass, contract_tool_snapshot, diff_contract_tool_snapshots,
    };

    let before = contract_tool_snapshot(BeforeParentRepresentationApi::contract());
    let after = contract_tool_snapshot(AfterParentRepresentationApi::contract())
        .replace(
            "AfterParentRepresentationApi",
            "BeforeParentRepresentationApi",
        )
        .replace(
            "after_parent_representation_api",
            "before_parent_representation_api",
        );
    let changes = diff_contract_tool_snapshots(&before, &after);
    assert!(changes.contains(&SemanticChangeClass::CallerApi));
    assert!(changes.contains(&SemanticChangeClass::Wire));
    assert!(changes.contains(&SemanticChangeClass::Capability));

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
        .response()
        .negotiation_media_type();
    let after_media = first_after
        .contract()
        .expect("after details")
        .response()
        .negotiation_media_type();
    assert_ne!(before_media, after_media);
}
