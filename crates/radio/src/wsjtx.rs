//! WSJT-X / JTDX UDP protocol (see WSJT-X `NetworkMessage.hpp`).
//!
//! Every datagram is a big-endian `QDataStream`: magic `0xADBCCBDA`, schema, message type and the
//! sending application's id, followed by the type's fields. Strings are UTF-8 `QByteArray`s.

use serde::Serialize;

/// First four bytes of every WSJT-X datagram.
pub const MAGIC: u32 = 0xADBC_CBDA;
/// Schema number written by the encoders here (also the highest one parsed).
pub const SCHEMA: u32 = 2;
/// Julian day number of 1970-01-01.
const UNIX_EPOCH_JULIAN_DAY: i64 = 2_440_588;

/// Decoding failures.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The datagram does not start with [`MAGIC`].
    #[error("not a WSJT-X datagram (bad magic {0:#010x})")]
    BadMagic(u32),
    /// Schema newer than this decoder understands.
    #[error("unsupported WSJT-X schema {0}")]
    UnsupportedSchema(u32),
    /// The datagram ended before a required field.
    #[error("truncated WSJT-X datagram")]
    Truncated,
    /// A `QDateTime` with a time zone spec this decoder does not handle.
    #[error("unsupported QDateTime timespec {0}")]
    UnsupportedTimeSpec(u8),
}

/// A datagram received from WSJT-X or JTDX.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Message {
    /// Type 0: sent periodically by both sides.
    Heartbeat {
        id: String,
        max_schema: u32,
        version: String,
        revision: String,
    },
    /// Type 1: the application's current state; sent on every change.
    Status {
        id: String,
        dial_freq: u64,
        mode: String,
        dx_call: String,
        report: String,
        tx_mode: String,
        tx_enabled: bool,
        transmitting: bool,
        decoding: bool,
        rx_df: u32,
        tx_df: u32,
        de_call: String,
        de_grid: String,
        dx_grid: String,
        tx_watchdog: bool,
        sub_mode: String,
        fast_mode: bool,
        special_op_mode: u8,
        frequency_tolerance: u32,
        tr_period: u32,
        configuration_name: String,
        tx_message: String,
    },
    /// Type 2: one decoded message.
    Decode(Decode),
    /// Type 3: the band activity (0), rx frequency (1) or both (2) windows were cleared.
    Clear { id: String, window: u8 },
    /// Type 5: the user logged a QSO. Date-times are (julian day, ms since midnight UTC).
    QsoLogged {
        id: String,
        time_off: (i64, u32),
        dx_call: String,
        dx_grid: String,
        tx_freq: u64,
        mode: String,
        report_sent: String,
        report_rcvd: String,
        tx_power: String,
        comments: String,
        name: String,
        time_on: (i64, u32),
        operator_call: String,
        my_call: String,
        my_grid: String,
        exchange_sent: String,
        exchange_rcvd: String,
        prop_mode: String,
    },
    /// Type 6: the application is closing.
    Close { id: String },
    /// Type 12: the logged QSO as an ADIF record (with header).
    LoggedAdif { id: String, adif: String },
    /// Any other message type, which this decoder ignores.
    Other { id: String, kind: u32 },
}

/// A decoded message (type 2), also the input for [`encode_reply`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Decode {
    pub id: String,
    pub new: bool,
    /// Milliseconds since midnight UTC.
    pub time_ms: u32,
    pub snr: i32,
    pub dt: f64,
    pub df: u32,
    pub mode: String,
    pub message: String,
    pub low_confidence: bool,
    pub off_air: bool,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let (head, rest) = self.0.split_first_chunk::<N>().ok_or(Error::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take::<1>()?[0])
    }
    fn bool(&mut self) -> Result<bool, Error> {
        Ok(self.u8()? != 0)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        self.take().map(u32::from_be_bytes)
    }
    fn i32(&mut self) -> Result<i32, Error> {
        self.take().map(i32::from_be_bytes)
    }
    fn u64(&mut self) -> Result<u64, Error> {
        self.take().map(u64::from_be_bytes)
    }
    fn i64(&mut self) -> Result<i64, Error> {
        self.take().map(i64::from_be_bytes)
    }
    fn f64(&mut self) -> Result<f64, Error> {
        self.take().map(f64::from_be_bytes)
    }
    fn bytes(&mut self) -> Result<&[u8], Error> {
        let len = self.u32()?;
        if len == u32::MAX {
            return Ok(&[]);
        }
        let len = len as usize;
        if self.0.len() < len {
            return Err(Error::Truncated);
        }
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(head)
    }
    fn string(&mut self) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(self.bytes()?).into_owned())
    }
    fn datetime(&mut self) -> Result<(i64, u32), Error> {
        let (day, ms) = (self.i64()?, self.u32()?);
        match self.u8()? {
            0 | 1 => {}
            2 => {
                self.i32()?;
            }
            spec => return Err(Error::UnsupportedTimeSpec(spec)),
        }
        Ok((day, ms))
    }
    /// Reads an optional trailing field, defaulting when the datagram has ended.
    fn opt<T: Default>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.0.is_empty() {
            Ok(T::default())
        } else {
            f(self)
        }
    }
}

/// Parses one WSJT-X / JTDX datagram.
///
/// Fields added in later WSJT-X versions (Status after `dx_grid`, Decode's `off_air`, ...) default
/// when absent so older JTDX builds still parse.
pub fn parse(buf: &[u8]) -> Result<Message, Error> {
    let mut r = Reader(buf);
    let magic = r.u32()?;
    if magic != MAGIC {
        return Err(Error::BadMagic(magic));
    }
    let schema = r.u32()?;
    if !(1..=3).contains(&schema) {
        return Err(Error::UnsupportedSchema(schema));
    }
    let kind = r.u32()?;
    let id = r.string()?;
    Ok(match kind {
        0 => Message::Heartbeat {
            id,
            max_schema: r.u32()?,
            version: r.opt(Reader::string)?,
            revision: r.opt(Reader::string)?,
        },
        1 => Message::Status {
            id,
            dial_freq: r.u64()?,
            mode: r.string()?,
            dx_call: r.string()?,
            report: r.string()?,
            tx_mode: r.string()?,
            tx_enabled: r.bool()?,
            transmitting: r.bool()?,
            decoding: r.bool()?,
            rx_df: r.u32()?,
            tx_df: r.u32()?,
            de_call: r.string()?,
            de_grid: r.string()?,
            dx_grid: r.string()?,
            tx_watchdog: r.opt(Reader::bool)?,
            sub_mode: r.opt(Reader::string)?,
            fast_mode: r.opt(Reader::bool)?,
            special_op_mode: r.opt(Reader::u8)?,
            frequency_tolerance: r.opt(Reader::u32)?,
            tr_period: r.opt(Reader::u32)?,
            configuration_name: r.opt(Reader::string)?,
            tx_message: r.opt(Reader::string)?,
        },
        2 => Message::Decode(Decode {
            id,
            new: r.bool()?,
            time_ms: r.u32()?,
            snr: r.i32()?,
            dt: r.f64()?,
            df: r.u32()?,
            mode: r.string()?,
            message: r.string()?,
            low_confidence: r.opt(Reader::bool)?,
            off_air: r.opt(Reader::bool)?,
        }),
        3 => Message::Clear {
            id,
            window: r.opt(Reader::u8)?,
        },
        5 => Message::QsoLogged {
            id,
            time_off: r.datetime()?,
            dx_call: r.string()?,
            dx_grid: r.string()?,
            tx_freq: r.u64()?,
            mode: r.string()?,
            report_sent: r.string()?,
            report_rcvd: r.string()?,
            tx_power: r.string()?,
            comments: r.string()?,
            name: r.string()?,
            time_on: r.opt(Reader::datetime)?,
            operator_call: r.opt(Reader::string)?,
            my_call: r.opt(Reader::string)?,
            my_grid: r.opt(Reader::string)?,
            exchange_sent: r.opt(Reader::string)?,
            exchange_rcvd: r.opt(Reader::string)?,
            prop_mode: r.opt(Reader::string)?,
        },
        6 => Message::Close { id },
        12 => Message::LoggedAdif {
            id,
            adif: r.string()?,
        },
        kind => Message::Other { id, kind },
    })
}

/// Converts a `QDateTime`'s julian day and ms since midnight UTC to Unix milliseconds.
pub fn julian_to_unix_ms(day: i64, ms: u32) -> i64 {
    (day - UNIX_EPOCH_JULIAN_DAY) * 86_400_000 + i64::from(ms)
}

struct Writer(Vec<u8>);

impl Writer {
    fn new(kind: u32, id: &str) -> Self {
        let mut w = Writer(Vec::with_capacity(64));
        w.u32(MAGIC).u32(SCHEMA).u32(kind).str(id);
        w
    }
    fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    fn u8(&mut self, v: u8) -> &mut Self {
        self.raw(&[v])
    }
    fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v.into())
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.raw(&v.to_be_bytes())
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.raw(&v.to_be_bytes())
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.raw(&v.to_be_bytes())
    }
    fn f64(&mut self, v: f64) -> &mut Self {
        self.raw(&v.to_be_bytes())
    }
    fn str(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32).raw(s.as_bytes())
    }
    /// QColor: spec, alpha, r, g, b, pad; an absent colour is an invalid QColor (clears it).
    fn color(&mut self, c: Option<(u8, u8, u8)>) -> &mut Self {
        match c {
            Some((r, g, b)) => self
                .u8(1)
                .u16(0xFFFF)
                .u16(u16::from(r) * 257)
                .u16(u16::from(g) * 257)
                .u16(u16::from(b) * 257),
            None => self.u8(0).u16(0xFFFF).u16(0).u16(0).u16(0),
        }
        .u16(0)
    }
    fn done(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

/// Encodes a Heartbeat (type 0) announcing this client.
pub fn encode_heartbeat(id: &str, version: &str) -> Vec<u8> {
    Writer::new(0, id).u32(SCHEMA).str(version).str("").done()
}

/// Encodes a Reply (type 4): asks WSJT-X to answer `d` as if the user double-clicked it.
/// `modifiers` are Qt keyboard modifier bits >> 24 (0x02 Shift, 0x04 Ctrl, 0x08 Alt).
pub fn encode_reply(d: &Decode, modifiers: u8) -> Vec<u8> {
    Writer::new(4, &d.id)
        .u32(d.time_ms)
        .i32(d.snr)
        .f64(d.dt)
        .u32(d.df)
        .str(&d.mode)
        .str(&d.message)
        .bool(d.low_confidence)
        .u8(modifiers)
        .done()
}

/// Encodes a Halt Tx (type 8); `auto_only` just disables auto Tx instead of stopping now.
pub fn encode_halt_tx(id: &str, auto_only: bool) -> Vec<u8> {
    Writer::new(8, id).bool(auto_only).done()
}

/// Encodes a Free Text (type 9) setting the Tx5 free-text message, optionally sending it.
pub fn encode_free_text(id: &str, text: &str, send: bool) -> Vec<u8> {
    Writer::new(9, id).str(text).bool(send).done()
}

/// Encodes a Highlight Callsign (type 13); `None` colours clear that highlight.
pub fn encode_highlight_callsign(
    id: &str,
    call: &str,
    background: Option<(u8, u8, u8)>,
    foreground: Option<(u8, u8, u8)>,
    highlight_last: bool,
) -> Vec<u8> {
    Writer::new(13, id)
        .str(call)
        .color(background)
        .color(foreground)
        .bool(highlight_last)
        .done()
}

/// The parts of a standard FT8/FT4/JT65/Q65 message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FtMessage {
    /// A CQ call.
    pub cq: bool,
    /// Directed CQ target, e.g. `DX`, `POTA`, `NA`.
    pub cq_target: Option<String>,
    /// The sending station (the second call, or the caller of a CQ).
    pub from: Option<String>,
    /// The addressed station (the first call).
    pub to: Option<String>,
    pub grid: Option<String>,
    /// Signal report without the roger prefix, e.g. `-12` for both `-12` and `R-12`.
    pub report: Option<String>,
    /// `RR73`, `RRR` or `73`: the QSO is complete from the sender's side.
    pub rr73: bool,
}

/// Strips hash brackets and returns the call when `t` looks like a callsign.
fn callsign(t: &str) -> Option<&str> {
    let c = t
        .strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(t);
    let ok = (3..=13).contains(&c.len())
        && c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/')
        && c.bytes().any(|b| b.is_ascii_digit())
        && c.bytes().any(|b| b.is_ascii_alphabetic());
    ok.then_some(c)
}

/// Like [`callsign`] but also accepts an unresolved hash `<...>` as an unknown call.
fn call_slot(t: &str) -> Option<Option<&str>> {
    if t == "<...>" {
        Some(None)
    } else {
        callsign(t).map(Some)
    }
}

fn is_grid(t: &str) -> bool {
    let b = t.as_bytes();
    t != "RR73"
        && (b.len() == 4 || b.len() == 6)
        && matches!(b[0], b'A'..=b'R')
        && matches!(b[1], b'A'..=b'R')
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4..].iter().all(|c| matches!(c, b'A'..=b'X'))
}

fn is_report(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 3 && matches!(b[0], b'+' | b'-') && b[1..].iter().all(u8::is_ascii_digit)
}

/// Splits a WSJT-X message into its standard parts. Free text returns everything `None`.
///
/// "from" is the sending station (second call), "to" the addressed one (first call).
pub fn parse_ft_message(text: &str) -> FtMessage {
    let upper = text.trim().to_ascii_uppercase();
    let t: Vec<&str> = upper.split_whitespace().collect();
    let own = |s: Option<&str>| s.map(str::to_owned);
    let mut m = FtMessage::default();
    if t.first() == Some(&"CQ") {
        let is_target = |s: &str| {
            (1..=4).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphabetic())
                || s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit())
        };
        let (target, rest) = match t.get(1..) {
            Some([tg, rest @ ..]) if is_target(tg) && !rest.is_empty() => (Some(*tg), rest),
            Some(rest) => (None, rest),
            None => return m,
        };
        let Some((call, rest)) = rest
            .split_first()
            .and_then(|(c, r)| Some((callsign(c)?, r)))
        else {
            return m;
        };
        match rest {
            [] => {}
            [g] if is_grid(g) => m.grid = Some(g.to_string()),
            _ => return m,
        }
        m.cq = true;
        m.cq_target = own(target);
        m.from = Some(call.to_owned());
        return m;
    }
    let (Some(Some(to_call)), Some(Some(from_call))) = (
        t.first().map(|s| call_slot(s)),
        t.get(1).map(|s| call_slot(s)),
    ) else {
        return m;
    };
    match &t[2..] {
        [] => {}
        [g] | ["R", g] if is_grid(g) => m.grid = Some(g.to_string()),
        [r] if is_report(r) => m.report = Some(r.to_string()),
        [r] if r.starts_with('R') && is_report(&r[1..]) => m.report = Some(r[1..].to_string()),
        [x] if matches!(*x, "RR73" | "RRR" | "73") => m.rr73 = true,
        _ => return m,
    }
    m.to = own(to_call);
    m.from = own(from_call);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_buf(full: bool) -> Vec<u8> {
        let mut w = Writer::new(1, "WSJT-X");
        w.raw(&14_074_000u64.to_be_bytes())
            .str("FT8")
            .str("K1ABC")
            .str("-12")
            .str("FT8")
            .bool(true)
            .bool(false)
            .bool(true)
            .u32(1500)
            .u32(1200)
            .str("W9XYZ")
            .str("EN37")
            .str("FN42");
        if full {
            w.bool(false)
                .str("")
                .bool(false)
                .u8(0)
                .u32(10)
                .u32(15)
                .str("Default")
                .str("K1ABC W9XYZ R-05");
        }
        w.done()
    }

    #[test]
    fn status_full_and_old() {
        let Message::Status {
            dial_freq,
            dx_call,
            de_grid,
            tr_period,
            configuration_name,
            tx_message,
            rx_df,
            tx_enabled,
            ..
        } = parse(&status_buf(true)).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            (dial_freq, dx_call.as_str(), de_grid.as_str()),
            (14_074_000, "K1ABC", "EN37")
        );
        assert_eq!(
            (tr_period, configuration_name.as_str(), tx_message.as_str()),
            (15, "Default", "K1ABC W9XYZ R-05")
        );
        assert_eq!((rx_df, tx_enabled), (1500, true));
        let Message::Status {
            dx_grid,
            tr_period,
            tx_message,
            ..
        } = parse(&status_buf(false)).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            (dx_grid.as_str(), tr_period, tx_message.as_str()),
            ("FN42", 0, "")
        );
    }

    #[test]
    fn decode_roundtrip_and_reply_layout() {
        let d = Decode {
            id: "WSJT-X".into(),
            new: true,
            time_ms: 45_015_000,
            snr: -12,
            dt: 0.2,
            df: 1234,
            mode: "~".into(),
            message: "CQ K1ABC FN42".into(),
            low_confidence: false,
            off_air: false,
        };
        let buf = Writer::new(2, &d.id)
            .bool(d.new)
            .u32(d.time_ms)
            .i32(d.snr)
            .f64(d.dt)
            .u32(d.df)
            .str(&d.mode)
            .str(&d.message)
            .bool(d.low_confidence)
            .bool(d.off_air)
            .done();
        assert_eq!(parse(&buf).unwrap(), Message::Decode(d.clone()));
        let mut old = buf.clone();
        old.truncate(buf.len() - 2);
        assert_eq!(parse(&old).unwrap(), Message::Decode(d.clone()));

        let mut want = vec![0xAD, 0xBC, 0xCB, 0xDA, 0, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0, 6];
        want.extend_from_slice(b"WSJT-X");
        want.extend_from_slice(&[0x02, 0xAE, 0xDF, 0xD8]); // 45015000
        want.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xF4]); // -12
        want.extend_from_slice(&[0x3F, 0xC9, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9A]); // 0.2
        want.extend_from_slice(&[0, 0, 0x04, 0xD2]); // 1234
        want.extend_from_slice(&[0, 0, 0, 1, b'~', 0, 0, 0, 13]);
        want.extend_from_slice(b"CQ K1ABC FN42");
        want.extend_from_slice(&[0, 0x02]);
        assert_eq!(encode_reply(&d, 0x02), want);
    }

    #[test]
    fn heartbeat_close_clear_adif_other() {
        let hb = encode_heartbeat("QRZero", "1.0");
        let want = Message::Heartbeat {
            id: "QRZero".into(),
            max_schema: 2,
            version: "1.0".into(),
            revision: "".into(),
        };
        assert_eq!(parse(&hb).unwrap(), want);
        let mut null_rev = Writer::new(0, "JTDX");
        null_rev.u32(3).str("2.2").u32(u32::MAX);
        assert!(
            matches!(parse(&null_rev.done()).unwrap(), Message::Heartbeat { max_schema: 3, revision, .. } if revision.is_empty())
        );
        assert_eq!(
            parse(&Writer::new(6, "X").done()).unwrap(),
            Message::Close { id: "X".into() }
        );
        assert_eq!(
            parse(&Writer::new(3, "X").u8(2).done()).unwrap(),
            Message::Clear {
                id: "X".into(),
                window: 2
            }
        );
        assert_eq!(
            parse(&Writer::new(3, "X").done()).unwrap(),
            Message::Clear {
                id: "X".into(),
                window: 0
            }
        );
        let adif = "<adif_ver:5>3.1.0<eoh><call:5>K1ABC<eor>";
        assert_eq!(
            parse(&Writer::new(12, "X").str(adif).done()).unwrap(),
            Message::LoggedAdif {
                id: "X".into(),
                adif: adif.into()
            }
        );
        assert_eq!(
            parse(&Writer::new(10, "X").u32(7).done()).unwrap(),
            Message::Other {
                id: "X".into(),
                kind: 10
            }
        );
    }

    #[test]
    fn qso_logged() {
        let day: i64 = 2_458_866; // 2020-01-17
        let mut w = Writer::new(5, "WSJT-X");
        w.raw(&day.to_be_bytes()).u32(60_218_000).u8(1);
        w.str("K1ABC")
            .str("FN42")
            .raw(&14_074_000u64.to_be_bytes())
            .str("FT8")
            .str("-12")
            .str("-05")
            .str("100")
            .str("tnx")
            .str("Bob");
        w.raw(&day.to_be_bytes()).u32(60_158_000).u8(2).i32(3600);
        w.str("").str("W9XYZ").str("EN37").str("").str("").str("");
        let Message::QsoLogged {
            time_off,
            time_on,
            dx_call,
            tx_freq,
            report_rcvd,
            name,
            my_grid,
            ..
        } = parse(&w.done()).unwrap()
        else {
            panic!()
        };
        assert_eq!((time_off, time_on), ((day, 60_218_000), (day, 60_158_000)));
        assert_eq!(
            (
                dx_call.as_str(),
                tx_freq,
                report_rcvd.as_str(),
                name.as_str(),
                my_grid.as_str()
            ),
            ("K1ABC", 14_074_000, "-05", "Bob", "EN37")
        );
        assert_eq!(julian_to_unix_ms(day, 60_218_000), 1_579_279_418_000);
        assert_eq!(julian_to_unix_ms(UNIX_EPOCH_JULIAN_DAY, 0), 0);
    }

    #[test]
    fn errors() {
        assert_eq!(parse(&[0, 0, 0, 0]), Err(Error::BadMagic(0)));
        assert_eq!(
            parse(&[0xAD, 0xBC, 0xCB, 0xDA, 0, 0, 0, 9]),
            Err(Error::UnsupportedSchema(9))
        );
        let mut status = status_buf(false);
        status.truncate(status.len() - 3);
        assert_eq!(parse(&status), Err(Error::Truncated));
        assert_eq!(parse(&Writer::new(1, "X").done()), Err(Error::Truncated));
    }

    #[test]
    fn outgoing_messages() {
        let halt = encode_halt_tx("WSJT-X", true);
        assert_eq!(&halt[8..12], &[0, 0, 0, 8]);
        assert_eq!(halt.last(), Some(&1));
        let ft = encode_free_text("WSJT-X", "TNX 73", false);
        assert_eq!(
            &ft[ft.len() - 11..],
            &[0, 0, 0, 6, b'T', b'N', b'X', b' ', b'7', b'3', 0]
        );
        let hl = encode_highlight_callsign("X", "K1ABC", Some((255, 0, 128)), None, true);
        let tail = &hl[4 + 4 + 4 + 5 + 9..];
        assert_eq!(
            &tail[..11],
            &[1, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0x80, 0x80, 0, 0]
        );
        assert_eq!(&tail[11..], &[0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    }

    fn ft(text: &str) -> FtMessage {
        parse_ft_message(text)
    }

    fn s(v: &str) -> Option<String> {
        Some(v.to_owned())
    }

    #[test]
    fn ft_cq() {
        assert_eq!(
            ft("CQ K1ABC FN42"),
            FtMessage {
                cq: true,
                from: s("K1ABC"),
                grid: s("FN42"),
                ..Default::default()
            }
        );
        assert_eq!(
            ft("CQ DX K1ABC FN42"),
            FtMessage {
                cq: true,
                cq_target: s("DX"),
                from: s("K1ABC"),
                grid: s("FN42"),
                ..Default::default()
            }
        );
        assert_eq!(
            ft("CQ POTA K1ABC"),
            FtMessage {
                cq: true,
                cq_target: s("POTA"),
                from: s("K1ABC"),
                ..Default::default()
            }
        );
        assert_eq!(ft("CQ 290 K1ABC FN42").cq_target, s("290"));
        assert_eq!(
            ft("cq k1abc/p"),
            FtMessage {
                cq: true,
                from: s("K1ABC/P"),
                ..Default::default()
            }
        );
        assert_eq!(ft("CQ <PJ4/K1ABC>").from, s("PJ4/K1ABC"));
        assert_eq!(ft("CQ DX"), FtMessage::default());
        assert_eq!(ft("CQ K1ABC HELLO"), FtMessage::default());
    }

    #[test]
    fn ft_directed() {
        assert_eq!(
            ft("K1ABC W9XYZ EN37"),
            FtMessage {
                to: s("K1ABC"),
                from: s("W9XYZ"),
                grid: s("EN37"),
                ..Default::default()
            }
        );
        assert_eq!(ft("K1ABC W9XYZ R EN37").grid, s("EN37"));
        assert_eq!(
            ft("K1ABC W9XYZ -12"),
            FtMessage {
                to: s("K1ABC"),
                from: s("W9XYZ"),
                report: s("-12"),
                ..Default::default()
            }
        );
        assert_eq!(ft("K1ABC W9XYZ R+05").report, s("+05"));
        for end in ["RR73", "RRR", "73"] {
            let m = ft(&format!("K1ABC W9XYZ {end}"));
            assert!(m.rr73 && m.grid.is_none() && m.report.is_none(), "{end}");
            assert_eq!(m.from, s("W9XYZ"));
        }
        assert_eq!(ft("<K1ABC/P> W9XYZ -08").to, s("K1ABC/P"));
        assert_eq!(
            ft("K1ABC <...> RR73"),
            FtMessage {
                to: s("K1ABC"),
                rr73: true,
                ..Default::default()
            }
        );
        assert_eq!(ft("VK2/K1ABC W9XYZ/R").from, s("W9XYZ/R"));
        assert_eq!(
            ft("K1ABC W9XYZ"),
            FtMessage {
                to: s("K1ABC"),
                from: s("W9XYZ"),
                ..Default::default()
            }
        );
    }

    #[test]
    fn ft_free_text() {
        for t in [
            "TNX BOB 73 GL",
            "HELLO WORLD",
            "",
            "73",
            "K1ABC",
            "K1ABC W9XYZ 599 0001",
        ] {
            assert_eq!(ft(t), FtMessage::default(), "{t}");
        }
    }
}
