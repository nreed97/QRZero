//! Shared plumbing for serial CAT backends: a blocking poll loop on its own thread and
//! request/response helpers for `;`-terminated ASCII protocols.

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use tokio::sync::mpsc::{self, error::TryRecvError};

use super::{publish_single, CmdError, CmdRx, RigCommand, RigState, StateTx, POLL};

/// Serial read timeout; bounds how long a blocking read stalls the loop.
const READ_TIMEOUT: Duration = Duration::from_millis(50);
/// How long to wait for a query's reply.
pub(super) const REPLY_TIMEOUT: Duration = Duration::from_millis(500);
/// How long to listen for an error reply after a set command.
const SET_WINDOW: Duration = Duration::from_millis(100);
const MAX_BUF: usize = 4096;

/// One serial rig protocol, driven synchronously by [`serve`].
pub(super) trait Protocol: Send + 'static {
    /// Whether to drop DTR and RTS after opening; USB CAT ports often use them to key PTT or CW.
    const CLEAR_LINES: bool = true;

    /// Probes the radio once after the port opens.
    fn init(&mut self, _io: &mut SerialIo) -> Result<()> {
        Ok(())
    }

    /// Reads the current state. An error drops the connection.
    fn poll(&mut self, io: &mut SerialIo) -> Result<RigState>;

    /// Executes a command; `freq_hz` is the last polled frequency. Errors are shown, not fatal.
    fn command(&mut self, io: &mut SerialIo, cmd: &RigCommand, freq_hz: u64) -> Result<()>;
}

/// Runs `proto` on a blocking thread until the port fails or the command channel closes.
pub(super) async fn spawn<P: Protocol>(path: String, baud: u32, proto: P, state: &StateTx, cmds: &mut CmdRx) -> Result<()> {
    let state = state.clone();
    // The thread owns the receiver while it runs and hands it back afterwards.
    let rx = std::mem::replace(cmds, mpsc::unbounded_channel().1);
    let (rx, result) = tokio::task::spawn_blocking(move || {
        let mut rx = rx;
        let result = open(&path, baud, P::CLEAR_LINES).and_then(|mut io| serve(&mut io, proto, &state, &mut rx));
        (rx, result.with_context(|| path.clone()))
    })
    .await
    .context("serial thread failed")?;
    *cmds = rx;
    result
}

fn open(path: &str, baud: u32, clear_lines: bool) -> Result<SerialIo> {
    let mut port = serialport::new(path, baud).timeout(READ_TIMEOUT).open()?;
    if clear_lines {
        let _ = port.write_data_terminal_ready(false);
        let _ = port.write_request_to_send(false);
    }
    Ok(SerialIo::new(Box::new(port)))
}

/// The blocking poll loop: run queued commands, poll, publish, wait for the next poll or command.
pub(super) fn serve<P: Protocol>(io: &mut SerialIo, mut proto: P, state: &StateTx, cmds: &mut CmdRx) -> Result<()> {
    proto.init(io)?;
    let mut cmd_err = CmdError::default();
    let mut freq_hz = 0;
    loop {
        loop {
            match cmds.try_recv() {
                Ok((0, cmd)) => cmd_err.record(proto.command(io, &cmd, freq_hz)),
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        let mut st = proto.poll(io)?;
        st.connected = true;
        st.error = cmd_err.get();
        freq_hz = st.freq_hz;
        publish_single(state, st);
        let next = Instant::now() + POLL;
        while Instant::now() < next && cmds.is_empty() {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Anything that moves bytes like a serial port (a real port, or a fake radio in tests).
pub(super) trait Link: Read + Write + Send {}
impl<T: Read + Write + Send> Link for T {}

/// Reply to an ASCII query.
#[derive(Debug, PartialEq)]
pub(super) enum Reply {
    /// The reply token, including its prefix and `;`.
    Value(String),
    /// The radio answered `?;` (or Kenwood's `E;`/`O;`): unsupported or busy.
    Rejected,
    /// Nothing within [`REPLY_TIMEOUT`].
    Silent,
}

impl Reply {
    pub(super) fn value(self) -> Option<String> {
        match self {
            Reply::Value(v) => Some(v),
            _ => None,
        }
    }
}

/// A serial link with a receive buffer.
pub(super) struct SerialIo {
    link: Box<dyn Link>,
    pub(super) buf: Vec<u8>,
}

impl SerialIo {
    pub(super) fn new(link: Box<dyn Link>) -> Self {
        SerialIo { link, buf: Vec::new() }
    }

    pub(super) fn write(&mut self, data: &[u8]) -> Result<()> {
        self.link.write_all(data)?;
        self.link.flush()?;
        Ok(())
    }

    /// Appends whatever arrives within the read timeout to `buf`; false if nothing came.
    pub(super) fn fill(&mut self) -> Result<bool> {
        let mut chunk = [0u8; 256];
        match self.link.read(&mut chunk) {
            Ok(n) => {
                if self.buf.len() + n > MAX_BUF {
                    self.buf.clear();
                }
                self.buf.extend_from_slice(&chunk[..n]);
                Ok(n > 0)
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// The next `;`-terminated token, or None if none completes before `deadline`.
    fn token(&mut self, deadline: Instant) -> Result<Option<String>> {
        loop {
            if let Some(i) = self.buf.iter().position(|&b| b == b';') {
                let tok: Vec<u8> = self.buf.drain(..=i).collect();
                return Ok(Some(String::from_utf8_lossy(&tok).trim().to_string()));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            self.fill()?;
        }
    }

    /// Sends an ASCII query such as `FA;` and waits for the token starting with `prefix`,
    /// skipping unrelated (e.g. auto-information) tokens.
    pub(super) fn ask(&mut self, cmd: &str, prefix: &str) -> Result<Reply> {
        self.write(cmd.as_bytes())?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        while let Some(tok) = self.token(deadline)? {
            if is_error_token(&tok) {
                return Ok(Reply::Rejected);
            }
            if tok.starts_with(prefix) {
                return Ok(Reply::Value(tok));
            }
        }
        Ok(Reply::Silent)
    }

    /// Sends ASCII set commands (which have no reply) and fails if the radio answers with an error.
    pub(super) fn set(&mut self, cmd: &str) -> Result<()> {
        self.write(cmd.as_bytes())?;
        let deadline = Instant::now() + SET_WINDOW;
        while let Some(tok) = self.token(deadline)? {
            if is_error_token(&tok) {
                bail!("radio rejected {cmd}");
            }
        }
        Ok(())
    }
}

fn is_error_token(tok: &str) -> bool {
    matches!(tok, "?;" | "E;" | "O;")
}

/// Parses the digits of an ASCII CAT frequency field.
pub(super) fn parse_digits(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten()
}

#[cfg(test)]
pub(super) mod fake {
    use std::collections::VecDeque;
    use std::io::{self, Read, Write};
    use std::sync::{Arc, Mutex};

    /// A fake radio: every write is passed to `respond`, whose output becomes readable.
    pub struct FakeLink<F> {
        respond: F,
        pending: VecDeque<u8>,
        pub written: Arc<Mutex<Vec<u8>>>,
    }

    impl<F: FnMut(&[u8]) -> Vec<u8> + Send> FakeLink<F> {
        pub fn new(respond: F) -> Self {
            FakeLink { respond, pending: VecDeque::new(), written: Default::default() }
        }
    }

    impl<F: FnMut(&[u8]) -> Vec<u8> + Send> Read for FakeLink<F> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pending.is_empty() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let n = buf.len().min(self.pending.len());
            for (dst, src) in buf.iter_mut().zip(self.pending.drain(..n)) {
                *dst = src;
            }
            Ok(n)
        }
    }

    impl<F: FnMut(&[u8]) -> Vec<u8> + Send> Write for FakeLink<F> {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.written.lock().unwrap().extend_from_slice(data);
            let reply = (self.respond)(data);
            self.pending.extend(reply);
            Ok(data.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A fake ASCII CAT radio answering each `;`-terminated command via `answer`.
    pub fn ascii(mut answer: impl FnMut(&str) -> Option<String> + Send) -> FakeLink<impl FnMut(&[u8]) -> Vec<u8> + Send> {
        FakeLink::new(move |data: &[u8]| {
            let text = String::from_utf8_lossy(data);
            text.split_inclusive(';').filter_map(&mut answer).collect::<String>().into_bytes()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_ask_and_set() {
        let link = fake::ascii(|cmd| match cmd {
            "FA;" => Some("IF00000000000;FA14074000;FB00007000000;".into()),
            "XX;" => Some("?;".into()),
            "FA1;" => Some("?;".into()),
            _ => None,
        });
        let mut io = SerialIo::new(Box::new(link));
        assert_eq!(io.ask("FA;", "FA").unwrap(), Reply::Value("FA14074000;".into()));
        assert_eq!(io.ask("XX;", "XX").unwrap(), Reply::Rejected);
        assert_eq!(io.ask("ZZ;", "ZZ").unwrap(), Reply::Silent);
        assert!(io.set("FA1;").is_err());
        assert!(io.set("FA00014074000;").is_ok());
    }

    #[test]
    fn digits() {
        assert_eq!(parse_digits("00014074000"), Some(14_074_000));
        assert_eq!(parse_digits(""), None);
        assert_eq!(parse_digits("+0100"), None);
    }
}
