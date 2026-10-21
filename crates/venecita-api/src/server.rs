//! The HTTP/WebSocket server (axum on tokio, in a thread of its own).
//!
//! Every request passes [`guard`] first:
//!
//! 1. Requests with an `Origin` header (sent by browsers) are refused: there is no CORS, so a
//!    web page can't drive Venecita.
//! 2. Unless LAN access is on, the `Host` must be `127.0.0.1`, `localhost` or `[::1]` with the
//!    server's port. This defeats DNS rebinding.
//! 3. An address with 10 failed logins within a minute gets 429 for the next minute.
//! 4. Everything except `/v1/health` and `/v1/openapi.json` needs the token, as
//!    `Authorization: Bearer <token>` or (for WebSockets) in `Sec-WebSocket-Protocol`,
//!    compared in constant time.
//!
//! Bodies are limited to 16 KiB and every error is a small JSON object. Tokens are never
//! logged.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use tokio::sync::{broadcast, oneshot, watch};
use venecita_core::Action;

/// Largest accepted request body.
pub const BODY_LIMIT: usize = 16 * 1024;
/// Failed logins from one address, within [`FAILURE_WINDOW`], before it is blocked.
pub const MAX_FAILURES: u32 = 10;
/// How long failed logins are counted.
pub const FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// How long a blocked address gets 429.
pub const BLOCK_FOR: Duration = Duration::from_secs(60);
/// How long the server waits for the application to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);

/// Read-only questions the server asks the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    /// Active profile and layout, pause and focus-mode flags.
    State,
    /// Connected displays and the definitions they resolved to.
    Displays,
    /// Managed windows with their app, zone and stack.
    Windows,
    /// Profiles and their layouts.
    Profiles,
}

/// Why an action could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// `activate_profile` named a profile that doesn't exist (404).
    UnknownProfile(String),
    /// Anything else, with a message for the client (400).
    Failed(String),
}

/// A request for the application. Answer through `reply`; dropping it answers 503.
#[derive(Debug)]
pub enum Call {
    /// Answer `query` with a JSON document.
    Query {
        /// The question.
        query: Query,
        /// Where the answer goes.
        reply: oneshot::Sender<Value>,
    },
    /// Run `action`.
    Action {
        /// The action.
        action: Action,
        /// Where the outcome goes.
        reply: oneshot::Sender<Result<(), ActionError>>,
    },
}

/// Receives the server's calls, on a server thread. The application typically forwards
/// them to its UI thread and answers from there.
pub type Handler = Arc<dyn Fn(Call) + Send + Sync>;

/// Events for WebSocket clients (`/v1/events`): the application publishes, every connected
/// client receives. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Events(broadcast::Sender<Value>);

impl Default for Events {
    fn default() -> Self {
        Self(broadcast::channel(64).0)
    }
}

impl Events {
    /// A new, empty event stream.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends an event to every connected client (dropped if nobody listens).
    pub fn publish(&self, event: Value) {
        let _ = self.0.send(event);
    }
}

/// How the server listens.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Address and port.
    pub bind: SocketAddr,
    /// Accept other hosts than loopback (`api.allow_lan`).
    pub allow_lan: bool,
    /// The access token.
    pub token: String,
}

/// A running server. Dropping it stops it.
#[derive(Debug)]
pub struct Server {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    closing: watch::Sender<bool>,
}

impl Server {
    /// The address it actually listens on (useful with port 0).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Stop accepting, and close the open WebSockets so the thread can end. It is not
        // joined: the UI thread must never wait for a slow client.
        let _ = self.closing.send(true);
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Starts the server in a thread of its own. Binding errors (port in use…) are returned
/// right away.
pub fn start(settings: Settings, handler: Handler, events: Events) -> std::io::Result<Server> {
    if !settings.allow_lan && !settings.bind.ip().is_loopback() {
        return Err(std::io::Error::other(
            "listening on a non-loopback address needs api.allow_lan = true",
        ));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("venecita-api")
        .enable_all()
        .build()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind(settings.bind))?;
    let addr = listener.local_addr()?;
    let (closing, closing_rx) = watch::channel(false);
    let app = router_with(
        &Settings {
            bind: addr,
            ..settings
        },
        handler,
        events,
        closing_rx,
    );
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    std::thread::Builder::new()
        .name("venecita-api".into())
        .spawn(move || {
            runtime.block_on(async move {
                let service = app.into_make_service_with_connect_info::<SocketAddr>();
                let serve = axum::serve(listener, service).with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });
                if let Err(e) = serve.await {
                    tracing::error!(error = %e, "API server stopped");
                }
            });
        })?;
    tracing::info!(%addr, "API listening");
    Ok(Server {
        addr,
        shutdown: Some(shutdown),
        closing,
    })
}

#[derive(Clone)]
struct AppState {
    token: Arc<str>,
    port: u16,
    allow_lan: bool,
    handler: Handler,
    events: Events,
    failures: Arc<Mutex<HashMap<IpAddr, Failures>>>,
    closing: watch::Receiver<bool>,
}

#[derive(Debug, Clone, Copy)]
struct Failures {
    since: Instant,
    count: u32,
    blocked_until: Option<Instant>,
}

/// The routes, with all the checks. Exposed for tests (`tower::ServiceExt::oneshot`).
pub fn router(settings: &Settings, handler: Handler, events: Events) -> Router {
    router_with(settings, handler, events, watch::channel(false).1)
}

fn router_with(
    settings: &Settings,
    handler: Handler,
    events: Events,
    closing: watch::Receiver<bool>,
) -> Router {
    let state = AppState {
        token: settings.token.as_str().into(),
        port: settings.bind.port(),
        allow_lan: settings.allow_lan,
        handler,
        events,
        failures: Arc::default(),
        closing,
    };
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/openapi.json", get(openapi))
        .route("/v1/state", get(|s| query(s, Query::State)))
        .route("/v1/displays", get(|s| query(s, Query::Displays)))
        .route("/v1/windows", get(|s| query(s, Query::Windows)))
        .route("/v1/profiles", get(|s| query(s, Query::Profiles)))
        .route("/v1/actions", post(action))
        .route("/v1/profiles/{name}/activate", post(activate))
        .route("/v1/events", get(events_socket))
        .fallback(|| async { error(StatusCode::NOT_FOUND, "no such endpoint") })
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(state)
}

/// A JSON error: `{"error": "…"}`.
fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

async fn guard(State(s): State<AppState>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if headers.contains_key(header::ORIGIN) {
        return error(
            StatusCode::FORBIDDEN,
            "requests from web pages are not allowed",
        );
    }
    if !s.allow_lan && !host_is_local(headers, s.port) {
        return error(StatusCode::FORBIDDEN, "unexpected Host header");
    }
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |c| c.0.ip());
    if s.blocked(ip) {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many failed attempts; try again in a minute",
        );
    }
    let public = matches!(request.uri().path(), "/v1/health" | "/v1/openapi.json");
    if !public && !authorized(headers, &s.token) {
        s.record_failure(ip);
        return error(StatusCode::UNAUTHORIZED, "missing or invalid token");
    }
    next.run(request).await
}

fn host_is_local(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|name| host.eq_ignore_ascii_case(&format!("{name}:{port}")))
}

fn same_token(candidate: &str, token: &str) -> bool {
    !token.is_empty() && bool::from(candidate.as_bytes().ct_eq(token.as_bytes()))
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|t| same_token(t.trim(), token));
    bearer || offered_token_protocol(headers, token).is_some()
}

/// The token, if a WebSocket client offered it as one of its subprotocols.
fn offered_token_protocol(headers: &HeaderMap, token: &str) -> Option<String> {
    headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|h| h.to_str().ok())?
        .split(',')
        .map(str::trim)
        .find(|p| same_token(p, token))
        .map(String::from)
}

impl AppState {
    fn blocked(&self, ip: IpAddr) -> bool {
        let mut failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        match failures.get(&ip).and_then(|f| f.blocked_until) {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                failures.remove(&ip);
                false
            }
            None => false,
        }
    }

    fn record_failure(&self, ip: IpAddr) {
        let now = Instant::now();
        let mut failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let entry = failures.entry(ip).or_insert(Failures {
            since: now,
            count: 0,
            blocked_until: None,
        });
        if now.duration_since(entry.since) > FAILURE_WINDOW {
            *entry = Failures {
                since: now,
                count: 0,
                blocked_until: None,
            };
        }
        entry.count += 1;
        if entry.count >= MAX_FAILURES {
            entry.blocked_until = Some(now + BLOCK_FOR);
            tracing::warn!(%ip, "API: too many failed logins, blocking for a minute");
        }
    }

    async fn ask<T>(&self, call: impl FnOnce(oneshot::Sender<T>) -> Call) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        (self.handler)(call(tx));
        tokio::time::timeout(ANSWER_TIMEOUT, rx).await.ok()?.ok()
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn openapi() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        include_str!("openapi.json"),
    )
        .into_response()
}

async fn query(State(s): State<AppState>, query: Query) -> Response {
    match s.ask(|reply| Call::Query { query, reply }).await {
        Some(value) => Json(value).into_response(),
        None => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Venecita Dock did not answer",
        ),
    }
}

/// The body is read as JSON whatever its `Content-Type`, so `curl -d '{…}'` works as is.
async fn action(State(s): State<AppState>, body: Result<Bytes, BytesRejection>) -> Response {
    let bytes = match body {
        Ok(bytes) => bytes,
        Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return error(StatusCode::PAYLOAD_TOO_LARGE, "request body too large");
        }
        Err(rejection) => return error(rejection.status(), "cannot read the request body"),
    };
    match serde_json::from_slice::<Action>(&bytes) {
        Ok(action) => run(&s, action).await,
        Err(e) => error(StatusCode::BAD_REQUEST, &format!("invalid action: {e}")),
    }
}

async fn activate(State(s): State<AppState>, Path(name): Path<String>) -> Response {
    run(&s, Action::ActivateProfile { name }).await
}

async fn run(s: &AppState, action: Action) -> Response {
    match s.ask(|reply| Call::Action { action, reply }).await {
        Some(Ok(())) => Json(json!({ "status": "ok" })).into_response(),
        Some(Err(ActionError::UnknownProfile(name))) => {
            error(StatusCode::NOT_FOUND, &format!("no profile named `{name}`"))
        }
        Some(Err(ActionError::Failed(message))) => error(StatusCode::BAD_REQUEST, &message),
        None => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Venecita Dock did not answer",
        ),
    }
}

async fn events_socket(
    State(s): State<AppState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    // A client that sent the token as a subprotocol must get it back, as the WebSocket
    // handshake requires.
    let upgrade = match offered_token_protocol(&headers, &s.token) {
        Some(protocol) => upgrade.protocols([protocol]),
        None => upgrade,
    };
    let events = s.events.0.subscribe();
    let closing = s.closing.clone();
    upgrade.on_upgrade(move |socket| forward(socket, events, closing))
}

/// Sends every published event to one client until it leaves or the server stops.
async fn forward(
    mut socket: WebSocket,
    mut events: broadcast::Receiver<Value>,
    mut closing: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => {
                    if socket.send(Message::Text(event.to_string().into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            message = socket.recv() => match message {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            _ = closing.changed() => break,
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}
