//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 4 of 8).

#[allow(unused_imports)]
use super::*;

#[test]
fn create_multiple_has_no_file_count_limit() {
    let dir = workdir("create-many");
    let mut a = auto(&dir);
    // The same 3-page PDF listed 1,001 times: every listed file is a source, however many there are.
    let paths = vec!["a.pdf"; 1001];
    let made = ok(&mut a, "doc_create_multiple", json!({ "paths": paths, "out": "many.pdf", "open": true }));
    assert_eq!(made["document"]["pages"], 3 * 1001, "{made}");
    assert!(dir.join("many.pdf").is_file());
}

#[test]
fn creating_images_with_dpi_through_tools() {
    let dir = workdir("image-dpi");
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 300, 150);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_pixel_dims(Some(png::PixelDimensions { xppu: 11811, yppu: 5906, unit: png::Unit::Meter }));
        enc.write_header().unwrap().write_image_data(&vec![100; 300 * 150 * 3]).unwrap();
    }
    std::fs::write(dir.join("scan.png"), png).unwrap();
    let mut a = auto(&dir);
    for (dpi, width, height) in [(None, 72.0, 72.0), (Some(72.0), 300.0, 150.0), (Some(300.0), 72.0, 36.0)] {
        let mut args = json!({ "from": "images", "paths": ["scan.png"] });
        if let Some(dpi) = dpi {
            args["dpi"] = json!(dpi);
        }
        let doc = ok(&mut a, "doc_create", args)["doc"].as_u64().unwrap();
        let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
        assert!((info["pages"][0]["width"].as_f64().unwrap() - width).abs() < 0.02);
        assert!((info["pages"][0]["height"].as_f64().unwrap() - height).abs() < 0.02);
        ok(&mut a, "doc_save", json!({ "doc": doc, "path": "made.pdf" }));
        let reopened = ok(&mut a, "doc_open", json!({ "path": "made.pdf" }))["doc"].as_u64().unwrap();
        let render = a.call("page_render", &json!({ "doc": reopened, "page": 1, "dpi": 72 })).unwrap();
        let Content::Png { width: w, height: h, .. } = &render[0] else { panic!("expected PNG") };
        assert!((*w as f64 - width).abs() <= 1.0 && (*h as f64 - height).abs() <= 1.0);
    }
    for dpi in [0.0, -72.0, 1201.0] {
        assert!(a.call("doc_create", &json!({ "from": "images", "paths": ["scan.png"], "dpi": dpi })).is_err());
    }
}

#[test]
fn saving_with_flatten_fill_sign_bakes_marks_and_leaves_other_comments() {
    let dir = workdir("fill-sign-flatten");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [40.0, 80.0], "text": "Hello" }));
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "highlight", "find": "Page", "contents": "keep" }));
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20.0, 120.0, 180.0, 142.0] }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "kept.pdf" }));
    let kept = ok(&mut a, "doc_open", json!({ "path": "kept.pdf" }))["doc"].as_u64().unwrap();
    let kept_comments = ok(&mut a, "comment_list", json!({ "doc": kept }))["comments"].as_array().unwrap().clone();
    assert!(kept_comments.iter().any(|c| c["contents"] == "Hello"), "without the flag the typewriter stays: {kept_comments:?}");

    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "flat.pdf", "flatten_fill_sign": true }));
    let flat = ok(&mut a, "doc_open", json!({ "path": "flat.pdf" }))["doc"].as_u64().unwrap();
    let comments = ok(&mut a, "comment_list", json!({ "doc": flat }))["comments"].as_array().unwrap().clone();
    assert!(comments.iter().all(|c| c["contents"] != "Hello"), "the typewriter was baked in: {comments:?}");
    assert!(comments.iter().any(|c| c["contents"] == "keep"), "the highlight stays: {comments:?}");
    let fields = ok(&mut a, "form_fields", json!({ "doc": flat }))["fields"].as_array().unwrap().clone();
    assert_eq!(fields.len(), 1, "the form field stays");
}

#[test]
fn flattening_through_tools() {
    let dir = workdir("flatten");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [10, 200, 190, 240], "contents": "Approved" }));
    ok(&mut a, "doc_flatten", json!({ "doc": doc }));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 0);
    assert!(page_text(&mut a, doc)[0].contains("Approved"), "the text box is now page text");
    assert!(matches!(a.call("doc_flatten", &json!({ "doc": doc, "comments": false, "fields": false })), Err(ToolError::InvalidArgs(_))));
}

/// Inserting a file's pages brings its bookmarks too, under one named after the file, at the
/// pages' place among the document's own bookmarks.
#[test]
fn inserting_a_file_brings_its_bookmarks() {
    let dir = workdir("insert-bookmarks");
    let mut a = auto(&dir);
    let annex = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "bookmark_add", json!({ "doc": annex, "title": "Annex intro", "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": annex, "title": "Annex table", "page": 2 }));
    ok(&mut a, "doc_save", json!({ "doc": annex, "path": "annex.pdf" }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Guide start", "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Guide end", "page": 3 }));
    ok(&mut a, "page_insert_file", json!({ "doc": doc, "path": "annex.pdf", "at": 2 }));
    let list = ok(&mut a, "bookmark_list", json!({ "doc": doc }));
    let top: Vec<(&str, Value)> = list["bookmarks"].as_array().unwrap().iter().map(|b| (b["title"].as_str().unwrap(), b["page"].clone())).collect();
    assert_eq!(top, [("Guide start", json!(1)), ("annex", json!(2)), ("Guide end", json!(5))], "{list}");
    let kids: Vec<(&str, Value)> =
        list["bookmarks"][1]["children"].as_array().unwrap().iter().map(|b| (b["title"].as_str().unwrap(), b["page"].clone())).collect();
    assert_eq!(kids, [("Annex intro", json!(2)), ("Annex table", json!(3))], "{list}");
}

#[test]
fn replacing_pages_through_tools() {
    let dir = workdir("replace");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_replace", json!({ "doc": doc, "pages": [2, 3], "path": "b.pdf", "from_pages": [2, 1] }));
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 1"]);
    assert!(
        matches!(a.call("page_replace", &json!({ "doc": doc, "pages": [1, 2, 3], "path": "b.pdf" })), Err(ToolError::Failed(_))),
        "b.pdf has only 2 pages"
    );
}

#[test]
fn preparing_a_form_through_tools() {
    let dir = workdir("prepare");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let add = |a: &mut Automation, args: Value| ok(a, "form_add_field", args)["field"].as_str().unwrap().to_owned();
    assert_eq!(add(&mut a, json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, 20, 180, 42] })), "Text1");
    assert_eq!(add(&mut a, json!({ "doc": doc, "page": 1, "type": "combo", "rect": [20, 60, 180, 82], "options": ["Red", "Green"] })), "Dropdown1");
    assert_eq!(add(&mut a, json!({ "doc": doc, "page": 1, "type": "radio", "rect": [20, 100, 34, 114], "group": "size", "export": "S" })), "size");
    assert_eq!(add(&mut a, json!({ "doc": doc, "page": 1, "type": "radio", "rect": [40, 100, 54, 114], "group": "size", "export": "L" })), "size");
    let r = ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "Text1", "name": "full name", "required": true, "tooltip": "Your name" }));
    assert_eq!(r["field"], "full name");
    ok(&mut a, "form_delete_field", json!({ "doc": doc, "field": "Dropdown1" }));
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "full name", "rect": [30, 20, 190, 42] }));
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let f = fields["fields"].as_array().unwrap();
    assert_eq!(f.iter().map(|f| f["name"].as_str().unwrap()).collect::<Vec<_>>(), ["full name", "size"]);
    assert_eq!((f[0]["required"].as_bool(), f[0]["tooltip"].as_str()), (Some(true), Some("Your name")));
    assert_eq!(f[0]["rect"], json!([30.0, 20.0, 190.0, 42.0]), "moved; the rect round-trips in view coordinates");
    assert_eq!(f[1]["options"], json!(["S", "L"]));
    // #94: the mark a check box or radio button shows.
    assert_eq!(f[1]["check_style"], "circle", "radio buttons default to a circle");
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "size", "check_style": "star" }));
    assert_eq!(ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"][1]["check_style"], "star");
    assert!(matches!(a.call("form_set_props", &json!({ "doc": doc, "field": "size", "check_style": "heart" })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("form_set_props", &json!({ "doc": doc, "field": "full name", "check_style": "star" })), Err(ToolError::Failed(_))));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "full name": "Ada", "size": "L" } }));
    assert!(page_text(&mut a, doc)[0].contains("Ada"));
    // Options tab: alignment, default, flags (comb needs a limit).
    assert!(matches!(a.call("form_set_props", &json!({ "doc": doc, "field": "full name", "flags": { "comb": true } })), Err(ToolError::Failed(_))));
    ok(
        &mut a,
        "form_set_props",
        json!({ "doc": doc, "field": "full name", "align": "center", "default": "Anon", "max_length": 8, "flags": { "comb": true, "spell_check": false } }),
    );
    assert!(matches!(
        a.call("form_set_props", &json!({ "doc": doc, "field": "full name", "flags": { "sparkles": true } })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(
        a.call("form_add_field", &json!({ "doc": doc, "page": 1, "type": "slider", "rect": [0, 0, 9, 9] })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(a.call("form_delete_field", &json!({ "doc": doc, "field": "nope" })), Err(ToolError::Failed(_))));
    // An image field shows the picture it is given.
    assert_eq!(add(&mut a, json!({ "doc": doc, "page": 2, "type": "image", "rect": [20, 20, 120, 120] })), "Image1");
    let png = |a: &mut Automation| match a.call("page_render", &json!({ "doc": doc, "page": 2, "dpi": 36 })).unwrap().remove(0) {
        Content::Png { data, .. } => data,
        other => panic!("{other:?}"),
    };
    let before = png(&mut a);
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "pics", "dpi": 18, "pages": [1] }));
    ok(&mut a, "form_set_image", json!({ "doc": doc, "field": "Image1", "path": "pics/a_page_1.png" }));
    let after = png(&mut a);
    assert_ne!(before, after, "the picture is drawn");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Set the image of Image1");
    assert!(matches!(a.call("form_set_image", &json!({ "doc": doc, "field": "size", "path": "pics/a_page_1.png" })), Err(ToolError::Failed(_))));
}

#[test]
fn renaming_a_radio_button_to_a_groups_name_joins_the_group_through_tools() {
    // #4949: two buttons placed separately are two groups; naming one after the other makes
    // them one group, and a clashing export value is renamed instead of shared.
    let dir = workdir("radio-join");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "radio", "rect": [20, 100, 34, 114], "group": "size", "export": "S" }));
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "radio", "rect": [40, 100, 54, 114], "group": "other", "export": "S" }));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "size": "S" } }));
    let r = ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "other", "name": "size" }));
    assert_eq!(r["field"], "size");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "joined.pdf" }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "joined.pdf" }))["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let f = fields["fields"].as_array().unwrap();
    assert_eq!(f.iter().map(|f| f["name"].as_str().unwrap()).collect::<Vec<_>>(), ["size"], "one group");
    assert_eq!(f[0]["options"], json!(["S", "Choice1"]), "the joining button takes a free export value");
    assert_eq!(f[0]["value"], "S", "the group keeps its selection");
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "size": "Choice1" } }));
    assert_eq!(ok(&mut a, "form_fields", json!({ "doc": doc }))["fields"][0]["value"], "Choice1");
}

#[test]
fn rotating_a_field_through_tools() {
    let dir = workdir("rotate-field");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, 20, 180, 42], "name": "City" }));
    let rect_of = |a: &mut Automation| {
        let f = &ok(a, "form_fields", json!({ "doc": doc }))["fields"][0];
        (f["rotation"].as_i64().unwrap(), f["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>())
    };
    let (rot, before) = rect_of(&mut a);
    assert_eq!(rot, 0);
    assert!(matches!(a.call("form_set_props", &json!({ "doc": doc, "field": "City", "rotation": 45 })), Err(ToolError::InvalidArgs(_))));
    assert_eq!(rect_of(&mut a).1, before, "a rejected rotation changes nothing");
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "City", "rotation": 90 }));
    let (rot, turned) = rect_of(&mut a);
    assert_eq!(rot, 90);
    let (bw, bh) = (before[2] - before[0], before[3] - before[1]);
    let (tw, th) = (turned[2] - turned[0], turned[3] - turned[1]);
    assert!((tw - bh).abs() < 0.2 && (th - bw).abs() < 0.2, "swapped {before:?} -> {turned:?}");
    let center = |r: &[f64]| ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (bc, tc) = (center(&before), center(&turned));
    assert!((bc.0 - tc.0).abs() < 0.2 && (bc.1 - tc.1).abs() < 0.2, "center moved {bc:?} -> {tc:?}");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Change field properties");
    let (rot, back) = rect_of(&mut a);
    assert_eq!((rot, back), (0, before));
}

/// A table of text fields through the tool: laid out from the top of the page as displayed, named
/// column by column, one undo step, and refused whole, with the reason, when any of it can't go in.
#[test]
fn adding_a_field_table_through_tools() {
    let dir = workdir("field-table");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let table =
        json!({ "doc": doc, "page": 1, "rect": [20, 20, 220, 120], "rows": 2, "names": ["desc_N", "price_N"], "widths": [3, 1], "row_gap": 10 });
    assert_eq!(ok(&mut a, "form_add_field_table", table.clone())["fields"], json!(["desc_1", "price_1", "desc_2", "price_2"]));
    let rects = |a: &mut Automation| {
        let fields = ok(a, "form_fields", json!({ "doc": doc }))["fields"].as_array().unwrap().clone();
        fields.iter().map(|f| (f["name"].as_str().unwrap().to_owned(), f["rect"].clone())).collect::<Vec<_>>()
    };
    // 150 and 50 points wide, (100 - 10) / 2 = 45 high, the first row at the top.
    let near = |v: &Value, want: [f64; 4]| v.as_array().unwrap().iter().zip(want).all(|(g, w)| (g.as_f64().unwrap() - w).abs() < 0.01);
    let got = rects(&mut a);
    for (name, want) in [
        ("desc_1", [20.0, 20.0, 170.0, 65.0]),
        ("price_1", [170.0, 20.0, 220.0, 65.0]),
        ("desc_2", [20.0, 75.0, 170.0, 120.0]),
        ("price_2", [170.0, 75.0, 220.0, 120.0]),
    ] {
        let rect = got.iter().find(|(n, _)| n == name).map(|(_, r)| r).unwrap();
        assert!(near(rect, want), "{name}: {rect} != {want:?}");
    }
    // Without names, each table takes the first free base.
    let plain = json!({ "doc": doc, "page": 2, "rect": [20, 20, 220, 120], "rows": 2 });
    assert_eq!(ok(&mut a, "form_add_field_table", plain.clone())["fields"], json!(["Table1_1", "Table1_2"]));
    assert_eq!(ok(&mut a, "form_add_field_table", plain)["fields"], json!(["Table2_1", "Table2_2"]));
    let count = |a: &mut Automation| ok(a, "form_fields", json!({ "doc": doc }))["fields"].as_array().unwrap().len();
    assert_eq!(count(&mut a), 8);
    // Refused whole, saying why: nothing is added.
    let refused = |a: &mut Automation, args: Value| match a.call("form_add_field_table", &args) {
        Err(ToolError::InvalidArgs(m)) => m,
        other => panic!("refused, not {other:?}"),
    };
    assert_eq!(refused(&mut a, table), "a field named \"desc_1\" already exists");
    let small = json!({ "doc": doc, "page": 1, "rect": [20, 200, 220, 300], "rows": 30 });
    assert!(refused(&mut a, small).starts_with("the rows are too short"));
    let dotted = json!({ "doc": doc, "page": 1, "rect": [20, 200, 220, 300], "rows": 2, "names": ["a.{n}"] });
    assert_eq!(refused(&mut a, dotted), "field names can't contain a period: \"a.1\"");
    let disagree = json!({ "doc": doc, "page": 1, "rect": [20, 200, 220, 300], "rows": 2, "columns": 3, "names": ["a_N", "b_N"] });
    assert!(refused(&mut a, disagree).contains("must agree"));
    let wide = json!({ "doc": doc, "page": 1, "rect": [20, 200, 220, 300], "rows": 2, "columns": 51 });
    assert_eq!(refused(&mut a, wide), "a table has 1 to 50 columns");
    let none = json!({ "doc": doc, "page": 1, "rect": [20, 200, 220, 300], "rows": -1 });
    assert_eq!(refused(&mut a, none), "a table has 1 to 100 rows");
    let huge = json!({ "doc": doc, "page": 1, "rect": [20, 200, 1e39, 300], "rows": 2 });
    assert!(refused(&mut a, huge).starts_with("the area, the spacing and the column widths must be numbers"));
    assert_eq!(count(&mut a), 8);
    // The table tool's command runs it too.
    let by_command = json!({ "id": "form.add.text_table", "params": { "doc": doc, "page": 3, "rect": [20, 20, 220, 120], "rows": 2 } });
    assert_eq!(ok(&mut a, "command_run", by_command)["fields"], json!(["Table3_1", "Table3_2"]));
    assert_eq!(count(&mut a), 10);
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Add text field table");
    assert_eq!(count(&mut a), 8, "one undo takes back the whole last table");
}

#[test]
fn redact_mark_respects_single_page_and_rejects_conflicting_selectors() {
    let dir = workdir("redact-single-page");
    std::fs::write(dir.join("memo.txt"), "Private 123-45-6789").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "memo.txt" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_duplicate", json!({ "doc": doc, "pages": [1] }));
    for selector in [json!({ "find": "Private" }), json!({ "words": ["Private"] }), json!({ "pattern": "ssn" }), json!({ "whole_pages": true })] {
        let mut args = selector;
        args["doc"] = json!(doc);
        args["page"] = json!(1);
        let result = ok(&mut a, "redact_mark", args);
        assert_eq!(result["marked"], 1);
        ok(&mut a, "redact_clear", json!({ "doc": doc }));
    }
    for args in [
        json!({ "doc": doc, "find": "Private", "page": 1, "pages": [2] }),
        json!({ "doc": doc, "rect": [1, 1, 10, 10], "pages": [2] }),
        json!({ "doc": doc, "find": "Private", "page": 3 }),
    ] {
        assert!(matches!(a.call("redact_mark", &args), Err(ToolError::InvalidArgs(_))));
    }
    let result = ok(&mut a, "redact_mark", json!({ "doc": doc, "find": "Private", "page": 1 }));
    assert_eq!(result["marks_pending"], 1, "invalid selections must not add marks");
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    let text = page_text(&mut a, doc);
    assert!(!text[0].contains("Private"));
    assert!(text[1].contains("Private"));
}

#[test]
fn redacting_through_tools() {
    let dir = workdir("redact");
    std::fs::write(dir.join("memo.txt"), "Call 555-123-4567 today\nSSN 123-45-6789 is private\nPublic line").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "memo.txt" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "redact_mark", json!({ "doc": doc, "pattern": "phone" }));
    assert_eq!((r["marked"].as_u64(), r["marks_pending"].as_u64()), (Some(1), Some(1)));
    ok(&mut a, "redact_mark", json!({ "doc": doc, "pattern": "ssn", "overlay": "SSN" }));
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": "private" }));
    assert_eq!(page_text(&mut a, doc)[0].matches("555-123-4567").count(), 1, "marks alone remove nothing");
    let r = ok(&mut a, "redact_apply", json!({ "doc": doc }));
    assert_eq!((r["applied"].as_u64(), r["marks_pending"].as_u64()), (Some(3), Some(0)));
    let text = page_text(&mut a, doc)[0].clone();
    assert!(!text.contains("555") && !text.contains("6789") && !text.contains("private"), "{text}");
    assert!(text.contains("Call") && text.contains("today") && text.contains("Public line"), "{text}");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "memo.pdf" }));
    let bytes = std::fs::read(dir.join("memo.pdf")).unwrap();
    assert!(!bytes.windows(4).any(|w| w == b"4567"), "the saved file has no trace of the number");
    assert!(matches!(a.call("redact_apply", &json!({ "doc": doc })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("redact_mark", &json!({ "doc": doc, "find": "nowhere to be found" })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("redact_mark", &json!({ "doc": doc })), Err(ToolError::InvalidArgs(_))));
}

#[test]
/// Mark, apply and save remove the complete international number while retaining labels and controls.
fn redacting_italian_international_slash_numbers_through_tools() {
    let dir = workdir("redact-italian-international-slash");
    std::fs::write(dir.join("phones.txt"), "A tel. +39 0426/123456\nB tel. 0426/123456\nC protocol 2024/123456\nD on 01/10/2026").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "phones.txt" }))["doc"].as_u64().unwrap();
    let marked = ok(&mut a, "redact_mark", json!({ "doc": doc, "pattern": "phone" }));
    assert_eq!(marked["marked"], 2, "both complete phone numbers need marks");
    assert!(page_text(&mut a, doc)[0].contains("+39 0426/123456"), "marks alone remove nothing");
    assert_eq!(ok(&mut a, "redact_apply", json!({ "doc": doc }))["applied"], 2);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "redacted.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "redacted.pdf" }))["doc"].as_u64().unwrap();
    let text = &page_text(&mut a, reopened)[0];
    assert!(!text.contains("+39") && !text.contains("0426"), "{text}");
    for label in ["A tel.", "B tel."] {
        assert!(text.lines().any(|line| line.trim() == label), "only the surrounding label remains: {text}");
    }
    assert!(text.contains("protocol 2024/123456") && text.contains("on 01/10/2026"), "unrelated digit runs remain: {text}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
/// A client obeying the advertised schema can select iban, and the mark made through the tool
/// covers the spaced number and nothing else.
fn redacting_an_iban_through_tools() {
    let table = tools();
    let advertised: Vec<&str> = table.iter().find(|t| t.name == "redact_mark").unwrap().input_schema["properties"]["pattern"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(advertised.contains(&"iban"), "the advertised redact_mark schema must offer iban: {advertised:?}");
    let dir = workdir("redact-iban");
    std::fs::write(dir.join("invoice.txt"), "Pay DE89 3704 0044 0532 0130 00 today\nAccount 12345678 is ours\nTotal due").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "invoice.txt" }))["doc"].as_u64().unwrap();
    let marked = ok(&mut a, "redact_mark", json!({ "doc": doc, "pattern": "iban" }));
    assert_eq!(marked["marked"], 1, "the spaced German sample IBAN needs one mark");
    assert!(page_text(&mut a, doc)[0].contains("DE89 3704 0044 0532 0130 00"), "marks alone remove nothing");
    assert_eq!(ok(&mut a, "redact_apply", json!({ "doc": doc }))["applied"], 1);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "redacted.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "redacted.pdf" }))["doc"].as_u64().unwrap();
    let text = &page_text(&mut a, reopened)[0];
    assert!(!text.contains("DE89") && !text.contains("0532"), "{text}");
    assert!(text.lines().any(|line| line.trim() == "Pay"), "only the label before the number remains: {text}");
    assert!(text.contains("Account 12345678") && text.contains("Total due"), "text without an IBAN remains: {text}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// A mark that can't be read is refused by `redact_apply`, with the reason in the reply, and the
/// page keeps its text. (Before, the mark was skipped and the reply said there were no marks.)
#[test]
fn an_unreadable_redaction_mark_is_refused_with_the_reason() {
    let dir = workdir("redact-unreadable");
    std::fs::write(dir.join("broken.pdf"), redact_pdf_with_unreadable_mark()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "broken.pdf" }))["doc"].as_u64().unwrap();
    let Err(ToolError::Failed(reason)) = a.call("redact_apply", &json!({ "doc": doc })) else {
        panic!("redact_apply must refuse a mark with no readable area");
    };
    assert!(reason.contains("1 redaction mark(s) have no readable area") && reason.contains("nothing was redacted"), "{reason}");
    assert!(page_text(&mut a, doc)[0].contains("Secret 1234"), "the page is unchanged");
}

#[test]
fn redacting_with_codes_through_tools() {
    let dir = workdir("redact-codes");
    std::fs::write(dir.join("memo.txt"), "Informant Jane Roe met the agent\nPublic line").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "memo.txt" }))["doc"].as_u64().unwrap();
    let bad = |a: &mut Automation, args: Value| matches!(a.call("redact_mark", &args), Err(ToolError::InvalidArgs(_)));
    assert!(bad(&mut a, json!({ "doc": doc, "find": "Jane Roe", "code_set": "gdpr", "codes": ["(b)(6)"] })));
    assert!(bad(&mut a, json!({ "doc": doc, "find": "Jane Roe", "code_set": "foia", "codes": ["(k)(1)"] })));
    assert!(bad(&mut a, json!({ "doc": doc, "find": "Jane Roe", "code_set": "foia", "codes": [] })));
    assert!(bad(&mut a, json!({ "doc": doc, "find": "Jane Roe", "codes": ["(b)(6)"] })));
    assert!(bad(&mut a, json!({ "doc": doc, "find": "Jane Roe", "overlay": "X", "code_set": "foia", "codes": ["(b)(6)"] })));
    let Err(ToolError::InvalidArgs(msg)) =
        a.call("redact_mark", &json!({ "doc": doc, "find": "Jane Roe", "code_set": "foia", "codes": ["(b)(10)"] }))
    else {
        panic!("an unknown code is rejected")
    };
    assert!(msg.contains("(b)(7)(C)"), "the error lists the allowed codes: {msg}");
    assert_eq!(
        ok(&mut a, "redact_mark", json!({ "doc": doc, "find": "Jane Roe", "code_set": "foia", "codes": ["(b)(7)(C)", "(b)(6)"] }))["marked"],
        1
    );
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    let text = page_text(&mut a, doc)[0].clone();
    assert!(!text.contains("Jane") && text.contains("(b)(6), (b)(7)(C)"), "{text}");
}

#[test]
fn redacting_word_lists_through_tools() {
    let dir = workdir("redact-words");
    std::fs::write(dir.join("memo.txt"), "Call Ada today\nAda is private\nPublic line").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "memo.txt" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "redact_mark", json!({ "doc": doc, "words": ["Ada", " private ", "absent", "Ada"] }));
    assert_eq!(r["marked"].as_u64(), Some(3), "{r}");
    assert_eq!(r["matched_words"], json!([{ "word": "Ada", "marked": 2 }, { "word": "private", "marked": 1 }, { "word": "absent", "marked": 0 }]));
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Mark for redaction");
    assert_eq!(ok(&mut a, "redact_mark", json!({ "doc": doc, "words": ["Ada", "private"] }))["marks_pending"].as_u64(), Some(3));
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    let text = page_text(&mut a, doc)[0].clone();
    assert!(!text.contains("Ada") && !text.contains("private"), "{text}");
    assert!(text.contains("Call") && text.contains("Public line"), "{text}");
    for bad in [json!({ "doc": doc, "words": [] }), json!({ "doc": doc, "words": ["  "] }), json!({ "doc": doc, "words": ["x"], "find": "x" })] {
        assert!(matches!(a.call("redact_mark", &bad), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
    assert!(matches!(a.call("redact_mark", &json!({ "doc": doc, "words": ["absent", "nowhere"] })), Err(ToolError::Failed(_))));
}

#[test]
fn redaction_search_respects_case_and_word_boundaries() {
    let dir = workdir("redact-find-options");
    std::fs::write(dir.join("memo.txt"), "Ada ada Adaline\nAda beta betamax").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "memo.txt" }))["doc"].as_u64().unwrap();
    let mark = |a: &mut Automation, args: Value| ok(a, "redact_mark", args)["marked"].as_u64().unwrap();
    assert_eq!(mark(&mut a, json!({ "doc": doc, "find": "Ada", "case_sensitive": true, "whole_words": true })), 2);
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(mark(&mut a, json!({ "doc": doc, "find": "Ada", "whole_words": true })), 3);
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    let result = ok(&mut a, "redact_mark", json!({ "doc": doc, "words": ["Ada", "beta"], "case_sensitive": true, "whole_words": true }));
    assert_eq!(result["matched_words"], json!([{ "word": "Ada", "marked": 2 }, { "word": "beta", "marked": 1 }]));
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert!(matches!(a.call("redact_mark", &json!({ "doc": doc, "pattern": "email", "whole_words": true })), Err(ToolError::InvalidArgs(_))));
    assert_eq!(mark(&mut a, json!({ "doc": doc, "find": "Ada", "case_sensitive": true, "whole_words": true })), 2);
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    let text = &page_text(&mut a, doc)[0];
    assert!(text.contains("ada") && text.contains("Adaline") && text.contains("betamax"), "{text}");
}

#[test]
fn removing_hidden_information_through_tools() {
    let dir = workdir("hidden");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [20, 20], "contents": "internal note" }));
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Secret plan" }));
    let info = ok(&mut a, "doc_hidden_info", json!({ "doc": doc }));
    let count = |v: &Value, c: &str| v["categories"].as_array().unwrap().iter().find(|x| x["category"] == c).unwrap()["count"].as_u64().unwrap();
    assert!(count(&info, "comments") >= 1 && count(&info, "metadata") >= 1, "{info}");
    let r = ok(&mut a, "doc_remove_hidden", json!({ "doc": doc, "categories": ["comments"] }));
    assert_eq!(r["undo"], "Remove hidden information");
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 0);
    assert!(count(&ok(&mut a, "doc_hidden_info", json!({ "doc": doc })), "metadata") >= 1, "only comments went");
    // With none left, removing them again says so, and Optimize discarding them still runs.
    let again = a.call("doc_remove_hidden", &json!({ "doc": doc, "categories": ["comments"] }));
    assert!(matches!(&again, Err(ToolError::Failed(m)) if m.starts_with("nothing to remove")), "{again:?}");
    let optimized = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "optimized.pdf", "discard": ["comments", "bookmarks"] }));
    assert_eq!(optimized["discarded"], json!([]), "{optimized}");
    let r = ok(&mut a, "doc_remove_hidden", json!({ "doc": doc }));
    assert_eq!(r["undo"], "Sanitize document");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "clean.pdf" }));
    let bytes = std::fs::read(dir.join("clean.pdf")).unwrap();
    assert!(!bytes.windows(11).any(|w| w == b"Secret plan"), "the old revision is gone");
    assert!(matches!(a.call("doc_remove_hidden", &json!({ "doc": doc, "categories": ["nonsense"] })), Err(ToolError::InvalidArgs(_))));
    // A document that can't be edited has no scan: the refusal says why, not that nothing is there.
    let junk = [vec![b'x'; 5000], std::fs::read(dir.join("a.pdf")).unwrap()].concat();
    std::fs::write(dir.join("junk.pdf"), junk).unwrap();
    let read_only = ok(&mut a, "doc_open", json!({ "path": "junk.pdf" }))["doc"].as_u64().unwrap();
    let refused = a.call("doc_remove_hidden", &json!({ "doc": read_only, "categories": ["comments"] }));
    assert!(matches!(&refused, Err(ToolError::Failed(m)) if m.contains("can't be edited")), "{refused:?}");
}

#[test]
fn auditing_faux_redactions_through_tools() {
    let dir = workdir("faux-redaction-audit");
    std::fs::write(dir.join("redacted.pdf"), faux_redacted_fixture()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "redacted.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_audit_redactions", json!({ "doc": doc }));
    assert_eq!(r["count"], 1, "{r}");
    let f = &r["findings"][0];
    assert_eq!(f["page"], 1);
    assert_eq!(f["kind"], "covered-text");
    assert!(f["covered_text"].as_str().unwrap().contains("TOP SECRET"), "{f}");
    // Points from the top-left of the page, like every other tool (the cover is `90 688 220 32 re`
    // on a 792 pt high page).
    assert_eq!(f["rect"], json!([90.0, 72.0, 310.0, 104.0]), "{f}");
    // A clean document audits clean.
    let clean = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "doc_audit_redactions", json!({ "doc": clean }))["count"], 0);
}

/// An applied redaction at the end of a line leaves the words before it, then a gap the text
/// matrix jumps over and the redaction box: nothing is under the box any more, so the audit must
/// not report the words beside it as covered. In the middle of a line it never did.
#[test]
fn an_applied_redaction_at_the_end_of_a_line_audits_clean() {
    let dir = workdir("audit-applied-end-of-line");
    let body = "BT /F1 11 Tf 72 692 Td (Mail the team: someone@example.com) Tj ET\nBT /F1 11 Tf 72 662 Td (Account 0042-7788 is in the middle of this line.) Tj ET";
    std::fs::write(dir.join("mail.pdf"), helvetica_page(body)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "mail.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "redact_mark", json!({ "doc": doc, "pattern": "email" }));
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": "0042-7788" }));
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    let text = ok(&mut a, "text_extract", json!({ "doc": doc }))["pages"][0]["text"].as_str().unwrap().to_string();
    assert!(text.contains("Mail the team:") && !text.contains("someone") && !text.contains("0042"), "{text}");
    let r = ok(&mut a, "doc_audit_redactions", json!({ "doc": doc }));
    assert_eq!(r["count"], 0, "{r}");
}

/// A long tagged document lists its tags a page at a time, and only the types, pages or depths
/// asked for: a 20-page guide has hundreds of tags, more than one tool result should hold.
#[test]
fn structure_tags_are_listed_a_page_at_a_time() {
    let dir = workdir("tags-paging");
    // A Document holding, on each of 3 pages, one H1 and 149 P: 451 tags.
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 200 200] >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
        String::new(), // 6: the root, below
        String::new(), // 7: the Document, below
    ];
    let mut kids = Vec::new();
    for page in 0..3 {
        for i in 0..150 {
            let n = objs.len() + 1;
            kids.push(format!("{n} 0 R"));
            objs.push(format!("<< /S /{} /P 7 0 R /Pg {} 0 R >>", if i == 0 { "H1" } else { "P" }, 3 + page));
        }
    }
    objs[5] = "<< /Type /StructTreeRoot /K 7 0 R >>".into();
    objs[6] = format!("<< /S /Document /P 6 0 R /K [{}] >>", kids.join(" "));
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
    std::fs::write(dir.join("long.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "long.pdf" }))["doc"].as_u64().unwrap();
    let first = ok(&mut a, "accessibility_tags", json!({ "doc": doc }));
    assert_eq!((first["count"].as_u64(), first["tags"].as_array().map(Vec::len), first["next"].as_u64()), (Some(451), Some(200), Some(200)));
    assert_eq!(first["tags"][0]["type"], "Document");
    let last = ok(&mut a, "accessibility_tags", json!({ "doc": doc, "offset": 400, "limit": 100 }));
    assert_eq!((last["tags"].as_array().map(Vec::len), last["next"].clone()), (Some(51), Value::Null));
    let headings = ok(&mut a, "accessibility_tags", json!({ "doc": doc, "types": ["H1"] }));
    let pages: Vec<u64> = headings["tags"].as_array().unwrap().iter().map(|t| t["page"].as_u64().unwrap()).collect();
    assert_eq!((headings["count"].as_u64(), pages), (Some(3), vec![1, 2, 3]));
    let page2 = ok(&mut a, "accessibility_tags", json!({ "doc": doc, "pages": [2], "types": ["H1", "P"], "limit": 1000 }));
    assert_eq!((page2["count"].as_u64(), page2["next"].clone()), (Some(150), Value::Null));
    assert_eq!(ok(&mut a, "accessibility_tags", json!({ "doc": doc, "max_depth": 0 }))["count"], 1, "the Document alone");
    for bad in [
        json!({ "doc": doc, "limit": 0 }),
        json!({ "doc": doc, "limit": 1001 }),
        json!({ "doc": doc, "offset": -1 }),
        json!({ "doc": doc, "pages": [0] }),
    ] {
        assert!(matches!(a.call("accessibility_tags", &bad), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
}

#[test]
fn printing_through_tools() {
    let dir = workdir("print");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert!(ok(&mut a, "printers", json!({}))["printers"].is_array());
    let none = ok(&mut a, "printer_options", json!({ "printer": "No_Such_Queue_PdfCraft" }));
    assert_eq!((none["count"].as_u64(), none["options"].as_array().map(Vec::len)), (Some(0), Some(0)));
    assert!(matches!(a.call("printer_options", &json!({})), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(
        a.call("doc_print", &json!({ "doc": doc, "printer": "default", "options": ["InputSlot"] })),
        Err(ToolError::InvalidArgs(m)) if m.contains("printer_options")
    ));
    assert!(matches!(
        a.call("doc_print", &json!({ "doc": doc, "printer": "default", "options": { "InputSlot": 2 } })),
        Err(ToolError::InvalidArgs(m)) if m.contains("options.InputSlot")
    ));
    let n = ok(&mut a, "doc_info", json!({ "doc": doc }))["document"]["pages"].as_u64().unwrap();
    let r = ok(&mut a, "doc_print", json!({ "doc": doc, "layout": "multiple", "per_sheet": 4, "path": "sheets.pdf" }));
    assert_eq!(r["sheets"].as_u64(), Some(n.div_ceil(4)));
    let printed = ok(&mut a, "doc_open", json!({ "path": "sheets.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": printed }))["document"]["pages"].as_u64(), Some(n.div_ceil(4)));
    let r = ok(&mut a, "doc_print", json!({ "doc": doc, "pages": "1", "layout": "poster", "scale": 400, "path": "poster.pdf" }));
    assert!(r["sheets"].as_u64().unwrap() > 1);
    assert!(matches!(a.call("doc_print", &json!({ "doc": doc })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("doc_print", &json!({ "doc": doc, "pages": "99", "path": "x.pdf" })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn adding_content_through_tools() {
    let dir = workdir("content");
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 40, 20);
        enc.set_color(png::ColorType::Rgb);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[200u8; 2400]).unwrap();
    }
    std::fs::write(dir.join("logo.png"), png).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "CONFIDENTIAL", "at": [20, 20], "size": 18, "bold": true, "color": "red" }));
    let r = ok(&mut a, "page_add_image", json!({ "doc": doc, "page": 1, "path": "logo.png", "rect": [100, 200, 180, 240] }));
    assert_eq!(r["rect"], json!([100.0, 200.0, 180.0, 240.0]));
    assert!(page_text(&mut a, doc)[0].contains("CONFIDENTIAL"));
    let list = ok(&mut a, "content_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 2);
    assert_eq!((list["items"][0]["type"].as_str(), list["items"][0]["bold"].as_bool()), (Some("text"), Some(true)));
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 1, "text": "DRAFT", "font": "times" }));
    let text = page_text(&mut a, doc)[0].clone();
    assert!(text.contains("DRAFT") && !text.contains("CONFIDENTIAL"), "{text}");
    // Image tools: rotate, flip, crop, replace.
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 2, "rotate": 90, "flip_h": true, "crop": [0.1, 0, 0.1, 0] }));
    // One side may trim most of the picture; together an axis keeps at least 5 %.
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 2, "crop": [0.7, 0, 0, 0] }));
    assert!(matches!(
        a.call("content_update", &json!({ "doc": doc, "page": 1, "index": 2, "crop": [0.6, 0, 0.4, 0] })),
        Err(ToolError::InvalidArgs(_))
    ));
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 2, "image": "logo.png" }));
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["document"]["dirty"], true);
    assert!(matches!(a.call("content_update", &json!({ "doc": doc, "page": 1, "index": 2, "rotate": 45 })), Err(ToolError::InvalidArgs(_))));
    assert!(
        matches!(a.call("content_update", &json!({ "doc": doc, "page": 1, "index": 1, "rotate": 90 })), Err(ToolError::InvalidArgs(_))),
        "text doesn't rotate"
    );
    ok(&mut a, "content_delete", json!({ "doc": doc, "page": 1, "index": 2 }));
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["count"], 1);
    assert!(matches!(a.call("content_delete", &json!({ "doc": doc, "page": 1, "index": 5 })), Err(ToolError::Failed(_))));
}

/// #6440: `page_add_text` paints text in CMYK or gray for a print file, so its black is 100 K
/// (`0 0 0 1 k`) rather than DeviceRGB; `content_update` restyles it either way.
#[test]
fn added_text_takes_a_cmyk_or_gray_ink_through_tools() {
    let dir = workdir("content-ink");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "PROOF", "at": [20, 20], "cmyk": [0, 0, 0, 1] }));
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "Note", "at": [20, 80], "gray": 0.5 }));
    let items = ok(&mut a, "content_list", json!({ "doc": doc }))["items"].clone();
    assert_eq!((&items[0]["cmyk"], &items[1]["gray"]), (&json!([0.0, 0.0, 0.0, 1.0]), &json!(0.5)));
    // The item streams of the saved file.
    let saved_items = |a: &mut Automation, name: &str| -> Vec<String> {
        ok(a, "doc_save", json!({ "doc": doc, "path": name }));
        let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join(name)).unwrap())).unwrap();
        let page = &pdfcraft_model::pages(&saved)[0];
        let list = saved.resolve(page.dict.get(b"Contents").unwrap()).as_array().cloned().unwrap();
        list.iter()
            .filter_map(|c| match &*saved.resolve(c) {
                pdfcraft_cos::Object::Stream(s) if s.dict.name(b"PCMark") == Some(&b"Added"[..]) => {
                    Some(String::from_utf8_lossy(&s.decoded().unwrap()).into_owned())
                }
                _ => None,
            })
            .collect()
    };
    let streams = saved_items(&mut a, "ink.pdf");
    assert!(streams[0].contains("0 0 0 1 k") && !streams[0].contains("rg"), "{}", streams[0]);
    assert!(streams[1].contains("0.5 g") && !streams[1].contains("rg"), "{}", streams[1]);
    // Restyled: to RGB with color, and back to CMYK.
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 2, "color": "#000000" }));
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 1, "cmyk": [1, 0, 0, 0] }));
    let items = ok(&mut a, "content_list", json!({ "doc": doc }))["items"].clone();
    assert_eq!((&items[0]["cmyk"], items[1].get("gray")), (&json!([1.0, 0.0, 0.0, 0.0]), None));
    let streams = saved_items(&mut a, "restyled.pdf");
    assert!(streams[0].contains("1 0 0 0 k") && streams[1].contains("0 0 0 rg"), "{streams:?}");
    // One colour at a time, four numbers from 0 to 1.
    for bad in [
        json!({ "color": "red", "cmyk": [0, 0, 0, 1] }),
        json!({ "gray": 0.5, "cmyk": [0, 0, 0, 1] }),
        json!({ "gray": 1.5 }),
        json!({ "cmyk": [0, 0, 1] }),
        json!({ "cmyk": [0, 0, 0, 1, 0] }),
        json!({ "cmyk": [0, "x", 0, 1] }),
        json!({ "cmyk": [0, 0, 0, -0.1] }),
    ] {
        let mut args = json!({ "doc": doc, "page": 1, "text": "x", "at": [20, 200] });
        args.as_object_mut().unwrap().extend(bad.as_object().unwrap().clone());
        assert!(matches!(a.call("page_add_text", &args), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["count"], 2, "nothing refused was added");
    // content_update refuses them too, an image takes no ink, and a refused update changes nothing.
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 40, 20);
        enc.set_color(png::ColorType::Rgb);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[200u8; 2400]).unwrap();
    }
    std::fs::write(dir.join("logo.png"), png).unwrap();
    ok(&mut a, "page_add_image", json!({ "doc": doc, "page": 1, "path": "logo.png", "rect": [100, 200, 180, 240] }));
    let before = ok(&mut a, "content_list", json!({ "doc": doc }))["items"].clone();
    for (index, bad) in [
        (1, json!({ "gray": 0.5, "cmyk": [0, 0, 0, 1] })),
        (2, json!({ "color": "red", "gray": 0.5 })),
        (1, json!({ "size": 30, "cmyk": [0, 0, 0, 2] })),
        (2, json!({ "bold": true, "gray": -1 })),
        (3, json!({ "gray": 0.5 })),
        (3, json!({ "cmyk": [0, 0, 0, 1] })),
    ] {
        let mut args = json!({ "doc": doc, "page": 1, "index": index });
        args.as_object_mut().unwrap().extend(bad.as_object().unwrap().clone());
        assert!(matches!(a.call("content_update", &args), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["items"], before, "a refused update changed nothing");
}

/// Edit ▸ Copy and Paste headlessly (#2636): an added text box or image, as it is, onto its own
/// page beside it, onto a point, and into another document.
#[test]
fn copying_added_content_through_tools() {
    let dir = workdir("content-copy");
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 40, 20);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[90u8; 2400]).unwrap();
    }
    std::fs::write(dir.join("logo.png"), png).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let other = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    ok(
        &mut a,
        "page_add_text",
        json!({ "doc": doc, "page": 1, "text": "Approved", "at": [20, 20], "width": 100, "size": 18, "bold": true, "font": "times", "color": "red", "align": "center" }),
    );
    ok(&mut a, "page_add_image", json!({ "doc": doc, "page": 1, "path": "logo.png", "rect": [100, 200, 180, 240] }));
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 2, "rotate": 90, "flip_h": true }));
    let original = ok(&mut a, "content_list", json!({ "doc": doc }))["items"].clone();
    // Beside itself on its own page: 18 points down and to the right, with the style content_list
    // shows (the engine test checks colour, alignment and the image transforms).
    let r = ok(&mut a, "content_copy", json!({ "doc": doc, "page": 1, "index": 1 }));
    assert_eq!((r["page"].as_u64(), r["index"].as_u64()), (Some(1), Some(3)));
    let items = ok(&mut a, "content_list", json!({ "doc": doc }))["items"].clone();
    let (from, to) = (&original[0], &items[2]);
    for key in ["type", "text", "font", "size", "bold", "italic"] {
        assert_eq!(from[key], to[key], "{key}");
    }
    let shifted: Vec<f64> = from["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() + 18.0).collect();
    assert_eq!(to["rect"], json!(shifted));
    assert_eq!(r["rect"], to["rect"]);
    // An image onto another page at a point, and into another document where it was.
    let r = ok(&mut a, "content_copy", json!({ "doc": doc, "page": 1, "index": 2, "to_page": 2, "at": [10, 30] }));
    assert_eq!((r["page"].as_u64(), r["index"].as_u64()), (Some(2), Some(1)));
    assert_eq!(r["rect"][0].as_f64().zip(r["rect"][1].as_f64()), Some((10.0, 30.0)), "its top-left at the point");
    let r = ok(&mut a, "content_copy", json!({ "doc": doc, "page": 1, "index": 2, "to_doc": other }));
    assert_eq!(r["rect"], original[1]["rect"], "in the same place");
    let theirs = ok(&mut a, "content_list", json!({ "doc": other }));
    assert_eq!((theirs["count"].as_u64(), theirs["items"][0]["type"].as_str()), (Some(1), Some("image")));
    // Copied there again: it steps past the first copy, whose picture that document stores itself.
    let r = ok(&mut a, "content_copy", json!({ "doc": doc, "page": 1, "index": 2, "to_doc": other }));
    let stepped: Vec<f64> = original[1]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() + 18.0).collect();
    assert_eq!(r["rect"], json!(stepped), "beside the first copy, not on it");
    // Each paste is one undo step in the document it went into.
    ok(&mut a, "edit_undo", json!({ "doc": other }));
    ok(&mut a, "edit_undo", json!({ "doc": other }));
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": other }))["count"], 0);
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["count"], 4);
    // Out of range: a readable failure.
    assert!(matches!(a.call("content_copy", &json!({ "doc": doc, "page": 1, "index": 9 })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("content_copy", &json!({ "doc": doc, "page": 1, "index": 1, "to_page": 9 })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("content_copy", &json!({ "doc": doc, "page": 1, "index": 1, "to_doc": 999 })), Err(ToolError::Failed(_))));
    for at in [json!([1]), json!([10, "x", 30]), json!("10, 30")] {
        assert!(matches!(a.call("content_copy", &json!({ "doc": doc, "page": 1, "index": 1, "at": at })), Err(ToolError::InvalidArgs(_))), "{at}");
    }
    // A second copy onto its own page steps past the first.
    let r = ok(&mut a, "content_copy", json!({ "doc": doc, "page": 1, "index": 1 }));
    let twice: Vec<f64> = from["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() + 36.0).collect();
    assert_eq!(r["rect"], json!(twice));

    // A document that doesn't allow copying keeps its content in, even with changes allowed.
    let mut cos = pdfcraft_cos::Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    cos.set_encryption(&pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Aes256,
        user_password: "",
        owner_password: "owner",
        permissions: -1 ^ 0b1_0000,
        encrypt_metadata: true,
        seed: [3; 32],
    })
    .unwrap();
    std::fs::write(dir.join("no-copy.pdf"), pdfcraft_cos::write_full(&cos, &Default::default()).unwrap()).unwrap();
    let locked = ok(&mut a, "doc_open", json!({ "path": "no-copy.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": locked, "page": 1, "text": "Secret", "at": [20, 20], "width": 100 }));
    for to in [locked, other] {
        let refused = a.call("content_copy", &json!({ "doc": locked, "page": 1, "index": 1, "to_doc": to }));
        assert!(matches!(&refused, Err(ToolError::Failed(m)) if m.contains("copying")), "{refused:?}");
    }
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": locked }))["count"], 1);
}

/// An out-of-range `index` (including `i64::MIN`, which overflowed `index - 1`) is a readable
/// failure for `content_update` and `content_delete`, not a panic.
#[test]
fn content_index_out_of_range_is_an_error_not_a_panic() {
    let dir = workdir("content-index-range");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "hi", "at": [10, 10] }));
    for index in [i64::MIN, i64::MIN + 1, -1, 0, i64::MAX] {
        for tool in ["content_delete", "content_update"] {
            let args = json!({ "doc": doc, "page": 1, "index": index, "text": "x" });
            let args = if tool == "content_delete" { json!({ "doc": doc, "page": 1, "index": index }) } else { args };
            assert!(matches!(a.call(tool, &args), Err(ToolError::Failed(_))), "{tool} index {index}");
        }
    }
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["count"], 1);
}

/// Character spacing on added text (#4245): set when adding or restyling, listed back, and refused
/// out of range or on images.
#[test]
fn added_text_takes_character_spacing() {
    let dir = workdir("content-char-spacing");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "Spaced", "at": [20, 20], "char_spacing": 2 }));
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["items"][0]["char_spacing"], 2.0);
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 1, "char_spacing": -0.5 }));
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["items"][0]["char_spacing"], -0.5);
    for bad in [json!(1000), json!(-1000), json!("wide")] {
        let args = json!({ "doc": doc, "page": 1, "index": 1, "char_spacing": bad });
        assert!(matches!(a.call("content_update", &args), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
    assert_eq!(ok(&mut a, "content_list", json!({ "doc": doc }))["items"][0]["char_spacing"], -0.5);
}

#[test]
fn large_added_image_rotations_keep_orientation_and_undo() {
    let dir = workdir("large-added-rotation");
    added_rotation_image(&dir);
    let mut a = auto(&dir);
    let image = |a: &Automation, doc| {
        let d = a.session().get(pdfcraft_engine::DocId(doc)).unwrap();
        let pdfcraft_engine::AddedContent::Image(image) = &d.added()[0].content else { panic!("added image") };
        image.clone()
    };
    let largest = i64::MAX / 90 * 90;
    let smallest = i64::MIN / 90 * 90;
    for initial in [1, 0, 2, 3] {
        let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300 }))["doc"].as_u64().unwrap();
        ok(&mut a, "page_add_image", json!({ "doc": doc, "page": 1, "path": "rotation.png", "rect": [20, 20, 120, 70] }));
        ok(
            &mut a,
            "content_update",
            json!({ "doc": doc, "page": 1, "index": 1, "rotate": initial * 90,
            "flip_h": true, "flip_v": true, "crop": [0.1, 0.05, 0.2, 0.1] }),
        );
        let before = image(&a, doc);
        for degrees in [largest, largest - 90, largest - 180, smallest, smallest + 90, smallest + 180, -450, -90, 0, 90, 450] {
            ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 1, "rotate": degrees }));
            let expected = pdfcraft_engine::AddedImage { rotation: (initial + (degrees / 90).rem_euclid(4)).rem_euclid(4) as u8, ..before.clone() };
            assert_eq!(image(&a, doc), expected, "initial quarter turn {initial}, degrees {degrees}");
            ok(&mut a, "edit_undo", json!({ "doc": doc }));
            assert_eq!(image(&a, doc), before, "Undo restores the image and its properties");
            ok(&mut a, "edit_redo", json!({ "doc": doc }));
            assert_eq!(image(&a, doc), expected, "Redo restores the normalized orientation");
            ok(&mut a, "edit_undo", json!({ "doc": doc }));
        }
        ok(&mut a, "doc_save", json!({ "doc": doc, "path": format!("initial-{initial}.pdf") }));
        let state = |a: &Automation| {
            let d = a.session().get(pdfcraft_engine::DocId(doc)).unwrap();
            (d.bytes.clone(), d.dirty, d.edit_generation(), d.can_undo().map(str::to_owned), d.can_redo().map(str::to_owned))
        };
        let saved = state(&a);
        assert!(!saved.1 && saved.4.is_some(), "a clean document with a redo step");
        for degrees in [i64::MIN, i64::MAX, -1, 1, 45] {
            assert!(matches!(
                a.call("content_update", &json!({ "doc": doc, "page": 1, "index": 1, "rotate": degrees })),
                Err(ToolError::InvalidArgs(_))
            ));
            assert_eq!(state(&a), saved, "invalid degrees {degrees} leave bytes and history alone");
        }
    }
}

#[cfg(feature = "mcp")]
#[test]
fn large_added_image_rotations_round_trip_through_mcp() {
    let dir = workdir("large-added-rotation-mcp");
    added_rotation_image(&dir);
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300 }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_image", json!({ "doc": doc, "page": 1, "path": "rotation.png", "rect": [20, 20, 120, 70] }));
    ok(
        &mut a,
        "content_update",
        json!({ "doc": doc, "page": 1, "index": 1, "rotate": 90,
        "flip_h": true, "flip_v": true, "crop": [0.1, 0.05, 0.2, 0.1] }),
    );
    let mut server = McpServer::new(a);
    let render = json!({ "name": "page_render", "arguments": { "doc": doc, "page": 1, "dpi": 72 } });
    let before = rpc(&mut server, 1, "tools/call", render.clone());
    let update = rpc(
        &mut server,
        2,
        "tools/call",
        json!({ "name": "content_update", "arguments": {
        "doc": doc, "page": 1, "index": 1, "rotate": i64::MAX / 360 * 360
    } }),
    );
    assert_eq!(update["result"]["isError"], false, "{update}");
    let after = rpc(&mut server, 3, "tools/call", render.clone());
    assert_eq!(after["result"]["content"], before["result"]["content"], "full turns retain the rendered orientation");
    let saved = rpc(&mut server, 4, "tools/call", json!({ "name": "doc_save", "arguments": { "doc": doc, "path": "rotated.pdf" } }));
    assert_eq!(saved["result"]["isError"], false, "{saved}");
    let mut reopened = auto(&dir);
    let doc = ok(&mut reopened, "doc_open", json!({ "path": "rotated.pdf" }))["doc"].as_u64().unwrap();
    let d = reopened.session().get(pdfcraft_engine::DocId(doc)).unwrap();
    let pdfcraft_engine::AddedContent::Image(image) = &d.added()[0].content else { panic!("saved image") };
    assert_eq!(image.rotation, 1);
    let png = reopened.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    let Content::Png { data, .. } = &png[0] else { panic!("rendered PNG") };
    assert_eq!(after["result"]["content"][0]["data"], base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data));
}

/// Text styling on an added image is refused (#1331), as image transforms are on text, and the
/// refusal leaves the content, the dirty flag and the undo and redo steps as they were.
#[test]
fn text_styling_on_an_image_is_refused_without_an_edit() {
    let dir = workdir("content-style");
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 1, 1);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[80, 160, 200]).unwrap();
    }
    std::fs::write(dir.join("logo.png"), png).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": doc, "page": 1, "text": "sample", "at": [20, 20] }));
    ok(&mut a, "page_add_image", json!({ "doc": doc, "page": 1, "path": "logo.png", "rect": [10, 100, 40, 130] }));
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 2, "rotate": 90 }));
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "saved.pdf" }));
    let state = |a: &Automation| {
        let d = a.session().docs().iter().find(|d| d.id.0 == doc).unwrap();
        (format!("{:?}", d.added()), d.dirty, d.edit_generation(), d.can_undo().map(str::to_owned), d.can_redo().map(str::to_owned))
    };
    let before = state(&a);
    assert!(!before.1 && before.4.is_some(), "a clean document with a step to redo: {before:?}");
    for (key, value) in [
        ("font", json!("times")),
        ("size", json!(24)),
        ("bold", json!(true)),
        ("italic", json!(true)),
        ("color", json!("red")),
        ("align", json!("center")),
    ] {
        for rect in [None, Some(json!([20, 100, 50, 130]))] {
            let mut args = json!({ "doc": doc, "page": 1, "index": 2 });
            args[key] = value.clone();
            if let Some(r) = rect {
                args["rect"] = r;
            }
            assert!(matches!(a.call("content_update", &args), Err(ToolError::InvalidArgs(_))), "{args}");
            assert_eq!(state(&a), before, "{args}");
        }
    }
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 1, "font": "times", "size": 24, "align": "center" }));
    assert!(state(&a).1, "text still takes its styling");
}

#[test]
fn form_formats_and_calculations_through_tools() {
    let dir = workdir("formcalc");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (name, y) in [("Price", 20), ("Qty", 60), ("Total", 100)] {
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    ok(
        &mut a,
        "form_set_props",
        json!({ "doc": doc, "field": "Price", "format": { "type": "number", "decimals": 2, "currency": "$" }, "validate": { "min": 0 } }),
    );
    ok(
        &mut a,
        "form_set_props",
        json!({ "doc": doc, "field": "Total", "format": { "type": "number", "decimals": 2, "currency": "$" }, "calculate": { "notation": "Price * Qty" } }),
    );
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "Price": "19.99", "Qty": "3" } }));
    let f = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let total = f["fields"].as_array().unwrap().iter().find(|x| x["name"] == "Total").unwrap().clone();
    assert_eq!((total["value"].as_str(), total["display"].as_str()), (Some("59.97"), Some("$59.97")), "{total}");
    assert_eq!(total["calculate"]["notation"], "Price * Qty");
    assert!(page_text(&mut a, doc)[0].contains("$59.97"));
    let err = a.call("form_fill", &json!({ "doc": doc, "values": { "Price": "-5" } })).unwrap_err();
    assert!(err.to_string().contains("greater than or equal to 0"), "{err}");
    assert!(matches!(
        a.call("form_set_props", &json!({ "doc": doc, "field": "Qty", "format": { "type": "roman" } })),
        Err(ToolError::InvalidArgs(_))
    ));
}

/// Renaming a field renames it in the formulas that read it: a sum dropped the field, notation
/// read it as 0 and a script kept a stale value.
#[test]
fn renaming_a_field_keeps_the_formulas_that_read_it() {
    let dir = workdir("formrename");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (i, name) in ["a", "b", "c", "d", "e", "x", "sum", "product", "double", "total"].into_iter().enumerate() {
        let y = 20 + 30 * i;
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "sum", "calculate": { "op": "sum", "fields": ["a", "b"] } }));
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "product", "calculate": { "notation": "c * d" } }));
    ok(
        &mut a,
        "form_set_script",
        json!({ "doc": doc, "field": "double", "event": "calculate", "script": "event.value = getField(\"e\").value * 2;" }),
    );
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": { "op": "sum", "fields": ["x"] } }));
    let renamed = |a: &mut Automation, from: &str, to: &str| ok(a, "form_set_props", json!({ "doc": doc, "field": from, "name": to }));
    assert_eq!(renamed(&mut a, "a", "qty")["formulas_updated"], json!(["sum"]));
    assert_eq!(renamed(&mut a, "c", "price")["formulas_updated"], json!(["product"]));
    assert_eq!(renamed(&mut a, "e", "base")["formulas_updated"], json!(["double"]));
    assert_eq!(renamed(&mut a, "total", "grand")["formulas_updated"], json!([]), "a calculated field's own name is in no formula");
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "qty": "10", "b": "2", "price": "10", "d": "4", "base": "10", "x": "9" } }));
    let values = |a: &mut Automation| {
        let f = ok(a, "form_fields", json!({ "doc": doc }));
        let value = |name: &str| f["fields"].as_array().unwrap().iter().find(|x| x["name"] == name).map(|x| x["value"].clone());
        [value("sum"), value("product"), value("double"), value("grand")]
    };
    assert_eq!(values(&mut a), [Some(json!("12")), Some(json!("40")), Some(json!("20")), Some(json!("9"))]);
    let f = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let field = |name: &str| f["fields"].as_array().unwrap().iter().find(|x| x["name"] == name).unwrap().clone();
    assert_eq!(field("sum")["calculate"]["fields"], json!(["qty", "b"]));
    assert_eq!(field("product")["calculate"]["notation"], "price * d");
    // The rename and the formulas it changed are one undo step: undo the fill, then the last
    // three renames.
    for _ in 0..4 {
        ok(&mut a, "edit_undo", json!({ "doc": doc }));
    }
    let f = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let field = |name: &str| f["fields"].as_array().unwrap().iter().find(|x| x["name"] == name).cloned();
    assert!(field("c").is_some() && field("price").is_none());
    assert_eq!(field("product").unwrap()["calculate"]["notation"], "c * d", "undone together with the rename");
    // The renamed field's own formula, changed in the same call, is not one the rename
    // rewrote, though the name is stored trimmed.
    let r = ok(
        &mut a,
        "form_set_props",
        json!({ "doc": doc, "field": "sum", "name": " added ", "calculate": { "op": "sum", "fields": ["qty", "b", "x"] } }),
    );
    assert_eq!(r["formulas_updated"], json!([]));
}

/// The calculation order through the tool: read with what each formula reads and what is wrong,
/// fixed so stale values come out right, moved, set, and undone.
#[test]
fn the_calculation_order_is_read_fixed_and_set() {
    let dir = workdir("calcorder");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (i, name) in ["qty", "price", "sub", "total", "tax", "x", "y"].into_iter().enumerate() {
        let y = 20 + 30 * i;
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    // Defined backwards: total and tax calculate before the fields they read.
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": { "op": "sum", "fields": ["sub", "tax"] } }));
    ok(
        &mut a,
        "form_set_script",
        json!({ "doc": doc, "field": "tax", "event": "calculate", "script": "event.value = getField(\"sub\").value / 10;" }),
    );
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "sub", "calculate": { "notation": "qty * price" } }));
    // x and y read each other.
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "x", "calculate": { "notation": "y + 1" } }));
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "y", "calculate": { "notation": "x + 1" } }));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "qty": "3", "price": "10" } }));
    let value = |a: &mut Automation, name: &str| {
        let f = ok(a, "form_fields", json!({ "doc": doc }));
        f["fields"].as_array().unwrap().iter().find(|x| x["name"] == name).map(|x| x["value"].clone()).unwrap()
    };
    assert_eq!((value(&mut a, "total"), value(&mut a, "tax")), (json!("0"), json!("0")), "stale: they calculate too early");
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc }));
    assert_eq!(r["calculation_order"], json!(["total", "tax", "sub", "x", "y"]));
    assert_eq!(r["fields"][0]["reads"], json!(["sub", "tax"]));
    assert_eq!(r["fields"][0]["issues"], json!([{ "issue": "runs_too_early", "field": "tax" }, { "issue": "runs_too_early", "field": "sub" }]));
    assert_eq!(r["fields"][1]["issues"], json!([{ "issue": "runs_too_early", "field": "sub" }]), "a script's reads are checked too");
    assert_eq!(r["fields"][3]["issues"], json!([{ "issue": "cycle", "fields": ["y"] }]));
    assert_eq!((r["fixable"].clone(), r["extra"].clone()), (json!(true), json!([])));
    assert!(r.get("undo").is_none(), "reading changes nothing");
    // Prepare a form ▸ Calculation order is this tool headlessly.
    let r = ok(&mut a, "command_run", json!({ "id": "form.calculation_order", "params": { "doc": doc } }));
    assert_eq!(r["calculation_order"], json!(["total", "tax", "sub", "x", "y"]));
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc, "fix": true }));
    assert_eq!(r["calculation_order"], json!(["sub", "tax", "total", "x", "y"]), "only what must move moves; the loop stays");
    assert_eq!(r["undo"], "Set calculation order");
    assert_eq!(r["fixable"], false);
    assert_eq!((value(&mut a, "total"), value(&mut a, "tax")), (json!("33"), json!("3")), "recalculated in the new order");
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc, "field": "y", "move": "earlier" }));
    assert_eq!(r["calculation_order"], json!(["sub", "tax", "total", "y", "x"]));
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc, "order": ["x"] }));
    assert_eq!(r["calculation_order"], json!(["x", "sub", "tax", "total", "y"]), "the fields named first, the others after them");
    for bad in [json!({ "order": ["qty"] }), json!({ "order": ["sub", "sub"] }), json!({ "field": "qty" }), json!({ "fix": true, "field": "x" })] {
        let mut args = bad.clone();
        args["doc"] = json!(doc);
        assert!(a.call("form_calculation_order", &args).is_err(), "{bad}");
    }
    for _ in 0..3 {
        ok(&mut a, "edit_undo", json!({ "doc": doc }));
    }
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc }));
    assert_eq!(r["calculation_order"], json!(["total", "tax", "sub", "x", "y"]));
    assert_eq!((value(&mut a, "total"), value(&mut a, "tax")), (json!("0"), json!("0")), "undone with the values it recalculated");
}

/// Each row of the calculation order gets its own field's formula, even where two fields share a
/// name (a malformed form): the first dup sums src, the second x.
#[test]
fn the_calculation_order_gives_each_field_its_own_formula() {
    let dir = workdir("calcdup");
    std::fs::write(dir.join("dup.pdf"), two_calculations_with_one_name()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "dup.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc }));
    assert_eq!(r["calculation_order"], json!(["dup", "src", "dup"]));
    let formulas: Vec<Value> = r["fields"].as_array().unwrap().iter().map(|f| f["formula"].clone()).collect();
    assert_eq!(
        formulas,
        [json!({ "op": "sum", "fields": ["src"] }), json!({ "op": "sum", "fields": ["x"] }), json!({ "op": "sum", "fields": ["x"] })]
    );
}

/// The calculation order through the tool shows each formula, in form_set_props's shape or as a
/// script, and flags a read of a field the form doesn't have and a script that builds the names
/// it reads; form_fields shows a custom script too.
#[test]
fn the_calculation_order_shows_formulas_and_what_they_cannot_read() {
    let dir = workdir("calcformulas");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (i, name) in ["qty", "price", "sub", "tax", "total"].into_iter().enumerate() {
        let y = 20 + 30 * i;
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    let script = "event.value = getField(\"sub\").value / 10 + getField(\"row\" + 1).value;";
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "sub", "calculate": { "notation": "qty * pric" } }));
    ok(&mut a, "form_set_script", json!({ "doc": doc, "field": "tax", "event": "calculate", "script": script }));
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": { "op": "sum", "fields": ["sub", "tax"] } }));
    let r = ok(&mut a, "form_calculation_order", json!({ "doc": doc }));
    assert_eq!(r["calculation_order"], json!(["sub", "tax", "total"]));
    let formulas: Vec<Value> = r["fields"].as_array().unwrap().iter().map(|f| f["formula"].clone()).collect();
    assert_eq!(formulas, [json!({ "notation": "qty * pric" }), json!({ "script": script }), json!({ "op": "sum", "fields": ["sub", "tax"] })]);
    assert_eq!(r["fields"][0]["issues"], json!([{ "issue": "missing_field", "field": "pric" }]));
    assert_eq!(r["fields"][1]["reads"], json!(["sub"]));
    assert_eq!(r["fields"][1]["issues"], json!([{ "issue": "built_names" }]));
    assert_eq!(r["fields"][2]["issues"], json!([]));
    assert_eq!(r["fixable"], false, "neither is an order to fix");
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let tax = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "tax").unwrap();
    assert_eq!(tax["calculate"], json!({ "script": script }));
    // A script is set with form_set_script, and form_set_props says so, also when a notation
    // comes with it.
    for calculate in [json!({ "script": script }), json!({ "notation": "qty * 2", "script": script })] {
        let e = err(&mut a, "form_set_props", json!({ "doc": doc, "field": "tax", "calculate": calculate }));
        assert!(e.to_string().contains("form_set_script"), "{calculate}: {e}");
    }
}

/// Every operation comes back from form_fields and form_calculation_order as form_set_props takes
/// it, so a formula can be read and given back: product and average came back as prd and avg,
/// which it refused.
#[test]
fn every_calculation_op_can_be_read_and_given_back() {
    let dir = workdir("calcops");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (name, y) in [("a", 20), ("b", 60), ("total", 100)] {
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    for op in ["sum", "product", "average", "min", "max"] {
        let calculate = json!({ "op": op, "fields": ["a", "b"] });
        ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": calculate }));
        let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
        let shown = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "total").unwrap()["calculate"].clone();
        let order = ok(&mut a, "form_calculation_order", json!({ "doc": doc }));
        assert_eq!((&shown, &order["fields"][0]["formula"]), (&calculate, &calculate), "{op}");
        ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": shown }));
    }
}

/// form_set_props refuses a calculate or validate it would only partly apply (a key it doesn't
/// know, a value of the wrong type, a notation beside op and fields) as a bad argument, before
/// the edit runs, and changes nothing. A validate read back from form_fields, with null for the
/// bound it doesn't have, is taken back.
#[test]
fn calculate_and_validate_refuse_what_they_would_drop() {
    let dir = workdir("calcstrict");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (name, y) in [("a", 20), ("b", 60), ("total", 100)] {
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, y, 180, y + 22], "name": name }));
    }
    let set = json!({ "op": "sum", "fields": ["a", "b"] });
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": set, "validate": { "min": 0, "max": 10 } }));
    let total = |a: &mut Automation| {
        let fields = ok(a, "form_fields", json!({ "doc": doc }));
        fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "total").unwrap().clone()
    };
    for (calculate, says) in [
        (json!({ "notation": "a", "op": "sum", "fields": ["b"] }), "not both"),
        (json!({ "notation": "a", "op": "sum" }), "not both"),
        (json!({ "notation": "a", "fields": ["b"] }), "not both"),
        // A notation that wouldn't read back as itself from the script's comment.
        (json!({ "notation": "a EVCALC b" }), "can't be stored"),
        (json!({ "notation": "a */ x /*" }), "can't be stored"),
        (json!({ "notation": " " }), "can't be stored"),
        (json!({ "op": "sum", "fields": ["a", 5] }), "field names"),
        (json!({ "op": "sum", "fields": "a" }), "list of field names"),
        (json!({ "notation": 5 }), "notation must be a string"),
        (json!({ "op": 5, "fields": ["a"] }), "op must be"),
        (json!({ "fields": ["a"] }), "needs op"),
        (json!({ "op": "sum", "feilds": ["a"] }), "unknown key \"feilds\""),
    ] {
        // InvalidArgs, not Failed: an unstorable notation is refused by the tool itself, not
        // only by the edit underneath it.
        let e = err(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "calculate": calculate }));
        assert!(matches!(e, ToolError::InvalidArgs(_)) && e.to_string().contains(says), "{calculate}: {e:?}");
    }
    for (validate, says) in [(json!({ "min": 0, "max": "100" }), "max must be a number"), (json!({ "min": 0, "mxa": 5 }), "unknown key \"mxa\"")] {
        let e = err(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "validate": validate }));
        assert!(matches!(e, ToolError::InvalidArgs(_)) && e.to_string().contains(says), "{validate}: {e:?}");
    }
    let t = total(&mut a);
    assert_eq!((&t["calculate"], &t["validate"]), (&set, &json!({ "min": 0.0, "max": 10.0 })), "nothing changed");
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "validate": { "min": null, "max": 5 } }));
    let shown = total(&mut a)["validate"].clone();
    assert_eq!(shown, json!({ "min": null, "max": 5.0 }));
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "total", "validate": shown }));
}

#[test]
fn page_tab_orders_are_read_and_set_per_page_through_tools() {
    let dir = workdir("pagetabs");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc }));
    assert_eq!(r["page_tabs"], json!(["annotations", "annotations", "annotations"]));
    assert!(r["undo"].is_null(), "reading changes nothing: {r}");
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "order": "column", "pages": [1, 3] }));
    assert_eq!(r["page_tabs"], json!(["column", "annotations", "column"]));
    assert_eq!((r["undo"].as_str(), r["dirty"].as_bool()), (Some("Set tab order"), Some(true)));
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "order": "structure", "pages": [2] }));
    assert_eq!(r["page_tabs"], json!(["column", "structure", "column"]));
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "order": "unspecified", "pages": [1] }));
    assert_eq!(r["page_tabs"], json!(["annotations", "structure", "column"]));
}

#[test]
fn tab_order_and_field_appearance_through_tools() {
    let dir = workdir("taborder");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    // Added out of reading order: B (top right), A (top left), C (below).
    for (name, x, y) in [("B", 110, 20), ("A", 10, 20), ("C", 10, 60)] {
        ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [x, y, x + 80, y + 20], "name": name }));
    }
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "order": "row" }));
    assert_eq!(r["tab_order"], json!(["A", "B", "C"]));
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "order": "column" }));
    assert_eq!(r["tab_order"], json!(["A", "C", "B"]));
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "field": "B", "move": "earlier" }));
    assert_eq!(r["tab_order"], json!(["A", "B", "C"]), "ordered manually");
    let r = ok(&mut a, "form_tab_order", json!({ "doc": doc, "order": "column" }));
    assert_eq!(r["tab_order"], json!(["A", "C", "B"]));
    ok(
        &mut a,
        "form_set_props",
        json!({ "doc": doc, "field": "A", "appearance": { "border": "red", "fill": "none", "style": "underline", "font": "courier" } }),
    );
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "A": "typed" } }));
    assert!(page_text(&mut a, doc)[0].contains("typed"));
    assert!(matches!(
        a.call("form_set_props", &json!({ "doc": doc, "field": "A", "appearance": { "style": "wavy" } })),
        Err(ToolError::InvalidArgs(_))
    ));
}
