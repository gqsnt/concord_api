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

/// The pods of a namespace, then the changes to them, for as long as the program runs and the
/// server keeps their history. `listed` receives the whole list: first, and again after a
/// `410 Gone`, until the limit below. Such a list is a new snapshot: it replaces what the caller
/// knows, and the changes between the old version and the new one are not delivered. `changed`
/// receives each `Added`, `Modified` and `Deleted` event after a list.
pub async fn follow(
    kube: &Kubernetes,
    mut listed: impl FnMut(Vec<Pod>),
    mut changed: impl FnMut(WatchEvent<Pod>),
) -> Result<(), BoxError> {
    // A choice of this example: list again at once after a `410`, and give up after three in a
    // row with no progress between them (an event or a bookmark). A caller that needs a longer
    // recovery waits on its own side, and calls `follow` again.
    const EXPIRED_IN_A_ROW: u32 = 3;
    let pods = kube.core_v1().pods("shop");
    let mut expired = 0;
    'list: loop {
        if expired == EXPIRED_IN_A_ROW {
            return Err("the resource version expired three times in a row".into());
        }
        // Every page of the list, asking for up to 500 objects a page: a size that you may tune.
        let (mut all, mut version) = (Vec::new(), None);
        let mut pages = pods.list().limit(500).pages();
        loop {
            match pages.try_next().await {
                Ok(Some(page)) => {
                    version = page.metadata.resource_version;
                    all.extend(page.items);
                }
                Ok(None) => break,
                Err(e) if e.is_status(410) => {                    // the token expired
                    expired += 1;
                    continue 'list;
                }
                Err(e) => return Err(e.into()),
            }
        }
        // The watch starts at the version of the list, never without one, nor at an empty one.
        let mut version = version.filter(|v| !v.is_empty()).ok_or("the list has no resourceVersion")?;
        listed(all);
        loop {
            let watch = pods.watch().resource_version(version.clone()).allow_watch_bookmarks(true);
            let mut events = match watch.await {
                Ok(events) => events,
                Err(e) if e.is_status(410) => {                    // the version is too old
                    expired += 1;
                    continue 'list;
                }
                Err(e) => return Err(e.into()),
            };
            while let Some(event) = events.try_next().await? {
                match &event {
                    WatchEvent::Added(pod) | WatchEvent::Modified(pod) | WatchEvent::Deleted(pod) => {
                        version = pod.metadata.resource_version.clone().filter(|v| !v.is_empty())
                            .ok_or("an event has no resourceVersion")?;
                        changed(event);
                    }
                    WatchEvent::Bookmark(mark) if mark.metadata.resource_version.is_empty() => {
                        return Err("a bookmark has no resourceVersion".into());
                    }
                    WatchEvent::Bookmark(mark) => version = mark.metadata.resource_version.clone(),
                    WatchEvent::Error(status) if status.code == Some(410) => {
                        expired += 1;
                        continue 'list;                            // the rest of this stream is dropped
                    }
                    WatchEvent::Error(status) => return Err(format!("{:?}", status.reason).into()),
                }
                expired = 0;                                       // an event or a bookmark: progress
            }
            // The server ended the watch: watch again from the last version seen.
        }
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
