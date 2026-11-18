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

    fn pod(name: &str, version: &str) -> String {
        format!(
            r#"{{"metadata":{{"name":"{name}","uid":"u-{name}","resourceVersion":"{version}"}}}}"#
        )
    }

    fn list(script: &Script, version: &str, next: Option<&str>, pods: &[&str]) {
        let items: Vec<String> = pods.iter().map(|name| pod(name, version)).collect();
        let next = next.map_or(String::new(), |token| format!(r#","continue":"{token}""#));
        script
            .answer(200)
            .header("content-type", "application/json")
            .body(format!(
                r#"{{"metadata":{{"resourceVersion":"{version}"{next}}},"items":[{}]}}"#,
                items.join(",")
            ));
    }

    fn gone(script: &Script) {
        script
            .answer(410)
            .header("content-type", "application/json")
            .body(r#"{"code":410,"reason":"Expired"}"#);
    }

    #[tokio::test(start_paused = true)]
    async fn c11_8_follow_lists_again_after_a_410_as_a_status_or_as_an_event() {
        let script = Script::new();
        list(&script, "10", Some("t1"), &["a"]);
        gone(&script); // the continue token of the second page expired
        list(&script, "11", None, &["a", "b"]);
        script
            .answer(200)
            .body(format!(
                "{{\"type\":\"ADDED\",\"object\":{}}}\n",
                pod("c", "12")
            ))
            .body("{\"type\":\"ERROR\",\"object\":{\"code\":410,\"reason\":\"Expired\"}}\n")
            .body(format!(
                "{{\"type\":\"MODIFIED\",\"object\":{}}}\n",
                pod("c", "13")
            ));
        list(&script, "20", None, &["a"]);
        gone(&script); // the watch is refused: the version is too old
        list(&script, "21", None, &[]);
        script.answer(200).body(
            "{\"type\":\"BOOKMARK\",\"object\":{\"metadata\":{\"name\":\"\",\"uid\":\"\",\"resourceVersion\":\"25\"}}}\n",
        );
        script
            .answer(200)
            .body("{\"type\":\"ERROR\",\"object\":{\"code\":500,\"reason\":\"InternalError\"}}\n");
        let kube = Kubernetes::new("https://k8s.test".parse::<concord::Base>().unwrap())
            .with_transport(script.clone());

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
        assert_eq!(lists, [vec!["a", "b"], vec!["a"], vec![]]);
        assert_eq!(changes, ["added c"]);
        let pods = "https://k8s.test/api/v1/namespaces/shop/pods";
        let watch = |version: &str| {
            format!("{pods}?watch=true&resourceVersion={version}&allowWatchBookmarks=true")
        };
        let urls: Vec<String> = script
            .requests()
            .iter()
            .map(|r| r.url().to_owned())
            .collect();
        assert_eq!(
            urls,
            [
                format!("{pods}?limit=500"),
                format!("{pods}?limit=500&continue=t1"),
                format!("{pods}?limit=500"),
                watch("11"),
                format!("{pods}?limit=500"),
                watch("20"),
                format!("{pods}?limit=500"),
                watch("21"),
                watch("25"),
            ]
        );
        assert_eq!(script.unused(), 0);
    }
}
