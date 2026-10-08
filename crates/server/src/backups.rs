//! Backups and restores of the log database (Settings, Backups).
//!
//! The file work lives in `qrzero_core::backup`; this is the API over it, the
//! startup restore and the daily automatic backup.

use std::path::{Path as FsPath, PathBuf};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use qrzero_core::backup::{self, Backup, Kind, Pending};
use qrzero_core::Store;
use serde::{Deserialize, Serialize};

use super::{db, ApiError, ApiResult, Shared};

const DEFAULT_KEEP: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct BackupSettings {
    /// Back up when QRZero starts, at most once a day.
    auto: bool,
    /// Automatic backups to keep.
    keep: usize,
}

/// What happened to a restore at the last start.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RestoreResult {
    ok: bool,
    message: String,
}

pub(crate) fn load_settings(st: &Store) -> qrzero_core::Result<BackupSettings> {
    Ok(BackupSettings {
        auto: st.get_setting("backup.auto")?.as_deref() != Some("0"),
        keep: st.get_setting("backup.keep")?.and_then(|k| k.parse().ok()).unwrap_or(DEFAULT_KEEP).clamp(1, 999),
    })
}

/// Swaps in a staged restore, then opens the log. A restored log that won't
/// open is replaced by the one from before the restore.
pub(crate) fn open_store(data_dir: &FsPath) -> anyhow::Result<(Store, Option<RestoreResult>)> {
    let path = backup::db_path(data_dir);
    let applied = match backup::apply_pending(data_dir) {
        Ok(Some(p)) => Some(p),
        Ok(None) => None,
        Err(e) => {
            tracing::error!("restore failed: {e}");
            let result = RestoreResult { ok: false, message: format!("The restore didn't happen: {e}") };
            return Ok((Store::open(&path)?, Some(result)));
        }
    };
    let Some(p) = applied else {
        return Ok((Store::open(&path)?, None));
    };
    match Store::open(&path) {
        Ok(store) => {
            let message = format!("Restored from {} ({} QSOs). The log as it was before is kept as a \"before restore\" backup.", p.source, p.summary.qsos);
            tracing::info!("{message}");
            Ok((store, Some(RestoreResult { ok: true, message })))
        }
        Err(e) => {
            tracing::error!("restored log won't open: {e}");
            let name = backup::roll_back(data_dir)?;
            let message = format!("The restored log wouldn't open ({e}), so the log from before the restore ({name}) was put back.");
            Ok((Store::open(&path)?, Some(RestoreResult { ok: false, message })))
        }
    }
}

/// The daily automatic backup, then pruning. Runs in the background at startup.
pub(crate) fn auto_backup(data_dir: &FsPath, settings: BackupSettings) {
    let run = || -> qrzero_core::Result<()> {
        if settings.auto && backup::auto_due(data_dir, chrono::Local::now().date_naive())? {
            let b = backup::create(data_dir, Kind::Auto)?;
            tracing::info!("automatic backup {}", b.name);
        }
        backup::prune(data_dir, settings.keep)?;
        Ok(())
    };
    if let Err(e) = run() {
        tracing::error!("automatic backup failed: {e}");
    }
}

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/backups", get(overview).post(create))
        .route("/backups/settings", axum::routing::put(put_settings))
        .route("/backups/files/{name}", get(download).delete(delete))
        .route("/backups/files/{name}/restore", post(restore_listed))
        .route(
            "/backups/restore",
            post(restore_upload).layer(DefaultBodyLimit::max(2048 * 1024 * 1024)),
        )
        .route("/backups/pending", axum::routing::delete(cancel))
}

#[derive(Serialize)]
struct Overview {
    folder: String,
    backups: Vec<Backup>,
    settings: BackupSettings,
    pending: Option<Pending>,
    last_restore: Option<RestoreResult>,
}

/// Runs file work off the async threads.
async fn files<T, F>(f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> qrzero_core::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(ApiError::from)
}

async fn overview(State(s): State<Shared>) -> ApiResult<Overview> {
    let settings = db(&s, |st| load_settings(st)).await?;
    let dir = s.data_dir.clone();
    let (backups, pending) = files(move || Ok((backup::list(&dir)?, backup::pending(&dir)))).await?;
    Ok(Json(Overview {
        folder: backup::backup_dir(&s.data_dir).display().to_string(),
        backups,
        settings,
        pending,
        last_restore: s.last_restore.clone(),
    }))
}

async fn put_settings(State(s): State<Shared>, Json(b): Json<BackupSettings>) -> ApiResult<Overview> {
    let keep = b.keep.clamp(1, 999);
    db(&s, move |st| {
        st.set_setting("backup.auto", if b.auto { "1" } else { "0" })?;
        st.set_setting("backup.keep", &keep.to_string())
    })
    .await?;
    let dir = s.data_dir.clone();
    files(move || backup::prune(&dir, keep)).await?;
    overview(State(s)).await
}

async fn create(State(s): State<Shared>) -> ApiResult<Backup> {
    let dir = s.data_dir.clone();
    files(move || backup::create(&dir, Kind::Manual)).await.map(Json)
}

async fn delete(State(s): State<Shared>, Path(name): Path<String>) -> ApiResult<()> {
    let dir = s.data_dir.clone();
    files(move || backup::delete(&dir, &name)).await.map(Json)
}

async fn download(State(s): State<Shared>, Path(name): Path<String>) -> Result<Response, ApiError> {
    use tokio::io::AsyncReadExt;
    let dir = s.data_dir.clone();
    let n = name.clone();
    let path: PathBuf = files(move || backup::file_path(&dir, &n)).await?;
    let file = tokio::fs::File::open(&path).await.map_err(|e| ApiError::from(qrzero_core::Error::from(e)))?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let stream = futures_util::stream::unfold(file, |mut file| async move {
        let mut buf = vec![0u8; 256 * 1024];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok::<_, std::io::Error>(Bytes::from(buf)), file))
            }
            Err(e) => Some((Err(e), file)),
        }
    });
    let safe: String = name.chars().map(|c| if c.is_ascii_graphic() && c != '"' { c } else { '_' }).collect();
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{safe}\"")).unwrap_or(HeaderValue::from_static("attachment"));
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/vnd.sqlite3")),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, HeaderValue::from(len)),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn restore_listed(State(s): State<Shared>, Path(name): Path<String>) -> ApiResult<Pending> {
    let dir = s.data_dir.clone();
    files(move || backup::stage_backup(&dir, &name)).await.map(Json)
}

#[derive(Deserialize)]
struct UploadQuery {
    #[serde(default)]
    name: String,
}

async fn restore_upload(State(s): State<Shared>, Query(q): Query<UploadQuery>, body: Bytes) -> ApiResult<Pending> {
    let dir = s.data_dir.clone();
    let source: String = q.name.chars().filter(|c| !c.is_control()).take(120).collect();
    let source = if source.trim().is_empty() { "an uploaded file".to_string() } else { source };
    files(move || backup::stage_file(&dir, &source, &body)).await.map(Json)
}

async fn cancel(State(s): State<Shared>) -> ApiResult<()> {
    let dir = s.data_dir.clone();
    files(move || backup::cancel(&dir)).await.map(Json)
}
