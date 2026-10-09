//! Hamlib rigctld: a line-based text protocol over TCP.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::{interval, timeout, MissedTickBehavior};

use super::{publish_single, CmdError, CmdRx, ModeInfo, ModeReq, RigCommand, RigState, StateTx, POLL};

const TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn run(host: &str, port: u16, state: &StateTx, cmds: &mut CmdRx) -> Result<()> {
    let stream = timeout(TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| anyhow!("timed out"))
        .and_then(|r| r.map_err(Into::into))
        .with_context(|| format!("rigctld {host}:{port}"))?;
    stream.set_nodelay(true)?;
    let (r, w) = stream.into_split();
    let mut conn = Conn { lines: BufReader::new(r).lines(), w };
    let mut cmd_err = CmdError::default();
    let mut last = RigState::default();
    let mut tick = interval(POLL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                last = conn.poll().await.context("rigctld")?;
                last.error = cmd_err.get();
                publish_single(state, last.clone());
            }
            cmd = cmds.recv() => {
                let Some((ch, cmd)) = cmd else { return Ok(()) };
                if ch == 0 {
                    cmd_err.record(conn.command(&cmd, last.freq_hz).await);
                    tick.reset_immediately();
                }
            }
        }
    }
}

struct Conn {
    lines: Lines<BufReader<OwnedReadHalf>>,
    w: OwnedWriteHalf,
}

impl Conn {
    /// Sends a command and reads `n` reply lines; `Ok(Err(code))` when rigctld answers `RPRT code`.
    async fn query(&mut self, cmd: &str, n: usize) -> Result<std::result::Result<Vec<String>, i32>> {
        self.w.write_all(format!("{cmd}\n").as_bytes()).await?;
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let line = timeout(TIMEOUT, self.lines.next_line())
                .await
                .map_err(|_| anyhow!("no reply to '{cmd}'"))??
                .ok_or_else(|| anyhow!("connection closed"))?;
            if let Some(code) = parse_rprt(&line) {
                return Ok(Err(code));
            }
            out.push(line);
        }
        Ok(Ok(out))
    }

    async fn poll(&mut self) -> Result<RigState> {
        let mut st = RigState { connected: true, ..Default::default() };
        let f = self.query("f", 1).await?.map_err(|c| anyhow!("get freq failed (RPRT {c})"))?;
        st.freq_hz = parse_freq(&f[0]).ok_or_else(|| anyhow!("bad frequency '{}'", f[0]))?;
        if let Ok(m) = self.query("m", 2).await? {
            decode_mode(&m[0]).apply(&mut st);
        }
        // Not every rig reports PTT; treat a refusal as receive.
        if let Ok(t) = self.query("t", 1).await? {
            st.tx = t[0].trim() == "1";
        }
        // Split is optional too: a rig that refuses `s` is shown as not in split.
        if let Ok(sp) = self.query("s", 2).await? {
            if sp[0].trim() == "1" {
                // The transmit frequency; if the rig won't say, leave it 0 and the UI shows one frequency.
                if let Ok(i) = self.query("i", 1).await? {
                    st.tx_freq_hz = parse_freq(&i[0]).unwrap_or(0);
                }
                st.split = st.tx_freq_hz > 0;
            }
        }
        Ok(st)
    }

    async fn command(&mut self, cmd: &RigCommand, freq_hz: u64) -> Result<()> {
        let line = match cmd {
            RigCommand::SetFreq(hz) => format!("F {hz}"),
            RigCommand::SetMode(m) => {
                let Some(req) = ModeReq::from_adif(m, freq_hz) else { return Ok(()) };
                format!("M {} 0", encode_mode(req))
            }
            RigCommand::SetSplit(tx) => {
                // Split on with VFO B transmitting, then its frequency; or split off with VFO A transmitting.
                let on = if tx.is_some() { "S 1 VFOB" } else { "S 0 VFOA" };
                self.set(on).await?;
                return match tx {
                    Some(hz) => self.set(&format!("I {hz}")).await,
                    None => Ok(()),
                };
            }
        };
        self.set(&line).await
    }

    async fn set(&mut self, line: &str) -> Result<()> {
        match self.query(line, 1).await? {
            Err(0) => Ok(()),
            Err(code) => bail!("rigctld refused '{line}' (RPRT {code})"),
            Ok(other) => bail!("rigctld: unexpected reply '{}' to '{line}'", other[0]),
        }
    }
}

fn parse_rprt(line: &str) -> Option<i32> {
    line.trim().strip_prefix("RPRT ")?.trim().parse().ok()
}

/// rigctld prints the frequency as an integer, some versions with a trailing ".000000".
fn parse_freq(line: &str) -> Option<u64> {
    let f: f64 = line.trim().parse().ok()?;
    (f >= 0.0).then(|| f.round() as u64)
}

fn decode_mode(m: &str) -> ModeInfo {
    let m = m.trim();
    match m {
        "USB" | "LSB" | "ECSSUSB" | "ECSSLSB" => ModeInfo::new("SSB", m),
        "CW" | "CWR" => ModeInfo::new("CW", m),
        "RTTY" | "RTTYR" => ModeInfo::new("RTTY", m),
        "AM" | "SAM" | "AMS" | "DSB" | "SAL" | "SAH" => ModeInfo::new("AM", m),
        "FM" | "WFM" | "FMN" => ModeInfo::new("FM", m),
        _ if m.starts_with("PKT") => ModeInfo::data(m),
        _ => ModeInfo::new("", m),
    }
}

fn encode_mode(req: ModeReq) -> &'static str {
    match req {
        ModeReq::Lsb => "LSB",
        ModeReq::Usb => "USB",
        ModeReq::Cw => "CW",
        ModeReq::Am => "AM",
        ModeReq::Fm => "FM",
        ModeReq::Rtty => "RTTY",
        ModeReq::Data => "PKTUSB",
    }
}

#[cfg(test)]
mod tests {
    use super::super::{spawn, RigConfig};
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[test]
    fn parsers() {
        assert_eq!(parse_rprt("RPRT 0"), Some(0));
        assert_eq!(parse_rprt("RPRT -11"), Some(-11));
        assert_eq!(parse_rprt("14074000"), None);
        assert_eq!(parse_freq("14074000"), Some(14_074_000));
        assert_eq!(parse_freq("7074000.000000"), Some(7_074_000));
        assert_eq!(parse_freq("USB"), None);
    }

    #[test]
    fn modes() {
        assert_eq!(decode_mode("USB"), ModeInfo::new("SSB", "USB"));
        assert_eq!(decode_mode("CWR"), ModeInfo::new("CW", "CWR"));
        assert_eq!(decode_mode("RTTYR"), ModeInfo::new("RTTY", "RTTYR"));
        assert_eq!(decode_mode("WFM"), ModeInfo::new("FM", "WFM"));
        assert_eq!(decode_mode("PKTUSB"), ModeInfo::data("PKTUSB"));
        assert_eq!(decode_mode("PKTFM"), ModeInfo::data("PKTFM"));
        assert_eq!(decode_mode("FAX"), ModeInfo::new("", "FAX"));
        assert_eq!(encode_mode(ModeReq::Data), "PKTUSB");
        assert_eq!(encode_mode(ModeReq::Lsb), "LSB");
    }

    /// A minimal rigctld that tracks freq/mode and logs every line it receives.
    async fn mock_rigctld() -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let (r, mut w) = sock.into_split();
            let mut lines = BufReader::new(r).lines();
            let (mut freq, mut mode) = (14_074_000u64, "PKTUSB".to_string());
            let (mut split, mut tx_freq) = (false, 0u64);
            while let Ok(Some(line)) = lines.next_line().await {
                log2.lock().unwrap().push(line.clone());
                let parts: Vec<&str> = line.split_whitespace().collect();
                let reply = match parts.as_slice() {
                    ["f"] => format!("{freq}\n"),
                    ["m"] => format!("{mode}\n3000\n"),
                    ["t"] => "RPRT -11\n".to_string(),
                    ["s"] => format!("{}\nVFOB\n", split as u8),
                    ["i"] => format!("{tx_freq}\n"),
                    ["S", on, _] => {
                        split = *on == "1";
                        "RPRT 0\n".into()
                    }
                    ["I", hz] => {
                        tx_freq = hz.parse().unwrap();
                        "RPRT 0\n".into()
                    }
                    ["F", "1"] => "RPRT -1\n".into(),
                    ["F", hz] => {
                        freq = hz.parse().unwrap();
                        "RPRT 0\n".into()
                    }
                    ["M", m, _] => {
                        mode = m.to_string();
                        "RPRT 0\n".into()
                    }
                    _ => "RPRT -4\n".into(),
                };
                w.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (port, log)
    }

    #[tokio::test]
    async fn end_to_end() {
        let (port, log) = mock_rigctld().await;
        let handle = spawn(RigConfig::Hamlib { host: "127.0.0.1".into(), port });
        let mut rx = handle.subscribe();
        let wait = |rx: &mut tokio::sync::watch::Receiver<Vec<RigState>>, f: fn(&RigState) -> bool| {
            let mut rx = rx.clone();
            async move {
                timeout(Duration::from_secs(5), rx.wait_for(|s| f(&s[0]))).await.unwrap().unwrap().clone()
            }
        };
        let st = wait(&mut rx, |s| s.connected).await;
        assert_eq!(st[0].freq_hz, 14_074_000);
        assert_eq!((st[0].mode.as_str(), st[0].rig_mode.as_str(), st[0].data), ("", "PKTUSB", true));
        assert!(!st[0].tx);

        handle.send(0, RigCommand::SetFreq(7_030_000));
        handle.send(0, RigCommand::SetMode("CW".into()));
        let st = wait(&mut rx, |s| s.freq_hz == 7_030_000 && s.mode == "CW").await;
        assert_eq!(st[0].rig_mode, "CW");
        handle.send(0, RigCommand::SetMode("SSB".into()));
        wait(&mut rx, |s| s.rig_mode == "LSB").await;
        {
            let log = log.lock().unwrap();
            assert!(log.contains(&"F 7030000".to_string()));
            assert!(log.contains(&"M CW 0".to_string()));
            assert!(log.contains(&"M LSB 0".to_string()));
        }
        // Split: VFO B transmits on its own frequency, then split goes off.
        assert!(!st[0].split);
        handle.send(0, RigCommand::SetSplit(Some(7_035_000)));
        let st = wait(&mut rx, |s| s.split && s.tx_freq_hz == 7_035_000).await;
        assert_eq!(st[0].freq_hz, 7_030_000);
        handle.send(0, RigCommand::SetSplit(None));
        wait(&mut rx, |s| !s.split).await;
        {
            let log = log.lock().unwrap();
            assert!(log.contains(&"S 1 VFOB".to_string()) && log.contains(&"I 7035000".to_string()));
            assert!(log.contains(&"S 0 VFOA".to_string()));
        }
        handle.send(0, RigCommand::SetFreq(1));
        let st = wait(&mut rx, |s| s.error.is_some()).await;
        assert!(st[0].connected);
        assert!(st[0].error.as_ref().unwrap().contains("RPRT -1"));
    }
}
