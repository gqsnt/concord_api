//! A caller counts an existing request, then explicitly chooses creation and streaming.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_anthropic_messages::{
    Anthropic, AnthropicConfig, InputMessage, MessageRequest, StreamEvent, http_client,
};
use serde_json::{Value, json};

#[tokio::test]
async fn count_existing_inputs_then_explicitly_create_and_stream() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match (request.path(), request.header("accept")) {
            ("/v1/messages/count_tokens", Some("application/json")) => {
                // Synthetic count; no assumption that later generation usage equals it.
                Reply::json(200, r#"{"input_tokens":37}"#)
                    .header("request-id", "req_consumer_count")
            }
            ("/v1/messages", Some("application/json")) => {
                Reply::json(200, include_str!("../fixtures/message.json"))
            }
            ("/v1/messages", Some("text/event-stream")) => Reply::text(
                200,
                "text/event-stream",
                include_str!("../fixtures/stream-basic.sse"),
            ),
            _ => Reply::json(
                404,
                r#"{"error":{"type":"not_found_error","message":"wrong route"}}"#,
            ),
        }
    })
    .await;
    let api = Anthropic::new(
        http_client().unwrap(),
        AnthropicConfig {
            api_key: "sk-ant-synthetic-count-consumer".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
    .set_anthropic_version("2023-06-01");
    let request = MessageRequest::new("claude-opus-5-5", 128, vec![
        InputMessage::user("Hello"),
        InputMessage::assistant("Hi!"),
        InputMessage::user("Describe a café."),
    ]).unwrap()
    .with_system(json!([{"type":"text","text":"Be concise."}]))
    .with_tools(vec![json!({
        "name":"get_weather",
        "description":"A fixture definition; never executed.",
        "input_schema":{"type":"object","properties":{"location":{"type":"string"}},"required":["location"]}
    })]);
    let counted = api
        .messages()
        .count_tokens(request.count_tokens())
        .send()
        .await
        .unwrap();
    assert_eq!(counted.value.input_tokens, 37);
    assert_eq!(counted.request_id(), Some("req_consumer_count"));
    assert_eq!(server.single().target, "/v1/messages/count_tokens");

    let created = api.messages().create(request.clone()).send().await.unwrap();
    assert_eq!(created.value.content[0].as_text(), Some("Hello!"));
    let mut stream = api
        .messages()
        .stream(request.streaming())
        .send()
        .await
        .unwrap()
        .value;
    let mut stopped = false;
    while let Some(event) = stream.next().await.unwrap() {
        if matches!(event, StreamEvent::MessageStop) {
            stopped = true;
        }
    }
    assert!(stopped);
    assert_eq!(stream.final_message().await.unwrap().text(), "Hello!");

    let recorded = server.requests();
    assert_eq!(recorded.len(), 3); // generation happened only on the caller's two explicit sends
    let expected_inputs = json!({
        "model":"claude-opus-5-5",
        "messages":[
            {"role":"user","content":"Hello"},
            {"role":"assistant","content":"Hi!"},
            {"role":"user","content":"Describe a café."}
        ],
        "system":[{"type":"text","text":"Be concise."}],
        "tools":[{
            "name":"get_weather",
            "description":"A fixture definition; never executed.",
            "input_schema":{"type":"object","properties":{"location":{"type":"string"}},"required":["location"]}
        }]
    });
    for (index, recorded) in recorded.iter().enumerate() {
        assert_eq!(recorded.method, "POST");
        assert_eq!(
            recorded.header_all("x-api-key"),
            ["sk-ant-synthetic-count-consumer"]
        );
        assert_eq!(recorded.header_all("anthropic-version"), ["2023-06-01"]);
        let mut body: Value = serde_json::from_slice(&recorded.body).unwrap();
        if index > 0 {
            assert_eq!(recorded.target, "/v1/messages");
            assert_eq!(
                body.as_object_mut().unwrap().remove("max_tokens"),
                Some(json!(128))
            );
        }
        if index == 2 {
            assert_eq!(
                body.as_object_mut().unwrap().remove("stream"),
                Some(json!(true))
            );
        }
        assert_eq!(body, expected_inputs);
    }
    server.shutdown().await;
}
