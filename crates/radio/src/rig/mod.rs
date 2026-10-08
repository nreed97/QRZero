//! Rig control: reads frequency, mode and PTT from a transceiver and tunes it.
//!
//! [`spawn`] runs one backend in a background task that polls the radio, publishes [`RigState`]
//! on a watch channel and reconnects after failures. Each protocol keeps its wire format in pure
//! functions so it can be tested without hardware.

mod hamlib;
mod icom;
mod kenwood;
mod serial;
mod tci;
mod yaesu;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// How the rig is reached.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RigConfig {
    /// Hamlib's rigctld network daemon (default port 4532).
    Hamlib { host: String, port: u16 },
    /// Expert Electronics TCI over WebSocket (default port 40001). Also used for Flex via TCI bridges; every TCI transceiver (trx) is one channel, so a Flex with several slices shows several channels.
    Tci { host: String, port: u16 },
    /// Kenwood-style ASCII CAT over serial: Kenwood, Elecraft K3/K4/KX, FlexRadio SmartSDR CAT ports, TS-2000 compatible.
    Kenwood { port: String, baud: u32 },
    /// Newer Yaesu ASCII CAT (FT-991A, FTDX10/101, FT-710): FA/MD commands with Yaesu mode codes.
    Yaesu { port: String, baud: u32 },
    /// Icom CI-V over serial (civ_addr is the radio's CI-V address, e.g. 0x94 for IC-7300).
    Icom { port: String, baud: u32, civ_addr: u8 },
}

/// What one channel of the rig is doing.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct RigState {
    pub connected: bool,
    pub freq_hz: u64,
    /// ADIF mode: "CW", "SSB", "AM", "FM", "RTTY", or "" when the rig is in a data mode / unknown.
    pub mode: String,
    /// The rig's own mode name, e.g. "USB", "CW-R", "DIGU", "PKTUSB".
    pub rig_mode: String,
    /// True for data modes (DIGU/DIGL/PKTUSB/USB-D/DATA etc.), where the logger keeps its own digital mode.
    pub data: bool,
    pub tx: bool,
    /// The radio is in split: it receives on `freq_hz` and transmits on `tx_freq_hz`.
    pub split: bool,
    /// The transmit VFO's frequency in Hz; 0 when the backend doesn't report it. Only meaningful while `split`.
    pub tx_freq_hz: u64,
    /// Last connection error, shown to the user. None when fine.
    pub error: Option<String>,
}

/// A request to change the rig.
#[derive(Clone, Debug, PartialEq)]
pub enum RigCommand {
    SetFreq(u64),
    /// ADIF mode, e.g. "CW", "SSB", "AM", "FM", "RTTY", "FT8", "FT4", "PSK31". SSB picks LSB below 10 MHz except 60 m (5.3-5.4 MHz) which is USB. Digital modes pick the rig's USB data mode (DIGU, PKTUSB, USB-D / data on, etc.).
    SetMode(String),
    /// Transmit on this frequency and turn split on, or turn split off (`None`). Only TCI supports it so far.
    SetSplit(Option<u64>),
}

/// Handle to a running rig backend. Dropping it stops the backend.
pub struct RigHandle {
    state: watch::Receiver<Vec<RigState>>,
    cmds: mpsc::UnboundedSender<(usize, RigCommand)>,
    task: JoinHandle<()>,
}

/// Starts a background tokio task that connects, polls (~4 times a second), and reconnects every 5 s after a failure. Dropping the handle stops the task.
pub fn spawn(config: RigConfig) -> RigHandle {
    let (state_tx, state) = watch::channel(vec![RigState::default()]);
    let (cmds, cmd_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(supervise(config, Arc::new(state_tx), cmd_rx));
    RigHandle { state, cmds, task }
}

impl RigHandle {
    /// One entry per channel (TCI: per trx; everything else: exactly one). Before the first connect, one disconnected default entry (with error None).
    pub fn subscribe(&self) -> watch::Receiver<Vec<RigState>> {
        self.state.clone()
    }

    /// Queue a command for a channel. Returns immediately; errors are reflected in state.error.
    pub fn send(&self, channel: usize, cmd: RigCommand) {
        let _ = self.cmds.send((channel, cmd));
    }
}

impl Drop for RigHandle {
    fn drop(&mut self) {
        // Serial backends run on a blocking thread; they stop when they see the command channel close.
        self.task.abort();
    }
}

type StateTx = Arc<watch::Sender<Vec<RigState>>>;
type CmdRx = mpsc::UnboundedReceiver<(usize, RigCommand)>;

const POLL: Duration = Duration::from_millis(250);
const RECONNECT: Duration = Duration::from_secs(5);
/// How long a failed command's error stays visible.
const CMD_ERROR_TTL: Duration = Duration::from_secs(10);

/// Runs the backend until the handle is dropped. Backends return `Ok` only on shutdown.
async fn supervise(config: RigConfig, state: StateTx, mut cmds: CmdRx) {
    loop {
        let result = match &config {
            RigConfig::Hamlib { host, port } => hamlib::run(host, *port, &state, &mut cmds).await,
            RigConfig::Tci { host, port } => tci::run(host, *port, &state, &mut cmds).await,
            RigConfig::Kenwood { port, baud } => {
                let proto = kenwood::Kenwood::default();
                serial::spawn(port.clone(), *baud, proto, &state, &mut cmds).await
            }
            RigConfig::Yaesu { port, baud } => {
                serial::spawn(port.clone(), *baud, yaesu::Yaesu::default(), &state, &mut cmds).await
            }
            RigConfig::Icom { port, baud, civ_addr } => {
                let proto = icom::Icom::new(*civ_addr);
                serial::spawn(port.clone(), *baud, proto, &state, &mut cmds).await
            }
        };
        let Err(err) = result else { return };
        let msg = format!("{err:#}");
        tracing::warn!("rig: {msg}");
        state.send_modify(|chans| {
            for ch in chans {
                ch.connected = false;
                ch.tx = false;
                ch.error = Some(msg.clone());
            }
        });
        // Commands sent while disconnected are dropped rather than replayed later.
        let wait = tokio::time::sleep(RECONNECT);
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                cmd = cmds.recv() => if cmd.is_none() { return },
            }
        }
    }
}

/// Publishes the state of a single-channel rig, notifying only on change.
fn publish_single(state: &watch::Sender<Vec<RigState>>, new: RigState) {
    state.send_if_modified(|chans| {
        if chans.len() == 1 && chans[0] == new {
            return false;
        }
        *chans = vec![new];
        true
    });
}

/// The error of the last failed command, shown for [`CMD_ERROR_TTL`] or until a command succeeds.
#[derive(Default)]
struct CmdError(Option<(String, Instant)>);

impl CmdError {
    fn record(&mut self, result: anyhow::Result<()>) {
        self.0 = result.err().map(|e| (format!("{e:#}"), Instant::now()));
    }

    fn get(&mut self) -> Option<String> {
        if self.0.as_ref().is_some_and(|(_, at)| at.elapsed() > CMD_ERROR_TTL) {
            self.0 = None;
        }
        self.0.as_ref().map(|(msg, _)| msg.clone())
    }
}

/// The rig mode a [`RigCommand::SetMode`] asks for, independent of protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeReq {
    Lsb,
    Usb,
    Cw,
    Am,
    Fm,
    Rtty,
    /// USB with the rig's data/packet setting (DIGU, PKTUSB, USB-D).
    Data,
}

impl ModeReq {
    /// Maps an ADIF mode to a rig mode; `freq_hz` picks the SSB sideband (0 = unknown, USB).
    fn from_adif(adif: &str, freq_hz: u64) -> Option<ModeReq> {
        Some(match adif.trim().to_ascii_uppercase().as_str() {
            "" => return None,
            "SSB" if lsb_band(freq_hz) => ModeReq::Lsb,
            "SSB" | "USB" => ModeReq::Usb,
            "LSB" => ModeReq::Lsb,
            "CW" => ModeReq::Cw,
            "AM" => ModeReq::Am,
            "FM" => ModeReq::Fm,
            "RTTY" => ModeReq::Rtty,
            _ => ModeReq::Data,
        })
    }
}

/// Conventional LSB below 10 MHz, except on 60 m where channels are USB.
fn lsb_band(freq_hz: u64) -> bool {
    freq_hz != 0 && freq_hz < 10_000_000 && !(5_250_000..=5_450_000).contains(&freq_hz)
}

/// A decoded rig mode in [`RigState`] terms.
#[derive(Clone, Debug, PartialEq)]
struct ModeInfo {
    adif: &'static str,
    rig_mode: String,
    data: bool,
}

impl ModeInfo {
    fn new(adif: &'static str, rig_mode: impl Into<String>) -> Self {
        ModeInfo { adif, rig_mode: rig_mode.into(), data: false }
    }

    fn data(rig_mode: impl Into<String>) -> Self {
        ModeInfo { adif: "", rig_mode: rig_mode.into(), data: true }
    }

    fn apply(self, st: &mut RigState) {
        st.mode = self.adif.to_string();
        st.rig_mode = self.rig_mode;
        st.data = self.data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adif_to_mode_request() {
        use ModeReq::*;
        let cases = [
            ("SSB", 7_150_000, Some(Lsb)),
            ("SSB", 3_700_000, Some(Lsb)),
            ("SSB", 5_357_000, Some(Usb)),
            ("SSB", 14_200_000, Some(Usb)),
            ("SSB", 0, Some(Usb)),
            ("ssb", 1_840_000, Some(Lsb)),
            ("USB", 7_100_000, Some(Usb)),
            ("LSB", 14_100_000, Some(Lsb)),
            ("CW", 7_000_000, Some(Cw)),
            ("AM", 7_000_000, Some(Am)),
            ("FM", 145_000_000, Some(Fm)),
            ("RTTY", 14_080_000, Some(Rtty)),
            ("FT8", 7_074_000, Some(Data)),
            ("FT4", 14_080_000, Some(Data)),
            ("PSK31", 14_070_000, Some(Data)),
            ("", 14_070_000, None),
        ];
        for (adif, f, want) in cases {
            assert_eq!(ModeReq::from_adif(adif, f), want, "{adif} @ {f}");
        }
    }

    #[test]
    fn cmd_error_clears_on_success() {
        let mut e = CmdError::default();
        e.record(Err(anyhow::anyhow!("nope")));
        assert_eq!(e.get().as_deref(), Some("nope"));
        e.record(Ok(()));
        assert_eq!(e.get(), None);
    }

    #[tokio::test]
    async fn initial_state_and_reconnect_error() {
        // Nothing listens on this port, so the first connect fails.
        let port = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();
        let handle = spawn(RigConfig::Hamlib { host: "127.0.0.1".into(), port });
        let mut rx = handle.subscribe();
        assert_eq!(*rx.borrow(), vec![RigState::default()]);
        let st = tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|s| s[0].error.is_some()))
            .await
            .unwrap()
            .unwrap()
            .clone();
        assert!(!st[0].connected);
    }

    #[test]
    fn config_serde() {
        let cfg = RigConfig::Icom { port: "COM3".into(), baud: 19200, civ_addr: 0x94 };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, r#"{"kind":"icom","port":"COM3","baud":19200,"civ_addr":148}"#);
        assert_eq!(serde_json::from_str::<RigConfig>(&json).unwrap(), cfg);
    }
}
