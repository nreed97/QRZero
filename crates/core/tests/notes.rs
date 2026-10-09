use qrzero_core::Store;

fn setup() -> (Store, i64) {
    let st = Store::open_in_memory().unwrap();
    let log = st.create_log("Test").unwrap();
    (st, log.id)
}

#[test]
fn set_get_and_update_keep_newlines() {
    let (st, log) = setup();
    assert_eq!(st.get_note(log, "DL1ABC").unwrap(), None);
    assert!(!st.has_note(log, "DL1ABC").unwrap());
    let n = st.set_note(log, " dl1abc ", "Hans, likes CW\r\nQSL direct only").unwrap().unwrap();
    assert_eq!(n.call, "DL1ABC");
    assert_eq!(n.text, "Hans, likes CW\nQSL direct only");
    assert_eq!(n.created_at, n.updated_at);
    assert!(st.has_note(log, "DL1ABC").unwrap());
    let n2 = st.set_note(log, "DL1ABC", "Hans\n\n  indented").unwrap().unwrap();
    assert_eq!(n2.text, "Hans\n\n  indented");
    assert_eq!(n2.created_at, n.created_at);
    assert_eq!(st.get_note(log, "dl1abc").unwrap().unwrap().text, "Hans\n\n  indented");
}

#[test]
fn portable_calls_share_the_base_note() {
    let (st, log) = setup();
    st.set_note(log, "DL1ABC/P", "portable note").unwrap();
    assert_eq!(st.get_note(log, "DL1ABC").unwrap().unwrap().text, "portable note");
    assert_eq!(st.get_note(log, "EA8/DL1ABC").unwrap().unwrap().call, "DL1ABC");
    assert!(st.has_note(log, "dl1abc/mm").unwrap());
    let (total, _) = st.list_notes(log, "", 0, 50).unwrap();
    assert_eq!(total, 1);
}

#[test]
fn blank_text_deletes() {
    let (st, log) = setup();
    st.set_note(log, "W1AW", "hello").unwrap();
    assert_eq!(st.set_note(log, "W1AW", "  \n ").unwrap(), None);
    assert_eq!(st.get_note(log, "W1AW").unwrap(), None);
    st.set_note(log, "W1AW", "again").unwrap();
    assert!(st.delete_note(log, "W1AW").unwrap());
    assert!(!st.delete_note(log, "W1AW").unwrap());
}

#[test]
fn rejects_empty_call() {
    let (st, log) = setup();
    assert!(st.set_note(log, " ", "x").is_err());
    assert!(st.get_note(log, "/").is_err());
}

#[test]
fn notes_are_per_log() {
    let (st, log) = setup();
    let other = st.create_log("Other").unwrap().id;
    st.set_note(log, "K1ABC", "first log").unwrap();
    assert_eq!(st.get_note(other, "K1ABC").unwrap(), None);
    st.set_note(other, "K1ABC", "second log").unwrap();
    assert_eq!(st.get_note(log, "K1ABC").unwrap().unwrap().text, "first log");
    st.delete_log(other).unwrap();
    assert_eq!(st.get_note(other, "K1ABC").unwrap(), None);
}

#[test]
fn list_searches_by_substring_newest_first_and_pages() {
    let (st, log) = setup();
    for c in ["DL1ABC", "DL2XYZ", "G4ABC", "K1ABD"] {
        st.set_note(log, c, &format!("note for {c}")).unwrap();
    }
    // Same-second timestamps; touch G4ABC later so it is clearly newest.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    st.set_note(log, "G4ABC", "updated").unwrap();
    let (total, rows) = st.list_notes(log, "", 0, 10).unwrap();
    assert_eq!(total, 4);
    assert_eq!(rows[0].call, "G4ABC");
    let (total, rows) = st.list_notes(log, "abc", 0, 10).unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.iter().map(|n| n.call.as_str()).collect::<Vec<_>>(), ["G4ABC", "DL1ABC"]);
    let (total, rows) = st.list_notes(log, "DL", 1, 1).unwrap();
    assert_eq!(total, 2);
    assert_eq!(rows.len(), 1);
    assert_eq!(st.list_notes(log, "%", 0, 10).unwrap().0, 0);
}

#[test]
fn reply_list_add_update_and_remove() {
    let (st, log) = setup();
    assert!(st.list_replies(log).unwrap().is_empty());
    st.add_reply_if_new(log, " dl1abc ").unwrap();
    st.save_reply(log, "DL1ABC", "2026-10-01", "send direct").unwrap();
    st.add_reply_if_new(log, "DL1ABC").unwrap(); // keeps the note
    st.save_reply(log, "K1ABC", "2026-09-20", "").unwrap();
    let rows = st.list_replies(log).unwrap();
    assert_eq!(rows.iter().map(|r| r.call.as_str()).collect::<Vec<_>>(), ["K1ABC", "DL1ABC"]);
    assert_eq!(rows[1].note, "send direct");
    assert_eq!(rows[1].received, "2026-10-01");
    assert!(st.delete_reply(log, "dl1abc").unwrap());
    assert!(!st.delete_reply(log, "DL1ABC").unwrap());
    assert!(st.save_reply(log, "  ", "", "").is_err());
}
