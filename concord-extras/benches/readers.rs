//! Identical 1 MiB JSON string, one Script frame, drained completely by each reader.

#[path = "../../concord-core/benches/support.rs"]
mod support;

use concord_core::read::{Incoming, Reader};
use concord_core::transport::{Transport, TryInfo};
use concord_core::{bytes::Bytes, http::Method, reqwest, MIB};
use concord_extras::read::{Binary, Json, Stream, Text};
use concord_testkit::Script;

fn main() {
    let mut c = support::criterion();
    let bytes = Bytes::from(format!("\"{}\"", "a".repeat(MIB - 2)));
    let setup = || {
        let script = Script::new();
        script
            .answer(200)
            .header("content-type", "application/json")
            .body(bytes.clone());
        let request = reqwest::Request::new(
            Method::GET,
            "https://bench.test/body".parse().expect("fixture URL"),
        );
        (script, request)
    };
    macro_rules! reader {
        ($name:literal, $reader:expr, $ty:ty) => {
            support::bench(&mut c, $name, setup, |(script, request)| async move {
                let response = script
                    .send(request, TryInfo::new("body.get", 1))
                    .await
                    .expect("response");
                let value: $ty = $reader.read(Incoming::new(response)).await.expect("read");
                value
            });
        };
    }
    reader!("body1m/json/concord", Json, String);
    reader!("body1m/text/concord", Text, String);
    reader!("body1m/binary/concord", Binary, Bytes);
    support::bench(
        &mut c,
        "body1m/stream/concord",
        setup,
        |(script, request)| async move {
            let response = script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response");
            let mut stream = Stream.read(Incoming::new(response)).await.expect("stream");
            let mut length = 0;
            while let Some(piece) = stream.try_next().await.expect("piece") {
                length += piece.len();
            }
            assert_eq!(length, MIB);
            length
        },
    );
    support::bench(
        &mut c,
        "body1m/json/reqwest",
        setup,
        |(script, request)| async move {
            let response = script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response");
            serde_json::from_slice::<String>(&response.bytes().await.expect("body")).expect("JSON")
        },
    );
    support::bench(
        &mut c,
        "body1m/text/reqwest",
        setup,
        |(script, request)| async move {
            let response = script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response");
            // reqwest has no charset feature here; this matches Text's UTF-8 contract.
            String::from_utf8(response.bytes().await.expect("body").to_vec()).expect("UTF-8")
        },
    );
    support::bench(
        &mut c,
        "body1m/binary/reqwest",
        setup,
        |(script, request)| async move {
            script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response")
                .bytes()
                .await
                .expect("body")
        },
    );
    support::bench(
        &mut c,
        "body1m/stream/reqwest",
        setup,
        |(script, request)| async move {
            use std::pin::pin;
            // Poll the reqwest stream directly, without an extra stream dependency.
            let response = script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response");
            let mut stream = pin!(response.bytes_stream());
            let mut length = 0;
            while let Some(piece) =
                std::future::poll_fn(|cx| futures_core::Stream::poll_next(stream.as_mut(), cx))
                    .await
            {
                length += piece.expect("piece").len();
            }
            assert_eq!(length, MIB);
            length
        },
    );
    let many = || {
        let script = Script::new();
        let mut answer = script.answer(200).declare_length(MIB as u64);
        for index in 0..16 {
            answer = answer.body(bytes.slice(index * (MIB / 16)..(index + 1) * (MIB / 16)));
        }
        drop(answer);
        let request = reqwest::Request::new(
            Method::GET,
            "https://bench.test/body".parse().expect("fixture URL"),
        );
        (script, request)
    };
    support::bench(
        &mut c,
        "body1m/binary16/concord",
        many,
        |(script, request)| async move {
            let response = script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response");
            let value = Binary.read(Incoming::new(response)).await.expect("read");
            assert_eq!(value.len(), MIB);
            value
        },
    );
    support::bench(
        &mut c,
        "body1m/binary16/reqwest",
        many,
        |(script, request)| async move {
            let value = script
                .send(request, TryInfo::new("body.get", 1))
                .await
                .expect("response")
                .bytes()
                .await
                .expect("read");
            assert_eq!(value.len(), MIB);
            value
        },
    );
    c.final_summary();
}
