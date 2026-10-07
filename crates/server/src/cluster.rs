//! DX cluster: one telnet connection (with failover through the user's node
//! list), spots flagged against the open log, and a console.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Weak};

use qrzero_core::band::band_for_freq;
use qrzero_core::cty::Entity;
use qrzero_core::worked::Needed;
use qrzero_radio::cluster::{self, ClusterEvent, ClusterHandle, ClusterNode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinHandle;

use crate::station::Hub;

const MAX_SPOTS: usize = 500;
const MAX_LINES: usize = 300;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClusterConfig {
    pub nodes: Vec<ClusterNode>,
    /// Connect when QRZero starts.
    pub auto_connect: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SpotInfo {
    pub seq: u64,
    /// Unix seconds when QRZero received it.
    pub received: i64,
    pub time: String,
    pub spotter: String,
    pub call: String,
    pub freq_hz: u64,
    pub band: Option<String>,
    pub mode: String,
    pub comment: String,
    pub entity: Option<Entity>,
    pub needed: Option<Needed>,
}

#[derive(Default)]
struct Inner {
    config: ClusterConfig,
    handle: Option<ClusterHandle>,
    task: Option<JoinHandle<()>>,
    /// "disconnected", "connecting to X", "connected to X".
    state: String,
    spots: VecDeque<SpotInfo>,
    lines: VecDeque<String>,
    seq: u64,
}

pub struct Cluster {
    hub: Weak<Hub>,
    inner: Mutex<Inner>,
}

impl Cluster {
    pub fn new(hub: &Arc<Hub>) -> Arc<Self> {
        let config: ClusterConfig = hub
            .setting("cluster")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let auto = config.auto_connect && !config.nodes.is_empty();
        let c = Arc::new(Cluster {
            hub: Arc::downgrade(hub),
            inner: Mutex::new(Inner { config, state: "disconnected".into(), ..Inner::default() }),
        });
        if auto {
            c.connect();
        }
        c
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let inner = self.lock();
        json!({
            "config": inner.config,
            "state": inner.state,
            "connected": inner.handle.is_some(),
            "spots": inner.spots,
            "lines": inner.lines,
        })
    }

    pub fn save_config(self: &Arc<Self>, config: ClusterConfig) -> Result<(), String> {
        let hub = self.hub.upgrade().ok_or("shutting down")?;
        hub.set_setting("cluster", &serde_json::to_string(&config).map_err(|e| e.to_string())?)?;
        let reconnect = {
            let mut inner = self.lock();
            let changed = inner.config.nodes != config.nodes;
            inner.config = config;
            changed && inner.handle.is_some()
        };
        if reconnect {
            self.connect();
        }
        Ok(())
    }

    pub fn connect(self: &Arc<Self>) {
        self.disconnect();
        let Some(hub) = self.hub.upgrade() else { return };
        let call = hub.active().station_callsign;
        let mut inner = self.lock();
        if inner.config.nodes.is_empty() || call.is_empty() {
            inner.state = if call.is_empty() { "add your callsign first".into() } else { "add a cluster node first".into() };
            return;
        }
        let handle = cluster::connect(inner.config.nodes.clone(), call);
        let rx = handle.subscribe();
        inner.task = Some(tokio::spawn(follow(Arc::downgrade(self), rx)));
        // The first status may have gone out before we subscribed.
        let first = handle.status();
        inner.handle = Some(handle);
        inner.state = "connecting".into();
        drop(inner);
        match first {
            Some(e) => self.on_event(e),
            None => self.emit_state(),
        }
    }

    pub fn disconnect(&self) {
        let mut inner = self.lock();
        if let Some(t) = inner.task.take() {
            t.abort();
        }
        if inner.handle.take().is_some() {
            inner.state = "disconnected".into();
            drop(inner);
            self.emit_state();
        }
    }

    pub fn send(&self, line: &str) -> Result<(), String> {
        let inner = self.lock();
        let handle = inner.handle.as_ref().ok_or("not connected to a cluster")?;
        handle.send(line);
        Ok(())
    }

    fn emit_state(&self) {
        let (state, connected) = {
            let inner = self.lock();
            (inner.state.clone(), inner.handle.is_some())
        };
        if let Some(hub) = self.hub.upgrade() {
            hub.emit(json!({"type": "cluster_state", "state": state, "connected": connected}));
        }
    }

    fn on_event(&self, e: ClusterEvent) {
        let Some(hub) = self.hub.upgrade() else { return };
        match e {
            ClusterEvent::Connecting { node } => {
                self.lock().state = format!("connecting to {node}");
                self.emit_state();
            }
            ClusterEvent::Connected { node } => {
                self.lock().state = format!("connected to {node}");
                self.emit_state();
            }
            ClusterEvent::Disconnected { node, reason } => {
                self.lock().state = format!("lost {node}: {reason}");
                self.emit_state();
            }
            ClusterEvent::Line { text } => {
                let mut inner = self.lock();
                inner.lines.push_back(text.clone());
                while inner.lines.len() > MAX_LINES {
                    inner.lines.pop_front();
                }
                drop(inner);
                hub.emit(json!({"type": "cluster_line", "text": text}));
            }
            ClusterEvent::Spot(s) => {
                let freq_hz = (s.freq_khz * 1000.0).round() as u64;
                let band = band_for_freq(s.freq_khz / 1000.0).map(str::to_string);
                let mode = cluster::guess_mode(s.freq_khz, &s.comment).to_string();
                let entity = hub.entity(&s.call);
                let needed = hub.needed(&s.call, entity.as_ref().and_then(|e| e.dxcc), band.as_deref(), Some(mode.as_str()).filter(|m| !m.is_empty()));
                let spot = {
                    let mut inner = self.lock();
                    inner.seq += 1;
                    let spot = SpotInfo {
                        seq: inner.seq,
                        received: chrono::Utc::now().timestamp(),
                        time: s.time,
                        spotter: s.spotter,
                        call: s.call,
                        freq_hz,
                        band,
                        mode,
                        comment: s.comment,
                        entity,
                        needed,
                    };
                    inner.spots.push_back(spot.clone());
                    while inner.spots.len() > MAX_SPOTS {
                        inner.spots.pop_front();
                    }
                    spot
                };
                hub.emit(json!({"type": "spot", "spot": spot}));
            }
        }
    }
}

async fn follow(cluster: Weak<Cluster>, mut rx: tokio::sync::broadcast::Receiver<ClusterEvent>) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match rx.recv().await {
            Ok(e) => match cluster.upgrade() {
                Some(c) => c.on_event(e),
                None => return,
            },
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return,
        }
    }
}
