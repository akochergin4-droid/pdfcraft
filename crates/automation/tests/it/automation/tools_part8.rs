//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 8 of 8).

#[allow(unused_imports)]
use super::*;

#[test]
fn deleting_one_radio_button_through_tools_keeps_the_group() {
    let dir = workdir("delete-radio");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (x, export) in [(20, "S"), (40, "M"), (60, "L")] {
        let args = json!({ "doc": doc, "page": 1, "type": "radio", "rect": [x, 100, x + 14, 114], "group": "size", "export": export });
        ok(&mut a, "form_add_field", args);
    }
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "size": "L" } }));
    ok(&mut a, "form_delete_field", json!({ "doc": doc, "field": "size", "widget": 2 }));
    let size = |a: &mut Automation| ok(a, "form_fields", json!({ "doc": doc }))["fields"][0].clone();
    assert_eq!((size(&mut a)["options"].clone(), size(&mut a)["value"].clone()), (json!(["S", "L"]), json!("L")), "only M is gone");
    for widget in [0, 3, -1] {
        let r = a.call("form_delete_field", &json!({ "doc": doc, "field": "size", "widget": widget }));
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "widget {widget}: {r:?}");
    }
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(size(&mut a)["options"], json!(["S", "M", "L"]), "one undo step brings it back");
}

#[test]
fn changing_a_lists_options_through_tools_keeps_its_saved_selection_on_the_same_items() {
    let dir = workdir("choice-index");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let args = json!({ "doc": doc, "page": 1, "type": "list", "rect": [20, 60, 180, 140], "options": ["A", "B", "C"], "multi_select": true, "name": "letters" });
    ok(&mut a, "form_add_field", args);
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "letters": ["B", "C"] } }));
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "letters", "options": ["B", "C", "D"] }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "letters.pdf" }));
    assert_eq!(saved_selection_indices(&dir.join("letters.pdf"), "letters"), Some(vec![0, 1]), "B and C are now items 0 and 1");
}

/// Clearing a field's length limit through `form_set_props` (`"max_length": 0`) leaves it with
/// none, even when its parent field has one it would otherwise inherit.
#[test]
fn clearing_a_kids_length_limit_through_tools_does_not_bring_back_its_parents() {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 400 300] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [6 0 R] >>",
        "<< /Fields [5 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >>",
        "<< /T (address) /FT /Tx /MaxLen 3 /Kids [6 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /T (city) /Parent 5 0 R /MaxLen 8 /Rect [10 250 150 268] /P 3 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    ];
    let dir = workdir("kid-max-len");
    std::fs::write(dir.join("form.pdf"), pdf_of(&objs)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "address.city", "max_length": 0 }));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "address.city": "Absalom" } }));
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let city = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "address.city").unwrap().clone();
    assert!(city.get("max_length").is_none(), "no limit, not the parent's 3: {city}");
    assert_eq!(city["value"], "Absalom", "the value is not cut to the parent's 3 characters");
}

/// Clearing a field's default through `form_set_props` (`"default": ""`) leaves it with none,
/// even when its parent field has one it would otherwise inherit.
#[test]
fn clearing_a_kids_default_through_tools_does_not_bring_back_its_parents() {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 400 300] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [6 0 R] >>",
        "<< /Fields [5 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >>",
        "<< /T (address) /FT /Tx /DV (Main St) /Kids [6 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /T (city) /Parent 5 0 R /DV (Elm) /V (Elm) /Rect [10 250 150 268] /P 3 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    ];
    let dir = workdir("kid-default");
    std::fs::write(dir.join("form.pdf"), pdf_of(&objs)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_set_props", json!({ "doc": doc, "field": "address.city", "default": "" }));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "address.city": "Oak" } }));
    ok(&mut a, "form_reset", json!({ "doc": doc }));
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let city = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "address.city").unwrap().clone();
    assert!(city["value"].is_null() || city["value"] == "", "reset clears it rather than restoring the parent's Main St: {}", city["value"]);
}

/// #2517, through the same tools as the report. One blank line is two paragraphs, and editing
/// the first leaves the second. The neighbour at y=656.8 is one blank line below (two 14.4 pt
/// leadings under the second row). Growing the first paragraph, saving, reopening and editing
/// again leaves that neighbour in place. An explicit TL of 27.6 pt stays one paragraph.
#[test]
fn a_blank_line_stays_a_separate_paragraph_through_tools() {
    let dir = workdir("blank-line");
    let body = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (First paragraph, first row.) Tj T* (First paragraph, second row.) Tj ET \
                BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Second paragraph, first row.) Tj T* (Second paragraph, second row.) Tj ET";
    std::fs::write(dir.join("a.pdf"), helvetica_page(body)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 1 }));
    assert_eq!(before["count"], 2, "{before}");
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": "First paragraph, edited row.\nFirst paragraph, second row." }));
    let after = ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 1 }));
    assert_eq!(after["count"], 2, "{after}");
    let second = after["paragraphs"][1]["text"].as_str().unwrap();
    assert!(second.contains("Second paragraph"), "{second}");

    let grown = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (One) Tj T* (Two) Tj ET \
                 BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbour) Tj ET";
    std::fs::write(dir.join("grown.pdf"), helvetica_page(grown)).unwrap();
    let grown_doc = ok(&mut a, "doc_open", json!({ "path": "grown.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_paragraphs", json!({ "doc": grown_doc, "page": 1 }))["count"], 2);
    ok(&mut a, "text_edit", json!({ "doc": grown_doc, "page": 1, "paragraph": 1, "text": "One\nTwo\nThree" }));
    ok(&mut a, "doc_save", json!({ "doc": grown_doc, "path": "grown-out.pdf" }));
    let saved = ok(&mut a, "doc_open", json!({ "path": "grown-out.pdf" }))["doc"].as_u64().unwrap();
    let after = ok(&mut a, "text_paragraphs", json!({ "doc": saved, "page": 1 }));
    assert_eq!(after["count"], 2, "{after}");
    assert!(after["paragraphs"][1]["text"].as_str().unwrap().contains("Neighbour"), "{after}");
    ok(&mut a, "text_edit", json!({ "doc": saved, "page": 1, "paragraph": 1, "text": "One\nTwo\nFour" }));
    let after = ok(&mut a, "text_paragraphs", json!({ "doc": saved, "page": 1 }));
    assert_eq!(after["count"], 2, "{after}");
    assert!(after["paragraphs"][1]["text"].as_str().unwrap().contains("Neighbour"), "{after}");

    let spaced = "BT /F1 12 Tf 27.6 TL 1 0 0 1 72 700 Tm (One) Tj T* (Two) Tj T* (Three) Tj ET";
    std::fs::write(dir.join("spaced.pdf"), helvetica_page(spaced)).unwrap();
    let spaced_doc = ok(&mut a, "doc_open", json!({ "path": "spaced.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_paragraphs", json!({ "doc": spaced_doc, "page": 1 }))["count"], 1);
}

/// #2529, through the same tools as a width drag. The typed double space falls on the wrap.
/// Omitting `text` reflows with the saved gap count. An incremental save and a full rewrite
/// both still have both spaces after the box is widened again.
#[test]
fn narrowing_then_widening_keeps_a_typed_double_space_through_tools() {
    let dir = workdir("double-space-width");
    std::fs::write(dir.join("a.pdf"), helvetica_page("BT /F1 24 Tf 20 150 Td (One  Two) Tj ET")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 1 }))["paragraphs"][0]["editor_text"], "One  Two");
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "width": 45.0 }));
    let narrow = &ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 1 }))["paragraphs"][0];
    assert_eq!(narrow["editor_text"], "One\nTwo", "{narrow}");
    for (name, full) in [("narrow-inc.pdf", false), ("narrow-full.pdf", true)] {
        ok(&mut a, "doc_save", json!({ "doc": doc, "path": name, "full": full }));
        let saved = ok(&mut a, "doc_open", json!({ "path": name }))["doc"].as_u64().unwrap();
        let block = &ok(&mut a, "text_paragraphs", json!({ "doc": saved, "page": 1 }))["paragraphs"][0];
        assert_eq!(block["editor_text"], "One\nTwo", "{name}: {block}");
        ok(&mut a, "text_edit", json!({ "doc": saved, "page": 1, "paragraph": 1, "width": 200.0 }));
        let wide_name = format!("wide-{name}");
        ok(&mut a, "doc_save", json!({ "doc": saved, "path": wide_name, "full": full }));
        let wide = ok(&mut a, "doc_open", json!({ "path": wide_name }))["doc"].as_u64().unwrap();
        let block = &ok(&mut a, "text_paragraphs", json!({ "doc": wide, "page": 1 }))["paragraphs"][0];
        assert_eq!(block["editor_text"], "One  Two", "{name}: the second typed space was lost: {block}");
    }
}

/// The reported edits, through open / text_paragraphs / text_edit / save / reopen / text_extract.
/// Covers an explicit TL, the three ways a file moves rows without TL, a blank line written with
/// Td, a two-row file that must stay split, and a partial rewrite that adds a row.
#[test]
fn paragraph_grouping_survives_save_and_reopen_through_tools() {
    let dir = workdir("paragraph-grouping");
    let mut a = auto(&dir);
    for step in [27.6_f64, 28.8] {
        let y2 = 700.0 - step;
        let y3 = 700.0 - 2.0 * step;
        let bodies = [
            format!("BT /F1 12 Tf 1 0 0 1 72 700 Tm (One) Tj 0 -{step} Td (Two) Tj 0 -{step} Td (Three) Tj ET"),
            format!(
                "BT /F1 12 Tf 1 0 0 1 72 700 Tm (One) Tj ET BT /F1 12 Tf 1 0 0 1 72 {y2} Tm (Two) Tj ET BT /F1 12 Tf 1 0 0 1 72 {y3} Tm (Three) Tj ET"
            ),
            format!("BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {y2} Td (Two) Tj ET BT /F1 12 Tf 72 {y3} Td (Three) Tj ET"),
        ];
        for (i, body) in bodies.iter().enumerate() {
            let path = format!("step-{step}-{i}.pdf");
            std::fs::write(dir.join(&path), helvetica_page(body)).unwrap();
            let doc = ok(&mut a, "doc_open", json!({ "path": path }))["doc"].as_u64().unwrap();
            let paras = paragraph_texts(&mut a, doc);
            assert_eq!(paras.len(), 1, "{path}: {paras:?}");
            assert!(paras[0].contains("One") && paras[0].contains("Two") && paras[0].contains("Three"), "{path}: {paras:?}");
            let extracted = page_text(&mut a, doc);
            assert!(extracted[0].contains("One") && extracted[0].contains("Three"), "{path}: {extracted:?}");
        }
    }

    // One gap and no third row: still two paragraphs.
    let pair = format!("BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {} Td (Two) Tj ET", 700.0 - 27.6);
    std::fs::write(dir.join("pair.pdf"), helvetica_page(&pair)).unwrap();
    let pair_doc = ok(&mut a, "doc_open", json!({ "path": "pair.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, pair_doc).len(), 2);

    // Blank line written with Td, not TL. The inner step repeats; the break does not.
    let inner = 14.4_f64;
    let blank = format!(
        "BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {} Td (Two) Tj ET \
         BT /F1 12 Tf 72 {} Td (Neighbour) Tj ET BT /F1 12 Tf 72 {} Td (Line) Tj ET",
        700.0 - inner,
        700.0 - 3.0 * inner,
        700.0 - 4.0 * inner
    );
    std::fs::write(dir.join("blank.pdf"), helvetica_page(&blank)).unwrap();
    let blank_doc = ok(&mut a, "doc_open", json!({ "path": "blank.pdf" }))["doc"].as_u64().unwrap();
    let blank_paras = paragraph_texts(&mut a, blank_doc);
    assert_eq!(blank_paras.len(), 2, "{blank_paras:?}");
    assert!(blank_paras[0].contains("One") && blank_paras[0].contains("Two"), "{blank_paras:?}");
    assert!(blank_paras[1].contains("Neighbour") && blank_paras[1].contains("Line"), "{blank_paras:?}");

    // The first row is unchanged. Save incremental, reopen, edit again, save in full, reopen.
    let partial = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
                   BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET";
    std::fs::write(dir.join("partial.pdf"), helvetica_page(partial)).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "partial.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, doc).len(), 2);
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": "Alpha first row.\nTwo\nThree" }));
    let incremental = ok(&mut a, "doc_save", json!({ "doc": doc, "path": "partial.pdf" }));
    assert_eq!(incremental["incremental"], true, "{incremental}");
    let reopened = ok(&mut a, "doc_open", json!({ "path": "partial.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, reopened), ["Alpha first row. Two Three", "Neighbor stays."]);
    ok(&mut a, "text_edit", json!({ "doc": reopened, "page": 1, "paragraph": 1, "text": "Alpha first row.\nTwo\nFour" }));
    let full = ok(&mut a, "doc_save", json!({ "doc": reopened, "path": "partial-full.pdf", "full": true }));
    assert_eq!(full["incremental"], false, "{full}");
    let again = ok(&mut a, "doc_open", json!({ "path": "partial-full.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, again), ["Alpha first row. Two Four", "Neighbor stays."]);
    let extracted = page_text(&mut a, again);
    assert!(extracted[0].contains("Neighbor stays.") && extracted[0].contains("Four"), "{extracted:?}");
}

/// Editing the only paragraph on the page, while leaving its second row in the source, stays
/// one paragraph through save and reopen. A neighbour in a different marked region stays in
/// its own paragraph.
#[test]
fn a_partial_rewrite_of_the_last_paragraph_stays_one_through_tools() {
    let dir = workdir("paragraph-tail");
    let mut a = auto(&dir);
    let only = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET";
    std::fs::write(dir.join("only.pdf"), helvetica_page(only)).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "only.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, doc), ["Alpha first row. Alpha next row."]);
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": "One.\nAlpha next row." }));
    assert_eq!(paragraph_texts(&mut a, doc), ["One. Alpha next row."]);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "only.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "only.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, reopened), ["One. Alpha next row."]);
    ok(&mut a, "text_edit", json!({ "doc": reopened, "page": 1, "paragraph": 1, "text": "One.\nTwo." }));
    assert_eq!(paragraph_texts(&mut a, reopened), ["One. Two."]);

    let marked = "/H1 << /MCID 0 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET EMC \
                  /P << /MCID 1 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET EMC";
    std::fs::write(dir.join("marked.pdf"), helvetica_page(marked)).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "marked.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, doc).len(), 2);
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": "One.\nAlpha next row." }));
    assert_eq!(paragraph_texts(&mut a, doc), ["One. Alpha next row.", "Neighbor stays."]);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "marked.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "marked.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, reopened), ["One. Alpha next row.", "Neighbor stays."]);
    ok(&mut a, "text_edit", json!({ "doc": reopened, "page": 1, "paragraph": 1, "text": "One.\nTwo." }));
    assert_eq!(paragraph_texts(&mut a, reopened), ["One. Two.", "Neighbor stays."]);
}

/// Both ends of one text object, through open / text_edit / save / reopen / text_extract.
/// Editing the last paragraph must not take the one before it, and editing the first must
/// not take the one after it. Incremental and full saves.
#[test]
fn a_shared_text_object_keeps_both_paragraphs_through_tools() {
    let dir = workdir("shared-text-object");
    let mut a = auto(&dir);
    let body =
        "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj T* T* (Neighbor stays.) Tj T* (Neighbor next.) Tj ET";
    std::fs::write(dir.join("last.pdf"), helvetica_page(body)).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "last.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, doc), ["Alpha first row. Alpha next row.", "Neighbor stays. Neighbor next."]);
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 2, "text": "Neighbor stays.\nChanged." }));
    assert_eq!(paragraph_texts(&mut a, doc), ["Alpha first row. Alpha next row.", "Neighbor stays. Changed."]);
    let incremental = ok(&mut a, "doc_save", json!({ "doc": doc, "path": "last.pdf" }));
    assert_eq!(incremental["incremental"], true, "{incremental}");
    let reopened = ok(&mut a, "doc_open", json!({ "path": "last.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, reopened), ["Alpha first row. Alpha next row.", "Neighbor stays. Changed."]);
    ok(&mut a, "text_edit", json!({ "doc": reopened, "page": 1, "paragraph": 2, "text": "Neighbor stays.\nEdited." }));
    let full = ok(&mut a, "doc_save", json!({ "doc": reopened, "path": "last-full.pdf", "full": true }));
    assert_eq!(full["incremental"], false, "{full}");
    let again = ok(&mut a, "doc_open", json!({ "path": "last-full.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, again), ["Alpha first row. Alpha next row.", "Neighbor stays. Edited."]);
    let extracted = page_text(&mut a, again);
    assert!(extracted[0].contains("Alpha first row.") && extracted[0].contains("Edited."), "{extracted:?}");

    std::fs::write(dir.join("first.pdf"), helvetica_page(body)).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "first.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": "Alpha first row.\nTwo\nThree" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "first.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "first.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, reopened), ["Alpha first row. Two Three", "Neighbor stays. Neighbor next."]);
    ok(&mut a, "text_edit", json!({ "doc": reopened, "page": 1, "paragraph": 1, "text": "Alpha first row.\nTwo\nFour" }));
    ok(&mut a, "doc_save", json!({ "doc": reopened, "path": "first-full.pdf", "full": true }));
    let again = ok(&mut a, "doc_open", json!({ "path": "first-full.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(paragraph_texts(&mut a, again), ["Alpha first row. Two Four", "Neighbor stays. Neighbor next."]);
    let extracted = page_text(&mut a, again);
    assert!(extracted[0].contains("Four") && extracted[0].contains("Neighbor stays."), "{extracted:?}");
}

/// Reviewing headlessly: corrections keep the boxes, a stale or foreign review is refused, and
/// applying consumes the handle (the pages already accepted were applied once).
#[test]
fn ocr_review_corrections_keep_boxes_are_guarded_and_apply_once() {
    fn render(a: &mut Automation, doc: u64) -> Vec<u8> {
        let mut result = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
        match result.remove(0) {
            Content::Png { data, .. } => data,
            other => panic!("expected a rendered page, got {other:?}"),
        }
    }
    let dir = workdir("ocr-review");
    let mut a = auto(&dir);
    if ok(&mut a, "ocr_status", json!({}))["available"] != true {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Review these original words" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 150, "pages": [1] }));
    let png = std::fs::read_dir(dir.join("scan")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    let before = render(&mut a, scan);
    let found = ok(&mut a, "ocr_words", json!({ "doc": scan, "retain_review": true }));
    assert!(found["words"].as_u64().unwrap() >= 3, "{found}");
    let review = found["review"].as_u64().unwrap();
    assert!(page_text(&mut a, scan).iter().all(String::is_empty));
    for args in [
        json!({ "doc": text, "review": review }),
        json!({ "doc": scan, "review": review, "corrections": [{"page":1,"word":999999,"text":"bad"}] }),
        json!({ "doc": scan, "review": review, "corrections": [{"page":1,"word":1,"text":"bad","origin":[0,0]}] }),
        json!({ "doc": scan, "review": review, "corrections": [{"page":1,"word":1,"text":"a".repeat(4*1024*1024+1)}] }),
        json!({ "doc": scan, "review": review, "corrections": [{"page":1,"word":1,"text":"a"},{"page":1,"word":1,"text":"b"}] }),
    ] {
        assert!(a.call("ocr_review_apply", &args).is_err());
        assert!(page_text(&mut a, scan).iter().all(String::is_empty));
    }
    let result = ok(
        &mut a,
        "ocr_review_apply",
        json!({ "doc": scan, "review": review, "corrections": [{"page":1,"word":1,"text":"日本語"},{"page":1,"word":2,"text":""}] }),
    );
    assert!(result["applied"].as_u64().unwrap() >= 2);
    let extracted = page_text(&mut a, scan).join("\n");
    assert!(extracted.contains("日本語"), "{extracted}");
    assert_eq!(render(&mut a, scan), before, "corrected text remains invisible at its original geometry");
    assert!(a.call("ocr_review_apply", &json!({ "doc": scan, "review": review })).is_err(), "consumed reviews cannot be replayed");
    ok(&mut a, "edit_undo", json!({ "doc": scan }));
    assert!(page_text(&mut a, scan).iter().all(String::is_empty));
    assert!(a.call("edit_undo", &json!({ "doc": scan })).is_err(), "review acceptance is one undo step");

    let found = ok(&mut a, "ocr_words", json!({ "doc": scan, "retain_review": true }));
    let stale = found["review"].as_u64().unwrap();
    ok(&mut a, "page_rotate", json!({ "doc": scan, "pages": [1], "degrees": 90 }));
    let error = a.call("ocr_review_apply", &json!({ "doc": scan, "review": stale })).unwrap_err().to_string();
    assert!(error.contains("changed since recognition"), "{error}");
    assert!(page_text(&mut a, scan).iter().all(String::is_empty));
    ok(&mut a, "ocr_review_apply", json!({ "doc": scan, "review": stale, "discard": true }));
    assert!(a.call("ocr_review_apply", &json!({ "doc": scan, "review": stale })).is_err());
    ok(&mut a, "edit_undo", json!({ "doc": scan }));

    let first = ok(&mut a, "ocr_words", json!({ "doc": scan, "retain_review": true }))["review"].as_u64().unwrap();
    let second = ok(&mut a, "ocr_words", json!({ "doc": scan, "retain_review": true }))["review"].as_u64().unwrap();
    assert_ne!(first, second);
    assert!(a.call("ocr_review_apply", &json!({ "doc": scan, "review": first })).is_err());
    ok(
        &mut a,
        "ocr_review_apply",
        json!({ "doc": scan, "review": second, "output_mode": "editable", "out": "reviewed.pdf", "corrections": [{"page":1,"word":1,"text":"Corrected"}] }),
    );
    assert!(page_text(&mut a, scan).iter().all(String::is_empty));
    let output = ok(&mut a, "doc_open", json!({ "path": "reviewed.pdf" }))["doc"].as_u64().unwrap();
    assert!(page_text(&mut a, output).join("\n").contains("Corrected"));
    // The editable output is a new file: an existing one is refused and the review is kept.
    let third = ok(&mut a, "ocr_words", json!({ "doc": scan, "retain_review": true }))["review"].as_u64().unwrap();
    let again = a.call("ocr_review_apply", &json!({ "doc": scan, "review": third, "output_mode": "editable", "out": "reviewed.pdf" }));
    assert!(matches!(&again, Err(e) if e.to_string().contains("already exists")), "{again:?}");
    ok(&mut a, "ocr_review_apply", json!({ "doc": scan, "review": third, "discard": true }));
}

#[test]
fn ocr_dictionary_updates_are_atomic_bounded_and_confined() {
    let dir = workdir("ocr-dictionary");
    let mut a = auto(&dir);
    let path = "ocr-dictionary-en.txt";
    let out = ok(&mut a, "ocr_dictionary", json!({ "path": path, "add": ["Quartz", "quartz", "日本語"] }));
    assert_eq!(out["count"], 2);
    assert_eq!(pdfcraft_engine::ocr::dictionary::words(&std::fs::read_to_string(dir.join(path)).unwrap()).unwrap().len(), 2);
    let before = std::fs::read(dir.join(path)).unwrap();
    for args in [
        json!({ "path": path, "add": ["a\nb"] }),
        json!({ "path": path, "add": ["x".repeat(1025)] }),
        json!({ "path": path, "add": ["İ".repeat(512)] }),
        json!({ "path": path, "add": vec!["x"; 65537] }),
        json!({ "path": "../outside-ocr-dictionary.txt", "add": ["x"] }),
    ] {
        assert!(a.call("ocr_dictionary", &args).is_err());
    }
    assert_eq!(std::fs::read(dir.join(path)).unwrap(), before);
    assert_eq!(ok(&mut a, "ocr_dictionary", json!({ "path": path, "remove": ["QUARTZ"] }))["words"], json!(["日本語"]));
    assert_eq!(ok(&mut a, "ocr_dictionary", json!({ "path": path }))["count"], 1);
    std::fs::write(dir.join("ocr-dictionary-large.txt"), vec![b'a'; 4 * 1024 * 1024 + 1]).unwrap();
    let too_big = a.call("ocr_dictionary", &json!({ "path": "ocr-dictionary-large.txt", "add": ["x"] })).unwrap_err().to_string();
    assert!(!too_big.contains("must name a dictionary file"), "refused for its size, not its name: {too_big}");
    assert_eq!(std::fs::metadata(dir.join("ocr-dictionary-large.txt")).unwrap().len(), 4 * 1024 * 1024 + 1);
}

/// The tool rewrites what it is pointed at (lower-cased, sorted), so it only takes dictionary file
/// names: another small text file under the root is neither read nor changed.
#[test]
fn ocr_dictionary_refuses_files_that_are_not_dictionaries() {
    let dir = workdir("ocr-dictionary-names");
    let mut a = auto(&dir);
    let env = b"TOKEN=Zebra
APPLE=1
";
    for name in [".env", "notes.txt", "ocr-dictionary-en.md", "ocr-dictionary-e n.txt", "sub/readme.txt"] {
        if let Some(parent) = std::path::Path::new(name).parent() {
            std::fs::create_dir_all(dir.join(parent)).unwrap();
        }
        std::fs::write(dir.join(name), env).unwrap();
        for args in [json!({ "path": name }), json!({ "path": name, "add": [] }), json!({ "path": name, "add": ["x"] })] {
            let err = a.call("ocr_dictionary", &args).unwrap_err().to_string();
            assert!(err.contains("must name a dictionary file"), "{name} {args}: {err}");
        }
        assert_eq!(std::fs::read(dir.join(name)).unwrap(), env, "{name} is untouched");
    }
}

#[test]
fn ocr_retained_reviews_have_eight_document_cap_and_close_releases_slot() {
    let dir = workdir("ocr-review-cap");
    let mut a = auto(&dir);
    if ok(&mut a, "ocr_status", json!({}))["available"] != true {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut docs = Vec::new();
    let mut handles = Vec::new();
    for _ in 0..9 {
        let doc = ok(&mut a, "doc_create", json!({"from":"text", "text":"Already recognized text"}))["doc"].as_u64().unwrap();
        docs.push(doc);
    }
    for doc in docs.iter().take(8) {
        handles.push(ok(&mut a, "ocr_words", json!({"doc":doc,"retain_review":true}))["review"].as_u64().unwrap());
    }
    let error = a.call("ocr_words", &json!({"doc":docs[8],"retain_review":true})).unwrap_err().to_string();
    assert!(error.contains("at most 8"), "{error}");
    let replacement = ok(&mut a, "ocr_words", json!({"doc":docs[0],"retain_review":true}))["review"].as_u64().unwrap();
    assert_ne!(replacement, handles[0]);
    assert!(a.call("ocr_review_apply", &json!({"doc":docs[0],"review":handles[0],"discard":true})).is_err());
    ok(&mut a, "doc_close", json!({"doc":docs[0],"discard_changes":true}));
    assert!(a.call("ocr_review_apply", &json!({"doc":docs[0],"review":replacement})).is_err());
    let final_handle = ok(&mut a, "ocr_words", json!({"doc":docs[8],"retain_review":true}))["review"].as_u64().unwrap();
    ok(&mut a, "ocr_review_apply", json!({"doc":docs[8],"review":final_handle,"discard":true}));
}

#[test]
fn editor_crop_group_move_and_arrange_survive_added_item_deletion_through_tools() {
    let dir = workdir("editor-crop-group-arrange");
    let mut a = auto(&dir);
    let source = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({"doc":source,"folder":"src","dpi":18,"pages":[1]}));
    let doc = ok(&mut a, "doc_create", json!({"from":"images","paths":["src/a_page_1.png"]}))["doc"].as_u64().unwrap();
    ok(&mut a, "image_edit", json!({"doc":doc,"page":1,"image":1,"action":"move","rect":[5,5,25,35]}));
    ok(&mut a, "image_edit", json!({"doc":doc,"page":1,"image":1,"action":"crop","rect":[10,10,20,30]}));
    ok(&mut a, "page_add_text", json!({"doc":doc,"page":1,"text":"Movable label","rect":[2,40,45,55],"size":6}));
    let inventory = ok(&mut a, "object_list", json!({"doc":doc,"page":1}));
    let objects: Vec<_> = inventory["objects"].as_array().unwrap().iter().map(|o| json!({"kind":o["kind"],"index":o["index"]})).collect();
    assert_eq!(objects.len(), 2);
    let moved = ok(&mut a, "object_move", json!({"doc":doc,"page":1,"objects":objects,"offset":[3,4]}));
    assert_eq!(moved["moved"], 2);
    let picture = ok(&mut a, "page_images", json!({"doc":doc,"page":1}))["images"][0].clone();
    assert_eq!(picture["rect"], json!([8.0, 9.0, 28.0, 39.0]));
    assert_eq!(picture["visible_rect"], json!([13.0, 14.0, 23.0, 34.0]));
    assert!(page_text(&mut a, doc)[0].contains("Movable label"));
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_eq!(ok(&mut a, "page_images", json!({"doc":doc,"page":1}))["images"][0]["visible_rect"], json!([10.0, 10.0, 20.0, 30.0]));
    ok(&mut a, "edit_redo", json!({"doc":doc}));
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"grouped.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path":"grouped.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "page_images", json!({"doc":reopened,"page":1}))["images"][0]["visible_rect"], picture["visible_rect"]);
    ok(&mut a, "image_edit", json!({"doc":reopened,"page":1,"image":1,"action":"bring_to_front"}));
    assert_eq!(ok(&mut a, "content_list", json!({"doc":reopened}))["count"], 1);
    ok(&mut a, "content_delete", json!({"doc":reopened,"page":1,"index":1}));
    ok(&mut a, "doc_save", json!({"doc":reopened,"path":"independent.pdf"}));
    let saved = ok(&mut a, "doc_open", json!({"path":"independent.pdf"}))["doc"].as_u64().unwrap();
    let after = ok(&mut a, "page_images", json!({"doc":saved,"page":1}));
    assert_eq!(after["count"], 1);
    assert_eq!(after["images"][0]["rect"], picture["rect"]);
    assert_eq!(after["images"][0]["visible_rect"], picture["visible_rect"]);
    let rendered = a.call("page_render", &json!({"doc":saved,"page":1,"dpi":72})).unwrap();
    assert!(rendered.iter().any(|c| matches!(c, Content::Png {data,width,height} if !data.is_empty() && *width > 0 && *height > 0)));
}

#[test]
fn image_flip_tools_follow_displayed_axes_on_every_page_rotation() {
    fn corners(a: &Automation, id: u64) -> [[f64; 2]; 4] {
        let doc = a.session().get(pdfcraft_engine::DocId(id)).unwrap();
        let [aa, b, c, d, e, f] = doc.page_images(0)[0].matrix;
        [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .map(|(x, y)| doc.info.pages[0].user_to_view((aa * x + c * y + e) as f32, (b * x + d * y + f) as f32).map(f64::from))
    }
    fn raster(a: &mut Automation, id: u64) -> (usize, usize, Vec<u8>) {
        let rendered = a.call("page_render", &json!({"doc":id,"page":1,"dpi":72})).unwrap();
        let Content::Png { data, .. } = &rendered[0] else { panic!("expected PNG") };
        let mut reader = png::Decoder::new(std::io::Cursor::new(data)).read_info().unwrap();
        let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut bytes).unwrap();
        bytes.truncate(frame.buffer_size());
        if frame.color_type == png::ColorType::Rgb {
            bytes = bytes.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
        } else {
            assert_eq!(frame.color_type, png::ColorType::Rgba);
        }
        (frame.width as usize, frame.height as usize, bytes)
    }
    let dir = workdir("displayed-image-flip");
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 8, 4);
        encoder.set_color(png::ColorType::Rgb);
        let colors = [[240, 20, 30], [20, 230, 40], [30, 40, 220], [220, 210, 20]];
        let pixels: Vec<u8> = (0..4).flat_map(|y| (0..8).flat_map(move |x| colors[usize::from(x >= 4) + 2 * usize::from(y >= 2)])).collect();
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    std::fs::write(dir.join("quadrants.png"), png_bytes).unwrap();
    let mut a = auto(&dir);
    let mut failures = Vec::new();
    for rotation in [0, 90, 180, 270] {
        let doc = ok(&mut a, "doc_create", json!({"from":"images","paths":["quadrants.png"]}))["doc"].as_u64().unwrap();
        if rotation != 0 {
            ok(&mut a, "page_rotate", json!({"doc":doc,"pages":[1],"degrees":rotation}));
        }
        let before = corners(&a, doc);
        let (width, height, pixels) = raster(&mut a, doc);
        let xmin = before.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let xmax = before.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let ymin = before.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let ymax = before.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        for (action, horizontal) in [("flip_horizontal", true), ("flip_vertical", false)] {
            ok(&mut a, "image_edit", json!({"doc":doc,"page":1,"image":1,"action":action}));
            let after = corners(&a, doc);
            for (old, new) in before.into_iter().zip(after) {
                let expected = if horizontal { [xmin + xmax - old[0], old[1]] } else { [old[0], ymin + ymax - old[1]] };
                if new.into_iter().zip(expected).any(|(actual, want)| (actual - want).abs() > 0.001) {
                    failures.push(format!("rotation {rotation} {action}: corner {old:?} became {new:?}, expected {expected:?}"));
                    break;
                }
            }
            let (new_width, new_height, actual_pixels) = raster(&mut a, doc);
            assert_eq!((new_width, new_height), (width, height));
            let mut expected_pixels = Vec::new();
            for y in 0..height {
                for x in 0..width {
                    let source = if horizontal { (y * width + width - 1 - x) * 4 } else { ((height - 1 - y) * width + x) * 4 };
                    expected_pixels.extend_from_slice(&pixels[source..source + 4]);
                }
            }
            if actual_pixels.iter().zip(&expected_pixels).any(|(actual, want)| actual.abs_diff(*want) > 1) {
                failures.push(format!("rotation {rotation} {action}: raster flipped the wrong displayed axis"));
            }
            assert_ne!(actual_pixels, pixels, "the asymmetric image makes both flip axes observable");
            ok(&mut a, "edit_undo", json!({"doc":doc}));
            assert_eq!(corners(&a, doc), before, "one undo restores the original placement");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn mixed_script_tools_normalize_but_undo_restores_the_exact_mixture() {
    let dir = workdir("mixed-script-policy");
    let mut a = auto(&dir);
    for (script, mixed, uniform, plain) in [("superscript", "Q²2", "Q²²", "Q22"), ("subscript", "H₂2", "H₂₂", "H22")] {
        let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
        ok(&mut a, "text_edit", json!({"doc":doc,"page":1,"paragraph":1,"text":mixed,"size":14,"width":160}));
        let paragraph =
            |a: &mut Automation| ok(a, "text_paragraphs", json!({"doc":doc,"page":1}))["paragraphs"][0]["editor_text"].as_str().unwrap().to_owned();
        assert_eq!(paragraph(&mut a), mixed);
        let before = a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes.clone();
        let args = json!({"doc":doc,"page":1,"paragraph":1,"script":script});
        let once = ok(&mut a, "text_script", args.clone());
        assert_eq!(once["paragraph"]["text"], uniform);
        assert_eq!(paragraph(&mut a), uniform);
        ok(&mut a, "text_script", args);
        assert_eq!(paragraph(&mut a), plain);
        assert_ne!(plain, mixed, "normalization does not promise to recreate the original mixture");
        ok(&mut a, "edit_undo", json!({"doc":doc}));
        assert_eq!(paragraph(&mut a), uniform, "one Undo restores the preceding scripted text");
        ok(&mut a, "edit_undo", json!({"doc":doc}));
        assert_eq!(paragraph(&mut a), mixed, "Undo restores the exact initial Unicode mixture");
        assert_eq!(
            a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes.as_ref(),
            before.as_ref(),
            "Undo also restores the exact prior document bytes"
        );
        ok(&mut a, "edit_redo", json!({"doc":doc}));
        assert_eq!(paragraph(&mut a), uniform);
        ok(&mut a, "edit_redo", json!({"doc":doc}));
        assert_eq!(paragraph(&mut a), plain);
        let path = format!("mixed-{script}.pdf");
        ok(&mut a, "doc_save", json!({"doc":doc,"path":path}));
        let reopened = ok(&mut a, "doc_open", json!({"path":path}))["doc"].as_u64().unwrap();
        assert_eq!(page_text(&mut a, reopened)[0], plain);
    }
}

#[test]
fn multiline_fill_sign_engine_round_trip_preserves_lines_and_appearance() {
    let dir = workdir("fill-sign-multiline");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    let text = "\nAda\n\nLovelace\n";
    ok(&mut a, "fill_sign_add", json!({"doc":doc,"page":1,"type":"text","at":[20,20],"text":text}));
    let id = pdfcraft_engine::DocId(doc);
    let annot = &a.session().get(id).unwrap().info.annotations[0];
    assert_eq!(annot.contents.as_deref(), Some(text), "typewriter contents keep outer line breaks");
    assert!(annot.rect[3] - annot.rect[1] >= 66.0, "five text lines need their full height, including blank lines: {:?}", annot.rect);
    let pixels = a.render_pixels(doc, 1, 1.0).unwrap();
    // The fixture's page text is at y=150; only the typewriter draws at y=65..80.
    assert!(
        (65..80).any(|y| (20..80).any(|x| {
            let i = (y * pixels.0 as usize + x) * 4;
            pixels.2[i..i + 3].iter().all(|v| *v < 100)
        })),
        "the lower nonempty line appears inside its box"
    );
    let rendered = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
    ok(&mut a, "doc_save", json!({"doc":doc,"path":"multiline.pdf"}));
    let mut re = auto(&dir);
    let reopened = ok(&mut re, "doc_open", json!({"path":"multiline.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(re.session().get(pdfcraft_engine::DocId(reopened)).unwrap().info.annotations[0].contents.as_deref(), Some(text));
    assert_eq!(rendered, re.call("page_render", &json!({"doc":reopened,"page":1,"dpi":72})).unwrap(), "saved appearances are identical");
    std::fs::remove_dir_all(dir).unwrap();
}

/// Export, then import into another document and copy between open documents: the tree comes
/// back unchanged, a page the target lacks is reported, and the import undoes in one step.
#[test]
fn bookmarks_export_import_and_copy_between_documents() {
    let dir = workdir("bookmark-io");
    let mut a = auto(&dir);
    let src = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "bookmark_add", json!({ "doc": src, "title": "Intro", "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": src, "title": "Detail", "page": 3, "parent": [1] }));
    ok(&mut a, "bookmark_add", json!({ "doc": src, "title": "End", "page": 2 }));
    ok(&mut a, "bookmark_set_style", json!({ "doc": src, "path": [1], "color": [1, 0, 0], "bold": true }));
    let exported = ok(&mut a, "bookmark_export", json!({ "doc": src, "out": "marks.json" }));
    assert_eq!(exported["truncated"], false);
    let tree = exported["bookmarks"].clone();
    assert_eq!((tree[0]["title"].clone(), tree[0]["color"].clone(), tree[0]["bold"].clone()), (json!("Intro"), json!([1.0, 0.0, 0.0]), json!(true)));
    assert_eq!((tree[0]["children"][0]["title"].clone(), tree[0]["children"][0]["page"].clone()), (json!("Detail"), json!(3)));
    assert_eq!(tree[0]["view"]["type"], "XYZ");
    let subtree = ok(&mut a, "bookmark_export", json!({ "doc": src, "path": [1, 1] }))["bookmarks"].clone();
    assert_eq!(subtree, json!([tree[0]["children"][0]]));

    // b.pdf has two pages: Detail goes to the third, so it arrives without a destination.
    let dst = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    let imported = ok(&mut a, "bookmark_import", json!({ "doc": dst, "file": "marks.json" }));
    assert_eq!(imported["unmapped"], json!([[1, 1]]));
    let back = ok(&mut a, "bookmark_export", json!({ "doc": dst }))["bookmarks"].clone();
    assert_eq!(back[0]["children"][0]["page"], json!(null));
    assert_eq!((back[0]["title"].clone(), back[1].clone()), (tree[0]["title"].clone(), tree[1].clone()));
    ok(&mut a, "edit_undo", json!({ "doc": dst }));
    assert_eq!(ok(&mut a, "bookmark_list", json!({ "doc": dst }))["bookmarks"], json!([]));

    // Into a document with every page, the tree is unchanged, wherever it is copied from.
    let same = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "bookmark_add", json!({ "doc": same, "title": "Old", "page": 1 }));
    let copied = ok(&mut a, "bookmark_copy", json!({ "doc": same, "from": src, "replace": true }));
    assert_eq!(copied["unmapped"], json!([]));
    assert_eq!(ok(&mut a, "bookmark_export", json!({ "doc": same }))["bookmarks"], tree);
    ok(&mut a, "bookmark_import", json!({ "doc": same, "bookmarks": subtree, "parent": [2], "position": 1 }));
    assert_eq!(ok(&mut a, "bookmark_list", json!({ "doc": same }))["bookmarks"][1]["children"][0]["title"], "Detail");
}

/// What a real file holds and the JSON form can't say exactly still copies and round-trips: an
/// empty title stays empty, a short /FitR keeps its page and drops the view, and bookmarks past
/// the listing depth are reported as left out at every step.
#[test]
fn odd_bookmarks_round_trip_and_a_cut_tree_says_so() {
    let dir = workdir("bookmark-io-odd");
    std::fs::write(dir.join("odd.pdf"), odd_outline(0)).unwrap();
    std::fs::write(dir.join("deep.pdf"), odd_outline(40)).unwrap();
    let mut a = auto(&dir);
    let src = ok(&mut a, "doc_open", json!({ "path": "odd.pdf" }))["doc"].as_u64().unwrap();
    let tree = ok(&mut a, "bookmark_export", json!({ "doc": src, "out": "odd.json" }))["bookmarks"].clone();
    assert_eq!((tree[0]["title"].clone(), tree[1]["page"].clone(), tree[1].get("view")), (json!(""), json!(1), None));
    let dst = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "bookmark_import", json!({ "doc": dst, "file": "odd.json", "replace": true }))["truncated"], false);
    // A destination with no view is written as XYZ with nulls: the same page, the reader's view.
    let back = ok(&mut a, "bookmark_export", json!({ "doc": dst }))["bookmarks"].clone();
    assert_eq!((back[0].clone(), back[1]["title"].clone(), back[1]["page"].clone()), (tree[0].clone(), json!("Short"), json!(1)));
    assert_eq!(back[1]["view"], json!({ "type": "XYZ", "values": [null, null, null] }));
    let copied = ok(&mut a, "bookmark_copy", json!({ "doc": dst, "from": src, "replace": true }));
    assert_eq!((copied["unmapped"].clone(), copied["truncated"].clone()), (json!([]), json!(false)));
    match a.call("bookmark_export", &json!({ "doc": src, "path": [3] })) {
        Err(ToolError::Failed(m)) => assert!(m.contains("no bookmark at [3]"), "{m}"),
        other => panic!("expected no bookmark, got {other:?}"),
    }

    let deep = ok(&mut a, "doc_open", json!({ "path": "deep.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "bookmark_export", json!({ "doc": deep, "out": "deep.json" }))["truncated"], true);
    let saved: Value = serde_json::from_slice(&std::fs::read(dir.join("deep.json")).unwrap()).unwrap();
    assert_eq!(saved["truncated"], true);
    assert_eq!(ok(&mut a, "bookmark_import", json!({ "doc": dst, "file": "deep.json", "replace": true }))["truncated"], true);
    assert_eq!(ok(&mut a, "bookmark_copy", json!({ "doc": dst, "from": deep, "replace": true }))["truncated"], true);
    let export = tools().into_iter().find(|t| t.name == "bookmark_export").unwrap();
    assert!(export.destructive && !export.read_only, "bookmark_export overwrites out");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_misspelt_bookmark_key_is_refused_not_dropped() {
    let dir = workdir("bookmark-io-key");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let tree = json!([{ "title": "Parent", "children": [{ "title": "Red", "page": 1, "colour": [1, 0, 0] }] }]);
    match a.call("bookmark_import", &json!({ "doc": doc, "bookmarks": tree })) {
        Err(ToolError::InvalidArgs(m)) => assert!(m.contains("[1, 1]") && m.contains("\"colour\""), "{m}"),
        other => panic!("expected InvalidArgs, got {other:?}"),
    }
    assert_eq!(ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"], json!([]));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_bad_bookmark_tree_is_refused_whole_and_names_the_node() {
    let dir = workdir("bookmark-io-bad");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let refused = |a: &mut Automation, bookmarks: Value| match a.call("bookmark_import", &json!({ "doc": doc, "bookmarks": bookmarks })) {
        Err(ToolError::InvalidArgs(m)) => m,
        other => panic!("expected InvalidArgs, got {other:?}"),
    };
    let fine = json!({ "title": "Fine", "page": 1 });
    let m = refused(&mut a, json!([fine, { "title": "Parent", "children": [{ "page": 1 }] }]));
    assert!(m.contains("[2, 1]") && m.contains("title"), "{m}");
    let m = refused(&mut a, json!([{ "title": 5 }]));
    assert!(m.contains("[1]") && m.contains("title"), "{m}");
    let m = refused(&mut a, json!([fine, { "title": "Bad", "page": 0 }]));
    assert!(m.contains("[2]") && m.contains("page"), "{m}");
    let m = refused(&mut a, json!([{ "title": "Bad", "color": [2, 0, 0] }]));
    assert!(m.contains("color"), "{m}");
    let m = refused(&mut a, json!([{ "title": "Bad", "view": { "type": "XYZ", "values": [1, 2, 3, 4, 5] } }]));
    assert!(m.contains("view"), "{m}");
    let mut deep = json!({ "title": "Leaf" });
    for _ in 0..40 {
        deep = json!({ "title": "Level", "children": [deep] });
    }
    assert!(refused(&mut a, json!([deep])).contains("33 levels"));
    let m = refused(&mut a, json!([fine, { "title": "Z", "page": 1, "view": { "type": "Zoom" } }]));
    assert!(m.contains("[2]") && m.contains("XYZ (3 values"), "{m}");
    let m = refused(&mut a, json!([fine, { "title": "Z", "page": 1, "view": { "type": "FitH", "values": [1, 2] } }]));
    assert!(m.contains("[2]") && m.contains("FitH view takes 1"), "{m}");
    let m = refused(&mut a, json!([{ "title": "R", "view": { "type": "FitR", "values": [0, null, 1, 1] } }]));
    assert!(m.contains("all four"), "{m}");
    assert!(matches!(a.call("bookmark_import", &json!({ "doc": doc })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("bookmark_import", &json!({ "doc": doc, "bookmarks": [fine], "file": "x.json" })), Err(ToolError::InvalidArgs(_))));
    // A real bookmark file outside the root is refused for being outside, not for being missing.
    let outside = dir.with_extension("outside.json");
    std::fs::write(&outside, br#"{"bookmarks": [{"title": "Outside", "page": 1}]}"#).unwrap();
    let name = format!("../{}", outside.file_name().unwrap().to_string_lossy());
    match a.call("bookmark_import", &json!({ "doc": doc, "file": name })) {
        Err(ToolError::Failed(m)) => assert!(m.contains("outside the allowed directory"), "{m}"),
        other => panic!("expected the root to refuse it, got {other:?}"),
    }
    let _ = std::fs::remove_file(&outside);
    let out = dir.with_extension("out.json");
    let name = format!("../{}", out.file_name().unwrap().to_string_lossy());
    match a.call("bookmark_export", &json!({ "doc": doc, "out": name })) {
        Err(ToolError::Failed(m)) => assert!(m.contains("outside the allowed directory"), "{m}"),
        other => panic!("expected the root to refuse it, got {other:?}"),
    }
    assert!(!out.exists());
    assert_eq!(ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"], json!([]));
}

/// Copy and paste comments between documents through the tools: the copies carry their replies,
/// land on the asked-for page under their own names (a second paste gives a second set), and a
/// plain text is refused.
#[test]
fn comments_copy_and_paste_between_documents_through_tools() {
    let dir = workdir("comments-clipboard");
    let mut a = auto(&dir);
    let src = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": src, "page": 2, "type": "note", "at": [30, 30], "contents": "Please review", "author": "Ada" }));
    ok(&mut a, "comment_add", json!({ "doc": src, "page": 2, "type": "rectangle", "rect": [20, 20, 80, 60], "contents": "Box" }));
    ok(&mut a, "comment_reply", json!({ "doc": src, "page": 2, "index": 1, "text": "Done", "author": "Bob" }));
    // Positions as comment_list reports them (a note's pop-up takes a position of its own).
    let listed = ok(&mut a, "comment_list", json!({ "doc": src, "page": 2 }));
    let pairs: Vec<Value> = listed["comments"].as_array().unwrap().iter().map(|c| json!([c["page"], c["index"]])).collect();
    assert_eq!(pairs.len(), 2, "{listed}");
    let copied = ok(&mut a, "comments_copy", json!({ "doc": src, "comments": pairs }));
    assert_eq!(copied["comments"], 2);
    let xfdf = copied["xfdf"].as_str().unwrap().to_owned();
    assert!(xfdf.contains("inreplyto=") && xfdf.contains("<square"), "{xfdf}");
    let dst = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "comments_paste", json!({ "doc": dst, "page": 1, "xfdf": xfdf }));
    assert_eq!(r["comments"], 3, "two comments and the reply: {r}");
    assert_eq!(r["undo"], "Paste comments");
    let list = ok(&mut a, "comment_list", json!({ "doc": dst }));
    let comments = list["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2, "the reply is in its thread, not a comment of its own: {list}");
    assert!(comments.iter().all(|c| c["page"] == 1), "{list}");
    assert_eq!(comments.iter().filter(|c| c["contents"] == "Please review").count(), 1);
    // The same again: a second set, not a replacement of the first.
    ok(&mut a, "comments_paste", json!({ "doc": dst, "page": 1, "xfdf": copied["xfdf"] }));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": dst }))["comments"].as_array().unwrap().len(), 4);
    // Placed: the copies' box is centred at `at` (top-left page coordinates, like comment_add).
    let r = ok(&mut a, "comments_paste", json!({ "doc": dst, "page": 2, "xfdf": copied["xfdf"], "at": [100, 150] }));
    assert_eq!(r["comments"], 3);
    let placed = ok(&mut a, "comment_list", json!({ "doc": dst, "page": 2 }));
    let rects: Vec<Vec<f64>> =
        placed["comments"].as_array().unwrap().iter().map(|c| c["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()).collect();
    let (x0, y0, x1, y1) =
        rects.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |(a, b, c, d), r| (a.min(r[0]), b.min(r[1]), c.max(r[2]), d.max(r[3])));
    assert!(((x0 + x1) / 2.0 - 100.0).abs() < 1.0 && ((y0 + y1) / 2.0 - 150.0).abs() < 1.0, "centred at [100, 150]: {rects:?}");
    assert!(matches!(a.call("comments_paste", &json!({ "doc": dst, "page": 1, "xfdf": "hello" })), Err(ToolError::InvalidArgs(_))));
    assert!(a.call("comments_copy", &json!({ "doc": src, "comments": [[9, 9]] })).is_err(), "no such comment");
    assert!(matches!(a.call("comments_copy", &json!({ "doc": src, "comments": [[2]] })), Err(ToolError::InvalidArgs(_))));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": src }))["comments"].as_array().unwrap().len(), 2, "the source is untouched");
}
