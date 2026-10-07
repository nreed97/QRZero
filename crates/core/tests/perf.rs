//! Large-log speed check. Run with:
//! `cargo test --release -p qrzero-core --test perf -- --ignored --nocapture`

use std::time::Instant;

use qrzero_core::model::*;
use qrzero_core::store::Sort;
use qrzero_core::Store;

const QSOS: usize = 200_000;

fn synthetic_adif(n: usize) -> String {
    let bands = [("160m", "1.830"), ("80m", "3.530"), ("40m", "7.030"), ("20m", "14.030"), ("15m", "21.030"), ("10m", "28.030")];
    let modes = ["CW", "SSB", "FT8", "RTTY"];
    let mut out = String::from("synthetic<EOH>\n");
    for i in 0..n {
        let call = format!("K{}{}{}", i % 10, (b'A' + (i / 10 % 26) as u8) as char, i / 260);
        let (band, freq) = bands[i % bands.len()];
        let mode = modes[i % modes.len()];
        let day = 1 + (i / 1440) % 28;
        let month = 1 + (i / (1440 * 28)) % 12;
        let year = 2000 + i / (1440 * 28 * 12);
        let date = format!("{year}{month:02}{day:02}");
        let time = format!("{:02}{:02}", (i / 60) % 24, i % 60);
        for (k, v) in [("CALL", call.as_str()), ("QSO_DATE", &date), ("TIME_ON", &time), ("BAND", band), ("FREQ", freq), ("MODE", mode), ("RST_SENT", "599"), ("RST_RCVD", "599"), ("NAME", "Operator Name"), ("COMMENT", "synthetic test QSO"), ("STATION_CALLSIGN", "N0CALL")] {
            out.push_str(&format!("<{k}:{}>{v} ", v.len()));
        }
        out.push_str("<EOR>\n");
    }
    out
}

#[test]
#[ignore]
fn large_log_speed() {
    let dir = tempfile::tempdir().unwrap();
    let path = std::env::var("QRZERO_PERF_DB").map(Into::into).unwrap_or_else(|_| dir.path().join("perf.db"));
    let _ = std::fs::remove_file(&path);
    let mut st = Store::open(&path).unwrap();
    let log = st.create_log("Perf").unwrap().id;
    let adi = synthetic_adif(QSOS);

    let t = Instant::now();
    let r = st.import_adif(log, adi.as_bytes(), &ImportOptions { skip_duplicates: true, ..Default::default() }).unwrap();
    println!("import {} QSOs ({} MB): {:?}", r.imported, adi.len() / 1_000_000, t.elapsed());
    assert_eq!(r.imported, QSOS);

    let timed = |label: &str, f: &dyn Fn() -> i64| {
        let t = Instant::now();
        let n = f();
        let e = t.elapsed();
        println!("{label}: {n} rows in {e:?}");
        e
    };
    timed("first query after import", &|| st.search(log, &QsoFilter::default(), Sort::Newest, 0, 200).unwrap().0);
    let page = timed("newest page of 200", &|| st.search(log, &QsoFilter::default(), Sort::Newest, 0, 200).unwrap().0);
    let call = timed("call prefix K5Q", &|| st.search(log, &QsoFilter { call: Some("K5Q".into()), ..Default::default() }, Sort::Newest, 0, 200).unwrap().0);
    let band = timed("20m CW", &|| st.search(log, &QsoFilter { bands: vec!["20m".into()], modes: vec!["CW".into()], ..Default::default() }, Sort::Newest, 0, 200).unwrap().0);
    let band_only = timed("all 20m", &|| st.search(log, &QsoFilter { bands: vec!["20m".into()], ..Default::default() }, Sort::Newest, 0, 200).unwrap().0);
    let wb = timed("worked before", &|| st.worked_before(log, "K5Q100", Some(291)).unwrap().call_count);
    let t = Instant::now();
    let (_, n) = st.export_adif(log, &QsoFilter::default(), ExportProfile::Full, "perf").unwrap();
    println!("export {n} QSOs: {:?}", t.elapsed());

    for e in [page, call, band, band_only, wb] {
        assert!(e.as_millis() < 100, "interactive query too slow: {e:?}");
    }
}
