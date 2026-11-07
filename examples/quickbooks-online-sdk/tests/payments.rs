//! Payment-specific regressions; every send targets a local TestServer.

mod support;

use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_quickbooks_online::{
    AuthorizationKind, Decimal, EntityId, Error, NewPayment, RefreshToken, UncertainCause,
};
use support::*;

fn payment(amount: &str) -> NewPayment {
    NewPayment::new(
        EntityId::new("58").unwrap(),
        EntityId::new("145").unwrap(),
        Decimal::parse(amount).unwrap(),
    )
    .unwrap()
}

async fn serve(reply: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> TestServer {
    TestServer::start(move |request: &Recorded| {
        if request.path() == TOKEN_PATH {
            json_fixture(200, "token-issued.json")
        } else {
            reply(request)
        }
    })
    .await
}

#[tokio::test]
async fn a_timed_out_payment_keeps_its_generated_id_and_is_not_resent() {
    let server = serve(|request| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path(), format!("/v3/company/{REALM_A}/payment"));
        json_fixture(200, "payment-created.json").delay(Duration::from_secs(2))
    })
    .await;
    let qbo = qbo_with(
        &server,
        connection(&server, REALM_A, "stored-a"),
        Duration::from_millis(300),
    );
    let error = qbo
        .payments()
        .create(payment("25.1300"))
        .send()
        .await
        .unwrap_err();
    let uncertain = error
        .uncertain()
        .expect("a timed-out create may have been applied");
    assert!(matches!(uncertain.cause(), UncertainCause::NoResponse(_)));
    assert_eq!(uncertain.operation(), "QuickBooks.payments.create");
    assert_eq!(uncertain.realm().as_str(), REALM_A);
    let id = uncertain.request_id().unwrap();
    assert_eq!(id.as_str().len(), 32);
    assert!(id.as_str().bytes().all(|b| b.is_ascii_hexdigit()));
    let sent = api_requests(&server);
    assert_eq!(sent.len(), 1, "SDK never resends payment creation");
    assert_eq!(sent[0].query_all("requestid"), [id.as_str()]);
    assert_eq!(token_requests(&server).len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn payment_failures_follow_the_single_send_policy() {
    for status in [400, 401, 503, 200] {
        let server = serve(move |_| {
            if status == 200 {
                Reply::json(200, "{malformed")
            } else {
                json_fixture(status, "fault-validation.json")
            }
        })
        .await;
        let qbo = qbo(&server, connection(&server, REALM_A, "stored-a"));
        let error = qbo
            .payments()
            .create(payment("25.1300"))
            .send()
            .await
            .unwrap_err();
        match status {
            400 => assert_eq!(error.provider().unwrap().status().as_u16(), 400),
            401 => assert_eq!(
                error.authorization().unwrap().kind(),
                AuthorizationKind::AccessTokenRejected
            ),
            503 => assert!(matches!(
                error.uncertain().unwrap().cause(),
                UncertainCause::ServerError(_)
            )),
            200 => assert!(matches!(
                error.uncertain().unwrap().cause(),
                UncertainCause::UnreadableSuccess(_)
            )),
            _ => unreachable!(),
        }
        let sent = api_requests(&server);
        assert_eq!(sent.len(), 1);
        assert_eq!(token_requests(&server).len(), 1);
        let ids = sent[0].query_all("requestid");
        assert_eq!(ids.len(), 1);
        if let Some(uncertain) = error.uncertain() {
            assert_eq!(uncertain.request_id().unwrap().as_str(), ids[0]);
        }
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn payment_amounts_are_exact_json_numbers_and_each_create_gets_a_fresh_id() {
    let server = serve(|_| json_fixture(200, "payment-created.json")).await;
    let qbo = qbo(&server, connection(&server, REALM_A, "stored-a"));
    for _ in 0..2 {
        qbo.payments()
            .create(payment("9007199254740993.0100"))
            .send()
            .await
            .unwrap();
    }
    let sent = api_requests(&server);
    assert_eq!(sent.len(), 2);
    for request in &sent {
        assert_eq!(
            request.body_text(),
            r#"{"CustomerRef":{"value":"58"},"TotalAmt":9007199254740993.0100,"Line":[{"Amount":9007199254740993.0100,"LinkedTxn":[{"TxnId":"145","TxnType":"Invoice"}]}]}"#
        );
    }
    assert_ne!(
        sent[0].query_all("requestid"),
        sent[1].query_all("requestid")
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn nonpositive_payment_amounts_are_refused() {
    for amount in ["0", "0.0000", "-0", "-0.01", "-25.1300"] {
        let result = NewPayment::new(
            EntityId::new("58").unwrap(),
            EntityId::new("145").unwrap(),
            Decimal::parse(amount).unwrap(),
        );
        assert!(
            matches!(result, Err(Error::InvalidInput { .. })),
            "{amount}"
        );
    }
    payment("0.0001");
}

#[tokio::test]
async fn a_replaced_refresh_token_blocks_payments_only_in_its_own_realm_and_recovers() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path() == TOKEN_PATH {
            let refresh = request.form_pairs().into_iter().find(|(key, _)| key == "refresh_token").unwrap().1;
            let realm = match refresh.as_str() {
                "current-a" => REALM_A,
                "current-b" => REALM_B,
                _ => return json_fixture(400, "oauth-invalid-grant.json"),
            };
            return Reply::json(200, format!(r#"{{"access_token":"access-{realm}","refresh_token":"rotated-{realm}","token_type":"bearer","expires_in":3600,"x_refresh_token_expires_in":8640000}}"#));
        }
        let realm = path_realm(request).unwrap();
        assert_eq!(request.path(), format!("/v3/company/{realm}/payment"));
        assert_eq!(request.header("authorization"), Some(format!("Bearer access-{realm}").as_str()));
        json_fixture(200, "payment-created.json")
    }).await;
    let a = connection(&server, REALM_A, "replaced-a");
    let qbo_a = qbo(&server, a.clone());
    let qbo_b = qbo(&server, connection(&server, REALM_B, "current-b"));
    let error = qbo_a
        .payments()
        .create(payment("25.1300"))
        .send()
        .await
        .unwrap_err();
    let auth = error.authorization().unwrap();
    assert_eq!(auth.kind(), AuthorizationKind::GrantRejected);
    assert_eq!(auth.realm().as_str(), REALM_A);
    assert_eq!(auth.oauth_error(), Some("invalid_grant"));
    assert!(auth.needs_reauthorization());
    assert!(a.needs_reauthorization());
    assert!(api_requests(&server).is_empty());
    qbo_b
        .payments()
        .create(payment("25.1300"))
        .send()
        .await
        .unwrap();
    let error = qbo_a
        .payments()
        .create(payment("25.1300"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.authorization().unwrap().realm().as_str(), REALM_A);
    assert_eq!(
        token_requests(&server).len(),
        2,
        "A's rejection is remembered"
    );
    assert_eq!(api_requests(&server).len(), 1, "only B was sent");
    assert_eq!(
        path_realm(&api_requests(&server)[0]).as_deref(),
        Some(REALM_B)
    );
    a.replace_refresh_token(RefreshToken::new("current-a").unwrap());
    qbo_a
        .payments()
        .create(payment("25.1300"))
        .send()
        .await
        .unwrap();
    assert!(!a.needs_reauthorization());
    assert_eq!(
        path_realm(&api_requests(&server)[1]).as_deref(),
        Some(REALM_A)
    );
    assert_eq!(token_requests(&server).len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}
