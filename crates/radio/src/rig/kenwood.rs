//! Kenwood-style ASCII CAT (Kenwood, Elecraft, FlexRadio SmartSDR CAT).
//!
//! The protocols agree on `IF;`, `FA` and `MD` but differ in data modes: Elecraft's MD6 is DATA with
//! a `DT` sub-mode, Flex's MD6/MD9 are DIGL/DIGU, and newer Kenwoods add data via `DA`.

use anyhow::{bail, Result};

use super::serial::{parse_digits, Protocol, Reply, SerialIo};
use super::{ModeInfo, ModeReq, RigCommand, RigState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Dialect {
    #[default]
    Kenwood,
    Elecraft,
    Flex,
}

#[derive(Default)]
pub(super) struct Kenwood {
    dialect: Dialect,
    /// Supports `DA` (data mode on/off, TS-590/890/990).
    has_da: bool,
    last: RigState,
}

impl Protocol for Kenwood {
    // Older Kenwoods use RTS/CTS handshaking, so leave the lines as the OS sets them.
    const CLEAR_LINES: bool = false;

    fn init(&mut self, io: &mut SerialIo) -> Result<()> {
        let id = match io.ask("ID;", "ID")? {
            Reply::Silent => bail!("no response from radio (check port and baud rate)"),
            r => r.value().and_then(|v| parse_field(&v, "ID")),
        };
        self.dialect = if id.is_some_and(|n| n >= 900) {
            Dialect::Flex
        } else if io.ask("DT;", "DT")?.value().and_then(|v| parse_field(&v, "DT")).is_some() {
            Dialect::Elecraft
        } else {
            Dialect::Kenwood
        };
        self.has_da = self.dialect == Dialect::Kenwood && io.ask("DA;", "DA")?.value().and_then(|v| parse_field(&v, "DA")).is_some();
        tracing::info!("kenwood CAT: id {id:?}, {:?}, DA {}", self.dialect, self.has_da);
        Ok(())
    }

    fn poll(&mut self, io: &mut SerialIo) -> Result<RigState> {
        let status = match io.ask("IF;", "IF")? {
            Reply::Value(v) => parse_if(&v).ok_or_else(|| anyhow::anyhow!("unexpected reply {v}"))?,
            // Busy: keep the last state.
            Reply::Rejected => return Ok(self.last.clone()),
            Reply::Silent => bail!("no response from radio"),
        };
        let dt = if self.dialect == Dialect::Elecraft && matches!(status.mode, b'6' | b'9') {
            io.ask("DT;", "DT")?.value().and_then(|v| parse_field(&v, "DT"))
        } else {
            None
        };
        let da = self.has_da && io.ask("DA;", "DA")?.value().and_then(|v| parse_field(&v, "DA")).is_some_and(|d| d != 0);
        let mut st = RigState { freq_hz: status.freq_hz, tx: status.tx, ..Default::default() };
        decode_mode(self.dialect, status.mode, dt, da).apply(&mut st);
        if status.split {
            // The transmit frequency is on the VFO we aren't receiving on.
            let ask = if status.rx_vfo_b { ("FA;", "FA") } else { ("FB;", "FB") };
            st.tx_freq_hz = io.ask(ask.0, ask.1)?.value().and_then(|v| parse_field(&v, ask.1)).unwrap_or(0);
            st.split = st.tx_freq_hz > 0;
        }
        self.last = st.clone();
        Ok(st)
    }

    fn command(&mut self, io: &mut SerialIo, cmd: &RigCommand, freq_hz: u64) -> Result<()> {
        match cmd {
            RigCommand::SetFreq(hz) => io.set(&encode_freq(*hz)),
            RigCommand::SetMode(m) => match ModeReq::from_adif(m, freq_hz) {
                Some(req) => io.set(&encode_mode(self.dialect, self.has_da, req)),
                None => Ok(()),
            },
            // Receive on VFO A, transmit on VFO B (FR before FT, as the radios want).
            RigCommand::SetSplit(Some(hz)) => io.set(&format!("FB{:011};FR0;FT1;", (*hz).min(99_999_999_999))),
            RigCommand::SetSplit(None) => io.set("FR0;FT0;"),
        }
    }
}

/// The fields of an `IF;` reply we use.
#[derive(Debug, PartialEq)]
struct IfStatus {
    freq_hz: u64,
    tx: bool,
    /// The MD digit, as an ASCII byte.
    mode: u8,
    /// The receive VFO is B (otherwise A or memory).
    rx_vfo_b: bool,
    split: bool,
}

/// Parses `IF<11-digit freq><5><5 RIT><1><1><1><2 mem><tx><mode>...;`.
fn parse_if(s: &str) -> Option<IfStatus> {
    let b = s.strip_prefix("IF")?.strip_suffix(';')?;
    if b.len() < 28 || !b.is_ascii() {
        return None;
    }
    let at = |i: usize| b.as_bytes().get(i).copied();
    Some(IfStatus {
        freq_hz: parse_digits(&b[..11])?,
        tx: b.as_bytes()[26] == b'1',
        mode: b.as_bytes()[27],
        rx_vfo_b: at(28) == Some(b'1'),
        // Right after the mode: receive VFO, scan, split.
        split: at(30) == Some(b'1'),
    })
}

/// Parses a numeric reply such as `ID019;` or `DT0;`.
fn parse_field(s: &str, prefix: &str) -> Option<u64> {
    parse_digits(s.strip_prefix(prefix)?.strip_suffix(';')?)
}

fn decode_mode(dialect: Dialect, md: u8, dt: Option<u64>, da: bool) -> ModeInfo {
    match (dialect, md) {
        (Dialect::Flex, b'6') => return ModeInfo::data("DIGL"),
        (Dialect::Flex, b'9') => return ModeInfo::data("DIGU"),
        (Dialect::Elecraft, b'6' | b'9') => {
            let rev = if md == b'9' { "-R" } else { "" };
            return match dt {
                Some(1) => ModeInfo::new("RTTY", format!("AFSK-A{rev}")),
                Some(2) => ModeInfo::new("RTTY", format!("FSK-D{rev}")),
                Some(3) => ModeInfo::data(format!("PSK-D{rev}")),
                Some(0) => ModeInfo::data(format!("DATA-A{rev}")),
                _ => ModeInfo::data(format!("DATA{rev}")),
            };
        }
        _ => {}
    }
    let (adif, name) = match md {
        b'1' => ("SSB", "LSB"),
        b'2' => ("SSB", "USB"),
        b'3' => ("CW", "CW"),
        b'4' => ("FM", "FM"),
        b'5' => ("AM", "AM"),
        b'6' => ("RTTY", "FSK"),
        b'7' => ("CW", "CW-R"),
        b'9' => ("RTTY", "FSK-R"),
        _ => return ModeInfo::new("", format!("MD{}", md as char)),
    };
    if da && matches!(md, b'1' | b'2' | b'4' | b'5') {
        ModeInfo::data(format!("{name}-D"))
    } else {
        ModeInfo::new(adif, name)
    }
}

fn encode_freq(hz: u64) -> String {
    format!("FA{:011};", hz.min(99_999_999_999))
}

fn encode_mode(dialect: Dialect, has_da: bool, req: ModeReq) -> String {
    let md = match (req, dialect) {
        (ModeReq::Lsb, _) => "MD1;",
        (ModeReq::Usb, _) => "MD2;",
        (ModeReq::Cw, _) => "MD3;",
        (ModeReq::Fm, _) => "MD4;",
        (ModeReq::Am, _) => "MD5;",
        (ModeReq::Rtty, Dialect::Elecraft) => "MD6;DT2;",
        (ModeReq::Rtty, _) => "MD6;",
        (ModeReq::Data, Dialect::Elecraft) => "MD6;DT0;",
        (ModeReq::Data, Dialect::Flex) => "MD9;",
        (ModeReq::Data, Dialect::Kenwood) => "MD2;",
    };
    match (has_da, req) {
        (false, _) | (true, ModeReq::Rtty) => md.to_string(),
        (true, ModeReq::Data) => format!("{md}DA1;"),
        (true, _) => format!("{md}DA0;"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::serial::{fake, serve};
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::{mpsc, watch};

    /// A TS-2000 style IF reply: freq, 5 blanks, RIT offset, RIT/XIT/bank, memory, TX, mode, rest.
    fn if_reply(freq_hz: u64, tx: bool, md: char) -> String {
        format!("IF{freq_hz:011}     +000000000{}{md}0000000;", tx as u8)
    }

    #[test]
    fn parse_replies() {
        let st = parse_if(&if_reply(14_025_000, false, '3')).unwrap();
        assert_eq!(st, IfStatus { freq_hz: 14_025_000, tx: false, mode: b'3', rx_vfo_b: false, split: false });
        // K3 style (blank fields, negative RIT) while transmitting USB.
        let st = parse_if("IF00007074000     -001000 0012000001 ;").unwrap();
        assert_eq!(st, IfStatus { freq_hz: 7_074_000, tx: true, mode: b'2', rx_vfo_b: false, split: false });
        // Split on (VFO A receiving), then VFO B receiving.
        let st = parse_if(&format!("{}001;", &if_reply(14_025_000, false, '3')[..30])).unwrap();
        assert!(!st.tx && st.mode == b'3' && !st.rx_vfo_b && st.split);
        assert!(parse_if(&format!("{}100;", &if_reply(14_025_000, false, '3')[..30])).unwrap().rx_vfo_b);
        assert_eq!(parse_if("IF000;"), None);
        assert_eq!(parse_if("FA00014025000;"), None);
        assert_eq!(parse_field("ID019;", "ID"), Some(19));
        assert_eq!(parse_field("DT3;", "DT"), Some(3));
        assert_eq!(parse_field("?;", "DT"), None);
        assert_eq!(encode_freq(14_025_000), "FA00014025000;");
    }

    #[test]
    fn modes() {
        use Dialect::*;
        assert_eq!(decode_mode(Kenwood, b'1', None, false), ModeInfo::new("SSB", "LSB"));
        assert_eq!(decode_mode(Kenwood, b'7', None, false), ModeInfo::new("CW", "CW-R"));
        assert_eq!(decode_mode(Kenwood, b'6', None, false), ModeInfo::new("RTTY", "FSK"));
        assert_eq!(decode_mode(Kenwood, b'2', None, true), ModeInfo::data("USB-D"));
        assert_eq!(decode_mode(Kenwood, b'3', None, true), ModeInfo::new("CW", "CW"));
        assert_eq!(decode_mode(Elecraft, b'6', Some(0), false), ModeInfo::data("DATA-A"));
        assert_eq!(decode_mode(Elecraft, b'6', Some(2), false), ModeInfo::new("RTTY", "FSK-D"));
        assert_eq!(decode_mode(Elecraft, b'9', Some(1), false), ModeInfo::new("RTTY", "AFSK-A-R"));
        assert_eq!(decode_mode(Elecraft, b'5', None, false), ModeInfo::new("AM", "AM"));
        assert_eq!(decode_mode(Flex, b'9', None, false), ModeInfo::data("DIGU"));
        assert_eq!(decode_mode(Flex, b'6', None, false), ModeInfo::data("DIGL"));

        assert_eq!(encode_mode(Kenwood, false, ModeReq::Cw), "MD3;");
        assert_eq!(encode_mode(Kenwood, false, ModeReq::Data), "MD2;");
        assert_eq!(encode_mode(Kenwood, true, ModeReq::Data), "MD2;DA1;");
        assert_eq!(encode_mode(Kenwood, true, ModeReq::Lsb), "MD1;DA0;");
        assert_eq!(encode_mode(Kenwood, true, ModeReq::Rtty), "MD6;");
        assert_eq!(encode_mode(Elecraft, false, ModeReq::Data), "MD6;DT0;");
        assert_eq!(encode_mode(Elecraft, false, ModeReq::Rtty), "MD6;DT2;");
        assert_eq!(encode_mode(Flex, false, ModeReq::Data), "MD9;");
    }

    /// Runs the full poll loop against a fake K3.
    #[tokio::test]
    async fn serve_fake_k3() {
        // freq, mode, DT, VFO B freq, split
        let radio = Arc::new(Mutex::new((14_074_000u64, '6', '0', 0u64, false)));
        let r = radio.clone();
        let link = fake::ascii(move |cmd| {
            let mut r = r.lock().unwrap();
            if let Some(f) = cmd.strip_prefix("FA").and_then(|f| f.strip_suffix(';')).filter(|f| !f.is_empty()) {
                r.0 = f.parse().unwrap();
                return None;
            }
            if let Some(f) = cmd.strip_prefix("FB").and_then(|f| f.strip_suffix(';')).filter(|f| !f.is_empty()) {
                r.3 = f.parse().unwrap();
                return None;
            }
            match cmd {
                "FT1;" => {
                    r.4 = true;
                    None
                }
                "FT0;" => {
                    r.4 = false;
                    None
                }
                "FB;" => Some(format!("FB{:011};", r.3)),
                "ID;" => Some("ID017;".into()),
                "DT;" => Some(format!("DT{};", r.2)),
                "IF;" => {
                    let mut reply = if_reply(r.0, false, r.1);
                    reply.replace_range(32..33, if r.4 { "1" } else { "0" });
                    Some(reply)
                }
                "MD3;" => {
                    r.1 = '3';
                    None
                }
                _ => None,
            }
        });
        let written = link.written.clone();
        let (state_tx, state) = watch::channel(vec![RigState::default()]);
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let thread = std::thread::spawn(move || {
            let mut io = SerialIo::new(Box::new(link));
            serve(&mut io, Kenwood::default(), &Arc::new(state_tx), &mut cmd_rx)
        });
        let wait = |pred: fn(&RigState) -> bool| {
            let mut state = state.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(5), state.wait_for(|s| pred(&s[0]))).await.unwrap().unwrap()[0].clone()
            }
        };
        let st = wait(|s| s.connected).await;
        assert_eq!((st.freq_hz, st.rig_mode.as_str(), st.data), (14_074_000, "DATA-A", true));
        cmd_tx.send((0, RigCommand::SetFreq(7_030_000))).unwrap();
        cmd_tx.send((0, RigCommand::SetMode("CW".into()))).unwrap();
        let st = wait(|s| s.mode == "CW").await;
        assert_eq!(st.freq_hz, 7_030_000);
        let log = String::from_utf8(written.lock().unwrap().clone()).unwrap();
        assert!(log.contains("FA00007030000;") && log.contains("MD3;"), "{log}");
        cmd_tx.send((0, RigCommand::SetSplit(Some(7_035_000)))).unwrap();
        let st = wait(|s| s.split).await;
        assert_eq!((st.freq_hz, st.tx_freq_hz), (7_030_000, 7_035_000));
        cmd_tx.send((0, RigCommand::SetSplit(None))).unwrap();
        wait(|s| !s.split).await;
        let log = String::from_utf8(written.lock().unwrap().clone()).unwrap();
        assert!(log.contains("FB00007035000;FR0;FT1;") && log.contains("FR0;FT0;"), "{log}");
        drop(cmd_tx);
        thread.join().unwrap().unwrap();
    }
}
