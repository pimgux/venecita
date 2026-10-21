//! The server's checks and routes, through `tower::ServiceExt::oneshot` (no real socket),
//! plus one WebSocket test on a real local socket.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use venecita_api::{router, ActionError, Call, Events, Handler, Query, Settings, MAX_FAILURES};
use venecita_core::Action;

const TOKEN: &str = "test-token-0123456789";
const PORT: u16 = 47800;

/// A fake application: answers queries with `{"query": "<name>"}` and records actions.
fn fake() -> (Handler, Arc<Mutex<Vec<Action>>>) {
    let actions = Arc::new(Mutex::new(Vec::new()));
    let seen = actions.clone();
    let handler: Handler = Arc::new(move |call| match call {
        Call::Query { query, reply } => {
            let _ = reply.send(json!({ "query": format!("{query:?}") }));
        }
        Call::Action { action, reply } => {
            let result = match &action {
                Action::ActivateProfile { name } if name != "Casa" => {
                    Err(ActionError::UnknownProfile(name.clone()))
                }
                _ => Ok(()),
            };
            seen.lock().unwrap().push(action);
            let _ = reply.send(result);
        }
    });
    (handler, actions)
}

fn settings() -> Settings {
    Settings {
        bind: SocketAddr::from(([127, 0, 0, 1], PORT)),
        allow_lan: false,
        token: TOKEN.into(),
    }
}

fn request(method: &str, path: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, format!("127.0.0.1:{PORT}"))
}

fn authed(method: &str, path: &str) -> axum::http::request::Builder {
    request(method, path).header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn health_needs_no_token() {
    let (handler, _) = fake();
    let app = router(&settings(), handler, Events::new());
    let (status, body) = send(
        &app,
        request("GET", "/v1/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn a_missing_or_wrong_token_gets_401() {
    let (handler, actions) = fake();
    let app = router(&settings(), handler, Events::new());
    let (status, body) = send(
        &app,
        request("GET", "/v1/state").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body["error"].is_string(), "errors are JSON");

    let wrong = request("POST", "/v1/actions")
        .header(header::AUTHORIZATION, "Bearer nope")
        .body(Body::from(r#"{"action":"relayout_all"}"#))
        .unwrap();
    assert_eq!(send(&app, wrong).await.0, StatusCode::UNAUTHORIZED);
    assert!(actions.lock().unwrap().is_empty(), "nothing ran");
}

#[tokio::test]
async fn queries_are_answered_by_the_application() {
    let (handler, _) = fake();
    let app = router(&settings(), handler, Events::new());
    for (path, query) in [
        ("/v1/state", Query::State),
        ("/v1/displays", Query::Displays),
        ("/v1/windows", Query::Windows),
        ("/v1/profiles", Query::Profiles),
    ] {
        let (status, body) = send(&app, authed("GET", path).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(body["query"], format!("{query:?}"));
    }
}

#[tokio::test]
async fn a_foreign_host_or_a_browser_origin_gets_403() {
    let (handler, _) = fake();
    let app = router(&settings(), handler, Events::new());
    let rebinding = Request::builder()
        .uri("/v1/state")
        .header(header::HOST, format!("evil.example:{PORT}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, rebinding).await.0, StatusCode::FORBIDDEN);

    let from_page = authed("GET", "/v1/state")
        .header(header::ORIGIN, "https://example.com")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, from_page).await.0, StatusCode::FORBIDDEN);

    // `localhost` with the right port is fine.
    let local = Request::builder()
        .uri("/v1/health")
        .header(header::HOST, format!("localhost:{PORT}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, local).await.0, StatusCode::OK);
}

#[tokio::test]
async fn valid_actions_reach_the_application() {
    let (handler, actions) = fake();
    let app = router(&settings(), handler, Events::new());
    // No Content-Type, like `curl -d`.
    let req = authed("POST", "/v1/actions")
        .body(Body::from(r#"{"action":"move_zone","dir":"left"}"#))
        .unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        actions.lock().unwrap().as_slice(),
        [Action::MoveZone {
            dir: venecita_core::Direction::Left
        }]
    );

    let bad = authed("POST", "/v1/actions")
        .body(Body::from(r#"{"action":"fly"}"#))
        .unwrap();
    let (status, body) = send(&app, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("invalid action"));
}

#[tokio::test]
async fn profiles_are_activated_by_name() {
    let (handler, actions) = fake();
    let app = router(&settings(), handler, Events::new());
    let ok = authed("POST", "/v1/profiles/Casa/activate")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, ok).await.0, StatusCode::OK);
    let missing = authed("POST", "/v1/profiles/Nope/activate")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, missing).await.0, StatusCode::NOT_FOUND);
    assert_eq!(actions.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn an_oversized_body_gets_413() {
    let (handler, actions) = fake();
    let app = router(&settings(), handler, Events::new());
    let big = format!(
        r#"{{"action":"activate_profile","name":"{}"}}"#,
        "x".repeat(20 * 1024)
    );
    let req = authed("POST", "/v1/actions").body(Body::from(big)).unwrap();
    let (status, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"], "request body too large");
    assert!(actions.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_rate_limiter_kicks_in_after_ten_failures() {
    let (handler, _) = fake();
    let app = router(&settings(), handler, Events::new());
    for _ in 0..MAX_FAILURES {
        let wrong = request("GET", "/v1/state")
            .header(header::AUTHORIZATION, "Bearer wrong")
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, wrong).await.0, StatusCode::UNAUTHORIZED);
    }
    // Now wrong tokens are refused for a while...
    let wrong = request("GET", "/v1/state")
        .header(header::AUTHORIZATION, "Bearer wrong")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, wrong).await.0, StatusCode::TOO_MANY_REQUESTS);
    // ...but the right one still works: nobody can lock the CLI out.
    let right = authed("GET", "/v1/state").body(Body::empty()).unwrap();
    assert_eq!(send(&app, right).await.0, StatusCode::OK);
}

#[tokio::test]
async fn unknown_paths_get_a_json_404() {
    let (handler, _) = fake();
    let app = router(&settings(), handler, Events::new());
    let (status, body) = send(&app, authed("GET", "/v1/nope").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn the_openapi_document_is_served() {
    let (handler, _) = fake();
    let app = router(&settings(), handler, Events::new());
    let (status, body) = send(
        &app,
        request("GET", "/v1/openapi.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["openapi"], "3.0.3");
}

#[test]
fn the_websocket_receives_an_event_after_an_action() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let events = Events::new();
    let publisher = events.clone();
    // The fake application publishes an event for every action, like the real one.
    let handler: Handler = Arc::new(move |call| {
        if let Call::Action { action, reply } = call {
            publisher.publish(json!({ "type": "action", "action": format!("{action:?}") }));
            let _ = reply.send(Ok(()));
        }
    });
    let server = venecita_api::start(
        Settings {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            allow_lan: false,
            token: TOKEN.into(),
        },
        handler,
        events,
    )
    .expect("server starts");
    let addr = server.addr();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        // The token as a subprotocol, like a browser-less WebSocket client may send it.
        let mut request = format!("ws://{addr}/v1/events").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", TOKEN.parse().unwrap());
        let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
        assert_eq!(
            response.headers().get("Sec-WebSocket-Protocol").unwrap(),
            TOKEN,
            "the chosen subprotocol is echoed back"
        );

        // Trigger an action over plain HTTP.
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let body = r#"{"action":"relayout_all"}"#;
        let http = format!(
            "POST /v1/actions HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            addr.port(),
            body.len()
        );
        tokio::io::AsyncWriteExt::write_all(&mut stream, http.as_bytes()).await.unwrap();

        let message = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .expect("an event arrives")
            .unwrap()
            .unwrap();
        let event: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
        assert_eq!(event["type"], "action");
        assert_eq!(event["action"], "RelayoutAll");
    });
    drop(server);
}
