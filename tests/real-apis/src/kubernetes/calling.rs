use serde_json::json;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Inside a pod: the last twenty lines of the log of every pod of one app, the list read in pages.
pub async fn tails() -> Result<(), BoxError> {
    let kube = in_cluster()?.with_retry(retry());
    let shop = kube.core_v1().pods("shop");
    let mut pods = shop.list().label_selector("app=web").limit(100).pages().items();
    while let Some(pod) = pods.try_next().await? {
        let tail = shop.log(&pod.metadata.name).container("app").tail_lines(20).await?;
        println!("== {}\n{tail}", pod.metadata.name);
    }
    Ok(())
}

/// Five replicas: read the scale, change it, write it back. A `409`: somebody wrote first.
pub async fn scale_to_five(kube: &Kubernetes) -> kubernetes::Result<()> {
    let web = kube.apps_v1().deployments("shop").scale("web");
    loop {
        let mut scale = web.get().await?;
        scale.spec.replicas = 5;
        match web.replace(scale).field_manager("shop-ops").await? {
            Ok(_) => return Ok(()),
            Err(conflict) => println!("{:?}: read it again", conflict.reason),
        }
    }
}

/// Every change to the pods of a namespace, for as long as the program runs.
pub async fn follow(kube: &Kubernetes, mut handle: impl FnMut(Pod)) -> Result<(), BoxError> {
    let pods = kube.core_v1().pods("shop");
    let mut version = pods.list().limit(1).send().await?.metadata.resource_version;
    loop {
        let mut events = pods.watch().resource_version(version.clone()).allow_watch_bookmarks(true).await?;
        while let Some(event) = events.try_next().await? {
            match event {
                WatchEvent::Added(pod) | WatchEvent::Modified(pod) | WatchEvent::Deleted(pod) => {
                    version = pod.metadata.resource_version.clone();
                    handle(pod);
                }
                WatchEvent::Bookmark(head) => version = head.metadata.resource_version,
                WatchEvent::Error(status) if status.code == Some(410) => version = None, // too old
                WatchEvent::Error(status) => return Err(format!("{:?}", status.reason).into()),
            }
        }
        // The server ended the watch: watch again from `version`.
    }
}

/// Mark a pod, first as a dry run, then for real; then delete it, only if it is still that pod.
pub async fn drain(kube: &Kubernetes) -> kubernetes::Result<()> {
    let pods = kube.core_v1().pods("shop");
    let mark = Patch::Strategic(json!({ "metadata": { "labels": { "drain": "yes" } } }));
    pods.patch("web-0", mark.clone()).dry_run(true).await?;      // checked by the server, written nowhere
    let pod = pods.patch("web-0", mark).await?;
    match pods.delete("web-0").preconditions(Preconditions::uid(&pod.metadata.uid)).await? {
        Deleted::Going(pod) => println!("stopping since {:?}", pod.metadata.deletion_timestamp),
        Deleted::Gone(_) => println!("gone"),
    }
    Ok(())
}
