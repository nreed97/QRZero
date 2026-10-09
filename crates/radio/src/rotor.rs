//! Direct rotator control: Hamlib `rotctld` (TCP) and Yaesu GS-232 (serial or TCP, which is also what
//! most network rotator boxes speak). One background thread per connection polls the heading and
//! sends turn and stop commands.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

/// rotctld's default TCP port.
pub const ROTCTLD_PORT: u16 = 4533;

const POLL: Duration = Duration::from_secs(1);
const READ_TIMEOUT: Duration = Duration::from_millis(1500);
const RETRY: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// Hamlib `rotctld`: `p` / `P az el` / `S`.
    Rotctld,
    /// Yaesu GS-232A/B: `C` / `Maaa` / `S`.
    Gs232,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// `host:port`.
    Tcp(String),
    Serial { path: String, baud: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub protocol: Protocol,
    pub link: Link,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Turn(f64),
    Stop,
}

/// What the connection thread reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Heading(f64),
    /// Connected, or why it isn't.
    Status(String),
}

fn norm(az: f64) -> u32 {
    (az.round().rem_euclid(360.0) as u32) % 360
}

/// Bytes to send for a command.
pub fn encode(p: Protocol, cmd: Cmd) -> String {
    match (p, cmd) {
        (Protocol::Rotctld, Cmd::Turn(az)) => format!("P {} 0\n", norm(az)),
        (Protocol::Rotctld, Cmd::Stop) => "S\n".into(),
        (Protocol::Gs232, Cmd::Turn(az)) => format!("M{:03}\r", norm(az)),
        (Protocol::Gs232, Cmd::Stop) => "S\r".into(),
    }
}

/// The heading query.
pub fn poll_command(p: Protocol) -> &'static str {
    match p {
        Protocol::Rotctld => "p\n",
        Protocol::Gs232 => "C\r",
    }
}

/// Azimuth from a GS-232 reply: `+0123`, `+0123+0045` (B) or `AZ=123 EL=045`.
pub fn parse_gs232(line: &str) -> Option<f64> {
    let l = line.trim();
    let digits = if let Some(i) = l.find("AZ=") {
        &l[i + 3..]
    } else {
        l.strip_prefix("+0")?
    };
    let n: String = digits.chars().take_while(|c| c.is_ascii_digit()).collect();
    let az: f64 = n.parse().ok()?;
    (n.len() >= 3 && az <= 450.0).then_some(az)
}

fn read_line(io: &mut dyn Read) -> std::io::Result<String> {
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    loop {
        match io.read(&mut b) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(_) => {
                if b[0] == b'\n' || b[0] == b'\r' {
                    if out.is_empty() {
                        continue;
                    }
                    return Ok(String::from_utf8_lossy(&out).into_owned());
                }
                out.push(b[0]);
                if out.len() > 200 {
                    return Err(ErrorKind::InvalidData.into());
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

trait Io: Read + Write + Send {}
impl<T: Read + Write + Send> Io for T {}

fn open(link: &Link) -> Result<Box<dyn Io>, String> {
    match link {
        Link::Tcp(addr) => {
            let sa = addr
                .to_socket_addrs()
                .map_err(|e| format!("{addr}: {e}"))?
                .next()
                .ok_or_else(|| format!("{addr}: no such address"))?;
            let s = TcpStream::connect_timeout(&sa, Duration::from_secs(3)).map_err(|e| format!("{addr}: {e}"))?;
            s.set_read_timeout(Some(READ_TIMEOUT)).map_err(|e| e.to_string())?;
            let _ = s.set_nodelay(true);
            Ok(Box::new(s))
        }
        Link::Serial { path, baud } => {
            let p = serialport::new(path, *baud).timeout(READ_TIMEOUT).open().map_err(|e| format!("{path}: {e}"))?;
            Ok(Box::new(p))
        }
    }
}

fn poll(io: &mut dyn Io, p: Protocol) -> std::io::Result<Option<f64>> {
    io.write_all(poll_command(p).as_bytes())?;
    io.flush()?;
    // A few lines may be left over from earlier commands (RPRT 0, blank replies): look through them.
    for _ in 0..4 {
        let line = read_line(io)?;
        match p {
            Protocol::Rotctld => {
                if line.starts_with("RPRT") {
                    if line.trim() == "RPRT 0" {
                        continue;
                    }
                    return Ok(None);
                }
                let az: f64 = line.trim().parse().map_err(|_| ErrorKind::InvalidData)?;
                // The second line is the elevation; leave it for the next poll's cleanup.
                let _ = read_line(io);
                return Ok(az.is_finite().then_some(az.rem_euclid(360.0)));
            }
            Protocol::Gs232 => {
                if let Some(az) = parse_gs232(&line) {
                    return Ok(Some(az % 360.0));
                }
            }
        }
    }
    Ok(None)
}

/// Starts the connection thread. It reconnects on its own and ends when the returned sender is dropped.
pub fn spawn(cfg: Config, on_event: impl Fn(Event) + Send + 'static) -> Sender<Cmd> {
    let (tx, rx) = mpsc::channel::<Cmd>();
    std::thread::Builder::new()
        .name("rotator".into())
        .spawn(move || {
            'outer: loop {
                let mut io = match open(&cfg.link) {
                    Ok(io) => io,
                    Err(e) => {
                        on_event(Event::Status(format!("can't connect to {e}")));
                        match rx.recv_timeout(RETRY) {
                            Err(RecvTimeoutError::Disconnected) => return,
                            _ => continue 'outer,
                        }
                    }
                };
                on_event(Event::Status("connected".into()));
                let mut silent = 0;
                loop {
                    match poll(io.as_mut(), cfg.protocol) {
                        Ok(Some(az)) => {
                            silent = 0;
                            on_event(Event::Heading(az));
                        }
                        Ok(None) => {}
                        Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                            silent += 1;
                            if silent == 3 {
                                on_event(Event::Status("connected, but the rotator isn't answering".into()));
                            }
                        }
                        Err(e) => {
                            on_event(Event::Status(format!("connection lost: {e}")));
                            continue 'outer;
                        }
                    }
                    // Wait for the next poll; a command goes out at once.
                    let mut wait = POLL;
                    loop {
                        match rx.recv_timeout(wait) {
                            Ok(cmd) => {
                                if io.write_all(encode(cfg.protocol, cmd).as_bytes()).and_then(|_| io.flush()).is_err() {
                                    on_event(Event::Status("connection lost".into()));
                                    continue 'outer;
                                }
                                wait = Duration::from_millis(300);
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
            }
        })
        .ok();
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[test]
    fn encoding() {
        assert_eq!(encode(Protocol::Rotctld, Cmd::Turn(123.4)), "P 123 0\n");
        assert_eq!(encode(Protocol::Rotctld, Cmd::Turn(-90.0)), "P 270 0\n");
        assert_eq!(encode(Protocol::Gs232, Cmd::Turn(5.0)), "M005\r");
        assert_eq!(encode(Protocol::Gs232, Cmd::Turn(359.6)), "M000\r");
        assert_eq!(encode(Protocol::Gs232, Cmd::Stop), "S\r");
        assert_eq!(encode(Protocol::Rotctld, Cmd::Stop), "S\n");
    }

    #[test]
    fn gs232_replies() {
        assert_eq!(parse_gs232("+0123"), Some(123.0));
        assert_eq!(parse_gs232("+0270+0045"), Some(270.0));
        assert_eq!(parse_gs232("AZ=045 EL=010"), Some(45.0));
        assert_eq!(parse_gs232("?>"), None);
        assert_eq!(parse_gs232("+0"), None);
    }

    /// A fake rotator: `rotctld` or GS-232 over TCP. Records the commands it gets.
    fn fake(p: Protocol) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut w = s.try_clone().unwrap();
            let mut r = BufReader::new(s);
            let mut az = 90u32;
            let sep = if p == Protocol::Rotctld { b'\n' } else { b'\r' };
            loop {
                let mut buf = Vec::new();
                if r.read_until(sep, &mut buf).unwrap_or(0) == 0 {
                    return;
                }
                let cmd = String::from_utf8_lossy(&buf).trim().to_string();
                log.lock().unwrap().push(cmd.clone());
                let reply = match (p, cmd.as_str()) {
                    (Protocol::Rotctld, "p") => format!("{az}.000000\n0.000000\n"),
                    (Protocol::Rotctld, c) if c.starts_with("P ") => {
                        az = c.split(' ').nth(1).unwrap().parse().unwrap();
                        "RPRT 0\n".into()
                    }
                    (Protocol::Gs232, "C") => format!("+0{az:03}\r\n"),
                    (Protocol::Gs232, c) if c.starts_with('M') => {
                        az = c[1..].parse().unwrap();
                        String::new()
                    }
                    _ => String::new(),
                };
                let _ = w.write_all(reply.as_bytes());
            }
        });
        (addr, seen)
    }

    fn run(p: Protocol) {
        let (addr, seen) = fake(p);
        let (ev_tx, ev_rx) = mpsc::channel();
        let tx = spawn(Config { protocol: p, link: Link::Tcp(addr) }, move |e| {
            let _ = ev_tx.send(e);
        });
        let wait_heading = |want: f64| {
            for _ in 0..40 {
                if let Ok(Event::Heading(a)) = ev_rx.recv_timeout(Duration::from_millis(500)) {
                    if a == want {
                        return true;
                    }
                }
            }
            false
        };
        assert!(wait_heading(90.0));
        tx.send(Cmd::Turn(200.0)).unwrap();
        assert!(wait_heading(200.0), "{:?}", seen.lock().unwrap());
    }

    #[test]
    fn rotctld_polls_and_turns() {
        run(Protocol::Rotctld);
    }

    #[test]
    fn gs232_polls_and_turns() {
        run(Protocol::Gs232);
    }
}
