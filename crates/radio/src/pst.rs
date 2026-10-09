//! PstRotatorAz UDP control. Commands go to its UDP port (default [`DEFAULT_PORT`]); it replies
//! to port + 1.

/// PstRotatorAz's default UDP command port.
pub const DEFAULT_PORT: u16 = 12000;

/// Command to turn the rotator to `az` degrees (rounded, normalised to 0..360).
pub fn set_azimuth(az: f64) -> String {
    let deg = az.round().rem_euclid(360.0) as u32;
    format!("<PST><AZIMUTH>{deg}</AZIMUTH></PST>")
}

/// Command to stop the rotator.
pub fn stop() -> String {
    "<PST><STOP>1</STOP></PST>".to_owned()
}

/// Command asking for the current azimuth; answered with `AZ:<degrees>`.
pub fn query() -> String {
    "<PST>AZ?</PST>".to_owned()
}

/// Parses an azimuth reply such as `AZ:123`.
pub fn parse_reply(s: &str) -> Option<f64> {
    let v: f64 = s.trim().strip_prefix("AZ:")?.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

/// What a program sending PstRotatorAz-style UDP (N1MM, DXLab, ...) asks of the rotator.
#[derive(Debug, PartialEq)]
pub enum Request {
    Turn(f64),
    Stop,
    /// Asks for the heading; answer with [`reply`].
    Query,
}

/// Answer to [`Request::Query`].
pub fn reply(az: f64) -> String {
    format!("AZ:{}", az.round().rem_euclid(360.0) as u32)
}

/// Reads a request: `<PST><AZIMUTH>n</AZIMUTH></PST>`, `<PST><STOP>1</STOP></PST>`, `<PST>AZ?</PST>`,
/// or just a bearing (`123`, `AZ123`, GS-232 `M123`, `S`).
pub fn parse_request(s: &str) -> Option<Request> {
    let t = s.trim();
    let number = |v: &str| v.trim().parse::<f64>().ok().filter(|a| a.is_finite()).map(|a| Request::Turn(a.rem_euclid(360.0)));
    if let Some(i) = t.find("<AZIMUTH>") {
        let rest = &t[i + 9..];
        return number(&rest[..rest.find('<')?]);
    }
    if t.contains("<STOP>") {
        return Some(Request::Stop);
    }
    if t.contains("AZ?") {
        return Some(Request::Query);
    }
    if t.eq_ignore_ascii_case("s") {
        return Some(Request::Stop);
    }
    let bare = t.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == ':' || c == '=');
    if bare.len() != t.len() && !t.starts_with(['M', 'm', 'A', 'a']) {
        return None;
    }
    number(bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(set_azimuth(123.4), "<PST><AZIMUTH>123</AZIMUTH></PST>");
        assert_eq!(set_azimuth(359.6), "<PST><AZIMUTH>0</AZIMUTH></PST>");
        assert_eq!(set_azimuth(-90.0), "<PST><AZIMUTH>270</AZIMUTH></PST>");
        assert_eq!(set_azimuth(725.0), "<PST><AZIMUTH>5</AZIMUTH></PST>");
        assert_eq!(stop(), "<PST><STOP>1</STOP></PST>");
        assert_eq!(query(), "<PST>AZ?</PST>");
    }

    #[test]
    fn requests() {
        assert_eq!(parse_request("<PST><AZIMUTH>123</AZIMUTH></PST>"), Some(Request::Turn(123.0)));
        assert_eq!(parse_request("<PST><AZIMUTH>370</AZIMUTH></PST>"), Some(Request::Turn(10.0)));
        assert_eq!(parse_request("<PST><STOP>1</STOP></PST>"), Some(Request::Stop));
        assert_eq!(parse_request("<PST>AZ?</PST>"), Some(Request::Query));
        assert_eq!(parse_request("45\r\n"), Some(Request::Turn(45.0)));
        assert_eq!(parse_request("M090\r"), Some(Request::Turn(90.0)));
        assert_eq!(parse_request("AZ:200"), Some(Request::Turn(200.0)));
        assert_eq!(parse_request("S"), Some(Request::Stop));
        assert_eq!(parse_request("hello"), None);
        assert_eq!(parse_request("<PST><AZIMUTH>x</AZIMUTH></PST>"), None);
        assert_eq!(reply(359.6), "AZ:0");
    }

    #[test]
    fn replies() {
        assert_eq!(parse_reply("AZ:123"), Some(123.0));
        assert_eq!(parse_reply(" AZ:45.5\r\n"), Some(45.5));
        assert_eq!(parse_reply("EL:10"), None);
        assert_eq!(parse_reply("AZ:"), None);
        assert_eq!(parse_reply("AZ:NaN"), None);
    }
}
