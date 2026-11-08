//! Unit tests of the engine (part 5 of 7).

#[allow(unused_imports)]
use super::*;

/// Importing data into a dynamic form adds and removes the rows of its repeating subform to
/// match, and the calculations over them run again.
#[test]
fn xfa_data_import_adds_and_removes_rows_of_a_dynamic_form() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template()));
    // Raised time limits: a busy machine does not stop the calculations this checks.
    let mut s = Session::new().with_clock(|| 1_700_000_000).with_xfa_time_scale(100);
    let id = s.open("rows.pdf", None, bytes, None).expect("opens");
    let names = |s: &Session| s.get(id).unwrap().form.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).map(|f| f.value.clone()).unwrap_or_default();
    assert_eq!(names(&s).iter().filter(|n| n.starts_with("amount")).count(), 1);
    let file = "<form><page1><qty>3</qty><table><row><item>a</item><amount>10</amount></row><row><item>b</item><amount>32</amount></row><row><item>c</item><amount>0.5</amount></row></table></page1></form>";
    s.apply(id, Edit::ImportData { name: "rows.xml".into(), bytes: Arc::new(file.as_bytes().to_vec()) }).unwrap();
    assert_eq!(names(&s).iter().filter(|n| n.starts_with("amount")).count(), 3, "{:?}", names(&s));
    assert_eq!(value(&s, "total"), vec!["15".to_string()], "qty 3 × price 5, calculated after the import: {:?}", s.take_js_output(id));
    assert_eq!(value(&s, "grand"), vec!["42.5".to_string()]);
    // More rows than the layout shows: the document says so.
    let rows: String = (0..60).map(|i| format!("<row><item>r{i}</item><amount>1</amount></row>")).collect();
    let file = format!("<form><page1><qty>1</qty><table>{rows}</table></page1></form>");
    s.apply(id, Edit::ImportData { name: "many.xml".into(), bytes: Arc::new(file.into_bytes()) }).unwrap();
    let out = s.take_js_output(id);
    assert!(out.errors.iter().chain(out.console.iter()).any(|l| l.contains("60 rows")), "{out:?}");
    // A file with one row takes the others away.
    let file = "<form><page1><qty>1</qty><table><row><item>only</item><amount>7</amount></row></table></page1></form>";
    s.apply(id, Edit::ImportData { name: "one.xml".into(), bytes: Arc::new(file.as_bytes().to_vec()) }).unwrap();
    assert_eq!(names(&s).iter().filter(|n| n.starts_with("amount")).count(), 1, "{:?}", names(&s));
    assert_eq!(value(&s, "grand"), vec!["7".to_string()]);
    // Not an XFA data file on an XFA form: refused with the usual message, nothing changed.
    assert!(s.apply(id, Edit::ImportData { name: "junk.xml".into(), bytes: Arc::new(b"<a><b></a>".to_vec()) }).is_err());
    assert_eq!(value(&s, "grand"), vec!["7".to_string()]);
    // The automation tool's view: export then import through a file round-trips the rows.
    let out = s.export_data(id, DataFormat::Xdp, false, true).unwrap();
    assert!(String::from_utf8_lossy(&out).contains("<item>only</item>"));
}

/// Hundreds of calculations, in JavaScript and FormCalc, run on opening in one engine, and a
/// change recalculates them in one more. The time limits are raised so that a busy machine
/// does not stop them; the next test checks that the limits stop them.
#[test]
fn hundreds_of_xfa_calculations_run_in_one_engine() {
    let (sections, rows) = (10, 20);
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::many_calculations_template(sections, rows)));
    let mut s = Session::new().with_clock(|| 1_700_000_000).with_xfa_time_scale(100);
    let engines = pdfcraft_js::xfa_session::engines_started();
    let started = std::time::Instant::now();
    let id = s.open("many.pdf", None, bytes, None).expect("opens");
    let elapsed = started.elapsed();
    assert_eq!(pdfcraft_js::xfa_session::engines_started() - engines, 1, "the scripts of the opening share one engine");
    let out = s.take_js_output(id);
    assert!(!out.errors.iter().any(|e| e.contains("were skipped") || e.contains("stopped")), "{:?}", out.errors);
    let form = &s.get(id).unwrap().form;
    let totals: Vec<&pdfcraft_forms::Field> = form.iter().filter(|f| f.name.starts_with("total")).collect();
    assert_eq!(totals.len(), sections * rows);
    assert!(totals.iter().all(|f| !f.value.is_empty()), "every row's total was calculated");
    let sums: Vec<&pdfcraft_forms::Field> = form.iter().filter(|f| f.name.starts_with("sum")).collect();
    assert_eq!(sums.len(), sections);
    // 2 × (1 + 2 + … + rows)
    let expect = (rows * (rows + 1)).to_string();
    assert!(sums.iter().all(|f| f.value == vec![expect.clone()]), "{:?}", sums.iter().map(|f| f.value.clone()).collect::<Vec<_>>());
    eprintln!("{} calculate scripts on open in {elapsed:?}", sections * rows + sections);
    // A change recalculates them all again, in one more engine.
    let started = std::time::Instant::now();
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("100".into()) }).unwrap();
    assert_eq!(pdfcraft_js::xfa_session::engines_started() - engines, 2, "the scripts of the change share one engine");
    let out = s.take_js_output(id);
    assert!(!out.errors.iter().any(|e| e.contains("were skipped")), "{:?}", out.errors);
    assert_eq!(s.get(id).unwrap().form.iter().find(|f| f.name == "total").unwrap().value, vec!["200".to_string()]);
    assert_eq!(s.get(id).unwrap().form.iter().find(|f| f.name == "sum").unwrap().value, vec![(rows * (rows + 1) - 2 + 200).to_string()]);
    eprintln!("recalculated after a change in {:?}", started.elapsed());
}

/// The time the scripts of an opening share stops the calculations it has no time for, and
/// says how many were skipped; a change's scripts are stopped the same way.
#[test]
fn the_xfa_open_budget_stops_the_calculations_it_has_no_time_for() {
    let (sections, rows) = (10, 20);
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::many_calculations_template(sections, rows)));
    // A time scale of 0 leaves no time at all: every script is skipped before it starts.
    let mut s = Session::new().with_clock(|| 1_700_000_000).with_xfa_time_scale(0);
    let engines = pdfcraft_js::xfa_session::engines_started();
    let id = s.open("many.pdf", None, bytes, None).expect("opens");
    let skipped = sections * rows + sections;
    let stopped = format!("The form's scripts were stopped: the scripts took more than 0 seconds; {skipped} more were skipped");
    assert_eq!(s.take_js_output(id).errors, vec![stopped.clone()]);
    let totals = |s: &Session| s.get(id).unwrap().form.iter().filter(|f| f.name.starts_with("total")).map(|f| f.value.clone()).collect::<Vec<_>>();
    assert_eq!(totals(&s).len(), sections * rows);
    assert!(totals(&s).iter().all(Vec::is_empty), "no total was calculated: {:?}", totals(&s));
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("100".into()) }).unwrap();
    assert!(s.take_js_output(id).errors.iter().any(|e| e.starts_with("The form's scripts were stopped: the scripts took more than 0 seconds")));
    assert!(totals(&s).iter().all(Vec::is_empty), "no total was calculated: {:?}", totals(&s));
    assert_eq!(pdfcraft_js::xfa_session::engines_started(), engines, "no script ran, so no engine started");
}

/// The scripts of one event share the form as the engine holds it: a failed script's
/// half-done writes reach nobody, and a script reads the saved value a choice list was given
/// by its shown text.
#[test]
fn xfa_scripts_of_one_event_see_the_form_as_the_engine_holds_it() {
    // In document order: `a` holds 1; `b`'s calculate writes `a` then fails; `c`'s calculate
    // reads `a`. A failed script keeps what it changed before the error, and the next script
    // sees it. And `d` sets a choice list by its shown text: the next script reads the saved
    // value the engine kept.
    let tpl = pdfcraft_xfa::fixtures::fields_template("<form><page1><country>JP</country></page1></form>").replace(
        r#"<field name="summary" w="3in" h="0.3in" access="readOnly"><ui><textEdit/></ui>"#,
        r#"<field name="a" w="1in" h="0.3in"><ui><numericEdit/></ui><value><integer>1</integer></value></field>
  <field name="b" w="1in" h="0.3in"><ui><numericEdit/></ui><calculate><script contentType="application/x-javascript">a.rawValue = 99; throw new Error("boom");</script></calculate></field>
  <field name="c" w="1in" h="0.3in"><ui><numericEdit/></ui><calculate><script contentType="application/x-javascript">a.rawValue + 1</script></calculate></field>
  <field name="d" w="1in" h="0.3in"><ui><textEdit/></ui><calculate><script contentType="application/x-javascript">country.rawValue = "Canada"; "set"</script></calculate></field>
  <field name="e" w="1in" h="0.3in"><ui><textEdit/></ui><calculate><script>country</script></calculate></field>
  <field name="summary" w="3in" h="0.3in" access="readOnly"><ui><textEdit/></ui>"#,
    );
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("iso.pdf", None, bytes, None).expect("opens");
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).map(|f| f.value.clone()).unwrap_or_default();
    let out = s.take_js_output(id);
    assert!(out.errors.iter().any(|e| e.contains("boom")), "{out:?}");
    assert_eq!(value(&s, "a"), vec!["99".to_string()], "the failed script's write before the error stands");
    assert_eq!(value(&s, "c"), vec!["100".to_string()]);
    assert_eq!(value(&s, "country"), vec!["CA".to_string()], "shown text mapped to the saved value");
    assert_eq!(value(&s, "e"), vec!["CA".to_string()], "the next script reads the saved value");
    assert_eq!(value(&s, "summary"), vec!["Country: CA".to_string()]);
}

/// A row of dates worked out the way leave forms do it: the calculation checks that the raw value
/// is a whole ISO date, then adds days to the date as `formattedValue` shows it (`YYYYMMDD`).
#[test]
fn xfa_date_arithmetic_reads_the_date_as_its_display_picture_shows_it() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::date_row_template()));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("row.pdf", None, bytes, None).expect("opens");
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).map(|f| f.value.clone()).unwrap_or_default();
    s.apply(id, Edit::SetFieldValue { name: "from".into(), value: FieldValue::Text("20261020".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "days".into(), value: FieldValue::Text("3".into()) }).unwrap();
    let out = s.take_js_output(id);
    assert_eq!(value(&s, "to"), ["20261022"], "{:?}", out.errors);
}

#[test]
fn filling_growable_xfa_text_reflows_and_undo_restores_the_layout() {
    let text: String = (0..150).map(|i| format!("Row{i:03} {}sample\n", "sample ".repeat(8))).collect();
    assert_eq!(text.len(), 10_500);
    for scripted in [false, true] {
        let template = pdfcraft_xfa::fixtures::growable_text_template(scripted);
        let mut s = Session::new();
        let id = s.open("growing.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&template)), None).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
        s.apply(id, Edit::SetFieldValue { name: "ACTIVIDADES".into(), value: FieldValue::Text(text.clone()) }).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.len(), 3, "scripted={scripted}");
        let data = pdfcraft_xfa::data_of(&s.get(id).unwrap().editor.as_ref().unwrap().cos).unwrap();
        assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form[0].Page3[0].ACTIVIDADES[0]")), Some(text.as_str()));
        s.undo(id).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
        assert_eq!(s.get(id).unwrap().form.iter().find(|f| f.name == "ACTIVIDADES").unwrap().value, vec!["Short"]);
        s.redo(id).unwrap();
        assert_eq!(s.get(id).unwrap().info.pages.len(), 3);
    }
}

#[test]
fn xfa_text_growth_refreshes_widget_geometry_but_fixed_height_fields_keep_their_widgets() {
    for fixed in [false, true] {
        let size = if fixed { r#"h="30pt""# } else { r#"minH="20pt""# };
        let template = pdfcraft_xfa::fixtures::growable_text_template(false).replace(r#"minH="207.017mm""#, size);
        let mut s = Session::new();
        let id = s.open("geometry.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&template)), None).unwrap();
        let field = |s: &Session| s.get(id).unwrap().form.iter().find(|f| f.name == "ACTIVIDADES").unwrap().clone();
        let before = field(&s);
        s.apply(id, Edit::SetFieldValue { name: "ACTIVIDADES".into(), value: FieldValue::Text("Line\n".repeat(10)) }).unwrap();
        let after = field(&s);
        assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
        if fixed {
            assert_eq!(before.obj, after.obj, "fixed-height fields must not trigger layout");
            assert_eq!(before.widgets[0].rect, after.widgets[0].rect);
        } else {
            assert_ne!(before.obj, after.obj);
            assert!(after.widgets[0].rect[3] - after.widgets[0].rect[1] > before.widgets[0].rect[3] - before.widgets[0].rect[1]);
            assert_eq!(
                s.get(id).unwrap().info.fields.iter().find(|f| f.name == "ACTIVIDADES").unwrap().value.as_deref(),
                Some("Line\n".repeat(10).trim_end())
            );
        }
        s.undo(id).unwrap();
        assert_eq!(field(&s).widgets[0].rect, before.widgets[0].rect);
        s.redo(id).unwrap();
        assert_eq!(field(&s).widgets[0].rect, after.widgets[0].rect);
    }
}

#[test]
fn xfa_text_that_cannot_sync_to_structured_data_is_not_replaced_by_a_relayout() {
    let template = pdfcraft_xfa::fixtures::growable_text_template(false)
        .replace("<ACTIVIDADES>Short</ACTIVIDADES>", "<ACTIVIDADES><nested>Short</nested></ACTIVIDADES>");
    let mut s = Session::new();
    let id = s.open("structured.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&template)), None).unwrap();
    let text = "Keep this accepted value\n".repeat(150);
    s.apply(id, Edit::SetFieldValue { name: "ACTIVIDADES".into(), value: FieldValue::Text(text.clone()) }).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.pages.len(), 2);
    assert_eq!(doc.form.iter().find(|f| f.name == "ACTIVIDADES").unwrap().value, vec![text]);
    assert!(doc.xfa_warnings.iter().any(|w| w.contains("structured content")));
}

#[test]
fn xfa_structured_data_whitespace_edits_survive_without_stale_relayout() {
    let template = pdfcraft_xfa::fixtures::growable_text_template(false)
        .replace("<ACTIVIDADES>Short</ACTIVIDADES>", "<ACTIVIDADES><nested>Short</nested></ACTIVIDADES>");
    for text in ["Short\n", "\nShort", " Short ", "\tShort\t"] {
        let mut s = Session::new();
        let id = s.open("structured.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&template)), None).unwrap();
        let field = |s: &Session| s.get(id).unwrap().form.iter().find(|f| f.name == "ACTIVIDADES").unwrap().clone();
        let before = field(&s);
        s.apply(id, Edit::SetFieldValue { name: "ACTIVIDADES".into(), value: FieldValue::Text(text.into()) }).unwrap();
        assert_eq!(field(&s).value, vec![text], "the accepted whitespace must not be replaced from stale datasets");
        assert_eq!(field(&s).obj, before.obj, "a skipped data write must not rebuild the widgets");
        let doc = s.get(id).unwrap();
        assert!(doc.dirty);
        assert!(doc.xfa_warnings.iter().any(|w| w.contains("structured content")), "{:?}", doc.xfa_warnings);
        let cos = pdfcraft_cos::Document::open(doc.bytes.clone()).unwrap();
        let xdp = pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp;
        assert!(xdp.contains("<ACTIVIDADES><nested>Short</nested></ACTIVIDADES>"));
        s.undo(id).unwrap();
        assert_eq!(field(&s).value, before.value);
        s.redo(id).unwrap();
        assert_eq!(field(&s).value, vec![text]);
    }
}

/// A script that fails partway keeps what it did before the error, as JavaScript does: a form
/// whose check box opens the next boxes and then trips over an object the form no longer has
/// still opens them (DA Form 31's leave types).
#[test]
fn an_xfa_script_that_fails_partway_keeps_what_it_changed_before_the_error() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::partly_failing_script_template()));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, bytes, None).expect("opens");
    let field = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).cloned();
    assert!(field(&s, "leave").is_some_and(|f| f.flags & 1 != 0), "protected until `kind` is checked");
    assert!(field(&s, "details").is_none(), "invisible: not laid out");
    s.take_js_output(id);
    s.apply(id, Edit::SetFieldValue { name: "kind".into(), value: FieldValue::Check(true) }).unwrap();
    let out = s.take_js_output(id);
    assert!(out.errors.iter().any(|e| e.contains("kind") && e.contains("change")), "the error is still reported: {out:?}");
    assert!(field(&s, "leave").is_some_and(|f| f.flags & 1 == 0), "opened before the error");
    assert!(field(&s, "details").is_some(), "shown before the error");
    s.apply(id, Edit::SetFieldValue { name: "leave".into(), value: FieldValue::Check(true) }).expect("can be checked now");
}

/// A scripted form end to end: initialize and calculate on open, recalculation after a change,
/// a failing validation's message, buttons that add and remove rows and show messages, a check
/// box that shows a subform (undoable), FormCalc, and a save and reopen that keep it all.
#[test]
fn repeated_xfa_print_buttons_run_their_click_script() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::repeated_print_button_template()));
    let mut s = Session::new();
    let id = s.open("print.pdf", None, bytes, None).unwrap();
    assert_eq!(s.get(id).unwrap().info.pages.len(), 3);
    for name in ["PrintButton1", "PrintButton1_2", "PrintButton1_3"] {
        let out = s.run_javascript(id, "throw new Error('AcroForm fallback ran')", Some(name)).unwrap();
        assert_eq!(out.error, None, "{name}: {out:?}");
        assert_eq!(out.requests, vec![pdfcraft_js::Request::Print], "{name}: {out:?}");
    }
}

/// An XFA form's scripts initialize, calculate, validate, show subforms and add and remove rows,
/// with undo and save/reopen.
#[test]
fn xfa_scripts_initialize_calculate_validate_toggle_and_add_rows() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template()));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("scripted.pdf", None, bytes, None).expect("opens");
    let add_row = s.get(id).unwrap().form.iter().find(|f| f.name == "addRow").expect("the XFA button is in the form snapshot");
    assert!(matches!(add_row.button.as_ref(), Some(pdfcraft_forms::af::ButtonAction::Script(_))));
    let value = |s: &Session, n: &str| {
        s.get(id).unwrap().form.iter().find(|f| f.name == n).map(|f| f.value.clone()).unwrap_or_else(|| panic!("no field {n}"))
    };
    let names = |s: &Session| s.get(id).unwrap().form.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    // On open: the initialize script set qty, the calculations ran, nothing is dirty.
    assert_eq!(value(&s, "qty"), vec!["2".to_string()]);
    assert_eq!(value(&s, "total"), vec!["10".to_string()], "{:?}", s.take_js_output(id));
    assert_eq!(value(&s, "grand"), vec!["0".to_string()]);
    assert!(!s.get(id).unwrap().xfa_scripts_off());
    assert!(!s.get(id).unwrap().dirty);
    assert!(!names(&s).iter().any(|n| n == "note"), "details is hidden");
    // A change recalculates.
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("3".into()) }).unwrap();
    assert_eq!(value(&s, "total"), vec!["15".to_string()], "{:?}", s.take_js_output(id));
    // A failing validate script shows its message; the value stays, as in Acrobat.
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("500".into()) }).unwrap();
    let out = s.take_js_output(id);
    assert!(out.alerts.iter().any(|a| a.text == "Quantity must be 100 or less"), "{out:?}");
    assert_eq!(value(&s, "total"), vec!["2500".to_string()]);
    // A button adds a row: new fields, same page, one undo step.
    let before = names(&s).len();
    let o = s.run_javascript(id, "", Some("addRow")).unwrap();
    assert_eq!(o.error, None);
    let after = names(&s);
    assert_eq!(after.len(), before + 2, "{after:?}");
    assert!(after.iter().any(|n| n == "amount_2"));
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Run form script"));
    s.apply(id, Edit::SetFieldValue { name: "amount".into(), value: FieldValue::Text("10".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "amount_2".into(), value: FieldValue::Text("32".into()) }).unwrap();
    assert_eq!(value(&s, "grand"), vec!["42".to_string()], "{:?}", s.take_js_output(id));
    // Message boxes reach the viewer.
    let o = s.run_javascript(id, "", Some("hello")).unwrap();
    assert_eq!(o.alerts, vec![crate::js::Message::plain("Hello 500")]);
    // The check box shows the hidden subform; undo hides it again.
    s.apply(id, Edit::SetFieldValue { name: "more".into(), value: FieldValue::Check(true) }).unwrap();
    assert!(names(&s).iter().any(|n| n == "note"), "{:?} {:?}", names(&s), s.take_js_output(id));
    s.undo(id).unwrap();
    assert!(!names(&s).iter().any(|n| n == "note"));
    s.redo(id).unwrap();
    assert!(names(&s).iter().any(|n| n == "note"));
    // Removing the row takes its amount out of the sum.
    s.run_javascript(id, "", Some("removeRow")).unwrap();
    assert!(!names(&s).iter().any(|n| n == "amount_2"));
    assert_eq!(value(&s, "grand"), vec!["10".to_string()]);
    // FormCalc scripts run too.
    let o = s.run_javascript(id, "", Some("legacy")).unwrap();
    assert_eq!(o.error, None);
    assert_eq!(o.alerts, vec![crate::js::Message::plain("FormCalc")]);
    // Saved and reopened: the visible subform, the values and the rows are what they were.
    let saved = s.save_bytes(id).unwrap();
    let id2 = s.open("again.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    let n2: Vec<&str> = d2.form.iter().map(|f| f.name.as_str()).collect();
    assert!(n2.contains(&"note") && !n2.contains(&"amount_2"), "{n2:?}");
    assert_eq!(d2.form.iter().find(|f| f.name == "grand").unwrap().value, vec!["10".to_string()]);
    assert_eq!(d2.form.iter().find(|f| f.name == "qty").unwrap().value, vec!["500".to_string()]);
    assert!(!d2.dirty);
}

/// A repeating table inside an area of a positioned page (as in real Designer forms): every
/// row is laid out, the values Adobe wrote (under the area's parent: areas are
/// not data scopes) are read, a FormCalc total over all rows computes, and what is filled here
/// is written back where Adobe's viewers read it, with no element for the area.
#[test]
fn xfa_rows_in_areas_are_laid_out_read_calculated_and_written_where_adobe_binds_them() {
    let data = "<form><page><table><row><reason>Regular</reason><days>4</days></row><row><reason/><days/></row><row><reason/><days/></row></table><carried>5</carried></page></form>";
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::leave_template(data)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("leave.pdf", None, bytes, None).expect("opens");
    let value = |s: &Session, n: &str| {
        s.get(id).unwrap().form.iter().find(|f| f.name == n).unwrap_or_else(|| panic!("no field {n}")).value.first().cloned().unwrap_or_default()
    };
    let names = |s: &Session| s.get(id).unwrap().form.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    // Three rows (occur min="3"), not one.
    assert_eq!(names(&s).iter().filter(|n| n.starts_with("reason")).count(), 3, "{:?}", names(&s));
    // Adobe's data, bound past the area.
    assert_eq!((value(&s, "reason"), value(&s, "days"), value(&s, "carried")), ("Regular".into(), "4".into(), "5".into()));
    // The total walks rows 0 to 2: 5 + 30 - 4.
    assert_eq!(value(&s, "rest"), "31", "{:?}", s.take_js_output(id));
    // A second regular row recalculates it.
    s.apply(id, Edit::SetFieldValue { name: "reason_2".into(), value: FieldValue::Text("Regular".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "days_2".into(), value: FieldValue::Text("2".into()) }).unwrap();
    assert_eq!(value(&s, "rest"), "29", "{:?}", s.take_js_output(id));
    assert!(s.take_js_output(id).errors.is_empty());
    // Written where Adobe reads it: no element for the area, the rows under the table.
    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let d = pdfcraft_xfa::data_of(&cos).unwrap();
    let path = |som: &str| pdfcraft_xfa::som_to_path(som);
    assert_eq!(d.count(&path("form[0].page[0]"), "box"), 0, "an element for the area was written");
    assert_eq!(d.count(&path("form[0].page[0].table[0]"), "row"), 3);
    assert_eq!(d.text_at(&path("form[0].page[0].table[0].row[1].days[0]")), Some("2"));
    assert_eq!(d.text_at(&path("form[0].page[0].rest[0]")), Some("29"));
    // Reopened, the values come back from that data.
    let mut s2 = Session::new().with_clock(|| 1_700_000_000);
    let id2 = s2.open("again.pdf", None, s.get(id).unwrap().bytes.clone(), None).expect("reopens");
    let v2 = |n: &str| s2.get(id2).unwrap().form.iter().find(|f| f.name == n).unwrap().value.first().cloned().unwrap_or_default();
    assert_eq!((v2("days_2"), v2("rest")), ("2".into(), "29".into()));
}

#[test]
fn a_value_a_script_sets_reaches_every_field_bound_to_the_same_data_node() {
    // `carried` inside the area and a second `carried` outside it bind to one data node (areas
    // are not data scopes). In document order: `setter` writes the inner one, `echo` reads the
    // outer one in the same event, and the saved data holds the one value.
    let tpl = pdfcraft_xfa::fixtures::leave_template("<form><page><carried>5</carried></page></form>")
        .replace(
            r#"    <field name="rest" y="2.5in" w="2in" h="0.3in" access="readOnly">"#,
            r#"    <field name="setter" y="3in" w="2in" h="0.3in"><ui><numericEdit/></ui><calculate><script>carried = 9
9</script></calculate></field>
    <field name="rest" y="2.5in" w="2in" h="0.3in" access="readOnly">"#,
        )
        .replace(
            "  </subform>\n </subform>\n</subform>",
            r#"  </subform>
  <field name="carried" y="8.2in" w="2in" h="0.3in"><ui><numericEdit/></ui></field>
  <field name="echo" y="8.5in" w="2in" h="0.3in"><ui><numericEdit/></ui><calculate><script>carried + 0</script></calculate></field>
 </subform>
</subform>"#,
        );
    assert!(tpl.contains("name=\"echo\"") && tpl.contains("name=\"setter\""), "the fixture was reshaped");
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("shared.pdf", None, bytes, None).expect("opens");
    let out = s.take_js_output(id);
    assert!(out.errors.is_empty(), "{out:?}");
    let values = |s: &Session, id: DocId, n: &str| {
        s.get(id).unwrap().form.iter().filter(|f| f.name.starts_with(n)).map(|f| f.value.join("")).collect::<Vec<_>>()
    };
    assert_eq!(values(&s, id, "carried"), vec!["9".to_string(), "9".to_string()], "both fields show the node's value");
    assert_eq!(values(&s, id, "echo"), vec!["9".to_string()], "read in the same event the setter ran in");
    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let d = pdfcraft_xfa::data_of(&cos).unwrap();
    assert_eq!(d.text_at(&pdfcraft_xfa::som_to_path("form[0].page[0].carried[0]")), Some("9"));
    assert_eq!(d.count(&pdfcraft_xfa::som_to_path("form[0].page[0]"), "carried"), 1, "one data node");
    // The inner field hidden (no widget): the outer widget still takes the node's value, and
    // an unrelated edit does not write the old value back over it.
    let hidden = Arc::new(pdfcraft_xfa::fixtures::shell(
        &tpl.replace(r#"<field name="carried" y="1.5in""#, r#"<field name="carried" presence="hidden" y="1.5in""#),
    ));
    let id = s.open("hidden.pdf", None, hidden, None).expect("opens");
    assert_eq!(values(&s, id, "carried"), vec!["9".to_string()]);
    assert_eq!(values(&s, id, "echo"), vec!["9".to_string()]);
    s.apply(id, Edit::SetFieldValue { name: "allowance".into(), value: FieldValue::Text("31".into()) }).unwrap();
    assert_eq!(values(&s, id, "carried"), vec!["9".to_string()]);
    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    assert_eq!(pdfcraft_xfa::data_of(&cos).unwrap().text_at(&pdfcraft_xfa::som_to_path("form[0].page[0].carried[0]")), Some("9"));
    // A check box bound to the node a text field sets: checked by its own on state.
    let checkbox = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl.replace(
        r#"<field name="carried" y="1.5in" w="2in" h="0.3in"><ui><numericEdit/></ui></field>"#,
        r#"<field name="carried" y="1.5in" w="2in" h="0.3in"><ui><checkButton/></ui><items><integer>9</integer><integer>0</integer></items></field>"#,
    )));
    let id = s.open("check.pdf", None, checkbox, None).expect("opens");
    let carried: Vec<(pdfcraft_forms::FieldKind, Vec<String>)> =
        s.get(id).unwrap().form.iter().filter(|f| f.name.starts_with("carried")).map(|f| (f.kind, f.value.clone())).collect();
    assert_eq!(
        carried,
        vec![(pdfcraft_forms::FieldKind::CheckBox, vec!["9".to_string()]), (pdfcraft_forms::FieldKind::Text, vec!["9".to_string()])],
        "{carried:?}"
    );
}

#[test]
fn flattening_an_xfa_form_bakes_its_pages_and_takes_the_xfa_out() {
    let data = "<form><page1><country>JP</country><langs>English\nSpanish</langs></page1></form>";
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("fields.pdf", None, bytes, None).expect("opens");
    assert_eq!(s.get(id).unwrap().info.xfa, Some(pdfcraft_render::Xfa::Dynamic));
    s.apply(id, Edit::SetFieldValue { name: "other".into(), value: FieldValue::Text("typed here".into()) }).unwrap();
    s.apply(id, Edit::Flatten { comments: false, fields: true }).unwrap();
    let d = s.get(id).unwrap();
    assert!(d.form.is_empty(), "no fields remain");
    assert_eq!(d.info.xfa, None, "no longer an XFA form: Adobe's viewers show the baked pages");
    assert!(d.xfa.is_none());
    let texts = page_texts(&s, id).join("\n");
    for want in ["Japan", "typed here", "Country: JP", "12345"] {
        assert!(texts.contains(want), "{want} is page content now: {texts}");
    }
    // Saved and reopened: a plain PDF, the same page content, nothing of the XFA left.
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    assert!(pdfcraft_xfa::read_packets(&cos).unwrap().is_none());
    let id2 = s.open("flat.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    assert_eq!((d2.info.xfa, d2.form.len()), (None, 0));
    assert!(page_texts(&s, id2).join("\n").contains("typed here"));
    // Undone, the form is a laid-out XFA form again.
    s.undo(id).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.info.xfa, Some(pdfcraft_render::Xfa::Dynamic));
    assert_eq!(d.form.iter().find(|f| f.name == "other").unwrap().value, vec!["typed here".to_string()]);
    // Flattening comments alone leaves the XFA form as it is.
    s.apply(id, Edit::Flatten { comments: true, fields: false }).unwrap();
    assert_eq!(s.get(id).unwrap().info.xfa, Some(pdfcraft_render::Xfa::Dynamic));
}

#[test]
fn a_dynamic_xfa_form_that_was_never_laid_out_keeps_its_xfa_and_the_user_is_told_what_went() {
    // A template that can't be read: the form keeps its placeholder page and its XFA.
    let broken = "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\"><template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"form\"</template></xdp:xdp>";
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("broken.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(broken)), None).expect("opens");
    assert_eq!(s.get(id).unwrap().info.xfa, Some(pdfcraft_render::Xfa::Dynamic));
    assert!(s.get(id).unwrap().xfa.is_none(), "not laid out");
    assert!(s.apply(id, Edit::XfaToStatic).is_err(), "nothing to keep");
    s.apply(id, Edit::Flatten { comments: false, fields: true }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.info.xfa, Some(pdfcraft_render::Xfa::Dynamic), "the XFA stays: Adobe can still render the form");
    assert!(d.info.warnings.iter().any(|w| w.contains("The XFA was kept")), "{:?}", d.info.warnings);
    // A laid-out form with data no field holds: converting says what went with the XFA, and
    // the note stays on the document, while the XFA's own warnings go; undo brings those back.
    let data = "<form><page1><country>JP</country><nobody>holds this</nobody></page1></form>";
    let id = s.open("fields.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data))), None).unwrap();
    let before: Vec<String> = s.get(id).unwrap().info.warnings.clone();
    assert!(before.iter().any(|w| w.contains("scripts changed it on opening")), "an XFA-only warning: {before:?}");
    s.apply(id, Edit::XfaToStatic).unwrap();
    let w = s.get(id).unwrap().info.warnings.clone();
    assert!(w.iter().any(|n| n.starts_with("XFA removed: The XFA was taken out") && n.contains("template, datasets")), "{w:?}");
    assert!(w.iter().any(|n| n.contains("1 value(s) the form's XFA data held that no field showed")), "{w:?}");
    assert!(w.iter().any(|n| n.contains("1 image field(s) do nothing now")), "{w:?}");
    assert!(!w.iter().any(|n| n.contains("scripts changed it on opening")), "the XFA's own warning is gone: {w:?}");
    let js = s.take_js_output(id);
    assert!(js.errors.is_empty(), "{js:?}");
    s.undo(id).unwrap();
    let w = s.get(id).unwrap().info.warnings.clone();
    assert!(!w.iter().any(|n| n.starts_with("XFA removed")), "{w:?}");
    assert_eq!(s.get(id).unwrap().info.xfa, Some(pdfcraft_render::Xfa::Dynamic));
}

#[test]
fn an_xfa_form_converts_to_a_static_form_that_keeps_its_fields() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template()));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("scripted.pdf", None, bytes, None).expect("opens");
    let value = |s: &Session, id: DocId, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).map(|f| f.value.clone()).unwrap_or_default();
    assert_eq!(value(&s, id, "total"), vec!["10".to_string()], "the scripts ran on open");
    let fields_before = s.get(id).unwrap().form.len();
    s.apply(id, Edit::XfaToStatic).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.info.xfa, None);
    assert_eq!(d.form.len(), fields_before, "the fields stay");
    assert_eq!(value(&s, id, "qty"), vec!["2".to_string()]);
    assert_eq!(value(&s, id, "total"), vec!["10".to_string()]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Convert to a static form"));
    // The form is an ordinary one now: a change no longer runs the XFA calculation.
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("7".into()) }).unwrap();
    assert_eq!(value(&s, id, "total"), vec!["10".to_string()]);
    assert!(s.take_js_output(id).is_empty());
    // Saved: no XFA, no private keys on the fields, the pages as laid out.
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    assert!(pdfcraft_xfa::read_packets(&cos).unwrap().is_none());
    assert!(pdfcraft_xfa::fields_by_som(&cos).is_empty(), "no /PCSom left");
    let id2 = s.open("static.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    assert_eq!(d2.info.xfa, None);
    assert_eq!(value(&s, id2, "qty"), vec!["7".to_string()]);
    assert_eq!(d2.info.pages.len(), 1);
    // Not an XFA form: refused.
    assert!(s.apply(id2, Edit::XfaToStatic).is_err());
    // Undone, the XFA form is back and its scripts run again.
    s.undo(id).unwrap();
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().info.xfa, Some(pdfcraft_render::Xfa::Dynamic));
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("3".into()) }).unwrap();
    assert_eq!(value(&s, id, "total"), vec!["15".to_string()]);
}

/// A dynamic XFA form scripted in FormCalc runs as the JavaScript one does.
#[test]
fn dynamic_xfa_forms_run_formcalc_scripts() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::formcalc_template()));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("formcalc.pdf", None, bytes, None).expect("opens");
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).unwrap().value.clone();
    let names = |s: &Session| s.get(id).unwrap().form.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    // Initialize set qty; the calculates ran from it, including the English words.
    assert_eq!(value(&s, "qty"), vec!["2".to_string()], "{:?}", s.take_js_output(id));
    assert_eq!(value(&s, "total"), vec!["10".to_string()]);
    assert_eq!(value(&s, "words"), vec!["Ten".to_string()]);
    assert!(s.take_js_output(id).is_empty());
    // Editing recalculates and validates.
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("7".into()) }).unwrap();
    assert_eq!(value(&s, "total"), vec!["35".to_string()]);
    assert_eq!(value(&s, "words"), vec!["Thirty-Five".to_string()]);
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("500".into()) }).unwrap();
    let out = s.take_js_output(id);
    assert!(out.alerts.iter().any(|a| a.text == "Quantity must be 100 or less"), "{out:?}");
    assert_eq!(value(&s, "qty"), vec!["500".to_string()]);
    // Buttons: a row is added, summed and removed; the message box reaches the viewer.
    let before = names(&s).len();
    let o = s.run_javascript(id, "", Some("addRow")).unwrap();
    assert_eq!(o.error, None, "{o:?}");
    assert_eq!(names(&s).len(), before + 2, "{:?}", names(&s));
    s.apply(id, Edit::SetFieldValue { name: "amount".into(), value: FieldValue::Text("10".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "amount_2".into(), value: FieldValue::Text("32".into()) }).unwrap();
    assert_eq!(value(&s, "grand"), vec!["42".to_string()], "{:?}", s.take_js_output(id));
    let o = s.run_javascript(id, "", Some("hello")).unwrap();
    assert_eq!(o.alerts, vec![crate::js::Message::plain("Hello 500")]);
    s.apply(id, Edit::SetFieldValue { name: "more".into(), value: FieldValue::Check(true) }).unwrap();
    assert!(names(&s).iter().any(|n| n == "note"), "{:?} {:?}", names(&s), s.take_js_output(id));
    s.run_javascript(id, "", Some("removeRow")).unwrap();
    assert!(!names(&s).iter().any(|n| n == "amount_2"));
    assert_eq!(value(&s, "grand"), vec!["10".to_string()]);
    // Saved and reopened, the form is what it was.
    let saved = s.save_bytes(id).unwrap();
    let id2 = s.open("again.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    assert_eq!(d2.form.iter().find(|f| f.name == "words").unwrap().value, vec!["Two Thousand Five Hundred".to_string()]);
    assert!(d2.form.iter().any(|f| f.name == "note"));
}

#[test]
fn hostile_formcalc_scripts_cannot_hang_or_bloat_the_document() {
    let tpl = pdfcraft_xfa::fixtures::formcalc_template()
        .replace(
            "<calculate><script>qty * price</script></calculate>",
            "<calculate><script>table._row.addInstance(1)
table._row.count</script></calculate>",
        )
        .replace("<script>table._row.addInstance(1)</script>", "<script>while (1) do table._row.addInstance(1) endwhile</script>")
        .replace(
            "<script>WordNum(total)</script>",
            "<script>func f(n) do f(n + 1) endfunc
f(1)</script>",
        );
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let started = std::time::Instant::now();
    let id = s.open("hostile.pdf", None, bytes, None).expect("opens");
    let rows = |s: &Session| s.get(id).unwrap().form.iter().filter(|f| f.name.starts_with("amount")).count();
    assert!(rows(&s) <= 50, "{} rows", rows(&s));
    let out = s.take_js_output(id);
    assert!(out.errors.iter().any(|l| l.contains("nested")), "{out:?}");
    let o = s.run_javascript(id, "", Some("addRow")).unwrap();
    assert!(o.error.is_some() || rows(&s) <= 50, "{o:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "{:?}", started.elapsed());
}

#[test]
fn a_formcalc_calculate_result_is_cut_before_it_is_saved() {
    let tpl = pdfcraft_xfa::fixtures::formcalc_template()
        .replace("<script>WordNum(total)</script>", "<script>Replace(Space(100000), \" \", \"x\")</script>");
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("long.pdf", None, bytes, None).expect("opens");
    let words = |s: &Session, id| s.get(id).unwrap().form.iter().find(|f| f.name == "words").unwrap().value.concat();
    assert_eq!(words(&s, id).chars().count(), 65_536);
    let out = s.take_js_output(id);
    assert!(out.errors.iter().any(|e| e.contains("cut")), "{out:?}");
    let saved = s.save_bytes(id).unwrap();
    let id2 = s.open("again.pdf", None, saved, None).unwrap();
    assert!(words(&s, id2).chars().count() <= 65_536);
}

#[test]
fn xfa_scripts_stay_off_with_javascript_off() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template()));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    s.set_javascript(false);
    let id = s.open("scripted.pdf", None, bytes, None).expect("opens");
    let qty = s.get(id).unwrap().form.iter().find(|f| f.name == "qty").unwrap().value.clone();
    assert!(qty.is_empty(), "no initialize script ran");
    assert!(s.run_javascript(id, "", Some("addRow")).is_err());
    assert!(!s.get(id).unwrap().xfa_scripts_off(), "the preference, not a script that ran away");
}

#[test]
fn hostile_xfa_scripts_cannot_hang_or_bloat_the_document() {
    // A calculate script that adds a row every time it runs, a click script that loops on
    // addInstance, and one that nests setInstances: each ends quickly with a bounded form.
    let tpl = pdfcraft_xfa::fixtures::scripted_template()
        .replace(
            r#"<calculate><script contentType="application/x-javascript">qty.rawValue * price.rawValue</script></calculate>"#,
            r#"<calculate><script contentType="application/x-javascript">table._row.addInstance(1); table._row.count</script></calculate>"#,
        )
        .replace(
            r#"table._row.addInstance(1);</script>"#,
            r#"for (var i = 0; i < 100000; i++) { table._row.addInstance(1); table._row.setInstances(0); }</script>"#,
        );
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let started = std::time::Instant::now();
    let id = s.open("hostile.pdf", None, bytes, None).expect("opens");
    let rows = |s: &Session| s.get(id).unwrap().form.iter().filter(|f| f.name.starts_with("amount")).count();
    assert!(rows(&s) <= 50, "{} rows", rows(&s));
    let o = s.run_javascript(id, "", Some("addRow")).unwrap();
    assert!(o.error.is_some() || rows(&s) <= 50, "{o:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(60), "{:?}", started.elapsed());
}

#[test]
fn xfa_scripts_can_remove_rows_twice_and_reset_fields_by_name() {
    let tpl = pdfcraft_xfa::fixtures::scripted_template()
        .replace(r#"if (table._row.count > 1) { table._row.removeInstance(table._row.count - 1); }"#, r#"table._row.setInstances(1);"#)
        .replace(r#"xfa.host.messageBox("Hello " + qty.rawValue);"#, r#"xfa.host.resetData("qty, table");"#);
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("rows.pdf", None, bytes, None).expect("opens");
    let rows = |s: &Session| s.get(id).unwrap().form.iter().filter(|f| f.name.starts_with("amount")).count();
    s.run_javascript(id, "", Some("addRow")).unwrap();
    s.run_javascript(id, "", Some("addRow")).unwrap();
    assert_eq!(rows(&s), 3);
    s.apply(id, Edit::SetFieldValue { name: "amount".into(), value: FieldValue::Text("7".into()) }).unwrap();
    // setInstances(1) removes two rows in one script: both go, nothing comes back.
    s.run_javascript(id, "", Some("removeRow")).unwrap();
    assert_eq!(rows(&s), 1, "{:?}", s.get(id).unwrap().form.iter().map(|f| &f.name).collect::<Vec<_>>());
    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let data = pdfcraft_xfa::data_of(&cos).unwrap();
    assert_eq!(data.count(&pdfcraft_xfa::som_to_path("form[0].page1[0].table[0]"), "row"), 1);
    // resetData with names clears those fields (the row's amount too) and nothing else.
    s.apply(id, Edit::SetFieldValue { name: "price".into(), value: FieldValue::Text("9".into()) }).unwrap();
    let o = s.run_javascript(id, "", Some("hello")).unwrap();
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).unwrap().value.clone();
    assert!(value(&s, "qty").is_empty(), "{:?} {o:?}", value(&s, "qty"));
    assert!(value(&s, "amount").is_empty());
    assert_eq!(value(&s, "price"), vec!["9".to_string()]);
}

/// Hostile scripts run on open are merged, capped, kept from printing, saving and opening links,
/// and noted.
#[test]
fn hostile_xfa_open_scripts_are_merged_capped_kept_from_printing_and_noted() {
    // On open: a calculate setting its own value 20 000 times (50 000 sat right at the 1 s
    // unasked-script timeout and abandoned whenever the test machine was loaded), an
    // initialize script showing a thousand messages and asking to print, open a link and save.
    let tpl = pdfcraft_xfa::fixtures::scripted_template()
        .replace(
            r#"<calculate><script contentType="application/x-javascript">qty.rawValue * price.rawValue</script></calculate>"#,
            r#"<calculate><script contentType="application/x-javascript">for (var i = 0; i &lt; 20000; i++) this.rawValue = i;</script></calculate>"#,
        )
        .replace(
            r#"if (qty.rawValue === null) qty.rawValue = 2;"#,
            r#"for (var i = 0; i &lt; 1000; i++) xfa.host.messageBox("m" + i); xfa.host.print(); xfa.host.gotoURL("https://example.com/"); app.execMenuItem("SaveAs"); if (qty.rawValue === null) qty.rawValue = 2;"#,
        );
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let started = std::time::Instant::now();
    let id = s.open("hostile.pdf", None, bytes, None).expect("opens");
    // 30 s and not 5 s: the run is wall-clock timed and a loaded test machine (all suites in
    // parallel) can take several seconds for the 20 000 iterations; a real hang still fails.
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "{:?}", started.elapsed());
    let doc = s.get(id).unwrap();
    assert_eq!(doc.form.iter().find(|f| f.name == "total").unwrap().value, vec!["19999".to_string()]);
    // One datasets stream for the scripts' revision, not one per value set.
    assert!(datasets_streams(&doc.bytes) <= 3, "{} datasets streams", datasets_streams(&doc.bytes));
    // What opening changed is noted with the form's other XFA notes.
    assert!(doc.xfa_warnings.iter().any(|w| w.contains("scripts changed it on opening")), "{:?}", doc.xfa_warnings);
    let out = s.take_js_output(id);
    assert_eq!(out.alerts.len(), 100, "message boxes are capped");
    assert!(out.errors.iter().any(|e| e.contains("more than 100 messages")), "{:?}", out.errors);
    assert!(out.requests.is_empty(), "only a click may print, save or open links: {:?}", out.requests);
    assert!(out.console.iter().any(|c| c.contains("only print when a button is clicked")), "{:?}", out.console);
}

/// An XFA button's message boxes reach the viewer with their titles, icons and buttons, from
/// JavaScript (`xfa.host.messageBox`, `app.alert` in its own order) and FormCalc alike, every
/// one of them and in order.
#[test]
fn xfa_message_boxes_keep_their_title_icon_and_buttons() {
    use crate::js::{Message, MessageButtons, MessageIcon};
    let tpl = pdfcraft_xfa::fixtures::scripted_template()
        .replace(
            r#"xfa.host.messageBox("Hello " + qty.rawValue);"#,
            r#"xfa.host.messageBox("Clear the rows?", "Order form", 2, 2); app.alert("Saved", 3, 0, "Order form");"#,
        )
        .replace(r#"$host.messageBox("FormCalc")"#, r#"$host.messageBox("Too many", "Limits", 1, 1)"#);
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("boxes.pdf", None, bytes, None).expect("opens");
    let o = s.run_javascript(id, "", Some("hello")).unwrap();
    assert_eq!(o.error, None);
    let titled = |text: &str, title: &str, icon, buttons| Message { text: text.into(), title: Some(title.into()), icon, buttons };
    assert_eq!(
        o.alerts,
        vec![
            titled("Clear the rows?", "Order form", MessageIcon::Question, MessageButtons::YesNo),
            titled("Saved", "Order form", MessageIcon::Status, MessageButtons::Ok),
        ]
    );
    let o = s.run_javascript(id, "", Some("legacy")).unwrap();
    assert_eq!(o.error, None);
    assert_eq!(o.alerts, vec![titled("Too many", "Limits", MessageIcon::Warning, MessageButtons::OkCancel)]);
}

#[test]
fn runaway_xfa_calculations_at_open_end_quickly_with_a_report() {
    // Nested loops in one frame stop at the loop limit; loops in nested calls (that limit is per
    // call) are abandoned at the time limit and turn the form's scripts off.
    let tpl = pdfcraft_xfa::fixtures::scripted_template()
        .replace(
            r#"<calculate><script contentType="application/x-javascript">qty.rawValue * price.rawValue</script></calculate>"#,
            r#"<calculate><script contentType="application/x-javascript">for (var i = 0; i &lt; 1e9; i++) { for (var j = 0; j &lt; 1e9; j++) {} }</script></calculate>"#,
        )
        .replace(
            r#"var t = 0; var rows"#,
            r#"function f() { for (var j = 0; j &lt; 99999; j++) {} } for (var k = 0; k &lt; 5000; k++) f(); var t = 0; var rows"#,
        );
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&tpl));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let started = std::time::Instant::now();
    let id = s.open("runaway.pdf", None, bytes, None).expect("opens");
    assert!(started.elapsed() < std::time::Duration::from_secs(4), "{:?}", started.elapsed());
    let out = s.take_js_output(id);
    assert!(out.errors.iter().any(|e| e.contains("total") && e.contains("maximum number of iteration")), "{:?}", out.errors);
    assert!(out.errors.iter().any(|e| e.contains("abandoned")), "{:?}", out.errors);
    assert!(out.errors.iter().any(|e| e.contains("scripts are off")), "{:?}", out.errors);
    assert!(s.get(id).unwrap().xfa_scripts_off());
    // The scripts stay off: a change runs nothing more.
    let started = std::time::Instant::now();
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("3".into()) }).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(s.take_js_output(id).errors.is_empty());
    assert!(s.get(id).unwrap().xfa_scripts_off());
}

/// #260 (page 7): comments without an appearance stream, here a FreeText box (`/C` background,
/// blue `/DA` text) and an Ink stroke, weren't drawn. The page view now draws them from
/// appearances made for display only; the working file and what Save writes stay as they were.
#[test]
fn comments_without_appearances_are_drawn_but_not_saved() {
    let objs: Vec<&[u8]> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        b"<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R] >>",
        b"<< /Type /Annot /Subtype /FreeText /Rect [20 200 180 260] /Contents (Box) /DA (/Helv 10 Tf 0 0 1 rg) /Border [0 0 1] /C [.85 .47 .02] >>",
        b"<< /Type /Annot /Subtype /Ink /Rect [20 20 180 120] /InkList [[30 30 100 110 170 30]] /Border [0 0 4] /C [0 .6 0] >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(o);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let original = Arc::new(bytes);
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("comments.pdf", None, original.clone(), None).expect("opens");
    let check = |s: &Session| {
        let p = shown(s, id, 0);
        let px = |x: u32, y: u32| p.rgba[((y * p.width + x) * 4) as usize..][..4].to_vec();
        // y down: the FreeText box spans rows 40..100, the Ink apex is at (100, 190).
        assert_eq!(px(100, 90), vec![217, 120, 5, 255], "the FreeText background");
        // On the apex row: Ink is drawn as a curve through its points (#374), level at the apex.
        assert_eq!(px(100, 190), vec![0, 153, 0, 255], "the Ink stroke");
    };
    check(&s);
    let doc = s.get(id).unwrap();
    assert!(Arc::ptr_eq(&doc.bytes, &original), "the working file is the file as opened");
    assert!(!Arc::ptr_eq(&doc.display, &doc.bytes) && Arc::ptr_eq(&doc.export_source().bytes, &doc.display), "page images show it too");
    // After an edit (a scoped refresh) they are still drawn, and Save writes no appearances.
    s.apply(id, rect_comment(0, [150.0, 270.0, 190.0, 290.0])).unwrap();
    check(&s);
    let saved = s.save_bytes(id).unwrap();
    let reopened = pdfcraft_cos::Document::open(saved).unwrap();
    for r in [pdfcraft_cos::ObjRef::new(4, 0), pdfcraft_cos::ObjRef::new(5, 0)] {
        assert!(reopened.get(r).as_dict().is_some_and(|d| !d.contains(b"AP")), "{r:?} is saved without /AP");
    }
}

/// ISO 32000-2 Table 178: with `/LL`, `/L` gives the ends of the leader lines, and the line itself
/// is drawn `/LL` from it (above, for a positive `/LL` on a line drawn left to right). The page
/// view shows the line there, with its leaders down to `/L`, and nothing along `/L` itself.
#[test]
fn a_line_without_an_appearance_is_drawn_at_its_leader_length() {
    let objs: Vec<&[u8]> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        b"<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>",
        b"<< /Type /Annot /Subtype /Line /Rect [0 0 200 300] /L [40 100 160 100] /LL 30 /C [1 0 0] /BS << /W 4 >> >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(o);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("line.pdf", None, Arc::new(bytes), None).expect("opens");
    let p = shown(&s, id, 0);
    let px = |x: u32, y: u32| p.rgba[((y * p.width + x) * 4) as usize..][..4].to_vec();
    // y down: /L is row 200, the line itself row 170.
    assert_eq!(px(100, 170), vec![255, 0, 0, 255], "the line, 30 above /L");
    assert_eq!(px(40, 185), vec![255, 0, 0, 255], "the leader line at the start");
    assert_eq!(px(160, 185), vec![255, 0, 0, 255], "the leader line at the end");
    assert_eq!(px(100, 200), vec![255, 255, 255, 255], "nothing along /L");
}

/// Rubber stamps without an appearance are drawn from their `/Name` (ISO 32000-2 Table 184), in
/// the colour of the stamp they name; an unknown name gets a grey placeholder; a stamp's own
/// artwork is drawn as it is. Display only: Save writes the file without new appearances.
#[test]
fn stamps_without_appearances_draw_the_stamp_they_name() {
    let (original, expected) = stamps_without_appearances();
    let mut s = Session::new();
    let id = s.open("stamps.pdf", None, original.clone(), None).expect("opens");
    let p = shown(&s, id, 0);
    for (i, (rect, family)) in expected.iter().enumerate() {
        let mut counts = std::collections::HashMap::new();
        for y in (792.0 - rect[3]) as u32..(792.0 - rect[1]) as u32 {
            for x in rect[0] as u32..rect[2] as u32 {
                let px = &p.rgba[((y * p.width + x) * 4) as usize..][..4];
                if let Some(f) = colour_family(px) {
                    *counts.entry(f).or_insert(0) += 1;
                }
            }
        }
        let total: u32 = counts.values().sum();
        let main = counts.iter().max_by_key(|(_, n)| **n).map(|(f, _)| *f);
        assert!(total > 300, "stamp {i} draws ink inside its /Rect: {counts:?}");
        assert_eq!(main, Some(*family), "stamp {i} is drawn in its colour: {counts:?}");
    }
    let saved = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    for n in 4..20 {
        assert!(saved.get(pdfcraft_cos::ObjRef::new(n, 0)).as_dict().is_some_and(|d| !d.contains(b"AP")), "{n} is saved without /AP");
    }
}

/// #2371: a comment without an appearance stream is displayed by the app, so the CLI's page image
/// must show it too. The helper's copy is for rendering only; documents that need no copy, and
/// files it can't display, keep exactly the original source bytes.
#[test]
fn cli_display_adds_missing_appearances_without_changing_the_source() {
    let original = free_text_without_appearance();
    let display = display_bytes_for(original.clone(), None);
    assert_ne!(display.as_slice(), original.as_slice(), "the display copy draws the missing comment appearance");

    let doc = pdfcraft_cos::Document::open(display).unwrap();
    let object = doc.get(pdfcraft_cos::ObjRef::new(4, 0));
    let annot = object.as_dict().unwrap();
    assert!(annot.contains(b"AP"), "the display copy has an appearance stream");

    let plain = Arc::new(fixture(1));
    assert!(Arc::ptr_eq(&display_bytes_for(plain.clone(), None), &plain), "a complete document needs no copy");
    let damaged = Arc::new(b"not a PDF".to_vec());
    assert!(Arc::ptr_eq(&display_bytes_for(damaged.clone(), None), &damaged), "an unreadable file falls back to its source");
}

/// Titles a browser writes for a blank page or a pop-up name no document; real titles stay.
#[test]
fn placeholder_titles_are_recognised() {
    for t in ["about:blank", "About:Blank", " about:srcdoc ", "blob:https://bank.example/3f2a", "data:text/html,x", "Untitled", "untitled"] {
        assert!(is_placeholder_title(t), "{t:?}");
    }
    for t in ["Quarterly report", "Untitled report", "About: our company", "Statement", "blank"] {
        assert!(!is_placeholder_title(t), "{t:?}");
    }
}

/// A form whose fields are only page widgets (the `/Fields` list is empty): they are adopted, and
/// the leniency is recorded in the repair log rather than applied silently.
#[test]
fn adopting_page_only_fields_is_noted_as_a_repair() {
    let objs: Vec<&str> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",                                     // 1
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",                     // 2
        "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R] >>",                                 // 3
        "<< /Fields [] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >>",               // 4
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (alpha) /Rect [10 200 90 220] /P 3 0 R >>", // 5
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (beta) /Rect [10 150 90 170] /P 3 0 R >>",  // 6
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",                                // 7
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("orphan.pdf", None, Arc::new(bytes), None).expect("opens");
    let doc = s.get(id).unwrap();
    let names: Vec<&str> = doc.form.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["alpha", "beta"]);
    assert_eq!(doc.repair_log(), vec!["the form's /Fields list omits 2 fields; they were read from the page annotations".to_string()]);
}

/// A page too large for the renderer at the asked resolution exports at the most it allows (and
/// says at what), as File ▸ Export does; a source that asks for strictness refuses it instead.
#[test]
fn oversized_exports_clamp_and_report_their_dpi_unless_strict() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    // 14,400 pt (200 in) wide: 600 dpi would be 120,000 px.
    let bytes = s.create_blank(14_400.0, 792.0, 1).unwrap();
    let id = s.open("wide.pdf", None, bytes, None).unwrap();
    let src = s.get(id).unwrap().export_source();
    let mut ex = export::Exporter::from_source(src.clone());
    let used = ex.dpi_used(0, 600.0);
    assert!(used < 600.0 && used > 0.0, "{used}");
    let png = ex.png(0, 600.0).expect("clamped, not refused");
    assert!(png.starts_with(b"\x89PNG"));
    assert!((ex.dpi_used(0, 18.0) - 18.0).abs() < 0.01, "a small enough request (3,600 px) is drawn as asked");
    let mut strict = src;
    strict.config.reject_oversize = true;
    let err = export::Exporter::from_source(strict).png(0, 600.0).unwrap_err();
    assert!(err.contains("exceeds renderer limits"), "{err}");
}

#[test]
fn mixed_object_moves_are_one_undo_step_and_survive_save_reopen() {
    let (mut s, id) = session_with(1);
    let text = AddedText { text: "Added label".into(), rect: [20.0, 240.0, 140.0, 265.0], ..Default::default() };
    s.apply(id, Edit::AddText { page: 0, text }).unwrap();
    let image = image::RgbImage::from_pixel(12, 8, image::Rgb([240, 20, 20]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    s.apply(id, Edit::AddImage { page: 0, rect: Some([40.0, 40.0, 80.0, 70.0]), name: "red.png".into(), bytes: Arc::new(png.into_inner()) }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved.clone(), None).unwrap();
    let before = s.get(id).unwrap().editable_objects(0).unwrap();
    assert_eq!(before.len(), 3, "one native paragraph, one added label and one added image");
    let generation = s.get(id).unwrap().edit_generation();
    let targets = before.iter().map(|o| o.target).collect();
    s.apply(id, Edit::MoveObjects { page: 0, objects: targets, offset: [24.0, -24.0] }).unwrap();
    let doc = s.get(id).unwrap();
    assert!(doc.dirty && doc.edit_generation() > generation);
    assert_eq!(doc.can_undo(), Some("Move objects"));
    let after = doc.editable_objects(0).unwrap();
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.text, b.text);
        for (i, (x, y)) in a.rect.iter().zip(b.rect).enumerate() {
            assert!((y - x - if i % 2 == 0 { 24.0 } else { -24.0 }).abs() < 0.001);
        }
    }
    let moved = s.save_bytes(id).unwrap();
    assert!(moved.starts_with(saved.as_ref()), "an incremental update preserves the original revision");
    let mut again = Session::new();
    let other = again.open("moved.pdf", None, moved.clone(), None).unwrap();
    assert_eq!(again.get(other).unwrap().editable_objects(0).unwrap(), after);
    assert_eq!(s.undo(id).unwrap().as_str(), "Move objects");
    assert_eq!(s.get(id).unwrap().editable_objects(0).unwrap(), before);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add image"), "one undo restores all three objects");
    s.redo(id).unwrap();
    assert_eq!(s.get(id).unwrap().editable_objects(0).unwrap(), after);
    let mut renderer = pdfcraft_render::PageRenderer::new(moved, Default::default());
    let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    let pixel = |x: usize, y: usize| &rendered.rgba[(y * rendered.width as usize + x) * 4..][..3];
    assert!(pixel(84, 269)[0] > 200 && pixel(84, 269)[1] < 50, "the added image renders at its new position");
    assert!(pixel(45, 240).iter().all(|v| *v > 240), "the old image position is clear");
}

#[test]
fn failed_group_moves_preserve_generation_dirty_state_and_redo_history() {
    let (mut s, id) = session_with(1);
    let objects: Vec<_> = s.get(id).unwrap().editable_objects(0).unwrap().iter().map(|o| o.target).collect();
    s.apply(id, Edit::MoveObjects { page: 0, objects: objects.clone(), offset: [10.0, 0.0] }).unwrap();
    s.undo(id).unwrap();
    let before = s.save_bytes(id).unwrap();
    let generation = s.get(id).unwrap().edit_generation();
    let dirty = s.get(id).unwrap().dirty;
    let mut bad = objects;
    bad.push(ObjectTarget { kind: ObjectKind::Image, index: 999 });
    assert!(s.apply(id, Edit::MoveObjects { page: 0, objects: bad, offset: [10.0, 0.0] }).is_err());
    let doc = s.get(id).unwrap();
    assert_eq!(doc.edit_generation(), generation);
    assert_eq!(doc.dirty, dirty);
    assert!(doc.can_undo().is_none());
    assert_eq!(doc.can_redo(), Some("Move object"));
    assert_eq!(s.save_bytes(id).unwrap(), before);
    s.redo(id).unwrap();
    assert!(s.get(id).unwrap().dirty);
}

#[test]
fn group_move_respects_modify_permission_and_display_coordinate_rotation() {
    for rotation in [0, 90, 180, 270] {
        let (mut s, id) = session_with(1);
        s.apply(id, Edit::RotatePages { pages: vec![0], degrees: rotation }).unwrap();
        let doc = s.get(id).unwrap();
        let delta = doc.object_move_offset(0, [17.0, 23.0]).unwrap();
        let info = &doc.info.pages[0];
        let a = info.user_to_view(30.0, 70.0);
        let b = info.user_to_view(30.0 + delta[0] as f32, 70.0 + delta[1] as f32);
        assert!((b[0] - a[0] - 17.0).abs() < 0.001 && (b[1] - a[1] - 23.0).abs() < 0.001);
    }
    let (mut s, id) = session_with(1);
    let mut p = protection(None, Some("owner"));
    p.changes = Changes::None;
    s.apply(id, Edit::Protect(p)).unwrap();
    let bytes = s.save_bytes(id).unwrap();
    let mut reader = Session::new();
    let id = reader.open("restricted.pdf", None, bytes, None).unwrap();
    let objects = reader.get(id).unwrap().editable_objects(0).unwrap().iter().map(|o| o.target).collect();
    let generation = reader.get(id).unwrap().edit_generation();
    assert!(reader.apply(id, Edit::MoveObjects { page: 0, objects, offset: [10.0, 0.0] }).is_err());
    let doc = reader.get(id).unwrap();
    assert!(!doc.dirty && doc.can_undo().is_none() && doc.edit_generation() == generation);
}

#[test]
fn group_moves_preserve_rotated_added_content_raster_geometry() {
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddText { page: 0, text: AddedText { text: "Added label".into(), rect: [20.0, 240.0, 140.0, 265.0], ..Default::default() } })
        .unwrap();
    let image = image::RgbImage::from_fn(20, 10, |x, _| image::Rgb(if x < 10 { [240, 20, 20] } else { [20, 20, 240] }));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    s.apply(id, Edit::AddImage { page: 0, rect: Some([40.0, 40.0, 80.0, 70.0]), name: "quadrants.png".into(), bytes: Arc::new(png.into_inner()) })
        .unwrap();
    let AddedContent::Image(mut image) = s.get(id).unwrap().added()[1].content.clone() else { panic!() };
    image.flip_h = true;
    image.crop = [0.1, 0.0, 0.2, 0.0];
    s.apply(id, Edit::UpdateContent { page: 0, index: 1, content: AddedContent::Image(image) }).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let render = |bytes| {
        let mut r = pdfcraft_render::PageRenderer::new(bytes, Default::default());
        r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() })
    };
    let before = render(s.get(id).unwrap().bytes.clone());
    let doc = s.get(id).unwrap();
    let offset = doc.object_move_offset(0, [24.0, 36.0]).unwrap();
    let objects = doc.editable_objects(0).unwrap().iter().map(|o| o.target).collect();
    s.apply(id, Edit::MoveObjects { page: 0, objects, offset }).unwrap();
    let after = render(s.get(id).unwrap().bytes.clone());
    assert_eq!((before.width, before.height), (after.width, after.height));
    let mut painted = 0;
    for y in 0..before.height as usize {
        for x in 0..before.width as usize {
            let pos = (y * before.width as usize + x) * 4;
            let expected = if x >= 24 && y >= 36 {
                let old = ((y - 36) * before.width as usize + x - 24) * 4;
                &before.rgba[old..old + 4]
            } else {
                &[255, 255, 255, 255]
            };
            // Raster coverage can round by one intensity level when equivalent placements
            // reach the renderer through different text operators. Geometry and image colours
            // must still agree across every pixel of the translated page.
            assert!(after.rgba[pos..pos + 4].iter().zip(expected).all(|(a, b)| a.abs_diff(*b) <= 2), "pixel {x},{y}");
            painted += usize::from(expected[..3].iter().any(|c| *c < 240));
        }
    }
    assert!(painted > 500, "the comparison contains real rendered text and image pixels");
}

#[test]
fn legacy_passwords_with_turkish_characters_open() {
    // RC4-128 (R3): user "ılık€" in PDFDocEncoding (ı = 0x9A, € = 0xA0), owner "Çağlar"
    // (PDFDocEncoding has no ğ: written without it, as pdfcraft-crypt writes R2–R4 passwords).
    let mut cos = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
    let params = pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Rc4_128,
        user_password: "ılık€",
        owner_password: "Çağlar",
        permissions: -1,
        encrypt_metadata: true,
        seed: [7; 32],
    };
    cos.set_encryption(&params).unwrap();
    let bytes = Arc::new(pdfcraft_cos::write_full(&cos, &pdfcraft_cos::SaveOptions::default()).unwrap());
    let mut s = Session::new();
    // The renderer used to try only the UTF-8 bytes, and the owner fallback handed it the
    // recovered user password as Latin-1: both came back as a wrong password.
    for pw in ["ılık€", "Çağlar"] {
        let id = s.open("x.pdf", None, bytes.clone(), Some(pw)).unwrap_or_else(|e| panic!("{pw}: {e:?}"));
        assert_eq!(page_texts(&s, id), ["Page 1"], "{pw}");
        assert!(s.get(id).unwrap().info.encrypted, "{pw}");
    }
    assert!(matches!(s.open("x.pdf", None, bytes, Some("ilik€")), Err(OpenError::WrongPassword)));
}

#[test]
fn recovered_user_passwords_reach_the_renderer_byte_for_byte() {
    assert_eq!(crate::renderer_password(b"pw"), "pw");
    // UTF-8 bytes (qpdf writes "şifre" so; its \xC5\x9F has no PDFDocEncoding reading).
    assert_eq!(crate::renderer_password("şifre".as_bytes()), "şifre");
    // PDFDocEncoding bytes: ı = 0x9A, € = 0xA0, Ç = 0xC7.
    assert_eq!(crate::renderer_password(&[0x9A, b'l', 0x9A, b'k', 0xA0]), "ılık€");
    assert_eq!(crate::renderer_password(&[0xC7, b'o', b'k']), "Çok");
}

#[test]
fn aes256_files_whose_passwords_saslprep_changes_open_as_typed() {
    // A fullwidth Ｏ: SASLprep makes the owner password "Owner".
    let (owner, owner_prepared) = ("\u{FF2F}wner", "Owner");
    for (typed, prepared) in SASLPREP_CHANGES {
        // pdfcraft-cos keys the file on the SASLprep forms, as ISO 32000-2 asks.
        let mut cos = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
        let params = pdfcraft_cos::NewEncryption {
            algorithm: pdfcraft_cos::Algorithm::Aes256,
            user_password: typed,
            owner_password: owner,
            permissions: -1,
            encrypt_metadata: true,
            seed: [7; 32],
        };
        cos.set_encryption(&params).unwrap();
        let bytes = Arc::new(pdfcraft_cos::write_full(&cos, &pdfcraft_cos::SaveOptions::default()).unwrap());
        let mut s = Session::new();
        // pdfcraft-cos accepts the password as typed (it applies SASLprep); the renderer used to
        // refuse it, so the session called it a wrong password.
        for pw in [typed, prepared, owner, owner_prepared] {
            let id = s.open("x.pdf", None, bytes.clone(), Some(pw)).unwrap_or_else(|e| panic!("{typed:?} as {pw:?}: {e:?}"));
            assert_eq!(page_texts(&s, id), ["Page 1"], "{typed:?} as {pw:?}");
            assert!(s.get(id).unwrap().info.encrypted, "{typed:?} as {pw:?}");
        }
        assert!(matches!(s.open("x.pdf", None, bytes, Some("cafe")), Err(OpenError::WrongPassword)), "{typed:?}");
    }
}

#[test]
fn enhance_camera_page_straightens_the_sheet() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let (png, sheet) = camera_photo();
    // 72 dpi: one point per pixel, so the page is 400 × 300 pt and y flips to user space.
    let bytes = s.create_from_images_with_resolution(&[("photo.png".into(), png)], ImageResolution::Dpi(72.0)).unwrap();
    let id = s.open("photo.pdf", None, bytes, None).unwrap();
    let corners = sheet.map(|[x, y]| [x, 300.0 - y]);
    // Corners in any order.
    s.enhance_camera_page(id, 0, [corners[2], corners[0], corners[3], corners[1]]).unwrap();

    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.pages.len(), 1);
    assert_eq!(doc.can_undo(), Some("Enhance camera image"));
    // The longer of each pair of opposite sides: 310 × 250 pt at the photo's own resolution.
    let p = &doc.info.pages[0];
    assert!((p.width - 310.0).abs() <= 1.0 && (p.height - 250.0).abs() <= 1.0, "{} × {}", p.width, p.height);

    let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
    let shot = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(shot.error.is_none(), "{:?}", shot.error);
    let at = |x: u32, y: u32| {
        let i = ((y * shot.width + x) * 4) as usize;
        shot.rgba[i..i + 3].to_vec()
    };
    // No table left in the corners: the sheet fills the page, and its mark is top left.
    for (x, y) in [(4, 4), (shot.width - 5, 4), (shot.width - 5, shot.height - 5), (4, shot.height - 5), (shot.width / 2, shot.height / 2)] {
        assert!(at(x, y).iter().all(|&v| v > 235), "({x}, {y}) = {:?}", at(x, y));
    }
    assert!(at(30, 30).iter().all(|&v| v < 40), "the mark: {:?}", at(30, 30));

    s.undo(id).unwrap();
    let p = &s.get(id).unwrap().info.pages[0];
    assert_eq!((p.width, p.height), (400.0, 300.0));
}

#[test]
fn detect_camera_corners_finds_the_sheet_in_user_space() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let (png, sheet) = camera_photo();
    let bytes = s.create_from_images_with_resolution(&[("photo.png".into(), png)], ImageResolution::Dpi(72.0)).unwrap();
    let id = s.open("photo.pdf", None, bytes, None).unwrap();
    let found = s.detect_camera_corners(id, 0).expect("the sheet's edges");
    // One point per pixel, y flipped: the sheet's corners in reading order.
    for (f, p) in found.iter().zip(sheet.iter()) {
        let want = [p[0], 300.0 - p[1]];
        assert!((f[0] - want[0]).hypot(f[1] - want[1]) < 4.0, "{found:?} vs {sheet:?}");
    }
    // A page with no photo on it: nothing to find.
    let id = s.open("doc.pdf", None, Arc::new(fixture(1)), None).unwrap();
    assert_eq!(s.detect_camera_corners(id, 0), None);
    assert_eq!(s.detect_camera_corners(id, 7), None);
}

#[test]
fn enhance_camera_page_at_a_level_whitens_a_shadowed_sheet() {
    // A sheet lit unevenly: full light at the left, a third of it at the right.
    let img = image::RgbImage::from_fn(400, 300, |x, y| {
        let light = 1.0 - 0.66 * x as f32 / 400.0;
        let ink = if (y / 10) % 3 == 1 && (40..360).contains(&x) { 0.12 } else { 0.96 };
        let v = (ink * light * 255.0) as u8;
        image::Rgb([v, v, (v as f32 * 0.85) as u8])
    });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let whole = [[0.0, 0.0], [400.0, 0.0], [400.0, 300.0], [0.0, 300.0]];
    let paper_at_right = |level: u8| {
        let mut s = Session::new().with_clock(|| 1_700_000_000);
        let bytes = s.create_from_images_with_resolution(&[("photo.png".into(), png.clone())], ImageResolution::Dpi(72.0)).unwrap();
        let id = s.open("photo.pdf", None, bytes, None).unwrap();
        s.enhance_camera_page_at(id, 0, whole, camera_clean::Look::Colour, level).unwrap();
        assert_eq!(s.get(id).unwrap().can_undo(), Some("Enhance camera image"), "one step at level {level}");
        let doc = s.get(id).unwrap();
        let shot = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone()).render(pdfcraft_render::RenderRequest {
            page: 0,
            scale: 1.0,
            ..Default::default()
        });
        // A paper pixel near the right edge, between text rows.
        let i = ((5 * shot.width + shot.width - 12) * 4) as usize;
        shot.rgba[i..i + 3].to_vec()
    };
    let (before, after) = (paper_at_right(0), paper_at_right(80));
    assert!(before.iter().all(|&v| v < 140), "the shadow: {before:?}");
    assert!(after.iter().all(|&v| v > 235), "white paper: {after:?}");
}

#[test]
fn enhance_camera_page_refuses_bad_corners_and_pages() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("doc.pdf", None, Arc::new(fixture(1)), None).unwrap();
    let line = [[0.0, 0.0], [50.0, 50.0], [100.0, 100.0], [150.0, 150.0]];
    let e = s.enhance_camera_page(id, 0, line).unwrap_err().to_string();
    assert!(e.contains("corners"), "{e}");
    let ok = [[0.0, 0.0], [200.0, 0.0], [200.0, 300.0], [0.0, 300.0]];
    assert!(s.enhance_camera_page(id, 5, ok).unwrap_err().to_string().contains("no page 6"));
    assert!(matches!(s.enhance_camera_page(DocId(999), 0, ok), Err(EditError::NoDocument)));
    assert_eq!(s.get(id).unwrap().can_undo(), None, "nothing changed");
}

/// A content-only edit must not disturb the inspection results: pages, annotations, fields,
/// links and marks are unchanged, only the working bytes, the added-content list and the
/// display copy move.
#[test]
fn content_edit_keeps_pages_annotations_fields_and_links() {
    let (mut session, id) = session_with(2);
    let before = session.get(id).unwrap();
    let pages = format!("{:?}", before.info.pages);
    let annotations = format!("{:?}", before.info.annotations);
    let links = format!("{:?}", before.links);
    let form = before.form.clone();
    let marks = before.marks().to_vec();
    let bytes = before.bytes.clone();
    let text = AddedText {
        rect: [20.0, 20.0, 200.0, 60.0],
        text: "hello".into(),
        family: FontFamily::Helvetica,
        bold: false,
        italic: false,
        size: 12.0,
        color: [0.0, 0.0, 0.0],
        ink: None,
        align: TextAlign::Left,
        aligns: Vec::new(),
        char_spacing: 0.0,
    };
    session.apply(id, Edit::AddText { page: 1, text }).unwrap();
    let after = session.get(id).unwrap();
    assert_eq!(format!("{:?}", after.info.pages), pages, "page geometry is untouched");
    assert_eq!(format!("{:?}", after.info.annotations), annotations);
    assert_eq!(format!("{:?}", after.links), links);
    assert_eq!(after.form, form, "no field was touched");
    assert_eq!(after.marks(), marks, "no mark was touched");
    assert!(after.added().iter().any(|a| a.page == 1), "the added text is listed");
    assert_ne!(after.bytes, bytes, "the working file carries the edit");
    assert_ne!(after.display, bytes, "the display copy carries the edit too");
    // The display bytes still open and render the edited page.
    let mut renderer = pdfcraft_render::PageRenderer::new(after.display.clone(), after.config.clone());
    let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 1, scale: 1.0, ..Default::default() });
    assert!(rendered.error.is_none(), "{:?}", rendered.error);
    // Undo goes through the same narrow refresh without the edit at hand.
    session.undo(id).unwrap();
    let undone = session.get(id).unwrap();
    assert!(undone.added().is_empty(), "undo drops the added text");
    assert_eq!(format!("{:?}", undone.info.pages), pages);
}

/// `refresh_content` re-reads only the derived lists the edit can change (undo/redo, with no
/// edit at hand, re-read both).
#[test]
fn content_refresh_lists_match_the_storages_an_edit_touches() {
    let target = |kind| ObjectTarget { kind, index: 0 };
    let added = [
        Edit::AddText {
            page: 0,
            text: AddedText {
                rect: [0.0, 0.0, 10.0, 10.0],
                text: String::new(),
                family: FontFamily::Helvetica,
                bold: false,
                italic: false,
                size: 12.0,
                color: [0.0, 0.0, 0.0],
                ink: None,
                align: TextAlign::Left,
                aligns: Vec::new(),
                char_spacing: 0.0,
            },
        },
        Edit::MoveObjects { page: 0, objects: vec![target(crate::ObjectKind::Added)], offset: [1.0, 0.0] },
    ];
    for edit in &added {
        assert_eq!(Session::content_refresh_lists(Some(edit)), (true, false), "{}", edit.label());
    }
    let plain = [
        Edit::MoveObjects { page: 0, objects: vec![target(crate::ObjectKind::Text)], offset: [1.0, 0.0] },
        Edit::EditTextBlock { page: 0, block: 0, text: String::new(), style: pdfcraft_edit::BlockStyle::default() },
    ];
    for edit in &plain {
        assert_eq!(Session::content_refresh_lists(Some(edit)), (false, false), "{}", edit.label());
    }
    let marks = [Edit::AddWatermark { pages: vec![0], settings: Watermark::default(), replace: false, file: None }];
    for edit in &marks {
        assert_eq!(Session::content_refresh_lists(Some(edit)), (false, true), "{}", edit.label());
    }
    assert_eq!(Session::content_refresh_lists(None), (true, true), "undo/redo re-read both");
}

/// A content edit can add a font (Add text in Times); the narrow refresh leaves the font list
/// as it was, and `refresh_fonts` reads it again before Properties ▸ Fonts or `doc_info` show it.
#[test]
fn fonts_added_by_a_content_edit_show_after_refresh_fonts() {
    let (mut session, id) = session_with(1);
    let has_times = |s: &Session| s.get(id).unwrap().info.fonts.iter().any(|f| f.name.contains("Times"));
    assert!(!has_times(&session));
    let text = AddedText { text: "serif".into(), rect: [20.0, 20.0, 200.0, 60.0], family: FontFamily::Times, ..Default::default() };
    session.apply(id, Edit::AddText { page: 0, text }).unwrap();
    assert!(!has_times(&session), "the narrow refresh doesn't re-inspect");
    session.refresh_fonts(id);
    assert!(has_times(&session), "{:?}", session.get(id).unwrap().info.fonts);
    // Nothing changed since: a second call is a no-op.
    let fonts = format!("{:?}", session.get(id).unwrap().info.fonts);
    session.refresh_fonts(id);
    assert_eq!(format!("{:?}", session.get(id).unwrap().info.fonts), fonts);
}

/// Gap 13: every edit writes a new working file (the original plus one revision). Before the
/// next one is written, the current one is let go of by the working bytes, the display bytes and
/// the renderer: on a 1 GiB file, keeping it is a whole extra gigabyte at the peak of each edit.
#[test]
fn the_working_file_is_let_go_of_before_the_next_is_written() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: 90 }).unwrap();
    let doc = s.doc_mut(id).unwrap();
    let (working, original) = (Arc::downgrade(&doc.bytes), doc.editor.as_ref().unwrap().cos.bytes().clone());
    assert!(!Arc::ptr_eq(&doc.bytes, &original), "an edited working file is a buffer of its own");
    Session::release_working(doc);
    assert!(Arc::ptr_eq(&doc.bytes, &original) && Arc::ptr_eq(&doc.display, &original));
    // The old renderer's workers let go of the file when they exit, just after.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while working.upgrade().is_some() {
        assert!(std::time::Instant::now() < deadline, "the working file is still held");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // The refresh that follows brings everything back.
    Session::refresh(doc).unwrap();
    assert_eq!(s.get(id).unwrap().info.pages[1].rotation, 90);
    let _ = shown(&s, id, 1);
}
