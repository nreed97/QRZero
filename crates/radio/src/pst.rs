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
    fn replies() {
        assert_eq!(parse_reply("AZ:123"), Some(123.0));
        assert_eq!(parse_reply(" AZ:45.5\r\n"), Some(45.5));
        assert_eq!(parse_reply("EL:10"), None);
        assert_eq!(parse_reply("AZ:"), None);
        assert_eq!(parse_reply("AZ:NaN"), None);
    }
}
