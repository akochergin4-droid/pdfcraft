//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 7 of 8).

#[allow(unused_imports)]
use super::*;

#[cfg(feature = "mcp")]
#[test]
fn conventions_annotations_and_strict_keys_in_both_modes() {
    for compact in [false, true] {
        let mut s = McpServer::new(Automation::new()).with_compact(compact);
        let r = rpc(&mut s, 1, "tools/list", json!({}));
        let tools = r["result"]["tools"].as_array().unwrap();
        for name in ["command_list", "command_run", "command_batch", "doc_inspect", "render_preview"] {
            assert!(tools.iter().any(|t| t["name"] == name), "missing {name}");
        }
        for tool in tools {
            for hint in ["readOnlyHint", "destructiveHint", "idempotentHint", "openWorldHint"] {
                assert!(tool["annotations"][hint].is_boolean(), "{}: {hint}", tool["name"]);
            }
            let r = rpc(&mut s, 2, "tools/call", json!({"name":tool["name"],"arguments":{"bogus_arg":1}}));
            assert_eq!(r["error"]["code"], -32602, "{}", tool["name"]);
            let message = r["error"]["message"].as_str().unwrap();
            assert!(message.contains("bogus_arg") && message.contains("expected:"));
        }
    }
}

#[cfg(feature = "mcp")]
#[test]
fn conventions_resources() {
    let dir = workdir("conventions-resources");
    let mut s = McpServer::new(auto(&dir));
    let read = |s: &mut McpServer, uri: &str| {
        let r = rpc(s, 1, "resources/read", json!({"uri":uri}));
        serde_json::from_str::<Value>(r["result"]["contents"][0]["text"].as_str().expect("JSON resource")).unwrap()
    };
    assert_eq!(read(&mut s, "pdfcraft://document"), json!({"documents":[]}));
    rpc(&mut s, 2, "tools/call", json!({"name":"doc_open","arguments":{"path":"a.pdf"}}));
    let doc = rpc(&mut s, 3, "tools/call", json!({"name":"doc_inspect","arguments":{}}));
    assert_eq!(read(&mut s, "pdfcraft://document"), doc["result"]["structuredContent"]);
    let commands = rpc(&mut s, 4, "tools/call", json!({"name":"command_list","arguments":{}}));
    assert_eq!(read(&mut s, "pdfcraft://commands"), commands["result"]["structuredContent"]);
    // Responses keep the shape every supported protocol revision expects: no cache hints.
    for (method, params) in [("tools/list", json!({})), ("resources/list", json!({})), ("resources/read", json!({"uri":"pdfcraft://document"}))] {
        assert!(rpc(&mut s, 5, method, params)["result"].get("resultType").is_none());
    }
    assert_eq!(rpc(&mut s, 7, "initialize", json!({"protocolVersion":"2026-07-28"}))["result"]["protocolVersion"], "2025-06-18");
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":999}}"#).is_none());
    assert_eq!(
        rpc(&mut s, 9, "tools/call", json!({"name":"doc_inspect","arguments":{},"_meta":{"progressToken":"quick"}}))["result"]["isError"],
        false
    );
}

#[test]
fn command_batch_refuses_more_than_a_thousand_steps() {
    let dir = workdir("batch-cap");
    let mut a = auto(&dir);
    let steps: Vec<Value> = (0..1001).map(|_| json!({"id":"no.such.command"})).collect();
    let err = a.call("command_batch", &json!({"steps": steps})).unwrap_err();
    assert!(err.to_string().contains("at most 1000 steps"), "{err}");
}

#[test]
fn command_batch_tells_a_tool_name_from_an_unknown_or_toolless_command() {
    let dir = workdir("batch-ids");
    let mut a = auto(&dir);
    let steps = json!([{"id":"page_rotate"}, {"id":"doc_inspect"}, {"id":"no.such.command"}, {"id":"help.about"}]);
    let r = ok(&mut a, "command_batch", json!({"steps": steps, "stop_on_error": false}));
    let errors: Vec<&str> = r["results"].as_array().unwrap().iter().map(|s| s["error"].as_str().unwrap()).collect();
    assert_eq!(
        errors,
        [
            r#""page_rotate" is a tool, not a command id; use the command id "page.rotate", or call the tool directly"#,
            r#""doc_inspect" is a tool, not a command id; call it directly (command ids are in command_list)"#,
            r#"unknown command id "no.such.command"; see command_list"#,
            r#"command "help.about" has no headless tool; see command_list"#,
        ]
    );
}

#[test]
fn thai_text_through_tools_saves_reopens_extracts_and_renders() {
    let dir = workdir("thai-content");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "a.pdf"}))["doc"].as_u64().unwrap();
    let text = "สวัสดี น้ำ ผู้ใช้ ปู่ ๑๒๓ Hello 123";
    let args = json!({"doc": doc, "page": 1, "text": text, "at": [10, 20], "size": 9});
    if pdfcraft_fonts::document_thai_font().is_none() {
        assert!(a.call("page_add_text", &args).unwrap_err().to_string().contains("Thai"));
        assert_eq!(ok(&mut a, "content_list", json!({"doc": doc}))["count"], 0);
        return;
    }
    ok(&mut a, "page_add_text", args);
    assert!(page_text(&mut a, doc)[0].contains(text), "{:?}", page_text(&mut a, doc));
    ok(&mut a, "doc_save", json!({"doc": doc, "path": "thai.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path": "thai.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "content_list", json!({"doc": reopened}))["items"][0]["text"], text);
    assert!(page_text(&mut a, reopened)[0].contains(text));
    let rendered = a.call("page_render", &json!({"doc": reopened, "page": 1, "dpi": 144})).unwrap();
    assert!(rendered.iter().any(|c| matches!(c, Content::Png { data, width, height } if !data.is_empty() && *width > 0 && *height > 0)));
    ok(&mut a, "content_update", json!({"doc": reopened, "page": 1, "index": 1, "text": "แก้ไข น้ำ"}));
    assert!(page_text(&mut a, reopened)[0].contains("แก้ไข น้ำ"));
}

#[test]
fn hebrew_text_through_tools_saves_reopens_extracts_and_renders() {
    let dir = workdir("hebrew-content");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "a.pdf"}))["doc"].as_u64().unwrap();
    let text = "שם משפחה: שרון, טל׳ 052-1234567 (נייד)";
    let args = json!({"doc": doc, "page": 1, "text": text, "rect": [10, 20, 190, 40], "size": 7, "align": "right"});
    if pdfcraft_fonts::document_hebrew_font(false).is_none() {
        assert!(a.call("page_add_text", &args).unwrap_err().to_string().contains("U+05E9"));
        assert_eq!(ok(&mut a, "content_list", json!({"doc": doc}))["count"], 0);
        return;
    }
    ok(&mut a, "page_add_text", args);
    // Extraction gives back the typing order, not the drawing order.
    assert!(page_text(&mut a, doc)[0].contains(text), "{:?}", page_text(&mut a, doc));
    ok(&mut a, "doc_save", json!({"doc": doc, "path": "hebrew.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path": "hebrew.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "content_list", json!({"doc": reopened}))["items"][0]["text"], text);
    assert!(page_text(&mut a, reopened)[0].contains(text));
    let found = ok(&mut a, "text_find", json!({"doc": reopened, "query": "שרון"}));
    assert_eq!(found["matches"].as_array().map(Vec::len), Some(1), "{found}");
    let rendered = a.call("page_render", &json!({"doc": reopened, "page": 1, "dpi": 144})).unwrap();
    assert!(rendered.iter().any(|c| matches!(c, Content::Png { data, width, height } if !data.is_empty() && *width > 0 && *height > 0)));
    ok(&mut a, "content_update", json!({"doc": reopened, "page": 1, "index": 1, "text": "מיכאל שָׁלוֹם"}));
    assert!(page_text(&mut a, reopened)[0].contains("מיכאל"));
}

#[test]
fn mixed_objects_move_through_tools_with_generation_and_one_undo() {
    let dir = workdir("mixed-object-move");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({"doc":doc,"page":1,"text":"Added label","rect":[20,20,130,45]}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"before.pdf"}));
    let inventory = ok(&mut a, "object_list", json!({"doc":doc,"page":1}));
    assert_eq!(inventory["count"], 2);
    let objects: Vec<_> = inventory["objects"].as_array().unwrap().iter().map(|o| json!({"kind":o["kind"],"index":o["index"]})).collect();
    let moved = ok(&mut a, "object_move", json!({"doc":doc,"page":1,"generation":inventory["generation"],"objects":objects,"offset":[18,27]}));
    assert_eq!(moved["moved"], 2);
    let after = ok(&mut a, "object_list", json!({"doc":doc,"page":1}));
    for (old, new) in inventory["objects"].as_array().unwrap().iter().zip(after["objects"].as_array().unwrap()) {
        for i in 0..4 {
            assert!((new["rect"][i].as_f64().unwrap() - old["rect"][i].as_f64().unwrap() - if i % 2 == 0 { 18.0 } else { 27.0 }).abs() < 0.02);
        }
        assert_eq!(old["text"], new["text"]);
    }
    assert!(a.call("object_move", &json!({"doc":doc,"page":1,"generation":inventory["generation"],"objects":objects,"offset":[1,1]})).is_err());
    let stable = ok(&mut a, "object_list", json!({"doc":doc,"page":1}));
    assert_eq!(stable, after, "a stale generation cannot edit");
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_eq!(ok(&mut a, "object_list", json!({"doc":doc,"page":1}))["objects"], inventory["objects"]);
    ok(&mut a, "edit_redo", json!({"doc":doc}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"moved.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path":"moved.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "object_list", json!({"doc":reopened,"page":1}))["objects"], after["objects"]);
    assert!(page_text(&mut a, reopened)[0].contains("Added label"));
}

#[test]
fn object_move_rejects_invalid_nested_references_without_any_change() {
    let dir = workdir("object-move-refusals");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "object_list", json!({"doc":doc,"page":1}));
    for objects in [
        json!([]),
        json!([{"kind":"text","index":0}]),
        json!([{"kind":"path","index":1}]),
        json!([{"kind":"text","index":1,"extra":true}]),
        json!([{"kind":"text","index":1.5}]),
        json!([{"kind":"text","index":1},{"kind":"text","index":1}]),
        json!([{"kind":"text","index":1},{"kind":"image","index":99}]),
        json!(vec![json!({"kind":"text","index":1}); 1001]),
    ] {
        assert!(a.call("object_move", &json!({"doc":doc,"page":1,"objects":objects,"offset":[10,20]})).is_err());
        assert_eq!(ok(&mut a, "object_list", json!({"doc":doc,"page":1})), before);
    }
    for offset in [json!([0, 0]), json!([1]), json!([1, 2, 3]), json!(["1", 2]), json!([1e30, 1])] {
        assert!(a.call("object_move", &json!({"doc":doc,"page":1,"objects":[{"kind":"text","index":1}],"offset":offset})).is_err());
        assert_eq!(ok(&mut a, "object_list", json!({"doc":doc,"page":1})), before);
    }
}

#[test]
fn a_deleted_image_is_not_kept_by_reduce_or_optimize() {
    let dir = workdir("deleted-image");
    let (w, h) = (600u32, 400u32);
    let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8 ^ (i & 7) as u8, (i / w * 255 / h) as u8, 128]).collect();
    let mut photo = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut photo, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    std::fs::write(dir.join("photo.png"), photo).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["photo.png"] }))["doc"].as_u64().unwrap();
    ok(&mut a, "image_edit", json!({ "doc": doc, "page": 1, "image": 1, "action": "delete" }));
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": doc, "page": 1 }))["count"], 0);
    // The page is blank, so the copies are tiny: the picture's data is not carried along.
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "optimized.pdf" }));
    assert_eq!(r["unused_xobjects"], 1, "{r}");
    assert!(r["bytes_after"].as_u64().unwrap() < 4_000, "{r}");
    let r = ok(&mut a, "doc_reduce", json!({ "doc": doc, "path": "reduced.pdf" }));
    assert!(r["bytes_after"].as_u64().unwrap() < 4_000, "{r}");
    let reopened = ok(&mut a, "doc_open", json!({ "path": "reduced.pdf" }));
    assert_eq!(reopened["pages"], 1);
}

/// Issue #740: `text_extract` with `rect` takes what Column select takes, in the displayed-page
/// coordinates `text_find` reports, on upright and turned pages alike.
#[test]
fn text_extract_with_a_rect_takes_one_column() {
    let dir = workdir("column");
    std::fs::write(dir.join("table.pdf"), table_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "table.pdf" }))["doc"].as_u64().unwrap();
    let reading = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1] }));
    assert_eq!(reading["pages"][0]["text"], "Name\nApple\nPear\nQty\n12\n7", "reading order is column by column");

    for page in [1u64, 2] {
        // The box around "Qty" and "7", as text_find reports them on this page.
        let rect_of = |a: &mut Automation, q: &str| -> [f64; 4] {
            let found = ok(a, "text_find", json!({ "doc": doc, "query": q }));
            let m = found["matches"].as_array().unwrap().iter().find(|m| m["page"] == page).unwrap().clone();
            let r: Vec<f64> = m["rects"][0].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            [r[0], r[1], r[2], r[3]]
        };
        let (qty, seven) = (rect_of(&mut a, "Qty"), rect_of(&mut a, "7"));
        let rect = [qty[0].min(seven[0]) - 2.0, qty[1].min(seven[1]) - 2.0, qty[2].max(seven[2]) + 2.0, qty[3].max(seven[3]) + 2.0];
        let column = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [page], "rect": rect }));
        assert_eq!(column["pages"][0]["text"], "Qty\n12\n7", "page {page}, rect {rect:?}");
    }

    // The whole table, row by row with tab-separated cells (upright page).
    let all = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1], "rect": [0, 0, 300, 300] }));
    assert_eq!(all["pages"][0]["text"], "Name\tQty\nApple\t12\nPear\t7");
    // Nothing inside: empty text, not an error.
    let none = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1], "rect": [0, 0, 5, 5] }));
    assert_eq!(none["pages"][0]["text"], "");
    // A rect that is not four numbers is refused.
    for bad in [json!([1, 2, 3]), json!("0 0 1 1"), json!([0, 0, "a", 1])] {
        let e = a.call("text_extract", &json!({ "doc": doc, "rect": bad })).unwrap_err();
        assert!(matches!(e, ToolError::InvalidArgs(_)), "{bad}: {e}");
    }
}

/// Copy with formatting, headless: `format: "html"` adds each page's text as the HTML fragment
/// Copy puts on the clipboard, for the whole page or only what `rect` takes.
#[test]
fn text_extract_as_html_keeps_the_formatting() {
    let dir = workdir("rich-copy");
    std::fs::write(dir.join("table.pdf"), table_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "table.pdf" }))["doc"].as_u64().unwrap();
    let page = &ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1], "format": "html" }))["pages"][0];
    let html = page["html"].as_str().unwrap();
    assert!(html.starts_with("<p style=\"margin:0\"><span style=\"font-family:'Helvetica';font-size:12pt\">Name"), "{html}");
    assert!(html.contains("Pear") && html.contains("Qty"), "{html}");
    assert_eq!(page["text"], "Name\nApple\nPear\nQty\n12\n7", "the plain text is unchanged");
    assert_eq!(page["styles_incomplete"], false);
    // Plain text by default: no html key.
    assert!(ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1] }))["pages"][0].get("html").is_none());
    // Only what the rect takes; nothing inside gives empty HTML.
    let column = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1], "rect": [120, 0, 300, 300], "format": "html" }));
    let html = column["pages"][0]["html"].as_str().unwrap();
    assert!(html.contains("Qty") && !html.contains("Name"), "{html}");
    let none = ok(&mut a, "text_extract", json!({ "doc": doc, "pages": [1], "rect": [0, 0, 5, 5], "format": "html" }));
    assert_eq!(none["pages"][0]["html"], "");
    // An unknown format is refused.
    for bad in [json!("rtf"), json!(1)] {
        let e = a.call("text_extract", &json!({ "doc": doc, "format": bad })).unwrap_err();
        assert!(matches!(e, ToolError::InvalidArgs(_)), "{bad}: {e}");
    }
    // Bold, italic, colour and size come through end to end.
    let body = "BT /F1 18 Tf 20 250 Td (Big) Tj ET BT /F2 12 Tf 20 200 Td (Strong) Tj ET \
        BT /F3 12 Tf 20 150 Td (Slanted) Tj ET BT 0 0 1 rg /F1 12 Tf 20 100 Td (Blue) Tj ET";
    let stream = format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len());
    let styled = pdf_of(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 300] >>",
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R >> >> >>",
        &stream,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>",
    ]);
    std::fs::write(dir.join("styled.pdf"), styled).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "styled.pdf" }))["doc"].as_u64().unwrap();
    let page = &ok(&mut a, "text_extract", json!({ "doc": doc, "format": "html" }))["pages"][0];
    let html = page["html"].as_str().unwrap();
    assert!(html.contains("font-size:18pt\">Big"), "{html}");
    assert!(html.contains("<strong>Strong</strong>"), "{html}");
    assert!(html.contains("<em>Slanted</em>"), "{html}");
    assert!(html.contains("color:#0000FF\">Blue"), "{html}");
}

#[test]
fn mixed_visual_region_edit_preview_save_reopen_and_undo_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, Stream, write_incremental};
    let dir = workdir("mixed-region");
    let mut cos = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&cos)[0].clone();
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica-Bold"));
    let bold = cos.add(Object::Dict(font));
    let mut resources = cos.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let mut fonts = cos.resolve(resources.get(b"Font").unwrap()).as_dict().unwrap().clone();
    fonts.set(b"F2".to_vec(), Object::Ref(bold));
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    let stream = cos.add(Object::Stream(Stream::flate(Dict::new(),
        b"BT /F2 1 Tf 18 0 0 18 20 240 Tm (Heading) Tj ET BT /F1 12 Tf 0 g 20 217 Td (Body ) Tj /F2 12 Tf 0 0.2 0.5 rg (bold) Tj /F1 12 Tf 0 g ( text) Tj ET")));
    cos.update_dict(page.obj, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(stream));
    })
    .unwrap();
    std::fs::write(dir.join("mixed.pdf"), write_incremental(&cos, &Default::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"mixed.pdf"}))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    assert_eq!(before["count"], 1);
    assert_eq!(before["paragraphs"][0]["editor_text"], "Heading\nBody bold text");
    assert_eq!(before["paragraphs"][0]["rows"].as_array().unwrap().len(), 2);
    assert_eq!(before["paragraphs"][0]["lines"].as_array().unwrap().len(), 4);
    let unchanged = ok(&mut a, "text_edit_layout", json!({"doc":doc,"page":1,"paragraph":1}));
    assert_eq!(unchanged["overflow"], false);
    assert_eq!(unchanged["source_rect"], unchanged["draft_rect"]);
    let growing = ok(&mut a, "text_edit_layout", json!({"doc":doc,"page":1,"paragraph":1,"text":"Heading\nBody bold text\nExtra row\nAnother row"}));
    assert_eq!(growing["overflow"], true);
    assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1})), before, "measuring does not apply a draft");
    let normal = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
    let preview = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72,"hide_text_block":1})).unwrap();
    assert_ne!(normal, preview);
    assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1})), before, "preview is read-only");
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"text":"Heading\nText bold text"}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"out/mixed-edited.pdf"}));
    let re = ok(&mut a, "doc_open", json!({"path":"out/mixed-edited.pdf"}))["doc"].as_u64().unwrap();
    let after = ok(&mut a, "text_paragraphs", json!({"doc":re,"page":1}));
    assert_eq!(after["count"], 1);
    assert_eq!(after["paragraphs"][0]["editor_text"], "Heading\nText bold text");
    let saved = Document::open(std::sync::Arc::new(std::fs::read(dir.join("out/mixed-edited.pdf")).unwrap())).unwrap();
    let mut session = pdfcraft_engine::Session::new();
    let id = session.open("saved.pdf", None, std::sync::Arc::new(write_incremental(&saved, &Default::default()).unwrap()), None).unwrap();
    let runs = session.get(id).unwrap().text_lines(0);
    let preserved = runs.iter().find(|l| l.text == "bold").unwrap();
    assert!(preserved.bold);
    assert_eq!(preserved.color, [0.0, 0.2, 0.5]);
    ok(&mut a, "text_edit", json!({"doc":re,"page":1,"paragraph":1,"text":""}));
    assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":re,"page":1}))["count"], 0);
    ok(&mut a, "edit_undo", json!({"doc":re}));
    assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":re,"page":1})), after);
}

#[test]
fn clipped_image_bounds_and_text_edit_save_reopen_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, Stream, write_incremental};
    let dir = workdir("clipped-artwork");
    let mut cos = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&cos)[0].clone();
    let mut image = Dict::new();
    for (key, value) in [
        ("Type", Object::name("XObject")),
        ("Subtype", Object::name("Image")),
        ("Width", Object::Int(1)),
        ("Height", Object::Int(1)),
        ("BitsPerComponent", Object::Int(8)),
        ("ColorSpace", Object::name("DeviceRGB")),
    ] {
        image.set(key.as_bytes().to_vec(), value);
    }
    let image = cos.add(Object::Stream(Stream::flate(image, &[220, 235, 245])));
    let mut resources = cos.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let mut xo = Dict::new();
    xo.set(b"Im0".to_vec(), Object::Ref(image));
    resources.set(b"XObject".to_vec(), Object::Dict(xo));
    let contents = cos.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 30 170 130 80 re W* n q 800 0 0 900 -300 -400 cm /Im0 Do Q BT /F1 12 Tf 30 195 Td (Shared caption) Tj ET Q BT /F1 12 Tf 30 130 Td (Editable caption) Tj ET",
    )));
    cos.update_dict(page.obj, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(contents));
    })
    .unwrap();
    std::fs::write(dir.join("clipped.pdf"), write_incremental(&cos, &Default::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"clipped.pdf"}))["doc"].as_u64().unwrap();
    let pixels = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
    let images = ok(&mut a, "page_images", json!({"doc":doc,"page":1}));
    assert_eq!(ok(&mut a, "image_hit", json!({"doc":doc,"page":1,"point":[50,60]}))["images"], json!([1]));
    assert_eq!(ok(&mut a, "image_hit", json!({"doc":doc,"page":1,"point":[20,60]}))["images"], json!([]));
    assert_eq!(images["images"][0]["visible_rect"], json!([30.0, 50.0, 160.0, 130.0]));
    assert_eq!(images["images"][0]["rect"], json!([-300.0, -200.0, 500.0, 700.0]), "raw placement remains available for transforms");
    let objects = ok(&mut a, "object_list", json!({"doc":doc,"page":1}));
    let object = objects["objects"].as_array().unwrap().iter().find(|o| o["kind"] == "image").unwrap();
    assert_eq!(object["rect"], images["images"][0]["visible_rect"]);
    assert_eq!(a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap(), pixels, "inventory does not change rendering");
    let labels = ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    ok(&mut a, "image_edit", json!({"doc":doc,"page":1,"image":1,"action":"move","rect":[-432.0,-300.0,688.0,960.0]}));
    let resized = ok(&mut a, "page_images", json!({"doc":doc,"page":1}));
    assert_eq!(
        resized["images"][0]["visible_rect"],
        json!([30.0, 50.0, 160.0, 130.0]),
        "a clip shared with the caption stays where it is (#1049 moves only an image's own clip)"
    );
    assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1})), labels);
    // The one-colour image still fills the fixed shared clip, so the page looks the same.
    let resized_pixels = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"out/resized.pdf"}));
    let resized_re = ok(&mut a, "doc_open", json!({"path":"out/resized.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "page_images", json!({"doc":resized_re,"page":1})), resized);
    assert_eq!(a.call("page_render", &json!({"doc":resized_re,"page":1,"dpi":72})).unwrap(), resized_pixels);
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_eq!(a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap(), pixels);
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":2,"text":"Edited caption"}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"out/edited.pdf"}));
    let re = ok(&mut a, "doc_open", json!({"path":"out/edited.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":re,"page":1}))["paragraphs"][1]["editor_text"], "Edited caption");
    assert_eq!(ok(&mut a, "page_images", json!({"doc":re,"page":1})), images, "image placement and crop survive text editing");
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_eq!(a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap(), pixels);
}

/// Independent inch/mm dimensions, checked in the saved file through the tool path.
#[test]
fn architectural_and_iso_papers_through_tools() {
    let dir = workdir("plotter-papers");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let mm = 72.0 / 25.4;
    for (name, w, h) in [
        ("ARCH A", 9.0 * 72.0, 12.0 * 72.0),
        ("arch b", 12.0 * 72.0, 18.0 * 72.0),
        ("ARCH C", 18.0 * 72.0, 24.0 * 72.0),
        ("ARCH D", 24.0 * 72.0, 36.0 * 72.0),
        ("ARCH E1", 30.0 * 72.0, 42.0 * 72.0),
        ("ARCH E (36 x 48 in)", 36.0 * 72.0, 48.0 * 72.0),
        ("ANSI C", 17.0 * 72.0, 22.0 * 72.0),
        ("ANSI D", 22.0 * 72.0, 34.0 * 72.0),
        ("ANSI E", 34.0 * 72.0, 44.0 * 72.0),
        ("A2", 420.0 * mm, 594.0 * mm),
        ("A1", 594.0 * mm, 841.0 * mm),
        ("A0", 841.0 * mm, 1189.0 * mm),
        ("A6", 105.0 * mm, 148.0 * mm),
        ("Letter", 612.0, 792.0),
        ("US Legal", 612.0, 1008.0),
        ("Tabloid", 792.0, 1224.0),
        ("US #10 Envelope", 4.125 * 72.0, 9.5 * 72.0),
    ] {
        for orientation in ["portrait", "landscape"] {
            let out = ok(
                &mut a,
                "doc_print",
                json!({"doc": doc, "pages": "1", "paper": name,
                "layout": "actual", "orientation": orientation, "path": "plot.pdf"}),
            );
            assert_eq!(out["sheets"], 1);
            let bytes = std::fs::read(dir.join("plot.pdf")).unwrap();
            let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
            let pages = pdfcraft_model::pages(&saved);
            assert_eq!(pages.len(), 1);
            let (ew, eh) = if orientation == "landscape" { (h, w) } else { (w, h) };
            let (aw, ah) = pages[0].display_size(&saved);
            assert!((aw - ew).abs() < 0.02 && (ah - eh).abs() < 0.02, "{name} {orientation}: {aw} x {ah}");
        }
    }
    let before = std::fs::read(dir.join("plot.pdf")).unwrap();
    let bad = a.call("doc_print", &json!({"doc": doc, "paper": "ARCH Z", "path": "plot.pdf"}));
    assert!(matches!(bad, Err(ToolError::InvalidArgs(m)) if m.contains("ARCH E1") && m.contains("A0")));
    assert_eq!(std::fs::read(dir.join("plot.pdf")).unwrap(), before);
    assert!(a.call("doc_print", &json!({"doc": doc, "paper": "ARCH D", "path": "../escape.pdf"})).is_err());
}

/// #2986: `layer_set` hides and shows layers in what `page_render` draws, by name (every group
/// with that name) or by position; `comments_hide` now changes `page_render` too. Neither
/// changes the file.
#[test]
fn layers_and_hidden_comments_change_what_page_render_draws() {
    let dir = workdir("layer-set");
    std::fs::write(dir.join("layers.pdf"), layered_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "layers.pdf" }))["doc"].as_u64().unwrap();
    let at = |a: &mut Automation, x: u32, y: u32| {
        let (w, _, px) = a.render_pixels(doc, 1, 1.0).unwrap();
        let i = ((y * w + x) * 4) as usize;
        [px.as_ref()[i], px.as_ref()[i + 1], px.as_ref()[i + 2]]
    };
    // Points (50, 40) red, (150, 40) green, (250, 40) blue, (150, 93) the comment; y down.
    assert_eq!(
        [at(&mut a, 50, 40), at(&mut a, 150, 40), at(&mut a, 250, 40), at(&mut a, 150, 93)],
        [[255, 0, 0], [0, 255, 0], [0, 0, 255], [0, 0, 0]]
    );

    let r = ok(&mut a, "layer_set", json!({ "doc": doc, "layer": "Walls", "visible": false }));
    assert_eq!(
        r["layers"],
        json!([{ "name": "Walls", "visible": false }, { "name": "Doors", "visible": true }, { "name": "Walls", "visible": false }])
    );
    assert_eq!(
        [at(&mut a, 50, 40), at(&mut a, 150, 40), at(&mut a, 250, 40)],
        [[255, 255, 255], [0, 255, 0], [255, 255, 255]],
        "both groups named Walls hide"
    );

    ok(&mut a, "layer_set", json!({ "doc": doc, "layer": 3, "visible": true }));
    assert_eq!([at(&mut a, 50, 40), at(&mut a, 250, 40)], [[255, 255, 255], [0, 0, 255]], "position 3 is the second Walls only");

    ok(&mut a, "comments_hide", json!({ "doc": doc }));
    assert_eq!(at(&mut a, 150, 93), [255, 255, 255], "hidden comments are not drawn");
    ok(&mut a, "comments_hide", json!({ "doc": doc, "hidden": false }));
    assert_eq!(at(&mut a, 150, 93), [0, 0, 0]);

    for (args, what) in [
        (json!({ "doc": doc, "layer": "Roof", "visible": false }), "an unknown name"),
        (json!({ "doc": doc, "layer": 0, "visible": false }), "position 0"),
        (json!({ "doc": doc, "layer": 4, "visible": false }), "a position past the end"),
        (json!({ "doc": doc, "layer": true, "visible": false }), "a layer that is neither"),
        (json!({ "doc": doc, "layer": "Doors" }), "no visible"),
    ] {
        assert!(matches!(a.call("layer_set", &args), Err(ToolError::InvalidArgs(_))), "{what} must be refused");
    }
    let list = ok(&mut a, "doc_list", json!({}));
    assert_eq!(list["documents"][0]["dirty"], json!(false), "a view setting leaves the document clean: {list}");
}

/// The `view_reading_colors` tool changes what `page_render` draws — text and background remap,
/// images never do — without touching the file, and its errors name what's wrong (#897).
#[test]
fn reading_colors_through_tools() {
    let dir = reading_colors_workdir();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let render = |a: &mut Automation, page: u64| png_pixels(&a.call("page_render", &json!({ "doc": doc, "page": page, "dpi": 72 })).unwrap()[0]);

    // Off: black text on a white page.
    let page1_off = render(&mut a, 1);
    let page2_off = render(&mut a, 2);
    assert_eq!(pixel(&page1_off, 10, 290), [255, 255, 255, 255]);

    let echo = ok(&mut a, "view_reading_colors", json!({ "doc": doc, "mode": "white-on-black" }));
    assert_eq!(echo, json!({ "mode": "white-on-black", "text": "#ffffff", "background": "#000000", "only_black": false, "line_art": true }));

    // The page clears to the setting's background, and the text paints with its colour.
    let page1_on = render(&mut a, 1);
    assert_eq!(pixel(&page1_on, 10, 290), [0, 0, 0, 255], "background clears to black");
    // The brightest pixel inside the text band is a stroke interior: a 48 pt bold stem covers it.
    let (tx, ty) = (20..172u32).flat_map(|x| (116..160u32).map(move |y| (x, y))).max_by_key(|&(x, y)| u32::from(pixel(&page1_on, x, y)[0])).unwrap();
    let on = pixel(&page1_on, tx, ty);
    assert!(on[0] >= 250, "a text-interior pixel renders white: {on:?}");
    let off = pixel(&page1_off, tx, ty);
    assert!(off[0] <= 60, "the same pixel is black ink without the setting: {off:?}");

    // Images are never remapped: page 2's picture draws byte-identically in both modes.
    let page2_on = render(&mut a, 2);
    fn image_row(px: &[u8], y: u32) -> &[u8] {
        let at = ((y * 200 + 122) * 4) as usize;
        &px[at..at + 36 * 4]
    }
    assert_eq!(pixel(&page2_on, 140, 80), [255, 0, 0, 255], "the image keeps its colour");
    for y in 62..98u32 {
        assert_eq!(image_row(&page2_on, y), image_row(&page2_off, y), "image row {y}");
    }

    // Errors name the problem and the allowed values.
    let missing = a.call("view_reading_colors", &json!({ "doc": doc, "mode": "custom", "text": "#00ff00" }));
    assert!(matches!(&missing, Err(ToolError::InvalidArgs(m)) if m.contains("background")), "{missing:?}");
    let unknown = a.call("view_reading_colors", &json!({ "doc": doc, "mode": "nope" }));
    assert!(matches!(&unknown, Err(ToolError::InvalidArgs(m)) if m.contains("off") && m.contains("custom")), "{unknown:?}");
    let no_mode = a.call("view_reading_colors", &json!({ "doc": doc }));
    assert!(matches!(&no_mode, Err(ToolError::InvalidArgs(m)) if m.contains("mode")), "{no_mode:?}");

    // Back off: the render matches the original one again.
    assert_eq!(
        ok(&mut a, "view_reading_colors", json!({ "doc": doc, "mode": "off" })),
        json!({ "mode": "off", "text": null, "background": null, "only_black": false, "line_art": false })
    );
    assert_eq!(render(&mut a, 1), page1_off);
}

#[test]
fn private_crops_with_nested_state_resize_save_reopen_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, Stream, write_incremental};
    let dir = workdir("private-crop-state");
    let mut cos = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&cos)[0].clone();
    let mut image = Dict::new();
    for (key, value) in [
        ("Type", Object::name("XObject")),
        ("Subtype", Object::name("Image")),
        ("Width", Object::Int(1)),
        ("Height", Object::Int(1)),
        ("BitsPerComponent", Object::Int(8)),
        ("ColorSpace", Object::name("DeviceRGB")),
    ] {
        image.set(key.as_bytes().to_vec(), value);
    }
    let image = cos.add(Object::Stream(Stream::flate(image, &[220, 235, 245])));
    let mut resources = cos.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let mut xo = Dict::new();
    xo.set(b"Im0".to_vec(), Object::Ref(image));
    resources.set(b"XObject".to_vec(), Object::Dict(xo));
    let parts: [&[u8]; 3] =
        [b"q 30 170 130 80 re W*", b"n 0.5 g q 800 0 0 900 -300 -400 cm /Im0 Do", b"Q Q BT /F1 12 Tf 30 130 Td (Neighbour caption) Tj ET"];
    let refs = parts.iter().map(|b| Object::Ref(cos.add(Object::Stream(Stream::flate(Dict::new(), b))))).collect();
    cos.update_dict(page.obj, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Array(refs));
    })
    .unwrap();
    std::fs::write(dir.join("crop.pdf"), write_incremental(&cos, &Default::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"crop.pdf"}))["doc"].as_u64().unwrap();
    let original = ok(&mut a, "page_images", json!({"doc":doc,"page":1}));
    let labels = ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    let pixels = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
    for factor in [1.2, 0.7] {
        let rect = [30.0 + (-300.0 - 30.0) * factor, 50.0 + (-200.0 - 50.0) * factor, 30.0 + (500.0 - 30.0) * factor, 50.0 + (700.0 - 50.0) * factor];
        ok(&mut a, "image_edit", json!({"doc":doc,"page":1,"image":1,"action":"move","rect":rect}));
        let images = ok(&mut a, "page_images", json!({"doc":doc,"page":1}));
        let expected = [30.0, 50.0, 30.0 + 130.0 * factor, 50.0 + 80.0 * factor];
        for (actual, expected) in images["images"][0]["visible_rect"].as_array().unwrap().iter().zip(expected) {
            assert!((actual.as_f64().unwrap() - expected).abs() < 0.01);
        }
        assert_eq!(images["images"][0]["pixels"], original["images"][0]["pixels"]);
        assert_eq!(ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1})), labels);
        let resized = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
        assert_ne!(resized, pixels, "the actual crop changes size");
        ok(&mut a, "doc_save", json!({"doc":doc,"path":"out/resized.pdf"}));
        let re = ok(&mut a, "doc_open", json!({"path":"out/resized.pdf"}))["doc"].as_u64().unwrap();
        assert_eq!(ok(&mut a, "page_images", json!({"doc":re,"page":1})), images);
        assert_eq!(a.call("page_render", &json!({"doc":re,"page":1,"dpi":72})).unwrap(), resized);
        ok(&mut a, "edit_undo", json!({"doc":doc}));
        assert_eq!(a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap(), pixels);
        assert_eq!(ok(&mut a, "page_images", json!({"doc":doc,"page":1})), original);
    }
}

#[test]
fn absolute_gotor_consent_does_not_bypass_validation_or_passwords() {
    let (base, root) = sandbox("gotor-absolute-consent");
    std::fs::create_dir_all(root.join("folder.pdf")).unwrap();
    std::fs::write(root.join("notes.txt"), fixture(1)).unwrap();
    std::fs::write(root.join("encrypted.pdf"), protected_fixture("secret")).unwrap();
    link_dir(&root, "outside-link", &base.join("outside"));
    let mut a = auto(&root);
    for (name, target, reason) in [
        ("missing", root.join("missing.pdf"), "missing.pdf"),
        ("folder", root.join("folder.pdf"), "not a regular file"),
        ("text", root.join("notes.txt"), "not a .pdf file"),
        ("outside", base.join("outside/secret.pdf"), "outside"),
        ("symlink", root.join("outside-link/secret.pdf"), "outside"),
    ] {
        let path = format!("{name}-source.pdf");
        std::fs::write(root.join(&path), remote_link_pdf(target.to_str().unwrap(), Some("[0 /Fit]"))).unwrap();
        let doc = ok(&mut a, "doc_open", json!({ "path": path }))["doc"].as_u64().unwrap();
        let before = a.session().docs().len();
        // The absence of a target or a root escape must not be probed before consent.
        assert!(
            matches!(err(&mut a, "link_open", json!({ "doc": doc, "page": 1, "index": 1 })), ToolError::Failed(m) if m.contains("allow_absolute"))
        );
        assert!(
            matches!(err(&mut a, "link_open", json!({ "doc": doc, "page": 1, "index": 1, "allow_absolute": true })), ToolError::Failed(m) if m.contains(reason)),
            "{name}"
        );
        assert_eq!(a.session().docs().len(), before);
    }
    std::fs::write(root.join("password-source.pdf"), remote_link_pdf(root.join("encrypted.pdf").to_str().unwrap(), Some("[0 /Fit]"))).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "password-source.pdf" }))["doc"].as_u64().unwrap();
    let before = a.session().docs().len();
    for password in ["", "wrong"] {
        assert!(matches!(
            err(&mut a, "link_open", json!({ "doc": doc, "page": 1, "index": 1, "allow_absolute": true, "password": password })),
            ToolError::Failed(_)
        ));
        assert_eq!(a.session().docs().len(), before);
    }
    let opened = ok(&mut a, "link_open", json!({ "doc": doc, "page": 1, "index": 1, "allow_absolute": true, "password": "secret" }));
    assert_eq!(opened["document"]["encrypted"], true);
    assert_eq!(opened["destination_applied"], true);
}

#[test]
fn finding_untagged_content_limits_results_to_content_annotations_and_widgets() {
    let dir = workdir("find-untagged");
    std::fs::write(dir.join("field.pdf"), form_pdf("name")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "field.pdf" }))["doc"].as_u64().unwrap();
    let found = ok(&mut a, "accessibility_find_untagged", json!({ "doc": doc }));
    let active: Vec<&str> = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|result| result["status"] != "skipped")
        .map(|result| result["rule"].as_str().unwrap())
        .collect();
    assert_eq!(active, ["tagged-content", "tagged-annotations", "tagged-form-fields"]);
    let fields = found["results"].as_array().unwrap().iter().find(|result| result["rule"] == "tagged-form-fields").unwrap();
    assert_eq!(fields["status"], "failed");
    assert_eq!(fields["findings"][0]["page"], 1);
}

#[test]
fn a_missing_path_under_the_root_names_where_it_was_looked_for() {
    // #1890: a script written from another working directory repeats the root's folder in its
    // paths. The refusal names the resolved path and the root, so there is something to correct.
    let (base, root) = sandbox("resolved-path");
    let mut a = auto(&root);
    let looked = |rel: &str| root.join(rel).display().to_string();
    for (path, rel) in
        [("root/inside.pdf", "root/inside.pdf"), ("missing/deeper/x.pdf", "missing/deeper/x.pdf"), ("./sub/../root/inside.pdf", "root/inside.pdf")]
    {
        let ToolError::Failed(m) = err(&mut a, "doc_open", json!({ "path": path })) else { panic!("{path}: not a failure") };
        assert_eq!(m, format!("{path} not found (looked for {}, --root {})", looked(rel), root.display()));
    }
    // The same mistake with a file that does exist outside still gets the plain refusal (#136).
    let ToolError::Failed(m) = err(&mut a, "doc_open", json!({ "path": "../outside/secret.pdf" })) else { panic!("not a failure") };
    assert!(!m.contains("looked for"), "{m}");
    ok(&mut a, "doc_open", json!({ "path": "inside.pdf" }));
    let _ = std::fs::remove_dir_all(base);
}

/// `doc_print` prints as image: pictures at `dpi`, grey with `grayscale`; a resolution outside
/// 72–600 is refused.
#[test]
fn doc_print_as_image() {
    let dir = workdir("print-image");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_print", json!({ "doc": doc, "pages": "1", "as_image": true, "dpi": 150, "grayscale": true, "path": "image.pdf" }));
    assert_eq!(r["sheets"], 1);
    let bytes = std::fs::read(dir.join("image.pdf")).unwrap();
    let out = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    let pictures =
        out.scan_objects().filter(|(_, o)| matches!(&**o, pdfcraft_cos::Object::Stream(st) if st.dict.name(b"Subtype") == Some(b"Image"))).count();
    assert_eq!(pictures, 1);
    match a.call("doc_print", &json!({ "doc": doc, "as_image": true, "dpi": 20, "path": "x.pdf" })) {
        Err(ToolError::InvalidArgs(m)) => assert!(m.contains("dpi"), "{m}"),
        other => panic!("{other:?}"),
    }
}

/// Without a root, `write_output` still writes through a staging file (#2166): the folder is
/// made, nothing is left beside the file, and a folder at the path is refused.
#[test]
fn a_root_less_write_output_is_atomic_too() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-automation-rootless-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let free = Automation::new();
    let target = dir.join("new").join("page.png");
    let written = free.write_output(&target.to_string_lossy(), &[1, 2, 3]).unwrap();
    assert_eq!(written, target);
    assert_eq!(std::fs::read(&target).unwrap(), [1, 2, 3]);
    let names: Vec<_> = std::fs::read_dir(dir.join("new")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["page.png"], "no staging file left beside it");
    let e = free.write_output(&dir.join("new").to_string_lossy(), &[4]).unwrap_err();
    assert!(matches!(&e, ToolError::Failed(m) if m.contains("is a folder")), "{e:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn attaching_and_deleting_files_through_the_tools() {
    let dir = workdir("attach");
    std::fs::write(dir.join("notes.txt"), b"keep this with the PDF").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert!(ok(&mut a, "doc_info", json!({ "doc": doc }))["attachments"].as_array().unwrap().is_empty());
    // A text file and a PDF, the PDF under a name of its own; the second notes.txt is kept apart.
    let r = ok(&mut a, "attachment_add", json!({ "doc": doc, "path": "notes.txt", "description": "Meeting notes" }));
    assert_eq!(r["name"], "notes.txt");
    let r = ok(&mut a, "attachment_add", json!({ "doc": doc, "path": "b.pdf", "name": "annex.pdf" }));
    assert_eq!(r["name"], "annex.pdf");
    let r = ok(&mut a, "attachment_add", json!({ "doc": doc, "path": "notes.txt" }));
    assert_eq!(r["name"], "notes.txt (2)");
    let listed = ok(&mut a, "doc_info", json!({ "doc": doc }))["attachments"].clone();
    let names: Vec<&str> = listed.as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["annex.pdf", "notes.txt", "notes.txt (2)"]);
    assert_eq!(listed[1]["description"], "Meeting notes");
    assert_eq!(listed[1]["size"], 22);
    // Saved and reopened, the attachment is there with its bytes.
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "with-notes.pdf" }));
    let saved = std::fs::read(dir.join("with-notes.pdf")).unwrap();
    let info = pdfcraft_render::inspect(std::sync::Arc::new(saved.clone()), None).unwrap();
    let att = info.attachments.iter().find(|x| x.name == "notes.txt").expect("notes.txt is in the file");
    assert_eq!(pdfcraft_render::attachment_data(&saved, None, att).unwrap(), b"keep this with the PDF");
    // Deleting one leaves the others; a name that is not there is refused; undo brings it back.
    ok(&mut a, "attachment_delete", json!({ "doc": doc, "name": "notes.txt" }));
    let names: Vec<String> = ok(&mut a, "doc_info", json!({ "doc": doc }))["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["annex.pdf", "notes.txt (2)"]);
    assert!(a.call("attachment_delete", &json!({ "doc": doc, "name": "notes.txt" })).is_err());
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["attachments"].as_array().unwrap().len(), 3);
    assert!(a.call("attachment_add", &json!({ "doc": doc, "path": "missing.txt" })).is_err(), "a file that is not there is refused");
}

#[test]
fn fdf_nested_comment_values_survive_export_import_and_save_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, PdfString, SaveOptions, write_full};
    use std::sync::Arc;
    let dir = workdir("fdf-nested-values");
    let mut source = Document::open(Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&source)[0].obj;
    let mut note = Dict::new();
    note.set(b"Type".to_vec(), Object::name("Annot"));
    note.set(b"Subtype".to_vec(), Object::name("Square"));
    note.set(b"NM".to_vec(), PdfString::text("nested-values"));
    note.set(b"Rect".to_vec(), Object::Array([10, 20, 30, 40].map(|value| Object::Ref(source.add(Object::Int(value)))).into()));
    note.set(b"C".to_vec(), Object::Array([0.25, 0.5, 0.75].map(|value| Object::Ref(source.add(Object::Real(value)))).into()));
    let note = source.add(note);
    source.update_dict(page, |dict| dict.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(note)]))).unwrap();
    std::fs::write(dir.join("source.pdf"), write_full(&source, &SaveOptions::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let source = ok(&mut a, "doc_open", json!({ "path": "source.pdf" }))["doc"].as_u64().unwrap();

    ok(&mut a, "doc_export_data", json!({ "doc": source, "path": "notes.fdf", "what": "comments" }));
    let target = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_import_data", json!({ "doc": target, "path": "notes.fdf" }));
    ok(&mut a, "doc_save", json!({ "doc": target, "path": "restored.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "restored.pdf" }))["doc"].as_u64().unwrap();
    let after = ok(&mut a, "comment_list", json!({ "doc": reopened }));
    // The saved input has legal indirect scalar numbers. The engine's source inspection
    // currently reads those as defaults, so compare the restored data to the fixture's
    // known values (converted from PDF bottom-left to the tool's top-left coordinates).
    assert_eq!(after["comments"][0]["rect"], json!([10.0, 260.0, 30.0, 280.0]));
    assert_eq!(after["comments"][0]["color"], "#4080BF");
}

#[test]
fn refused_fdf_export_keeps_the_destination_and_source_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, write_full};
    use std::sync::Arc;
    let dir = workdir("fdf-export-limit");
    let mut source = Document::open(Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&source)[0].obj;
    let mut note = Dict::new();
    note.set(b"Type".to_vec(), Object::name("Annot"));
    note.set(b"Subtype".to_vec(), Object::name("Square"));
    note.set(b"Rect".to_vec(), Object::Array(vec![10.into(), 20.into(), 30.into(), 40.into()]));
    note.set(b"PCData".to_vec(), Object::Array(vec![Object::Null; 100_001]));
    let note = source.add(note);
    source.update_dict(page, |dict| dict.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(note)]))).unwrap();
    let bytes = write_full(&source, &SaveOptions::default()).unwrap();
    std::fs::write(dir.join("source.pdf"), &bytes).unwrap();
    std::fs::write(dir.join("notes.fdf"), b"existing destination").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "source.pdf" }))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let error = a.call("doc_export_data", &json!({ "doc": doc, "path": "notes.fdf", "what": "comments" })).unwrap_err();
    assert!(error.to_string().contains("100,000 values"), "{error}");
    assert_eq!(std::fs::read(dir.join("notes.fdf")).unwrap(), b"existing destination");
    assert_eq!(std::fs::read(dir.join("source.pdf")).unwrap(), bytes);
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc })), before);
}

#[test]
fn text_box_and_callout_fill_and_border_through_tools() {
    let dir = workdir("textbox-fill");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for kind in ["textbox", "callout"] {
        let mut args = json!({ "doc": doc, "page": 1, "type": kind, "rect": [80, 40, 180, 100], "contents": "Example" });
        if kind == "callout" {
            args["to"] = json!([20, 120]);
        }
        let added = ok(&mut a, "comment_add", args);
        let comment = &added["comment"];
        ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": comment["index"], "fill": "white", "width": 3 }));
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "filled.pdf" }));
    let mut session = pdfcraft_engine::Session::new();
    let bytes = std::sync::Arc::new(std::fs::read(dir.join("filled.pdf")).unwrap());
    let saved = session.open("filled.pdf", None, bytes, None).unwrap();
    for index in 0..2 {
        let props = session.get(saved).unwrap().comment_props(0, index).unwrap();
        assert_eq!((props.fill, props.width), (Some([1.0; 3]), Some(3.0)));
        ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": index + 1, "fill": "none" }));
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "transparent.pdf" }));
    let bytes = std::sync::Arc::new(std::fs::read(dir.join("transparent.pdf")).unwrap());
    let saved = session.open("transparent.pdf", None, bytes, None).unwrap();
    for index in 0..2 {
        assert_eq!(session.get(saved).unwrap().comment_props(0, index).unwrap().fill, None);
    }
}

/// Emptying a paragraph that another paragraph overlaps must not hand the emptied paragraph's
/// number to the neighbour that took over its box: the next edit by that number would rewrite it.
#[test]
fn an_emptied_paragraph_does_not_pass_its_number_to_an_overlapping_neighbour() {
    let dir = workdir("text-edit-overlap-number");
    std::fs::write(dir.join("p.pdf"), overlaid_paragraph_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"p.pdf"}))["doc"].as_u64().unwrap();
    let listed = ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    assert_eq!(listed["count"], 2, "{listed}");
    let k = listed["paragraphs"].as_array().unwrap().iter().position(|p| p["text"].as_str().unwrap().starts_with("Alpha")).unwrap() + 1;

    let first = ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":k,"text":""}));
    assert_eq!(first["stale_paragraphs"], json!([k]), "the emptied paragraph's number is gone: {first}");
    let again = a.call("text_edit", &json!({"doc":doc,"page":1,"paragraph":k,"text":"Rewritten."}));
    assert!(matches!(&again, Err(ToolError::InvalidArgs(m)) if m.contains("text_paragraphs")), "{again:?}");
    assert!(page_text(&mut a, doc).join(" ").contains("Zulu overlay."), "the neighbour is untouched");
    std::fs::remove_dir_all(dir).unwrap();
}

/// #2516: a paragraph number names the paragraph `text_paragraphs` listed. When an edit regroups
/// the page so that paragraph no longer exists on its own, the next edit by that number is
/// refused. It used to rewrite whichever paragraph had taken the number, and that text was gone.
///
/// Moving Alpha down (no text change) is the regrouping edit: #2517 wraps a rewrite that adds or
/// keeps rows in a marked region precisely so it can't be mistaken for a neighbour by geometry
/// alone, so a text rewrite can no longer manufacture this scenario; a pure move still can.
#[test]
fn text_edit_refuses_a_paragraph_number_an_earlier_edit_regrouped() {
    let dir = workdir("text-edit-stale-number");
    std::fs::write(dir.join("p.pdf"), three_paragraph_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"p.pdf"}))["doc"].as_u64().unwrap();
    let listed = ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    assert_eq!(listed["count"], 3);
    assert!(listed["paragraphs"][1]["text"].as_str().unwrap().starts_with("Bravo"));

    // Listing again here would renumber the page, so the first edit's result says what moved.
    // Moving Alpha down by 21.2 pt brings its last row within 14.4 pt of Bravo, its own leading.
    let first = ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"dy":-21.2}));
    assert_eq!(first["stale_paragraphs"], json!([1, 2]), "moving Alpha down regroups it with Bravo: {first}");

    // Paragraph 2 of the earlier listing was Bravo, which is now part of Alpha's paragraph.
    let stale = a.call("text_edit", &json!({"doc":doc,"page":1,"paragraph":2,"text":"Bravo paragraph, edited."}));
    assert!(matches!(&stale, Err(ToolError::InvalidArgs(m)) if m.contains("text_paragraphs")), "{stale:?}");
    let text = page_text(&mut a, doc).join(" ");
    assert!(text.contains("Charlie paragraph, first row."), "Charlie is untouched: {text}");
    assert!(text.contains("Charlie paragraph, second row."), "Charlie is untouched: {text}");
    assert!(!text.contains("Bravo paragraph, edited."), "nothing was rewritten: {text}");

    // After listing again, the number names what the listing says.
    let relisted = ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    assert!(relisted["paragraphs"][1]["text"].as_str().unwrap().starts_with("Charlie"));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":2,"text":"Charlie paragraph, edited."}));
    let text = page_text(&mut a, doc).join(" ");
    assert!(text.contains("Charlie paragraph, edited.") && text.contains("Alpha paragraph, second row."), "{text}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// An edit that merges its paragraph into a neighbour leaves no paragraph of its own to follow:
/// the merged one holds the edited text too, but rewriting it by the edited paragraph's number
/// would drop the neighbour's text (#2516). Moving Alpha down (see the previous test) is the
/// regrouping edit, since a text rewrite is marked precisely so it cannot merge this way (#2517).
#[test]
fn text_edit_does_not_follow_a_paragraph_into_the_one_it_merged_with() {
    let dir = workdir("text-edit-merged-number");
    std::fs::write(dir.join("p.pdf"), three_paragraph_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"p.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    let first = ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"dy":-21.2}));
    assert!(first.get("paragraph").is_none(), "the result must not present the merged paragraph as the edited one: {first}");

    // Paragraph 1 was Alpha. Alpha now shares a paragraph with Bravo, so the number is refused.
    let again = a.call("text_edit", &json!({"doc":doc,"page":1,"paragraph":1,"text":"Alpha only."}));
    assert!(matches!(&again, Err(ToolError::InvalidArgs(m)) if m.contains("text_paragraphs")), "{again:?}");
    let text = page_text(&mut a, doc).join(" ");
    assert!(text.contains("Bravo paragraph, a single row."), "Bravo is untouched: {text}");
    assert!(!text.contains("Alpha only."), "nothing was rewritten: {text}");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The editor background preview takes the same paragraph number as `text_edit`, so it hides the
/// paragraph the number named in the listing and refuses a number an edit made stale (#2516).
/// Moving Alpha down (see the two tests above) is the regrouping edit; a text rewrite is marked
/// precisely so it cannot merge this way (#2517).
#[test]
fn page_render_hide_text_block_refuses_a_paragraph_number_an_earlier_edit_regrouped() {
    let dir = workdir("hide-text-block-stale-number");
    std::fs::write(dir.join("p.pdf"), three_paragraph_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"p.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"dy":-21.2}));

    // Paragraph 2 of the listing was Bravo, now inside Alpha's paragraph; position 2 is Charlie.
    let stale = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72,"hide_text_block":2}));
    assert!(matches!(&stale, Err(ToolError::InvalidArgs(m)) if m.contains("text_paragraphs")), "{:?}", stale.map(|_| ()));
    // Paragraph 3 of the listing is Charlie, wherever it is now.
    assert!(a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72,"hide_text_block":3})).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

/// The listings kept for paragraph numbers are capped. A page whose listing was dropped must not
/// fall back to reading its numbers as positions, which an edit may have regrouped: it is refused
/// until the page is listed again (#2516).
#[test]
fn text_edit_refuses_a_paragraph_number_whose_listing_was_dropped_at_the_cap() {
    let dir = workdir("text-edit-evicted-listing");
    std::fs::write(dir.join("p.pdf"), three_paragraph_pdf()).unwrap();
    let mut a = auto(&dir);
    let open = |a: &mut Automation| ok(a, "doc_open", json!({"path":"p.pdf"}))["doc"].as_u64().unwrap();
    let doc = open(&mut a);
    ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    // 256 more pages fill the cap, so the next new page drops every earlier listing.
    let others: Vec<u64> = (0..256).map(|_| open(&mut a)).collect();
    for &other in &others {
        ok(&mut a, "text_paragraphs", json!({"doc":other,"page":1}));
    }

    let dropped = a.call("text_edit", &json!({"doc":doc,"page":1,"paragraph":2,"text":"Bravo paragraph, edited."}));
    assert!(matches!(&dropped, Err(ToolError::InvalidArgs(m)) if m.contains("text_paragraphs")), "{dropped:?}");
    assert!(!page_text(&mut a, doc).join(" ").contains("Bravo paragraph, edited."), "nothing was rewritten");
    let hidden = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72,"hide_text_block":2}));
    assert!(matches!(&hidden, Err(ToolError::InvalidArgs(m)) if m.contains("text_paragraphs")), "{:?}", hidden.map(|_| ()));

    // A page listed after the drop keeps its numbers, and so does the page once it is listed again.
    ok(&mut a, "text_edit", json!({"doc":others[255],"page":1,"paragraph":2,"text":"Bravo paragraph, edited."}));
    ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":2,"text":"Bravo paragraph, edited."}));
    assert!(page_text(&mut a, doc).join(" ").contains("Bravo paragraph, edited."));
    std::fs::remove_dir_all(dir).unwrap();
}

/// A number from the listing keeps naming its paragraph while other paragraphs are edited, and
/// a paragraph can be edited twice in a row by the same number.
#[test]
fn text_edit_numbers_survive_edits_that_do_not_regroup() {
    let dir = workdir("text-edit-stable-number");
    std::fs::write(dir.join("p.pdf"), three_paragraph_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"p.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "text_paragraphs", json!({"doc":doc,"page":1}));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":3,"text":"Charlie one.\nCharlie two."}));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"text":"Alpha one.\nAlpha two."}));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"text":"Alpha again.\nAlpha two."}));
    ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":3,"text":"Charlie once more.\nCharlie two."}));
    // A rewrite rewraps at the width of the widest row, so compare without line breaks.
    let text = page_text(&mut a, doc).join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
    for want in ["Alpha again.", "Bravo paragraph, a single row.", "Charlie once more."] {
        assert!(text.contains(want), "{want}: {text}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_bookmark_added_at_a_position_reports_the_same_position_back_on_any_page_rotation() {
    let dir = workdir("bookmark-position");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [3], "degrees": 90 }));
    let near = |v: &Value, want: Option<f64>, what: &str| match want {
        Some(w) => assert!((v.as_f64().unwrap_or_else(|| panic!("{what}: {v}")) - w).abs() < 0.05, "{what}: {v} vs {w}"),
        None => assert!(v.is_null(), "{what}: {v}"),
    };
    // Page 1 upright, page 3 turned a quarter: a displayed top and left come back as given.
    for (page, title) in [(1, "Upright"), (3, "Turned")] {
        for (suffix, left, top) in [(" both", Some(72.5), Some(120.25)), (" top", None, Some(200.0)), (" left", Some(50.0), None)] {
            let name = format!("{title}{suffix}");
            let mut args = json!({ "doc": doc, "title": name, "page": page });
            if let Some(l) = left {
                args["left"] = json!(l);
            }
            if let Some(t) = top {
                args["top"] = json!(t);
            }
            ok(&mut a, "bookmark_add", args);
            let d = bookmark_destination(&mut a, doc, &name);
            assert_eq!(d["fit"], "XYZ", "{name}");
            near(&d["left"], left, &format!("{name} left"));
            near(&d["top"], top, &format!("{name} top"));
            assert!(d["zoom"].is_null(), "the zoom is kept: {name}");
        }
    }
    // No position is the page top at the reader's zoom, as before.
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Plain", "page": 2 }));
    let plain = bookmark_destination(&mut a, doc, "Plain");
    assert!(plain["left"].is_null() && plain["top"].is_null() && plain["zoom"].is_null(), "{plain}");

    // A position that is not a usable number is refused and adds nothing.
    let before = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    for bad in [json!("high"), json!(2_000_000), json!(-2_000_000.5), json!(true)] {
        for key in ["top", "left"] {
            let args = json!({ "doc": doc, "title": "Bad", "page": 1, key: bad });
            assert!(matches!(a.call("bookmark_add", &args), Err(ToolError::InvalidArgs(_))), "{key} {bad}");
        }
    }
    assert_eq!(ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"], before, "nothing was added");

    // It survives a save and reopen.
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "placed.pdf" }));
    let mut b = auto(&dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "placed.pdf" }))["doc"].as_u64().unwrap();
    let d = bookmark_destination(&mut b, re, "Turned both");
    near(&d["left"], Some(72.5), "reopened left");
    near(&d["top"], Some(120.25), "reopened top");
}

/// `comment_edit points` reshapes a line, connected lines or a polygon, in one undo step, and
/// `comment_list` reports the points it takes.
#[test]
fn comments_are_reshaped_through_the_tool() {
    let dir = workdir("reshape");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "line", "from": [20, 40], "to": [120, 40] }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc, "page": 1 }));
    assert_eq!(list["comments"][0]["points"], json!([[20.0, 40.0], [120.0, 40.0]]));
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": 1, "points": [[20, 40], [20, 140]] }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc, "page": 1 }));
    assert_eq!(list["comments"][0]["points"], json!([[20.0, 40.0], [20.0, 140.0]]));
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Reshape comment");
    assert!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 1, "points": [[1, 2]] })).is_err(), "a line has two ends");
    assert!(matches!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 1, "points": "no" })), Err(ToolError::InvalidArgs(_))));
}

/// #3193: bookmark_list reports a bookmark's colour and style (ISO 32000-2 §12.3.3, Tables 151
/// and 152), only where they differ from black and regular. The list is read from the opened file
/// and, once the document has been edited, from the edited one; both read them alike.
/// bookmark_set_style changes them, and they survive a save.
#[test]
fn bookmark_list_reports_colour_and_style() {
    let dir = workdir("bookmark-style");
    let objects: Vec<&[u8]> = vec![
        b"<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        b"<< /Type /Page /Parent 2 0 R >>",
        b"<< /Type /Outlines /First 5 0 R /Last 7 0 R /Count 3 >>",
        b"<< /Title (Plain) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>",
        b"<< /Title (Red bold) /Parent 4 0 R /Prev 5 0 R /Next 7 0 R /Dest [3 0 R /Fit] /C [1 0 0] /F 2 >>",
        b"<< /Title (Blue italic) /Parent 4 0 R /Prev 6 0 R /Dest [3 0 R /Fit] /C [0 0 1] /F 1 >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    std::fs::write(dir.join("styled.pdf"), bytes).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "styled.pdf" }))["doc"].as_u64().unwrap();
    let style = |list: &Value| -> Vec<(Value, Value, Value)> {
        list.as_array().unwrap().iter().map(|b| (b["color"].clone(), b["bold"].clone(), b["italic"].clone())).collect()
    };
    let expected = vec![
        (Value::Null, Value::Null, Value::Null),
        (json!([1.0, 0.0, 0.0]), json!(true), Value::Null),
        (json!([0.0, 0.0, 1.0]), Value::Null, json!(true)),
    ];
    let opened = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    assert_eq!(style(&opened), expected, "as opened: {opened}");
    ok(&mut a, "bookmark_rename", json!({ "doc": doc, "path": [1], "title": "Still plain" }));
    let edited = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    assert_eq!(style(&edited), expected, "after an edit: {edited}");

    // bookmark_set_style changes only what it is given.
    ok(&mut a, "bookmark_set_style", json!({ "doc": doc, "path": [1], "color": [0, 0.5, 0], "italic": true }));
    ok(&mut a, "bookmark_set_style", json!({ "doc": doc, "path": [2], "bold": false }));
    ok(&mut a, "bookmark_set_style", json!({ "doc": doc, "path": [3], "color": [0, 0, 0] }));
    let set = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    assert_eq!(
        style(&set),
        vec![
            (json!([0.0, 0.5, 0.0]), Value::Null, json!(true)),
            (json!([1.0, 0.0, 0.0]), Value::Null, Value::Null),
            (Value::Null, Value::Null, json!(true))
        ],
        "{set}"
    );
    // A colour and a style in one call are one undo step.
    ok(&mut a, "bookmark_set_style", json!({ "doc": doc, "path": [2], "color": [0, 0, 0], "bold": true, "italic": true }));
    let both = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    assert_eq!(style(&both)[1], (Value::Null, json!(true), json!(true)), "{both}");
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    let undone = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    assert_eq!(style(&undone), style(&set), "one undo takes back both: {undone}");
    for bad in [
        json!({ "doc": doc, "path": [1] }),
        json!({ "doc": doc, "path": [1], "color": [2, 0, 0] }),
        json!({ "doc": doc, "path": [1], "color": [1, 0] }),
    ] {
        assert!(matches!(a.call("bookmark_set_style", &bad), Err(ToolError::InvalidArgs(_))), "{bad}");
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "restyled.pdf" }));
    let mut b = auto(&dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "restyled.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(style(&ok(&mut b, "bookmark_list", json!({ "doc": re }))["bookmarks"]), style(&set), "saved");
}

/// `aligns` gives each line of added text an alignment of its own; `align` alone is the whole box
/// again. Text edits take `aligns` for a paragraph's rows the same way.
#[test]
fn added_text_lines_take_their_own_alignments() {
    let dir = workdir("paragraph-aligns");
    let mut a = auto(&dir);
    let id = ok(&mut a, "doc_create", json!({"from":"blank","width":600,"height":400}))["doc"].as_u64().unwrap();
    ok(
        &mut a,
        "page_add_text",
        json!({"doc":id,"page":1,"text":"Left line\nRight line\nInherited line","at":[40,50],"width":500,"size":12,"aligns":[null,"right","inherit"]}),
    );
    let rect = |a: &mut Automation, text: &str| -> Vec<f64> {
        let lines = ok(a, "text_lines", json!({"doc":id,"page":1}));
        let line = lines["lines"].as_array().unwrap().iter().find(|l| l["text"] == text).cloned().unwrap();
        line["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    };
    assert!((rect(&mut a, "Left line")[0] - 40.0).abs() < 1.0);
    assert!((rect(&mut a, "Right line")[2] - 540.0).abs() < 1.0, "{:?}", rect(&mut a, "Right line"));
    assert!((rect(&mut a, "Inherited line")[0] - 40.0).abs() < 1.0);
    // The whole box centred: the right-aligned line goes with it.
    ok(&mut a, "content_update", json!({"doc":id,"page":1,"index":1,"align":"center"}));
    let right = rect(&mut a, "Right line");
    assert!(((right[0] + right[2]) / 2.0 - 290.0).abs() < 1.0, "{right:?}");
    let inherited = rect(&mut a, "Inherited line");
    assert!(((inherited[0] + inherited[2]) / 2.0 - 290.0).abs() < 1.0, "{inherited:?}");
    assert!(a.call("content_update", &json!({"doc":id,"page":1,"index":1,"aligns":["sideways"]})).is_err());
    assert!(a.call("content_update", &json!({"doc":id,"page":1,"index":1,"aligns":"right"})).is_err());
}

/// Existing-row alignment is withheld until its intent survives later editing sessions.
#[test]
fn text_edit_row_alignment_is_refused_without_changing_existing_text() {
    let dir = workdir("paragraph-row-aligns");
    let body = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (The quick brown fox jumps over the lazy dog) Tj T* (and keeps running.) Tj ET";
    std::fs::write(dir.join("rows.pdf"), win_ansi_paragraph_pdf(body)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "rows.pdf" }))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "text_lines", json!({ "doc": doc, "page": 1 }));
    let refused = err(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "aligns": [null, "right"] }));
    assert!(matches!(&refused, ToolError::Failed(m) | ToolError::InvalidArgs(m) if m.contains("selected-row alignment")), "{refused:?}");
    assert_eq!(ok(&mut a, "text_lines", json!({ "doc": doc, "page": 1 })), before);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "unchanged.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "unchanged.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_lines", json!({ "doc": reopened, "page": 1 })), before);
}

/// An editable combo box (ISO 32000-2 Table 233, Edit) takes a value that isn't one of its
/// options, and its page appearance shows it, before and after a save; one without Edit takes
/// only its options.
#[test]
fn form_fill_takes_free_text_only_in_an_editable_combo_box() {
    let dir = workdir("editable-combo");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let combo = |name: &str, top: f64, editable: bool| json!({ "doc": doc, "page": 1, "type": "combo", "rect": [20, top, 200, top + 22.0], "name": name, "options": ["Apple", "Banana"], "editable": editable });
    ok(&mut a, "form_add_field", combo("fruit", 20.0, true));
    ok(&mut a, "form_add_field", combo("veg", 60.0, false));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "fruit": "Cherry" } }));
    assert!(page_text(&mut a, doc)[0].contains("Cherry"), "the typed value is drawn");
    let refused = err(&mut a, "form_fill", json!({ "doc": doc, "values": { "veg": "Cherry" } }));
    assert!(matches!(&refused, ToolError::Failed(m) | ToolError::InvalidArgs(m) if m.contains("no option")), "{refused:?}");
    // A choice from the list still works in both.
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "veg": "Banana" } }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "filled.pdf" }));
    let saved = ok(&mut a, "doc_open", json!({ "path": "filled.pdf" }))["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({ "doc": saved }));
    let value = |name: &str| fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).map(|f| f["value"].clone());
    assert_eq!(value("fruit"), Some(json!("Cherry")), "{fields}");
    assert_eq!(value("veg"), Some(json!("Banana")), "{fields}");
    let text = page_text(&mut a, saved);
    assert!(text[0].contains("Cherry") && text[0].contains("Banana"), "{text:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `page_render`'s pixels show a rubber stamp written without an appearance as the stamp its
/// `/Name` names (ISO 32000-2 Table 184), and one with an unknown name as a grey placeholder.
#[test]
fn page_render_draws_stamps_without_an_appearance() {
    let dir = workdir("stamps-without-appearance");
    let objects: Vec<&[u8]> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 100] >>",
        b"<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R] >>",
        b"<< /Type /Annot /Subtype /Stamp /Rect [10 20 130 80] /Name /Approved >>",
        b"<< /Type /Annot /Subtype /Stamp /Rect [170 20 290 80] /Name /MyCustomStamp >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    std::fs::write(dir.join("stamps.pdf"), bytes).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "stamps.pdf" }))["doc"].as_u64().unwrap();
    let (width, _, pixels) = a.render_pixels(doc, 1, 1.0).unwrap();
    let pixels = pixels.as_ref();
    // Ink in columns `x0..x1` of rows 20..80, which are the stamps' rows whether counted from the top or the bottom.
    let ink = |x0: u32, x1: u32| -> Vec<[u8; 3]> {
        (20..80u32)
            .flat_map(|y| (x0..x1).map(move |x| (y, x)))
            .map(|(y, x)| {
                let i = ((y * width + x) * 4) as usize;
                [pixels[i], pixels[i + 1], pixels[i + 2]]
            })
            .filter(|p| p.iter().any(|v| *v < 200))
            .collect()
    };
    let approved = ink(10, 130);
    let green = approved.iter().filter(|p| p[1] > p[0] && p[1] > p[2]).count();
    assert!(green > 300 && green * 10 > approved.len() * 9, "Approved is drawn green: {green} of {}", approved.len());
    let custom = ink(170, 290);
    let grey = custom.iter().filter(|p| p[0].abs_diff(p[1]) < 24 && p[1].abs_diff(p[2]) < 24).count();
    assert!(grey > 100 && grey == custom.len(), "an unknown name is a grey placeholder: {grey} of {}", custom.len());
}
