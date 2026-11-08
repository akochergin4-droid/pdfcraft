//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 3 of 8).

#[allow(unused_imports)]
use super::*;

#[test]
fn exporting_images_and_text_through_tools() {
    let dir = workdir("export");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "out", "dpi": 72, "pages": [1, 3] }));
    assert_eq!(r["count"], 2);
    assert!(dir.join("out/a_page_3.png").exists());
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "out", "dpi": 72, "pages": [2], "format": "jpeg", "quality": 70 }));
    assert!(std::fs::read(dir.join("out/a_page_2.jpg")).unwrap().starts_with(&[0xFF, 0xD8]));
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "out", "dpi": 72, "pages": [2], "format": "tiff" }));
    let tif = std::fs::read(dir.join("out/a_page_2.tif")).unwrap();
    assert!(tif.starts_with(b"II*\0") || tif.starts_with(b"MM\0*"));
    assert!(matches!(a.call("doc_export_images", &json!({ "doc": doc, "folder": "out", "format": "webp" })), Err(ToolError::InvalidArgs(_))));
    ok(&mut a, "doc_export_text", json!({ "doc": doc, "path": "a.txt" }));
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "Page 1\n\u{c}Page 2\n\u{c}Page 3\n");
    assert!(a.call("doc_export_text", &json!({ "doc": doc, "path": "/etc/x.txt" })).is_err(), "confined to the root");
}

#[test]
fn accessibility_character_encoding_accepts_identity_cjk_through_tools() {
    use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, SaveOptions, Stream, write_full};
    use std::sync::Arc;

    let dir = workdir("a11y-identity-cjk");
    let mut a = auto(&dir);
    for encoding in ["Identity-H", "Identity-V"] {
        let mut source = Document::open(Arc::new(fixture(1))).unwrap();
        let mut system = Dict::new();
        system.set(b"Registry".to_vec(), Object::String(PdfString::literal(b"Adobe".to_vec())));
        system.set(b"Ordering".to_vec(), Object::String(PdfString::literal(b"Japan1".to_vec())));
        system.set(b"Supplement".to_vec(), Object::Int(0));
        let system = source.add(Object::Dict(system));
        let mut descriptor = Dict::new();
        descriptor.set(b"Type".to_vec(), Object::Name(b"FontDescriptor".to_vec()));
        descriptor.set(b"FontName".to_vec(), Object::Name(b"SyntheticCID".to_vec()));
        descriptor.set(b"Flags".to_vec(), Object::Int(6));
        descriptor.set(b"FontBBox".to_vec(), Object::Array([0, -200, 1000, 800].into_iter().map(Object::Int).collect()));
        descriptor.set(b"ItalicAngle".to_vec(), Object::Int(0));
        descriptor.set(b"Ascent".to_vec(), Object::Int(800));
        descriptor.set(b"Descent".to_vec(), Object::Int(-200));
        descriptor.set(b"CapHeight".to_vec(), Object::Int(700));
        descriptor.set(b"StemV".to_vec(), Object::Int(80));
        let descriptor = source.add(Object::Dict(descriptor));
        let mut cid = Dict::new();
        cid.set(b"Type".to_vec(), Object::Name(b"Font".to_vec()));
        cid.set(b"Subtype".to_vec(), Object::Name(b"CIDFontType0".to_vec()));
        cid.set(b"BaseFont".to_vec(), Object::Name(b"SyntheticCID".to_vec()));
        cid.set(b"CIDSystemInfo".to_vec(), Object::Ref(system));
        cid.set(b"FontDescriptor".to_vec(), Object::Ref(descriptor));
        cid.set(b"DW".to_vec(), Object::Int(1000));
        let cid = source.add(Object::Dict(cid));
        source
            .update_dict(ObjRef::new(3, 0), |font| {
                font.set(b"Subtype".to_vec(), Object::Name(b"Type0".to_vec()));
                font.set(b"BaseFont".to_vec(), Object::Name(b"SyntheticCID".to_vec()));
                font.set(b"Encoding".to_vec(), Object::Name(encoding.as_bytes().to_vec()));
                font.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Ref(cid)]));
            })
            .unwrap();
        source.set(ObjRef::new(5, 0), Object::Stream(Stream::flate(Dict::new(), b"BT /F1 24 Tf 20 150 Td <002200230024> Tj ET")));
        let name = format!("{encoding}.pdf");
        let bytes = write_full(&source, &SaveOptions::default()).unwrap();
        std::fs::write(dir.join(&name), &bytes).unwrap();
        let doc = ok(&mut a, "doc_open", json!({ "path": name }))["doc"].as_u64().unwrap();
        assert_eq!(page_text(&mut a, doc), ["ABC"], "the collection maps the CIDs to Unicode without ToUnicode");
        let report = ok(&mut a, "accessibility_check", json!({ "doc": doc, "rules": ["character-encoding"] }));
        let result = report["results"].as_array().unwrap().iter().find(|r| r["rule"] == "character-encoding").unwrap();
        assert_eq!(result["status"], "passed", "{encoding}: {result}");
        assert_eq!(result["findings"], json!([]));
        assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["document"]["dirty"], false);
        assert_eq!(std::fs::read(dir.join(name)).unwrap(), bytes, "checking is read-only");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn accessibility_check_report_and_fixes_through_tools() {
    let dir = workdir("a11y");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "accessibility_check", json!({ "doc": doc }));
    assert_eq!(r["results"].as_array().unwrap().len(), 32);
    let status =
        |r: &Value, id: &str| r["results"].as_array().unwrap().iter().find(|x| x["rule"] == id).unwrap()["status"].as_str().unwrap().to_owned();
    for id in ["tagged-pdf", "primary-language", "title", "tagged-content"] {
        assert_eq!(status(&r, id), "failed", "{id}");
    }
    assert_eq!((status(&r, "color-contrast"), status(&r, "scripts")), ("skipped".into(), "manual".into()));
    assert_eq!(status(&ok(&mut a, "accessibility_check", json!({ "doc": doc, "all": true })), "color-contrast"), "manual");
    let docs = ok(&mut a, "accessibility_check", json!({ "doc": doc, "categories": ["document"] }));
    assert_eq!(docs["skipped"], 25, "24 other rules and colour contrast");
    // Fixes.
    assert!(matches!(a.call("accessibility_fix", &json!({ "doc": doc, "rule": "primary-language" })), Err(ToolError::Failed(_))), "needs a language");
    assert_eq!(ok(&mut a, "accessibility_fix", json!({ "doc": doc, "rule": "primary-language", "value": "en-GB" }))["status"], "passed");
    assert_eq!(ok(&mut a, "accessibility_fix", json!({ "doc": doc, "rule": "title" }))["status"], "passed");
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["title"], "a");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Set document title");
    assert!(matches!(a.call("accessibility_fix", &json!({ "doc": doc, "rule": "tagged-pdf" })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("accessibility_check", &json!({ "doc": doc, "rules": ["nope"] })), Err(ToolError::InvalidArgs(_))));
    // The report.
    let r = ok(&mut a, "accessibility_report", json!({ "doc": doc, "path": "report.html" }));
    assert_eq!(r["failed"].as_u64(), Some(3), "tagging, tagged content and (undone) title");
    let html = std::fs::read_to_string(dir.join("report.html")).unwrap();
    assert!(html.contains("Accessibility Report") && html.contains("a.pdf"));
    ok(&mut a, "accessibility_report", json!({ "doc": doc, "path": "manual-report.html", "rules": ["scripts"] }));
    let manual_html = std::fs::read_to_string(dir.join("manual-report.html")).unwrap();
    assert!(manual_html.contains("<p>Needs manual check: 1</p>"));
    assert!(!manual_html.contains("no problems"));
}

#[test]
fn accessibility_figure_tags_and_alternative_text_through_tools() {
    let dir = workdir("a11y-figures");
    let mut a = auto(&dir);
    // Figures: list, describe, mark decorative.
    std::fs::write(
        dir.join("figures.pdf"),
        b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /StructParents 0 >> endobj
4 0 obj << /Length 74 >> stream
/Figure << /MCID 0 >> BDC 10 150 40 20 re f EMC /Figure << /MCID 1 >> BDC 100 100 50 30 re f EMC
endstream endobj
5 0 obj << /Type /StructTreeRoot /K 6 0 R /ParentTree 9 0 R >> endobj
6 0 obj << /S /Document /P 5 0 R /K [7 0 R 8 0 R] >> endobj
7 0 obj << /S /Figure /P 6 0 R /Pg 3 0 R /K 0 >> endobj
8 0 obj << /S /Figure /P 6 0 R /Pg 3 0 R /K 1 >> endobj
9 0 obj << /Nums [0 [7 0 R 8 0 R]] >> endobj
trailer << /Root 1 0 R >>
%%EOF"
            .as_slice(),
    )
    .unwrap();
    let fd = ok(&mut a, "doc_open", json!({ "path": "figures.pdf" }))["doc"].as_u64().unwrap();
    let tags = ok(&mut a, "accessibility_tags", json!({ "doc": fd }));
    assert_eq!(tags["count"], 3);
    assert_eq!(tags["tags"][1]["object"], 7);
    assert_eq!(tags["tags"][1]["type"], "Figure");
    assert_eq!(tags["tags"][1]["parent"], json!({ "object": 6, "generation": 0 }));
    assert_eq!(tags["tags"][1]["depth"], 1);
    let changed = ok(
        &mut a,
        "accessibility_set_tag_property",
        json!({
            "doc": fd, "object": 7, "generation": 0, "property": "title", "value": "First shape"
        }),
    );
    assert_eq!(changed["tags"][1]["title"], "First shape");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": fd }))["undone"], "Change tag properties");
    assert_eq!(ok(&mut a, "accessibility_tags", json!({ "doc": fd }))["tags"][1]["title"], Value::Null);
    ok(&mut a, "accessibility_set_tag_property", json!({ "doc": fd, "object": 8, "generation": 0, "property": "title", "value": "Second shape" }));
    let moved = ok(
        &mut a,
        "accessibility_move_tag",
        json!({ "doc": fd, "object": 8, "generation": 0, "anchor": 7, "anchor_generation": 0, "placement": "before" }),
    );
    assert_eq!((moved["tags"][1]["object"].as_u64(), moved["tags"][2]["object"].as_u64()), (Some(8), Some(7)));
    ok(&mut a, "doc_save", json!({ "doc": fd, "path": "reordered.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("reordered.pdf")).unwrap())).unwrap();
    assert_eq!(pdfcraft_engine::a11y::tags(&saved).into_iter().map(|tag| tag.title).collect::<Vec<_>>(), [None, Some("Second shape".into()), None]);
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": fd }))["undone"], "Move object");
    assert_eq!(ok(&mut a, "accessibility_tags", json!({ "doc": fd }))["tags"][1]["object"], 7);
    assert!(matches!(
        a.call(
            "accessibility_move_tag",
            &json!({ "doc": fd, "object": 8, "generation": 0, "anchor": 7, "anchor_generation": 0, "placement": "sideways" })
        ),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(
        a.call(
            "accessibility_set_tag_property",
            &json!({
                "doc": fd, "object": 7, "generation": 1, "property": "title", "value": "Wrong generation"
            })
        ),
        Err(ToolError::Failed(_))
    ));
    let findings = ok(&mut a, "accessibility_check", json!({ "doc": fd, "rules": ["figures-alt-text"] }));
    let figures_rule = findings["results"].as_array().unwrap().iter().find(|x| x["rule"] == "figures-alt-text").unwrap();
    assert_eq!(figures_rule["findings"][0]["object"], 7);
    assert_eq!(figures_rule["findings"][0]["object_generation"], 0);
    assert_eq!(figures_rule["findings"][0]["bbox_user_space"], json!([10.0, 150.0, 50.0, 170.0]));
    assert_eq!(figures_rule["findings"][1]["object"], 8);
    assert_eq!(figures_rule["findings"][1]["object_generation"], 0);
    assert_eq!(figures_rule["findings"][1]["bbox_user_space"], json!([100.0, 100.0, 150.0, 130.0]));
    let figs = ok(&mut a, "accessibility_figures", json!({ "doc": fd }));
    assert_eq!(figs["count"], 2);
    assert_eq!(figs["figures"][0]["rect"], json!([10.0, 30.0, 50.0, 50.0]), "top-left-origin points");
    let first = figs["figures"][0]["figure"].as_u64().unwrap();
    let second = figs["figures"][1]["figure"].as_u64().unwrap();
    let r = ok(&mut a, "accessibility_set_alt", json!({ "doc": fd, "figure": first, "alt": "A small bar" }));
    assert_eq!(r["figures"][0]["alt"], "A small bar");
    let r = ok(&mut a, "accessibility_set_alt", json!({ "doc": fd, "figure": second, "decorative": true }));
    assert_eq!(r["count"], 1);
    let check = ok(&mut a, "accessibility_check", json!({ "doc": fd, "rules": ["figures-alt-text", "tagged-content"] }));
    assert_eq!(check["failed"], 0, "{check}");
    assert!(matches!(a.call("accessibility_set_alt", &json!({ "doc": fd, "figure": 6, "alt": "x" })), Err(ToolError::Failed(_))));
}

/// A table's summary and a header cell's scope are set through `accessibility_set_tag_property`
/// (#4103, #4104): the summary clears the table-summary finding, both are listed with the tags,
/// undone, and saved.
#[test]
fn table_summary_and_header_scope_through_automation() {
    let dir = workdir("a11y-table");
    let content = "/TH << /MCID 0 >> BDC 10 150 40 20 re f EMC /TD << /MCID 1 >> BDC 60 150 40 20 re f EMC";
    let pdf = format!(
        "%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /StructParents 0 >> endobj
4 0 obj << /Length {} >> stream
{content}
endstream endobj
5 0 obj << /Type /StructTreeRoot /K 6 0 R /ParentTree 11 0 R >> endobj
6 0 obj << /S /Document /P 5 0 R /K [7 0 R] >> endobj
7 0 obj << /S /Table /P 6 0 R /K [8 0 R] >> endobj
8 0 obj << /S /TR /P 7 0 R /K [9 0 R 10 0 R] >> endobj
9 0 obj << /S /TH /P 8 0 R /Pg 3 0 R /K 0 >> endobj
10 0 obj << /S /TD /P 8 0 R /Pg 3 0 R /K 1 >> endobj
11 0 obj << /Nums [0 [9 0 R 10 0 R]] >> endobj
trailer << /Root 1 0 R >>
%%EOF",
        content.len()
    );
    std::fs::write(dir.join("table.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let fd = ok(&mut a, "doc_open", json!({ "path": "table.pdf" }))["doc"].as_u64().unwrap();
    let summary_status = |a: &mut Automation| {
        let r = ok(a, "accessibility_check", json!({ "doc": fd, "rules": ["table-summary"] }));
        r["results"].as_array().unwrap().iter().find(|x| x["rule"] == "table-summary").unwrap()["status"].as_str().unwrap().to_owned()
    };
    assert_eq!(summary_status(&mut a), "failed");
    let args =
        |object: u32, property: &str, value: &str| json!({ "doc": fd, "object": object, "generation": 0, "property": property, "value": value });
    let tags = ok(&mut a, "accessibility_set_tag_property", args(7, "summary", "Opening hours by day"));
    let tag = |tags: &Value, object: u64| tags["tags"].as_array().unwrap().iter().find(|t| t["object"] == object).unwrap().clone();
    assert_eq!(tag(&tags, 7)["summary"], "Opening hours by day");
    assert_eq!(summary_status(&mut a), "passed");
    let tags = ok(&mut a, "accessibility_set_tag_property", args(9, "scope", "Column"));
    assert_eq!(tag(&tags, 9)["scope"], "Column");
    // A scope is Row, Column or Both; only a table has a summary and only a header cell a scope.
    for (object, property, value) in [(9, "scope", "column"), (10, "scope", "Row"), (9, "summary", "No")] {
        assert!(
            matches!(err(&mut a, "accessibility_set_tag_property", args(object, property, value)), ToolError::Failed(_)),
            "{property} {value} on {object}"
        );
    }
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": fd }))["undone"], "Change tag properties");
    assert_eq!(tag(&ok(&mut a, "accessibility_tags", json!({ "doc": fd })), 9)["scope"], Value::Null);
    ok(&mut a, "edit_redo", json!({ "doc": fd }));
    ok(&mut a, "doc_save", json!({ "doc": fd, "path": "tagged-table.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("tagged-table.pdf")).unwrap())).unwrap();
    // Saving renumbers the objects: the tags are found by type.
    let saved = pdfcraft_engine::a11y::tags(&saved);
    assert_eq!(saved.iter().find(|t| t.kind == "Table").and_then(|t| t.summary.as_deref()), Some("Opening hours by day"));
    assert_eq!(saved.iter().find(|t| t.kind == "TH").and_then(|t| t.scope.as_deref()), Some("Column"));
}

#[test]
fn marking_a_leaf_tag_as_an_artifact_through_automation_is_undoable_and_saved() {
    let dir = workdir("a11y-artifact");
    std::fs::write(
        dir.join("tagged.pdf"),
        b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 5 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /StructParents 0 >> endobj
4 0 obj << /Length 48 >> stream
/Figure << /MCID 0 >> BDC 10 150 40 20 re f EMC
endstream endobj
5 0 obj << /Type /StructTreeRoot /K 6 0 R /ParentTree 8 0 R >> endobj
6 0 obj << /S /Document /P 5 0 R /K [7 0 R] >> endobj
7 0 obj << /S /Figure /P 6 0 R /Pg 3 0 R /K 0 >> endobj
8 0 obj << /Nums [0 [7 0 R]] >> endobj
trailer << /Root 1 0 R >>
%%EOF",
    )
    .unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "tagged.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "accessibility_tags", json!({ "doc": doc }))["count"], 2);
    let tagged = ok(&mut a, "accessibility_mark_tag_artifact", json!({ "doc": doc, "object": 7, "generation": 0 }));
    assert_eq!(tagged["count"], 1);
    assert_eq!(ok(&mut a, "accessibility_check", json!({ "doc": doc, "rules": ["tagged-content"] }))["failed"], 0);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "artifact.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("artifact.pdf")).unwrap())).unwrap();
    assert_eq!(pdfcraft_engine::a11y::tags(&saved).len(), 1);
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Mark content as artifact");
    assert_eq!(ok(&mut a, "accessibility_tags", json!({ "doc": doc }))["count"], 2);
    assert!(matches!(a.call("accessibility_mark_tag_artifact", &json!({ "doc": doc, "object": 6, "generation": 0 })), Err(ToolError::Failed(_))));
}

#[test]
fn accessibility_review_record_compares_two_versions_without_changing_them() {
    let dir = workdir("a11y-review-record");
    let source = std::fs::read(dir.join("a.pdf")).unwrap();
    let mut a = auto(&dir);
    let before = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let after = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "accessibility_fix", json!({ "doc": after, "rule": "primary-language", "value": "en-GB" }));

    let output = ok(&mut a, "accessibility_review_record", json!({ "before": before, "after": after, "path": "review.json" }));
    let file: Value = serde_json::from_slice(&std::fs::read(dir.join("review.json")).unwrap()).unwrap();
    assert_eq!(output["record"], file);
    assert_eq!(file["schema"], 1);
    assert_eq!(file["before"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(file["after"]["sha256"].as_str().unwrap().len(), 64);
    assert_ne!(file["before"]["sha256"], file["after"]["sha256"]);
    assert_eq!(file["changed"], json!(["primary-language"]));
    let language = file["rules"].as_array().unwrap().iter().find(|r| r["rule"] == "primary-language").unwrap();
    assert_eq!(language["before"]["status"], "failed");
    assert_eq!(language["after"]["status"], "passed");
    assert!(file["review_required"].as_array().unwrap().iter().any(|r| r == "tagged-pdf"));
    let html_output = ok(&mut a, "accessibility_review_html", json!({ "before": before, "after": after, "path": "review.html" }));
    assert_eq!(html_output["record"], file);
    let html = std::fs::read_to_string(dir.join("review.html")).unwrap();
    assert!(html.contains("<html lang=\"en\">") && html.contains("Accessibility checker outcomes"));
    assert!(html.contains("Primary language") && html.contains("<td>Changed</td>"));
    assert!(html.contains("Still requiring review") && html.contains("Tagged PDF"));
    assert!(html.contains(file["before"]["sha256"].as_str().unwrap()));
    assert!(html.contains("validate the final PDF independently"));
    assert_eq!(std::fs::read(dir.join("a.pdf")).unwrap(), source);
    assert!(matches!(
        a.call("accessibility_review_record", &json!({ "before": before, "after": before, "path": "same.json" })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(!dir.join("same.json").exists());
    assert!(matches!(
        a.call("accessibility_review_html", &json!({ "before": before, "after": before, "path": "same.html" })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(!dir.join("same.html").exists());
    assert!(a.call("accessibility_review_record", &json!({ "before": before, "after": after, "path": "../escape.json" })).is_err());
    assert!(a.call("accessibility_review_html", &json!({ "before": before, "after": after, "path": "../escape.html" })).is_err());
    assert!(!dir.parent().unwrap().join("escape.html").exists());
}

#[test]
fn accessibility_review_sidecar_tracks_human_decisions_for_exact_pdf_revision() {
    let dir = workdir("a11y-review-sidecar");
    let source = std::fs::read(dir.join("a.pdf")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let decisions = json!([
        { "rule": "logical-reading-order", "decision": "needs_work", "page": 1, "note": "Check heading order" },
        { "rule": "figures-alt-text", "decision": "accepted", "page": 2 }
    ]);
    let saved = ok(&mut a, "accessibility_review_save", json!({ "doc": doc, "path": "review.json", "decisions": decisions }));
    assert_eq!(saved["review"]["schema"], 1);
    assert_eq!(saved["review"]["sha256"].as_str().unwrap().len(), 64);
    let original_sidecar = std::fs::read(dir.join("review.json")).unwrap();
    assert_eq!(ok(&mut a, "accessibility_review_load", json!({ "doc": doc, "path": "review.json" }))["review"], saved["review"]);
    let mut reopened = auto(&dir);
    let reopened_doc = ok(&mut reopened, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut reopened, "accessibility_review_load", json!({ "doc": reopened_doc, "path": "review.json" }))["review"], saved["review"]);
    assert!(matches!(
        reopened.call("accessibility_review_save", &json!({ "doc": reopened_doc, "path": "a.pdf", "decisions": [], "replace": true })),
        Err(ToolError::Failed(_))
    ));
    assert_eq!(std::fs::read(dir.join("a.pdf")).unwrap(), source);

    assert!(matches!(
        a.call(
            "accessibility_review_save",
            &json!({ "doc": doc, "path": "review.json", "decisions": [
        { "rule": "title", "decision": "accepted" }, { "rule": "title", "decision": "skipped" }
    ] })
        ),
        Err(ToolError::InvalidArgs(_))
    ));
    assert_eq!(std::fs::read(dir.join("review.json")).unwrap(), original_sidecar);
    ok(&mut a, "accessibility_fix", json!({ "doc": doc, "rule": "primary-language", "value": "en-GB" }));
    assert!(matches!(a.call("accessibility_review_load", &json!({ "doc": doc, "path": "review.json" })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("accessibility_review_save", &json!({ "doc": doc, "path": "review.json", "decisions": [] })), Err(ToolError::Failed(_))));
    assert_eq!(std::fs::read(dir.join("review.json")).unwrap(), original_sidecar);
    let replacement = ok(&mut a, "accessibility_review_save", json!({ "doc": doc, "path": "review.json", "decisions": decisions, "replace": true }));
    assert_ne!(replacement["review"]["sha256"], saved["review"]["sha256"]);
    assert_eq!(ok(&mut a, "accessibility_review_load", json!({ "doc": doc, "path": "review.json" }))["review"], replacement["review"]);
    assert_eq!(std::fs::read(dir.join("a.pdf")).unwrap(), source);
    assert!(a.call("accessibility_review_save", &json!({ "doc": doc, "path": "../escape.json", "decisions": [] })).is_err());
    assert!(!dir.parent().unwrap().join("escape.json").exists());

    // Malformed or oversized files are not sidecars: loading fails cleanly and
    // saving leaves them alone, without reading more than the size cap.
    let oversized = [b"{\"schema\": 1, \"pad\": \"".as_slice(), &vec![b'x'; (1 << 20) + 16], b"\"}"].concat();
    for (name, bytes) in [("malformed.json", b"{ not json".to_vec()), ("binary.json", vec![0xff, 0x00, 0xfe]), ("oversized.json", oversized)] {
        std::fs::write(dir.join(name), &bytes).unwrap();
        assert!(matches!(a.call("accessibility_review_load", &json!({ "doc": doc, "path": name })), Err(ToolError::Failed(_))), "{name}");
        assert!(
            matches!(
                a.call("accessibility_review_save", &json!({ "doc": doc, "path": name, "decisions": [], "replace": true })),
                Err(ToolError::Failed(_))
            ),
            "{name}"
        );
        assert_eq!(std::fs::read(dir.join(name)).unwrap(), bytes, "{name}");
    }
}

#[test]
fn editing_existing_text_through_tools() {
    let dir = workdir("edit-text");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let lines = ok(&mut a, "text_lines", json!({ "doc": doc, "page": 2 }));
    assert_eq!((lines["count"].as_u64(), lines["lines"][0]["text"].as_str()), (Some(1), Some("Page 2")));
    let r = ok(&mut a, "text_edit", json!({ "doc": doc, "page": 2, "line": 1, "text": "Section two" }));
    assert_eq!(r["line"]["text"], "Section two");
    assert_eq!(page_text(&mut a, doc)[1], "Section two");
    assert!(matches!(a.call("text_edit", &json!({ "doc": doc, "page": 2, "line": 9, "text": "x" })), Err(ToolError::InvalidArgs(_))));
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Edit text");
    let paras = ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 3 }));
    assert_eq!(paras["paragraphs"][0]["text"], "Page 3");
    let r = ok(&mut a, "text_edit", json!({ "doc": doc, "page": 3, "paragraph": 1, "text": "Part three" }));
    assert_eq!(r["paragraph"]["text"], "Part three");
    assert!(matches!(a.call("text_edit", &json!({ "doc": doc, "page": 3, "paragraph": 4, "text": "x" })), Err(ToolError::InvalidArgs(_))));
    // Formatting only: font, size, colour, alignment.
    ok(
        &mut a,
        "text_edit",
        json!({ "doc": doc, "page": 3, "paragraph": 1, "font": "times", "bold": true, "size": 20, "color": "#cc0000", "align": "center" }),
    );
    let p = &ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 3 }))["paragraphs"][0];
    assert_eq!((p["text"].as_str(), p["font"].as_str(), p["size"].as_f64()), (Some("Part three"), Some("Times-Bold"), Some(20.0)));
    // Moved 15 pt right and 10 pt down (dy is up; the listing's rect is measured from the top).
    let rect = |p: &serde_json::Value| -> Vec<f64> { p["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() };
    let before = rect(p);
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 3, "paragraph": 1, "dx": 15, "dy": -10 }));
    let p = ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 3 }))["paragraphs"][0].clone();
    let after = rect(&p);
    assert!((after[0] - before[0] - 15.0).abs() < 0.5 && (after[1] - before[1] - 10.0).abs() < 0.5, "{before:?} → {after:?}");
    assert_eq!(p["text"], "Part three");
    // A narrow width rewraps "Part three" onto two lines.
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 3, "paragraph": 1, "width": 50 }));
    let p = &ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 3 }))["paragraphs"][0];
    assert_eq!(p["lines"].as_array().map(Vec::len), Some(2), "{p}");
}

/// Format text ▸ Superscript and Subscript without the GUI: a range, then back again over a space
/// (which has no superscript form), each one undo step; refusals change nothing.
#[test]
fn superscript_and_subscript_through_tools() {
    let dir = workdir("text-script");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let paragraph =
        |a: &mut Automation| ok(a, "text_paragraphs", json!({ "doc": doc, "page": 2 }))["paragraphs"][0]["text"].as_str().unwrap().to_string();
    let script = |script: &str, range: Value| json!({ "doc": doc, "page": 2, "paragraph": 1, "script": script, "range": range });
    // "Page 2": the 2 is byte 5.
    let r = ok(&mut a, "text_script", script("superscript", json!([5, 6])));
    assert_eq!((r["script"].as_str(), r["paragraph"]["text"].as_str()), (Some("superscript"), Some("Page ²")), "{r}");
    assert_eq!(page_text(&mut a, doc)[1], "Page ²");
    // " ²": the space has no superscript form, and the range still toggles back.
    ok(&mut a, "text_script", script("superscript", json!([4, 7])));
    assert_eq!(paragraph(&mut a), "Page 2");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Edit text");
    assert_eq!(paragraph(&mut a), "Page ²");
    // Refused, changing nothing: a range that splits ², one past the end, an empty one, a range
    // with nothing that has a subscript form, an unknown script.
    for (name, range) in [
        ("superscript", json!([5, 6])),
        ("superscript", json!([0, 99])),
        ("superscript", json!([3, 3])),
        ("subscript", json!([4, 5])),
        ("italic", json!([0, 1])),
    ] {
        assert!(matches!(a.call("text_script", &script(name, range.clone())), Err(ToolError::InvalidArgs(_))), "{name} {range}");
    }
    assert_eq!(paragraph(&mut a), "Page ²");
}

#[test]
fn styled_cyrillic_fallback_is_batched_through_tools_and_reopen() {
    if pdfcraft_fonts::document_japanese_font().is_none() {
        eprintln!("skipping: built without craft-fonts");
        return;
    }
    let dir = workdir("styled-fallback");
    std::fs::write(
        dir.join("mixed.pdf"),
        helvetica_page(
            "BT /F1 12 Tf 14.4 TL 72 700 Td (The quick brown fox jumps over the lazy dog) Tj T* (and keeps running through the long field.) Tj ET",
        ),
    )
    .unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "mixed.pdf"}))["doc"].as_u64().unwrap();
    let replacement = "The quick brown fox jumps over the lazy dog шалом\nand keeps running through the long field.";
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"text":replacement,"width":520}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"saved.pdf"}));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("saved.pdf")).unwrap())).unwrap();
    let page = pdfcraft_model::pages(&saved).swap_remove(0);
    let resources = saved.dict(page.dict.get(b"Resources").unwrap()).unwrap();
    let fonts = saved.dict(resources.get(b"Font").unwrap()).unwrap();
    let fallback_count = fonts.iter().filter(|(name, _)| name.starts_with(b"PCJp")).count();
    assert_eq!(fallback_count, 1, "one shared fallback font for the new word");
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert!(!page_text(&mut a, doc)[0].contains("шалом"));
    ok(&mut a, "edit_redo", json!({"doc":doc}));
    assert!(page_text(&mut a, doc)[0].contains("шалом"));
    let reopened = ok(&mut a, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    let lines = ok(&mut a, "text_lines", json!({"doc":reopened,"page":1}));
    let runs = lines["lines"].as_array().unwrap();
    assert!(runs.iter().any(|l| l["text"].as_str().is_some_and(|t| t.starts_with("The quick")) && l["font"] == "Helvetica"), "{lines}");
    assert_eq!(runs.iter().filter(|l| l["text"] == "шалом").count(), 1, "{lines}");
    assert!(page_text(&mut a, reopened)[0].contains("шалом"));
}

/// #2593: a paragraph whose own font cannot show a character is written from PdfCraft's fallback
/// faces. The Cyrillic the craft-fonts faces lack (Uzbek қ ғ ҳ, Kazakh ә ө ү ұ һ) comes from the
/// bundled Inter face, so the edit is not refused even without `CRAFT_FONTS_DIR`, and the words
/// survive a save and a reopen. (`crates/edit/src/tests.rs::uzbek_and_kazakh_cyrillic_use_the_bundled_latin_face`
/// pins which face drew them.)
#[test]
fn editing_uzbek_and_kazakh_cyrillic_through_tools() {
    let dir = workdir("cyrillic-fallback");
    let body = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (The quick brown fox jumps over the lazy dog) Tj T* (and keeps running through the long field.) Tj ET";
    std::fs::write(dir.join("cy.pdf"), win_ansi_paragraph_pdf(body)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "cy.pdf" }))["doc"].as_u64().unwrap();
    for word in ["қизил", "әже"] {
        let text = format!("The quick brown fox jumps over the lazy dog {word}\nand keeps running through the long field.");
        let edited = ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": text }));
        let paragraph = edited["paragraph"]["text"].as_str().unwrap();
        assert!(paragraph.starts_with("The quick brown fox") && paragraph.ends_with("long field."), "{word}: {edited}");
        assert!(paragraph.contains(word), "{word}: {edited}");
        assert!(page_text(&mut a, doc)[0].contains(word), "{word}: the edited page reads it back");
        assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Edit text");
    }
    // Keep the Kazakh edit, save it, and read it back from the file on disk.
    let text = "The quick brown fox jumps over the lazy dog әже\nand keeps running through the long field.";
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": text }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "cy-out.pdf" }));
    let saved = ok(&mut a, "doc_open", json!({ "path": "cy-out.pdf" }))["doc"].as_u64().unwrap();
    assert!(page_text(&mut a, saved)[0].contains("әже"), "the saved file reads it back");
}

#[test]
fn a_line_split_across_content_streams_is_listed_and_edited() {
    let dir = workdir("split-streams");
    std::fs::write(dir.join("split.pdf"), split_streams_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "split.pdf" }))["doc"].as_u64().unwrap();
    let lines = ok(&mut a, "text_lines", json!({ "doc": doc, "page": 1 }));
    let texts: Vec<&str> = lines["lines"].as_array().unwrap().iter().filter_map(|l| l["text"].as_str()).collect();
    assert_eq!(texts, ["Target", "Afterwards"], "{lines}");
    let r = ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "line": 2, "text": "Later" }));
    assert_eq!(r["line"]["text"], "Later");
    assert_eq!(page_text(&mut a, doc)[0].split_whitespace().collect::<Vec<_>>(), ["Target", "Later"]);
}

#[test]
fn paragraph_bold_without_font_keeps_the_source_family() {
    let dir = workdir("paragraph-bold");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 2, "paragraph": 1, "font": "times" }));
    let bold = ok(&mut a, "text_edit", json!({ "doc": doc, "page": 2, "paragraph": 1, "bold": true }));
    assert_eq!(bold["paragraph"]["font"], "Times-Bold");
    let regular = ok(&mut a, "text_edit", json!({ "doc": doc, "page": 2, "paragraph": 1, "bold": false }));
    assert_eq!(regular["paragraph"]["font"], "Times-Roman");
    assert_eq!(regular["paragraph"]["text"], "Page 2");
    assert!(matches!(a.call("text_edit", &json!({ "doc": doc, "page": 2, "paragraph": 1, "bold": "yes" })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn editing_page_images_through_tools() {
    let dir = workdir("page-images");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 18, "pages": [1, 2] }));
    let made = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/a_page_1.png"] }))["doc"].as_u64().unwrap();
    let list = ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }));
    assert_eq!(list["count"], 1);
    let r = ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "move", "rect": [5, 5, 25, 35] }));
    assert_eq!(r["undo"], "Move image");
    let moved = ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["images"][0]["rect"].clone();
    assert_eq!(moved, json!([5.0, 5.0, 25.0, 35.0]));
    // Crop keeps part of it in place: the placement stays, visible_rect says what shows.
    let r = ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "crop", "rect": [10, 10, 25, 30] }));
    assert_eq!(r["undo"], "Crop image");
    let cropped = ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["images"][0].clone();
    assert_eq!(cropped["rect"], json!([5.0, 5.0, 25.0, 35.0]));
    assert_eq!(cropped["visible_rect"], json!([10.0, 10.0, 25.0, 30.0]));
    ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "rotate" }));
    ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "flip_horizontal" }));
    // A single image is already in front and at the back; no-op arrange is refused without dirtying undo.
    assert!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "bring_to_front" })).is_err());
    assert!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "send_to_back" })).is_err());
    assert!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "bring_forward" })).is_err());
    assert!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "send_backward" })).is_err());
    let r = ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "remove_crop" }));
    assert_eq!(r["undo"], "Remove crop");
    // Any angle: the box grows to the turned picture's bounds, and it undoes like a quarter turn.
    let before = ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["images"][0]["rect"].clone();
    let r = ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "rotate_by", "degrees": 30.5 }));
    assert_eq!(r["undo"], "Rotate image");
    let turned = ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["images"][0]["rect"].clone();
    assert_ne!(turned, before);
    ok(&mut a, "edit_undo", json!({ "doc": made }));
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["images"][0]["rect"], before);
    assert!(matches!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "rotate_by" })), Err(ToolError::InvalidArgs(_))));
    let saved = ok(&mut a, "image_save", json!({ "doc": made, "page": 1, "image": 1, "path": "out/picture" }));
    assert_eq!(saved["format"], "png");
    assert!(std::fs::read(dir.join("out/picture.png")).unwrap().starts_with(b"\x89PNG"));
    ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "replace", "path": "src/a_page_2.png" }));
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["count"], 1);
    ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "delete" }));
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["count"], 0);
    assert!(matches!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "delete" })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn arranging_page_images_through_tools() {
    let dir = workdir("arrange-page-images");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 18, "pages": [1, 2] }));
    let made = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/a_page_1.png"] }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_image", json!({ "doc": made, "page": 1, "path": "src/a_page_2.png", "rect": [50, 50, 100, 100] }));
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": made, "page": 1 }))["count"], 2);
    let r = ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "bring_to_front" }));
    assert_eq!(r["undo"], "Bring to front");
    let r = ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 2, "action": "send_to_back" }));
    assert_eq!(r["undo"], "Send to back");
    ok(&mut a, "doc_save", json!({ "doc": made, "path": "arranged.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "arranged.pdf" }))["doc"].as_u64().unwrap();
    let after_reopen = ok(&mut a, "page_images", json!({ "doc": reopened, "page": 1 }));
    assert_eq!(after_reopen["count"], 2);
    assert_eq!(after_reopen["images"][0]["rect"], json!([0.0, 0.0, 50.0, 75.0]));
    assert_eq!(after_reopen["images"][1]["rect"], json!([50.0, 50.0, 100.0, 100.0]));
}

/// The transform a rotated-image resize sends (`ImageEdit::Transform`) is reachable headlessly,
/// in the same top-left-origin points as the other image tools, also on a turned page.
#[test]
fn transforming_page_images_through_tools() {
    let dir = workdir("transform-page-images");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 18, "pages": [1] }));
    let made = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/a_page_1.png"] }))["doc"].as_u64().unwrap();
    let rect = |a: &mut Automation| ok(a, "page_images", json!({ "doc": made, "page": 1 }))["images"][0]["rect"].clone();
    let transform =
        |a: &mut Automation, m: Value| ok(a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "transform", "matrix": m }));
    ok(&mut a, "image_edit", json!({ "doc": made, "page": 1, "image": 1, "action": "move", "rect": [5, 5, 25, 35] }));

    // Right 10, down 20.
    assert_eq!(transform(&mut a, json!([1, 0, 0, 1, 10, 20]))["undo"], "Transform image");
    assert_eq!(rect(&mut a), json!([15.0, 25.0, 35.0, 55.0]));
    // Twice the size about the top-left corner of the page.
    transform(&mut a, json!([2, 0, 0, 2, 0, 0]));
    assert_eq!(rect(&mut a), json!([30.0, 50.0, 70.0, 110.0]));
    // A quarter turn, (x, y) → (200 − y, x): the box turns with it, and it undoes in one step.
    transform(&mut a, json!([0, 1, -1, 0, 200, 0]));
    assert_eq!(rect(&mut a), json!([90.0, 30.0, 150.0, 70.0]));
    ok(&mut a, "edit_undo", json!({ "doc": made }));
    assert_eq!(rect(&mut a), json!([30.0, 50.0, 70.0, 110.0]));

    // On a turned page "right" is still right on screen.
    ok(&mut a, "page_rotate", json!({ "doc": made, "pages": [1], "degrees": 90 }));
    let numbers = |v: Value| [0, 1, 2, 3].map(|i| v[i].as_f64().unwrap());
    let [x0, y0, x1, y1] = numbers(rect(&mut a));
    transform(&mut a, json!([1, 0, 0, 1, 10, 0]));
    let after = numbers(rect(&mut a));
    for (got, want) in after.into_iter().zip([x0 + 10.0, y0, x1 + 10.0, y1]) {
        assert!((got - want).abs() < 0.015, "{after:?} is not ({x0}, {y0}, {x1}, {y1}) moved right by 10");
    }

    // Flat, non-finite, short and missing matrices are refused before anything changes.
    let before = rect(&mut a);
    for bad in [
        json!([1, 2, 2, 4, 0, 0]),
        json!([0, 0, 0, 0, 0, 0]),
        json!([1e300, 0, 0, 1e300, 0, 0]),
        json!([1, 0, 0, 1]),
        json!([1, 0, "invalid", 0, 1, 10, 0]),
        json!([1, 0, null, 0, 1, 10, 0]),
    ] {
        let r = a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "transform", "matrix": bad }));
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "{bad}: {r:?}");
    }
    assert!(matches!(a.call("image_edit", &json!({ "doc": made, "page": 1, "image": 1, "action": "transform" })), Err(ToolError::InvalidArgs(_))));
    assert_eq!(rect(&mut a), before);
}

#[test]
fn enhance_camera_image_through_tools() {
    let dir = workdir("enhance-camera");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 72, "pages": [1] }));
    let photo = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/a_page_1.png"] }))["doc"].as_u64().unwrap();
    // A 200 × 300 pt page at one pixel per point; the corners, in any order, keep a 160 × 240 part.
    let r = ok(&mut a, "page_enhance_camera", json!({ "doc": photo, "page": 1, "corners": [180, 270, 20, 30, 20, 270, 180, 30] }));
    assert_eq!(r["undo"], "Enhance camera image");
    assert_eq!(r["pages"], 1);
    let images = ok(&mut a, "page_images", json!({ "doc": photo, "page": 1 }));
    assert_eq!(images["count"], 1);
    assert_eq!(images["images"][0]["pixels"], json!([160, 240]));
    assert_eq!(images["images"][0]["rect"], json!([0.0, 0.0, 160.0, 240.0]));
    for bad in [json!([0, 0, 10, 10, 20, 20, 30, 30]), json!([0, 0, 10]), json!("corners")] {
        let e = a.call("page_enhance_camera", &json!({ "doc": photo, "page": 1, "corners": bad }));
        assert!(matches!(e, Err(ToolError::InvalidArgs(_) | ToolError::Failed(_))), "{bad}: {e:?}");
    }
    // Without corners on a page with no photo, the edges can't be found: an error that says so.
    let e = a.call("page_enhance_camera", &json!({ "doc": photo, "page": 1 }));
    assert!(matches!(&e, Err(ToolError::Failed(m)) if m.contains("corners")), "{e:?}");
    assert!(matches!(
        a.call("page_enhance_camera", &json!({ "doc": photo, "page": 9, "corners": [0, 0, 9, 0, 9, 9, 0, 9] })),
        Err(ToolError::InvalidArgs(_))
    ));
}

#[test]
fn enhance_scanned_file_cleans_the_scan_in_place_through_tools() {
    let dir = workdir("enhance-scan");
    // A 300 × 400 grey scan: dull paper, a dark gutter at the left, text bars and a speck.
    let img = image::GrayImage::from_fn(300, 400, |x, y| {
        let light = 0.55 + 0.45 * (x as f32 / 60.0).min(1.0);
        let ink = if ((60..360).contains(&y) && (y - 60) % 40 < 6 && (60..240).contains(&x)) || (x, y) == (150, 25) { 0.12 } else { 0.78 };
        image::Luma([(ink * light * 255.0) as u8])
    });
    img.save(dir.join("scan.jpg")).unwrap();
    let mut a = auto(&dir);
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["scan.jpg"] }))["doc"].as_u64().unwrap();
    ok(&mut a, "image_save", json!({ "doc": scan, "page": 1, "image": 1, "path": "before.jpg" }));

    let r = ok(&mut a, "page_enhance_scan", json!({ "doc": scan, "level": 100 }));
    assert_eq!(r["undo"], "Enhance scanned file");
    assert_eq!((r["pages_cleaned"].clone(), r["images"].clone(), r["not_scanned"].clone()), (json!([1]), json!(1), json!([])));
    assert_eq!(r["skipped"], json!([]));
    // In place: still one picture of the same size, still a JPEG, now with white paper.
    let images = ok(&mut a, "page_images", json!({ "doc": scan, "page": 1 }));
    assert_eq!((images["count"].clone(), images["images"][0]["pixels"].clone()), (json!(1), json!([300, 400])));
    let saved = ok(&mut a, "image_save", json!({ "doc": scan, "page": 1, "image": 1, "path": "after" }));
    assert_eq!(saved["format"], "jpg");
    let (before, after) = (image::open(dir.join("before.jpg")).unwrap().to_luma8(), image::open(dir.join("after.jpg")).unwrap().to_luma8());
    for (x, y) in [(150, 40), (3, 40), (3, 380), (150, 25)] {
        assert!(
            before.get_pixel(x, y)[0] < 200 && after.get_pixel(x, y)[0] >= 230,
            "({x}, {y}): {:?} → {:?}",
            before.get_pixel(x, y),
            after.get_pixel(x, y)
        );
    }
    assert!(after.get_pixel(150, 62)[0] <= 60, "a stroke stays dark");
    // Undo puts the scan back as it was.
    ok(&mut a, "edit_undo", json!({ "doc": scan }));
    ok(&mut a, "image_save", json!({ "doc": scan, "page": 1, "image": 1, "path": "undone.jpg" }));
    assert_eq!(std::fs::read(dir.join("undone.jpg")).unwrap(), std::fs::read(dir.join("before.jpg")).unwrap());

    // Switches: despeckle alone leaves everything but the speck.
    let r = ok(&mut a, "page_enhance_scan", json!({ "doc": scan, "pages": [1], "level": 0, "despeckle": true, "edge_shadows": false }));
    assert_eq!(r["images"], 1);
    // A document of text pages has no scans: nothing changes, and it says so.
    let text = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "page_enhance_scan", json!({ "doc": text }));
    assert_eq!((r["images"].clone(), r["not_scanned"].clone(), r["undo"].clone()), (json!(0), json!([1, 2, 3]), Value::Null));
    // Bad arguments are refused before anything changes.
    for bad in [
        json!({ "doc": scan, "level": 101 }),
        json!({ "doc": scan, "pages": [9] }),
        json!({ "doc": scan, "despeckle": "yes" }),
        json!({ "doc": scan, "pages": [] }),
    ] {
        assert!(matches!(a.call("page_enhance_scan", &bad), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
}

#[test]
fn enhance_camera_image_finds_the_edges_by_itself() {
    let dir = workdir("enhance-camera-auto");
    // A 600 × 450 photo: a white sheet, tilted, on a dark table.
    let sheet = [[90.0, 60.0], [500.0, 30.0], [525.0, 405.0], [60.0, 375.0]];
    let inside = |x: f64, y: f64| {
        (0..4).all(|i| {
            let (a, b): ([f64; 2], [f64; 2]) = (sheet[i], sheet[(i + 1) % 4]);
            (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]) >= 0.0
        })
    };
    let img = image::RgbImage::from_fn(600, 450, |x, y| {
        if inside(x as f64 + 0.5, y as f64 + 0.5) { image::Rgb([240, 238, 232]) } else { image::Rgb([70, 60, 50]) }
    });
    img.save(dir.join("photo.jpg")).unwrap();
    let mut a = auto(&dir);
    let photo = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["photo.jpg"] }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "page_enhance_camera", json!({ "doc": photo, "page": 1 }));
    assert_eq!(r["detected"], true);
    assert_eq!(r["undo"], "Enhance camera image");
    // A clean-up level outside 0–100 is refused before anything changes.
    let e = a.call("page_enhance_camera", &json!({ "doc": photo, "page": 1, "corners": [0, 0, 100, 0, 100, 100, 0, 100], "level": 101 }));
    assert!(matches!(&e, Err(ToolError::InvalidArgs(m)) if m.contains("level")), "{e:?}");
    ok(&mut a, "page_enhance_camera", json!({ "doc": photo, "page": 1, "corners": [0, 0, 100, 0, 100, 100, 0, 100], "level": 80 }));
    // One point per pixel: the corners it used are the sheet's.
    let used: Vec<f64> = r["corners"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    for (k, p) in sheet.iter().enumerate() {
        let (x, y) = (used[2 * k], used[2 * k + 1]);
        assert!((x - p[0]).hypot(y - p[1]) < 4.0, "{used:?} vs {sheet:?}");
    }
}

#[test]
fn editing_form_artwork_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_incremental};
    let dir = workdir("form-artwork");
    let mut cos = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&cos)[0].clone();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 80, 40].map(Object::Int).to_vec()));
    let artwork = cos.add(Object::Stream(Stream::flate(form, b"0.2 0.5 0.9 rg 0 0 80 40 re f")));
    let contents = cos.add(Object::Stream(Stream::flate(Dict::new(), b"q 1 0 0 1 20 240 cm /Figure Do Q")));
    cos.update_dict(page.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Ref(contents));
        let mut xo = Dict::new();
        xo.set(b"Figure".to_vec(), Object::Ref(artwork));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        d.set(b"Resources".to_vec(), Object::Dict(resources));
    })
    .unwrap();
    std::fs::write(dir.join("figure.pdf"), write_incremental(&cos, &SaveOptions::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "figure.pdf"}))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "page_images", json!({"doc": doc, "page": 1}))["images"][0].clone();
    assert_eq!(before["kind"], "form");
    assert_eq!(before["pixels"], json!([0, 0]));
    assert_eq!(before["rect"], json!([20.0, 20.0, 100.0, 60.0]));
    let rendered_before = a.call("page_render", &json!({"doc": doc, "page": 1, "dpi": 72})).unwrap();
    assert!(a.call("image_save", &json!({"doc": doc, "page": 1, "image": 1, "path": "out/figure"})).is_err());
    ok(&mut a, "image_edit", json!({"doc": doc, "page": 1, "image": 1, "action": "move", "rect": [40, 50, 120, 90]}));
    assert_eq!(ok(&mut a, "page_images", json!({"doc": doc, "page": 1}))["images"][0]["rect"], json!([40.0, 50.0, 120.0, 90.0]));
    let rendered_moved = a.call("page_render", &json!({"doc": doc, "page": 1, "dpi": 72})).unwrap();
    assert_ne!(rendered_before, rendered_moved, "the rendered artwork actually moves");
    ok(&mut a, "doc_save", json!({"doc": doc, "path": "out/moved.pdf"}));
    let re = ok(&mut a, "doc_open", json!({"path": "out/moved.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "page_images", json!({"doc": re, "page": 1}))["images"][0]["rect"], json!([40.0, 50.0, 120.0, 90.0]));
    ok(&mut a, "image_edit", json!({"doc": re, "page": 1, "image": 1, "action": "delete"}));
    assert_eq!(ok(&mut a, "page_images", json!({"doc": re, "page": 1}))["count"], 0);
    ok(&mut a, "edit_undo", json!({"doc": re}));
    assert_eq!(ok(&mut a, "page_images", json!({"doc": re, "page": 1}))["count"], 1);
    ok(&mut a, "edit_undo", json!({"doc": doc}));
    assert_eq!(ok(&mut a, "page_images", json!({"doc": doc, "page": 1}))["images"][0], before);
    assert_eq!(a.call("page_render", &json!({"doc": doc, "page": 1, "dpi": 72})).unwrap(), rendered_before);
}

#[test]
fn auditing_space_through_tools() {
    let dir = workdir("audit");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_audit_space", json!({ "doc": doc }));
    let rows = r["categories"].as_array().unwrap();
    let total: f64 = rows.iter().map(|x| x["percent"].as_f64().unwrap()).sum();
    assert!((total - 100.0).abs() < 0.1, "{r}");
    let content = rows.iter().find(|x| x["category"] == "Content Streams").unwrap();
    assert!(content["bytes"].as_u64().unwrap() > 0, "{r}");
}

#[test]
fn exporting_all_images_through_tools() {
    let dir = workdir("export-all-images");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "doc_export_all_images", json!({ "doc": doc, "folder": "none" }))["count"], 0, "text only");
    // A PDF made from two page renders (PNG, JPEG) holds two images.
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 36, "pages": [1] }));
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 36, "pages": [2], "format": "jpeg" }));
    let made = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/a_page_1.png", "src/a_page_2.jpg"] }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_export_all_images", json!({ "doc": made, "folder": "imgs" }));
    assert_eq!(r["count"], 2, "{r}");
    let files: Vec<&str> = r["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
    assert!(files[0].ends_with("_Page_1_Image_0001.png") && files[1].ends_with("_Page_2_Image_0002.jpg"), "{files:?}");
    assert_eq!(std::fs::read(files[1]).unwrap(), std::fs::read(dir.join("src/a_page_2.jpg")).unwrap(), "JPEG unchanged");
    assert!(std::fs::read(files[0]).unwrap().starts_with(b"\x89PNG"));
    assert_eq!(ok(&mut a, "doc_export_all_images", json!({ "doc": made, "folder": "imgs2", "pages": [2] }))["count"], 1);
    assert_eq!(ok(&mut a, "doc_export_all_images", json!({ "doc": made, "folder": "imgs3", "min_size": 10000 }))["count"], 0);
}

#[test]
fn fill_and_sign_through_tools() {
    let dir = workdir("fill");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 40], "text": "Ada Lovelace" }));
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "check", "at": [20, 80] }));
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 100] }));
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "signature", "at": [20, 150], "text": "Ada Lovelace" }));
    assert!(matches!(
        a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "initials", "at": [20, 190], "text": "   " })),
        Err(ToolError::InvalidArgs(_))
    ));
    let texts = page_text(&mut a, doc);
    assert!(texts[0].contains("Ada Lovelace") && texts[0].contains("11/14/2023"), "{texts:?}");
    // Preferences ▸ Date format, and a one-off pattern.
    let f = ok(&mut a, "fill_sign_date_format", json!({}));
    assert_eq!((f["format"].as_str(), f["today"].as_str()), (Some("m/d/yyyy"), Some("11/14/2023")), "{f}");
    assert_eq!(ok(&mut a, "fill_sign_date_format", json!({ "format": "yyyy.mm.dd." }))["today"], "2023.11.14.");
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 220] }));
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 240], "format": "d \\de mmmm" }));
    let texts = page_text(&mut a, doc);
    assert!(texts[0].contains("2023.11.14.") && texts[0].contains("14 de November"), "{texts:?}");
    assert!(matches!(a.call("fill_sign_date_format", &json!({ "format": "HH:MM" })), Err(ToolError::InvalidArgs(_))));
    assert_eq!(ok(&mut a, "fill_sign_date_format", json!({}))["format"], "yyyy.mm.dd.", "a rejected format keeps the previous one");
    // Month and weekday names in a chosen language, for the preference or one date.
    let f = ok(&mut a, "fill_sign_date_format", json!({ "format": "d. mmmm yyyy", "language": "cs" }));
    assert_eq!((f["language"].as_str(), f["today"].as_str()), (Some("cs"), Some("14. listopadu 2023")), "{f}");
    assert_eq!(f["languages"].as_array().unwrap().len(), pdfcraft_engine::dates::DATE_LANGUAGES.len(), "every date language");
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 280], "format": "d \\de mmmm", "language": "es" }));
    assert!(page_text(&mut a, doc)[0].contains("14 de noviembre"));
    assert!(matches!(a.call("fill_sign_date_format", &json!({ "format": "yyy" })), Err(ToolError::InvalidArgs(_))));
    // A date the PDF's text font can't hold is refused, not saved as "?".
    match a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 300], "format": "dddd", "language": "ja" })) {
        Err(ToolError::InvalidArgs(e)) => assert!(e.contains("火曜日") && e.contains("can't be written into the PDF yet"), "{e}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let f = ok(&mut a, "fill_sign_date_format", json!({ "format": "dddd", "language": "ja" }));
    assert_eq!((f["today"].as_str(), f["unwritable"].as_str()), (Some("火曜日"), Some("火曜日")));
    ok(&mut a, "fill_sign_date_format", json!({ "format": "d. mmmm yyyy", "language": "cs" }));
    assert!(matches!(a.call("fill_sign_date_format", &json!({ "format": "dd", "language": "xx" })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(
        a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 300], "language": "xx" })),
        Err(ToolError::InvalidArgs(_))
    ));
    let f = ok(&mut a, "fill_sign_date_format", json!({ "language": "auto" }));
    assert_eq!((f["format"].as_str(), f["language"].as_str(), f["today"].as_str()), (Some("d. mmmm yyyy"), Some("auto"), Some("14. November 2023")));
    assert!(matches!(
        a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "date", "at": [20, 260], "format": "Year" })),
        Err(ToolError::InvalidArgs(_))
    ));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 7);
    assert!(matches!(a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "text", "at": [1, 1] })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn fill_sign_text_font_size_is_saved_and_validated() {
    let dir = workdir("fill-sign-text-size");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 40], "text": "Big", "font_size": 24 }));
    let comments = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let rect = comments["comments"][0]["rect"].as_array().unwrap();
    let height = rect[3].as_f64().unwrap() - rect[1].as_f64().unwrap();
    assert!(height > 30.0, "24 pt text has a taller annotation: {comments}");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "sized.pdf" }));
    let saved = std::fs::read(dir.join("sized.pdf")).unwrap();
    let pdf = pdfcraft_cos::Document::open(std::sync::Arc::new(saved)).unwrap();
    let page = pdfcraft_model::pages(&pdf).remove(0);
    let annotations = pdf.resolve(page.dict.get(b"Annots").unwrap());
    let annotation = pdf.resolve(annotations.as_array().unwrap().first().unwrap());
    let da = annotation.as_dict().unwrap().get(b"DA").unwrap().as_string().unwrap();
    assert!(da.bytes.windows(b"/Helv 24 Tf".len()).any(|w| w == b"/Helv 24 Tf"), "saved annotation uses the requested font size");
    for size in [0, 3, 301] {
        assert!(matches!(
            a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 80], "text": "No", "font_size": size })),
            Err(ToolError::InvalidArgs(_))
        ));
    }
    assert!(matches!(
        a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "signature", "at": [20, 80], "text": "Ada", "font_size": 24 })),
        Err(ToolError::InvalidArgs(_))
    ));
}

#[test]
fn fill_sign_text_and_date_colours_are_saved_and_validated() {
    let dir = workdir("fill-sign-text-colour");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (kind, y) in [("text", 40), ("date", 80)] {
        ok(
            &mut a,
            "fill_sign_add",
            json!({ "doc": doc, "page": 1, "type": kind, "at": [20, y], "text": "Blue", "color": "#3366CC", "font_size": 6.5 }),
        );
    }
    for (kind, color) in [("text", "not-a-colour"), ("signature", "#3366CC"), ("check", "#3366CC")] {
        assert!(matches!(
            a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": kind, "at": [20, 100], "text": "No", "color": color })),
            Err(ToolError::InvalidArgs(_))
        ));
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "coloured.pdf" }));
    let pdf = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("coloured.pdf")).unwrap())).unwrap();
    let page = pdfcraft_model::pages(&pdf).remove(0);
    let annotations = pdf.resolve(page.dict.get(b"Annots").unwrap());
    let annotations = annotations.as_array().unwrap();
    assert_eq!(annotations.len(), 2, "invalid requests add nothing");
    for reference in annotations {
        let annotation = pdf.resolve(reference);
        // FreeText stores its text colour in DA; C is the annotation background.
        let da = annotation.as_dict().unwrap().get(b"DA").unwrap().as_string().unwrap();
        assert_eq!(String::from_utf8(da.bytes.to_vec()).unwrap(), "0.2 0.4 0.8 rg /Helv 6.5 Tf");
    }
}

#[test]
fn fill_sign_text_border_is_saved_and_validated() {
    let dir = workdir("fill-sign-text-border");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 40], "text": "Plain" }));
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 80], "text": "Plain", "border": 1.5 }));
    let comments = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let width = |k: usize| {
        let rect = comments["comments"][k]["rect"].as_array().unwrap();
        rect[2].as_f64().unwrap() - rect[0].as_f64().unwrap()
    };
    assert!((width(1) - width(0) - 3.0).abs() < 1e-6, "1.5 pt on each side: {comments}");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "framed.pdf" }));
    let saved = std::fs::read(dir.join("framed.pdf")).unwrap();
    let pdf = pdfcraft_cos::Document::open(std::sync::Arc::new(saved)).unwrap();
    let page = pdfcraft_model::pages(&pdf).remove(0);
    let annotations = pdf.resolve(page.dict.get(b"Annots").unwrap());
    let annotation = pdf.resolve(annotations.as_array().unwrap().get(1).unwrap());
    let bs = annotation.as_dict().unwrap().get(b"BS").unwrap().as_dict().unwrap().get(b"W").unwrap().as_f64().unwrap();
    assert_eq!(bs, 1.5, "saved annotation has the requested border");
    // Its own colour, then the text's again through comment_edit.
    ok(
        &mut a,
        "fill_sign_add",
        json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 160], "text": "Red", "border": 1, "border_color": "#ff0000" }),
    );
    let list = ok(&mut a, "comment_list", json!({ "doc": doc, "page": 1 }));
    let red = list["comments"].as_array().unwrap().iter().find(|c| c["contents"] == "Red").unwrap()["index"].clone();
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": red, "border_color": "text" }));
    assert!(
        a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 200], "text": "No", "border_color": "red" })).is_err(),
        "no border"
    );
    for (kind, border) in [("text", -1.0), ("text", 13.0), ("check", 1.0)] {
        assert!(matches!(
            a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": kind, "at": [20, 120], "text": "No", "border": border })),
            Err(ToolError::InvalidArgs(_))
        ));
    }
}

#[test]
fn image_signatures_through_tools_preserve_transparency_and_survive_save() {
    let dir = workdir("image-signatures");
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 120, 40);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let rgba: Vec<u8> = (0..40)
            .flat_map(|y| (0..120).flat_map(move |x| if (40..80).contains(&x) && (10..30).contains(&y) { [0, 0, 0, 255] } else { [0, 0, 0, 0] }))
            .collect();
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
    std::fs::write(dir.join("signature.png"), png).unwrap();
    std::fs::write(dir.join("broken.png"), b"broken").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300 }))["doc"].as_u64().unwrap();
    for (kind, y) in [("signature", 60), ("initials", 120)] {
        ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": kind, "at": [20, y], "path": "signature.png" }));
    }
    let render = |a: &mut Automation, doc| {
        let output = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
        let Content::Png { data, .. } = &output[0] else { panic!() };
        image::load_from_memory(data).unwrap().to_rgba8()
    };
    let before = render(&mut a, doc);
    assert_eq!(before.get_pixel(25, 60).0, [255, 255, 255, 255], "transparent margin exposes the page");
    assert_eq!(before.get_pixel(65, 60).0, [0, 0, 0, 255], "signature ink is embedded");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Add initials");
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 1);
    ok(&mut a, "edit_redo", json!({ "doc": doc }));
    // The same resize edit the selection handles use must keep the imported appearance.
    for rect in [json!([20, 40, 20, 88]), json!([20, 40, 100000000, 88])] {
        assert!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 1, "rect": rect })).is_err());
    }
    ok(&mut a, "comment_lock", json!({ "doc": doc, "page": 1, "index": 1 }));
    assert!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 1, "rect": [20, 40, 164, 88] })).is_err());
    ok(&mut a, "comment_lock", json!({ "doc": doc, "page": 1, "index": 1, "locked": false }));
    assert_eq!(render(&mut a, doc), before, "invalid and locked resizes leave the image intact");
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": 1, "rect": [20, 40, 164, 88] }));
    let resized = render(&mut a, doc);
    assert_eq!(resized.get_pixel(110, 64).0, [0, 0, 0, 255], "resizing scales the original ink");
    assert_eq!(resized.get_pixel(25, 64).0, [255, 255, 255, 255], "resizing retains alpha");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Resize comment");
    assert_eq!(render(&mut a, doc), before);
    ok(&mut a, "edit_redo", json!({ "doc": doc }));
    assert_eq!(render(&mut a, doc), resized);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "signed.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "signed.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": reopened }))["count"], 2);
    assert_eq!(render(&mut a, reopened), resized, "resized appearances retain alpha and geometry after save");
    for args in [
        json!({ "type": "signature", "path": "broken.png" }),
        json!({ "type": "signature", "path": "signature.png", "text": "Ada" }),
        json!({ "type": "check", "path": "signature.png" }),
        json!({ "type": "signature", "path": "../outside.png" }),
    ] {
        let mut args = args;
        args["doc"] = json!(doc);
        args["page"] = json!(1);
        args["at"] = json!([20, 80]);
        assert!(a.call("fill_sign_add", &args).is_err(), "{args}");
    }
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 2, "errors leave the PDF intact");
}

#[test]
fn scanned_signatures_through_tools_drop_the_paper_and_report_bad_arguments() {
    let dir = workdir("scanned-signatures");
    // A grey, unevenly lit sheet with a pen stroke for the signature, and a blank one.
    let photo = |ink: bool| {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, 400, 300);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let rgb: Vec<u8> = (0..300_u32)
            .flat_map(|y| {
                (0..400_u32).flat_map(move |x| {
                    let paper = (215 - x / 20) as u8;
                    if ink && (100..300).contains(&x) && (144..148).contains(&y) { [20, 20, 25] } else { [paper, paper, paper] }
                })
            })
            .collect();
        encoder.write_header().unwrap().write_image_data(&rgb).unwrap();
        png
    };
    std::fs::write(dir.join("photo.png"), photo(true)).unwrap();
    std::fs::write(dir.join("blank.png"), photo(false)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300 }))["doc"].as_u64().unwrap();
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "signature", "at": [20, 60], "path": "photo.png", "scan": true }));
    let output = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    let Content::Png { data, .. } = &output[0] else { panic!() };
    let page = image::load_from_memory(data).unwrap().to_rgba8();
    let dark = (20..170).filter(|&x| page.get_pixel(x, 60).0[0] < 80).count();
    assert!(dark > 100, "the stroke is placed as ink across the signature width ({dark} px)");
    for (x, y) in [(22, 50), (100, 50), (100, 70)] {
        assert_eq!(page.get_pixel(x, y).0, [255, 255, 255, 255], "the grey paper is gone at {x},{y}");
    }
    ok(
        &mut a,
        "fill_sign_add",
        json!({ "doc": doc, "page": 1, "type": "initials", "at": [20, 150], "path": "photo.png", "scan": true, "sensitivity": 0 }),
    );
    for args in [
        json!({ "type": "signature", "path": "blank.png", "scan": true }),
        json!({ "type": "signature", "path": "photo.png", "scan": true, "sensitivity": 101 }),
        json!({ "type": "signature", "path": "photo.png", "scan": true, "sensitivity": -1 }),
        json!({ "type": "signature", "path": "photo.png", "sensitivity": 50 }),
        json!({ "type": "signature", "text": "Ada", "scan": true }),
        json!({ "type": "signature", "path": "photo.png", "scan": "yes" }),
    ] {
        let mut args = args;
        args["doc"] = json!(doc);
        args["page"] = json!(1);
        args["at"] = json!([20, 80]);
        let error = a.call("fill_sign_add", &args).unwrap_err();
        assert!(matches!(error, ToolError::InvalidArgs(_)), "{args}: {error:?}");
    }
    let Err(ToolError::InvalidArgs(no_ink)) =
        a.call("fill_sign_add", &json!({ "doc": doc, "page": 1, "type": "signature", "at": [20, 80], "path": "blank.png", "scan": true }))
    else {
        panic!()
    };
    assert!(no_ink.contains("No signature was found"), "{no_ink}");
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 2, "errors leave the PDF intact");
}

#[test]
fn image_signature_preview_layers_are_read_only_and_survive_encrypted_save() {
    let dir = workdir("signature-preview");
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 120, 40);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let rgba: Vec<u8> = (0..40)
            .flat_map(|y| (0..120).flat_map(move |x| if (40..80).contains(&x) && (10..30).contains(&y) { [20, 40, 60, 128] } else { [0, 0, 0, 0] }))
            .collect();
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
    std::fs::write(dir.join("signature.png"), png).unwrap();
    std::fs::write(dir.join("form.pdf"), fixture(1)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [1], "degrees": 90 }));
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "initials", "at": [20, 40], "path": "signature.png" }));
    let background = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    let Content::Png { data: expected, .. } = &background[0] else { panic!() };
    let expected = image::load_from_memory(expected).unwrap().to_rgba8();
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "signature", "at": [120, 100], "path": "signature.png" }));
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": 2, "opacity": 0.5 }));
    ok(&mut a, "doc_protect", json!({ "doc": doc, "open_password": "preview-test" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "signed.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "signed.pdf", "password": "preview-test" }))["doc"].as_u64().unwrap();
    for doc in [doc, reopened] {
        let before = ok(&mut a, "doc_info", json!({ "doc": doc }));
        let d = a.session().docs().iter().find(|d| d.id.0 == doc).unwrap();
        let generation = d.edit_generation();
        let bytes = d.bytes.clone();
        let output = a.call("comment_image_preview", &json!({ "doc": doc, "page": 1, "index": 2, "dpi": 72 })).unwrap();
        let [Content::Json(meta), Content::Png { data: background, .. }] = output.as_slice() else { panic!("labeled background layer") };
        let image = a.call("comment_image_preview", &json!({ "doc": doc, "page": 1, "index": 2, "layer": "image" })).unwrap();
        let [Content::Json(image_meta), Content::Png { data: signature, width, height }] = image.as_slice() else { panic!("labeled image layer") };
        assert_eq!(meta["layer"], "background");
        assert_eq!(image_meta["layer"], "image");
        assert_eq!(meta["rotation"], 90);
        assert_eq!(meta["image_rotation"], 0, "placed upright as displayed on the turned page");
        assert_eq!(meta["opacity"], 0.5);
        assert_eq!((*width, *height), (120, 40));
        assert_eq!(image::load_from_memory(background).unwrap().to_rgba8(), expected, "page text and the other signature remain");
        let signature = image::load_from_memory(signature).unwrap().to_rgba8();
        assert_eq!(signature.get_pixel(5, 20).0[3], 0);
        assert_eq!(signature.get_pixel(60, 20).0, [20, 40, 60, 128], "embedded alpha and colour survive reopen");
        assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc })), before);
        let d = a.session().docs().iter().find(|d| d.id.0 == doc).unwrap();
        assert_eq!(d.edit_generation(), generation);
        assert!(std::sync::Arc::ptr_eq(&bytes, &d.bytes));
        assert!(d.image_signature_preview(0, 1).unwrap().unwrap().render_background(f32::NAN).is_err());
        assert!(d.image_signature_preview(0, 999).is_err());
    }
    assert!(tools().iter().find(|t| t.name == "comment_image_preview").unwrap().read_only);
    for dpi in [0, 601] {
        assert!(matches!(
            a.call("comment_image_preview", &json!({ "doc": reopened, "page": 1, "index": 2, "dpi": dpi })),
            Err(ToolError::InvalidArgs(_))
        ));
    }
    ok(&mut a, "fill_sign_add", json!({ "doc": reopened, "page": 1, "type": "check", "at": [10, 10] }));
    assert!(a.call("comment_image_preview", &json!({ "doc": reopened, "page": 1, "index": 3 })).is_err());
}

#[test]
fn creating_and_reducing_through_tools() {
    let dir = workdir("create");
    std::fs::write(dir.join("notes.txt"), "Meeting notes\nAction items").unwrap();
    let mut a = auto(&dir);
    let blank = ok(&mut a, "doc_create", json!({ "from": "blank", "pages": 2 }));
    assert_eq!((blank["pages"].as_u64(), blank["dirty"].as_bool()), (Some(2), Some(true)));
    let t = ok(&mut a, "doc_create", json!({ "from": "text", "path": "notes.txt" }))["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, t), ["Meeting notes\nAction items"]);
    ok(&mut a, "doc_save", json!({ "doc": t, "path": "notes.pdf" }));
    // A new text PDF is already compact: Reduce writes nothing rather than a copy no smaller.
    let r = ok(&mut a, "doc_reduce", json!({ "doc": t, "path": "notes-small.pdf" }));
    assert!(r["bytes_after"].as_u64().unwrap() >= r["bytes_before"].as_u64().unwrap(), "{r}");
    assert_eq!(r["written"], false);
    assert!(!dir.join("notes-small.pdf").exists());
    assert!(matches!(a.call("doc_create", &json!({ "from": "images", "paths": ["notes.txt"] })), Err(ToolError::Failed(_))));
}

#[test]
fn creating_from_text_refuses_what_the_font_cannot_draw() {
    let dir = workdir("create-undrawable");
    std::fs::write(dir.join("kr.txt"), "ABC 제안서 시험").unwrap();
    let mut a = auto(&dir);
    let refused = |r: Result<Vec<Content>, ToolError>| match r {
        Err(ToolError::InvalidArgs(m) | ToolError::Failed(m)) => assert!(m.contains("U+C81C"), "{m}"),
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert!(matches!(a.call("doc_create", &json!({ "from": "text", "text": "ABC 제안서 시험" })), Err(ToolError::InvalidArgs(_))));
    refused(a.call("doc_create", &json!({ "from": "text", "text": "ABC 제안서 시험" })));
    refused(a.call("doc_create", &json!({ "from": "text", "path": "kr.txt" })));
    // WinAnsi characters, line breaks and tabs are still fine.
    let d = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Price: 5 € at café\r\n\tnext\u{c}page two" }))["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, d).len(), 2);
    // #959: page_add_text refuses it too.
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank" }))["doc"].as_u64().unwrap();
    refused(a.call("page_add_text", &json!({ "doc": doc, "page": 1, "text": "ABC 제안서 시험", "at": [20, 20] })));
}

#[test]
fn creating_from_multiple_files_through_tools() {
    let dir = workdir("create-multiple");
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 4, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&[100; 4 * 2 * 3]).unwrap();
    }
    std::fs::write(dir.join("scan.png"), png).unwrap();
    std::fs::write(dir.join("notes.txt"), "hello").unwrap();
    std::fs::write(dir.join("report.docx"), b"PK\x03\x04").unwrap();
    let mut a = auto(&dir);

    // Combine: every file converted, in the order given, with a bookmark per file.
    let made = ok(
        &mut a,
        "doc_create_multiple",
        json!({ "paths": ["notes.txt", "a.pdf", "scan.png"], "pages": [null, "3", null], "out": "all.pdf", "open": true }),
    );
    let doc = made["document"]["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, doc), ["hello", "Page 3", ""]);
    let titles: Vec<String> = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["title"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(titles, ["notes", "a", "scan"]);
    assert!(dir.join("all.pdf").is_file());
    // A file that can't be converted fails the whole combine, naming it.
    let err = a.call("doc_create_multiple", &json!({ "paths": ["a.pdf", "report.docx"] })).unwrap_err();
    assert!(matches!(&err, ToolError::Failed(m) if m.contains("report.docx") && m.contains("can't be converted")), "{err}");
    assert!(matches!(a.call("doc_create_multiple", &json!({ "paths": [] })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("doc_create_multiple", &json!({ "paths": ["a.pdf"], "mode": "zip" })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("doc_create_multiple", &json!({ "paths": ["a.pdf"], "mode": "separate" })), Err(ToolError::InvalidArgs(_))));

    // Separate: one PDF per file; PDFs are skipped, a bad file doesn't stop the rest, and an
    // existing file is never overwritten.
    std::fs::create_dir_all(dir.join("out")).unwrap();
    std::fs::write(dir.join("out/notes.pdf"), b"mine").unwrap();
    let args = json!({ "paths": ["notes.txt", "report.docx", "a.pdf", "scan.png", "missing.txt"], "mode": "separate", "out_dir": "out" });
    let made = ok(&mut a, "doc_create_multiple", args);
    let files = made["files"].as_array().unwrap();
    assert!(files[0]["output"].as_str().unwrap().ends_with("notes (2).pdf"), "{files:?}");
    assert!(files[1]["error"].as_str().unwrap().contains("can't be converted"));
    assert_eq!(files[2]["skipped"], "already a PDF");
    assert!(files[3]["output"].as_str().unwrap().ends_with("scan.pdf"));
    assert!(files[4]["error"].is_string());
    assert_eq!(std::fs::read(dir.join("out/notes.pdf")).unwrap(), b"mine");
    let reopened = ok(&mut a, "doc_open", json!({ "path": "out/notes (2).pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, reopened), ["hello"]);
    assert!(!dir.join("out/a.pdf").exists());
}
