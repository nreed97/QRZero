//! Icom CI-V: binary frames `FE FE <to> <from> <cmd> [sub] [data] FD` with BCD frequencies.

use std::time::Instant;

use anyhow::{bail, Result};

use super::serial::{Protocol, SerialIo, REPLY_TIMEOUT};
use super::{ModeInfo, ModeReq, RigCommand, RigState};

/// Our address as the controller.
const CONTROLLER: u8 = 0xE0;
/// Address of transceive broadcasts.
const BROADCAST: u8 = 0x00;
const PREAMBLE: u8 = 0xFE;
const END: u8 = 0xFD;
/// Bus collision marker; a frame containing it is garbage.
const COLLISION: u8 = 0xFC;
const OK: u8 = 0xFB;
const NG: u8 = 0xFA;

const READ_FREQ: u8 = 0x03;
const READ_MODE: u8 = 0x04;
const SET_FREQ: u8 = 0x05;
const SET_MODE: u8 = 0x06;
const EXTENDED: u8 = 0x1A;
const DATA_MODE: u8 = 0x06;
const TX_STATE: u8 = 0x1C;
const SPLIT: u8 = 0x0F;
/// Reads or sets the selected (`00`) or unselected (`01`) VFO's frequency; newer rigs only.
const VFO_FREQ: u8 = 0x25;

const NO_RESPONSE: &str = "no response from radio (check port, baud rate and CI-V address)";

#[derive(Clone, Debug, PartialEq)]
struct Frame {
    to: u8,
    from: u8,
    cmd: u8,
    /// Sub-command and data bytes.
    data: Vec<u8>,
}

enum Answer {
    Ok(Frame),
    Ng,
    Silent,
}

pub(super) struct Icom {
    addr: u8,
    /// Supports reading/setting data mode (`1A 06`).
    has_data: bool,
    /// Supports reading PTT (`1C 00`).
    has_tx: bool,
    /// Supports reading/setting the unselected VFO's frequency (`25 01`), which is where split transmits.
    has_unselected: bool,
    freq_hz: u64,
    mode: Option<u8>,
    data: bool,
    tx: bool,
}

impl Icom {
    pub(super) fn new(addr: u8) -> Self {
        Icom { addr, has_data: false, has_tx: false, has_unselected: false, freq_hz: 0, mode: None, data: false, tx: false }
    }

    /// Sends a command and waits for the radio's reply: the same command (and sub-command) for
    /// reads, `FB` or `FA` for sets. Transceive broadcasts seen meanwhile update the cached state.
    fn request(&mut self, io: &mut SerialIo, cmd: u8, sub: &[u8], data: &[u8]) -> Result<Answer> {
        io.write(&encode_frame(self.addr, cmd, &[sub, data].concat()))?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            while let Some(f) = next_frame(&mut io.buf) {
                // Skips our own echo on single-wire CI-V and other devices on the bus.
                if f.from != self.addr {
                    continue;
                }
                if f.to == BROADCAST {
                    self.transceive(&f);
                } else if f.to == CONTROLLER {
                    match f.cmd {
                        NG => return Ok(Answer::Ng),
                        OK => return Ok(Answer::Ok(f)),
                        c if c == cmd && f.data.starts_with(sub) => return Ok(Answer::Ok(f)),
                        _ => {}
                    }
                }
            }
            if Instant::now() >= deadline {
                return Ok(Answer::Silent);
            }
            io.fill()?;
        }
    }

    fn transceive(&mut self, f: &Frame) {
        match f.cmd {
            0x00 => self.freq_hz = bcd_to_freq(&f.data).unwrap_or(self.freq_hz),
            0x01 => self.mode = f.data.first().copied().or(self.mode),
            _ => {}
        }
    }

    /// Sends a set command and checks that the radio acknowledged it.
    fn set(&mut self, io: &mut SerialIo, cmd: u8, sub: &[u8], data: &[u8], what: &str) -> Result<()> {
        match self.request(io, cmd, sub, data)? {
            Answer::Ok(_) => Ok(()),
            Answer::Ng => bail!("radio rejected {what}"),
            Answer::Silent => bail!("no reply to {what}"),
        }
    }
}

impl Protocol for Icom {
    fn init(&mut self, io: &mut SerialIo) -> Result<()> {
        if let Answer::Silent = self.request(io, READ_FREQ, &[], &[])? {
            bail!(NO_RESPONSE);
        }
        self.has_data = matches!(self.request(io, EXTENDED, &[DATA_MODE], &[])?, Answer::Ok(_));
        self.has_tx = matches!(self.request(io, TX_STATE, &[0x00], &[])?, Answer::Ok(_));
        self.has_unselected = matches!(self.request(io, VFO_FREQ, &[0x01], &[])?, Answer::Ok(_));
        tracing::info!(
            "CI-V {:#04x}: data mode {}, PTT {}, split frequency {}",
            self.addr,
            self.has_data,
            self.has_tx,
            self.has_unselected
        );
        Ok(())
    }

    fn poll(&mut self, io: &mut SerialIo) -> Result<RigState> {
        match self.request(io, READ_FREQ, &[], &[])? {
            Answer::Ok(f) => self.freq_hz = bcd_to_freq(&f.data).unwrap_or(self.freq_hz),
            Answer::Ng => {}
            Answer::Silent => bail!(NO_RESPONSE),
        }
        if let Answer::Ok(f) = self.request(io, READ_MODE, &[], &[])? {
            self.mode = f.data.first().copied().or(self.mode);
        }
        if self.has_data {
            if let Answer::Ok(f) = self.request(io, EXTENDED, &[DATA_MODE], &[])? {
                self.data = f.data.get(1).is_some_and(|&d| d != 0);
            }
        }
        if self.has_tx {
            if let Answer::Ok(f) = self.request(io, TX_STATE, &[0x00], &[])? {
                self.tx = f.data.get(1) == Some(&1);
            }
        }
        let mut st = RigState { freq_hz: self.freq_hz, tx: self.tx, ..Default::default() };
        // Split transmits on the unselected VFO. Rigs that can't tell us that stay shown as not split.
        if self.has_unselected {
            if let Answer::Ok(f) = self.request(io, SPLIT, &[], &[])? {
                if f.data.first() == Some(&1) {
                    if let Answer::Ok(f) = self.request(io, VFO_FREQ, &[0x01], &[])? {
                        st.tx_freq_hz = f.data.get(1..).and_then(bcd_to_freq).unwrap_or(0);
                        st.split = st.tx_freq_hz > 0;
                    }
                }
            }
        }
        if let Some(mode) = self.mode {
            decode_mode(mode, self.data).apply(&mut st);
        }
        Ok(st)
    }

    fn command(&mut self, io: &mut SerialIo, cmd: &RigCommand, freq_hz: u64) -> Result<()> {
        match cmd {
            RigCommand::SetFreq(hz) => self.set(io, SET_FREQ, &[], &freq_to_bcd(*hz), "frequency"),
            RigCommand::SetMode(m) => {
                let Some(req) = ModeReq::from_adif(m, freq_hz) else { return Ok(()) };
                let (mode, data) = encode_mode(req);
                self.set(io, SET_MODE, &[], &[mode], "mode")?;
                // Setting the mode can leave data mode as it was, so set it explicitly. Filter 1 with data on.
                if self.has_data {
                    self.set(io, EXTENDED, &[DATA_MODE], &[data as u8, data as u8], "data mode")?;
                }
                Ok(())
            }
            RigCommand::SetSplit(Some(hz)) => {
                if !self.has_unselected {
                    bail!("this radio can't set the split transmit frequency over CI-V");
                }
                self.set(io, VFO_FREQ, &[0x01], &freq_to_bcd(*hz), "split frequency")?;
                self.set(io, SPLIT, &[], &[1], "split")
            }
            RigCommand::SetSplit(None) => self.set(io, SPLIT, &[], &[0], "split"),
        }
    }
}

fn encode_frame(to: u8, cmd: u8, data: &[u8]) -> Vec<u8> {
    let mut f = vec![PREAMBLE, PREAMBLE, to, CONTROLLER, cmd];
    f.extend_from_slice(data);
    f.push(END);
    f
}

/// Removes and returns the next complete frame in `buf`, dropping noise and collided frames.
fn next_frame(buf: &mut Vec<u8>) -> Option<Frame> {
    loop {
        let Some(start) = buf.windows(2).position(|w| w == [PREAMBLE, PREAMBLE]) else {
            // Keep a trailing preamble byte that may start the next frame.
            let keep = usize::from(buf.last() == Some(&PREAMBLE));
            buf.drain(..buf.len() - keep);
            return None;
        };
        buf.drain(..start);
        let end = buf.iter().position(|&b| b == END)?;
        let frame: Vec<u8> = buf.drain(..=end).collect();
        let body: Vec<u8> = frame[..end].iter().copied().skip_while(|&b| b == PREAMBLE).collect();
        if body.len() >= 3 && !body.contains(&COLLISION) {
            return Some(Frame { to: body[0], from: body[1], cmd: body[2], data: body[3..].to_vec() });
        }
    }
}

/// Decodes a little-endian BCD frequency (two digits per byte, 10 Hz/1 Hz first).
fn bcd_to_freq(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 6 {
        return None;
    }
    bytes.iter().rev().try_fold(0u64, |acc, &b| {
        let (hi, lo) = (b >> 4, b & 0x0F);
        (hi <= 9 && lo <= 9).then(|| acc * 100 + u64::from(hi * 10 + lo))
    })
}

fn freq_to_bcd(hz: u64) -> [u8; 5] {
    let mut out = [0u8; 5];
    let mut rest = hz.min(9_999_999_999);
    for b in &mut out {
        let two = (rest % 100) as u8;
        *b = (two / 10) << 4 | (two % 10);
        rest /= 100;
    }
    out
}

fn decode_mode(mode: u8, data: bool) -> ModeInfo {
    let (adif, name) = match mode {
        0x00 => ("SSB", "LSB"),
        0x01 => ("SSB", "USB"),
        0x02 => ("AM", "AM"),
        0x03 => ("CW", "CW"),
        0x04 => ("RTTY", "RTTY"),
        0x05 => ("FM", "FM"),
        0x06 => ("FM", "WFM"),
        0x07 => ("CW", "CW-R"),
        0x08 => ("RTTY", "RTTY-R"),
        0x11 => ("AM", "S-AM"),
        0x12 => return ModeInfo::data("PSK"),
        0x13 => return ModeInfo::data("PSK-R"),
        0x17 => ("", "DV"),
        0x22 => ("", "DD"),
        m => return ModeInfo::new("", format!("{m:02X}")),
    };
    if data && matches!(mode, 0x00 | 0x01 | 0x02 | 0x05) {
        ModeInfo::data(format!("{name}-D"))
    } else {
        ModeInfo::new(adif, name)
    }
}

/// The CI-V mode byte and data-mode flag for a request.
fn encode_mode(req: ModeReq) -> (u8, bool) {
    match req {
        ModeReq::Lsb => (0x00, false),
        ModeReq::Usb => (0x01, false),
        ModeReq::Am => (0x02, false),
        ModeReq::Cw => (0x03, false),
        ModeReq::Rtty => (0x04, false),
        ModeReq::Fm => (0x05, false),
        ModeReq::Data => (0x01, true),
    }
}

#[cfg(test)]
mod tests {
    use super::super::serial::fake::FakeLink;
    use super::*;

    #[test]
    fn bcd() {
        assert_eq!(freq_to_bcd(14_074_000), [0x00, 0x40, 0x07, 0x14, 0x00]);
        assert_eq!(freq_to_bcd(1_296_123_450), [0x50, 0x34, 0x12, 0x96, 0x12]);
        assert_eq!(bcd_to_freq(&[0x00, 0x40, 0x07, 0x14, 0x00]), Some(14_074_000));
        assert_eq!(bcd_to_freq(&[0x50, 0x34, 0x12, 0x96, 0x12]), Some(1_296_123_450));
        assert_eq!(bcd_to_freq(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x01]), Some(10_000_000_000));
        assert_eq!(bcd_to_freq(&[0x0A]), None);
        assert_eq!(bcd_to_freq(&[]), None);
    }

    #[test]
    fn frames() {
        assert_eq!(encode_frame(0x94, 0x03, &[]), [0xFE, 0xFE, 0x94, 0xE0, 0x03, 0xFD]);
        let mut buf = vec![0x12, 0xFE, 0xFE, 0x94, 0xE0, 0x03, 0xFD]; // noise + our echo
        buf.extend([0xFE, 0xFE, 0xFE, 0xE0, 0x94, 0x04, 0x01, 0x02, 0xFD]); // extra preamble byte
        buf.extend([0xFE, 0xFE, 0xE0, 0x94, 0xFC, 0xFD]); // collision
        buf.extend([0xFE, 0xFE, 0x00, 0x94, 0x00, 0x00]); // incomplete
        assert_eq!(next_frame(&mut buf), Some(Frame { to: 0x94, from: 0xE0, cmd: 0x03, data: vec![] }));
        assert_eq!(next_frame(&mut buf), Some(Frame { to: 0xE0, from: 0x94, cmd: 0x04, data: vec![0x01, 0x02] }));
        assert_eq!(next_frame(&mut buf), None);
        assert_eq!(buf, [0xFE, 0xFE, 0x00, 0x94, 0x00, 0x00]);
        buf.extend([0x40, 0x07, 0x14, 0x00, 0xFD]);
        assert_eq!(
            next_frame(&mut buf),
            Some(Frame { to: 0x00, from: 0x94, cmd: 0x00, data: vec![0x00, 0x40, 0x07, 0x14, 0x00] })
        );
        assert!(buf.is_empty());
        buf.extend([0x01, 0x02, 0xFE]);
        assert_eq!(next_frame(&mut buf), None);
        assert_eq!(buf, [0xFE]);
    }

    #[test]
    fn modes() {
        assert_eq!(decode_mode(0x01, false), ModeInfo::new("SSB", "USB"));
        assert_eq!(decode_mode(0x01, true), ModeInfo::data("USB-D"));
        assert_eq!(decode_mode(0x03, true), ModeInfo::new("CW", "CW"));
        assert_eq!(decode_mode(0x07, false), ModeInfo::new("CW", "CW-R"));
        assert_eq!(decode_mode(0x08, false), ModeInfo::new("RTTY", "RTTY-R"));
        assert_eq!(decode_mode(0x05, true), ModeInfo::data("FM-D"));
        assert_eq!(decode_mode(0x17, false), ModeInfo::new("", "DV"));
        assert_eq!(encode_mode(ModeReq::Data), (0x01, true));
        assert_eq!(encode_mode(ModeReq::Lsb), (0x00, false));
    }

    /// A fake IC-7300 at 0x94 that echoes every frame (single-wire CI-V) and answers it.
    fn fake_ic7300() -> FakeLink<impl FnMut(&[u8]) -> Vec<u8> + Send> {
        let (mut freq, mut mode, mut data) = ([0x00, 0x40, 0x07, 0x14, 0x00], 0x01u8, 1u8);
        let (mut vfo_b, mut split) = ([0u8; 5], 0u8);
        FakeLink::new(move |req: &[u8]| {
            let mut out = req.to_vec();
            let reply = |payload: &[u8]| [&[0xFE, 0xFE, 0xE0, 0x94][..], payload, &[0xFD]].concat();
            out.extend(match &req[4..req.len() - 1] {
                [0x03] => reply(&[&[0x03][..], &freq[..]].concat()),
                [0x04] => reply(&[0x04, mode, 0x01]),
                [0x1A, 0x06] => reply(&[0x1A, 0x06, data, data]),
                [0x1C, 0x00] => reply(&[0xFA]),
                [0x0F] => reply(&[0x0F, split]),
                [0x0F, on] => {
                    split = *on;
                    reply(&[0xFB])
                }
                [0x25, 0x01] => reply(&[&[0x25, 0x01][..], &vfo_b[..]].concat()),
                [0x25, 0x01, f @ ..] => {
                    vfo_b.copy_from_slice(f);
                    reply(&[0xFB])
                }
                [0x05, f @ ..] => {
                    freq.copy_from_slice(f);
                    reply(&[0xFB])
                }
                [0x06, m] => {
                    mode = *m;
                    reply(&[0xFB])
                }
                [0x1A, 0x06, d, _] => {
                    data = *d;
                    reply(&[0xFB])
                }
                _ => reply(&[0xFA]),
            });
            // An unsolicited transceive broadcast mixed in.
            out.extend([0xFE, 0xFE, 0x00, 0x94, 0x01, mode, 0x01, 0xFD]);
            out
        })
    }

    #[test]
    fn poll_and_command() {
        let link = fake_ic7300();
        let written = link.written.clone();
        let mut io = SerialIo::new(Box::new(link));
        let mut icom = Icom::new(0x94);
        icom.init(&mut io).unwrap();
        assert!(icom.has_data && !icom.has_tx);
        let st = icom.poll(&mut io).unwrap();
        assert_eq!((st.freq_hz, st.rig_mode.as_str(), st.data), (14_074_000, "USB-D", true));

        icom.command(&mut io, &RigCommand::SetFreq(7_030_000), st.freq_hz).unwrap();
        icom.command(&mut io, &RigCommand::SetMode("CW".into()), 7_030_000).unwrap();
        let st = icom.poll(&mut io).unwrap();
        assert_eq!((st.freq_hz, st.mode.as_str(), st.data), (7_030_000, "CW", false));
        let w = written.lock().unwrap();
        let has = |frame: &[u8]| w.windows(frame.len()).any(|x| x == frame);
        assert!(has(&[0xFE, 0xFE, 0x94, 0xE0, 0x05, 0x00, 0x00, 0x03, 0x07, 0x00, 0xFD]));
        assert!(has(&[0xFE, 0xFE, 0x94, 0xE0, 0x06, 0x03, 0xFD]));
        assert!(has(&[0xFE, 0xFE, 0x94, 0xE0, 0x1A, 0x06, 0x00, 0x00, 0xFD]));
        drop(w);

        // Split: VFO B (the unselected one) transmits.
        assert!(icom.has_unselected && !st.split);
        icom.command(&mut io, &RigCommand::SetSplit(Some(7_035_000)), 0).unwrap();
        let st = icom.poll(&mut io).unwrap();
        assert_eq!((st.freq_hz, st.split, st.tx_freq_hz), (7_030_000, true, 7_035_000));
        icom.command(&mut io, &RigCommand::SetSplit(None), 0).unwrap();
        assert!(!icom.poll(&mut io).unwrap().split);
        // A radio without the unselected-VFO command can't be given a split frequency.
        icom.has_unselected = false;
        assert!(icom.command(&mut io, &RigCommand::SetSplit(Some(7_035_000)), 0).is_err());

        // A silent radio fails to initialise.
        let mut rig = Icom::new(0x94);
        let mut io = SerialIo::new(Box::new(FakeLink::new(|_req: &[u8]| Vec::new())));
        assert!(rig.init(&mut io).unwrap_err().to_string().contains("no response"));
    }
}
