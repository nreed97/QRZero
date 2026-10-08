//! Expert Electronics TCI: `cmd:arg,arg;` text over a WebSocket, with the server pushing changes.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tokio_tungstenite::tungstenite::{Bytes, Message};

use super::{CmdRx, ModeInfo, ModeReq, RigCommand, RigState, StateTx};

/// Upper bound on trx numbers we accept, so a garbled index cannot allocate a huge state vector.
const MAX_TRX: usize = 16;
const KEEPALIVE: Duration = Duration::from_secs(5);
const SILENCE_LIMIT: Duration = Duration::from_secs(15);

pub(super) async fn run(host: &str, port: u16, state: &StateTx, cmds: &mut CmdRx) -> Result<()> {
    let url = format!("ws://{host}:{port}");
    let (ws, _) = timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(url.as_str()))
        .await
        .map_err(|_| anyhow!("timed out"))
        .and_then(|r| r.map_err(Into::into))
        .with_context(|| format!("TCI {host}:{port}"))?;
    let (mut sink, mut stream) = ws.split();
    let mut chans = vec![connected()];
    state.send_replace(chans.clone());
    let mut keepalive = interval(KEEPALIVE);
    keepalive.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            msg = stream.next() => {
                let msg = msg.ok_or_else(|| anyhow!("TCI: connection closed"))?.context("TCI")?;
                heard = Instant::now();
                match msg {
                    Message::Text(text) => {
                        let before = chans.clone();
                        for m in parse(&text) {
                            apply(&mut chans, &m);
                        }
                        if chans != before {
                            state.send_replace(chans.clone());
                        }
                    }
                    Message::Close(_) => bail!("TCI: server closed the connection"),
                    _ => {}
                }
            }
            cmd = cmds.recv() => {
                let Some((ch, cmd)) = cmd else { return Ok(()) };
                let freq = chans.get(ch).map_or(0, |c| c.freq_hz);
                if let Some(text) = encode_command(ch, &cmd, freq) {
                    sink.send(Message::text(text)).await.context("TCI")?;
                }
            }
            _ = keepalive.tick() => {
                if heard.elapsed() > SILENCE_LIMIT {
                    bail!("TCI: server stopped responding");
                }
                sink.send(Message::Ping(Bytes::new())).await.context("TCI")?;
            }
        }
    }
}

fn connected() -> RigState {
    RigState { connected: true, ..Default::default() }
}

/// One `name:args;` command; the name is lowercased (TCI names are case-insensitive).
#[derive(Debug, PartialEq)]
struct TciMsg {
    name: String,
    args: Vec<String>,
}

/// Splits a text frame, which may hold several commands, into its commands.
fn parse(frame: &str) -> Vec<TciMsg> {
    frame
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|cmd| {
            let (name, args) = cmd.split_once(':').unwrap_or((cmd, ""));
            let args = if args.is_empty() { Vec::new() } else { args.split(',').map(|a| a.trim().to_string()).collect() };
            TciMsg { name: name.trim().to_ascii_lowercase(), args }
        })
        .collect()
}

/// Applies one server message to the per-trx state.
fn apply(chans: &mut Vec<RigState>, msg: &TciMsg) {
    let arg = |i: usize| msg.args.get(i).map(String::as_str);
    let trx = || arg(0).and_then(|a| a.parse::<usize>().ok()).filter(|&t| t < MAX_TRX);
    match msg.name.as_str() {
        "trx_count" => {
            if let Some(n) = arg(0).and_then(|a| a.parse::<usize>().ok()) {
                chans.resize_with(n.clamp(1, MAX_TRX), connected);
            }
        }
        "vfo" if arg(1) == Some("1") => {
            if let (Some(t), Some(hz)) = (trx(), arg(2).and_then(|a| a.parse::<u64>().ok())) {
                chan(chans, t).tx_freq_hz = hz;
            }
        }
        "vfo" => {
            if let (Some(t), Some("0"), Some(hz)) = (trx(), arg(1), arg(2).and_then(|a| a.parse::<u64>().ok())) {
                chan(chans, t).freq_hz = hz;
            }
        }
        "split_enable" => {
            if let (Some(t), Some(on)) = (trx(), arg(1)) {
                chan(chans, t).split = on.eq_ignore_ascii_case("true");
            }
        }
        "modulation" => {
            if let (Some(t), Some(m)) = (trx(), arg(1)) {
                decode_mode(m).apply(chan(chans, t));
            }
        }
        "trx" => {
            if let (Some(t), Some(on)) = (trx(), arg(1)) {
                chan(chans, t).tx = on.eq_ignore_ascii_case("true");
            }
        }
        _ => {}
    }
}

/// The state of `trx`, growing the list when the server mentions a trx before `trx_count`.
fn chan(chans: &mut Vec<RigState>, trx: usize) -> &mut RigState {
    if chans.len() <= trx {
        chans.resize_with(trx + 1, connected);
    }
    &mut chans[trx]
}

fn decode_mode(m: &str) -> ModeInfo {
    let m = m.to_ascii_lowercase();
    let upper = m.to_ascii_uppercase();
    match m.as_str() {
        "lsb" | "usb" => ModeInfo::new("SSB", upper),
        "cw" => ModeInfo::new("CW", upper),
        "am" | "sam" | "dsb" => ModeInfo::new("AM", upper),
        "nfm" | "wfm" => ModeInfo::new("FM", upper),
        "digl" | "digu" => ModeInfo::data(upper),
        _ => ModeInfo::new("", upper),
    }
}

fn encode_mode(req: ModeReq) -> &'static str {
    match req {
        ModeReq::Lsb => "lsb",
        ModeReq::Usb => "usb",
        ModeReq::Cw => "cw",
        ModeReq::Am => "am",
        ModeReq::Fm => "nfm",
        // TCI has no FSK mode; AFSK RTTY conventionally runs on LSB.
        ModeReq::Rtty => "digl",
        ModeReq::Data => "digu",
    }
}

/// The text frame for a command on `trx`, whose current frequency picks the SSB sideband.
fn encode_command(trx: usize, cmd: &RigCommand, freq_hz: u64) -> Option<String> {
    match cmd {
        RigCommand::SetFreq(hz) => Some(format!("vfo:{trx},0,{hz};")),
        // The transmit frequency lives on VFO B, which split switches transmit to.
        RigCommand::SetSplit(Some(hz)) => Some(format!("vfo:{trx},1,{hz};split_enable:{trx},true;")),
        RigCommand::SetSplit(None) => Some(format!("split_enable:{trx},false;")),
        RigCommand::SetMode(m) => {
            ModeReq::from_adif(m, freq_hz).map(|req| format!("modulation:{trx},{};", encode_mode(req)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{spawn, RigConfig};
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[test]
    fn parse_frames() {
        let msgs = parse("trx_count:2;VFO:0,0,14074000; ready;\n");
        assert_eq!(
            msgs,
            vec![
                TciMsg { name: "trx_count".into(), args: vec!["2".into()] },
                TciMsg { name: "vfo".into(), args: vec!["0".into(), "0".into(), "14074000".into()] },
                TciMsg { name: "ready".into(), args: vec![] },
            ]
        );
        assert!(parse(";;").is_empty());
    }

    #[test]
    fn apply_burst() {
        let mut chans = vec![connected()];
        for m in parse("trx_count:2;vfo:0,0,14074000;vfo:0,1,14080000;vfo:1,0,7030000;modulation:0,digu;modulation:1,CW;trx:1,true;") {
            apply(&mut chans, &m);
        }
        assert_eq!(chans.len(), 2);
        assert_eq!(chans[0].freq_hz, 14_074_000);
        assert_eq!((chans[0].mode.as_str(), chans[0].rig_mode.as_str(), chans[0].data), ("", "DIGU", true));
        assert_eq!((chans[1].freq_hz, chans[1].mode.as_str(), chans[1].tx), (7_030_000, "CW", true));
        assert!(!chans[0].tx);
        assert_eq!((chans[0].split, chans[0].tx_freq_hz), (false, 14_080_000));
        apply(&mut chans, &parse("split_enable:0,true;")[0]);
        assert!(chans[0].split);
        // Unannounced trx grows the list; garbage indexes are ignored.
        apply(&mut chans, &parse("vfo:2,0,3500000;vfo:99,0,1;vfo:x,0,1;")[0]);
        assert_eq!(chans.len(), 3);
    }

    #[test]
    fn modes() {
        assert_eq!(decode_mode("usb"), ModeInfo::new("SSB", "USB"));
        assert_eq!(decode_mode("nfm"), ModeInfo::new("FM", "NFM"));
        assert_eq!(decode_mode("sam"), ModeInfo::new("AM", "SAM"));
        assert_eq!(decode_mode("digl"), ModeInfo::data("DIGL"));
        assert_eq!(decode_mode("drm"), ModeInfo::new("", "DRM"));
        assert_eq!(encode_command(1, &RigCommand::SetFreq(7_074_000), 0).unwrap(), "vfo:1,0,7074000;");
        let set = |m: &str, f| encode_command(0, &RigCommand::SetMode(m.into()), f);
        assert_eq!(set("SSB", 7_100_000).unwrap(), "modulation:0,lsb;");
        assert_eq!(set("SSB", 14_200_000).unwrap(), "modulation:0,usb;");
        assert_eq!(set("FT8", 7_074_000).unwrap(), "modulation:0,digu;");
        assert_eq!(set("FM", 145_000_000).unwrap(), "modulation:0,nfm;");
        assert_eq!(set("", 0), None);
        assert_eq!(encode_command(0, &RigCommand::SetSplit(Some(14_205_000)), 0).unwrap(), "vfo:0,1,14205000;split_enable:0,true;");
        assert_eq!(encode_command(1, &RigCommand::SetSplit(None), 0).unwrap(), "split_enable:1,false;");
    }

    #[tokio::test]
    async fn end_to_end() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let log2 = log.clone();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            ws.send(Message::text("protocol:ExpertSDR3,1.9;device:SunSDR2PRO;trx_count:2;")).await.unwrap();
            ws.send(Message::text("vfo:0,0,14074000;vfo:0,1,14075000;vfo:1,0,7030000;modulation:0,digu;modulation:1,cw;trx:0,false;trx:1,false;ready;"))
                .await
                .unwrap();
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(t) = msg {
                    log2.lock().unwrap().push(t.to_string());
                    // ExpertSDR echoes accepted settings back to all clients.
                    ws.send(Message::text(t.to_string())).await.unwrap();
                }
            }
        });

        let handle = spawn(RigConfig::Tci { host: "127.0.0.1".into(), port });
        let rx = handle.subscribe();
        let wait = |pred: fn(&[RigState]) -> bool| {
            let mut rx = rx.clone();
            async move { timeout(Duration::from_secs(5), rx.wait_for(|s| pred(s))).await.unwrap().unwrap().clone() }
        };
        let st = wait(|s| s.len() == 2 && s[1].mode == "CW").await;
        assert!(st.iter().all(|c| c.connected && c.error.is_none()));
        assert_eq!((st[0].freq_hz, st[0].data), (14_074_000, true));
        assert_eq!(st[1].freq_hz, 7_030_000);

        handle.send(1, RigCommand::SetFreq(7_074_000));
        handle.send(1, RigCommand::SetMode("FT8".into()));
        handle.send(0, RigCommand::SetMode("SSB".into()));
        let st = wait(|s| s[1].freq_hz == 7_074_000 && s[1].data && s[0].rig_mode == "USB").await;
        assert_eq!(st[0].mode, "SSB");
        assert_eq!(*log.lock().unwrap(), ["vfo:1,0,7074000;", "modulation:1,digu;", "modulation:0,usb;"]);

        // Split: VFO B becomes the transmit frequency, and the echo shows it on.
        assert!(!st[0].split);
        handle.send(0, RigCommand::SetSplit(Some(14_079_000)));
        let st = wait(|s| s[0].split && s[0].tx_freq_hz == 14_079_000).await;
        assert_eq!(st[0].freq_hz, 14_074_000);
        handle.send(0, RigCommand::SetSplit(None));
        let st = wait(|s| !s[0].split).await;
        assert_eq!(st[0].tx_freq_hz, 14_079_000);
    }
}
