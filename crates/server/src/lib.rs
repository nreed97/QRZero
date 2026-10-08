//! QRZero's local HTTP API and embedded web UI.
//!
//! The desktop app starts this server on 127.0.0.1 and points its window at
//! it; it can also run on its own (`qrzero-server`) and be used from a
//! browser. Every /api request must carry the session token.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use qrzero_core::adif::Fields;
use qrzero_core::awards::{Award, AwardHint, AwardQso, AwardTable, Counts, CtyFacts, Tally};
use qrzero_core::model::*;
use qrzero_core::qrz::{QrzClient, DEFAULT_ENDPOINT};
use qrzero_core::store::Sort;
use qrzero_core::{secrets, Error, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

mod backups;
mod cluster;
mod propagation;
mod qsl;
mod startup;
mod station;
mod udp_out;
mod watch;

pub use qsl::Endpoints as QslEndpoints;
use cluster::{Cluster, ClusterConfig};
use qsl::{Qsl, QslConfig, SecretsUpdate};
use station::{cty_facts, Active, Hub, Integrations};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const LOOKUP_CACHE_SECS: i64 = 30 * 24 * 3600;

#[derive(rust_embed::Embed)]
#[folder = "../../ui/dist"]
struct UiAssets;

pub struct Config {
    pub data_dir: PathBuf,
    pub addr: SocketAddr,
    /// Session token; a random one is generated when None.
    pub token: Option<String>,
    /// QRZ XML endpoint (overridable for tests).
    pub qrz_endpoint: String,
    /// Credential store namespace for saved passwords.
    pub secret_service: String,
    /// Download the country file when it's missing or old.
    pub update_cty: bool,
    /// QRZ Logbook and Club Log endpoints (overridable for tests).
    /// QSL service URLs (tests point these at local stand-ins).
    pub qsl_endpoints: QslEndpoints,
    /// N0NBH's solar data feed (tests point this at a stand-in).
    pub propagation_url: String,
    /// Start the programs listed under Settings, Startup programs.
    pub launch_apps: bool,
}

impl Config {
    pub fn local(data_dir: PathBuf) -> Self {
        Config {
            data_dir,
            addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            token: None,
            qrz_endpoint: DEFAULT_ENDPOINT.to_string(),
            secret_service: "QRZero".to_string(),
            update_cty: true,
            qsl_endpoints: QslEndpoints::default(),
            propagation_url: propagation::DEFAULT_URL.to_string(),
            launch_apps: true,
        }
    }
}

pub struct Running {
    pub addr: SocketAddr,
    pub token: String,
    pub handle: tokio::task::JoinHandle<()>,
}

impl Running {
    /// URL that opens the UI already signed in.
    pub fn url(&self) -> String {
        format!("http://{}/?token={}", self.addr, self.token)
    }
}

struct AppState {
    store: Arc<Mutex<Store>>,
    qrz: tokio::sync::Mutex<Option<QrzClient>>,
    qrz_endpoint: String,
    secret_service: String,
    token: String,
    data_dir: PathBuf,
    hub: Arc<Hub>,
    cluster: Arc<Cluster>,
    qsl: Arc<Qsl>,
    propagation: Arc<propagation::Propagation>,
    startup: Arc<startup::Startup>,
    /// Award tables by request, with the QSO version they were counted at.
    award_cache: AwardCache,
    /// What happened to a restore staged before this start.
    last_restore: Option<backups::RestoreResult>,
}

type AwardCache = Arc<Mutex<std::collections::HashMap<String, (i64, Arc<AwardTable>)>>>;

type Shared = Arc<AppState>;

/// Default data folder: %APPDATA%\QRZero on Windows, the platform equivalent elsewhere.
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("QRZero")
}

/// Opens the database (creating a first log if there is none) and starts serving.
pub async fn start(cfg: Config) -> anyhow::Result<Running> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    let (store, last_restore) = backups::open_store(&cfg.data_dir)?;
    if store.list_logs()?.is_empty() {
        store.create_log("My log")?;
    }
    let backup_settings = backups::load_settings(&store)?;
    let dir = cfg.data_dir.clone();
    tokio::task::spawn_blocking(move || backups::auto_backup(&dir, backup_settings));
    let token = cfg.token.unwrap_or_else(random_token);
    let store = Arc::new(Mutex::new(store));
    let hub = Hub::new(store.clone(), cfg.data_dir.clone());
    hub.start(cfg.update_cty);
    let cluster = Cluster::new(&hub);
    let qsl = Qsl::new(store.clone(), hub.clone(), cfg.secret_service.clone(), cfg.data_dir.clone(), cfg.qsl_endpoints);
    qsl.start();
    let startup = startup::Startup::new();
    if cfg.launch_apps {
        startup.launch_all(saved_startup_apps(&hub));
    }
    let state = Arc::new(AppState {
        store,
        hub,
        startup,
        cluster,
        qsl,
        propagation: propagation::Propagation::new(cfg.propagation_url),
        qrz: tokio::sync::Mutex::new(None),
        qrz_endpoint: cfg.qrz_endpoint,
        secret_service: cfg.secret_service,
        token: token.clone(),
        data_dir: cfg.data_dir,
        award_cache: AwardCache::default(),
        last_restore,
    });
    // QSOs logged by WSJT-X, JTDX or N1MM are looked up on QRZ in the background.
    let (lookup_tx, mut lookup_rx) = tokio::sync::mpsc::unbounded_channel::<(i64, i64, String)>();
    state.hub.set_auto_lookup(lookup_tx);
    // Weak, so this task doesn't keep the database open after the server stops (Windows can't then replace the file on restore).
    let weak = Arc::downgrade(&state);
    tokio::spawn(async move {
        while let Some((log_id, id, call)) = lookup_rx.recv().await {
            let Some(s2) = weak.upgrade() else { break };
            match lookup_into_qso(&s2, id, &call).await {
                Ok(filled) if !filled.is_empty() => {
                    s2.hub.emit(json!({"type": "qso_logged", "log_id": log_id, "call": "", "source": "QRZ", "added": false}));
                }
                Ok(_) => {}
                Err(e) => tracing::info!("QRZ lookup for auto-logged {call}: {e}"),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(cfg.addr).await?;
    let addr = listener.local_addr()?;
    let app = router(state);
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("server stopped: {e}");
        }
    });
    Ok(Running { addr, token, handle })
}

fn random_token() -> String {
    use rand::Rng;
    let bytes: [u8; 24] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn router(state: Shared) -> Router {
    let api = Router::new()
        .route("/info", get(info))
        .route("/logs", get(list_logs).post(create_log))
        .route("/logs/{id}", put(rename_log).delete(delete_log))
        .route("/logs/{id}/callsigns", get(list_callsigns).post(add_callsign))
        .route("/callsigns/{id}", axum::routing::delete(delete_callsign))
        .route("/callsigns/{id}/default", post(default_callsign))
        .route("/logs/{id}/locations", get(list_locations).post(create_location))
        .route("/locations/{id}", put(update_location).delete(delete_location))
        .route("/locations/{id}/default", post(default_location))
        .route("/logs/{id}/qsos", post(insert_qso))
        .route("/logs/{id}/qsos/search", post(search_qsos))
        .route("/qsos/{id}", get(get_qso).put(update_qso))
        .route("/qsos/delete", post(delete_qsos))
        .route("/qsos/mark", post(mark_qsos))
        .route("/qsos/lookup", post(lookup_qsos))
        .route("/qsos/send", post(send_qsos))
        .route("/logs/{id}/paper-queue", get(paper_queue))
        .route("/logs/{id}/awards/{award}", get(award))
        .route("/logs/{id}/lookup/{call}", get(lookup))
        .route("/logs/{id}/award-hints", get(award_hints))
        .route("/logs/{id}/notes", get(list_notes))
        .route("/logs/{id}/notes/{call}", get(get_note).put(put_note).delete(delete_note))
        .route(
            "/logs/{id}/import",
            post(import).layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route("/logs/{id}/export", post(export))
        .route("/logs/{id}/equipment", get(list_equipment))
        .route("/locations/{id}/equipment", post(create_equipment))
        .route("/equipment/{id}", put(update_equipment).delete(delete_equipment))
        .route("/equipment/{id}/move", post(move_equipment))
        .route("/prefs/{key}", get(get_pref).put(put_pref))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/settings/qrz/test", post(test_qrz))
        .route("/events", get(events))
        .route("/station/active", post(set_active))
        .route("/radios/tune", post(tune))
        .route("/integrations", get(get_integrations).put(put_integrations))
        .route("/udp-connections", get(udp_get).put(udp_put))
        .route("/udp-connections/test", post(udp_test))
        .route("/startup-apps", get(startup_get).put(startup_put))
        .route("/startup-apps/launch", post(startup_launch))
        .route("/ftx", get(ftx))
        .route("/ftx/reply", post(ftx_reply))
        .route("/ftx/halt", post(ftx_halt))
        .route("/ftx/free-text", post(ftx_free_text))
        .route("/ftx/configure", post(ftx_configure))
        .route("/ftx/replay", post(ftx_replay))
        .route("/ftx/clear", post(ftx_clear))
        .route("/ftx/switch-configuration", post(ftx_switch_configuration))
        .route("/rotator", post(rotate))
        .route("/cty", get(cty_status).post(cty_install))
        .route("/cty/update", post(cty_update))
        .route("/cty/entities", get(cty_entities))
        .route("/watch", get(watch_get).put(watch_put))
        .route("/watch/hits", get(watch_hits))
        .route("/cluster", get(cluster_get).put(cluster_put))
        .route("/cluster/connect", post(cluster_connect))
        .route("/cluster/send", post(cluster_send))
        .route("/qsl", get(qsl_get).put(qsl_put))
        .route("/propagation", get(propagation_get))
        .route("/qsl/qrz/test", post(qsl_test_qrz))
        .route("/qsl/upload/{service}", post(qsl_upload))
        .route("/qsl/download/{service}", post(qsl_download))
        .merge(backups::routes())
        .layer(middleware::from_fn_with_state(state.clone(), require_token))
        .with_state(state);
    Router::new().nest("/api", api).fallback(static_file)
}

async fn require_token(State(s): State<Shared>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get("x-qrzero-token")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| constant_time_eq(t.as_bytes(), s.token.as_bytes()));
    if ok {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, Json(json!({"error": "missing or wrong session token"}))).into_response()
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn static_file(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (file, path) = match UiAssets::get(path) {
        Some(f) if !path.is_empty() => (f, path),
        _ => match UiAssets::get("index.html") {
            Some(f) => (f, "index.html"),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    (
        [(header::CONTENT_TYPE, mime.as_ref().to_string()), (header::CACHE_CONTROL, cache.to_string())],
        Body::from(file.data.into_owned()),
    )
        .into_response()
}

// ---- errors ------------------------------------------------------------

struct ApiError(StatusCode, String);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        let code = match e {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Invalid(_) => StatusCode::BAD_REQUEST,
            Error::Lookup(_) => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError(code, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

/// Runs a database call off the async threads.
async fn db<T, F>(s: &Shared, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Store) -> qrzero_core::Result<T> + Send + 'static,
{
    let store = s.store.clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = store.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut guard)
    })
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(ApiError::from)
}

// ---- handlers ----------------------------------------------------------

async fn info(State(s): State<Shared>) -> Json<serde_json::Value> {
    Json(json!({ "version": VERSION, "data_dir": s.data_dir.display().to_string() }))
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

async fn list_logs(State(s): State<Shared>) -> ApiResult<Vec<Log>> {
    db(&s, |st| st.list_logs()).await.map(Json)
}

async fn create_log(State(s): State<Shared>, Json(b): Json<NameBody>) -> ApiResult<Log> {
    db(&s, move |st| st.create_log(&b.name)).await.map(Json)
}

async fn rename_log(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<NameBody>) -> ApiResult<()> {
    db(&s, move |st| st.rename_log(id, &b.name)).await.map(Json)
}

async fn delete_log(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| {
        if st.list_logs()?.len() <= 1 {
            return Err(Error::Invalid("you can't delete the only log".into()));
        }
        st.delete_log(id)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct CallsignBody {
    callsign: String,
}

async fn list_callsigns(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<Vec<StationCallsign>> {
    db(&s, move |st| st.list_callsigns(id)).await.map(Json)
}

async fn add_callsign(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<CallsignBody>) -> ApiResult<StationCallsign> {
    db(&s, move |st| st.add_callsign(id, &b.callsign)).await.map(Json)
}

async fn delete_callsign(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| st.delete_callsign(id)).await.map(Json)
}

async fn default_callsign(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| st.set_default_callsign(id)).await.map(Json)
}

#[derive(Deserialize)]
struct LocationBody {
    name: String,
    #[serde(default)]
    fields: Fields,
}

async fn list_locations(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<Vec<Location>> {
    db(&s, move |st| st.list_locations(id)).await.map(Json)
}

async fn create_location(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<LocationBody>) -> ApiResult<Location> {
    db(&s, move |st| st.create_location(id, &b.name, &b.fields)).await.map(Json)
}

async fn update_location(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<LocationBody>) -> ApiResult<Location> {
    db(&s, move |st| st.update_location(id, &b.name, &b.fields)).await.map(Json)
}

async fn delete_location(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| st.delete_location(id)).await.map(Json)
}

async fn default_location(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| st.set_default_location(id)).await.map(Json)
}

#[derive(Deserialize)]
struct EquipmentBody {
    location_id: Option<i64>,
    kind: String,
    name: String,
    #[serde(default)]
    fields: Fields,
}

async fn list_equipment(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<Vec<Equipment>> {
    db(&s, move |st| st.list_equipment(id)).await.map(Json)
}

async fn create_equipment(State(s): State<Shared>, Path(loc): Path<i64>, Json(b): Json<EquipmentBody>) -> ApiResult<Equipment> {
    let e = db(&s, move |st| st.create_equipment(loc, &b.kind, &b.name, &b.fields)).await?;
    s.hub.reload_rigs();
    Ok(Json(e))
}

async fn update_equipment(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<EquipmentBody>) -> ApiResult<Equipment> {
    let e = db(&s, move |st| {
        let loc = match b.location_id {
            Some(l) => l,
            None => st.get_equipment(id)?.location_id,
        };
        st.update_equipment(id, loc, &b.kind, &b.name, &b.fields)
    })
    .await?;
    s.hub.reload_rigs();
    Ok(Json(e))
}

async fn delete_equipment(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| st.delete_equipment(id)).await?;
    s.hub.reload_rigs();
    Ok(Json(()))
}

#[derive(Deserialize)]
struct MoveBody {
    delta: i64,
}

async fn move_equipment(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<MoveBody>) -> ApiResult<()> {
    db(&s, move |st| st.move_equipment(id, b.delta)).await.map(Json)
}

/// UI preferences (entry field layout, grid columns, ...) stored as JSON in
/// the database so they survive reinstalls and follow the log file.
fn pref_key(key: &str) -> Result<String, ApiError> {
    if key.is_empty() || key.len() > 64 || !key.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "bad preference name".into()));
    }
    Ok(format!("pref.{key}"))
}

async fn get_pref(State(s): State<Shared>, Path(key): Path<String>) -> ApiResult<serde_json::Value> {
    let key = pref_key(&key)?;
    let raw = db(&s, move |st| st.get_setting(&key)).await?;
    Ok(Json(raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or(serde_json::Value::Null)))
}

async fn put_pref(State(s): State<Shared>, Path(key): Path<String>, Json(v): Json<serde_json::Value>) -> ApiResult<()> {
    let key = pref_key(&key)?;
    db(&s, move |st| if v.is_null() { st.delete_setting(&key) } else { st.set_setting(&key, &v.to_string()) })
        .await
        .map(Json)
}

#[derive(Deserialize)]
struct QsoBody {
    location_id: Option<i64>,
    fields: Fields,
}

async fn insert_qso(State(s): State<Shared>, Path(id): Path<i64>, Json(mut b): Json<QsoBody>) -> ApiResult<Qso> {
    s.hub.fill_from_cty(&mut b.fields);
    let qso = db(&s, move |st| st.insert_qso(id, b.location_id, &b.fields)).await?;
    s.hub.note_qso(id, &qso.fields);
    s.hub.send_qso(&qso.fields, qso.id);
    Ok(Json(qso))
}

async fn get_qso(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<Qso> {
    db(&s, move |st| st.get_qso(id)).await.map(Json)
}

async fn update_qso(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<QsoBody>) -> ApiResult<Qso> {
    let qso = db(&s, move |st| st.update_qso(id, b.location_id, &b.fields)).await?;
    s.hub.rebuild_worked();
    Ok(Json(qso))
}

#[derive(Deserialize)]
struct IdsBody {
    ids: Vec<i64>,
}

#[derive(Deserialize)]
struct MarkBody {
    ids: Vec<i64>,
    fields: Fields,
}

/// Sets QSL fields on several QSOs at once (paper QSL queue, sent, received).
async fn mark_qsos(State(s): State<Shared>, Json(b): Json<MarkBody>) -> ApiResult<usize> {
    let n = b.ids.len();
    db(&s, move |st| st.mark_qsos(&b.ids, &b.fields)).await?;
    s.hub.refresh_awards();
    s.hub.emit(json!({"type": "qso_logged", "log_id": null, "call": "", "source": "mark", "added": false}));
    Ok(Json(n))
}

#[derive(Deserialize)]
struct AwardQuery {
    /// Comma-separated station callsigns; empty means all.
    #[serde(default)]
    calls: String,
    #[serde(default)]
    lotw: bool,
    #[serde(default)]
    paper: bool,
    #[serde(default)]
    eqsl: bool,
    #[serde(default)]
    unworked: bool,
}

async fn award(State(s): State<Shared>, Path((id, award)): Path<(i64, Award)>, Query(q): Query<AwardQuery>) -> ApiResult<AwardTable> {
    let cty = s.hub.cty();
    let mut names = std::collections::BTreeMap::new();
    if let (Award::Dxcc, Some(cty)) = (award, &cty) {
        for e in cty.entities() {
            if let Some(d) = e.dxcc {
                names.entry(d.to_string()).or_insert_with(|| e.name.clone());
            }
        }
    }
    let calls: Vec<String> = q.calls.split(',').map(|c| c.trim().to_ascii_uppercase()).filter(|c| !c.is_empty()).collect();
    let counts = Counts { lotw: q.lotw, paper: q.paper, eqsl: q.eqsl };
    // Counting a big log takes a moment, so keep the last few tables until a QSO changes.
    let key = format!("{id}|{award:?}|{}|{}{}{}|{}|{}", calls.join(","), q.lotw, q.paper, q.eqsl, q.unworked, cty.as_ref().map_or(0, |c| c.len()));
    let cache = s.award_cache.clone();
    let table = db(&s, move |st| {
        let version = st.qso_version()?;
        if let Some(t) = cache.lock().unwrap_or_else(|p| p.into_inner()).get(&key).filter(|(v, _)| *v == version) {
            return Ok(t.1.clone());
        }
        let mut tally = Tally::new(award, counts, names);
        let resolve = |c: &str| cty_facts(cty.as_deref(), c);
        st.for_each_award_qso(id, &calls, resolve, |qso| tally.add(qso))?;
        let table = Arc::new(tally.finish(q.unworked));
        let mut c = cache.lock().unwrap_or_else(|p| p.into_inner());
        c.retain(|_, (v, _)| *v == version);
        c.insert(key, (version, table.clone()));
        Ok(table)
    })
    .await?;
    Ok(Json(table.as_ref().clone()))
}

#[derive(Deserialize)]
struct HintQuery {
    call: String,
    #[serde(default)]
    band: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    state: String,
    /// CQ zone and DXCC entity; taken from the country file when missing.
    cqz: Option<String>,
    dxcc: Option<String>,
    /// Grid square, IOTA reference and county ("ST,Name") of the station, when known.
    grid: Option<String>,
    iota: Option<String>,
    cnty: Option<String>,
    #[serde(default)]
    lotw: bool,
    #[serde(default)]
    paper: bool,
    #[serde(default)]
    eqsl: bool,
}

/// What a QSO with a station on a band and mode would add to each award.
async fn award_hints(State(s): State<Shared>, Path(log_id): Path<i64>, Query(q): Query<HintQuery>) -> ApiResult<Vec<AwardHint>> {
    let mut fields = Fields::new();
    for (k, v) in [("CALL", &q.call), ("BAND", &q.band), ("MODE", &q.mode), ("STATE", &q.state)] {
        fields.insert(k.into(), v.clone());
    }
    for (k, v) in [("CQZ", &q.cqz), ("DXCC", &q.dxcc), ("GRIDSQUARE", &q.grid), ("IOTA", &q.iota), ("CNTY", &q.cnty)] {
        if let Some(v) = v.as_ref().filter(|v| !v.trim().is_empty()) {
            fields.insert(k.into(), v.clone());
        }
    }
    // The same country-file facts a logged QSO would get.
    s.hub.fill_from_cty(&mut fields);
    let qso = AwardQso::from_fields(&fields, |_| CtyFacts::default());
    let counts = Counts { lotw: q.lotw, paper: q.paper, eqsl: q.eqsl };
    let hub = s.hub.clone();
    let mut hints = tokio::task::spawn_blocking(move || hub.award_hints(log_id, &qso, counts))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(ApiError::from)?;
    if let Some(cty) = s.hub.cty() {
        for h in hints.iter_mut().filter(|h| h.award == Award::Dxcc) {
            if let Some(e) = cty.entities().iter().find(|e| e.dxcc.is_some_and(|d| d.to_string() == h.key)) {
                h.name = e.name.clone();
            }
        }
    }
    Ok(Json(hints))
}

async fn paper_queue(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<Vec<Qso>> {
    Ok(Json(db(&s, move |st| st.paper_queue(id)).await?))
}

/// Fields a callsign lookup may fill in on a logged QSO: never the call, the operator's own
/// MY_ fields, or anything the QSO already has.
fn fill_blanks(fields: &mut Fields, station: &Fields) -> Vec<String> {
    let mut filled = Vec::new();
    for (k, v) in station {
        if k == "CALL" || k.starts_with("MY_") || v.trim().is_empty() {
            continue;
        }
        if fields.get(k).is_none_or(|old| old.trim().is_empty()) {
            fields.insert(k.clone(), v.clone());
            filled.push(k.clone());
        }
    }
    filled
}

/// Looks up a logged QSO's call on QRZ and fills in its blank fields. Returns the fields filled.
async fn lookup_into_qso(s: &Shared, id: i64, call: &str) -> Result<Vec<String>, String> {
    let Some((station, _)) = qrz_lookup(s, call).await? else {
        let settings = db(s, |st| load_settings(st)).await.map_err(|e| e.1)?;
        if !settings.qrz_enabled || settings.qrz_username.is_empty() {
            return Err("QRZ lookups are off: add your QRZ login in Settings".into());
        }
        return Err("not found on QRZ".into());
    };
    let filled = db(s, move |st| {
        let qso = st.get_qso(id)?;
        let mut fields = qso.fields.clone();
        let filled = fill_blanks(&mut fields, &station);
        if !filled.is_empty() {
            st.update_qso(id, qso.location_id, &fields)?;
        }
        Ok(filled)
    })
    .await
    .map_err(|e| e.1)?;
    if !filled.is_empty() {
        s.hub.rebuild_worked();
    }
    Ok(filled)
}

#[derive(Serialize)]
struct LookupQsosResult {
    /// QSOs that gained at least one field.
    updated: usize,
    /// One line per QSO that couldn't be looked up.
    errors: Vec<String>,
}

/// Looks up the chosen QSOs on QRZ and fills in what they're missing.
async fn lookup_qsos(State(s): State<Shared>, Json(b): Json<IdsBody>) -> ApiResult<LookupQsosResult> {
    let mut out = LookupQsosResult { updated: 0, errors: Vec::new() };
    for id in b.ids {
        let qso = db(&s, move |st| st.get_qso(id)).await?;
        let call = qso.fields.get("CALL").cloned().unwrap_or_default();
        match lookup_into_qso(&s, id, &call).await {
            Ok(f) if !f.is_empty() => out.updated += 1,
            Ok(_) => {}
            Err(e) => {
                let off = e.starts_with("QRZ lookups are off");
                out.errors.push(if off { e } else { format!("{call}: {e}") });
                if off {
                    break;
                }
            }
        }
    }
    if out.updated > 0 {
        s.hub.emit(json!({"type": "qso_logged", "log_id": null, "call": "", "source": "QRZ", "added": false}));
    }
    Ok(Json(out))
}

/// Sends the chosen QSOs out through the user's "QSO logged" UDP connections, as if just logged.
async fn send_qsos(State(s): State<Shared>, Json(b): Json<IdsBody>) -> ApiResult<usize> {
    if !s.hub.udp.wants(udp_out::Event::QsoLogged) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "No UDP connection is set up for QSO logged. Add one in Settings, UDP connections.".into()));
    }
    let ids = b.ids;
    let qsos = db(&s, move |st| ids.iter().map(|&id| st.get_qso(id)).collect::<Result<Vec<_>, _>>()).await?;
    for q in &qsos {
        s.hub.send_qso(&q.fields, q.id);
    }
    Ok(Json(qsos.len()))
}

async fn delete_qsos(State(s): State<Shared>, Json(b): Json<IdsBody>) -> ApiResult<usize> {
    let n = db(&s, move |st| st.delete_qsos(&b.ids)).await?;
    s.hub.rebuild_worked();
    Ok(Json(n))
}

#[derive(Deserialize)]
struct SearchBody {
    #[serde(default)]
    filter: QsoFilter,
    #[serde(default)]
    sort: Sort,
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    200
}

#[derive(Serialize)]
struct SearchResult {
    total: i64,
    rows: Vec<Qso>,
}

async fn search_qsos(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<SearchBody>) -> ApiResult<SearchResult> {
    db(&s, move |st| st.search(id, &b.filter, b.sort, b.offset, b.limit))
        .await
        .map(|(total, rows)| Json(SearchResult { total, rows }))
}

#[derive(Serialize)]
struct LookupResult {
    /// The call looked up, uppercase.
    call: String,
    worked: WorkedBefore,
    /// The DXCC entity from the country file.
    entity: Option<qrzero_core::cty::Entity>,
    /// Station details from the lookup service, as ADIF fields.
    station: Option<Fields>,
    source: Option<&'static str>,
    error: Option<String>,
    /// The operator's station note for the (base) call.
    note: Option<String>,
}

async fn lookup(State(s): State<Shared>, Path((log_id, call)): Path<(i64, String)>) -> ApiResult<LookupResult> {
    let call = call.trim().to_ascii_uppercase();
    let call_for_udp = call.clone();
    let mut result = LookupResult { call: call.clone(), worked: WorkedBefore::default(), entity: s.hub.entity(&call), station: None, source: None, error: None, note: None };
    match qrz_lookup(&s, &call).await {
        Ok(Some((fields, source))) => {
            result.station = Some(fields);
            result.source = Some(source);
        }
        Ok(None) => {}
        Err(e) => result.error = Some(e),
    }
    let dxcc = result
        .station
        .as_ref()
        .and_then(|f| f.get("DXCC"))
        .and_then(|d| d.parse().ok())
        .or_else(|| result.entity.as_ref().and_then(|e| e.dxcc).map(i64::from));
    let (worked, note) = db(&s, move |st| {
        let note = if qrzero_core::store::base_call(&call).is_empty() { None } else { st.get_note(log_id, &call)? };
        Ok((st.worked_before(log_id, &call, dxcc)?, note.map(|n| n.text)))
    })
    .await?;
    result.worked = worked;
    result.note = note;
    if s.hub.udp.wants(udp_out::Event::Lookup) {
        let (hub, station, entity, call) = (s.hub.clone(), result.station.clone(), result.entity.clone(), call_for_udp);
        tokio::task::spawn_blocking(move || hub.send_lookup(&call, station.as_ref(), entity.as_ref()));
    }
    Ok(Json(result))
}

#[derive(Deserialize)]
struct NotesQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    offset: i64,
    limit: Option<i64>,
}

#[derive(Serialize)]
struct NotesPage {
    total: i64,
    rows: Vec<Note>,
}

async fn list_notes(State(s): State<Shared>, Path(log_id): Path<i64>, Query(q): Query<NotesQuery>) -> ApiResult<NotesPage> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    db(&s, move |st| st.list_notes(log_id, &q.q, q.offset, limit))
        .await
        .map(|(total, rows)| Json(NotesPage { total, rows }))
}

async fn get_note(State(s): State<Shared>, Path((log_id, call)): Path<(i64, String)>) -> ApiResult<Option<Note>> {
    db(&s, move |st| st.get_note(log_id, &call)).await.map(Json)
}

#[derive(Deserialize)]
struct NoteBody {
    text: String,
}

/// Saves a note; blank text deletes it and answers null.
async fn put_note(State(s): State<Shared>, Path((log_id, call)): Path<(i64, String)>, Json(b): Json<NoteBody>) -> ApiResult<Option<Note>> {
    db(&s, move |st| st.set_note(log_id, &call, &b.text)).await.map(Json)
}

async fn delete_note(State(s): State<Shared>, Path((log_id, call)): Path<(i64, String)>) -> ApiResult<bool> {
    db(&s, move |st| st.delete_note(log_id, &call)).await.map(Json)
}

/// QRZ lookup with a 30-day cache. Ok(None) when lookups are off or the call is unknown.
async fn qrz_lookup(s: &Shared, call: &str) -> Result<Option<(Fields, &'static str)>, String> {
    let c = call.to_string();
    let cached = db(s, move |st| st.cached_lookup(&c, LOOKUP_CACHE_SECS)).await.map_err(|e| e.1)?;
    if let Some(f) = cached {
        return Ok(Some((f, "QRZ (cached)")));
    }
    let Some(mut guard) = qrz_client(s).await? else {
        return Ok(None);
    };
    let client = guard.as_mut().expect("client present");
    let mut found = client.lookup(call).await.map_err(|e| e.to_string())?;
    // Portable or prefixed calls (EA8/G4ABC, W1AW/P): fall back to the home call.
    if found.is_none() && call.contains('/') {
        if let Some(base) = call.split('/').max_by_key(|p| p.len()) {
            found = client.lookup(base).await.map_err(|e| e.to_string())?;
        }
    }
    drop(guard);
    if let Some(f) = &found {
        let (c, f2) = (call.to_string(), f.clone());
        db(s, move |st| st.cache_lookup(&c, "qrz", &f2)).await.map_err(|e| e.1)?;
    }
    Ok(found.map(|f| (f, "QRZ")))
}

/// Returns the QRZ client, creating it from settings, or None when lookups are disabled.
async fn qrz_client(s: &Shared) -> Result<Option<tokio::sync::MutexGuard<'_, Option<QrzClient>>>, String> {
    let settings = db(s, |st| load_settings(st)).await.map_err(|e| e.1)?;
    if !settings.qrz_enabled || settings.qrz_username.is_empty() {
        return Ok(None);
    }
    let svc = s.secret_service.clone();
    let password = db(s, move |st| secrets::get(st, &svc, "qrz")).await.map_err(|e| e.1)?.unwrap_or_default();
    if password.is_empty() {
        return Err("QRZ password is not set".into());
    }
    let mut guard = s.qrz.lock().await;
    let stale = guard
        .as_ref()
        .is_none_or(|c| !c.same_credentials(&settings.qrz_username, &password));
    if stale {
        *guard = Some(QrzClient::new(&s.qrz_endpoint, &settings.qrz_username, &password));
    }
    Ok(Some(guard))
}

#[derive(Deserialize)]
struct ImportQuery {
    location_id: Option<i64>,
    #[serde(default)]
    apply_location: ApplyLocation,
    #[serde(default)]
    skip_duplicates: bool,
    #[serde(default)]
    add_station_callsigns: bool,
}

async fn import(State(s): State<Shared>, Path(id): Path<i64>, Query(q): Query<ImportQuery>, body: Bytes) -> ApiResult<ImportReport> {
    let opts = ImportOptions {
        location_id: q.location_id,
        apply_location: q.apply_location,
        skip_duplicates: q.skip_duplicates,
        add_station_callsigns: q.add_station_callsigns,
    };
    let report = db(&s, move |st| st.import_adif(id, &body, &opts)).await?;
    s.hub.rebuild_worked();
    Ok(Json(report))
}

#[derive(Deserialize)]
struct ExportBody {
    #[serde(default)]
    filter: QsoFilter,
    #[serde(default)]
    profile: ExportProfile,
}

async fn export(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<ExportBody>) -> Result<Response, ApiError> {
    let (text, count) = db(&s, move |st| st.export_adif(id, &b.filter, b.profile, VERSION)).await?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/plain; charset=utf-8".parse().unwrap());
    headers.insert("x-qso-count", count.into());
    Ok((headers, text).into_response())
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
    qrz_enabled: bool,
    qrz_username: String,
    #[serde(default)]
    qrz_password_set: bool,
}

fn load_settings(st: &Store) -> qrzero_core::Result<Settings> {
    Ok(Settings {
        qrz_enabled: st.get_setting("qrz.enabled")?.as_deref() == Some("1"),
        qrz_username: st.get_setting("qrz.username")?.unwrap_or_default(),
        qrz_password_set: false,
    })
}

async fn get_settings(State(s): State<Shared>) -> ApiResult<Settings> {
    let svc = s.secret_service.clone();
    db(&s, move |st| {
        let mut out = load_settings(st)?;
        out.qrz_password_set = secrets::get(st, &svc, "qrz")?.is_some_and(|p| !p.is_empty());
        Ok(out)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct SettingsUpdate {
    qrz_enabled: Option<bool>,
    qrz_username: Option<String>,
    /// Omit to keep the stored password; empty string clears it.
    qrz_password: Option<String>,
}

async fn put_settings(State(s): State<Shared>, Json(b): Json<SettingsUpdate>) -> ApiResult<Settings> {
    let svc = s.secret_service.clone();
    db(&s, move |st| {
        if let Some(e) = b.qrz_enabled {
            st.set_setting("qrz.enabled", if e { "1" } else { "0" })?;
        }
        if let Some(u) = b.qrz_username {
            st.set_setting("qrz.username", u.trim())?;
        }
        if let Some(p) = b.qrz_password {
            secrets::set(st, &svc, "qrz", if p.is_empty() { None } else { Some(&p) })?;
        }
        Ok(())
    })
    .await?;
    *s.qrz.lock().await = None;
    get_settings(State(s)).await
}

async fn test_qrz(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    let settings = db(&s, |st| load_settings(st)).await?;
    let svc = s.secret_service.clone();
    let password = db(&s, move |st| secrets::get(st, &svc, "qrz")).await?.unwrap_or_default();
    let mut client = QrzClient::new(&s.qrz_endpoint, &settings.qrz_username, &password);
    client.test_login().await.map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

// ---- live station ------------------------------------------------------

/// Server-sent events: radio state, FTx decodes, QSOs logged by other programs.
async fn events(State(s): State<Shared>) -> axum::response::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use tokio::sync::broadcast::error::RecvError;
    let (snapshot, rx) = s.hub.subscribe();
    let first = futures_util::stream::iter(snapshot.into_iter().map(|e| Ok(Event::default().data(&*e))));
    let live = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(e) => return Some((Ok(Event::default().data(&*e)), rx)),
                // A slow client skips what it missed rather than stalling everyone.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(futures_util::StreamExt::chain(first, live)).keep_alive(KeepAlive::default())
}

async fn set_active(State(s): State<Shared>, Json(b): Json<Active>) -> ApiResult<()> {
    s.hub.set_active(b);
    Ok(Json(()))
}

#[derive(Deserialize)]
struct TuneBody {
    key: String,
    freq_hz: Option<u64>,
    mode: Option<String>,
}

async fn tune(State(s): State<Shared>, Json(b): Json<TuneBody>) -> ApiResult<()> {
    s.hub.tune(&b.key, b.freq_hz, b.mode).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(()))
}

async fn get_integrations(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    let (cfg, status) = s.hub.integrations();
    Ok(Json(json!({ "config": cfg, "status": status })))
}

async fn put_integrations(State(s): State<Shared>, Json(cfg): Json<Integrations>) -> ApiResult<serde_json::Value> {
    s.hub.save_integrations(cfg)?;
    // Give the listeners a moment to bind so the status says whether it worked.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    get_integrations(State(s)).await
}

// ---- UDP connections and startup programs ------------------------------------

async fn udp_get(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    Ok(Json(json!({ "connections": s.hub.udp.connections(), "status": s.hub.udp.status() })))
}

async fn udp_put(State(s): State<Shared>, Json(conns): Json<Vec<udp_out::UdpConnection>>) -> ApiResult<serde_json::Value> {
    let saved = s.hub.udp.set(conns);
    let text = serde_json::to_string(&saved).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    s.hub.set_setting(udp_out::SETTING_KEY, &text).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    // Relay addresses are looked up in the background; give that a moment for the status.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    udp_get(State(s)).await
}

/// Sends an example message on a connection (saved or not) and answers with what was sent.
async fn udp_test(State(s): State<Shared>, Json(conn): Json<udp_out::UdpConnection>) -> ApiResult<serde_json::Value> {
    let sent = s.hub.udp.test(&conn).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!({ "sent": sent })))
}

fn saved_startup_apps(hub: &Hub) -> Vec<startup::StartupApp> {
    hub.setting(startup::SETTING_KEY).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

async fn startup_get(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    Ok(Json(json!({ "apps": saved_startup_apps(&s.hub), "status": s.startup.status() })))
}

async fn startup_put(State(s): State<Shared>, Json(apps): Json<Vec<startup::StartupApp>>) -> ApiResult<serde_json::Value> {
    let apps = startup::normalize(apps);
    let text = serde_json::to_string(&apps).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    s.hub.set_setting(startup::SETTING_KEY, &text).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    s.startup.forget_others(&apps);
    startup_get(State(s)).await
}

/// Starts one program now ("Launch now" in Settings).
async fn startup_launch(State(s): State<Shared>, Json(app): Json<startup::StartupApp>) -> ApiResult<startup::AppStatus> {
    let st = s.startup.clone();
    let status = tokio::task::spawn_blocking(move || st.launch_one(&app))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(status))
}

#[derive(Deserialize)]
struct FtxQuery {
    #[serde(default)]
    since: u64,
}

async fn ftx(State(s): State<Shared>, Query(q): Query<FtxQuery>) -> ApiResult<serde_json::Value> {
    Ok(Json(s.hub.ftx_snapshot(q.since)))
}

#[derive(Deserialize)]
struct SeqBody {
    seq: u64,
}

async fn ftx_reply(State(s): State<Shared>, Json(b): Json<SeqBody>) -> ApiResult<()> {
    s.hub.ftx_reply(b.seq).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(()))
}

// Requests to one WSJT-X / JTDX instance, named by its id. They only work when "Accept UDP
// requests" is ticked in its Reporting settings.

fn ftx_err(e: String) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, e)
}

#[derive(Deserialize)]
struct FtxHaltBody {
    instance: String,
    /// Just untick Enable Tx, so the current transmission finishes.
    #[serde(default)]
    auto_only: bool,
}

async fn ftx_halt(State(s): State<Shared>, Json(b): Json<FtxHaltBody>) -> ApiResult<()> {
    s.hub.ftx_send(&b.instance, |i| qrzero_radio::wsjtx::encode_halt_tx(&i.id, b.auto_only)).await.map_err(ftx_err)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct FtxFreeTextBody {
    instance: String,
    text: String,
    /// Send it next (as the Tx5 "Now" button would) rather than just setting it.
    #[serde(default)]
    send: bool,
}

async fn ftx_free_text(State(s): State<Shared>, Json(b): Json<FtxFreeTextBody>) -> ApiResult<()> {
    let text = b.text.trim().to_ascii_uppercase();
    s.hub.ftx_send(&b.instance, |i| qrzero_radio::wsjtx::encode_free_text(&i.id, &text, b.send)).await.map_err(ftx_err)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct FtxConfigureBody {
    instance: String,
    mode: Option<String>,
    tr_period: Option<u32>,
    rx_df: Option<u32>,
    dx_call: Option<String>,
    dx_grid: Option<String>,
    #[serde(default)]
    generate_messages: bool,
}

async fn ftx_configure(State(s): State<Shared>, Json(b): Json<FtxConfigureBody>) -> ApiResult<()> {
    let up = |v: Option<String>| v.map(|v| v.trim().to_ascii_uppercase()).filter(|v| !v.is_empty());
    let mut c = qrzero_radio::wsjtx::Configure {
        mode: up(b.mode),
        tr_period: b.tr_period,
        rx_df: b.rx_df,
        dx_call: up(b.dx_call),
        dx_grid: b.dx_grid.map(|g| g.trim().to_string()).filter(|g| !g.is_empty()),
        generate_messages: b.generate_messages,
        ..Default::default()
    };
    s.hub
        .ftx_send(&b.instance, |i| {
            c.fast_mode = i.fast_mode;
            qrzero_radio::wsjtx::encode_configure(&i.id, &c)
        })
        .await
        .map_err(ftx_err)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct FtxInstanceBody {
    instance: String,
}

async fn ftx_replay(State(s): State<Shared>, Json(b): Json<FtxInstanceBody>) -> ApiResult<()> {
    s.hub.ftx_send(&b.instance, |i| qrzero_radio::wsjtx::encode_replay(&i.id)).await.map_err(ftx_err)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct FtxClearBody {
    instance: String,
    /// 0 Band Activity, 1 Rx Frequency, 2 both.
    #[serde(default = "both_windows")]
    window: u8,
}

fn both_windows() -> u8 {
    2
}

async fn ftx_clear(State(s): State<Shared>, Json(b): Json<FtxClearBody>) -> ApiResult<()> {
    if b.window > 2 {
        return Err(ftx_err("window is 0, 1 or 2".into()));
    }
    s.hub.ftx_clear(&b.instance, b.window).await.map_err(ftx_err)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct FtxSwitchBody {
    instance: String,
    name: String,
}

async fn ftx_switch_configuration(State(s): State<Shared>, Json(b): Json<FtxSwitchBody>) -> ApiResult<()> {
    let name = b.name.trim().to_string();
    if name.is_empty() {
        return Err(ftx_err("name the configuration to switch to".into()));
    }
    s.hub.ftx_send(&b.instance, |i| qrzero_radio::wsjtx::encode_switch_configuration(&i.id, &name)).await.map_err(ftx_err)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct RotateBody {
    azimuth: f64,
}

async fn rotate(State(s): State<Shared>, Json(b): Json<RotateBody>) -> ApiResult<()> {
    s.hub.rotate(b.azimuth).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(()))
}

async fn cty_status(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    Ok(Json(s.hub.cty_status()))
}

/// Installs a country file the user picked (when the download isn't possible).
async fn cty_install(State(s): State<Shared>, body: Bytes) -> ApiResult<serde_json::Value> {
    let text = String::from_utf8_lossy(&body).into_owned();
    s.hub.install_cty(&text).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?;
    s.hub.rebuild_worked();
    Ok(Json(s.hub.cty_status()))
}

async fn cty_update(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    s.hub
        .update_cty()
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, format!("couldn't download the country file: {e}")))?;
    s.hub.rebuild_worked();
    Ok(Json(s.hub.cty_status()))
}

/// Every entity in the country file, by name, for pickers.
async fn cty_entities(State(s): State<Shared>) -> ApiResult<Vec<serde_json::Value>> {
    let mut list: Vec<_> = s.hub.cty().map_or_else(Vec::new, |db| {
        db.entities().iter().map(|e| json!({"prefix": e.prefix, "name": e.name, "dxcc": e.dxcc, "cont": e.cont})).collect()
    });
    list.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(Json(list))
}

// ---- watch list ------------------------------------------------------------

async fn watch_get(State(s): State<Shared>) -> ApiResult<Vec<watch::WatchEntry>> {
    Ok(Json(s.hub.watch.entries()))
}

async fn watch_put(State(s): State<Shared>, Json(entries): Json<Vec<watch::WatchEntry>>) -> ApiResult<Vec<watch::WatchEntry>> {
    let saved = s.hub.watch.set_entries(entries);
    let text = serde_json::to_string(&saved).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    s.hub.set_setting(watch::SETTING_KEY, &text).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(saved))
}

async fn watch_hits(State(s): State<Shared>) -> ApiResult<Vec<watch::WatchHit>> {
    Ok(Json(s.hub.watch.hits()))
}

// ---- cluster -------------------------------------------------------------

async fn cluster_get(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    Ok(Json(s.cluster.snapshot()))
}

async fn cluster_put(State(s): State<Shared>, Json(cfg): Json<ClusterConfig>) -> ApiResult<serde_json::Value> {
    s.cluster.save_config(cfg).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(s.cluster.snapshot()))
}

#[derive(Deserialize)]
struct ConnectBody {
    connect: bool,
}

async fn cluster_connect(State(s): State<Shared>, Json(b): Json<ConnectBody>) -> ApiResult<()> {
    if b.connect {
        s.cluster.connect();
    } else {
        s.cluster.disconnect();
    }
    Ok(Json(()))
}

#[derive(Deserialize)]
struct LineBody {
    line: String,
}

async fn cluster_send(State(s): State<Shared>, Json(b): Json<LineBody>) -> ApiResult<()> {
    s.cluster.send(&b.line).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(()))
}

// ---- QSL services ----------------------------------------------------------

#[derive(Deserialize)]
struct PropagationQuery {
    #[serde(default)]
    refresh: bool,
}

async fn propagation_get(State(s): State<Shared>, Query(q): Query<PropagationQuery>) -> ApiResult<serde_json::Value> {
    Ok(Json(s.propagation.get(q.refresh).await))
}

async fn qsl_get(State(s): State<Shared>) -> ApiResult<serde_json::Value> {
    let q = s.qsl.clone();
    let v = tokio::task::spawn_blocking(move || q.overview())
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct QslBody {
    config: QslConfig,
    #[serde(default)]
    secrets: SecretsUpdate,
}

async fn qsl_put(State(s): State<Shared>, Json(b): Json<QslBody>) -> ApiResult<serde_json::Value> {
    s.qsl.save(b.config, b.secrets).map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    qsl_get(State(s)).await
}

async fn qsl_test_qrz(State(s): State<Shared>, Json(b): Json<CallsignBody>) -> ApiResult<serde_json::Value> {
    let call = s.qsl.test_qrz(&b.callsign).await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(json!({ "callsign": call })))
}

async fn qsl_download(State(s): State<Shared>, Path(service): Path<String>) -> ApiResult<qsl::Download> {
    Ok(Json(s.qsl.download(&service).await))
}

async fn qsl_upload(State(s): State<Shared>, Path(service): Path<String>) -> ApiResult<qsl::Run> {
    let run = match service.as_str() {
        "lotw" => s.qsl.upload_lotw().await,
        other => s.qsl.upload(other).await,
    };
    Ok(Json(run))
}
