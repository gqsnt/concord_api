//! `docs/examples/kubernetes.md`: the description, the Rust beside it and the calls, as the page
//! prints them, with stand-ins for the data types that the page only names.
#![allow(missing_docs, dead_code, clippy::print_stdout, clippy::print_stderr)]

include!("stand_ins.rs");
include!("description.rs");

mod beside {
    use super::*;
    include!("beside.rs");
}
pub use beside::*;

mod calling {
    use super::*;
    include!("calling.rs");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use concord_testkit::Script;
    use std::time::Duration;

    const PODS: &str = "https://k8s.test/api/v1/namespaces/shop/pods";

    fn pod(name: &str, version: &str) -> String {
        format!(
            r#"{{"metadata":{{"name":"{name}","uid":"u-{name}","resourceVersion":"{version}"}}}}"#
        )
    }

    /// One page of a list; `version` is left out when it is `None`.
    fn page(script: &Script, version: Option<&str>, next: Option<&str>, pods: &[&str]) {
        let items: Vec<String> = pods.iter().map(|name| pod(name, "1")).collect();
        let mut meta = Vec::new();
        if let Some(version) = version {
            meta.push(format!(r#""resourceVersion":"{version}""#));
        }
        if let Some(token) = next {
            meta.push(format!(r#""continue":"{token}""#));
        }
        script
            .answer(200)
            .header("content-type", "application/json")
            .body(format!(
                r#"{{"metadata":{{{}}},"items":[{}]}}"#,
                meta.join(","),
                items.join(",")
            ));
    }

    fn gone(script: &Script) {
        script
            .answer(410)
            .header("content-type", "application/json")
            .body(r#"{"kind":"Status","code":410,"reason":"Expired"}"#);
    }

    fn line(kind: &str, object: &str) -> String {
        format!("{{\"type\":\"{kind}\",\"object\":{object}}}\n")
    }

    fn watch(version: &str) -> String {
        format!("{PODS}?watch=true&resourceVersion={version}&allowWatchBookmarks=true")
    }

    fn client(script: &Script) -> Kubernetes {
        Kubernetes::new("https://k8s.test".parse::<concord::Base>().unwrap())
            .with_transport(script.clone())
    }

    #[tokio::test(start_paused = true)]
    async fn c11_8_follow_joins_pages_resumes_at_bookmarks_and_lists_again_after_a_410() {
        let script = Script::new();
        // A list of two pages, one snapshot.
        page(&script, Some("10"), Some("t1"), &["a"]);
        page(&script, Some("10"), None, &["b"]);
        // A change, then a bookmark as Kubernetes sends it, then the server ends the watch.
        script
            .answer(200)
            .body(line("ADDED", &pod("c", "12")))
            .body(line(
                "BOOKMARK",
                r#"{"kind":"Pod","apiVersion":"v1","metadata":{"resourceVersion":"15","annotations":{"k8s.io/initial-events-end":"true"}}}"#,
            ));
        // Watched again from the bookmark: an ERROR 410, and a line after it that is dropped.
        script
            .answer(200)
            .body(line("ERROR", r#"{"code":410,"reason":"Expired"}"#))
            .body(line("MODIFIED", &pod("c", "13")));
        // A new snapshot; its continue token expires on the second page; the next list works.
        page(&script, Some("20"), Some("t2"), &["a"]);
        gone(&script);
        page(&script, Some("21"), None, &["a", "c"]);
        script
            .answer(200)
            .body(line("ERROR", r#"{"code":500,"reason":"InternalError"}"#));
        let kube = client(&script);

        let mut lists = Vec::new();
        let mut changes = Vec::new();
        let error = calling::follow(
            &kube,
            |pods| {
                lists.push(
                    pods.into_iter()
                        .map(|pod| pod.metadata.name)
                        .collect::<Vec<_>>(),
                )
            },
            |event| match event {
                WatchEvent::Added(pod) => changes.push(format!("added {}", pod.metadata.name)),
                other => changes.push(format!("{other:?}")),
            },
        )
        .await
        .unwrap_err();

        assert_eq!(error.to_string(), "Some(\"InternalError\")");
        assert_eq!(lists, [vec!["a", "b"], vec!["a", "c"]]);
        assert_eq!(changes, ["added c"]);
        let urls: Vec<String> = script
            .requests()
            .iter()
            .map(|r| r.url().to_owned())
            .collect();
        assert_eq!(
            urls,
            [
                format!("{PODS}?limit=500"),
                format!("{PODS}?limit=500&continue=t1"),
                watch("10"),
                watch("15"),
                format!("{PODS}?limit=500"),
                format!("{PODS}?limit=500&continue=t2"),
                format!("{PODS}?limit=500"),
                watch("21"),
            ]
        );
        assert_eq!(script.unused(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn c11_8_follow_gives_up_after_three_410s_in_a_row() {
        let script = Script::new();
        page(&script, Some("10"), None, &["a"]);
        gone(&script); // the watch: the version is too old
        gone(&script); // the next list
        page(&script, Some("30"), None, &["a"]);
        script
            .answer(200)
            .body(line("ERROR", r#"{"code":410,"reason":"Expired"}"#));
        let kube = client(&script);

        let mut lists = 0;
        let start = tokio::time::Instant::now();
        let error = calling::follow(&kube, |_| lists += 1, |_| {})
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "the resource version expired three times in a row"
        );
        assert_eq!(lists, 2);
        assert_eq!(script.requests().len(), 5);
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(script.unused(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn c11_8_follow_counts_a_bookmark_as_progress_between_410s() {
        let script = Script::new();
        page(&script, Some("10"), None, &["a"]);
        gone(&script); // the watch: 1
        gone(&script); // the list: 2
        page(&script, Some("11"), None, &["a"]);
        script.answer(200).body(line(
            "BOOKMARK",
            r#"{"kind":"Pod","metadata":{"resourceVersion":"20"}}"#,
        )); // progress: the count starts again
        gone(&script); // the watch from the bookmark: 1
        gone(&script); // 2
        gone(&script); // 3
        let kube = client(&script);

        let mut lists = 0;
        let error = calling::follow(&kube, |_| lists += 1, |_| {})
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "the resource version expired three times in a row"
        );
        assert_eq!(lists, 2);
        assert_eq!(script.requests()[5].url(), watch("20"));
        assert_eq!(script.unused(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn c11_8_follow_refuses_to_watch_without_a_version() {
        let script = Script::new();
        page(&script, None, None, &["a"]);
        let kube = client(&script);

        let mut lists = 0;
        let error = calling::follow(&kube, |_| lists += 1, |_| {})
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), "the list has no resourceVersion");
        assert_eq!(lists, 0);
        assert_eq!(script.requests().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn c11_8_follow_refuses_an_empty_version_from_a_list_an_event_or_a_bookmark() {
        let bookmark = line(
            "BOOKMARK",
            r#"{"kind":"Pod","metadata":{"resourceVersion":""}}"#,
        );
        for (case, expected) in [
            ("list", "the list has no resourceVersion"),
            ("event", "an event has no resourceVersion"),
            ("bookmark", "a bookmark has no resourceVersion"),
        ] {
            let script = Script::new();
            if case == "list" {
                page(&script, Some(""), None, &["a"]);
            } else {
                page(&script, Some("10"), None, &["a"]);
                let wrong = if case == "event" {
                    line("ADDED", &pod("c", ""))
                } else {
                    bookmark.clone()
                };
                script.answer(200).body(wrong);
            }
            let kube = client(&script);

            let (mut lists, mut changes) = (0, 0);
            let error = calling::follow(&kube, |_| lists += 1, |_| changes += 1)
                .await
                .unwrap_err();

            assert_eq!(error.to_string(), expected, "{case}");
            assert_eq!(lists, usize::from(case != "list"), "{case}");
            assert_eq!(changes, 0, "{case}");
            assert_eq!(script.unused(), 0, "{case}");
        }
    }
}
