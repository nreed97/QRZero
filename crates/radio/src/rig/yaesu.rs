//! Newer Yaesu ASCII CAT (FT-991A, FTDX10/101, FT-710).

use anyhow::{anyhow, bail, Result};

use super::serial::{parse_digits, Protocol, Reply, SerialIo};
use super::{ModeInfo, ModeReq, RigCommand, RigState};

#[derive(Default)]
pub(super) struct Yaesu {
    last: RigState,
}

impl Protocol for Yaesu {
    fn poll(&mut self, io: &mut SerialIo) -> Result<RigState> {
        let mut st = RigState::default();
        match io.ask("FA;", "FA")? {
            Reply::Value(v) => st.freq_hz = parse_fa(&v).ok_or_else(|| anyhow!("unexpected reply {v}"))?,
            // Busy: keep the last state.
            Reply::Rejected => return Ok(self.last.clone()),
            Reply::Silent => bail!("no response from radio (check port and baud rate)"),
        }
        if let Some(code) = io.ask("MD0;", "MD")?.value().and_then(|v| parse_md(&v)) {
            decode_mode(code).apply(&mut st);
        }
        st.tx = io.ask("TX;", "TX")?.value().is_some_and(|v| v != "TX0;");
        self.last = st.clone();
        Ok(st)
    }

    fn command(&mut self, io: &mut SerialIo, cmd: &RigCommand, freq_hz: u64) -> Result<()> {
        match cmd {
            RigCommand::SetFreq(hz) => io.set(&encode_freq(*hz)),
            RigCommand::SetMode(m) => match ModeReq::from_adif(m, freq_hz) {
                Some(req) => io.set(&format!("MD0{};", encode_mode(req))),
                None => Ok(()),
            },
            RigCommand::SetSplit(_) => anyhow::bail!("split control needs a TCI connection"),
        }
    }
}

/// Parses `FA014025000;` (9 digits on most models, any width accepted).
fn parse_fa(s: &str) -> Option<u64> {
    parse_digits(s.strip_prefix("FA")?.strip_suffix(';')?)
}

/// Parses `MD0C;` into its mode code character.
fn parse_md(s: &str) -> Option<char> {
    let mut chars = s.strip_prefix("MD")?.strip_suffix(';')?.chars();
    let (_vfo, code) = (chars.next()?, chars.next()?);
    chars.next().is_none().then_some(code)
}

fn decode_mode(code: char) -> ModeInfo {
    match code {
        '1' => ModeInfo::new("SSB", "LSB"),
        '2' => ModeInfo::new("SSB", "USB"),
        '3' => ModeInfo::new("CW", "CW-U"),
        '4' => ModeInfo::new("FM", "FM"),
        '5' => ModeInfo::new("AM", "AM"),
        '6' => ModeInfo::new("RTTY", "RTTY-L"),
        '7' => ModeInfo::new("CW", "CW-L"),
        '8' => ModeInfo::data("DATA-L"),
        '9' => ModeInfo::new("RTTY", "RTTY-U"),
        'A' => ModeInfo::data("DATA-FM"),
        'B' => ModeInfo::new("FM", "FM-N"),
        'C' => ModeInfo::data("DATA-U"),
        'D' => ModeInfo::new("AM", "AM-N"),
        'E' => ModeInfo::data("PSK"),
        c => ModeInfo::new("", format!("MD{c}")),
    }
}

fn encode_mode(req: ModeReq) -> char {
    match req {
        ModeReq::Lsb => '1',
        ModeReq::Usb => '2',
        ModeReq::Cw => '3',
        ModeReq::Fm => '4',
        ModeReq::Am => '5',
        ModeReq::Rtty => '6',
        ModeReq::Data => 'C',
    }
}

fn encode_freq(hz: u64) -> String {
    format!("FA{:09};", hz.min(999_999_999))
}

#[cfg(test)]
mod tests {
    use super::super::serial::fake;
    use super::*;

    #[test]
    fn parse_replies() {
        assert_eq!(parse_fa("FA014025000;"), Some(14_025_000));
        assert_eq!(parse_fa("FA0014025000;"), Some(14_025_000));
        assert_eq!(parse_fa("FA;"), None);
        assert_eq!(parse_md("MD0C;"), Some('C'));
        assert_eq!(parse_md("MD02;"), Some('2'));
        assert_eq!(parse_md("MD0;"), None);
        assert_eq!(encode_freq(7_074_000), "FA007074000;");
    }

    #[test]
    fn modes() {
        assert_eq!(decode_mode('2'), ModeInfo::new("SSB", "USB"));
        assert_eq!(decode_mode('7'), ModeInfo::new("CW", "CW-L"));
        assert_eq!(decode_mode('9'), ModeInfo::new("RTTY", "RTTY-U"));
        assert_eq!(decode_mode('C'), ModeInfo::data("DATA-U"));
        assert_eq!(decode_mode('A'), ModeInfo::data("DATA-FM"));
        assert_eq!(decode_mode('D'), ModeInfo::new("AM", "AM-N"));
        assert_eq!(encode_mode(ModeReq::Data), 'C');
        assert_eq!(encode_mode(ModeReq::Cw), '3');
        assert_eq!(encode_mode(ModeReq::Lsb), '1');
    }

    #[test]
    fn poll_and_command() {
        let link = fake::ascii(|cmd| match cmd {
            "FA;" => Some("FA014074000;".into()),
            "MD0;" => Some("MD0C;".into()),
            "TX;" => Some("TX1;".into()),
            "MD0Z;" => Some("?;".into()),
            _ => None,
        });
        let written = link.written.clone();
        let mut io = SerialIo::new(Box::new(link));
        let mut yaesu = Yaesu::default();
        let st = yaesu.poll(&mut io).unwrap();
        assert_eq!((st.freq_hz, st.rig_mode.as_str(), st.data, st.tx), (14_074_000, "DATA-U", true, true));
        yaesu.command(&mut io, &RigCommand::SetMode("SSB".into()), 7_100_000).unwrap();
        yaesu.command(&mut io, &RigCommand::SetFreq(7_100_000), 0).unwrap();
        assert!(io.set("MD0Z;").is_err());
        let log = String::from_utf8(written.lock().unwrap().clone()).unwrap();
        assert!(log.ends_with("MD01;FA007100000;MD0Z;"), "{log}");
    }
}
