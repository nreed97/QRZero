//! LoTW / eQSL confirmation clients against a local mock server, and QSO matching.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::Router;
use qrzero_core::adif::Fields;
use qrzero_core::confirm::*;
use qrzero_core::qsl::{QslError, Upload};

type Seen = Arc<Mutex<Vec<HashMap<String, String>>>>;

/// Serves `app` on a random local port; returns its base URL.
async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// A GET route at `path` replying with `reply(query)`, recording each query.
async fn mock_get(path: &str, reply: fn(&HashMap<String, String>) -> String) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let app = Router::new()
        .route(
            path,
            get(move |State(seen): State<Seen>, Query(q): Query<HashMap<String, String>>| async move {
                let body = reply(&q);
                seen.lock().unwrap().push(q);
                body
            }),
        )
        .route(
            "/downloadedfiles/{name}",
            get(|axum::extract::Path(name): axum::extract::Path<String>| async move {
                assert_eq!(name, "K1ABC_abc123.adi");
                EQSL_ADI
            }),
        )
        .with_state(seen.clone());
    (format!("{}{path}", serve(app).await), seen)
}

fn f(pairs: &[(&str, &str)]) -> Fields {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

const LOTW_ADI: &str = "ARRL Logbook of the World Status Report\n\
<PROGRAMID:4>LoTW\n<APP_LoTW_LASTQSL:19>2026-10-01 12:34:56\n<APP_LoTW_NUMREC:1>2\n<eoh>\n\
<CALL:4>W1AW <BAND:3>20M <MODE:2>CW <QSO_DATE:8>20260930 <TIME_ON:6>120300 <QSL_RCVD:1>Y <QSLRDATE:8>20261001 <DXCC:3>291 <STATE:2>CT <eor>\n\
<CALL:5>DL1AB <BAND:3>40M <MODE:3>FT8 <QSO_DATE:8>20260929 <TIME_ON:4>0815 <QSL_RCVD:1>Y <QSLRDATE:8>20261001 <eor>\n";

fn lotw_reply(q: &HashMap<String, String>) -> String {
    if q["password"] != "p&ss word" {
        return "<html><body>Username/password incorrect</body></html>".into();
    }
    LOTW_ADI.into()
}

#[tokio::test]
async fn lotw_report() {
    let (url, seen) = mock_get("/lotwuser/lotwreport.adi", lotw_reply).await;
    let r = lotw_confirmations(&url, "k1abc", "p&ss word", Some("K1ABC"), "2026-09-01").await.unwrap();
    assert_eq!(r.last_qsl.as_deref(), Some("2026-10-01 12:34:56"));
    assert_eq!(r.records.len(), 2);
    assert_eq!(r.records[0]["CALL"], "W1AW");
    assert_eq!(r.records[1]["MODE"], "FT8");
    let q = seen.lock().unwrap()[0].clone();
    for (k, v) in [
        ("login", "k1abc"),
        ("qso_query", "1"),
        ("qso_qsl", "yes"),
        ("qso_qsldetail", "yes"),
        ("qso_withown", "yes"),
        ("qso_qslsince", "2026-09-01"),
        ("qso_owncall", "K1ABC"),
    ] {
        assert_eq!(q[k], v, "{k}");
    }
}

#[tokio::test]
async fn lotw_omits_empty_params_and_rejects_bad_login() {
    let (url, seen) = mock_get("/lotw", lotw_reply).await;
    let err = lotw_confirmations(&url, "k1abc", "wrong", None, "").await.unwrap_err();
    assert_eq!(err, QslError::Auth("LoTW refused the username or password".into()));
    let q = seen.lock().unwrap()[0].clone();
    assert!(!q.contains_key("qso_qslsince") && !q.contains_key("qso_owncall"));
}

#[tokio::test]
async fn lotw_other_html_is_service_error() {
    let (url, _) = mock_get("/lotw", |_| "<html><body>Site maintenance</body></html>".into()).await;
    assert_eq!(
        lotw_confirmations(&url, "a", "b", None, "").await,
        Err(QslError::Service("LoTW: Site maintenance".into()))
    );
}

const EQSL_ADI: &str = "eQSL download <PROGRAMID:4>eQSL <EOH>\n\
<CALL:5>JA1XX <QSO_DATE:8>20260915 <TIME_ON:4>1100 <BAND:3>15M <MODE:3>SSB <QSL_SENT:1>Y <GRIDSQUARE:4>PM95 <EOR>\n\
<CALL:4>G4AB <QSO_DATE:8>20260916 <TIME_ON:4>1300 <BAND:3>20M <MODE:4>MFSK <SUBMODE:3>FT4 <EOR>\n";

fn eqsl_reply(q: &HashMap<String, String>) -> String {
    match q["Password"].as_str() {
        "good" => "<HTML><BODY>Your file is ready. <A HREF=\"../downloadedfiles/K1ABC_abc123.adi\">.ADI file</A></BODY></HTML>".into(),
        "empty" => "<HTML><BODY>Error: You have no log entries for this period</BODY></HTML>".into(),
        "odd" => "<HTML><BODY><P>Something   unusual\n happened</P></BODY></HTML>".into(),
        _ => "<HTML><BODY>Error: No such Username/Password found</BODY></HTML>".into(),
    }
}

#[tokio::test]
async fn eqsl_inbox_two_step_download() {
    let (url, seen) = mock_get("/qslcard/DownloadInBox.cfm", eqsl_reply).await;
    let recs = eqsl_confirmations(&url, "K1ABC", "good", Some("Home"), "2026-09-01").await.unwrap();
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0]["CALL"], "JA1XX");
    assert_eq!(recs[1]["SUBMODE"], "FT4");
    let q = seen.lock().unwrap()[0].clone();
    assert_eq!((q["UserName"].as_str(), q["RcvdSince"].as_str(), q["QTHNickname"].as_str()), ("K1ABC", "20260901", "Home"));
}

#[tokio::test]
async fn eqsl_inbox_failures() {
    let (url, seen) = mock_get("/qslcard/DownloadInBox.cfm", eqsl_reply).await;
    assert!(matches!(eqsl_confirmations(&url, "K1ABC", "bad", None, "").await, Err(QslError::Auth(_))));
    let q = seen.lock().unwrap()[0].clone();
    assert!(!q.contains_key("RcvdSince") && !q.contains_key("QTHNickname"));
    assert_eq!(eqsl_confirmations(&url, "K1ABC", "empty", Some(""), "").await, Ok(vec![]));
    assert_eq!(
        eqsl_confirmations(&url, "K1ABC", "odd", None, "").await,
        Err(QslError::Service("eQSL: Something unusual happened".into()))
    );
}

#[tokio::test]
async fn eqsl_network_error() {
    assert!(matches!(eqsl_confirmations("http://127.0.0.1:1/x", "a", "b", None, "").await, Err(QslError::Network(_))));
}

fn upload_reply(form: &HashMap<String, String>) -> String {
    let adif = &form["ADIFData"];
    if !adif.contains("<EQSL_PSWD:4>good ") {
        return "<HTML><BODY>Result: 0 out of 1 records added<BR>Error: No match on eQSL_User/eQSL_Pswd</BODY></HTML>".into();
    }
    if adif.contains("<CALL:4>DUPE") {
        "<HTML><BODY>Result: 0 out of 1 records added<BR>Warning: Y=2026 M=9 D=1 DUPE 20M CW Duplicate</BODY></HTML>".into()
    } else if adif.contains("<CALL:3>BAD") {
        "<HTML><BODY>Result: 0 out of 1 records added<BR>Warning: Y=2026 Bad record: Invalid band</BODY></HTML>".into()
    } else {
        "<HTML><BODY>Result: 1 out of 1 records added<BR></BODY></HTML>".into()
    }
}

#[tokio::test]
async fn eqsl_upload_results() {
    let seen: Seen = Arc::default();
    let app = Router::new()
        .route(
            "/",
            post(|State(seen): State<Seen>, axum::Form(form): axum::Form<HashMap<String, String>>| async move {
                let body = upload_reply(&form);
                seen.lock().unwrap().push(form);
                body
            }),
        )
        .with_state(seen.clone());
    let url = format!("{}/", serve(app).await);
    let qso = |call: &str| f(&[("CALL", call), ("QSO_DATE", "20260901"), ("TIME_ON", "1200"), ("BAND", "20M"), ("MODE", "CW")]);

    assert_eq!(eqsl_upload(&url, "K1ABC", "good", Some("Home QTH"), &qso("W1AW")).await, Ok(Upload::Added));
    assert_eq!(eqsl_upload(&url, "K1ABC", "good", None, &qso("DUPE")).await, Ok(Upload::Duplicate));
    assert_eq!(
        eqsl_upload(&url, "K1ABC", "good", None, &qso("BAD")).await,
        Ok(Upload::Rejected("Warning: Y=2026 Bad record: Invalid band".into()))
    );
    assert!(matches!(eqsl_upload(&url, "K1ABC", "nope", None, &qso("W1AW")).await, Err(QslError::Auth(_))));

    let adif = seen.lock().unwrap()[0]["ADIFData"].clone();
    assert!(adif.starts_with("QRZero upload <PROGRAMID:6>QRZero <EQSL_USER:5>K1ABC <EQSL_PSWD:4>good <EOH>"), "{adif}");
    assert!(adif.contains("<CALL:4>W1AW") && adif.contains("<APP_EQSL_QTH_NICKNAME:8>Home QTH <EOR>"), "{adif}");
    let parsed = qrzero_core::adif::parse(adif.as_bytes());
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0]["APP_EQSL_QTH_NICKNAME"], "Home QTH");
}

fn q(call: &str, band: &str, mode: &str, date: &str, time: &str) -> Fields {
    let mut r = f(&[("CALL", call), ("QSO_DATE", date), ("TIME_ON", time)]);
    for (k, v) in [("BAND", band), ("MODE", mode)] {
        if !v.is_empty() {
            r.insert(k.into(), v.into());
        }
    }
    r
}

#[test]
fn matching() {
    let log = q("W1AW", "20m", "SSB", "20260930", "1200");
    // Time slack: within 30 minutes either way, across midnight too.
    assert!(same_qso(&log, &q("w1aw", "20M", "SSB", "20260930", "122959")));
    assert!(same_qso(&log, &q("W1AW", "20m", "SSB", "20260930", "1130")));
    assert!(!same_qso(&log, &q("W1AW", "20m", "SSB", "20260930", "1231")));
    assert!(!same_qso(&log, &q("W1AW", "20m", "SSB", "20261001", "1200")));
    assert!(same_qso(&q("W1AW", "20m", "CW", "20260930", "2350"), &q("W1AW", "20m", "CW", "20261001", "0010")));
    // Mode groups.
    assert!(same_qso(&log, &q("W1AW", "20m", "USB", "20260930", "1200")));
    assert!(same_qso(&q("W1AW", "20m", "FT8", "20260930", "1200"), &q("W1AW", "20m", "FT8", "20260930", "1200")));
    assert!(same_qso(&q("W1AW", "20m", "FT8", "20260930", "1200"), &q("W1AW", "20m", "DATA", "20260930", "1200")));
    let mut ft4 = q("W1AW", "20m", "MFSK", "20260930", "1200");
    ft4.insert("SUBMODE".into(), "FT4".into());
    assert!(same_qso(&ft4, &q("W1AW", "20m", "FT8", "20260930", "1200")));
    assert!(!same_qso(&q("W1AW", "20m", "CW", "20260930", "1200"), &q("W1AW", "20m", "FT8", "20260930", "1200")));
    assert!(!same_qso(&log, &q("W1AW", "20m", "CW", "20260930", "1200")));
    // Call and band.
    assert!(!same_qso(&log, &q("W1AX", "20m", "SSB", "20260930", "1200")));
    assert!(!same_qso(&log, &q("W1AW", "40m", "SSB", "20260930", "1200")));
    // Band from FREQ.
    let mut by_freq = q("W1AW", "", "SSB", "20260930", "1205");
    by_freq.insert("FREQ".into(), "14.250".into());
    assert!(same_qso(&log, &by_freq));
    by_freq.insert("FREQ".into(), "7.150".into());
    assert!(!same_qso(&log, &by_freq));
    // Missing data never matches.
    assert!(!same_qso(&log, &q("W1AW", "20m", "SSB", "20260930", "")));

    assert_eq!(confirmation_key(&log), Some(("W1AW".into(), "20m".into(), 1_790_769_600, "PHONE")));
}

#[test]
fn updates() {
    let rec = f(&[
        ("CALL", "W1AW"),
        ("QSLRDATE", "20261001"),
        ("DXCC", "291"),
        ("STATE", "CT"),
        ("GRIDSQUARE", "FN31pr"),
        ("CNTY", ""),
        ("COMMENT", "x"),
    ]);
    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let u = confirmation_updates(Service::Lotw, &rec);
    assert_eq!(u.set, f(&[("LOTW_QSL_RCVD", "Y"), ("LOTW_QSL_SENT", "Y"), ("LOTW_QSLRDATE", "20261001")]));
    assert_eq!(u.fill, f(&[("DXCC", "291"), ("STATE", "CT"), ("GRIDSQUARE", "FN31pr"), ("LOTW_QSLSDATE", &today)]));

    let u = confirmation_updates(Service::Eqsl, &rec);
    assert_eq!(u.set, f(&[("EQSL_QSL_RCVD", "Y"), ("EQSL_QSL_SENT", "Y"), ("EQSL_QSLRDATE", "20261001")]));
    assert_eq!(u.fill, f(&[("GRIDSQUARE", "FN31pr"), ("EQSL_QSLSDATE", &today)]));

    let u = confirmation_updates(Service::Eqsl, &f(&[("CALL", "W1AW"), ("QSLRDATE", "bogus")]));
    assert_eq!(u.set["EQSL_QSLRDATE"], today);
    assert_eq!(u.fill, f(&[("EQSL_QSLSDATE", &today)]));
}
