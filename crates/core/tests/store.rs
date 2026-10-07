use qrzero_core::adif::{self, Fields};
use qrzero_core::model::*;
use qrzero_core::store::Sort;
use qrzero_core::Store;

fn f(pairs: &[(&str, &str)]) -> Fields {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn setup() -> (Store, i64) {
    let st = Store::open_in_memory().unwrap();
    let log = st.create_log("Test").unwrap();
    (st, log.id)
}

#[test]
fn insert_normalizes_and_derives_band() {
    let (st, log) = setup();
    let q = st
        .insert_qso(log, None, &f(&[("call", " w1aw "), ("QSO_DATE", "20240101"), ("TIME_ON", "1200"), ("FREQ", "14.025"), ("MODE", "cw"), ("GRIDSQUARE", "fn31PR")]))
        .unwrap();
    assert_eq!(q.fields["CALL"], "W1AW");
    assert_eq!(q.fields["BAND"], "20m");
    assert_eq!(q.fields["MODE"], "CW");
    assert_eq!(q.fields["GRIDSQUARE"], "FN31pr");
}

#[test]
fn rejects_qso_without_time() {
    let (st, log) = setup();
    assert!(st.insert_qso(log, None, &f(&[("CALL", "W1AW")])).is_err());
}

#[test]
fn location_fills_my_fields() {
    let (st, log) = setup();
    let loc = st
        .create_location(log, "Home", &f(&[("MY_GRIDSQUARE", "FN42"), ("MY_POTA_REF", "K-0001"), ("NOT_MY", "x")]))
        .unwrap();
    assert!(loc.is_default);
    assert!(!loc.fields.contains_key("NOT_MY"));
    let q = st
        .insert_qso(log, Some(loc.id), &f(&[("CALL", "K1ABC"), ("QSO_DATE", "20240101"), ("TIME_ON", "0000"), ("MY_POTA_REF", "K-9999")]))
        .unwrap();
    assert_eq!(q.fields["MY_GRIDSQUARE"], "FN42");
    assert_eq!(q.fields["MY_POTA_REF"], "K-9999", "existing values are kept");
    assert_eq!(q.location_id, Some(loc.id));
}

#[test]
fn search_filters() {
    let (st, log) = setup();
    for (call, band, mode, sub, time) in [
        ("W1AW", "20m", "CW", "", "1200"),
        ("W1ABC", "40m", "SSB", "", "1300"),
        ("K1ABC", "20m", "MFSK", "FT4", "1400"),
        ("DL1ABC/P", "20m", "FT8", "", "1500"),
    ] {
        let mut fields = f(&[("CALL", call), ("BAND", band), ("MODE", mode), ("QSO_DATE", "20240101"), ("TIME_ON", time), ("STATION_CALLSIGN", "N0CALL")]);
        if !sub.is_empty() {
            fields.insert("SUBMODE".into(), sub.into());
        }
        st.insert_qso(log, None, &fields).unwrap();
    }
    let calls = |filter: QsoFilter| -> Vec<String> {
        let (_, rows) = st.search(log, &filter, Sort::Oldest, 0, 100).unwrap();
        rows.into_iter().map(|q| q.fields["CALL"].clone()).collect()
    };
    assert_eq!(calls(QsoFilter { call: Some("w1".into()), ..Default::default() }), ["W1AW", "W1ABC"]);
    assert_eq!(calls(QsoFilter { call: Some("*abc*".into()), ..Default::default() }), ["W1ABC", "K1ABC", "DL1ABC/P"]);
    assert_eq!(calls(QsoFilter { bands: vec!["20M".into()], ..Default::default() }), ["W1AW", "K1ABC", "DL1ABC/P"]);
    assert_eq!(calls(QsoFilter { modes: vec!["FT4".into(), "FT8".into()], ..Default::default() }), ["K1ABC", "DL1ABC/P"]);
    let t1330 = qrzero_core::store::parse_time("20240101", "1330").unwrap();
    assert_eq!(calls(QsoFilter { from: Some(t1330), ..Default::default() }), ["K1ABC", "DL1ABC/P"]);
    assert_eq!(calls(QsoFilter { fields: f(&[("SUBMODE", "")]), ..Default::default() }), ["W1AW", "W1ABC", "DL1ABC/P"]);
    let (total, rows) = st.search(log, &QsoFilter::default(), Sort::Newest, 1, 2).unwrap();
    assert_eq!(total, 4);
    assert_eq!(rows[0].fields["CALL"], "K1ABC");
    let ids = vec![rows[0].id];
    assert_eq!(calls(QsoFilter { ids: Some(ids), ..Default::default() }), ["K1ABC"]);
}

#[test]
fn import_with_duplicates_location_and_callsigns() {
    let (mut st, log) = setup();
    let loc = st.create_location(log, "Portable", &f(&[("MY_GRIDSQUARE", "EM10")])).unwrap();
    let adi = b"header<EOH>
<CALL:4>W1AW<QSO_DATE:8>20240101<TIME_ON:4>1200<BAND:3>20m<MODE:2>CW<STATION_CALLSIGN:5>N0OLD<APP_L4ONG_X:1>1<EOR>
<CALL:4>W1AW<QSO_DATE:8>20240101<TIME_ON:6>120030<BAND:3>20m<MODE:2>CW<STATION_CALLSIGN:5>N0NEW<EOR>
<CALL:4>W1AW<QSO_DATE:8>20240101<TIME_ON:4>1200<BAND:3>40m<MODE:2>CW<EOR>
<CALL:4>K1AB<EOR>";
    let opts = ImportOptions {
        location_id: Some(loc.id),
        skip_duplicates: true,
        add_station_callsigns: true,
        ..Default::default()
    };
    let r = st.import_adif(log, adi, &opts).unwrap();
    assert_eq!((r.imported, r.duplicates, r.rejected), (2, 1, 1));
    assert_eq!(r.added_callsigns, ["N0OLD"]);
    let calls = st.list_callsigns(log).unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].is_default);

    // Importing the same file again finds only duplicates.
    let again = st.import_adif(log, adi, &opts).unwrap();
    assert_eq!((again.imported, again.duplicates), (0, 3));

    let (_, rows) = st.search(log, &QsoFilter::default(), Sort::Oldest, 0, 10).unwrap();
    assert!(rows.iter().all(|q| q.fields["MY_GRIDSQUARE"] == "EM10" && q.location_id == Some(loc.id)));
}

#[test]
fn export_standard_and_full() {
    let (mut st, log) = setup();
    let adi = b"<CALL:4>W1AW<QSO_DATE:8>20240101<TIME_ON:4>1200<BAND:3>20m<MODE:2>CW<APP_L4ONG_X:1>1<EOR>
<CALL:4>K1AB<QSO_DATE:8>20240102<TIME_ON:4>1200<BAND:3>40m<MODE:3>SSB<EOR>";
    st.import_adif(log, adi, &ImportOptions::default()).unwrap();
    let (std_text, n) = st.export_adif(log, &QsoFilter::default(), ExportProfile::Standard, "test").unwrap();
    assert_eq!(n, 2);
    assert!(!std_text.contains("APP_L4ONG_X"));
    let (full, _) = st.export_adif(log, &QsoFilter::default(), ExportProfile::Full, "test").unwrap();
    let parsed = adif::parse(full.as_bytes());
    assert_eq!(parsed.records.len(), 2);
    assert_eq!(parsed.records[0]["APP_L4ONG_X"], "1");
    assert_eq!(parsed.header["PROGRAMID"], "QRZero");
    let (only_40, n) = st
        .export_adif(log, &QsoFilter { bands: vec!["40m".into()], ..Default::default() }, ExportProfile::Full, "test")
        .unwrap();
    assert_eq!(n, 1);
    assert!(only_40.contains("<CALL:4>K1AB"));
}

#[test]
fn worked_before_by_call_and_dxcc() {
    let (st, log) = setup();
    for (call, band, mode, dxcc) in [("W1AW", "20m", "CW", "291"), ("W1AW", "40m", "CW", "291"), ("K1AB", "15m", "SSB", "291")] {
        st.insert_qso(log, None, &f(&[("CALL", call), ("BAND", band), ("MODE", mode), ("DXCC", dxcc), ("QSO_DATE", "20240101"), ("TIME_ON", "1200")]))
            .unwrap();
    }
    let wb = st.worked_before(log, "w1aw", Some(291)).unwrap();
    assert_eq!(wb.call_count, 2);
    assert_eq!(wb.call_slots, [("20m".to_string(), "CW".to_string()), ("40m".into(), "CW".into())]);
    assert_eq!(wb.dxcc_count, 3);
    assert_eq!(wb.dxcc_bands, ["15m", "20m", "40m"]);
    assert_eq!(wb.dxcc_modes, ["CW", "SSB"]);
}

#[test]
fn defaults_and_update() {
    let (mut st, log) = setup();
    let a = st.add_callsign(log, "n0old").unwrap();
    let b = st.add_callsign(log, "N0NEW").unwrap();
    assert!(a.is_default && !b.is_default);
    st.set_default_callsign(b.id).unwrap();
    let list = st.list_callsigns(log).unwrap();
    assert_eq!(list[0].callsign, "N0NEW");
    assert!(list[0].is_default && !list[1].is_default);

    let q = st.insert_qso(log, None, &f(&[("CALL", "W1AW"), ("QSO_DATE", "20240101"), ("TIME_ON", "1200")])).unwrap();
    let mut fields = q.fields.clone();
    fields.insert("CALL".into(), "W1AX".into());
    let q2 = st.update_qso(q.id, None, &fields).unwrap();
    assert_eq!(q2.fields["CALL"], "W1AX");
    let (_, rows) = st.search(log, &QsoFilter { call: Some("W1AX".into()), ..Default::default() }, Sort::Newest, 0, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(st.delete_qsos(&[q.id]).unwrap(), 1);
}

#[test]
fn reopens_existing_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.db");
    {
        let st = Store::open(&path).unwrap();
        let log = st.create_log("Keep").unwrap();
        st.insert_qso(log.id, None, &f(&[("CALL", "W1AW"), ("QSO_DATE", "20240101"), ("TIME_ON", "1200")])).unwrap();
    }
    let st = Store::open(&path).unwrap();
    assert_eq!(st.list_logs().unwrap()[0].qso_count, 1);
}

#[test]
fn equipment_per_location() {
    let (mut st, log) = setup();
    let home = st.create_location(log, "Home", &Fields::new()).unwrap();
    let park = st.create_location(log, "Park", &Fields::new()).unwrap();
    let k3 = st.create_equipment(home.id, "rig", "K3", &f(&[("POWER_W", "100"), ("EMPTY", " ")])).unwrap();
    let ic7300 = st.create_equipment(home.id, "Rig", "IC-7300", &Fields::new()).unwrap();
    st.create_equipment(home.id, "antenna", "Hex beam", &Fields::new()).unwrap();
    st.create_equipment(park.id, "rig", "KX2", &Fields::new()).unwrap();
    assert!(st.create_equipment(home.id, "toaster", "x", &Fields::new()).is_err());
    assert_eq!(k3.fields.len(), 1);
    assert_eq!(ic7300.kind, "rig");

    st.move_equipment(ic7300.id, -1).unwrap();
    let all = st.list_equipment(log).unwrap();
    let home_rigs: Vec<_> = all.iter().filter(|e| e.location_id == home.id && e.kind == "rig").map(|e| e.name.as_str()).collect();
    assert_eq!(home_rigs, ["IC-7300", "K3"]);
    assert_eq!(all.len(), 4);

    let moved = st.update_equipment(k3.id, park.id, "rig", "K3", &k3.fields).unwrap();
    assert_eq!(moved.location_id, park.id);
    st.delete_location(park.id).unwrap();
    assert_eq!(st.list_equipment(log).unwrap().len(), 2, "equipment goes with its location");
}
