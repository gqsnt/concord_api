//! The two workflows end to end through the public API, against the deterministic fake registry
//! and its separate token service (`support::Fake`): every request needs a scoped Bearer token.

mod support;

use std::num::NonZeroUsize;

use concord_sdk_distribution_registry::concord::ErrorKind;
use concord_sdk_distribution_registry::concord::bytes::Bytes;
use concord_sdk_distribution_registry::{
    Descriptor, Digest, ErrorCode, ImageManifest, IntegrityProblem, ListOptions, ManifestMediaType,
    PageSize, Reference, RepositoryName,
};
use support::*;

const OCI_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
const OCI_LAYER: &str = "application/vnd.oci.image.layer.v1.tar";

fn pages(n: u32) -> ListOptions {
    ListOptions::default().page_size(PageSize::new(n).unwrap())
}

/// Workflow A: discover repositories and paginated tags, retrieve a manifest with an explicit
/// supported `Accept`, then HEAD and GET its referenced blobs with digest and byte validation.
#[tokio::test]
async fn workflow_a_discover_retrieve_and_verify() -> Result<(), Box<dyn std::error::Error>> {
    let fake = Fake::start().await;
    fake.seed("acme/app", "v1", OCI);
    fake.seed("acme/app", "v2", DOCKER_V2);
    fake.seed("acme/app", "v3", OCI);
    fake.add_repo("acme/tools/cli");
    fake.add_repo("library/base");
    let registry = fake.client();

    assert_eq!(registry.ping().await?.version(), Some("registry/2.0"));

    // The catalog, as this registry shows it to this credential, two names per page.
    let mut catalog = registry.catalog(pages(2));
    let mut names = Vec::new();
    let mut page_count = 0;
    while let Some(page) = catalog.next_page().await? {
        page_count += 1;
        names.extend(page.items);
    }
    assert_eq!(names, ["acme/app", "acme/tools/cli", "library/base"]);
    assert_eq!(page_count, 2);

    let repository = registry.repository(&RepositoryName::new("acme/app")?)?;
    let tags = repository.tags(pages(2)).items().collect_complete().await?;
    assert_eq!(tags, ["v1", "v2", "v3"]);

    // An explicit, supported Accept; the digest is computed over the exact bytes.
    let manifest = repository
        .manifest(&Reference::tag("v1")?, &[ManifestMediaType::OciV1])
        .await?;
    assert_eq!(manifest.media_type, ManifestMediaType::OciV1);
    assert_eq!(manifest.digest, digest(MANIFEST_OCI));

    let mut verified = Vec::new();
    for descriptor in manifest.image.blobs() {
        let info = repository
            .blob_info(&descriptor.digest)
            .await?
            .expect("the blob exists");
        assert_eq!(info.size, Some(descriptor.size));
        assert_eq!(info.digest.as_ref(), Some(&descriptor.digest));
        let bytes = repository
            .blob_for(descriptor)
            .await?
            .read_to_end(descriptor.size)
            .await?;
        assert_eq!(Digest::sha256_of(&bytes), descriptor.digest);
        verified.push(bytes);
    }
    assert_eq!(
        verified,
        [Bytes::from_static(CONFIG), Bytes::from_static(LAYER)]
    );

    // The same manifest by digest (immutable), and the Docker schema 2 one under its own Accept.
    let again = repository
        .manifest(
            &Reference::digest(manifest.digest.clone()),
            &[ManifestMediaType::OciV1],
        )
        .await?;
    assert_eq!(again.bytes, manifest.bytes);
    let docker = repository
        .manifest(&Reference::tag("v2")?, &[ManifestMediaType::DockerV2])
        .await?;
    assert_eq!(docker.image.layers[0].digest, digest(LAYER));
    // Asked only for OCI, the Docker-typed tag is not served by this fake (MANIFEST_UNKNOWN).
    let refused = repository
        .manifest(&Reference::tag("v2")?, &[ManifestMediaType::OciV1])
        .await
        .unwrap_err();
    assert!(refused.has_code(&ErrorCode::ManifestUnknown));

    // One token per access: `/v2/` (no scope), the catalog, the repository's pull scope.
    let scopes: Vec<Vec<String>> = fake.with(|s| {
        s.token_requests
            .iter()
            .map(|(_, scopes, _)| scopes.clone())
            .collect()
    });
    assert_eq!(
        scopes,
        [
            vec![],
            vec!["registry:catalog:*".to_owned()],
            vec!["repository:acme/app:pull".to_owned()]
        ]
    );
    assert!(fake.with(|s| {
        s.token_requests
            .iter()
            .all(|(service, _, _)| service.as_deref() == Some(SERVICE))
    }));

    // On the wire: the catalog link was followed as sent; blobs by literal digest.
    let targets: Vec<String> = fake
        .registry
        .requests()
        .into_iter()
        .map(|r| r.target)
        .collect();
    assert!(targets.contains(&"/v2/_catalog?last=acme%2Ftools%2Fcli&n=2".to_owned()));
    assert!(targets.contains(&format!("/v2/acme/app/blobs/{}", digest(LAYER))));
    Ok(())
}

/// Workflow A, perturbed: a registry that serves corrupted blob bytes. The read fails on the
/// digest, nothing unverified is returned, and the error shows no token.
#[tokio::test]
async fn workflow_a_a_corrupted_blob_fails_verification() -> Result<(), Box<dyn std::error::Error>>
{
    let fake = Fake::start().await;
    fake.seed("acme/app", "v1", OCI);
    fake.with(|s| s.perturb.corrupt_blobs = true);
    let repository = fake
        .client()
        .repository(&RepositoryName::new("acme/app")?)?;
    let manifest = repository
        .manifest(&Reference::tag("v1")?, &[ManifestMediaType::OciV1])
        .await?;
    let layer = &manifest.image.layers[0];
    let error = repository
        .blob_for(layer)
        .await?
        .read_to_end(layer.size)
        .await
        .unwrap_err();
    let Some(IntegrityProblem::DigestMismatch { expected, actual }) = error.integrity_problem()
    else {
        panic!("{error:?}");
    };
    assert_eq!(expected, &layer.digest);
    assert_ne!(actual, &layer.digest);
    let tokens: Vec<String> = fake.with(|s| s.tokens.keys().cloned().collect());
    let secrets: Vec<&str> = tokens.iter().map(String::as_str).collect();
    assert_hides(&error, &secrets);
    Ok(())
}

/// Workflow B: initiate an upload, PATCH ordered inclusive ranges to a `Location` (path and
/// opaque state) that changes after every chunk, lose the answer to the second chunk (stored by
/// the registry), query the status, resume from the registry's range, complete with the digest
/// and publish a manifest that refers to the blob — then read it back.
#[tokio::test]
async fn workflow_b_chunked_upload_interrupted_resumed_and_published()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Fake::start().await;
    fake.with(|s| s.perturb.hang_up_patch = Some((2, true)));
    let repository = fake
        .client()
        .repository(&RepositoryName::new("acme/app")?)?;

    let layer: Bytes = (0..29u8).map(|i| b'a' + i % 26).collect::<Vec<u8>>().into();
    let layer_digest = Digest::sha256_of(&layer);
    let mut upload = repository.start_upload().await?;
    let mut targets = vec![upload.location().clone()];
    let mut interruptions = 0;
    while (upload.offset() as usize) < layer.len() {
        let start = upload.offset() as usize;
        let end = layer.len().min(start + 8);
        match upload.patch(layer.slice(start..end)).await {
            Ok(progress) => assert_eq!(progress.offset as usize, end),
            Err(error) => {
                assert_eq!(error.request_kind(), Some(ErrorKind::Transport));
                assert!(upload.needs_status());
                interruptions += 1;
                // The registry kept the chunk: resume from its range, not from what was sent.
                assert_eq!(upload.status().await?.offset, 16);
            }
        }
        targets.push(upload.location().clone());
    }
    assert_eq!(interruptions, 1);
    let pushed = upload.complete(&layer_digest, None).await?;
    assert_eq!(
        (pushed.digest.clone(), pushed.size),
        (layer_digest.clone(), 29)
    );

    // The configuration blob through the convenience push (two chunks).
    let config = Bytes::from_static(CONFIG);
    let config_pushed = repository
        .push_blob(config.clone(), NonZeroUsize::new(64).unwrap())
        .await?;
    assert_eq!(config_pushed.digest, digest(CONFIG));

    let image = ImageManifest::new(
        ManifestMediaType::OciV1,
        Descriptor::of(OCI_CONFIG, &config),
        vec![Descriptor::new(
            OCI_LAYER,
            layer.len() as u64,
            layer_digest.clone(),
        )],
    );
    let manifest_upload = image.to_upload()?;
    let published = repository
        .put_manifest(&Reference::tag("v1")?, &manifest_upload)
        .await?;
    assert_eq!(&published.digest, manifest_upload.digest());

    // Read back: the tag, the manifest bytes, the verified layer.
    let tags = repository
        .tags(ListOptions::default())
        .items()
        .collect_complete()
        .await?;
    assert_eq!(tags, ["v1"]);
    let fetched = repository
        .manifest(&Reference::tag("v1")?, &[ManifestMediaType::OciV1])
        .await?;
    assert_eq!(fetched.digest, published.digest);
    assert_eq!(&fetched.bytes, manifest_upload.bytes());
    let read = repository
        .blob_for(&fetched.image.layers[0])
        .await?
        .read_to_end(1024)
        .await?;
    assert_eq!(read, layer);

    // On the wire, for the first upload: each PATCH, the status query and the completion went to
    // the latest Location exactly (opaque state included); the resumed chunk started at 16.
    let requests = fake.registry.requests();
    let session: Vec<_> = requests
        .iter()
        .filter(|r| r.target.contains("/blobs/uploads/p1-") && r.header("authorization").is_some())
        .collect();
    let sent: Vec<(&str, String, Option<&str>)> = session
        .iter()
        .map(|r| {
            (
                r.method.as_str(),
                r.target.clone(),
                r.header("content-range"),
            )
        })
        .collect();
    let at = |i: usize| {
        targets[i].as_str()[targets[i].origin().ascii_serialization().len()..].to_owned()
    };
    assert_eq!(
        sent,
        [
            ("PATCH", at(0), Some("0-7")),
            ("PATCH", at(1), Some("8-15")),
            ("GET", at(1), None),
            ("PATCH", at(2), Some("16-23")),
            ("PATCH", at(3), Some("24-28")),
            (
                "PUT",
                format!("{}&digest=sha256%3A{}", at(4), layer_digest.hex()),
                None
            ),
        ]
    );
    let distinct: std::collections::HashSet<_> = targets.iter().collect();
    assert_eq!(
        distinct.len(),
        targets.len(),
        "the Location changed after every chunk"
    );
    assert!(
        targets
            .iter()
            .all(|t| t.query().is_some_and(|q| q.starts_with("_state=s1.")))
    );
    assert!(
        requests.iter().all(|r| !r.target.contains("uuid-")),
        "never built from the UUID"
    );

    // A push token for the upload, then a pull token for the read-back: distinct scopes.
    let scopes: Vec<Vec<String>> = fake.with(|s| {
        s.token_requests
            .iter()
            .map(|(_, scopes, _)| scopes.clone())
            .collect()
    });
    assert_eq!(
        scopes,
        [
            vec!["repository:acme/app:pull,push".to_owned()],
            vec!["repository:acme/app:pull".to_owned()]
        ]
    );
    Ok(())
}

/// Workflow B, perturbed differently: the lost chunk was **not** stored, so the registry's range
/// sends the caller back to re-send it.
#[tokio::test]
async fn workflow_b_a_lost_chunk_is_resent_from_the_reported_range()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Fake::start().await;
    fake.with(|s| s.perturb.hang_up_patch = Some((2, false)));
    let repository = fake
        .client()
        .repository(&RepositoryName::new("acme/app")?)?;
    let blob = Bytes::from_static(b"0123456789abcdefghij");
    let mut upload = repository.start_upload().await?;
    upload.patch(blob.slice(0..8)).await?;
    assert!(upload.patch(blob.slice(8..16)).await.is_err());
    assert_eq!(upload.status().await?.offset, 8);
    upload.patch(blob.slice(8..16)).await?;
    upload.patch(blob.slice(16..)).await?;
    upload.complete(&Digest::sha256_of(&blob), None).await?;
    let ranges: Vec<String> = fake
        .registry
        .requests()
        .into_iter()
        .filter(|r| r.method == "PATCH")
        .filter_map(|r| r.header("content-range").map(str::to_owned))
        .collect();
    assert_eq!(ranges, ["0-7", "8-15", "8-15", "16-19"]);
    assert!(
        repository
            .blob_info(&Digest::sha256_of(&blob))
            .await?
            .is_some()
    );
    Ok(())
}

/// Workflow B's failures: a wrong digest does not publish the blob, and a manifest that refers
/// to a blob the repository lacks is refused with the missing digest named.
#[tokio::test]
async fn workflow_b_wrong_digest_and_missing_blob_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Fake::start().await;
    let repository = fake
        .client()
        .repository(&RepositoryName::new("acme/app")?)?;
    let blob = Bytes::from_static(b"payload bytes");
    let mut upload = repository.start_upload().await?;
    upload.patch(blob.clone()).await?;
    let claimed = Digest::sha256_of(b"something else");
    let error = upload.complete(&claimed, None).await.unwrap_err();
    assert!(error.has_code(&ErrorCode::DigestInvalid));
    assert!(repository.blob_info(&claimed).await?.is_none());
    assert!(
        repository
            .blob_info(&Digest::sha256_of(&blob))
            .await?
            .is_none()
    );

    let image = ImageManifest::new(
        ManifestMediaType::OciV1,
        Descriptor::of(OCI_CONFIG, b"{}"),
        vec![Descriptor::of(OCI_LAYER, &blob)],
    );
    let error = repository
        .put_manifest(&Reference::tag("v1")?, &image.to_upload()?)
        .await
        .unwrap_err();
    let missing: Vec<Digest> = error
        .api()
        .unwrap()
        .errors()
        .iter()
        .filter_map(|entry| entry.detail_digest())
        .collect();
    assert_eq!(
        missing,
        [Digest::sha256_of(b"{}"), Digest::sha256_of(&blob)]
    );
    Ok(())
}

/// A pull token precedes an interrupted push; the resumed upload then fails digest validation.
#[tokio::test]
async fn scoped_interrupted_upload_uses_rotated_state_and_reports_digest_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Fake::start().await;
    fake.with(|s| s.perturb.hang_up_patch = Some((2, true)));
    let repository = fake
        .client()
        .repository(&RepositoryName::new("acme/app")?)?;
    let blob = Bytes::from_static(b"0123456789abcdefghijklmn");
    let actual = Digest::sha256_of(&blob);
    let claimed = Digest::sha256_of(b"different bytes");
    assert!(repository.blob_info(&actual).await?.is_none());

    let mut upload = repository.start_upload().await?;
    upload.patch(blob.slice(0..8)).await?;
    let interrupted = upload.patch(blob.slice(8..16)).await.unwrap_err();
    assert_eq!(interrupted.request_kind(), Some(ErrorKind::Transport));
    assert!(upload.needs_status());
    assert_eq!(upload.status().await?.offset, 16);
    upload.patch(blob.slice(16..)).await?;
    let error = upload.complete(&claimed, None).await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(400));
    assert!(error.has_code(&ErrorCode::DigestInvalid));
    assert!(!upload.is_finished());
    assert!(upload.needs_status());
    assert!(repository.blob_info(&actual).await?.is_none());
    assert!(repository.blob_info(&claimed).await?.is_none());

    let (scopes, tokens) = fake.with(|s| {
        (
            s.token_requests
                .iter()
                .map(|(_, scopes, _)| scopes.clone())
                .collect::<Vec<_>>(),
            s.tokens
                .iter()
                .map(|(token, scopes)| (token.clone(), scopes.clone()))
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(
        scopes,
        [
            vec!["repository:acme/app:pull"],
            vec!["repository:acme/app:pull,push"]
        ]
    );
    let push_token = &tokens.iter().find(|(_, s)| s == &scopes[1]).unwrap().0;
    let pull_token = &tokens.iter().find(|(_, s)| s == &scopes[0]).unwrap().0;
    let secrets: Vec<&str> = tokens
        .iter()
        .map(|(t, _)| t.as_str())
        .chain(["_state"])
        .collect();
    assert_hides(&interrupted, &secrets);
    assert_hides(&error, &secrets);
    let requests = fake.registry.requests();
    let session: Vec<_> = requests
        .iter()
        .filter(|r| r.target.contains("/blobs/uploads/p1-"))
        .collect();
    let wire: Vec<_> = session
        .iter()
        .map(|r| {
            (
                r.method.as_str(),
                r.target.as_str(),
                r.header("content-range"),
            )
        })
        .collect();
    let final_target = format!(
        "/v2/acme/app/blobs/uploads/p1-g3?_state=s1.3%2Bopaque%3D%3D&digest=sha256%3A{}",
        claimed.hex()
    );
    assert_eq!(
        wire,
        [
            (
                "PATCH",
                "/v2/acme/app/blobs/uploads/p1-g0?_state=s1.0%2Bopaque%3D%3D",
                Some("0-7")
            ),
            (
                "PATCH",
                "/v2/acme/app/blobs/uploads/p1-g1?_state=s1.1%2Bopaque%3D%3D",
                Some("8-15")
            ),
            (
                "GET",
                "/v2/acme/app/blobs/uploads/p1-g1?_state=s1.1%2Bopaque%3D%3D",
                None
            ),
            (
                "PATCH",
                "/v2/acme/app/blobs/uploads/p1-g2?_state=s1.2%2Bopaque%3D%3D",
                Some("16-23")
            ),
            ("PUT", final_target.as_str(), None),
        ]
    );
    assert!(
        session
            .iter()
            .all(|r| r.header("authorization") == Some(format!("Bearer {push_token}").as_str()))
    );
    assert!(
        requests
            .iter()
            .filter(|r| r.method == "HEAD" && r.header("authorization").is_some())
            .all(|r| r.header("authorization") == Some(format!("Bearer {pull_token}").as_str()))
    );
    Ok(())
}
