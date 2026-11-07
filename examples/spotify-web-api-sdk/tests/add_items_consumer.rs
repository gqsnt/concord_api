//! A consumer selects an owned playlist, adds tracks, and reads its new version.
//! Uses only public APIs and an independent local provider fixture.

use std::{sync::Mutex, time::Duration};

use concord_sdk_spotify_web_api::{
    AddItems, AppCredentials, AuthorizationCode, AuthorizeRequest, ClientSecret, OAuth, Playable,
    SessionOptions, State, WebApi, WebApiConfig,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde_json::{Value, json};

const PLAYLIST: &str = "3cEYpjA9oz9GiPac4AsH4n";
const TRACK: &str = "4iV5W9uYEdYUVa79Axb7Rh";
const SCOPES: &str = "playlist-read-private playlist-modify-private";

#[tokio::test]
async fn select_an_owned_playlist_add_tracks_and_read_the_new_snapshot() {
    let added = Mutex::new(false);
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        let mut added = added.lock().unwrap();
        let body = match (request.method.as_str(), request.path()) {
            ("POST", "/api/token") => {
                assert_eq!(request.form_pairs(), [
                    ("grant_type".into(), "authorization_code".into()),
                    ("code".into(), "fixture-code".into()),
                    ("redirect_uri".into(), "https://consumer.example/callback".into()),
                ]);
                json!({"access_token": "consumer-write-access", "token_type": "Bearer",
                    "expires_in": 3600, "refresh_token": "consumer-write-refresh", "scope": SCOPES})
            }
            ("GET", "/v1/me/playlists") => json!({
                "href": "https://api.spotify.com/v1/me/playlists", "limit": 50,
                "offset": 0, "next": null, "previous": null, "total": 1,
                "items": [{"id": PLAYLIST, "name": "My playlist", "owner": {"id": "consumer-user"},
                    "public": false, "snapshot_id": "before", "uri": format!("spotify:playlist:{PLAYLIST}")}]
            }),
            ("GET", "/v1/playlists/3cEYpjA9oz9GiPac4AsH4n") => json!({
                "id": PLAYLIST, "name": "My playlist", "owner": {"id": "consumer-user"},
                "snapshot_id": if *added { "after+/=" } else { "before" },
                "uri": format!("spotify:playlist:{PLAYLIST}"),
                "tracks": {"href": "https://api.spotify.com/v1/playlists/fixture/tracks",
                    "limit": 100, "offset": 0, "next": null, "previous": null,
                    "total": if *added { 1 } else { 0 },
                    "items": if *added { vec![json!({"track": {"type": "track", "id": TRACK,
                        "name": "Added track", "duration_ms": 215000}})] } else { vec![] }}
            }),
            ("POST", "/v1/playlists/3cEYpjA9oz9GiPac4AsH4n/tracks") => {
                assert!(!*added, "the addition must happen once");
                assert_eq!(request.header("authorization"), Some("Bearer consumer-write-access"));
                assert_eq!(request.header("content-type"), Some("application/json"));
                assert_eq!(serde_json::from_slice::<Value>(&request.body).unwrap(),
                    json!({"uris": [format!("spotify:track:{TRACK}")], "position": 0}));
                *added = true;
                return Reply::json(201, json!({"snapshot_id": "after+/="}).to_string());
            }
            _ => panic!("unexpected local request: {} {}", request.method, request.target),
        };
        Reply::json(200, body.to_string())
    }).await;

    let http = concord_sdk_spotify_web_api::concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord_sdk_spotify_web_api::concord::reqwest::redirect::Policy::none())
        .retry(concord_sdk_spotify_web_api::concord::reqwest::retry::never())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let app = AppCredentials::new("consumer", &ClientSecret::new("fixture-secret")).unwrap();
    let oauth = OAuth::new(http.clone(), app)
        .with_base_url(server.url())
        .unwrap();
    let authorize = oauth
        .authorize_url(&AuthorizeRequest {
            redirect_uri: "https://consumer.example/callback".into(),
            scopes: SCOPES.split_whitespace().map(String::from).collect(),
            state: State::new("fixture-state"),
            show_dialog: false,
        })
        .unwrap();
    assert_eq!(
        authorize
            .query_pairs()
            .find(|(key, _)| key == "scope")
            .unwrap()
            .1,
        SCOPES
    );
    let grant = oauth
        .exchange_code(
            &AuthorizationCode::new("fixture-code"),
            "https://consumer.example/callback",
        )
        .await
        .unwrap();
    let session = oauth.session(grant, SessionOptions::new());
    let api = WebApi::new(
        http,
        WebApiConfig {
            session: session.clone(),
        },
    )
    .with_base_url(server.url())
    .unwrap();

    let playlists = api.me().playlists().send().await.unwrap();
    let selected = playlists
        .items
        .into_iter()
        .find(|playlist| playlist.owner.id == "consumer-user")
        .unwrap();
    let playlist = api.playlist(selected.id);
    let before = playlist.get().send().await.unwrap();
    let snapshot = playlist
        .add_items(
            AddItems::new([format!("spotify:track:{TRACK}")])
                .unwrap()
                .position(0),
        )
        .send()
        .await
        .unwrap();
    assert_ne!(snapshot.snapshot_id, before.snapshot_id);
    assert_eq!(snapshot.snapshot_id, "after+/=");
    let after = playlist.get().send().await.unwrap();
    assert_eq!(after.snapshot_id, snapshot.snapshot_id);
    assert_eq!(after.tracks.total, 1);
    let Some(Playable::Track(track)) = &after.tracks.items[0].track else {
        panic!("expected added track")
    };
    assert_eq!(track.id.as_ref().unwrap().as_str(), TRACK);
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "consumer-write-refresh"
    );
    assert_eq!(session.scope().as_deref(), Some(SCOPES));

    let requests = server.requests();
    assert_eq!(requests.len(), 5); // code exchange, listing, get, addition, get
    assert!(
        requests
            .iter()
            .all(|request| request.host == server.addr().to_string())
    );
    assert_eq!(
        requests.iter().filter(|r| r.path() == "/api/token").count(),
        1
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}
