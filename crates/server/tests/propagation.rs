//! Solar data from a stand-in for N0NBH's feed.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use qrzero_server::{start, Config};
use serde_json::Value;
use tokio::net::TcpListener;

/// Shaped like the real hamqsl.com/solarxml.php output, quirks included.
const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<solar>
	<solardata>
		<source url="http://www.hamqsl.com/solar.html">N0NBH</source>
		<updated> 08 Oct 2026 1400 GMT</updated>
		<solarflux>148</solarflux>
		<aindex> 12</aindex>
		<kindex> 3</kindex>
		<kindexnt>No Report</kindexnt>
		<xray>C1.4</xray>
		<sunspots>112</sunspots>
		<heliumline>141.6</heliumline>
		<protonflux>45</protonflux>
		<electonflux>1340</electonflux>
		<aurora> 2</aurora>
		<normalization>1.99</normalization>
		<latdegree>67.5</latdegree>
		<solarwind>412.3</solarwind>
		<magneticfield> -2.1</magneticfield>
		<calculatedconditions>
			<band name="80m-40m" time="day">Fair</band>
			<band name="30m-20m" time="day">Good</band>
			<band name="17m-15m" time="day">Good</band>
			<band name="12m-10m" time="day">Fair</band>
			<band name="80m-40m" time="night">Good</band>
			<band name="30m-20m" time="night">Good</band>
			<band name="17m-15m" time="night">Fair</band>
			<band name="12m-10m" time="night">Poor</band>
		</calculatedconditions>
		<calculatedvhfconditions>
			<phenomenon name="vhf-aurora" location="northern_hemi">Band Closed</phenomenon>
			<phenomenon name="E-Skip" location="europe">Band Closed</phenomenon>
			<phenomenon name="E-Skip" location="north_america">Band Closed</phenomenon>
			<phenomenon name="E-Skip" location="europe_6m">50MHz ES</phenomenon>
			<phenomenon name="E-Skip" location="europe_4m">Band Closed</phenomenon>
		</calculatedvhfconditions>
		<geomagfield>UNSETTLED</geomagfield>
		<signalnoise>S1-S2</signalnoise>
		<fof2>6.25</fof2>
		<muffactor>2.88</muffactor>
		<muf>18.02</muf>
	</solardata>
</solar>
"#;

#[derive(Default)]
struct Feed {
    hits: AtomicUsize,
    broken: AtomicBool,
}

async fn feed(feed: Arc<Feed>) -> String {
    let app = Router::new().route(
        "/solarxml.php",
        get(move || {
            let f = feed.clone();
            async move {
                f.hits.fetch_add(1, Ordering::SeqCst);
                if f.broken.load(Ordering::SeqCst) {
                    (StatusCode::BAD_GATEWAY, "upstream down").into_response()
                } else {
                    ([("content-type", "text/xml")], SAMPLE).into_response()
                }
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/solarxml.php")
}

#[tokio::test]
async fn propagation_is_fetched_once_and_cached() {
    let state = Arc::new(Feed::default());
    let url = feed(state.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
    cfg.propagation_url = url;
    let running = start(cfg).await.unwrap();
    let http = reqwest::Client::new();
    let get = |q: &'static str| {
        let req = http.get(format!("http://{}/api/propagation{q}", running.addr)).header("x-qrzero-token", &running.token);
        async move {
            let resp = req.send().await.unwrap();
            assert_eq!(resp.status(), 200);
            serde_json::from_str::<Value>(&resp.text().await.unwrap()).unwrap()
        }
    };

    // Nothing is fetched until someone asks.
    assert_eq!(state.hits.load(Ordering::SeqCst), 0);

    let v = get("").await;
    assert_eq!(state.hits.load(Ordering::SeqCst), 1);
    assert!(v["error"].is_null(), "{v}");
    assert!(v["fetched_at"].is_string());
    let d = &v["data"];
    assert_eq!(d["source"], "N0NBH");
    assert_eq!(d["updated_utc"], "2026-10-08T14:00:00+00:00");
    assert_eq!(d["values"]["solarflux"], "148");
    assert_eq!(d["values"]["aindex"], "12");
    assert_eq!(d["values"]["kindex"], "3");
    assert_eq!(d["values"]["magneticfield"], "-2.1");
    assert_eq!(d["values"]["geomagfield"], "UNSETTLED");
    assert_eq!(d["values"]["muf"], "18.02");
    assert_eq!(d["bands"].as_array().unwrap().len(), 8);
    assert_eq!(d["bands"][7]["name"], "12m-10m");
    assert_eq!(d["bands"][7]["time"], "night");
    assert_eq!(d["bands"][7]["condition"], "Poor");
    assert_eq!(d["vhf"][3]["location"], "europe_6m");
    assert_eq!(d["vhf"][3]["condition"], "50MHz ES");

    // Cached: neither a second read nor an early manual refresh goes out again.
    get("").await;
    get("?refresh=true").await;
    assert_eq!(state.hits.load(Ordering::SeqCst), 1);
    // Many at once share one fetch.
    let all = futures_util::future::join_all((0..5).map(|_| get(""))).await;
    assert!(all.iter().all(|x| x["data"]["values"]["sunspots"] == "112"));
    assert_eq!(state.hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn propagation_reports_a_dead_feed() {
    let state = Arc::new(Feed::default());
    state.broken.store(true, Ordering::SeqCst);
    let url = feed(state.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_path_buf());
    cfg.secret_service = format!("QRZero-test-{}", std::process::id());
    cfg.update_cty = false;
    cfg.propagation_url = url;
    let running = start(cfg).await.unwrap();
    let v: Value = reqwest::Client::new()
        .get(format!("http://{}/api/propagation", running.addr))
        .header("x-qrzero-token", &running.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(v["data"].is_null());
    assert!(v["fetched_at"].is_null());
    assert!(v["error"].as_str().unwrap().contains("502"), "{v}");
}
