//! Unit tests of the editing operations (part 5 of 7).

#[allow(unused_imports)]
use super::*;

/// [`crate::Reach`] grows the old box to the move, the width either way and the lines below.
#[test]
fn a_rewrites_reach_covers_its_move_width_and_lines() {
    let reach = |offset, width, rows, upright| crate::Reach { rect: [10.0, 90.0, 30.0, 100.0], offset, width, rows, pitch: 10.0, upright }.bounds();
    assert_eq!(reach([0.0, 0.0], 20.0, 1, true), [10.0, 90.0, 30.0, 110.0]);
    assert_eq!(reach([5.0, -50.0], 20.0, 1, true), [10.0, 40.0, 35.0, 100.0]);
    assert_eq!(reach([0.0, 0.0], 50.0, 3, true), [-20.0, 70.0, 60.0, 110.0]);
    assert_eq!(reach([0.0, 0.0], 30.0, 4, false), [-40.0, 40.0, 80.0, 150.0]);
    assert_eq!(reach([f64::NAN, 0.0], 20.0, 1, true)[0], f64::NEG_INFINITY);
    let larger = crate::Reach { rect: [10.0, 90.0, 30.0, 100.0], offset: [0.0, 0.0], width: 20.0, rows: 1, pitch: 24.0, upright: true };
    assert_eq!(larger.bounds(), [10.0, 76.0, 30.0, 124.0], "a larger size grows the line up as well as down");
}

/// Revealed OCR text may have to go on top of the page; a page whose marked content nests too deep
/// for that is refused before either rewrite path changes anything.
#[test]
fn an_ocr_rewrite_refused_on_top_leaves_the_document_as_it_was() {
    let content = format!("{} BT /F1 20 Tf 3 Tr 42 44 Td (John) Tj ET /Im Do {}", "/X BMC ".repeat(10_001), "EMC ".repeat(10_001));
    let mut doc = text_page(&content);
    assert!(text::text_blocks(&doc, 0).unwrap()[0].invisible);
    let before = write_full(&doc, &SaveOptions::default()).unwrap();
    for edit in ["Jane", "Jane\nDoe"] {
        let error = text::rewrite_block(&mut doc, 0, 0, Some(edit), &text::BlockStyle::default()).unwrap_err();
        assert!(error.to_string().contains("nested too deeply"), "{edit:?}: {error}");
        assert!(write_full(&doc, &SaveOptions::default()).unwrap() == before, "{edit:?}: a refused rewrite changes nothing");
    }
}

#[test]
fn group_move_budget_refusals_keep_the_original_page_selectable() {
    let text = text_page(&format!("{} BT /F1 12 Tf 40 400 Td (Target) Tj ET", "n ".repeat(99_995)));
    assert_eq!(pdfcraft_content::parse(&page_content_bytes(&text, 0)).ops.len(), 100_000);
    let mut image = image_page();
    let page = pdfcraft_model::pages(&image)[0].obj;
    let bytes = format!("{} /Im0 Do {}", "q ".repeat(1024), "Q ".repeat(1024));
    let content = image.add(Object::Stream(Stream::flate(Dict::new(), bytes.as_bytes())));
    image.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(content))).unwrap();
    for mut doc in [text, image] {
        let objects = editable_objects(&doc, 0).unwrap();
        assert_eq!(objects.len(), 1);
        let before = pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap();
        let error = move_objects(&mut doc, 0, &[objects[0].target], [10.0, 5.0]).unwrap_err();
        assert!(error.to_string().contains("too many content operations") || error.to_string().contains("nested too deeply"), "{error}");
        assert_eq!(pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap(), before);
        assert_eq!(editable_objects(&doc, 0).unwrap(), objects, "refusal retains a usable inventory");
    }
}

#[test]
fn arabic_actual_text_keeps_typing_order_when_opened_for_editing() {
    for logical in ["مرحبا بالعالم", "سَلَام 123 (PDF)", "English مرحبا ١٢٣", "PDFمرحبا", "مرحبا", "שלום עולם", "Latin text"]
    {
        let hex: String = logical.encode_utf16().map(|u| format!("{u:04X}")).collect();
        for drawing in ["(abcd) Tj (efgh) Tj", "(X) Tj"] {
            let doc = text_page(&format!("/Span <</ActualText <FEFF{hex}>>> BDC BT /F1 12 Tf 72 700 Td {drawing} ET EMC"));
            for doc in [&doc, &reopen(&doc)] {
                let lines = text::text_lines(doc, 0).unwrap();
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].text, logical, "ActualText is already in typing order");
                assert_eq!(text::text_blocks(doc, 0).unwrap()[0].editor_text, logical);
            }
        }
    }
}

#[test]
fn latin_ligature_actual_text_stays_on_its_line() {
    // Word and Chrome wrap ligatures in ActualText spans mid-word ("ﬁ" drawn, "fi" meant).
    let hex: String = "fi".encode_utf16().map(|u| format!("{u:04X}")).collect();
    let doc = text_page(&format!("BT /F1 12 Tf 72 700 Td (de) Tj /Span <</ActualText <FEFF{hex}>>> BDC (X) Tj EMC (ned well) Tj ET"));
    for doc in [&doc, &reopen(&doc)] {
        let lines = text::text_lines(doc, 0).unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["defined well"]);
        assert_eq!(text::text_blocks(doc, 0).unwrap()[0].editor_text, "defined well");
    }
}

/// #3110: `/ActualText` may come from a named property list (`/Span /MC0 BDC`) looked up in the
/// page's or the form's `/Properties`, as well as from an inline one.
#[test]
fn actual_text_from_named_property_list() {
    let properties = "/Properties << /MC0 << /ActualText (Hello World) >> /MC1 << /Lang (en) >> >>";
    let page_with = |content: &str| {
        build(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            &format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> {properties} >> /Contents 4 0 R >>"),
            &stream("", content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        ])
    };
    for (content, expected) in [
        ("/Span /MC0 BDC BT /F1 24 Tf 72 700 Td (Glyphs) Tj ET EMC", "Hello World"),
        ("/Span <</ActualText (Hello World)>> BDC BT /F1 24 Tf 72 700 Td (Glyphs) Tj ET EMC", "Hello World"),
        ("/Span /MC1 BDC BT /F1 24 Tf 72 700 Td (Glyphs) Tj ET EMC", "Glyphs"),
        ("/Span /Missing BDC BT /F1 24 Tf 72 700 Td (Glyphs) Tj ET EMC", "Glyphs"),
    ] {
        let doc = page_with(content);
        for doc in [&doc, &reopen(&doc)] {
            let lines = text::text_lines(doc, 0).unwrap();
            assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), [expected], "{content}");
            assert_eq!(text::text_blocks(doc, 0).unwrap()[0].editor_text, expected, "{content}");
            let reading: Vec<String> = text::reading_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect();
            assert_eq!(reading, [expected], "{content}");
        }
    }

    // A form's named list is looked up in the form's own /Properties, not the page's.
    let fm = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /Properties << /MC0 << /ActualText (Form text) >> >> >>",
        "/Span /MC0 BDC BT /F1 24 Tf 72 600 Td (Glyphs) Tj ET EMC",
    );
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Fm1 6 0 R >> {properties} >> /Contents 4 0 R >>"
        ),
        &stream("", "/Fm1 Do"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        &fm,
    ]);
    let reading: Vec<String> = text::reading_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(reading, ["Form text"]);
}

#[test]
fn actual_text_and_visual_rtl_glyphs_keep_their_own_reading_order() {
    let hex: String = "שלום".encode_utf16().map(|u| format!("{u:04X}")).collect();
    let doc = hebrew_page(&format!("BT /F1 12 Tf 72 700 Td (EDCBA) Tj /Span <</ActualText <FEFF{hex}>>> BDC (FGHD) Tj EMC (EDCBA) Tj ET"));
    for doc in [&doc, &reopen(&doc)] {
        let lines = text::text_lines(doc, 0).unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["בדיוק", "שלום", "בדיוק"]);
    }
}

#[test]
fn arabic_ligature_actual_text_stays_on_its_line() {
    for (ligature, composite) in [("لا", false), ("لأ", false), ("لإ", false), ("لآ", false), ("لَا", false), ("لا", true)] {
        let hex: String = ligature.encode_utf16().map(|u| format!("{u:04X}")).collect();
        let (left, glyph, right) = if composite { ("<0042>", "<0058>", "<0041>") } else { ("(B)", "(X)", "(A)") };
        let doc = arabic_ligature_page(
            &format!("BT /F1 12 Tf 72 700 Td {left} Tj /Span <</ActualText <FEFF{hex}>>> BDC [{glyph}] TJ EMC {right} Tj ET"),
            composite,
        );
        let expected = format!("س{ligature}م");
        for doc in [&doc, &reopen(&doc)] {
            let lines = text::text_lines(doc, 0).unwrap();
            assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), [expected.as_str()]);
            assert_eq!(text::text_blocks(doc, 0).unwrap()[0].editor_text, expected);
        }
    }
}

#[test]
fn multi_glyph_rtl_actual_text_keeps_its_own_reading_order() {
    // Two encoded glyphs are a logical span, even if their replacement looks like a ligature.
    for drawing in ["(XX) Tj", "(X) Tj (X) Tj", "[(X) (X)] TJ"] {
        let doc =
            arabic_ligature_page(&format!("BT /F1 12 Tf 72 700 Td (B) Tj /Span <</ActualText <FEFF06440627>>> BDC {drawing} EMC (A) Tj ET"), false);
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["م", "لا", "س"]);
    }
}

#[test]
fn replacing_word_with_actual_text_ligature_removes_all_its_source_operators() {
    let mut doc = arabic_ligature_page(
        "BT /F1 12 Tf 72 700 Td (B) Tj /Span <</ActualText <FEFF06440627>>> BDC /Span BMC (X) Tj EMC EMC (A) Tj ET \
         BT /F1 12 Tf 72 600 Td (AB) Tj ET",
        false,
    );
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].editor_text, "سلام");
    text::replace_block(&mut doc, 0, 0, "Replacement").unwrap();
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Replacement", "مس"]);
    let bytes = page_content_bytes(&doc, 0);
    let content = String::from_utf8_lossy(&bytes);
    assert!(!content.contains("ActualText") && !content.contains("(X)") && !content.contains("(B)") && !content.contains("(A)"), "{content}");
}

#[test]
fn replacing_arabic_actual_text_removes_the_whole_original_drawing() {
    let logical = "مرحبا بالعالم";
    let hex: String = logical.encode_utf16().map(|u| format!("{u:04X}")).collect();
    let mut doc = text_page(&format!(
        "/Span <</ActualText <FEFF{hex}>>> BDC BT /F1 12 Tf 72 700 Td (abcd) Tj (efgh) Tj ET EMC \
         BT /F1 12 Tf 72 600 Td (Keep this line) Tj ET"
    ));
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].editor_text, logical);
    text::replace_block(&mut doc, 0, 0, "Replacement").unwrap();
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Replacement", "Keep this line"]);
    let bytes = page_content_bytes(&doc, 0);
    let content = String::from_utf8_lossy(&bytes);
    assert!(!content.contains("ActualText") && !content.contains("(abcd)") && !content.contains("(efgh)"));
}

#[test]
fn mixed_styles_and_matrix_sizes_form_visual_regions_without_merging_columns() {
    let doc = mixed_region_page();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].editor_text, "TITLE\ntwo lines\nlast");
    assert_eq!(blocks[0].rows.len(), 3);
    assert_eq!(blocks[1].editor_text, "About\nPlain bold text.\nItalic footer");
    assert_eq!(blocks[1].rows.len(), 3);
    assert_eq!(blocks[1].lines.len(), 5);
    assert_eq!(blocks[2].text, "Other column");
}

#[test]
fn a_single_row_with_mixed_styles_grows_before_wrapping() {
    let mut doc = text_page("BT /F1 10 Tf 72 582 Td (Plain ) Tj /F2 10 Tf (bold) Tj /F1 10 Tf ( text.) Tj ET");
    doc.update_dict(pdfcraft_cos::ObjRef::new(6, 0), |f| {
        *f = Dict::new();
        f.set(b"Type".to_vec(), Object::name("Font"));
        f.set(b"Subtype".to_vec(), Object::name("Type1"));
        f.set(b"BaseFont".to_vec(), Object::name("Helvetica-Bold"));
    })
    .unwrap();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let index = blocks.iter().position(|b| b.editor_text == "Plain bold text.").unwrap();
    let source = &blocks[index];
    assert_eq!(source.rows.len(), 1);
    assert!(source.lines.len() > 1);
    let draft = "Plain bold text. More words on the same line";
    text::replace_block(&mut doc, 0, index, draft).unwrap();
    let edited = text::text_blocks(&reopen(&doc), 0).unwrap();
    let block = edited.iter().find(|b| b.text == draft).unwrap();
    assert_eq!(block.rows.len(), 1, "a style boundary must not pin a single row to its original width");
    assert!((block.rect[0] - source.rect[0]).abs() < 0.1);
    assert!(block.rect[2] > source.rect[2] + 20.0);
}

#[test]
fn explicit_editor_rows_survive_single_run_edit_and_reopen() {
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Original heading) Tj ET");
    let text = "First row\nSecond row\nLast row";
    let layout = text::text_edit_layout(&doc, 0, 0, text, &text::BlockStyle::default()).unwrap();
    text::replace_block(&mut doc, 0, 0, text).unwrap();
    let blocks = text::text_blocks(&reopen(&doc), 0).unwrap();
    assert_eq!(blocks[0].editor_text, text);
    assert_eq!(blocks[0].rows.len(), 3);
    assert_eq!(Some(blocks[0].rect), layout.draft_rect);
}

#[test]
fn rich_region_edits_preserve_styles_positions_and_delete_all_runs_after_reopen() {
    let original = mixed_region_page();
    let before = text::text_lines(&original, 0).unwrap();
    let block = text::text_blocks(&original, 0).unwrap()[1].clone();
    let mut doc = original.clone();
    text::replace_block(&mut doc, 0, 1, &block.editor_text.replace("Plain", "Other")).unwrap();
    let reopened = reopen(&doc);
    let blocks = text::text_blocks(&reopened, 0).unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[1].editor_text, "About\nOther bold text.\nItalic footer");
    let lines = text::text_lines(&reopened, 0).unwrap();
    for word in ["About", "bold", "Italic footer"] {
        let source = before.iter().find(|l| l.text == word).unwrap();
        let after = lines.iter().find(|l| l.text == word).unwrap();
        assert_eq!(
            (after.base_font.as_str(), after.size, after.color, after.bold, after.italic),
            (source.base_font.as_str(), source.size, source.color, source.bold, source.italic)
        );
        assert!((after.origin_baseline() - source.origin_baseline()).abs() < 0.001);
    }
    for i in [0, 2] {
        assert_eq!(blocks[i].editor_text, text::text_blocks(&original, 0).unwrap()[i].editor_text);
        assert_eq!(blocks[i].rect, text::text_blocks(&original, 0).unwrap()[i].rect);
    }
    let mut removed = original.clone();
    text::replace_block(&mut removed, 0, 1, "").unwrap();
    let after = text::text_blocks(&reopen(&removed), 0).unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].editor_text, "TITLE\ntwo lines\nlast");
    assert_eq!(after[1].text, "Other column");
    let mut preview = original.clone();
    text::clear_block(&mut preview, 0, 1).unwrap();
    assert_eq!(text::text_blocks(&reopen(&preview), 0).unwrap().len(), 2);
    assert_eq!(text::text_blocks(&original, 0).unwrap().len(), 3);
}

/// Two edits applied together (heading and footer edited before leaving the editor, #2198): the
/// runs between them are unchanged and keep their own styles instead of taking the heading's.
#[test]
fn unchanged_runs_between_two_edits_keep_their_styles() {
    let original = mixed_region_page();
    let look =
        |doc: &Document| text::text_lines(doc, 0).unwrap().into_iter().map(|l| (l.text.trim().to_string(), l.base_font, l.color)).collect::<Vec<_>>();
    let before = look(&original);
    let block = text::text_blocks(&original, 0).unwrap()[1].clone();
    let mut doc = original.clone();
    let draft = block.editor_text.replace("About", "About us").replace("footer", "footnote");
    text::replace_block(&mut doc, 0, 1, &draft).unwrap();
    let after = look(&reopen(&doc));
    let style = |lines: &[(String, String, [f64; 3])], word: &str| lines.iter().find(|l| l.0 == word).map(|l| (l.1.clone(), l.2));
    for word in ["Plain", "bold", "text."] {
        assert_eq!(style(&after, word), style(&before, word), "{word:?} keeps its own style; lines after the edit: {after:?}");
    }
    assert_eq!(style(&after, "About us"), style(&before, "About"));
    assert_eq!(style(&after, "Italic footnote"), style(&before, "Italic footer"));
}

#[test]
fn disjoint_style_ranges_preserve_unselected_runs_and_validate_utf8_atomically() {
    let mut doc = mixed_region_page();
    let block = text::text_blocks(&doc, 0).unwrap()[1].clone();
    let a = block.editor_text.find("Plain").unwrap();
    let b = block.editor_text.find("bold").unwrap();
    let style = BlockStyle {
        ranges: vec![
            RangeStyle {
                range: a..a + 5,
                family: None,
                resource: None,
                size: Some(15.0),
                color: Some([0.8, 0.1, 0.2]),
                underline: Some(true),
                char_spacing: None,
                scale: None,
            },
            RangeStyle {
                range: b..b + 4,
                family: Some((Family::Courier, false, true)),
                resource: None,
                size: None,
                color: None,
                underline: None,
                char_spacing: Some(0.2),
                scale: Some(90.0),
            },
        ],
        width: Some(300.0),
        ..Default::default()
    };
    text::rewrite_block(&mut doc, 0, 1, Some(&block.editor_text), &style).unwrap();
    doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    let plain = lines.iter().find(|l| l.text == "Plain").unwrap();
    assert_eq!((plain.size, plain.color), (15.0, [0.8, 0.1, 0.2]));
    let bold = lines.iter().find(|l| l.text == "bold").unwrap();
    assert_eq!(bold.base_font, "Courier-Oblique");
    let original = mixed_region_page();
    for word in ["About", "Italic footer"] {
        let prior = text::text_lines(&original, 0).unwrap().into_iter().find(|l| l.text == word).unwrap();
        let after = lines.iter().find(|l| l.text == word).unwrap();
        assert_eq!((after.size, after.color, &after.font, after.baseline()), (prior.size, prior.color, &prior.font, prior.baseline()));
    }
    let before = pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap();
    let invalid = BlockStyle {
        ranges: vec![RangeStyle {
            range: 1..2,
            family: None,
            resource: None,
            size: Some(12.0),
            color: None,
            underline: None,
            char_spacing: None,
            scale: None,
        }],
        ..Default::default()
    };
    assert!(text::rewrite_block(&mut doc, 0, 1, Some("é bad"), &invalid).is_err());
    assert_eq!(pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap(), before);
}

#[test]
fn centered_mixed_size_title_keeps_three_rows_when_changed() {
    let mut doc = mixed_region_page();
    let title = text::text_blocks(&doc, 0).unwrap()[0].clone();
    text::replace_block(&mut doc, 0, 0, &title.editor_text.replace("last", "tail")).unwrap();
    let reopened = reopen(&doc);
    let blocks = text::text_blocks(&reopened, 0).unwrap();
    assert_eq!(blocks[0].editor_text, "TITLE\ntwo lines\ntail");
    assert_eq!(blocks[0].rows.len(), 3);
    let lines = text::text_lines(&reopened, 0).unwrap();
    for row in &blocks[0].rows {
        let line = &lines[row[0]];
        assert!(((line.rect[0] + line.rect[2]) / 2.0 - 150.0).abs() < 0.01);
    }
}

#[test]
fn image_selection_bounds_respect_clips_and_restore_graphics_state() {
    let mut doc = image_page();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let content = b"q 1 0 0 1 10 20 cm 20 30 100 80 re W* n q 1200 0 0 1400 -500 -600 cm /Im0 Do Q Q q 200 0 0 100 100 100 cm /Im0 Do Q q 0 0 10 10 re W n 100 0 0 100 200 200 cm /Im0 Do Q";
    let stream = doc.add(Object::Stream(Stream::flate(Dict::new(), content)));
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Ref(stream))).unwrap();
    let before = images::page_images(&doc, 0).unwrap();
    assert_eq!(before.len(), 3, "fully clipped placements retain source indexes");
    assert_eq!(before[0].visible_rect, Some([30.0, 50.0, 130.0, 130.0]));
    assert_eq!(before[1].visible_rect, Some(before[1].rect), "Q restores the prior clip");
    assert_eq!(before[2].visible_rect, None);
    let objects = crate::editable_objects(&doc, 0).unwrap();
    assert_eq!(objects.len(), 2, "hidden images cannot intercept selection");
    assert_eq!(objects[0].rect, before[0].visible_rect.unwrap());
    let offset = [12.0, 14.0];
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(pdfcraft_content::Matrix::translate(offset[0], offset[1]).0)).unwrap();
    let reopened = reopen(&doc);
    let after = images::page_images(&reopened, 0).unwrap();
    assert_eq!(after[0].visible_rect, Some([42.0, 64.0, 142.0, 144.0]), "an exclusive crop moves with the image even across nested graphics states");
    for (a, b) in after[0].rect.iter().zip(before[0].rect.iter()).enumerate() {
        assert!((b.0 - b.1 - offset[a % 2]).abs() < 0.001);
    }
    assert_eq!(after[1].rect, before[1].rect, "neighbor placement unchanged");
    assert_eq!(after[1].visible_rect, before[1].visible_rect);
    assert_eq!(after[1].matrix, before[1].matrix);
    assert_eq!(after[1].object, before[1].object);
}

#[test]
fn image_clip_path_and_graphics_state_continue_across_content_streams() {
    let mut doc = image_page();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let pieces: [&[u8]; 3] = [b"q 20 30 40 50 re W", b"n 1000 0 0 1000 -300 -400 cm /Im0 Do", b"Q q 50 0 0 50 100 200 cm /Im0 Do Q"];
    let refs: Vec<_> = pieces.into_iter().map(|b| Object::Ref(doc.add(Object::Stream(Stream::flate(Dict::new(), b))))).collect();
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Array(refs))).unwrap();
    let images = images::page_images(&doc, 0).unwrap();
    assert_eq!(images[0].visible_rect, Some([20.0, 30.0, 60.0, 80.0]));
    assert_eq!(images[1].visible_rect, Some([100.0, 200.0, 150.0, 250.0]));
    images::change_image(
        &mut doc,
        0,
        0,
        &images::ImageChange::Transform(images::rect_to_rect(images[0].visible_rect.unwrap(), [20.0, 30.0, 80.0, 105.0])),
    )
    .unwrap();
    let after = images::page_images(&reopen(&doc), 0).unwrap();
    assert!(close(after[0].visible_rect.unwrap(), [20.0, 30.0, 80.0, 105.0]), "an exclusive crop scales with its image across streams");
    assert_eq!(after[1].matrix, images[1].matrix, "graphics state restored across stream boundaries");
}

#[test]
fn cropped_image_resizing_moves_private_clips_and_keeps_shared_ones() {
    let mut doc = image_page();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let stream = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 30 170 130 80 re W* n 800 0 0 900 -300 -400 cm /Im0 Do Q BT /F1 12 Tf 30 130 Td (Caption) Tj ET",
    )));
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Ref(stream))).unwrap();
    for target in [[30.0, 170.0, 190.0, 270.0], [30.0, 170.0, 110.0, 220.0], [70.0, 210.0, 150.0, 260.0]] {
        let before = images::page_images(&doc, 0).unwrap()[0].clone();
        let transform = images::rect_to_rect(before.visible_rect.unwrap(), target);
        images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(transform)).unwrap();
        doc = reopen(&doc);
        let after = images::page_images(&doc, 0).unwrap()[0].clone();
        // The moved clip's `cm` is written with the content stream's number precision.
        let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.05);
        assert!(near(after.visible_rect.unwrap(), target), "{:?} vs {target:?}", after.visible_rect);
        assert!(near(after.rect, pdfcraft_content::Matrix(transform).bbox(before.rect)), "full pixels use the same crop transform");
        assert_eq!(after.object, before.object, "pixels are never resampled or replaced");
        assert_eq!(text::text_lines(&doc, 0).unwrap()[0].baseline(), [30.0, 130.0]);
    }
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let stream = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 30 170 130 80 re W n q 800 0 0 900 -300 -400 cm /Im0 Do Q BT /F1 12 Tf 40 200 Td (Shared caption) Tj ET Q",
    )));
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Ref(stream))).unwrap();
    let before = images::page_images(&doc, 0).unwrap();
    let caption = text::text_lines(&doc, 0).unwrap();
    let transform = [1.2, 0.0, 0.0, 1.2, 0.0, 0.0];
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(transform)).unwrap();
    doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert!(close(after[0].visible_rect.unwrap(), before[0].visible_rect.unwrap()), "a shared clip stays where it is");
    assert_eq!(after[0].object, before[0].object);
    assert_eq!(
        text::text_lines(&doc, 0).unwrap().iter().map(|l| (&l.text, l.rect, l.baseline())).collect::<Vec<_>>(),
        caption.iter().map(|l| (&l.text, l.rect, l.baseline())).collect::<Vec<_>>()
    );
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform([0.5, 0.0, 0.0, 0.5, 0.0, 0.0])).unwrap();
    assert_eq!(text::text_lines(&doc, 0).unwrap().len(), 1);
}

/// Report generators draw each picture as `q re W n cm Do Q` inside a section's clip that the
/// section's text shares. Resizing the picture scales its own clip with it (the enlarged picture
/// used to show only through its old box) while the section's clip and text stay put.
#[test]
fn resizing_an_image_under_a_shared_clip_moves_its_own_clip() {
    // The picture's own clip as wide as the picture, and wider (both seen in one report).
    for clip_width in [61.2, 85.0] {
        let mut doc = image_page_with(&format!(
            "q 20 20 560 760 re W n q 162 79 {clip_width} 28.3 re W n 61.2 0 0 28.3 162 79 cm /Im0 Do Q BT /F1 12 Tf 30 50 Td (Section text) Tj ET Q"
        ));
        let before = images::page_images(&doc, 0).unwrap()[0].clone();
        assert!(close(before.visible_rect.unwrap(), [162.0, 79.0, 223.2, 107.3]), "{:?}", before.visible_rect);
        let caption = text::text_lines(&doc, 0).unwrap();
        let target = [162.0, 79.0, 284.4, 135.6];
        let transform = images::rect_to_rect(before.visible_rect.unwrap(), target);
        images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(transform)).unwrap();
        doc = reopen(&doc);
        let after = images::page_images(&doc, 0).unwrap()[0].clone();
        let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.05);
        assert!(near(after.visible_rect.unwrap(), target), "clip {clip_width}: the whole enlarged picture shows, {:?}", after.visible_rect);
        assert_eq!(after.object, before.object, "pixels are never resampled or replaced");
        assert_eq!(
            text::text_lines(&doc, 0).unwrap().iter().map(|l| (&l.text, l.rect, l.baseline())).collect::<Vec<_>>(),
            caption.iter().map(|l| (&l.text, l.rect, l.baseline())).collect::<Vec<_>>(),
            "the section's text stays where it was"
        );
        // The section's clip is shared, so it still bounds the picture: past it, nothing shows.
        let past = images::rect_to_rect(after.visible_rect.unwrap(), [500.0, 79.0, 700.0, 135.6]);
        images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(past)).unwrap();
        let clipped = images::page_images(&reopen(&doc), 0).unwrap()[0].visible_rect.unwrap();
        assert!(clipped[2] <= 580.05, "the section's clip stays in force: {clipped:?}");
    }
}

#[test]
fn private_image_crop_with_intervening_state_resizes_across_streams() {
    let variants: Vec<Vec<&[u8]>> = vec![
        vec![b"q 1 0 0 1 10 20 cm 20 150 130 80 re W* n 0.5 g q 800 0 0 900 -300 -400 cm /Im0 Do Q Q BT /F1 12 Tf 30 130 Td (Caption) Tj ET"],
        vec![b"q 1 0 0 1 10 20 cm 20 150 130 80 re W*", b"n 0.5 g q 800 0 0 900 -300 -400 cm /Im0 Do", b"Q Q BT /F1 12 Tf 30 130 Td (Caption) Tj ET"],
    ];
    for pieces in variants {
        let mut doc = image_page();
        let page = pdfcraft_model::pages(&doc)[0].obj;
        let refs = pieces.iter().map(|b| Object::Ref(doc.add(Object::Stream(Stream::flate(Dict::new(), b))))).collect();
        doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Array(refs))).unwrap();
        let caption = text::text_lines(&doc, 0).unwrap();
        for target in [[30.0, 170.0, 190.0, 270.0], [30.0, 170.0, 110.0, 220.0], [70.0, 210.0, 150.0, 260.0]] {
            let before = images::page_images(&doc, 0).unwrap()[0].clone();
            let transform = images::rect_to_rect(before.visible_rect.unwrap(), target);
            images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(transform)).unwrap();
            doc = reopen(&doc);
            let after = images::page_images(&doc, 0).unwrap()[0].clone();
            assert!(close(after.visible_rect.unwrap(), target), "the private crop follows the image without a literal operator pattern");
            assert!(close(after.rect, pdfcraft_content::Matrix(transform).bbox(before.rect)));
            assert_eq!(after.object, before.object);
            assert_eq!(
                text::text_lines(&doc, 0).unwrap().iter().map(|l| (&l.text, l.rect, l.baseline())).collect::<Vec<_>>(),
                caption.iter().map(|l| (&l.text, l.rect, l.baseline())).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn an_image_whose_operator_starts_the_next_stream_can_be_selected_moved_and_deleted() {
    // #3113: an operator's operands end one piece and its keyword starts the next.
    let square = [72.0, 500.0, 172.0, 600.0];
    let cases: [(&[&[u8]], bool); 4] = [
        (&[b"q 100 0 0 100 72 500 cm /Im0", b"Do Q"], false),
        (&[b"q 100 0 0 100 72 500", b"cm /Im0 Do Q"], false),
        (&[b"q /Fm0", b"Do Q"], true),
        (&[b"q 100 0 0 100 72 500 cm /Im0 Do Q"], false),
    ];
    for (pieces, is_form) in cases {
        let doc = split_image_page(pieces);
        let label: Vec<_> = pieces.iter().map(|p| String::from_utf8_lossy(p)).collect();
        let images = images::page_images(&doc, 0).unwrap();
        assert_eq!(images.len(), 1, "{label:?}");
        assert!(close(images[0].rect, square), "{label:?}: {:?}", images[0].rect);
        assert_eq!(images[0].is_form, is_form);
        assert!(images[0].contains([122.0, 550.0]), "{label:?}");
        if !is_form {
            let read = images::reading_images(&doc, 0).unwrap();
            assert!(read.len() == 1 && close(read[0].rect, square), "{label:?}: {read:?}");
        }

        let mut deleted = doc.clone();
        images::change_image(&mut deleted, 0, 0, &images::ImageChange::Delete).unwrap();
        let deleted = reopen(&deleted);
        assert!(images::page_images(&deleted, 0).unwrap().is_empty(), "{label:?}");
        let (joined, ops) = joined_ops(&deleted);
        assert!(!ops.iter().any(|o| o.is("Do")), "{joined}");

        let target = [300.0, 500.0, 400.0, 600.0];
        let mut moved = doc.clone();
        images::change_image(&mut moved, 0, 0, &images::ImageChange::Transform(images::rect_to_rect(square, target))).unwrap();
        let moved = reopen(&moved);
        let after = images::page_images(&moved, 0).unwrap();
        assert!(after.len() == 1 && close(after[0].rect, target), "{label:?}: {after:?} {:?}", joined_ops(&moved).0);

        let mut shifted = doc.clone();
        images::translate_images(&mut shifted, 0, &[0], [228.0, 0.0]).unwrap();
        let shifted = reopen(&shifted);
        let after = images::page_images(&shifted, 0).unwrap();
        assert!(after.len() == 1 && close(after[0].rect, target), "{label:?}: {after:?} {:?}", joined_ops(&shifted).0);
    }
}

#[test]
fn curved_image_clips_have_analytic_bounds_and_holes_do_not_capture_points() {
    let mut doc = image_page();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let bytes =
        b"q 0 0 m 0 200 100 200 100 0 c h W n 300 0 0 300 0 0 cm /Im0 Do Q q 100 100 100 100 re 125 125 50 50 re W* n 400 0 0 400 0 0 cm /Im0 Do Q";
    let content = doc.add(Object::Stream(Stream::flate(Dict::new(), bytes)));
    doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(content))).unwrap();
    let images = images::page_images(&doc, 0).unwrap();
    assert!(close(images[0].visible_rect.unwrap(), [0.0, 0.0, 100.0, 150.0]), "cubic extrema, not controls");
    assert!(images[0].contains([50.0, 100.0]));
    assert!(!images[0].contains([2.0, 140.0]), "empty corners of curve bounds are not artwork");
    assert!(images[1].contains([110.0, 110.0]));
    assert!(!images[1].contains([150.0, 150.0]), "even-odd hole");
    images::change_image(&mut doc, 0, 1, &images::ImageChange::Transform([1.2, 0.0, 0.0, 1.2, 10.0, 15.0])).unwrap();
    let after = images::page_images(&reopen(&doc), 0).unwrap();
    assert!(!after[1].contains([190.0, 195.0]), "the transformed even-odd hole remains empty");
    assert!(after[1].contains([142.0, 147.0]), "the transformed filled corner remains selectable");
    assert_eq!(after[0].visible_rect, images[0].visible_rect, "the neighbouring curved clip stays put");
}

#[test]
fn marked_paragraph_boundaries_and_cross_stream_transforms_survive_rewrite() {
    let mut doc =
        text_page("/P << /MCID 0 >> BDC BT /F1 12 Tf 40 500 Td (One) Tj ET EMC /P << /MCID 1 >> BDC BT /F1 12 Tf 40 482 Td (Two) Tj ET EMC");
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2, "distinct explicit paragraphs win over nearby geometry");
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let parts: [&[u8]; 2] =
        [b"/P << /MCID 2 >> BDC BT /F1 12 Tf 40 500 Td (First row) Tj ET", b"q 1 0 0 1 10 12 cm BT /F1 12 Tf 30 470 Td (Second row) Tj ET Q EMC"];
    let refs = parts.iter().map(|p| Object::Ref(doc.add(Object::Stream(Stream::flate(Dict::new(), p))))).collect();
    doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Array(refs))).unwrap();
    let block = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(block.len(), 1, "one marked paragraph across streams and coordinate spaces");
    assert_eq!(block[0].editor_text, "First row\nSecond row");
    let original = text::text_lines(&doc, 0).unwrap();
    text::rewrite_block(&mut doc, 0, 0, Some("First row\nChanged row"), &text::BlockStyle { width: Some(200.0), ..Default::default() }).unwrap();
    let after = text::text_lines(&reopen(&doc), 0).unwrap();
    for (a, b) in after.iter().zip(original) {
        assert!(close(a.rect, b.rect) || a.baseline() == b.baseline());
    }
    assert_eq!(after[1].text, "Changed row");
}

#[test]
fn logical_indic_with_nuktas_not_reordered_on_standard_font() {
    // Already logical Hindi text with nuktas (बढ़िया) must not be corrupted by pre-base reordering
    // when the font does not require visual order.
    let hex_actual = "<FEFF092C0922093C093F092F093E>";
    let doc = text_page(&format!("/Span << /ActualText {hex_actual} >> BDC BT /F1 12 Tf 40 500 Td (ab) Tj ET EMC"));
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}");
    assert!(!lines[0].visual_order);
}

/// A legacy visual-order font (Kruti Dev) whose ToUnicode gives the glyphs in drawing order: the
/// pre-base vowel sign is read back after its consonant, for editing and for reading.
#[test]
fn legacy_indic_font_flags_visual_order_and_reorders_pre_base_vowels() {
    let doc = devanagari_page("KrutiDev010", "BT /F1 12 Tf 40 500 Td (ABCDE) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].visual_order);
    assert_eq!(lines[0].text, "\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}");
    let reading: Vec<String> = text::reading_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(reading, ["\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}"]);
}

/// Typed (logical) text is written back in the font's visual order and reads back as typed.
#[test]
fn legacy_indic_font_edits_are_written_in_visual_order() {
    let mut doc = devanagari_page("ABCDEF+KrutiDev010", "BT /F1 12 Tf 40 500 Td (ABCDE) Tj ET");
    let kitab = "\u{0915}\u{093F}\u{0924}\u{093E}\u{092C}";
    assert_eq!(text::replace_line(&mut doc, 0, 0, kitab).unwrap().substituted, None);
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains("(BFGEA)"), "{content}");
    assert_eq!(text::text_lines(&reopen(&doc), 0).unwrap()[0].text, kitab);
}

/// `/ActualText` is already in logical order, even in a visual-order font: "बढ़िया" must not
/// become "बढ़यिा".
#[test]
fn logical_actual_text_in_a_legacy_indic_font_is_not_reordered() {
    let doc = devanagari_page("KrutiDev010", "/Span << /ActualText <FEFF092C0922093C093F092F093E> >> BDC BT /F1 12 Tf 40 500 Td (ABCDE) Tj ET EMC");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}");
    let reading: Vec<String> = text::reading_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(reading, ["\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}"]);
}

/// A modern subset font with a logical ToUnicode and no GSUB in the PDF (subsets and bare CFF
/// often drop it) is not visual order: its text is read as it is.
#[test]
fn subset_indic_font_without_legacy_name_keeps_logical_text() {
    let doc = devanagari_page("ABCDEF+NotoSansDevanagari-Regular", "BT /F1 12 Tf 40 500 Td (ACBDE) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(!lines[0].visual_order);
    assert_eq!(lines[0].text, "\u{092C}\u{0922}\u{093C}\u{093F}\u{092F}\u{093E}");
}

/// Lines of a paragraph drawn turned on the page (a landscape page drawn a quarter turn round)
/// are separate rows, and adding a word keeps them apart (#1443).
#[test]
fn turned_paragraph_keeps_its_line_breaks_when_a_word_is_added() {
    for cm in ["0 1 -1 0 600 0", "0 -1 1 0 0 800", "-1 0 0 -1 600 800"] {
        let original =
            text_page(&format!("q {cm} cm BT /F1 12 Tf 72 500 Td (First line here) Tj 0 -14 Td (Second line) Tj 0 -14 Td (Third) Tj ET Q"));
        let block = text::text_blocks(&original, 0).unwrap().remove(0);
        assert_eq!(block.editor_text, "First line here\nSecond line\nThird", "{cm}");
        let mut doc = original.clone();
        text::rewrite_block(&mut doc, 0, 0, Some("First line here\nSecond line\nThird added"), &text::BlockStyle::default()).unwrap();
        let reopened = reopen(&doc);
        assert_eq!(text::text_blocks(&reopened, 0).unwrap()[0].editor_text, "First line here\nSecond line\nThird added", "{cm}");
        let (before, after) = (text::text_lines(&original, 0).unwrap(), text::text_lines(&reopened, 0).unwrap());
        assert_eq!(after.len(), 3, "{cm}");
        for (a, b) in after.iter().zip(&before).take(2) {
            assert!(close(a.rect, b.rect), "{cm}: untouched rows stay put: {:?} {:?}", a.rect, b.rect);
        }
        // The last row grows along its own baseline from where it started.
        let (grown, old) = (after[2].rect, before[2].rect);
        assert!((0..2).all(|i| grown[i] <= old[i] + 0.01) && (2..4).all(|i| grown[i] >= old[i] - 0.01), "{cm}: {grown:?} {old:?}");
        assert!((after[2].origin_baseline() - before[2].origin_baseline()).abs() < 0.01, "{cm}");
    }
    // A turned single line grows rather than wrapping across the page's other axis.
    let mut doc = text_page("BT /F1 12 Tf 0 1 -1 0 300 300 Tm (Only line) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, Some("Only line with several more words added"), &text::BlockStyle::default()).unwrap();
    let blocks = text::text_blocks(&reopen(&doc), 0).unwrap();
    assert_eq!(blocks[0].editor_text, "Only line with several more words added");
    // Formatting part of turned text isn't laid out yet: refused, not drawn along the page's x axis.
    let mut doc = text_page("BT /F1 12 Tf 0 1 -1 0 300 300 Tm (Only line) Tj ET");
    let range = text::RangeStyle {
        range: 0..4,
        family: None,
        resource: None,
        size: Some(20.0),
        color: None,
        underline: None,
        char_spacing: None,
        scale: None,
    };
    let style = text::BlockStyle { ranges: vec![range], ..Default::default() };
    assert!(text::rewrite_block(&mut doc, 0, 0, Some("Only line"), &style).is_err());
}

/// A paragraph drawn at a slant rewraps at the width its rows have along their baseline, as the
/// same paragraph upright does. Its upright bounding box turned into the baseline's frame is wider
/// by the rows' height times |sin 2θ|, which let a word that doesn't fit stay on its row.
#[test]
fn a_paragraph_at_a_slant_rewraps_at_the_width_it_has_upright() {
    let rows = ["Rotated paragraph, first row of text", "that continues on a second row and", "ends on the third row here."];
    let draft = "Rotated paragraph, first row of text that continues on a second row and now carries a few extra words \
        before it ends on the third row here.";
    let rewrapped = |degrees: f64| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let shown = rows.iter().map(|r| format!("({r}) Tj")).collect::<Vec<_>>().join(" T* ");
        let mut doc = text_page(&format!("BT /F1 12 Tf 14.4 TL {cos:.6} {sin:.6} {:.6} {cos:.6} 300 400 Tm {shown} ET", -sin));
        let block = text::text_blocks(&doc, 0).unwrap().remove(0);
        assert_eq!(block.editor_text, rows.join("\n"), "{degrees}°");
        text::rewrite_block(&mut doc, 0, 0, Some(draft), &text::BlockStyle::default()).unwrap();
        text::text_lines(&reopen(&doc), 0).unwrap().into_iter().map(|l| l.text).collect::<Vec<_>>()
    };
    let upright = rewrapped(0.0);
    // The widest source row is 189.44 pt; "now carries a few extra words before" is 196.07 pt.
    assert_eq!(upright[2], "now carries a few extra words", "{upright:?}");
    for degrees in [30.0, 45.0, 60.0, 90.0, 135.0, 210.0, 330.0] {
        assert_eq!(rewrapped(degrees), upright, "{degrees}°");
    }
}

/// A paragraph in a Chrome-style subset font: `/W` leaves out the glyphs of the `/DW` width.
/// Adding a word keeps every row as it was and shows the word in the page's own font (#1443).
#[test]
fn default_width_subset_paragraph_keeps_its_rows_when_a_word_is_added() {
    let hex = |s: &str| s.chars().map(|c| format!("00{:02X}", u32::from(c))).collect::<String>();
    let rows = ["This is the first line", "the second", "and third"];
    let content = rows
        .iter()
        .enumerate()
        .map(|(i, row)| format!("BT /F3 12 Tf 1 0 0 1 72 {} Tm <{}> Tj ET", 700 - 14 * i, hex(row)))
        .collect::<Vec<_>>()
        .join(" ");
    let mut doc = text_page(&content);
    let cmap = b"/CIDInit /ProcSet findresource begin 1 begincodespacerange <0000> <FFFF> endcodespacerange \
        1 beginbfrange <0020> <007E> <0020> endbfrange end";
    let to_unicode = doc.add(Object::Stream(Stream::from_raw(Dict::new(), cmap.to_vec())));
    let mut cid = Dict::new();
    cid.set(b"Type".to_vec(), Object::name("Font"));
    cid.set(b"Subtype".to_vec(), Object::name("CIDFontType2"));
    cid.set(b"BaseFont".to_vec(), Object::name("AAAAAA+LiberationSans"));
    cid.set(b"CIDToGIDMap".to_vec(), Object::name("Identity"));
    cid.set(b"DW".to_vec(), Object::Int(500));
    cid.set(
        b"W".to_vec(),
        Object::Array(vec![Object::Int(32), Object::Array(vec![Object::Int(278)]), Object::Int(84), Object::Array(vec![Object::Int(611)])]),
    );
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type0"));
    font.set(b"BaseFont".to_vec(), Object::name("AAAAAA+LiberationSans"));
    font.set(b"Encoding".to_vec(), Object::name("Identity-H"));
    font.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Dict(cid)]));
    font.set(b"ToUnicode".to_vec(), Object::Ref(to_unicode));
    let font = doc.add(Object::Dict(font));
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let mut fonts = doc.resolve(resources.get(b"Font").unwrap()).as_dict().unwrap().clone();
    fonts.set(b"F3".to_vec(), Object::Ref(font));
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
    let block = text::text_blocks(&doc, 0).unwrap().remove(0);
    assert_eq!(block.editor_text, rows.join("\n"));
    let before = text::text_lines(&doc, 0).unwrap();
    let edit = text::rewrite_block(&mut doc, 0, 0, Some(&format!("{} added", block.editor_text)), &text::BlockStyle::default()).unwrap();
    assert_eq!(edit.substituted, None, "the subset shows every letter of the new word");
    let reopened = reopen(&doc);
    assert_eq!(text::text_blocks(&reopened, 0).unwrap()[0].editor_text, "This is the first line\nthe second\nand third added");
    let after = text::text_lines(&reopened, 0).unwrap();
    for (a, b) in after.iter().zip(&before).take(2) {
        assert_eq!(a.text, b.text);
        assert!(close(a.rect, b.rect), "untouched rows keep their place and width: {:?} {:?}", a.rect, b.rect);
    }
}

/// #2578: a single turned row grows along its baseline only up to the page's 36 pt margin and wraps
/// onto further rows from there, instead of running off the page.
#[test]
fn turned_single_row_wraps_at_the_page_margin() {
    let draft = "A single turned row that keeps growing with many more words added until it is far longer than the page is tall, so it has to stop or wrap somewhere";
    // Reading upwards (90° CCW) from y = 600 on the 600 × 800 test page: top margin is at y = 764.
    let before = text_page("BT /F1 12 Tf 0 1 -1 0 300 600 Tm (A single turned row) Tj ET");
    let layout = text::text_edit_layout(&before, 0, 0, draft, &text::BlockStyle::default()).unwrap();
    let mut doc = before;
    text::rewrite_block(&mut doc, 0, 0, Some(draft), &text::BlockStyle::default()).unwrap();
    let reopened = reopen(&doc);
    let lines = text::text_lines(&reopened, 0).unwrap();
    assert!(lines.len() > 1, "expected the turned row to wrap at the top margin: {lines:?}");
    for line in &lines {
        assert!((line.rect[1] - 600.0).abs() < 0.5, "row starts at y = 600: {:?}", line.rect);
        assert!(line.rect[3] <= 800.0 - 36.0 + 0.5, "row stays within the top margin: {:?}", line.rect);
    }
    let blocks = text::text_blocks(&reopened, 0).unwrap();
    assert_eq!(blocks.len(), 1, "wrapped turned rows stay one paragraph: {blocks:?}");
    assert_eq!(blocks[0].text, draft);
    assert_eq!(layout.draft_rect, Some(blocks[0].rect));

    // Reading downwards (90° CW) from y = 200: bottom margin is at y = 36.
    let mut doc = text_page("BT /F1 12 Tf 0 -1 1 0 300 200 Tm (A single turned row) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, Some(draft), &text::BlockStyle::default()).unwrap();
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert!(lines.len() > 1, "expected 90° CW row to wrap at the bottom margin: {lines:?}");
    for line in &lines {
        assert!((line.rect[3] - 200.0).abs() < 0.5, "row starts at y = 200: {:?}", line.rect);
        assert!(line.rect[1] >= 36.0 - 0.5, "row stays within the bottom margin: {:?}", line.rect);
    }

    // Reading upside down (180°) from x = 200: left margin is at x = 36.
    let mut doc = text_page("BT /F1 12 Tf -1 0 0 -1 200 600 Tm (A single turned row) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, Some(draft), &text::BlockStyle::default()).unwrap();
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert!(lines.len() > 1, "expected 180° row to wrap at the left margin: {lines:?}");
    for line in &lines {
        assert!((line.rect[2] - 200.0).abs() < 0.5, "row starts at x = 200: {:?}", line.rect);
        assert!(line.rect[0] >= 36.0 - 0.5, "row stays within the left margin: {:?}", line.rect);
    }

    // Slanted (45°) from (400, 600): baseline stops where it first reaches a 36 pt margin.
    let (sin, cos) = 45.0_f64.to_radians().sin_cos();
    let mut doc = text_page(&format!("BT /F1 12 Tf {cos:.6} {sin:.6} {:.6} {cos:.6} 400 600 Tm (A single turned row) Tj ET", -sin));
    text::rewrite_block(&mut doc, 0, 0, Some(draft), &text::BlockStyle::default()).unwrap();
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert!(lines.len() > 1, "expected 45° slanted row to wrap at the margin: {lines:?}");
    for line in &lines {
        assert!(line.rect[2] <= 600.0 && line.rect[3] <= 800.0, "slanted row stays on the page: {:?}", line.rect);
    }

    // Moving a turned single row with `offset` reduces the room left before the margin.
    let mut doc = text_page("BT /F1 12 Tf 0 1 -1 0 300 500 Tm (A single turned row) Tj ET");
    let moved = text::BlockStyle { offset: Some([0.0, 150.0]), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some(draft), &moved).unwrap();
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert!(lines.len() > 1, "{lines:?}");
    for line in &lines {
        assert!((line.rect[1] - 650.0).abs() < 0.5, "moved row starts at y = 650: {:?}", line.rect);
        assert!(line.rect[3] <= 800.0 - 36.0 + 0.5, "moved row stops at the top margin: {:?}", line.rect);
    }

    // A turned row that already starts inside the 36 pt margin keeps its own width instead of collapsing.
    let mut doc = text_page("BT /F1 12 Tf 0 1 -1 0 300 770 Tm (Edge label) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, Some("Edge label updated"), &text::BlockStyle::default()).unwrap();
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert!(lines.len() >= 2, "{lines:?}");
}

/// An added item is written out as a specific standard-14 face (`/Times-Italic`, `/Helvetica-Bold`
/// …), so it has to be *measured* with that same face. Measuring the upright face while writing
/// the italic one mis-sizes every box: Times-Italic `A` is 611 against Times-Roman's 722.
#[test]
fn an_items_measured_face_is_the_face_it_is_written_as() {
    for family in [Family::Helvetica, Family::Times, Family::Courier] {
        for bold in [false, true] {
            for italic in [false, true] {
                let written = family.base_font(bold, italic);
                let measured = family.std14(bold, italic);
                assert_eq!(written, measured.base_font(), "{family:?} bold={bold} italic={italic}");
                // ... and the public measure reaches that face, rather than an upright stand-in.
                assert_eq!(family.width("Anna", 12.0, bold, italic), measured.text_width("Anna", 12.0), "{family:?} bold={bold} italic={italic}");
            }
        }
    }
    // The faces really do differ, so the assertions above are not all comparing the same number.
    let times = |italic| Family::Times.width("Anna", 12.0, false, italic);
    assert_ne!(times(false), times(true), "Times-Italic is not Times-Roman");
    // Courier is monospaced in every style: equal widths here are correct, not a collapsed face.
    let courier = |italic| Family::Courier.width("Anna", 12.0, false, italic);
    assert_eq!(courier(false), courier(true));
}

#[test]
fn page_marks_and_added_content_preserve_fractional_user_unit_matrices() {
    for rotation in [0, 90] {
        let mut doc = fixture();
        let page = pdfcraft_model::pages(&doc)[2].obj;
        doc.update_dict(page, |d| {
            let bounds = Object::Array([0.0, 0.0, 0.1, 0.1].into_iter().map(Object::Real).collect());
            d.set(b"MediaBox".to_vec(), bounds.clone());
            d.set(b"CropBox".to_vec(), bounds);
            d.set(b"UserUnit".to_vec(), Object::Int(4096));
            d.set(b"Rotate".to_vec(), Object::Int(rotation));
        })
        .unwrap();
        let mut hf = HeaderFooter::default();
        hf.text[0] = "Matrix header".into();
        add_header_footer(&mut doc, &[2], &hf, false, &cx()).unwrap();
        let text = AddedText { rect: [72.0, 100.0, 200.0, 144.0], text: "Matrix text".into(), size: 12.0, ..AddedText::default() };
        add_content(&mut doc, 2, &Content::Text(text)).unwrap();
        let saved = reopen(&doc);
        let contents = streams(&saved, 2);
        for marker in ["/PCMark /HeaderFooter", "(Matrix text) Tj"] {
            let matching: Vec<_> = contents.iter().filter(|s| s.contains(marker)).collect();
            assert_eq!(matching.len(), 1, "{contents:?}");
            let stream = matching[0];
            let parsed = pdfcraft_content::parse(stream.as_bytes());
            assert_eq!(parsed.skipped, 0);
            let cm = parsed.ops.iter().find(|op| op.is("cm")).unwrap();
            assert_eq!(cm.operands.len(), 6);
            assert!(!stream[cm.span.clone()].contains('e') && !stream[cm.span.clone()].contains('E'), "PDF numbers cannot use exponents");
            let matrix = cm.nums::<6>().unwrap();
            let expected = if rotation == 0 {
                [0.000244140625, 0.0, 0.0, 0.000244140625, 0.0, 0.0]
            } else {
                [0.0, 0.000244140625, -0.000244140625, 0.0, 0.1, 0.0]
            };
            assert_eq!(matrix, expected, "rotation={rotation}, stream={stream}");
            // A fixed physical display point must remain at its intended raw page position.
            let raw = [matrix[0] * 72.0 + matrix[2] * 144.0 + matrix[4], matrix[1] * 72.0 + matrix[3] * 144.0 + matrix[5]];
            let expected_raw = if rotation == 0 { [0.017578125, 0.03515625] } else { [0.06484375, 0.017578125] };
            for (actual, expected) in raw.into_iter().zip(expected_raw) {
                assert!((actual - expected).abs() < 1e-12);
            }
        }
    }
}

#[test]
fn rows_the_edit_does_not_reach_keep_their_source_operators() {
    // Word gaps in TJ, as justified text from a layout program has; the text state alone
    // can't reproduce them, so a rewritten row would lose them.
    let rows = [
        "[(justified) -400 ( text) -400 ( uses) -400 ( wide) -400 ( gaps)] TJ",
        "[(between) -400 ( every) -400 ( single) -400 ( word) -400 ( here)] TJ",
        "(last line.) Tj",
    ];
    let content = format!("BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm {} T* {} T* {} ET", rows[0], rows[1], rows[2]);
    let row_texts = |doc: &Document| -> Vec<(String, [f64; 2])> {
        text::text_lines(doc, 0).unwrap().into_iter().map(|l| (l.text.clone(), l.baseline())).collect()
    };
    let source = row_texts(&text_page(&content));
    assert_eq!(source.len(), 3);
    for (row, from, to) in [(2, "last line.", "last line!"), (0, "justified", "justify"), (1, "single", "lone")] {
        let mut doc = text_page(&content);
        let block = text::text_blocks(&doc, 0).unwrap().into_iter().position(|b| b.text.contains("justified")).unwrap();
        let draft = text::text_blocks(&doc, 0).unwrap()[block].editor_text.replacen(from, to, 1);
        text::rewrite_block(&mut doc, 0, block, Some(&draft), &text::BlockStyle::default()).unwrap();
        let doc = reopen(&doc);
        let bytes = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
        let after = row_texts(&doc);
        assert_eq!(after.len(), 3, "{to}: {after:?}");
        for (i, ((text, at), (old, was))) in after.iter().zip(&source).enumerate() {
            assert!((at[0] - was[0]).abs() < 0.01 && (at[1] - was[1]).abs() < 0.01, "{to}: row {i} stays at {was:?}, not {at:?}");
            if i == row {
                assert_eq!(*text, old.replacen(from, to, 1), "{to}: the edited row");
                assert!(!bytes.contains(rows[i]), "{to}: the edited row is rewritten");
            } else {
                assert_eq!(text, old, "{to}: row {i}");
                assert_eq!(bytes.matches(rows[i]).count(), 1, "{to}: row {i} keeps its own operator, kerning included:\n{bytes}");
            }
        }
    }
    // Colouring a selection in the last row leaves the rows above it as they were.
    let mut doc = text_page(&content);
    let block = text::text_blocks(&doc, 0).unwrap().into_iter().position(|b| b.text.contains("justified")).unwrap();
    let draft = text::text_blocks(&doc, 0).unwrap()[block].editor_text.clone();
    let at = draft.find("line").unwrap();
    let range = text::RangeStyle {
        range: at..at + 4,
        family: None,
        resource: None,
        size: None,
        color: Some([1.0, 0.0, 0.0]),
        underline: None,
        char_spacing: None,
        scale: None,
    };
    text::rewrite_block(&mut doc, 0, block, Some(&draft), &text::BlockStyle { ranges: vec![range], ..Default::default() }).unwrap();
    let bytes = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(rows[..2].iter().all(|row| bytes.matches(row).count() == 1) && !bytes.contains(rows[2]), "{bytes}");
    assert!(bytes.contains("1 0 0 rg"), "{bytes}");
    // The coloured word is its own run now, so only the rows above compare whole.
    assert_eq!(row_texts(&reopen(&doc))[..2], source[..2]);
}

#[test]
fn kept_rows_stay_in_place_when_rows_wrap_or_move_with_quote_operators() {
    let lines_of = |doc: &Document| -> Vec<(String, [f64; 2])> {
        text::text_lines(doc, 0).unwrap().into_iter().map(|l| (l.text.clone(), l.baseline())).collect()
    };
    let edit = |content: &str, from: &str, to: &str| {
        let mut doc = text_page(content);
        let block = text::text_blocks(&doc, 0).unwrap().into_iter().position(|b| b.editor_text.contains(from)).unwrap();
        let draft = text::text_blocks(&doc, 0).unwrap()[block].editor_text.replacen(from, to, 1);
        text::rewrite_block(&mut doc, 0, block, Some(&draft), &text::BlockStyle::default()).unwrap();
        reopen(&doc)
    };
    // `'` moves to the next line as it shows: a row after a rewritten `'` row can't stay as it is.
    for (quoted, from, to) in [
        ("BT /F1 12 Tf 14 TL 72 700 Td (first row here) Tj (second row here) ' (third row here) ' ET", "second", "2nd"),
        ("BT /F1 12 Tf 14 TL 72 700 Td (first row here) Tj (second row here) ' (third row here) ' ET", "third", "3rd"),
        // Here the first row's own `'` makes the step down that the rows after it build on.
        ("BT /F1 12 Tf 14 TL 72 714 Td (first row here) ' (second row here) ' (third row here) ' ET", "first", "1st"),
    ] {
        let source = lines_of(&text_page(quoted));
        let after = lines_of(&edit(quoted, from, to));
        assert_eq!(after.len(), 3, "{to}: {after:?}");
        for ((text, at), (old, was)) in after.iter().zip(&source) {
            assert_eq!(*text, old.replacen(from, to, 1));
            assert!((at[0] - was[0]).abs() < 0.01 && (at[1] - was[1]).abs() < 0.01, "{to}: {text} stays at {was:?}, not {at:?}");
        }
    }
    // A row that grows past the paragraph's width wraps and pushes the rows below it down;
    // those are rewritten in their new places, never left behind as well.
    let rows = "BT /F1 12 Tf 14.4 TL 72 700 Td (one two three four) Tj T* (five six seven eight) Tj T* (nine ten eleven twelve) Tj ET";
    let doc = edit(rows, "two", "two and a great many more words");
    let text: String = lines_of(&doc).into_iter().map(|(t, _)| t + " ").collect();
    for word in ["one", "five", "nine", "twelve"] {
        assert_eq!(text.matches(word).count(), 1, "{word} appears once: {text}");
    }
    assert!(lines_of(&doc).len() > 3, "the edited row wrapped: {text}");
}

#[test]
fn a_new_character_comes_from_the_source_fonts_own_embedded_program() {
    let inter = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
    for (content, from, to) in [
        // One row (the single-style path) and a row of a two-row paragraph (the row editor's path).
        ("BT /F1 12 Tf 72 700 Td (Stable text) Tj ET", "text", "tőxt"),
        ("BT /F1 12 Tf 14 TL 72 700 Td (First row) Tj T* (Second row) Tj ET", "Second", "Sőcond"),
    ] {
        let mut doc = embedded_program_page(content, "Inter-Regular");
        let block = text::text_blocks(&doc, 0).unwrap()[0].clone();
        let draft = block.editor_text.replacen(from, to, 1);
        let edit = text::rewrite_block(&mut doc, 0, 0, Some(&draft), &text::BlockStyle::default()).unwrap();
        assert_eq!(edit.substituted, None, "{to}: no other family stands in");
        let doc = reopen(&doc);
        // The new character is its own run in the other resource, read back into the same word.
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].editor_text, draft, "{to}");
        let fonts = page_font_programs(&doc);
        assert!(fonts.iter().all(|(subtype, _)| subtype != b"Type3"), "{to}: no fallback glyphs");
        assert!(
            fonts.iter().any(|(subtype, program)| subtype == b"Type0" && program.as_deref() == Some(inter.as_slice())),
            "{to}: the source program draws it"
        );
    }
}

#[test]
fn a_shaped_run_is_one_text_object_with_each_glyph_where_shaping_put_it() {
    let inter = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Stable text) Tj ET");
    let resource = text::register_text_font(&mut doc, 0, inter).unwrap();
    // Marks shaped beside their letters (q́, x̣) and one stacked above another (ñ̄).
    let typed = "q\u{301}x\u{323} n\u{303}\u{304} Cafe\u{301}";
    let style = text::BlockStyle { width: Some(400.0), resource: Some(resource), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some(typed), &style).unwrap();
    assert_eq!(text::text_blocks(&reopen(&doc), 0).unwrap()[0].editor_text, typed);
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let start = ops.iter().position(|o| o.is("BDC")).unwrap();
    let end = start + ops[start..].iter().position(|o| o.is("EMC")).unwrap();
    let run = &ops[start..end];
    let count = |name: &str| run.iter().filter(|o| o.is(name)).count();
    assert_eq!((count("BT"), count("Tm"), count("Tj")), (1, 1, 0), "{run:?}");
    // Each glyph's origin, read back with the text-space displacements of ISO 32000-1 §9.4.4.
    let font = run.iter().find(|o| o.is("Tf")).unwrap();
    let (resource, size) = (font.operands[0].as_name().unwrap().to_vec(), font.operands[1].as_f64().unwrap());
    let page = pdfcraft_model::pages(&doc).remove(0);
    let resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let fonts = doc.resolve(resources.get(b"Font").unwrap()).as_dict().unwrap().clone();
    let metrics = pdfcraft_fonts::pdf::Metrics::from_dict(&doc, doc.resolve(fonts.get(&resource).unwrap()).as_dict().unwrap());
    let (mut x, mut rise, mut drawn) = (0.0, 0.0, Vec::new());
    for op in run {
        match op.op.as_slice() {
            b"Ts" => rise = op.operands[0].as_f64().unwrap(),
            b"Tc" | b"Tw" => assert_eq!(op.operands[0].as_f64(), Some(0.0), "{run:?}"),
            b"Tz" => assert_eq!(op.operands[0].as_f64(), Some(100.0), "{run:?}"),
            b"TJ" => {
                for item in op.operands[0].as_array().unwrap() {
                    match item.as_string() {
                        Some(s) => {
                            for code in s.bytes.chunks(2) {
                                drawn.push((x, rise));
                                x += metrics.width(u32::from(u16::from_be_bytes([code[0], code[1]]))) * size;
                            }
                        }
                        None => x -= item.as_f64().unwrap() / 1000.0 * size,
                    }
                }
            }
            _ => {}
        }
    }
    let shaped = pdfcraft_fonts::source_shaping::shape(inter, typed).unwrap();
    assert_eq!(drawn.len(), shaped.glyphs.len());
    assert!(shaped.glyphs.iter().any(|g| g.position[1] != 0.0), "a mark is raised");
    for (glyph, (x, y)) in shaped.glyphs.iter().zip(&drawn) {
        assert!((glyph.position[0] * size - x).abs() < 1e-3 && (glyph.position[1] * size - y).abs() < 1e-3, "{glyph:?} at {x}, {y}");
    }
    assert_eq!(rise, 0.0, "the rise is reset after the run");
}

#[test]
fn a_subset_program_is_not_trusted_for_new_characters_and_the_error_names_the_character() {
    let mut doc = embedded_program_page("BT /F1 12 Tf 72 700 Td (Stable text) Tj ET", "ABCDEF+Inter-Regular");
    // The subset's own program is not trusted for a character: a fallback face draws it through a
    // generated font instead (the subset may map a code whose outline was dropped). The Japanese
    // sans face leads the fallbacks when built with craft-fonts; the bundled Inter face otherwise.
    let edit = text::rewrite_block(&mut doc, 0, 0, Some("Stable tőxt"), &text::BlockStyle::default()).unwrap();
    let face = pdfcraft_fonts::document_japanese_font_for_style(false, false).map_or("Inter", |f| f.family);
    assert_eq!(edit.substituted, Some(format!("{face} Type3")));
    assert_eq!(text::text_blocks(&reopen(&doc), 0).unwrap()[0].editor_text, "Stable tőxt");
    // A character no fallback face has is refused, named, and left as it was.
    let mut doc = embedded_program_page("BT /F1 12 Tf 72 700 Td (Stable text) Tj ET", "ABCDEF+Inter-Regular");
    let before = page_content_bytes(&doc, 0);
    let result = text::rewrite_block(&mut doc, 0, 0, Some("Stable กtext"), &text::BlockStyle::default());
    let Err(EditError::Invalid(msg)) = result else { panic!("expected a clear error, got {result:?}") };
    assert!(msg.contains("ก (U+0E01)") && msg.contains("Load font") && !msg.contains("Japanese"), "{msg}");
    assert_eq!(page_content_bytes(&doc, 0), before);
}

#[test]
fn draft_layout_matches_the_paragraph_an_applied_edit_draws() {
    let content = "BT /F1 12 Tf 14 TL 72 700 Td (A first paragraph of words) Tj T* (with a second row) Tj ET \
                   BT /F1 12 Tf 72 600 Td (Another paragraph below) Tj ET BT /F1 12 Tf 320 700 Td (Side note) Tj ET";
    let doc = text_page(content);
    let block = text::text_blocks(&doc, 0).unwrap().into_iter().position(|b| b.text.starts_with("A first")).unwrap();
    let source = text::text_blocks(&doc, 0).unwrap()[block].clone();
    for draft in [
        source.editor_text.replacen("words", "many more words than before", 1),
        source.editor_text.replacen(" of words", "", 1),
        format!("{}\nand a third row", source.editor_text),
    ] {
        let layout = text::text_edit_layout(&doc, 0, block, &draft, &text::BlockStyle::default()).unwrap();
        // Apply the same edit and measure every run that isn't one of the page's other runs.
        let mut applied = doc.clone();
        text::rewrite_block(&mut applied, 0, block, Some(&draft), &text::BlockStyle::default()).unwrap();
        let others: Vec<_> =
            text::text_lines(&doc, 0).unwrap().into_iter().enumerate().filter(|(i, _)| !source.lines.contains(i)).map(|(_, l)| l.rect).collect();
        let drawn = text::text_lines(&applied, 0)
            .unwrap()
            .into_iter()
            .map(|l| l.rect)
            .filter(|r| !others.contains(r))
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
        assert_eq!(layout.draft_rect, drawn, "{draft:?}");
        assert_eq!(layout.source_rect, source.rect);
    }
}

#[test]
fn japanese_paragraphs_wrap_between_characters_without_closing_marks_at_a_row_start() {
    if pdfcraft_fonts::document_japanese_font().is_none() {
        eprintln!("skipping the Japanese wrapping checks: built without craft-fonts (set CRAFT_FONTS_DIR)");
        return;
    }
    let text = "日本語の文章は単語の間に空白がないので、どの文字の間でも行を折り返せます。ただし「句読点」や小さい「っ」は行頭に来ません。";
    // One paragraph (the single-style path), and a heading row before it (the row editor's path).
    for draft in [text.to_string(), format!("見出し\n{text}")] {
        let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Placeholder text) Tj ET");
        text::rewrite_block(&mut doc, 0, 0, Some(&draft), &text::BlockStyle { width: Some(120.0), ..Default::default() }).unwrap();
        let block = text::text_blocks(&reopen(&doc), 0).unwrap().remove(0);
        let rows: Vec<&str> = block.editor_text.split('\n').collect();
        assert!(rows.len() > 4, "{rows:?}");
        assert_eq!(rows.concat(), draft.replace('\n', ""), "no space is added between the pieces");
        assert!(block.rect[2] - block.rect[0] <= 120.5, "{:?} {rows:?}", block.rect);
        for row in &rows {
            assert!(!row.starts_with(['、', '。', '」', 'っ']) && !row.ends_with('「'), "{row:?} in {rows:?}");
        }
    }
}

#[test]
fn rewritten_paragraph_is_drawn_after_backgrounds_between_its_rows() {
    // Each row has its own background box drawn immediately before its BT..ET, as cards and
    // highlighted headings exported from LibreOffice or browsers do (#2377).
    let card = "0 0.45 0.25 rg 60 696 240 16 re f BT /F1 10 Tf 1 1 1 rg 64 700 Td (SEMINARIO ONLINE) Tj ET \
                0 0.45 0.25 rg 60 668 240 24 re f BT /F1 16 Tf 1 1 1 rg 64 674 Td (Dalla teoria alla pratica:) Tj ET \
                0 0.45 0.25 rg 60 632 240 32 re f BT /F1 24 Tf 1 1 1 rg 64 640 Td (gestire con metodo) Tj ET \
                0 0.45 0.25 rg 60 596 240 32 re f BT /F1 24 Tf 1 1 1 rg 64 604 Td (un progetto complesso) Tj ET";
    let shown_at = |ops: &[pdfcraft_content::Op], needle: &str| {
        ops.iter()
            .position(|op| {
                matches!(op.op.as_slice(), b"Tj" | b"TJ")
                    && op.operands.iter().any(|o| matches!(o, Object::String(s) if String::from_utf8_lossy(&s.bytes).contains(needle)))
            })
            .unwrap()
    };

    // 1. Editing the title rows emits all rewritten rows after all four background fills.
    let mut doc = text_page(card);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 1);
    let edited = "SEMINARIO ONLINE\nDalla teoria alla pratica:\ngestire con criterio\nun progetto nuovo";
    text::rewrite_block(&mut doc, 0, 0, Some(edited), &text::BlockStyle::default()).unwrap();
    let doc = reopen(&doc);
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].editor_text, edited);
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let fills: Vec<usize> = ops.iter().enumerate().filter_map(|(i, op)| op.is("f").then_some(i)).collect();
    assert_eq!(fills.len(), 4);
    assert!(
        shown_at(&ops, "criterio") > fills[3] && shown_at(&ops, "nuovo") > fills[3],
        "rewritten title rows must be drawn after their row background fills (fills at {fills:?}): {ops:?}"
    );

    // 2. Partial edit with a kept suffix row: rewritten rows 0..=2 are drawn after fills[0..=2]
    // and still before kept row 3, preserving reading order.
    let mut doc = text_page(card);
    let partial = "WORKSHOP ONLINE\nDalla teoria alla pratica:\ngestire con criterio\nun progetto complesso";
    text::rewrite_block(&mut doc, 0, 0, Some(partial), &text::BlockStyle::default()).unwrap();
    let doc = reopen(&doc);
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].editor_text, partial);
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let fills: Vec<usize> = ops.iter().enumerate().filter_map(|(i, op)| op.is("f").then_some(i)).collect();
    assert!(
        shown_at(&ops, "WORKSHOP") > fills[2]
            && shown_at(&ops, "pratica:") > fills[2]
            && shown_at(&ops, "criterio") > fills[2]
            && shown_at(&ops, "criterio") < shown_at(&ops, "complesso")
            && shown_at(&ops, "complesso") > fills[3],
        "rewritten prefix rows must follow their fills and precede the kept final row"
    );

    // 3. Editing row 2 so that it wraps pushes row 3 down; all rewritten rows must follow fills[3].
    let mut doc = text_page(card);
    let wrapped_edit = "SEMINARIO ONLINE\nDalla teoria alla pratica:\ngestire con criterio e metodo approfondito\nun progetto complesso";
    text::rewrite_block(&mut doc, 0, 0, Some(wrapped_edit), &text::BlockStyle::default()).unwrap();
    let doc = reopen(&doc);
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let last_fill = ops.iter().rposition(|op| op.is("f")).unwrap();
    assert!(
        shown_at(&ops, "approfondito") > last_fill && shown_at(&ops, "complesso") > last_fill,
        "wrapped rows and pushed-down suffix row must follow all row background fills"
    );

    // 4. Single-style paragraph path across a CTM change between rows, with movement, underline,
    // and a trailing unrelated line sharing the second row's BT..ET.
    let cross_ctm = "0.9 0.9 0.2 rg 60 696 240 18 re f BT /F1 12 Tf 0 0 0 rg 64 700 Td (First highlighted row) Tj ET \
                     q 1 0 0 1 10 12 cm 0.9 0.9 0.2 rg 50 668 240 18 re f \
                     BT /F1 12 Tf 0 0 0 rg 54 672 Td (Second highlighted row) Tj 246 -40 Td (Side footer) Tj ET Q";
    let mut doc = text_page(cross_ctm);
    let para = text::text_blocks(&doc, 0).unwrap().into_iter().position(|b| b.text.starts_with("First highlighted")).unwrap();
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[para].rows.len(), 2);
    text::rewrite_block(
        &mut doc,
        0,
        para,
        Some("Updated first highlighted row and updated second highlighted row"),
        &text::BlockStyle { offset: Some([6.0, -4.0]), underline: Some(true), ..Default::default() },
    )
    .unwrap();
    let doc = reopen(&doc);
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let last_fill = ops.iter().rposition(|op| op.is("f")).unwrap();
    assert!(
        shown_at(&ops, "Updated first") > last_fill && shown_at(&ops, "Side footer") > shown_at(&ops, "Updated first"),
        "cross-CTM single-style rewrite must follow both background fills and precede the shared BT's trailing line"
    );
    let lines = text::text_lines(&doc, 0).unwrap();
    let updated_first = lines.iter().find(|l| l.text.starts_with("Updated first")).unwrap();
    let side_footer = lines.iter().find(|l| l.text == "Side footer").unwrap();
    assert!(
        (updated_first.baseline()[0] - 70.0).abs() < 0.01 && (updated_first.baseline()[1] - 696.0).abs() < 0.01,
        "offset baseline in user space across CTM: {:?}",
        updated_first.baseline()
    );
    assert!(
        (side_footer.baseline()[0] - 310.0).abs() < 0.01 && (side_footer.baseline()[1] - 644.0).abs() < 0.01,
        "trailing line in shared BT keeps its tlm across CTM: {:?}",
        side_footer.baseline()
    );
}

#[test]
fn new_marks_isolate_state_appended_after_old_wrappers() {
    for dirt in ["0.5 0 0 0.5 0 0 cm", "0 0 200 200 re W n", "50 Tz", "5 Tc 20 Tw", "3 Tr", "/Pattern cs"] {
        for tag in ["", "OCR", "Wrap"] {
            let mut doc = isolation_fixture(&[
                ("Wrap", "q %PdfCraft\n"),
                ("", "2 0 0 2 0 0 cm"),
                ("Wrap", "Q %PdfCraft\n"),
                ("Added", "q /PCProbe Do Q"),
                (tag, dirt),
            ]);
            let before = probe_states(&doc);
            stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
            let after = probe_states(&reopen(&doc));
            assert_eq!(&after[..before.len()], &before, "existing content must not change");
            assert!(after.last().unwrap().is_empty(), "tag={tag}, dirt={dirt}: {after:?}");
        }
    }
}

#[test]
fn new_marks_isolate_dirty_tagged_only_pages() {
    let mut doc = isolation_fixture(&[("OCR", "50 Tz 5 Tc 20 Tw")]);
    stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    assert!(probe_states(&doc).last().unwrap().is_empty());
}

#[test]
fn new_marks_do_not_accumulate_wrappers_for_self_contained_marks() {
    let mut doc = isolation_fixture(&[("", "2 0 0 2 0 0 cm")]);
    for _ in 0..10 {
        stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    }
    assert_eq!(streams(&doc, 0).len(), 13);
    assert!(probe_states(&doc).iter().all(|s| s.is_empty()));
}

#[test]
fn new_marks_close_wrapper_after_an_unterminated_comment() {
    let mut doc = isolation_fixture(&[("", "50 Tz % no final newline")]);
    stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    // Check literal concatenation too: a separator must exist in the saved stream itself,
    // not just in the inspection parser, or the comment can swallow the closing Q.
    let bytes = streams(&doc, 0).concat();
    let parsed = pdfcraft_content::parse(bytes.as_bytes());
    assert_eq!(parsed.ops.iter().filter(|op| op.is("Q")).count(), 2);
    assert!(probe_states(&doc).last().unwrap().is_empty());
}

#[test]
fn new_marks_wrap_unbalanced_graphics_state_whole_without_changing_it() {
    // Content that can't be isolated precisely is wrapped whole, as before #3084, instead of
    // refusing: redaction stamps its boxes after removing the text, so it can't back out.
    for (data, probe_clean) in [("q 50 Tz", true), ("50 Tz q", true), ("q q Q", true), ("Q 50 Tz", false), ("q Q Q 50 Tz", false)] {
        let mut doc = isolation_fixture(&[("Wrap", data)]);
        let page = page_list(&doc).remove(0);
        place_tagged(&mut doc, &page, "Probe", b"q /PCProbe Do Q".to_vec(), false).unwrap();
        let after = streams(&doc, 0);
        assert_eq!(after.len(), 4, "{data}: {after:?}");
        assert_eq!((after[0].as_str(), after[1].as_str()), ("q %PdfCraft\n", data), "{data}");
        // A `q` the content leaves open is closed after it, so the mark starts clean. A stray
        // `Q` pops the wrapper itself: nothing placed after it can undo that.
        let probe = probe_states_lenient(&doc);
        assert_eq!(probe.last().unwrap().is_empty(), probe_clean, "{data}: {after:?}");
    }
}

#[test]
fn new_marks_preserve_cross_stream_operands_and_q_pairs() {
    let mut doc = isolation_fixture(&[("", "q 2 0 0 2"), ("", "0 0 cm Q 50"), ("", "Tz")]);
    stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    assert!(probe_states(&doc).last().unwrap().is_empty());
}

#[test]
fn removing_marks_keeps_wrappers_needed_by_later_external_content() {
    let mut doc = isolation_fixture(&[
        ("Wrap", "q %PdfCraft\n"),
        ("", "2 0 0 2 0 0 cm"),
        ("Wrap", "Q %PdfCraft\n"),
        ("HeaderFooter", "q /PCProbe Do Q"),
        ("", "/PCProbe Do"),
    ]);
    remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap();
    assert_eq!(probe_states(&doc).len(), 1);
    assert!(probe_states(&doc)[0].is_empty(), "the external content still needs the original reset");
}

#[test]
fn new_marks_wrap_unfinished_text_paths_and_operands_whole() {
    for data in ["BT", "ET", "BT BT ET", "0 0 10 10 re", "q 0 0 m Q", "50"] {
        let mut doc = isolation_fixture(&[("Background", "q Q"), ("", data)]);
        let page = page_list(&doc).remove(0);
        place_tagged(&mut doc, &page, "Probe", b"q /PCProbe Do Q".to_vec(), false).unwrap();
        // The behind-mark stays outside the wrapper and the content itself is untouched.
        assert_eq!(streams(&doc, 0), ["q Q", "q %PdfCraft\n", data, "\nQ %PdfCraft\n", "q /PCProbe Do Q"], "{data}");
    }
}

#[test]
fn new_marks_do_not_reveal_cross_stream_comments() {
    let mut doc = isolation_fixture(&[("", "q 50 Tz % comment continues"), ("", "Q 20 Tc\nQ 5 Tw")]);
    stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    assert!(probe_states(&doc).last().unwrap().is_empty());
    let joined = pdfcraft_content::Pieces::join(&streams(&doc, 0));
    assert!(!joined.parse().ops.iter().any(|op| op.is("Tc")), "commented content must remain inert");
}
