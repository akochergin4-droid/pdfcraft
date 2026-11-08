//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 6 of 8).

#[allow(unused_imports)]
use super::*;

#[test]
fn ocr_tools_make_a_scan_searchable() {
    let dir = workdir("ocr");
    let mut a = auto(&dir);
    let status = ok(&mut a, "ocr_status", json!({}));
    assert_eq!(status["languages"][0]["code"], "en");
    assert_eq!(status["engines"][0]["id"], "ocrs", "{status}");
    assert!(status["engines"][1]["id"] == "tesseract" && status["engines"][1]["available"].is_boolean(), "{status}");
    if status["available"] != true {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Searchable scans with recognised words" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 150, "pages": [1] }));
    let png = std::fs::read_dir(dir.join("scan")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "ocr_recognize", json!({ "doc": scan }));
    assert!(r["pages"][0]["text"].as_str().unwrap().to_lowercase().contains("searchable"), "{r}");
    assert!(r["words"].as_u64().unwrap() >= 4);
    let again = ok(&mut a, "ocr_recognize", json!({ "doc": scan, "pages": [1] }));
    assert!(again["pages"][0]["skipped"].is_string(), "{again}");
    assert!(matches!(a.call("ocr_recognize", &json!({ "doc": scan, "language": "xx" })), Err(ToolError::InvalidArgs(_))));
    // An engine that cannot run is named up front, before anything is read.
    if status["engines"][1]["available"] != true {
        let why = a.call("ocr_recognize", &json!({ "doc": scan, "engine": "tesseract", "skip_text_pages": false })).unwrap_err();
        assert!(matches!(why, ToolError::Failed(_)), "{why}");
        assert!(why.to_string().contains("tesseract"), "{why}");
    }
    // ocr_words: the recognition's words with confidence, band and suspects, changing nothing.
    // ocrs reports no confidences, so `auto` + a confidence merge is asked for when the second
    // engine is there to supply them; without it confidence and band are honestly null.
    ok(&mut a, "doc_close", json!({ "doc": scan, "discard_changes": true }));
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    let mut args = json!({ "doc": scan });
    if status["engines"][1]["available"] == true {
        args["engine"] = json!("auto");
        args["strategy"] = json!("confidence");
    }
    let words = ok(&mut a, "ocr_words", args);
    assert!(words["words"].as_u64().unwrap() >= 4, "{words}");
    let first = &words["pages"][0]["words"][0];
    assert!(first["text"].is_string() && first["box"].as_array().is_some_and(|b| b.len() == 4), "{words}");
    if status["engines"][1]["available"] == true {
        let with_confidence = words["pages"][0]["words"]
            .as_array()
            .expect("words")
            .iter()
            .filter(|w| w["confidence"].is_number() && w["band"].as_str().is_some())
            .count();
        assert!(with_confidence >= 4, "the second engine's confidences arrive: {words}");
    } else {
        assert!(first["confidence"].is_null() && first["band"].is_null() && first["suspect"] == false, "{words}");
    }
    assert!(a.call("edit_undo", &json!({ "doc": scan })).is_err(), "ocr_words changes nothing, so there is no undo");
}

/// An editable-text recognition writes a NEW file of visible text, never the source.
#[test]
fn ocr_recognize_editable_output_writes_visible_text() {
    let dir = workdir("ocr-editable");
    let mut a = auto(&dir);
    if ok(&mut a, "ocr_status", json!({}))["available"] != true {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Editable output words" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 150, "pages": [1] }));
    let png = std::fs::read_dir(dir.join("scan")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": scan, "path": "src/scan.pdf" }));
    // The output never lands on the source.
    let why = a.call("ocr_recognize", &json!({ "doc": scan, "pages": [1], "output_mode": "editable", "out": "src/scan.pdf" })).unwrap_err();
    assert!(why.to_string().contains("source"), "{why}");
    let r = ok(&mut a, "ocr_recognize", json!({ "doc": scan, "pages": [1], "output_mode": "editable", "out": "out/ocr.pdf" }));
    assert!(r["words"].as_u64().unwrap() >= 3, "{r}");
    let ocr = ok(&mut a, "doc_open", json!({ "path": "out/ocr.pdf" }))["doc"].as_u64().unwrap();
    // The text is visible (the engine may misread a letter at this dpi — look for a word that
    // survives).
    let found = ok(&mut a, "text_find", json!({ "doc": ocr, "query": "output" }));
    assert!(found["count"].as_u64() >= Some(1), "the new file's text is visible: {found}");
}

/// #1028: `skip_text_pages: false` reads a picture on a page that has its own text, and the
/// page's own words are left out (counted) instead of being added a second time.
#[test]
fn ocr_recognize_reads_pictures_on_pages_with_text() {
    let dir = workdir("ocr-hybrid");
    let mut a = auto(&dir);
    if ok(&mut a, "ocr_status", json!({}))["available"] != true {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Pictures of words become searchable" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 150, "pages": [1] }));
    let png = std::fs::read_dir(dir.join("scan")).unwrap().next().unwrap().unwrap().path();
    let doc = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "Invoice reference 4711", "at": [40, 500], "size": 24 }));

    let skipped = ok(&mut a, "ocr_recognize", json!({ "doc": doc }));
    assert!(skipped["pages"][0]["skipped"].is_string(), "{skipped}");
    let r = ok(&mut a, "ocr_recognize", json!({ "doc": doc, "skip_text_pages": false }));
    let page = &r["pages"][0];
    let added = page["text"].as_str().unwrap().to_lowercase();
    assert!(added.contains("searchable") && !added.contains("invoice"), "{r}");
    assert!(page["already_on_page"].as_u64().unwrap() >= 2, "{r}");
    let found = |a: &mut Automation, q: &str| ok(a, "text_find", json!({ "doc": doc, "query": q }))["count"].as_u64().unwrap();
    assert_eq!((found(&mut a, "invoice"), found(&mut a, "searchable")), (1, 1));
    assert!(matches!(a.call("ocr_recognize", &json!({ "doc": doc, "skip_text_pages": "no" })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn ocr_recognize_files_refuses_paths_outside_the_root() {
    let dir = workdir("ocr-files-root");
    let mut a = auto(&dir);
    // Refused before anything else, with or without the OCR models installed.
    for path in ["../outside.pdf", "sub/../../outside.pdf", "/outside.pdf"] {
        let r = a.call("ocr_recognize_files", &json!({ "paths": ["a.pdf", path], "folder": "out" }));
        assert!(matches!(&r, Err(e) if e.to_string().contains("outside the allowed directory")), "{path}: {r:?}");
    }
    assert!(!dir.join("out").exists(), "nothing was started");
}

/// Two inputs of one file name from different folders would write the same output in the
/// batch folder: the batch is refused up front, with or without the OCR engines, naming the
/// file, and nothing is written.
#[test]
fn ocr_recognize_files_refuses_inputs_that_share_a_file_name() {
    let dir = workdir("ocr-files-same-name");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "one/scan.pdf" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "two/scan.pdf" }));
    let r = a.call("ocr_recognize_files", &json!({ "paths": ["one/scan.pdf", "two/scan.pdf"], "folder": "out" }));
    assert!(matches!(&r, Err(ToolError::InvalidArgs(m)) if m.contains("scan.pdf") && m.contains("overwrite")), "{r:?}");
    assert!(std::fs::read_dir(dir.join("out")).map_or(true, |mut d| d.next().is_none()), "nothing was written");
}

/// `out` belongs to the editable output. A searchable run (the default) writes into the open
/// document, so an `out` is refused rather than silently ignored, with or without the engines.
#[test]
fn ocr_recognize_refuses_an_out_that_nothing_would_use() {
    let dir = workdir("ocr-out-ignored");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = a.call("ocr_recognize", &json!({ "doc": doc, "out": "../outside.pdf" }));
    assert!(matches!(&r, Err(ToolError::InvalidArgs(m)) if m.contains("editable")), "{r:?}");
    assert!(!dir.join("../outside.pdf").exists());
}

/// The editable output is a NEW file: an existing `out` is refused before anything runs (with
/// or without the engines), and the file is left as it was.
#[test]
fn ocr_recognize_editable_output_never_replaces_an_existing_file() {
    let dir = workdir("ocr-editable-exists");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    std::fs::write(dir.join("taken.pdf"), b"keep me").unwrap();
    let r = a.call("ocr_recognize", &json!({ "doc": doc, "output_mode": "editable", "out": "taken.pdf" }));
    assert!(matches!(&r, Err(e) if e.to_string().contains("already exists")), "{r:?}");
    assert_eq!(std::fs::read(dir.join("taken.pdf")).unwrap(), b"keep me");
}

#[test]
fn ocr_recognize_files_writes_searchable_copies() {
    let dir = workdir("ocr-files");
    let mut a = auto(&dir);
    if ok(&mut a, "ocr_status", json!({}))["available"] != true {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Batch recognition works" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 150, "pages": [1] }));
    let png = std::fs::read_dir(dir.join("scan")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": scan, "path": "in/scan.pdf" }));
    // A file that already has text: the skip-files option passes it over as its own bucket.
    let typed = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Already typed" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": typed, "path": "in/typed.pdf" }));
    // Collision pre-flight: the existing out/scan.pdf (written below) stops a rerun.
    let r = ok(&mut a, "ocr_recognize_files", json!({ "paths": ["in/scan.pdf", "missing.pdf"], "folder": "out" }));
    assert!(r["files"][0]["words"].as_u64().unwrap() >= 3, "{r}");
    assert!(r["files"][1]["error"].is_string(), "{r}");
    let why = a.call("ocr_recognize_files", &json!({ "paths": ["in/scan.pdf"], "folder": "out" })).unwrap_err();
    assert!(matches!(why, ToolError::Failed(_)), "{why}");
    assert!(why.to_string().contains("scan.pdf"), "{why}");
    // The skip-files bucket and the low-confidence note.
    let r = ok(&mut a, "ocr_recognize_files", json!({ "paths": ["in/typed.pdf", "in/scan.pdf"], "folder": "out2", "skip_text_files": true }));
    assert_eq!(r["skipped_files"].as_u64(), Some(1), "{r}");
    assert!(r["files"][0]["skipped"].is_string(), "{r}");
    assert_eq!(r["recognized"].as_u64(), Some(1), "{r}");
    assert!(r["files"][1]["output"].is_string(), "{r}");
    let out = ok(&mut a, "doc_open", json!({ "path": "out/scan.pdf" }))["doc"].as_u64().unwrap();
    let found = ok(&mut a, "text_find", json!({ "doc": out, "query": "recognition" }));
    assert!(found.to_string().contains("\"page\""), "{found}");
}

/// Two inputs that share a file name (from different folders) would both write `out/scan.pdf`:
/// the second output replaces the first while both results report success. The batch is refused
/// up front, naming the file, before the folder is created, anything is read or the OCR engines
/// load, so the outcome is the same with or without the models installed.
/// File names are compared without regard to case on Windows and macOS (`scan.pdf` and `Scan.PDF`
/// are one file there), and exactly elsewhere, where they are two files and the batch is not
/// refused.
#[test]
fn ocr_recognize_files_compares_names_the_way_the_filesystem_does() {
    let dir = workdir("ocr-files-case");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "one/scan.pdf" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "two/Scan.PDF" }));
    let r = a.call("ocr_recognize_files", &json!({ "paths": ["one/scan.pdf", "two/Scan.PDF"], "folder": "out" }));
    if cfg!(any(windows, target_os = "macos")) {
        assert!(matches!(&r, Err(ToolError::InvalidArgs(m)) if m.contains("Scan.PDF")), "{r:?}");
        assert!(!dir.join("out").exists(), "nothing was started: the output folder was not created");
    } else {
        assert!(!matches!(&r, Err(ToolError::InvalidArgs(_))), "{r:?}");
    }
}

/// Inputs with distinct names are not refused, and the batch writes one searchable copy per input
/// under its own name. Without the models it stops at the models error, as it did before.
#[test]
fn ocr_recognize_files_with_distinct_names_runs_as_before() {
    let dir = workdir("ocr-files-distinct");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "one/scan.pdf" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "two/other.pdf" }));
    if ok(&mut a, "ocr_status", json!({}))["available"] == true {
        let r = ok(&mut a, "ocr_recognize_files", json!({ "paths": ["one/scan.pdf", "two/other.pdf"], "folder": "out" }));
        assert!(r["files"][0]["error"].is_null() && r["files"][1]["error"].is_null(), "{r}");
        assert!(dir.join("out/scan.pdf").is_file() && dir.join("out/other.pdf").is_file(), "{r}");
    } else {
        let r = a.call("ocr_recognize_files", &json!({ "paths": ["one/scan.pdf", "two/other.pdf"], "folder": "out" }));
        assert!(matches!(&r, Err(ToolError::Failed(m)) if m.contains("not installed")), "{r:?}");
    }
}

/// Field scripts set through the tools validate, calculate and format, and `js_run` reports console
/// lines, every message with its details, and requests.
#[test]
fn javascript_through_tools() {
    let dir = workdir("js");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (name, y) in [("Price", 20), ("Qty", 60), ("Total", 100)] {
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    ok(
        &mut a,
        "js_set_document_script",
        json!({ "doc": doc, "name": "lib", "script": "function money(v) { return util.printf('EUR %,2.2f', v); }" }),
    );
    assert_eq!(ok(&mut a, "js_document_scripts", json!({ "doc": doc }))["scripts"][0]["name"], "lib");
    ok(
        &mut a,
        "form_set_script",
        json!({ "doc": doc, "field": "Total", "event": "calculate", "script": "event.value = getField('Price').value * getField('Qty').value;" }),
    );
    ok(&mut a, "form_set_script", json!({ "doc": doc, "field": "Total", "event": "format", "script": "event.value = money(event.value);" }));
    ok(
        &mut a,
        "form_set_script",
        json!({ "doc": doc, "field": "Qty", "event": "validate", "script": "if (event.value < 1) { app.alert('Order at least one'); event.rc = false; }" }),
    );
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "Price": "1250", "Qty": "2" } }));
    assert!(page_text(&mut a, doc)[0].contains("EUR 2.500,00"), "{:?}", page_text(&mut a, doc));
    let err = a.call("form_fill", &json!({ "doc": doc, "values": { "Qty": "0" } })).unwrap_err();
    assert!(err.to_string().contains("Order at least one"), "{err}");

    let r = ok(
        &mut a,
        "js_run",
        json!({ "doc": doc, "script": "console.println(getField('Total').value); getField('Qty').value = 3; this.pageNum = 1; app.alert('ok'); app.alert({ cMsg: 'Clear?', cTitle: 'Order', nIcon: 2, nType: 3 });" }),
    );
    assert_eq!(r["console"][0], "2500");
    // Every message, in order, with its title, icon and buttons.
    assert_eq!(
        r["alerts"],
        json!([
            { "message": "ok", "title": null, "icon": "error", "buttons": "ok" },
            { "message": "Clear?", "title": "Order", "icon": "question", "buttons": "yes_no_cancel" },
        ])
    );
    assert_eq!(r["requests"][0]["page"], 2);
    let f = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let total = f["fields"].as_array().unwrap().iter().find(|x| x["name"] == "Total").unwrap().clone();
    assert_eq!(total["value"], "3750");
    let r = ok(&mut a, "js_run", json!({ "doc": doc, "script": "nope()" }));
    assert!(r["error"].as_str().unwrap().contains("nope"));
    assert_eq!(r["document_errors"], json!([]));
    // A document script that throws (#3940) is reported, and the others and the script still run.
    ok(&mut a, "js_set_document_script", json!({ "doc": doc, "name": "zz", "script": "getField('Total').buttonSetIcon(null);" }));
    let r = ok(&mut a, "js_run", json!({ "doc": doc, "script": "money(5)" }));
    assert_eq!((r["error"].clone(), r["result"].clone()), (json!(null), json!("EUR 5,00")));
    assert!(r["document_errors"][0].as_str().unwrap().starts_with("document script 2: TypeError"), "{r}");
    assert_eq!(ok(&mut a, "js_enabled", json!({ "enabled": false }))["enabled"], false);
    assert!(a.call("js_run", &json!({ "doc": doc, "script": "1" })).is_err());
}

#[test]
fn merging_form_data_into_a_spreadsheet() {
    let dir = workdir("merge-data");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, 20, 180, 42], "name": "City" }));
    for (city, file) in [("Paris", "one.xfdf"), ("Oslo", "two.fdf")] {
        ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "City": city } }));
        ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": file, "what": "fields" }));
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "filled.pdf" }));
    let r = ok(&mut a, "form_merge_data", json!({ "paths": ["one.xfdf", "two.fdf", "filled.pdf"], "path": "report.csv" }));
    assert_eq!(r["rows"], 3);
    assert_eq!(std::fs::read_to_string(dir.join("report.csv")).unwrap(), "City\nParis\nOslo\nOslo\n");
}

#[test]
fn field_actions_through_tools() {
    let dir = workdir("field-actions");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "button", "rect": [20, 20, 120, 42], "name": "Go" }));
    let r = ok(
        &mut a,
        "form_set_actions",
        json!({ "doc": doc, "field": "Go", "actions": [
            { "trigger": "mouse_up", "javascript": "this.pageNum = 2;" },
            { "trigger": "mouse_enter", "hide": ["Go"] },
            { "trigger": "on_focus", "page": 3 }
        ] }),
    );
    assert_eq!(r["actions"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(r["actions"][0], json!({ "trigger": "mouse_up", "javascript": "this.pageNum = 2;" }));
    assert_eq!(r["actions"][1], json!({ "trigger": "mouse_enter", "hide": ["Go"] }));
    assert_eq!(r["actions"][2], json!({ "trigger": "on_focus", "page": 3 }));
    let run = ok(&mut a, "js_run", json!({ "doc": doc, "script": "this.pageNum = 2;", "field": "Go" }));
    assert_eq!(run["requests"][0]["page"], 3);
    assert!(a.call("form_set_actions", &json!({ "doc": doc, "field": "Go", "actions": [{ "trigger": "wave" }] })).is_err());
}

#[test]
fn detecting_form_fields_through_tools() {
    let dir = workdir("detect-fields");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Name: ____________________\n\nCity: ____________________" }))["doc"]
        .as_u64()
        .unwrap();
    let r = ok(&mut a, "form_detect_fields", json!({ "doc": doc, "add": false }));
    assert_eq!(r["fields"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(r["fields"][1]["name"], "City");
    assert_eq!(ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"].as_array().unwrap().len(), 0);
    ok(&mut a, "form_detect_fields", json!({ "doc": doc }));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "City": "Lisbon" } }));
    let f = ok(&mut a, "form_fields", json!({ "doc": doc }));
    assert_eq!(f["fields"][1]["value"], "Lisbon", "{f}");
}

#[test]
fn detecting_form_fields_preserves_content_piece_comments() {
    for visible in [false, true] {
        let first = format!("BT /F1 18 Tf 40 250 Td (FIRST) Tj ET % trailing comment{}", if visible { "\n" } else { "" });
        let last = b"40 120 300 30 re S\nBT /F1 18 Tf 40 50 Td (LAST) Tj ET\n";
        let mut source = pdfcraft_cos::Document::open(std::sync::Arc::new(fixture(1))).unwrap();
        let page = pdfcraft_model::pages(&source)[0].obj;
        let contents = [first.as_bytes(), last]
            .iter()
            .map(|bytes| pdfcraft_cos::Object::Ref(source.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(Default::default(), bytes)))))
            .collect();
        source
            .update_dict(page, |d| {
                d.set(b"Contents".to_vec(), pdfcraft_cos::Object::Array(contents));
                d.set(b"MediaBox".to_vec(), pdfcraft_cos::Object::Array(vec![0.into(), 0.into(), 400.into(), 300.into()]));
            })
            .unwrap();
        let dir = workdir(if visible { "detect-visible-piece" } else { "detect-hidden-piece" });
        std::fs::write(dir.join("pieces.pdf"), pdfcraft_cos::write_full(&source, &Default::default()).unwrap()).unwrap();
        let mut a = auto(&dir);
        let doc = ok(&mut a, "doc_open", json!({"path": "pieces.pdf"}))["doc"].as_u64().unwrap();
        let candidates = if visible { json!([{"page": 1, "kind": "text", "name": "Text1", "rect": [41.0, 121.0, 339.0, 149.0]}]) } else { json!([]) };
        let inventory = if visible {
            json!([{"name": "Text1", "type": "text", "value": null, "page": 1, "rect": [41.0, 151.0, 339.0, 179.0], "rotation": 0, "read_only": false, "required": false, "multiline": false}])
        } else {
            json!([])
        };
        let check_text = |a: &mut Automation, doc| {
            let text = page_text(a, doc).join(" ");
            assert_eq!(text.split_whitespace().collect::<Vec<_>>(), ["FIRST", "LAST"]);
        };
        let check_render = |a: &mut Automation, doc| {
            let (width, height, pixels) = a.render_pixels(doc, 1, 1.0).unwrap();
            assert_eq!((width, height), (400, 300));
            let region: Vec<_> = (145..185).flat_map(|y| (35..345).map(move |x| (y * 400 + x) * 4)).collect();
            if visible {
                assert!(region.iter().any(|&i| pixels[i..i + 3].iter().all(|&v| v < 128)), "the visible box has ink");
            } else {
                assert!(region.iter().all(|&i| pixels[i..i + 3] == [255, 255, 255]), "the hidden box region stays white");
            }
        };
        check_text(&mut a, doc);
        check_render(&mut a, doc);
        assert_eq!(ok(&mut a, "form_fields", json!({"doc": doc}))["fields"], json!([]));
        assert_eq!(ok(&mut a, "form_detect_fields", json!({"doc": doc, "add": false}))["fields"], candidates);
        assert_eq!(ok(&mut a, "form_fields", json!({"doc": doc}))["fields"], json!([]));
        assert_eq!(ok(&mut a, "form_detect_fields", json!({"doc": doc}))["fields"], candidates);
        assert_eq!(ok(&mut a, "form_fields", json!({"doc": doc}))["fields"], inventory);
        assert_eq!(ok(&mut a, "form_detect_fields", json!({"doc": doc}))["fields"], json!([]));
        ok(&mut a, "doc_save", json!({"doc": doc, "path": "saved.pdf"}));
        let mut reopened = auto(&dir);
        let again = ok(&mut reopened, "doc_open", json!({"path": "saved.pdf"}))["doc"].as_u64().unwrap();
        assert_eq!(ok(&mut reopened, "form_fields", json!({"doc": again}))["fields"], inventory);
        check_text(&mut reopened, again);
        check_render(&mut reopened, again);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn detecting_comb_boxes_through_tools() {
    let mut body = String::from("BT /F1 10 Tf 40 105 Td (Postcode) Tj ET\n");
    for i in 0..5 {
        body.push_str(&format!("{} 100 16 20 re S\n", 120 + i * 16));
    }
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 400 300] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let dir = workdir("detect-comb");
    std::fs::write(dir.join("comb.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "comb.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "form_detect_fields", json!({ "doc": doc }));
    assert_eq!(r["fields"], json!([{ "page": 1, "kind": "text", "comb": 5, "name": "Postcode", "rect": [120.0, 100.0, 200.0, 120.0] }]), "{r}");
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "Postcode": "75001" } }));
    assert_eq!(ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"][0]["value"], "75001");
}

#[test]
fn comparing_documents_through_tools() {
    let dir = workdir("compare");
    let mut a = auto(&dir);
    let v1 = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Rent is 900 per month. Pets are not allowed." }))["doc"].as_u64().unwrap();
    let v2 = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Rent is 950 per month. Pets are allowed. Parking included." }))["doc"]
        .as_u64()
        .unwrap();
    let r = ok(&mut a, "doc_compare", json!({ "doc": v2, "other": v1, "visual": true }));
    assert!(!r["visual"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!((r["replaced"].as_u64(), r["inserted"].as_u64(), r["deleted"].as_u64()), (Some(1), Some(1), Some(1)), "{r}");
    assert_eq!(r["changes"][0]["old"]["text"], "900");
    assert_eq!(r["changes"][0]["new"]["text"], "950");
    assert_eq!(r["changes"][0]["new"]["page"], 1);
    ok(&mut a, "doc_compare_report", json!({ "doc": v2, "other": v1, "path": "report.pdf" }));
    assert!(std::fs::read(dir.join("report.pdf")).unwrap().starts_with(b"%PDF"));
    assert_eq!(ok(&mut a, "doc_compare_mark", json!({ "doc": v2, "other": v1 }))["comments"], 3);
    assert!(a.call("doc_compare", &json!({ "doc": v2, "other": 999 })).is_err());
}

#[test]
fn actions_through_tools() {
    let dir = workdir("actions");
    let mut a = auto(&dir);
    let list = ok(&mut a, "action_list", json!({}));
    assert!(list["actions"].as_array().unwrap().iter().any(|x| x["name"] == "Prepare for Distribution"));
    assert!(list["steps"].as_array().unwrap().iter().any(|x| x["step"] == "add_watermark" && x["takes_arg"] == true));
    let r = ok(&mut a, "action_run", json!({ "action": "add page numbers", "paths": ["a.pdf", "b.pdf", "missing.pdf"], "folder": "out" }));
    assert_eq!(r["files"][0]["log"][0], "Add footer");
    assert!(r["files"][2]["error"].is_string());
    let doc = ok(&mut a, "doc_open", json!({ "path": "out/b.pdf" }))["doc"].as_u64().unwrap();
    assert!(page_text(&mut a, doc)[1].contains("Page 2 of 2"), "{:?}", page_text(&mut a, doc));
    let r = ok(&mut a, "action_run", json!({ "steps": [{ "step": "set_title", "arg": "Hello" }], "paths": ["a.pdf"], "folder": "out2" }));
    assert_eq!(r["files"][0]["log"][0], "Set document title");
    assert!(a.call("action_run", &json!({ "steps": [{ "step": "fly" }], "paths": ["a.pdf"], "folder": "x" })).is_err());
}

#[test]
fn action_run_keeps_outputs_with_duplicate_input_basenames() {
    let dir = workdir("action-duplicate-basenames");
    std::fs::create_dir_all(dir.join("left")).unwrap();
    std::fs::create_dir_all(dir.join("right")).unwrap();
    std::fs::write(dir.join("left/report.pdf"), fixture(1)).unwrap();
    std::fs::write(dir.join("right/report.pdf"), fixture(2)).unwrap();
    let mut a = auto(&dir);
    let args = json!({
        "steps": [{ "step": "set_title", "arg": "Processed" }],
        "paths": ["left/report.pdf", "right/report.pdf"],
        "folder": "results"
    });
    let result = ok(&mut a, "action_run", args.clone());
    let files = result["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    let first_path = files[0]["output"].as_str().unwrap();
    let second_path = files[1]["output"].as_str().unwrap();
    assert!(first_path.ends_with("report.pdf"), "{first_path}");
    assert!(second_path.ends_with("report (2).pdf"), "{second_path}");
    let first_bytes = std::fs::read(first_path).unwrap();
    assert_eq!(ok(&mut a, "doc_open", json!({ "path": first_path }))["pages"], 1);
    assert_eq!(ok(&mut a, "doc_open", json!({ "path": second_path }))["pages"], 2);

    let repeated = ok(&mut a, "action_run", args);
    assert!(repeated["files"][0]["output"].as_str().unwrap().ends_with("report (3).pdf"));
    assert!(repeated["files"][1]["output"].as_str().unwrap().ends_with("report (4).pdf"));
    assert_eq!(std::fs::read(first_path).unwrap(), first_bytes, "an earlier output must not be replaced");
}

#[test]
fn pdfa_through_tools() {
    let dir = workdir("pdfa");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Archive me" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "pdfa_verify", json!({ "doc": doc }));
    assert_eq!(r["compliant"], false);
    assert!(r["issues"].as_array().unwrap().iter().any(|i| i["clause"] == "6.6.2.1"), "{r}");
    let r = ok(&mut a, "pdfa_convert", json!({ "doc": doc, "level": "3b" }));
    assert_eq!(r["declared"]["pdfa"], "PDF/A-3b", "{r}");
    assert!(r["issues"].as_array().unwrap().iter().all(|i| i["fixable"] == false), "{r}");
    assert!(a.call("pdfa_verify", &json!({ "doc": doc, "level": "9z" })).is_err());
    ok(&mut a, "standards_remove_id", json!({ "doc": doc, "standard": "pdfa" }));
    let r = ok(&mut a, "pdfa_verify", json!({ "doc": doc }));
    assert!(r["declared"]["pdfa"].is_null() && !r["declared"]["output_intents"].as_array().unwrap().is_empty(), "{r}");
    assert!(matches!(a.call("standards_remove_id", &json!({ "doc": doc, "standard": "pdfa" })), Err(ToolError::Failed(_))), "nothing left");
}

#[test]
fn print_readiness_through_tools() {
    let dir = workdir("print-readiness");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Proof me" }))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "doc_info", json!({ "doc": doc }));
    let r = ok(&mut a, "print_readiness", json!({ "doc": doc }));
    assert!(r["text"].as_str().unwrap().contains("verification only"), "{r}");
    assert_eq!(r["pages"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["pages"][0]["trim_set"], false, "{r}");
    assert_eq!(r["resolution_unknown"], 0, "{r}");
    let findings = r["findings"].as_array().unwrap();
    // A page without a TrimBox is one informational note for the document, not a bleed warning.
    assert!(!findings.iter().any(|f| f["id"] == "page-bleed"), "{r}");
    let trim: Vec<_> = findings.iter().filter(|f| f["id"] == "page-trim").collect();
    assert_eq!(trim.len(), 1, "{r}");
    assert_eq!(trim[0]["severity"], "info");
    assert!(trim[0]["page"].is_null(), "{r}");
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc })), before, "verification doesn't change the document");
    assert!(a.call("print_readiness", &json!({ "doc": 999 })).is_err());
    assert!(pdfcraft_automation::tools().iter().any(|t| t.name == "print_readiness" && t.read_only));
}

#[test]
fn exporting_to_word_html_and_rtf() {
    let dir = workdir("office");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for ext in ["docx", "pptx", "html", "rtf", "md"] {
        let r = ok(&mut a, "doc_export_office", json!({ "doc": doc, "path": format!("a.{ext}") }));
        assert_eq!(r["format"], ext);
    }
    let html = std::fs::read_to_string(dir.join("a.html")).unwrap();
    assert!(html.contains("Page 1") && html.contains("Page 3") && html.matches("<hr>").count() == 2, "{html}");
    ok(&mut a, "doc_export_office", json!({ "doc": doc, "path": "simple.html", "html_mode": "simple" }));
    let simple_html = std::fs::read_to_string(dir.join("simple.html")).unwrap();
    assert!(
        simple_html.contains("Page 1")
            && !simple_html.contains("<style")
            && !simple_html.contains("style=")
            && !simple_html.contains("<span")
            && !simple_html.contains("<div")
    );
    assert!(a.call("doc_export_office", &json!({ "doc": doc, "path": "bad.html", "html_mode": "compact" })).is_err());
    assert!(a.call("doc_export_office", &json!({ "doc": doc, "path": "bad.docx", "html_mode": "simple" })).is_err());
    assert!(std::fs::read(dir.join("a.docx")).unwrap().starts_with(b"PK"));
    assert!(std::fs::read(dir.join("a.pptx")).unwrap().starts_with(b"PK"));
    assert!(std::fs::read_to_string(dir.join("a.rtf")).unwrap().contains("Page 2"));
    // #1595: Markdown, with a rule between pages.
    let md = std::fs::read_to_string(dir.join("a.md")).unwrap();
    assert!(md.contains("Page 1") && md.contains("Page 3") && md.matches("\n---\n").count() == 2, "{md}");
    assert_eq!(ok(&mut a, "doc_export_office", json!({ "doc": doc, "path": "b.markdown" }))["format"], "md");
    assert!(a.call("doc_export_office", &json!({ "doc": doc, "path": "a.xyz" })).is_err());
}

/// #3120: export reads forms as deeply nested as the renderer draws them (13 used to lose their
/// text), and its result says which pages draw forms past that, whose text is missing.
#[test]
fn exporting_office_files_says_which_pages_left_form_text_out() {
    use pdfcraft_engine::compare::{MAX_FORM_DEPTH, MAX_FORM_VISITS};
    let dir = workdir("office-forms");
    std::fs::write(dir.join("deep.pdf"), nested_forms_pdf(&[13, MAX_FORM_DEPTH + 1, MAX_FORM_DEPTH])).unwrap();
    std::fs::write(dir.join("fine.pdf"), nested_forms_pdf(&[13])).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "deep.pdf" }))["doc"].as_u64().unwrap();
    for ext in ["html", "docx"] {
        let r = ok(&mut a, "doc_export_office", json!({ "doc": doc, "path": format!("deep.{ext}") }));
        assert_eq!((&r["truncated"], &r["truncated_pages"]), (&json!(true), &json!([2])), "{r}");
        let warning = r["warnings"][0].as_str().unwrap();
        assert!(warning.contains(&format!("{MAX_FORM_DEPTH} deep")) && warning.contains(&MAX_FORM_VISITS.to_string()), "{warning}");
        assert!(warning.contains("truncated_pages"), "{warning}");
    }
    let html = std::fs::read_to_string(dir.join("deep.html")).unwrap();
    let tails: Vec<usize> = html.split("<hr>").map(|page| page.matches("TAIL").count()).collect();
    assert_eq!(tails, [1, 0, 1], "{html}");
    assert_eq!(html.matches("Head").count(), 3, "{html}");

    let fine = ok(&mut a, "doc_open", json!({ "path": "fine.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_export_office", json!({ "doc": fine, "path": "fine.html" }));
    assert_eq!((&r["truncated"], &r["truncated_pages"]), (&json!(false), &json!([])), "{r}");
    assert!(r.get("warnings").is_none(), "{r}");
    assert!(std::fs::read_to_string(dir.join("fine.html")).unwrap().contains("TAIL"));
}

/// #773: a diagonal watermark must not enter the body flow or move invoice fields.
#[test]
fn exporting_an_invoice_to_word_keeps_the_watermark_out_of_flow() {
    use pdfcraft_cos::{Dict, Document, ObjRef, SaveOptions, Stream, write_full};
    let dir = workdir("invoice-word");
    let mut pdf = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let content = b"q BT /F1 60 Tf 0.8 g 0.70710678 0.70710678 -0.70710678 0.70710678 30 80 Tm (DRAFT) Tj ET Q \
BT /F1 11 Tf 20 250 Td (Description) Tj 110 0 Td (Amount) Tj \
-110 -20 Td (Design) Tj 110 0 Td (100.00) Tj \
-110 -20 Td (Review) Tj 110 0 Td (50.00) Tj ET \
BT /F1 11 Tf 20 20 Td (Total due: 150.00) Tj ET";
    pdf.set(ObjRef::new(5, 0), Stream { dict: Dict::new(), raw: content.to_vec().into() });
    std::fs::write(dir.join("invoice.pdf"), write_full(&pdf, &SaveOptions::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "invoice.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_office", json!({ "doc": doc, "path": "invoice.docx" }));
    // Inspect the actual package emitted by the tool, not an intermediate page model.
    let zip = std::fs::read(dir.join("invoice.docx")).unwrap();
    let mut at = 0;
    let mut xml = String::new();
    while zip[at..].starts_with(b"PK\x03\x04") {
        let size = u32::from_le_bytes(zip[at + 18..at + 22].try_into().unwrap()) as usize;
        let len = u16::from_le_bytes(zip[at + 26..at + 28].try_into().unwrap()) as usize;
        let data = at + 30 + len;
        if &zip[at + 30..data] == b"word/document.xml" {
            std::io::Read::read_to_string(&mut flate2::read::DeflateDecoder::new(&zip[data..data + size]), &mut xml).unwrap();
            break;
        }
        at = data + size;
    }
    assert!(xml.contains("rotation:315.000"), "diagonal text survives extraction: {xml}");
    assert!(xml.contains("z-index:-2"), "the large watermark stays behind the invoice");
    assert!(!xml.contains("<wp:inline>") && !xml.contains("<w:tbl>"));
    assert!(!xml.contains("w:val=\"single\""));
    assert_eq!(xml.matches(">DRAFT</w:t>").count(), 1);
    assert_eq!(xml.matches(">Total due: 150.00</w:t>").count(), 1);
    assert_eq!(xml.matches("<w:sectPr>").count(), 1);
    assert!(!xml.contains("w:type=\"page\""));
    assert!(xml.contains("margin-left:20.000pt"), "invoice fields retain their left edge");
    let rendered = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    assert!(rendered.iter().any(|c| matches!(c, Content::Png { .. })));
}

/// #4158: a word set in italic inside a sentence ("Before *Chevron* after.") reaches Word as an
/// italic run of the sentence's paragraph, not as a paragraph of its own between two halves.
#[test]
fn exporting_to_word_keeps_an_italic_word_inside_its_sentence() {
    let dir = workdir("italic-run-word");
    let content = "BT /F1 12 Tf 72 700 Td (Before ) Tj /F2 12 Tf (Chevron) Tj /F1 12 Tf ( after.) Tj ET \
BT /F1 12 Tf 72 640 Td (A paragraph further down.) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    std::fs::write(dir.join("opinion.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "opinion.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_office", json!({ "doc": doc, "path": "opinion.docx" }));
    let zip = std::fs::read(dir.join("opinion.docx")).unwrap();
    let mut at = 0;
    let mut xml = String::new();
    while zip[at..].starts_with(b"PK\x03\x04") {
        let size = u32::from_le_bytes(zip[at + 18..at + 22].try_into().unwrap()) as usize;
        let len = u16::from_le_bytes(zip[at + 26..at + 28].try_into().unwrap()) as usize;
        let data = at + 30 + len;
        if &zip[at + 30..data] == b"word/document.xml" {
            std::io::Read::read_to_string(&mut flate2::read::DeflateDecoder::new(&zip[data..data + size]), &mut xml).unwrap();
            break;
        }
        at = data + size;
    }
    // The paragraphs that hold text, each as its runs: (text, italic).
    let paragraphs: Vec<Vec<(String, bool)>> = xml
        .split("<w:p>")
        .skip(1)
        .map(|p| {
            let p = p.split_once("</w:p>").map_or(p, |(inside, _)| inside);
            p.split("<w:r>")
                .skip(1)
                .filter_map(|r| Some((r.split_once("<w:t xml:space=\"preserve\">")?.1.split_once("</w:t>")?.0.to_string(), r.contains("<w:i/>"))))
                .collect::<Vec<_>>()
        })
        .filter(|runs| !runs.is_empty())
        .collect();
    let sentence = paragraphs.iter().find(|p| p.iter().any(|(t, _)| t.contains("Before"))).unwrap_or_else(|| panic!("{xml}"));
    assert_eq!(sentence.len(), 3, "one paragraph, three runs: {xml}");
    assert_eq!(sentence.iter().map(|(t, _)| t.as_str()).collect::<String>(), "Before Chevron after.", "{xml}");
    assert_eq!(sentence.iter().map(|(_, i)| *i).collect::<Vec<_>>(), [false, true, false], "{xml}");
    assert_eq!(paragraphs.len(), 2, "{xml}");
}

/// Hundreds of calculations run on opening and again after a change through the tools. The
/// time limits are raised so that a busy machine does not stop them (the engine's tests check
/// that the limits stop them).
#[test]
fn hundreds_of_xfa_calculations_open_and_recalculate_through_tools() {
    let dir = workdir("xfa-many");
    let (sections, rows) = (10, 20);
    std::fs::write(dir.join("many.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::many_calculations_template(sections, rows))).unwrap();
    let mut a = auto(&dir).with_xfa_time_scale(100);
    let started = std::time::Instant::now();
    let opened = ok(&mut a, "doc_open", json!({ "path": "many.pdf" }));
    let open_took = started.elapsed();
    let doc = opened["doc"].as_u64().unwrap();
    let warnings = opened["warnings"].to_string();
    assert!(!warnings.contains("were skipped") && !warnings.contains("stopped"), "{warnings}");
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let totals: Vec<&serde_json::Value> =
        fields["fields"].as_array().unwrap().iter().filter(|f| f["name"].as_str().is_some_and(|n| n.starts_with("total"))).collect();
    assert_eq!(totals.len(), sections * rows);
    assert!(totals.iter().all(|f| !f["value"].as_str().unwrap_or("").is_empty()), "every total calculated");
    let started = std::time::Instant::now();
    let r = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "qty": "100" } }));
    let change_took = started.elapsed();
    assert!(!r.to_string().contains("were skipped"), "{r}");
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let total = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "total").unwrap();
    assert_eq!(total["value"], "200");
    eprintln!("{} calculate scripts: open {open_took:?}, recalculation after a change {change_took:?}", sections * rows + sections);
}

#[test]
fn xfa_forms_flatten_and_convert_to_static_through_tools() {
    let dir = workdir("xfa-flat");
    let data = "<form><page1><country>JP</country></page1></form>";
    std::fs::write(dir.join("fields.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data))).unwrap();
    let mut a = auto(&dir);
    // Convert to static: the fields stay, the XFA goes.
    let doc = ok(&mut a, "doc_open", json!({ "path": "fields.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["xfa"], "dynamic");
    let before = ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"].as_array().unwrap().len();
    ok(&mut a, "xfa_to_static", json!({ "doc": doc }));
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert!(info["xfa"].is_null(), "{info}");
    assert_eq!(ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"].as_array().unwrap().len(), before);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "static.pdf" }));
    let again = ok(&mut a, "doc_open", json!({ "path": "static.pdf" }))["doc"].as_u64().unwrap();
    assert!(ok(&mut a, "doc_info", json!({ "doc": again }))["xfa"].is_null());
    assert!(a.call("xfa_to_static", &json!({ "doc": again })).is_err(), "not an XFA form");
    // Flatten the fields: a plain PDF with the values as page content.
    let doc = ok(&mut a, "doc_open", json!({ "path": "fields.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_flatten", json!({ "doc": doc, "comments": false, "fields": true }));
    assert!(ok(&mut a, "doc_info", json!({ "doc": doc }))["xfa"].is_null());
    assert_eq!(ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"].as_array().unwrap().len(), 0);
    let text = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1] }));
    assert!(text.to_string().contains("Japan"), "{text}");
}

#[test]
fn xfa_data_files_export_and_import_through_tools() {
    let dir = workdir("xfa-data");
    let data = "<form><page1><country>JP</country></page1></form>";
    std::fs::write(dir.join("fields.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data))).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "fields.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "data.xml", "what": "fields" }));
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "data.xdp", "what": "fields" }));
    let xml = std::fs::read_to_string(dir.join("data.xml")).unwrap();
    let xdp = std::fs::read_to_string(dir.join("data.xdp")).unwrap();
    assert!(xml.contains("<form") && xml.contains("<country>JP</country>"), "{xml}");
    assert!(xdp.contains("<xdp:xdp") && xdp.contains("<country>JP</country>"), "{xdp}");
    // Edit the exported data and import it back.
    std::fs::write(dir.join("edited.xdp"), xdp.replace("<country>JP</country>", "<country>CA</country>")).unwrap();
    let r = ok(&mut a, "doc_import_data", json!({ "doc": doc, "path": "edited.xdp" }));
    assert!(r["filled_fields"].as_u64().unwrap() >= 1);
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let country = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "country").unwrap();
    assert_eq!(country["value"], json!("CA"));
    // A picture for the image field, through form_set_image, lands in the data.
    std::fs::write(dir.join("pic.gif"), pdfcraft_xfa::fixtures::TINY_GIF).unwrap();
    ok(&mut a, "form_set_image", json!({ "doc": doc, "field": "photo", "path": "pic.gif" }));
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "after.xml", "what": "fields" }));
    let after = std::fs::read_to_string(dir.join("after.xml")).unwrap();
    assert!(after.contains("<photo xfa:contentType=\"image/gif\">R0lGODlh"), "{after}");
    // XDP is for XFA forms only.
    std::fs::write(dir.join("plain.pdf"), pdfcraft_xfa::fixtures::static_shell("<form1><page1><name>Ada</name></page1></form1>")).unwrap();
    let plain = ok(&mut a, "doc_open", json!({ "path": "plain.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_data", json!({ "doc": plain, "path": "static.xml", "what": "fields" }));
    assert!(std::fs::read_to_string(dir.join("static.xml")).unwrap().contains("<name>Ada</name>"), "a static XFA form exports its data too");
}

#[test]
fn xfa_rich_text_max_length_is_reported_and_enforced_through_tools() {
    let dir = workdir("xfa-rich-limit");
    let template = pdfcraft_xfa::fixtures::template_with_data(0, "<form><head><familyName>Old</familyName></head></form>")
        .replace(r#"<text maxChars="30"/>"#, r#"<exData contentType="text/html" maxLength="5"/>"#)
        .replace("<textEdit>", "<textEdit allowRichText=\"1\">");
    std::fs::write(dir.join("rich.pdf"), pdfcraft_xfa::fixtures::shell(&template)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "rich.pdf" }))["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let family = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "familyName").unwrap();
    assert_eq!(family["max_length"], 5);

    // The boundary counts Unicode characters, not UTF-8 bytes.
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "familyName": "éàöüñ" } }));
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "before.xml", "what": "fields" }));
    let before = std::fs::read(dir.join("before.xml")).unwrap();
    assert!(std::str::from_utf8(&before).unwrap().contains("<familyName>éàöüñ</familyName>"));
    let error = a.call("form_fill", &json!({ "doc": doc, "values": { "familyName": "éàöüñX" } })).unwrap_err();
    assert!(error.to_string().contains("at most 5 characters"), "{error}");
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "after.xml", "what": "fields" }));
    assert_eq!(std::fs::read(dir.join("after.xml")).unwrap(), before, "refused edits leave the XFA data unchanged");

    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "out.pdf" }))["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let family = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "familyName").unwrap();
    assert_eq!(family["max_length"], 5);
    assert_eq!(family["value"], "éàöüñ");
}

/// A link in a dynamic XFA form's rich text is a link on the laid-out page, as link_list reports
/// it, and stays one after a save and a fresh open.
#[test]
fn links_in_xfa_rich_text_are_listed_through_tools() {
    let dir = workdir("xfa-links");
    let xdp = r##"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="form" layout="tb">
 <pageSet><pageArea name="page"><contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/><medium short="8.5in" long="11in"/></pageArea></pageSet>
 <draw name="help" w="6in"><font typeface="Arial" size="10pt"/><value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p>Questions? Email <a href="mailto:forms@example.com">forms@example.com</a>.</p></body></exData></value></draw>
 <field name="name" w="3in" h="0.3in"><ui><textEdit/></ui></field>
</subform></template></xdp:xdp>"##;
    std::fs::write(dir.join("links.pdf"), pdfcraft_xfa::fixtures::shell(xdp)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "links.pdf" }))["doc"].as_u64().unwrap();
    let urls = |a: &mut Automation, doc: u64| -> Vec<String> {
        let listed = ok(a, "link_list", json!({ "doc": doc }));
        let links = listed["links"].as_array().cloned().unwrap_or_default();
        links.iter().filter_map(|l| l["url"].as_str().map(str::to_string)).collect()
    };
    assert_eq!(urls(&mut a, doc), ["mailto:forms@example.com"]);
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "name": "Ada" } }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "saved.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "saved.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(urls(&mut a, doc), ["mailto:forms@example.com"], "one link after a save and a fresh open, not two");
}

#[test]
fn dynamic_xfa_forms_open_render_fill_and_save_through_tools() {
    let dir = workdir("xfa");
    std::fs::write(dir.join("xfa.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template(2))).unwrap();
    let mut a = auto(&dir);
    let opened = ok(&mut a, "doc_open", json!({ "path": "xfa.pdf" }));
    assert_eq!(opened["pages"], 2, "laid out from the template, not the placeholder page");
    let doc = opened["doc"].as_u64().unwrap();
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert_eq!(info["xfa"], "dynamic");
    assert_eq!(info["xfa_layout"]["pages"], 2);
    assert_eq!(info["xfa_layout"]["fields"], 11);
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let family = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "familyName").expect("familyName");
    assert_eq!((family["type"].as_str(), family["tooltip"].as_str(), family["page"].as_u64()), (Some("text"), Some("Your family name"), Some(1)));
    let answer = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "answer").expect("radio group");
    assert_eq!(answer["options"], json!(["Y", "N"]));
    // The page renders with the widgets' own appearances: the check box's border is drawn.
    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 36 })).unwrap();
    let Content::Png { data, .. } = &png[0] else { panic!("expected an image") };
    let decoder = png::Decoder::new(std::io::Cursor::new(data.as_slice()));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    assert!(buf.iter().filter(|b| **b < 128).count() > 200, "the page is not blank");
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "familyName": "Singh", "agree": true, "answer": "N", "born": "2001-02-03" } }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "out.pdf" }));
    assert_eq!(reopened["pages"], 2, "not laid out twice");
    let doc2 = reopened["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc2 }));
    let by = |n: &str| fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == n).unwrap()["value"].clone();
    assert_eq!((by("familyName"), by("agree"), by("answer"), by("born")), (json!("Singh"), json!(true), json!("N"), json!("2001-02-03")));
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc2 }))["xfa_layout"]["pages"], 2);
}

#[test]
fn cut_stack_printing_through_tools() {
    let dir = workdir("cut-stack");
    std::fs::write(dir.join("numbered.pdf"), fixture(10)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "numbered.pdf"}))["doc"].as_u64().unwrap();
    let before = page_text(&mut a, doc);
    for reverse in [false, true] {
        let r = ok(
            &mut a,
            "doc_print",
            json!({
                "doc": doc, "layout": "multiple", "order": "cut-stack", "per_sheet": 4,
                "orientation": "portrait", "auto_rotate": false, "reverse": reverse, "path": "cut.pdf"
            }),
        );
        assert_eq!(r["sheets"], 3);
        let printed = ok(&mut a, "doc_open", json!({"path": "cut.pdf"}))["doc"].as_u64().unwrap();
        let expected =
            if reverse { vec![vec![10, 7, 4, 1], vec![9, 6, 3], vec![8, 5, 2]] } else { vec![vec![1, 4, 7, 10], vec![2, 5, 8], vec![3, 6, 9]] };
        for (text, expected) in page_text(&mut a, printed).iter().zip(&expected) {
            let actual: Vec<usize> = text.split_whitespace().filter_map(|t| t.parse().ok()).collect();
            assert_eq!(&actual, expected, "saved PDF must contain the imposed order: {text}");
        }
        ok(&mut a, "doc_close", json!({"doc": printed}));
    }
    let r = ok(
        &mut a,
        "doc_print",
        json!({
            // Even pages by page number (#3984): 2, 4, 6, 8 and 10 (counting places in the range,
            // "even" would have been 3, 5, 7 and 9).
            "doc": doc, "pages": "2-10", "subset": "even", "reverse": true,
            "layout": "multiple", "order": "cut-stack", "per_sheet": 2, "auto_rotate": false,
            "orientation": "landscape", "path": "range.pdf"
        }),
    );
    assert_eq!(r["pages"], 5);
    let printed = ok(&mut a, "doc_open", json!({"path": "range.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, printed), ["Page 10\nPage 4", "Page 8\nPage 2", "Page 6"]);
    assert_eq!(page_text(&mut a, doc), before);
    assert_eq!(ok(&mut a, "doc_info", json!({"doc": doc}))["document"]["dirty"], false);
    for duplex in ["long-edge", "short-edge"] {
        assert!(matches!(
            a.call(
                "doc_print",
                &json!({
                    "doc": doc, "layout": "multiple", "order": "cut-stack", "duplex": duplex, "path": "refused.pdf"
                })
            ),
            Err(ToolError::InvalidArgs(_))
        ));
    }
    assert!(matches!(a.call("doc_print", &json!({"doc": doc, "order": "cut-stack", "path": "refused.pdf"})), Err(ToolError::InvalidArgs(_))));
    assert!(!dir.join("refused.pdf").exists());
    // The printer's own two-sided default isn't refused: cut and stack sends single-sided (#2422).
    ok(&mut a, "doc_print", json!({"doc": doc, "layout": "multiple", "order": "cut-stack", "duplex": "printer", "path": "printer-default.pdf"}));
    assert!(dir.join("printer-default.pdf").exists());
    assert!(a.call("doc_print", &json!({"doc": doc, "layout": "multiple", "order": "cut-stack", "path": "../escaped.pdf"})).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn measurements_calibrate_draw_save_reopen_and_export() {
    let dir = workdir("measurements");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    let scale = ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"points":[[10,10],[70,10]],"distance":6,"unit":"m","precision":3}));
    assert!((scale["scale"]["x"].as_f64().unwrap() - 0.1).abs() < 1e-12);
    for (tool, points) in [
        ("measure_distance", json!([[10, 20], [70, 100]])),
        ("measure_perimeter", json!([[10, 20], [70, 20], [70, 100]])),
        ("measure_area", json!([[10, 20], [70, 20], [70, 100], [10, 100]])),
    ] {
        ok(&mut a, tool, json!({"doc":doc,"page":1,"points":points,"label":"Room, \"A\"","author":"Tester"}));
    }
    let all = ok(&mut a, "measure_list", json!({"doc":doc}));
    assert_eq!(all["count"], 3);
    assert_eq!(all["unsupported"], json!([]));
    assert_eq!(all["truncated"], false);
    for (m, value) in all["measurements"].as_array().unwrap().iter().zip([10.0, 14.0, 48.0]) {
        assert!((m["reading"]["value"].as_f64().unwrap() - value).abs() < 1e-6);
        assert_eq!(m["page"], 1);
        assert_eq!(m["label"], "Room, \"A\"");
    }
    let preview = ok(&mut a, "measure_info", json!({"doc":doc,"page":1,"type":"area","points":[[10,20],[70,20],[70,100],[10,100]]}));
    assert!((preview["reading"]["value"].as_f64().unwrap() - 48.0).abs() < 1e-6);
    let rendered = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
    assert!(matches!(rendered.first(), Some(Content::Png { .. })));
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_eq!(ok(&mut a, "measure_list", json!({"doc":doc}))["count"], 2);
    ok(&mut a, "edit_redo", json!({"doc":doc}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"measured.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path":"measured.pdf"}))["doc"].as_u64().unwrap();
    let after = ok(&mut a, "measure_list", json!({"doc":reopened}));
    assert_eq!(after["measurements"], all["measurements"]);
    let exported = ok(&mut a, "measure_export", json!({"doc":reopened,"out":"measurements.csv"}));
    assert_eq!((exported["count"].as_u64(), exported["unsupported"].as_u64()), (Some(3), Some(0)));
    let csv = std::fs::read_to_string(dir.join("measurements.csv")).unwrap();
    assert!(csv.contains("area,48,\"m^2\",\"Room, \"\"A\"\"\""), "{csv}");
    assert!(a.call("measure_export", &json!({"doc":doc,"out":"../outside.csv"})).is_err());
    // A new viewport changes future readings, without recalibrating saved measurements.
    ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"units_per_point":1,"rect":[0,0,50,50],"unit":"cm"}));
    assert_eq!(ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"at":[20,20]}))["scale"]["unit"], "cm");
    assert_eq!(ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"at":[80,80]}))["scale"]["unit"], "m");
    assert_eq!(ok(&mut a, "measure_list", json!({"doc":doc}))["measurements"], all["measurements"]);
    // Rotate the page, then measure in its displayed coordinates.
    ok(&mut a, "page_rotate", json!({"doc":doc,"pages":[1],"degrees":90}));
    ok(&mut a, "measure_distance", json!({"doc":doc,"page":1,"points":[[100,100],[180,160]]}));
    let all = ok(&mut a, "measure_list", json!({"doc":doc}));
    let last = all["measurements"].as_array().unwrap().last().unwrap();
    assert!((last["reading"]["value"].as_f64().unwrap() - 10.0).abs() < 1e-6);
    assert_eq!(last["points"], json!([[100.0, 100.0], [180.0, 160.0]]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn measurement_scale_ratio_takes_the_page_side_in_a_unit() {
    // #2752: a plan drawn at 1:50 is "1 mm = 50 mm", so a line 10 mm long on the page is 500 mm.
    let dir = workdir("measurement-ratio");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    let mm = 72.0 / 25.4;
    let line = json!([[20.0, 30.0], [20.0 + 10.0 * mm, 30.0]]);
    let read = |a: &mut Automation, doc: u64| ok(a, "measure_info", json!({"doc":doc,"page":1,"points":line}))["reading"]["value"].as_f64().unwrap();
    let set = ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"page_distance":1,"page_unit":"mm","distance":50,"unit":"mm"}));
    assert_eq!(set["scale"]["ratio"], "1 mm = 50 mm");
    assert!((read(&mut a, doc) - 500.0).abs() < 1e-9);
    // The PDF keeps its factors per point: 50 mm for every 25.4 / 72 mm of page.
    assert!((set["scale"]["x"].as_f64().unwrap() - 50.0 / mm).abs() < 1e-9);
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert!((read(&mut a, doc) - 10.0 / 25.4).abs() < 1e-9, "true size in inches without a scale");
    ok(&mut a, "edit_redo", json!({"doc":doc}));
    assert!((read(&mut a, doc) - 500.0).abs() < 1e-9);
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"ratio.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path":"ratio.pdf"}))["doc"].as_u64().unwrap();
    assert!((read(&mut a, reopened) - 500.0).abs() < 1e-9);
    assert_eq!(ok(&mut a, "measure_scale", json!({"doc":reopened,"page":1,"at":[20,30]}))["scale"]["ratio"], "1 mm = 50 mm");
    // The page side defaults to the real-world unit; any page unit converts to points.
    ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"page_distance":2,"distance":1,"unit":"cm"}));
    assert!((read(&mut a, doc) - 0.5).abs() < 1e-9);
    ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"page_distance":0.25,"page_unit":"in","distance":1,"unit":"ft"}));
    assert!((read(&mut a, doc) - 10.0 / 25.4 * 4.0).abs() < 1e-9);
    // Units per point keep meaning points.
    ok(&mut a, "measure_scale", json!({"doc":doc,"page":1,"units_per_point":50,"unit":"mm"}));
    assert!((read(&mut a, doc) - 500.0 * mm).abs() < 1e-9);
    let before = ok(&mut a, "doc_list", json!({}));
    for args in [
        json!({"doc":doc,"page":1,"page_distance":1,"page_unit":"ft","distance":1,"unit":"ft"}),
        json!({"doc":doc,"page":1,"page_distance":1,"distance":1,"unit":"m"}),
        json!({"doc":doc,"page":1,"page_distance":1,"unit":"mm"}),
        json!({"doc":doc,"page":1,"page_unit":"mm","units_per_point":1}),
        json!({"doc":doc,"page":1,"page_distance":1,"units_per_point":1,"distance":1}),
        json!({"doc":doc,"page":1,"page_distance":1,"points":[[0,0],[10,0]],"distance":1}),
    ] {
        assert!(a.call("measure_scale", &args).is_err(), "{args}");
        assert_eq!(ok(&mut a, "doc_list", json!({})), before);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn measurements_bad_arguments_leave_document_and_history_unchanged() {
    let dir = workdir("measurement-errors");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "doc_list", json!({}));
    for (tool, args) in [
        ("measure_distance", json!({"doc":doc,"page":1,"points":[[0,0]]})),
        ("measure_distance", json!({"doc":doc,"page":1,"points":[[0,0],[0,0]]})),
        ("measure_area", json!({"doc":doc,"page":1,"points":[[0,0],[20,20],[0,20],[20,0]]})),
        ("measure_scale", json!({"doc":doc,"page":1,"points":[[0,0],[0,0]],"distance":10})),
        ("measure_snap", json!({"doc":doc,"page":1,"at":[1e100,0]})),
        ("measure_scale", json!({"doc":doc,"page":1,"units_per_point":1,"precision":8})),
    ] {
        assert!(a.call(tool, &args).is_err(), "{tool} {args}");
        assert_eq!(ok(&mut a, "doc_list", json!({})), before);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn measurement_snap_tool_covers_all_targets() {
    let dir = workdir("measurement-snap");
    let mut a = auto(&dir);
    let source = String::from_utf8(fixture(1)).unwrap();
    let old = "BT /F1 24 Tf 20 150 Td (Page 1) Tj ET";
    let drawing = "10 20 m 110 20 l S 60 0 m 60 80 l S";
    assert!(drawing.len() <= old.len());
    let source = source.replace(old, &format!("{drawing:<width$}", width = old.len()));
    std::fs::write(dir.join("drawing.pdf"), source).unwrap();
    let doc = ok(&mut a, "doc_open", json!({"path":"drawing.pdf"}))["doc"].as_u64().unwrap();
    for (at, kind, point, midpoints) in [
        ([11, 280], "endpoint", [10, 280], true),
        ([60, 259], "midpoint", [60, 260], true),
        ([59, 279], "intersection", [60, 280], false),
        ([32, 278], "path", [32, 280], true),
    ] {
        let snap = ok(&mut a, "measure_snap", json!({"doc":doc,"page":1,"at":at,"tolerance":3,"midpoints":midpoints}));
        assert_eq!(snap["snap"]["kind"], kind);
        assert_eq!(snap["snap"]["point"], json!(point.map(f64::from)));
        assert_eq!(snap["truncated"], false);
    }
    assert!(ok(&mut a, "measure_snap", json!({"doc":doc,"page":1,"at":[180,180]}))["snap"].is_null());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn growable_xfa_text_paginates_after_fill_save_and_print() {
    let dir = workdir("xfa-growable-text");
    let text: String = (0..150).map(|i| format!("Row{i:03} {}sample\n", "sample ".repeat(8))).collect();
    assert_eq!(text.len(), 10_500);
    for scripted in [false, true] {
        let template = pdfcraft_xfa::fixtures::growable_text_template(scripted);
        std::fs::write(dir.join("growing.pdf"), pdfcraft_xfa::fixtures::shell(&template)).unwrap();
        let mut a = auto(&dir);
        let opened = ok(&mut a, "doc_open", json!({ "path": "growing.pdf" }));
        let doc = opened["doc"].as_u64().unwrap();
        assert_eq!(opened["pages"], 2);
        let filled = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "ACTIVIDADES": text } }));
        assert_eq!(filled["pages"], 3, "scripted={scripted}: {filled}");
        ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "data.xml", "what": "fields" }));
        let data = std::fs::read_to_string(dir.join("data.xml")).unwrap();
        assert!(data.contains(text.trim_end()), "the full value remains in the datasets");
        ok(&mut a, "doc_save", json!({ "doc": doc, "path": "saved.pdf" }));
        ok(&mut a, "doc_close", json!({ "doc": doc }));
        let reopened = ok(&mut a, "doc_open", json!({ "path": "saved.pdf" }));
        assert_eq!(reopened["pages"], 3);
        let doc = reopened["doc"].as_u64().unwrap();
        ok(&mut a, "doc_print", json!({ "doc": doc, "path": "printed.pdf" }));
        let printed = ok(&mut a, "doc_open", json!({ "path": "printed.pdf" }));
        assert_eq!(printed["pages"], 3);
        let output = ok(&mut a, "text_extract", json!({ "doc": printed["doc"] }));
        let printed_text = output["pages"].as_array().unwrap().iter().map(|p| p["text"].as_str().unwrap()).collect::<Vec<_>>().join("\n");
        let expected = format!("{text}\nFollowing section");
        assert_eq!(printed_text.split_whitespace().collect::<Vec<_>>(), expected.split_whitespace().collect::<Vec<_>>(), "scripted={scripted}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn repeated_xfa_print_buttons_work_through_tools_and_after_reopen() {
    let dir = workdir("xfa-print-buttons");
    let template = pdfcraft_xfa::fixtures::repeated_print_button_template();
    std::fs::write(dir.join("print.pdf"), pdfcraft_xfa::fixtures::shell(&template)).unwrap();
    // Raised time limits: a busy machine does not stop the click scripts this checks.
    let mut a = auto(&dir).with_xfa_time_scale(100);
    let doc = ok(&mut a, "doc_open", json!({ "path": "print.pdf" }))["doc"].as_u64().unwrap();
    let check = |a: &mut Automation, doc| {
        let fields = ok(a, "form_fields", json!({ "doc": doc }));
        let buttons: Vec<_> = fields["fields"].as_array().unwrap().iter().filter(|f| f["type"] == "button").collect();
        assert_eq!(buttons.len(), 3);
        for (index, field) in buttons.iter().enumerate() {
            assert_eq!(field["page"], index + 1);
            let out = ok(a, "js_run", json!({ "doc": doc, "field": field["name"], "script": "throw new Error('AcroForm fallback ran')" }));
            assert!(out["error"].is_null(), "{out}");
            assert_eq!(out["requests"], json!([{ "print": true }]), "{out}");
            assert_eq!(out["alerts"], json!([]));
        }
    };
    check(&mut a, doc);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "saved.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "saved.pdf" }))["doc"].as_u64().unwrap();
    check(&mut a, doc);
}

/// A dynamic XFA form's scripts run on open, on fills and for buttons through the tools, with undo,
/// and `js_run` reports their messages.
#[test]
fn xfa_scripts_run_for_buttons_and_field_changes_through_tools() {
    let dir = workdir("xfa-scripts");
    std::fs::write(dir.join("scripted.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template())).unwrap();
    // Raised time limits: a busy machine does not stop the scripts this checks.
    let mut a = auto(&dir).with_xfa_time_scale(100);
    let doc = ok(&mut a, "doc_open", json!({ "path": "scripted.pdf" }))["doc"].as_u64().unwrap();
    let field = |a: &mut Automation, n: &str| {
        let f = ok(a, "form_fields", json!({ "doc": doc }));
        f["fields"].as_array().unwrap().iter().find(|f| f["name"] == n).cloned()
    };
    // Opening ran the initialize and calculate scripts.
    assert_eq!(field(&mut a, "qty").unwrap()["value"], "2");
    assert_eq!(field(&mut a, "total").unwrap()["value"], "10");
    // Filling recalculates; a bad value shows its message (the tool reports alerts in js output? no: it is applied, the value stays).
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "qty": "4" } }));
    assert_eq!(field(&mut a, "total").unwrap()["value"], "20");
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["xfa_scripts_off"], false);
    // A button's XFA click script runs through js_run, like any button.
    let before = ok(&mut a, "doc_info", json!({ "doc": doc }))["xfa_layout"]["fields"].as_u64().unwrap();
    let r = ok(&mut a, "js_run", json!({ "doc": doc, "script": "", "field": "addRow" }));
    assert!(r["error"].is_null(), "{r}");
    assert!(field(&mut a, "amount_2").is_some());
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["xfa_layout"]["fields"].as_u64().unwrap(), before + 2);
    // Undo takes the row away again.
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert!(field(&mut a, "amount_2").is_none());
    let r = ok(&mut a, "js_run", json!({ "doc": doc, "script": "", "field": "hello" }));
    assert_eq!(r["alerts"], json!([{ "message": "Hello 4", "title": null, "icon": "error", "buttons": "ok" }]));
}

#[test]
fn active_xfa_skips_container_fallback_scripts_after_save_and_reopen() {
    let dir = workdir("xfa-container-scripts");
    std::fs::write(dir.join("form.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template())).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "form.pdf"}))["doc"].as_u64().unwrap();
    let fallback = "app.alert('This PDF form requires a newer version of Adobe Acrobat');";
    ok(&mut a, "js_set_document_script", json!({"doc":doc,"name":"viewerFallback","script":fallback}));
    let out = ok(&mut a, "js_run", json!({"doc":doc,"script":"app.alert('Console message');"}));
    assert!(out["error"].is_null(), "{out}");
    assert_eq!(out["alerts"], json!([{ "message": "Console message", "title": null, "icon": "error", "buttons": "ok" }]));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"saved.pdf"}));
    ok(&mut a, "doc_close", json!({"doc":doc}));
    let doc = ok(&mut a, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "js_document_scripts", json!({"doc":doc}))["scripts"], json!([{"name":"viewerFallback","script":fallback}]));
    let out = ok(&mut a, "js_run", json!({"doc":doc,"script":""}));
    assert!(out["error"].is_null(), "{out}");
    assert_eq!(out["alerts"], json!([]));
    let out = ok(&mut a, "js_run", json!({"doc":doc,"script":"","field":"hello"}));
    assert!(out["error"].is_null(), "{out}");
    assert_eq!(out["alerts"], json!([{ "message": "Hello 2", "title": null, "icon": "error", "buttons": "ok" }]));
    ok(&mut a, "xfa_to_static", json!({"doc":doc}));
    let out = ok(&mut a, "js_run", json!({"doc":doc,"script":""}));
    assert_eq!(out["alerts"].as_array().unwrap().len(), 1);
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_eq!(ok(&mut a, "js_run", json!({"doc":doc,"script":""}))["alerts"], json!([]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn doc_info_says_when_a_runaway_script_turned_the_forms_scripts_off() {
    let dir = workdir("xfa-scripts-off");
    // An initialize script whose loops are in nested calls runs until it is abandoned.
    let tpl = pdfcraft_xfa::fixtures::scripted_template().replace(
        "if (qty.rawValue === null) qty.rawValue = 2;",
        "function f() { for (var j = 0; j &lt; 99999; j++) {} } for (var k = 0; k &lt; 5000; k++) f();",
    );
    std::fs::write(dir.join("runaway.pdf"), pdfcraft_xfa::fixtures::shell(&tpl)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "runaway.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["xfa_scripts_off"], true);
    let _ = std::fs::remove_dir_all(dir);
}

/// doc_info describes a link's set-layer-visibility action by layer name, and each layer's
/// visibility now and as the file opens.
#[test]
fn doc_info_describes_set_layer_links() {
    let dir = workdir("layer-links");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [4 0 R 5 0 R] /D << /OFF [5 0 R] >> >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [6 0 R] >>",
        "<< /Type /OCG /Name (Red) >>",
        "<< /Type /OCG /Name (Green) >>",
        "<< /Type /Annot /Subtype /Link /Rect [10 10 90 30] /A << /S /SetOCGState /State [/Toggle 5 0 R /OFF 4 0 R 9 0 R] /PreserveRB false >> >>",
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    std::fs::write(dir.join("layers.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "layers.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    // A group that isn't a layer has no name.
    assert_eq!(
        info["links"][0]["target"],
        json!({
            "layers": [{ "layer": "Green", "state": "toggle" }, { "layer": "Red", "state": "off" }, { "layer": null, "state": "off" }],
            "preserve_rb": false,
        })
    );
    assert_eq!(info["layers"], json!([{ "name": "Red", "visible": true, "initial": true }, { "name": "Green", "visible": false, "initial": false }]));
}

#[cfg(feature = "mcp")]
#[test]
fn conventions_core_tools_preserve_root_and_batch_errors() {
    let dir = workdir("conventions-core");
    let mut s = McpServer::new(auto(&dir));
    let run = |s: &mut McpServer, name: &str, args: Value| rpc(s, 1, "tools/call", json!({"name":name,"arguments":args}));
    assert_eq!(run(&mut s, "doc_inspect", json!({}))["result"]["structuredContent"]["documents"], json!([]));
    let opened = run(&mut s, "command_run", json!({"id":"file.open","params":{"path":"a.pdf"}}));
    assert_eq!(opened["result"]["isError"], false, "{opened}");
    let doc = opened["result"]["structuredContent"]["doc"].as_u64().unwrap();
    let filtered = run(&mut s, "command_list", json!({"doc":doc,"filter":"rotate","enabled_only":true}));
    let commands = filtered["result"]["structuredContent"]["commands"].as_array().unwrap();
    assert!(!commands.is_empty());
    assert!(commands.iter().all(|c| c["enabled"] == true));
    assert!(commands.iter().find(|c| c["id"] == "page.rotate").unwrap()["params"].is_object());
    for stop in [true, false] {
        let r = run(
            &mut s,
            "command_batch",
            json!({"steps":[{"id":"page.rotate","params":{"doc":doc,"degrees":90}},{"id":"no.such.command"},{"id":"page.rotate","params":{"doc":doc,"degrees":90}}],"stop_on_error":stop}),
        );
        assert_eq!(r["result"]["isError"], true, "{r}");
        assert_eq!(r["result"]["structuredContent"]["completed"], if stop { 1 } else { 2 });
        assert_eq!(r["result"]["structuredContent"]["failed"], 1);
    }
    let r = run(&mut s, "command_run", json!({"id":"file.save","params":{"doc":doc,"path":"../escaped.pdf"}}));
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("outside the allowed directory"));
    let r = run(&mut s, "command_run", json!({"id":"page.rotate","params":{"doc":doc,"degrees":90,"typo":1}}));
    assert_eq!(r["result"]["isError"], false);
    assert!(r["result"]["structuredContent"]["warnings"][0].as_str().unwrap().contains("typo"));
    let before = run(&mut s, "doc_info", json!({"doc":doc}));
    let r = run(&mut s, "render_preview", json!({"doc":doc,"page":1,"max_side":64}));
    assert_eq!(r["result"]["isError"], false, "{r}");
    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD.decode(r["result"]["content"][0]["data"].as_str().unwrap()).unwrap();
    let image = image::load_from_memory(&png).unwrap();
    assert!(image.width().max(image.height()) <= 64);
    assert_eq!(before["result"], run(&mut s, "doc_info", json!({"doc":doc}))["result"]);
}
