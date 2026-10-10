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
fn search_exact_call_with_portable_forms() {
    let (st, log) = setup();
    for (call, time) in [
        ("DL1ABC", "1200"),
        ("DL1ABC/P", "1300"),
        ("DL1ABCD", "1400"),
        ("EA8/DL1ABC", "1500"),
        ("DL1AB", "1600"),
        ("DL1ABC/M", "1700"),
    ] {
        let fields = f(&[("CALL", call), ("BAND", "20m"), ("MODE", "CW"), ("QSO_DATE", "20240101"), ("TIME_ON", time)]);
        st.insert_qso(log, None, &fields).unwrap();
    }
    let calls = |exact: &str| -> Vec<String> {
        let filter = QsoFilter { exact_call: Some(exact.into()), ..Default::default() };
        let (total, rows) = st.search(log, &filter, Sort::Newest, 0, 100).unwrap();
        assert_eq!(total as usize, rows.len());
        rows.into_iter().map(|q| q.fields["CALL"].clone()).collect()
    };
    // Not a prefix search: DL1ABCD and DL1AB stay out; portable forms come in, newest first.
    assert_eq!(calls("dl1abc"), ["DL1ABC/M", "DL1ABC/P", "DL1ABC"]);
    assert_eq!(calls("DL1ABC/P"), ["DL1ABC/M", "DL1ABC/P", "DL1ABC"]);
    assert_eq!(calls("EA8/DL1ABC"), ["DL1ABC/M", "EA8/DL1ABC", "DL1ABC/P", "DL1ABC"]);
    assert_eq!(calls("DL1AB"), ["DL1AB"]);
    assert_eq!(calls("W1AW"), Vec::<String>::new());
    assert_eq!(qrzero_core::store::base_call("EA8/DL1ABC/P"), "DL1ABC");
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

#[test]
fn duplicates_lookups_and_worked_index() {
    let (st, log) = setup();
    let qso = f(&[("CALL", "W1AW"), ("QSO_DATE", "20240101"), ("TIME_ON", "1200"), ("BAND", "20m"), ("MODE", "CW"), ("DXCC", "291"), ("APP_N1MM_ID", "abc")]);
    let id = st.insert_qso(log, None, &qso).unwrap().id;
    let near = f(&[("CALL", "w1aw"), ("QSO_DATE", "20240101"), ("TIME_ON", "120045"), ("BAND", "20M"), ("MODE", "cw")]);
    assert_eq!(st.find_duplicate(log, &near).unwrap(), Some(id));
    let other_band = f(&[("CALL", "W1AW"), ("QSO_DATE", "20240101"), ("TIME_ON", "1200"), ("BAND", "40m"), ("MODE", "CW")]);
    assert_eq!(st.find_duplicate(log, &other_band).unwrap(), None);

    assert_eq!(st.find_qso_by_field(log, "APP_N1MM_ID", "abc").unwrap(), Some(id));
    assert_eq!(st.find_qso_by_field(log, "APP_N1MM_ID", "zzz").unwrap(), None);
    assert!(st.find_qso_by_field(log, "X') OR 1=1 --", "abc").is_err());

    let idx = st.worked_index(log, |_| None).unwrap();
    assert!(!idx.needed("W1AW", Some(291), Some("20m"), Some("CW")).new_call);
    assert!(idx.needed("W1AW", Some(291), Some("40m"), Some("CW")).new_band);
}

#[test]
fn pending_uploads_and_marking() {
    let (mut st, log) = setup();
    let home = st.create_location(log, "Home", &Fields::new()).unwrap();
    let q = |call: &str, date: &str, extra: &[(&str, &str)]| {
        let mut v = vec![("CALL", call), ("QSO_DATE", date), ("TIME_ON", "1200"), ("BAND", "20m"), ("MODE", "CW"), ("STATION_CALLSIGN", "N0CALL")];
        v.extend_from_slice(extra);
        f(&v)
    };
    let a = st.insert_qso(log, Some(home.id), &q("W1AW", "20240101", &[])).unwrap();
    let b = st.insert_qso(log, Some(home.id), &q("K1ABC", "20240201", &[("QRZCOM_QSO_UPLOAD_STATUS", "Y")])).unwrap();
    st.insert_qso(log, None, &q("K2ABC", "20240301", &[("STATION_CALLSIGN", "N0OLD")])).unwrap();
    st.insert_qso(log, Some(home.id), &q("K3ABC", "20230101", &[])).unwrap();
    let calls = vec!["N0CALL".to_string()];
    let since = qrzero_core::store::parse_time("20240101", "0000").unwrap();
    let key = "QRZCOM_QSO_UPLOAD_STATUS";
    let pending = st.pending_uploads(log, key, &calls, None, since, 100).unwrap();
    assert_eq!(pending.iter().map(|q| q.id).collect::<Vec<_>>(), [a.id], "uploaded, other callsign and too old are skipped");
    assert_eq!(st.count_pending(log, key, &calls, Some(home.id), 0).unwrap(), 2);
    assert!(st.pending_uploads(log, "X') --", &calls, None, 0, 10).is_err());

    st.mark_qsos(&[a.id], &f(&[(key, "Y"), ("QRZCOM_QSO_UPLOAD_DATE", "20240102")])).unwrap();
    assert_eq!(st.count_pending(log, key, &calls, None, since).unwrap(), 0);
    assert!(st.mark_qsos(&[a.id], &f(&[("CALL", "X")])).is_err());
    st.mark_qsos(&[a.id], &f(&[("APP_QRZERO_OQRS", "Y"), ("QRZCOM_QSO_DOWNLOAD_STATUS", "Y")])).unwrap();

    // Editing an uploaded QSO marks it modified, so it goes up again.
    let mut changed = b.fields.clone();
    changed.insert("RST_SENT".into(), "579".into());
    assert_eq!(st.update_qso(b.id, b.location_id, &changed).unwrap().fields[key], "M");
    assert_eq!(st.count_pending(log, key, &calls, None, since).unwrap(), 1);
    // Changing only QSL fields doesn't.
    let a2 = st.get_qso(a.id).unwrap();
    let mut qsl_only = a2.fields.clone();
    qsl_only.insert("QSL_RCVD".into(), "Y".into());
    assert_eq!(st.update_qso(a.id, a2.location_id, &qsl_only).unwrap().fields[key], "Y");
}

#[test]
fn paper_queue_holds_only_queued_cards_and_marking_touches_only_the_ids_given() {
    let (mut st, log) = setup();
    let q = |call: &str, sent: &str| {
        let mut v = vec![("CALL", call), ("QSO_DATE", "20240101"), ("TIME_ON", "1200"), ("BAND", "20m"), ("MODE", "CW")];
        if !sent.is_empty() {
            v.push(("QSL_SENT", sent));
        }
        f(&v)
    };
    let a = st.insert_qso(log, None, &q("W1AW", "")).unwrap();
    let b = st.insert_qso(log, None, &q("W1AW", "")).unwrap();
    st.insert_qso(log, None, &q("K1ABC", "R")).unwrap();
    st.insert_qso(log, None, &q("K2ABC", "Y")).unwrap();
    st.insert_qso(log, None, &q("K3ABC", "N")).unwrap();
    assert!(st.paper_queue(log).unwrap().is_empty(), "requested, sent, not-sent and blank are not queued");

    // Queue one of two QSOs with the same station: only that one joins the queue.
    st.mark_qsos(&[a.id], &f(&[("QSL_SENT", "Q")])).unwrap();
    let queue = st.paper_queue(log).unwrap();
    assert_eq!(queue.iter().map(|q| q.id).collect::<Vec<_>>(), [a.id]);
    assert!(!queue.iter().any(|q| q.id == b.id));
}
