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
use qrzero_core::model::*;
use qrzero_core::qrz::{QrzClient, DEFAULT_ENDPOINT};
use qrzero_core::store::Sort;
use qrzero_core::{secrets, Error, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
}

impl Config {
    pub fn local(data_dir: PathBuf) -> Self {
        Config {
            data_dir,
            addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            token: None,
            qrz_endpoint: DEFAULT_ENDPOINT.to_string(),
            secret_service: "QRZero".to_string(),
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
}

type Shared = Arc<AppState>;

/// Default data folder: %APPDATA%\QRZero on Windows, the platform equivalent elsewhere.
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("QRZero")
}

/// Opens the database (creating a first log if there is none) and starts serving.
pub async fn start(cfg: Config) -> anyhow::Result<Running> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    let store = Store::open(&cfg.data_dir.join("qrzero.db"))?;
    if store.list_logs()?.is_empty() {
        store.create_log("My log")?;
    }
    let token = cfg.token.unwrap_or_else(random_token);
    let state = Arc::new(AppState {
        store: Arc::new(Mutex::new(store)),
        qrz: tokio::sync::Mutex::new(None),
        qrz_endpoint: cfg.qrz_endpoint,
        secret_service: cfg.secret_service,
        token: token.clone(),
        data_dir: cfg.data_dir,
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
        .route("/logs/{id}/lookup/{call}", get(lookup))
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
    db(&s, move |st| st.create_equipment(loc, &b.kind, &b.name, &b.fields)).await.map(Json)
}

async fn update_equipment(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<EquipmentBody>) -> ApiResult<Equipment> {
    db(&s, move |st| {
        let loc = match b.location_id {
            Some(l) => l,
            None => st.get_equipment(id)?.location_id,
        };
        st.update_equipment(id, loc, &b.kind, &b.name, &b.fields)
    })
    .await
    .map(Json)
}

async fn delete_equipment(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<()> {
    db(&s, move |st| st.delete_equipment(id)).await.map(Json)
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

async fn insert_qso(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<QsoBody>) -> ApiResult<Qso> {
    db(&s, move |st| st.insert_qso(id, b.location_id, &b.fields)).await.map(Json)
}

async fn get_qso(State(s): State<Shared>, Path(id): Path<i64>) -> ApiResult<Qso> {
    db(&s, move |st| st.get_qso(id)).await.map(Json)
}

async fn update_qso(State(s): State<Shared>, Path(id): Path<i64>, Json(b): Json<QsoBody>) -> ApiResult<Qso> {
    db(&s, move |st| st.update_qso(id, b.location_id, &b.fields)).await.map(Json)
}

#[derive(Deserialize)]
struct IdsBody {
    ids: Vec<i64>,
}

async fn delete_qsos(State(s): State<Shared>, Json(b): Json<IdsBody>) -> ApiResult<usize> {
    db(&s, move |st| st.delete_qsos(&b.ids)).await.map(Json)
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
    worked: WorkedBefore,
    /// Station details from the lookup service, as ADIF fields.
    station: Option<Fields>,
    source: Option<&'static str>,
    error: Option<String>,
}

async fn lookup(State(s): State<Shared>, Path((log_id, call)): Path<(i64, String)>) -> ApiResult<LookupResult> {
    let call = call.trim().to_ascii_uppercase();
    let mut result = LookupResult { worked: WorkedBefore::default(), station: None, source: None, error: None };
    match qrz_lookup(&s, &call).await {
        Ok(Some((fields, source))) => {
            result.station = Some(fields);
            result.source = Some(source);
        }
        Ok(None) => {}
        Err(e) => result.error = Some(e),
    }
    let dxcc = result.station.as_ref().and_then(|f| f.get("DXCC")).and_then(|d| d.parse().ok());
    result.worked = db(&s, move |st| st.worked_before(log_id, &call, dxcc)).await?;
    Ok(Json(result))
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
    db(&s, move |st| st.import_adif(id, &body, &opts)).await.map(Json)
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
