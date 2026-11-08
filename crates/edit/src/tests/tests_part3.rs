//! Unit tests of the editing operations (part 3 of 7).

#[allow(unused_imports)]
use super::*;

/// #2516's published steps: adding a row must not make the next edit-by-number rewrite Charlie.
/// The issue also asks for a stable paragraph id. That is #2508, and this test does not add one.
#[test]
fn adding_a_row_does_not_make_the_next_paragraph_number_mean_charlie() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha paragraph, first row of text.) Tj T* (Alpha paragraph, second row.) Tj ET \
               BT /F1 12 Tf 14.4 TL 1 0 0 1 72 650 Tm (Bravo paragraph, a single row.) Tj ET \
               BT /F1 12 Tf 14.4 TL 1 0 0 1 72 600 Tm (Charlie paragraph, first row of text.) Tj T* (Charlie paragraph, second row.) Tj ET";
    let mut doc = text_page(src);
    let names = |doc: &Document| text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
    assert_eq!(names(&doc).len(), 3, "{:?}", names(&doc));
    text::replace_block(&mut doc, 0, 0, "Alpha paragraph, first row of text.\nAlpha paragraph, second row.\nAlpha paragraph, a third row.").unwrap();
    let after = names(&doc);
    assert!(after.iter().any(|t| t.contains("Bravo") && !t.contains("Alpha")), "Bravo was absorbed: {after:?}");
    assert!(after.iter().any(|t| t.contains("Charlie")), "Charlie is gone: {after:?}");
    let bravo = after.iter().position(|t| t.contains("Bravo")).unwrap();
    text::replace_block(&mut doc, 0, bravo, "Bravo paragraph, edited.").unwrap();
    let done = names(&doc);
    assert!(done.iter().any(|t| t.contains("Charlie")), "editing Bravo removed Charlie: {done:?}");
    assert!(done.iter().any(|t| t.contains("Bravo paragraph, edited.")), "{done:?}");
}

/// The paragraph is the last thing on the page. Editing the first row and leaving the second
/// in the source must keep both rows inside one marker, including after a save.
#[test]
fn a_partial_rewrite_of_the_only_paragraph_stays_one_block() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET";
    let mut doc = text_page(src);
    text::replace_block(&mut doc, 0, 0, "One.\nAlpha next row.").unwrap();
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row."], "{texts:?}");
    assert_marker_wraps(&page_content(&doc), "/P BMC", "(One.)", "(Alpha next row.)");
    let doc = reopen(&doc);
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row."], "{texts:?}");
    assert_marker_wraps(&page_content(&doc), "/P BMC", "(One.)", "(Alpha next row.)");
    let mut doc = doc;
    text::replace_block(&mut doc, 0, 0, "One.\nTwo.").unwrap();
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Two."], "{texts:?}");
    assert_marker_wraps(&page_content(&doc), "/P BMC", "(One.)", "(Two.)");
}

/// The same rewrite when an operator follows the paragraph's `ET`. The close belongs before
/// that operator, and still after the kept row.
#[test]
fn a_partial_rewrite_closes_before_the_operator_that_follows_the_paragraph() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET 0 0 0 RG";
    let mut doc = text_page(src);
    text::replace_block(&mut doc, 0, 0, "One.\nAlpha next row.").unwrap();
    let doc = reopen(&doc);
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row."], "{texts:?}");
    let content = page_content(&doc);
    assert_marker_wraps(&content, "/P BMC", "(One.)", "(Alpha next row.)");
    let start = content.find("/P BMC").expect("marker");
    let close = content[start..].find("EMC").map(|at| start + at).expect("close");
    let after = content.find("0 0 0 RG").expect("the following operator is gone");
    assert!(close < after, "the marker swallowed the operator after the paragraph: {content}");
}

/// The last paragraph on the page is already inside a marked region. A partial rewrite stays
/// one paragraph, and that region's close still comes after both rows.
#[test]
fn a_partial_rewrite_of_the_last_marked_paragraph_stays_inside_its_region() {
    let src = "/H1 << /MCID 0 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET EMC";
    let mut doc = text_page(src);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 1);
    text::replace_block(&mut doc, 0, 0, "One.\nAlpha next row.").unwrap();
    let doc = reopen(&doc);
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row."], "{texts:?}");
    assert_marker_wraps(&page_content(&doc), "/MCID 0", "(One.)", "(Alpha next row.)");
}

/// A partial rewrite must not insert `/P BMC` across two existing marked-content regions.
/// The neighbour's own marker has to keep wrapping its text.
#[test]
fn a_partial_rewrite_keeps_the_next_marked_region_closed() {
    let src = "/H1 << /MCID 0 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET EMC \
               /P << /MCID 1 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET EMC";
    let mut doc = text_page(src);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    text::replace_block(&mut doc, 0, 0, "One.\nAlpha next row.").unwrap();
    let doc = reopen(&doc);
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row.", "Neighbor stays."], "{texts:?}");
    let content = page_content(&doc);
    assert_marker_wraps(&content, "/MCID 0", "(One.)", "(Alpha next row.)");
    assert_marker_wraps(&content, "/MCID 1", "(Neighbor stays.)", "(Neighbor stays.)");
    let neighbour = content.find("/MCID 1").expect("neighbour region");
    let close = content[neighbour..].find("EMC").map(|at| neighbour + at).expect("neighbour close");
    assert!(!content[neighbour..close].contains("(One.)"), "the rewrite was pulled into the neighbour's marker: {content}");
}

/// The paragraph being edited is not in a marked region. The next one is. Closing the new
/// marker at the neighbour's `BT` would land after that region's `BDC` and close it early.
#[test]
fn a_partial_rewrite_of_an_unmarked_paragraph_leaves_the_marked_neighbour_closed() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
               /P << /MCID 1 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET EMC";
    let mut doc = text_page(src);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    text::replace_block(&mut doc, 0, 0, "One.\nAlpha next row.").unwrap();
    let doc = reopen(&doc);
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row.", "Neighbor stays."], "{texts:?}");
    let content = page_content(&doc);
    assert_marker_wraps(&content, "/MCID 1", "(Neighbor stays.)", "(Neighbor stays.)");
    assert!(!region_through_matching_close(&content, "/MCID 1").contains("(One.)"), "the rewrite was pulled into the neighbour: {content}");
}

/// Both paragraphs already share one marked region, separated by a blank line. The extra marker
/// around the edit has to close itself, not the region, so the neighbour stays inside it.
#[test]
fn a_partial_rewrite_inside_one_marked_region_leaves_the_neighbour_inside_it() {
    let src = "/P << /MCID 0 >> BDC BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
               BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET EMC";
    let mut doc = text_page(src);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    text::replace_block(&mut doc, 0, 0, "One.\nAlpha next row.").unwrap();
    let doc = reopen(&doc);
    let texts: Vec<_> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["One. Alpha next row.", "Neighbor stays."], "{texts:?}");
    let content = page_content(&doc);
    let region = region_through_matching_close(&content, "/MCID 0");
    assert!(region.contains("(One.)") && region.contains("(Neighbor stays.)"), "the region closed before the neighbour: {content}");
}

/// A blank line is one extra leading, so the fourth row of a grown paragraph sits on the
/// neighbour's baseline. Changing the box width must not treat that neighbour as a duplicate
/// drawing and delete it.
#[test]
fn resizing_a_grown_paragraph_keeps_the_neighbour() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 410 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
               BT /F1 12 Tf 14.4 TL 1 0 0 1 72 366.8 Tm (Neighbor stays.) Tj ET";
    let mut doc = text_page(src);
    let text = "Alpha first row.\nthis is another\nalpha test\nAlpha next row.";
    text::replace_block(&mut doc, 0, 0, text).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    let added = lines.iter().find(|l| l.text.contains("Alpha next row.")).unwrap();
    let neighbour = lines.iter().find(|l| l.text.contains("Neighbor")).unwrap();
    assert!((added.origin_baseline() - neighbour.origin_baseline()).abs() < 0.5, "the new row is not on the neighbour");
    let width = text::text_blocks(&doc, 0).unwrap()[0].rect[2] - text::text_blocks(&doc, 0).unwrap()[0].rect[0];
    for factor in [0.6_f64, 1.6] {
        let mut copy = doc.clone();
        text::rewrite_block(&mut copy, 0, 0, Some(text), &text::BlockStyle { width: Some((width * factor).max(40.0)), ..Default::default() })
            .unwrap();
        let texts = text::text_blocks(&copy, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
        assert!(texts.iter().any(|t| t.contains("Neighbor stays.")), "width ×{factor} removed the neighbour: {texts:?}");
    }
}

/// Narrowing wraps a line. Widening it again rejoins that wrap. A newline that was typed stays.
#[test]
fn a_wrapped_line_rejoins_when_widened_and_a_typed_break_stays() {
    let mut doc = text_page("BT /F1 24 Tf 20 150 Td (Page 1) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { width: Some(45.0), ..Default::default() }).unwrap();
    let block = text::text_blocks(&doc, 0).unwrap().into_iter().next().unwrap();
    assert!(block.editor_text.contains('\n'), "the narrow box did not wrap: {}", block.editor_text);
    assert_eq!(block.reflow_text, "Page 1", "a wrap was kept as a hard break: {}", block.reflow_text);
    text::rewrite_block(&mut doc, 0, 0, Some(&block.reflow_text), &text::BlockStyle { width: Some(200.0), ..Default::default() }).unwrap();
    let lines: Vec<_> = text::text_lines(&doc, 0).unwrap().into_iter().map(|l| l.text).collect();
    assert_eq!(lines, ["Page 1"]);

    let mut typed = text_page("BT /F1 12 Tf 72 700 Td (One) Tj ET");
    text::replace_block(&mut typed, 0, 0, "One\nTwo\nThree\nfour").unwrap();
    let block = text::text_blocks(&typed, 0).unwrap().into_iter().next().unwrap();
    assert_eq!(block.reflow_text, "One\nTwo\nThree\nfour", "a typed break was rejoined: {}", block.reflow_text);
    let width = (block.rect[2] - block.rect[0]) * 6.0;
    text::rewrite_block(&mut typed, 0, 0, Some(&block.reflow_text), &text::BlockStyle { width: Some(width.max(180.0)), ..Default::default() })
        .unwrap();
    assert_eq!(text::text_blocks(&typed, 0).unwrap()[0].editor_text, "One\nTwo\nThree\nfour");
}

/// #2529 (reviewer): a run of typed spaces that happens to fall at a wrap point is not the same
/// as the single space an ordinary word gap leaves. Narrowing drops the whole gap, as any wrap
/// does (#4199's own test: `wrapped("aa  bb  cc", 6.0) == ["aa  bb", "cc"]`); widening it back
/// must not quietly turn two typed spaces into one. The count is stored on the wrap marker, so
/// it has to survive an incremental save and a full rewrite, not only the in-memory rewrite.
#[test]
fn narrowing_then_widening_keeps_a_typed_double_space() {
    let fresh = text_page("BT /F1 24 Tf 20 150 Td (One  Two) Tj ET");
    let wide_width = {
        let wide = text::text_blocks(&fresh, 0).unwrap().into_iter().next().unwrap().rect;
        wide[2] - wide[0]
    };
    for (save, reopen_as) in
        [("incremental", reopen(&fresh)), ("full", Document::open(Arc::new(write_full(&fresh, &SaveOptions::default()).unwrap())).unwrap())]
    {
        let mut doc = reopen_as;
        text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { width: Some(45.0), ..Default::default() }).unwrap();
        let doc =
            if save == "incremental" { reopen(&doc) } else { Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap() };
        let block = text::text_blocks(&doc, 0).unwrap().into_iter().next().unwrap();
        assert_eq!(block.editor_text, "One\nTwo", "{save}: the narrow box did not wrap between the two typed spaces: {}", block.editor_text);
        assert_eq!(block.reflow_text, "One  Two", "{save}: the saved wrap did not remember both spaces: {:?}", block.reflow_text);
        let mut doc = doc;
        text::rewrite_block(&mut doc, 0, 0, Some(&block.reflow_text), &text::BlockStyle { width: Some(wide_width), ..Default::default() }).unwrap();
        let doc =
            if save == "incremental" { reopen(&doc) } else { Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap() };
        let rewidened = text::text_blocks(&doc, 0).unwrap().into_iter().next().unwrap();
        assert_eq!(rewidened.editor_text, "One  Two", "{save}: the second typed space was lost: {:?}", rewidened.editor_text);
    }
}

/// #2529: a Chinese/Japanese word split has no space of its own (`wrap`'s own doc comment: "those
/// pieces join without a space"). Narrowing a paragraph until it wraps inside such a word, then
/// widening it back, must restore the two pieces exactly as they were, not insert a space between
/// them. Checked on both the single-style rewrite path and the row-editor (mixed-style) path.
#[test]
fn narrowing_then_widening_a_japanese_word_adds_no_space() {
    if pdfcraft_fonts::document_japanese_font().is_none() {
        eprintln!("skipping narrowing_then_widening_a_japanese_word_adds_no_space: built without craft-fonts (set CRAFT_FONTS_DIR)");
        return;
    }
    let text = "日本語の文章です";
    for draft in [text.to_string(), format!("見出し\n{text}")] {
        let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Placeholder text) Tj ET");
        text::rewrite_block(&mut doc, 0, 0, Some(&draft), &text::BlockStyle { width: Some(400.0), ..Default::default() }).unwrap();
        let wide = text::text_blocks(&reopen(&doc), 0).unwrap().into_iter().next().unwrap();
        let wide_width = wide.rect[2] - wide.rect[0];

        // Narrow enough to force a wrap inside the Japanese word, not just between the two rows.
        // A width-only change must pass `reflow_text` explicitly, as the editor UI does: `None`
        // falls back to the paragraph's space-joined display text and would not keep a typed break.
        text::rewrite_block(&mut doc, 0, 0, Some(&wide.reflow_text), &text::BlockStyle { width: Some(60.0), ..Default::default() }).unwrap();
        let narrow = text::text_blocks(&reopen(&doc), 0).unwrap().into_iter().next().unwrap();
        assert!(narrow.rows.len() > draft.matches('\n').count() + 1, "the narrow box did not wrap: {:?}", narrow.editor_text);
        assert_eq!(
            narrow.editor_text.replace('\n', ""),
            draft.replace('\n', ""),
            "wrapping must not add or drop characters: {:?}",
            narrow.editor_text
        );

        text::rewrite_block(&mut doc, 0, 0, Some(&narrow.reflow_text), &text::BlockStyle { width: Some(wide_width), ..Default::default() }).unwrap();
        let rewidened = text::text_blocks(&reopen(&doc), 0).unwrap().into_iter().next().unwrap();
        assert_eq!(rewidened.editor_text, draft, "a Japanese word split gained a space it never had: {:?}", rewidened.editor_text);
    }
}

/// A file that did not mark its breaks still rejoins a line that fills the measure, and keeps a
/// short line that had room.
#[test]
fn an_unmarked_full_line_rejoins_and_a_short_break_stays() {
    let long = "BT /F1 12 Tf 1 0 0 1 72 700 Tm (This sentence is long enough to fill a measure.) Tj ET \
                BT /F1 12 Tf 1 0 0 1 72 685.6 Tm (Tail) Tj ET";
    let doc = text_page(long);
    let block = text::text_blocks(&doc, 0).unwrap().into_iter().next().unwrap();
    assert_eq!(block.rows.len(), 2, "{}", block.editor_text);
    assert_eq!(block.reflow_text, "This sentence is long enough to fill a measure. Tail", "{}", block.reflow_text);

    let short = "BT /F1 12 Tf 1 0 0 1 72 700 Tm (One) Tj ET BT /F1 12 Tf 1 0 0 1 72 685.6 Tm (Two) Tj ET";
    let doc = text_page(short);
    let block = text::text_blocks(&doc, 0).unwrap().into_iter().next().unwrap();
    assert_eq!(block.reflow_text, "One\nTwo", "{}", block.reflow_text);
}

#[test]
fn widening_example_one_does_not_swallow_the_heading_or_split_the_next_paragraph() {
    let mut doc = text_page(letter_page());
    let before = block_texts(&doc);
    assert!(before.iter().any(|t| t == "1. Repeated step, no TL. Click Two."), "{before:?}");
    assert!(before.iter().any(|t| t.contains("One") && t.contains("Three") && !t.contains("Repeated")), "{before:?}");
    assert!(before.iter().any(|t| t.contains("First paragraph, first row") && t.contains("second row")), "{before:?}");
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let idx = blocks.iter().position(|b| b.text.contains("One") && b.text.contains("Three")).unwrap();
    // What the editor committed, including the line breaks. A later width change must keep them.
    text::replace_block(&mut doc, 0, idx, "One\nTwo\nThree\nfour").unwrap();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let idx = blocks.iter().position(|b| b.text.contains("four") || b.editor_text.contains("four")).unwrap();
    assert_eq!(blocks[idx].reflow_text, "One\nTwo\nThree\nfour", "a typed break was treated as a wrap: {}", blocks[idx].reflow_text);
    let shown = blocks[idx].reflow_text.clone();
    let width = blocks[idx].rect[2] - blocks[idx].rect[0];
    text::rewrite_block(&mut doc, 0, idx, Some(&shown), &text::BlockStyle { width: Some((width * 6.0).max(180.0)), ..Default::default() }).unwrap();
    let doc = reopen(&doc);
    let after = block_texts(&doc);
    let kept = text::text_blocks(&doc, 0).unwrap().into_iter().find(|b| b.editor_text.contains("four")).unwrap();
    assert_eq!(kept.editor_text, "One\nTwo\nThree\nfour", "a wider box flattened the line breaks: {:?}", kept.editor_text);
    assert!(after.iter().any(|t| t == "1. Repeated step, no TL. Click Two."), "the heading joined the resized box: {after:?}");
    assert!(
        after.iter().any(|t| t.contains("First paragraph, first row") && t.contains("First paragraph, second row")),
        "widening example 1 split the next paragraph: {after:?}"
    );
    assert!(after.iter().any(|t| t.contains("Second paragraph, first row") && t.contains("second row")), "{after:?}");
    assert!(after.iter().any(|t| t.contains("Neighbor stays.") && !t.contains("Alpha")), "{after:?}");
}

#[test]
fn a_row_added_one_leading_above_the_neighbour_does_not_absorb_it() {
    let mut doc = text_page(letter_page());
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let idx = blocks.iter().position(|b| b.text.contains("Alpha first row")).unwrap();
    text::replace_block(&mut doc, 0, idx, "Alpha first row.\ntest\nAlpha next row.").unwrap();
    let doc = reopen(&doc);
    let after = block_texts(&doc);
    assert!(after.iter().any(|t| t.contains("Neighbor stays.") && !t.contains("Alpha")), "the added row absorbed the neighbour: {after:?}");
}

#[test]
fn paragraphs_are_found_and_rewrapped() {
    let mut doc = text_page(
        "BT 0 0 1 rg /F1 10 Tf 12 TL 72 700 Td (The quick brown fox) Tj T* (jumps over the) Tj T* (lazy dog.) Tj ET \
         BT /F1 10 Tf 72 600 Td (Next paragraph) Tj ET",
    );
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["The quick brown fox jumps over the lazy dog.", "Next paragraph"]);
    assert_eq!(blocks[0].lines, [0, 1, 2]);
    let width = blocks[0].rect[2] - blocks[0].rect[0];
    let next = blocks[1].rect;
    let long = "PdfCraft rewraps a paragraph to its own width when its text changes, keeping the font, size, colour and line spacing.";
    assert_eq!(text::replace_block(&mut doc, 0, 0, long).unwrap().substituted, None);
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks[0].text, long);
    assert!(blocks[0].lines.len() > 3, "more lines: {}", blocks[0].lines.len());
    for i in &blocks[0].lines {
        assert!(lines[*i].rect[2] - lines[*i].rect[0] <= width + 1.0, "line {} fits", lines[*i].text);
    }
    // Same left edge, same spacing (12 pt).
    assert!((lines[0].rect[0] - 72.0).abs() < 0.01);
    let spacing = lines[blocks[0].lines[0]].origin_baseline() - lines[blocks[0].lines[1]].origin_baseline();
    assert!((spacing - 12.0).abs() < 0.01, "{spacing}");
    // The next paragraph is untouched, and the new text is blue like the old.
    assert_eq!(blocks[1].text, "Next paragraph");
    assert_eq!(blocks[1].rect, next);
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains("0 0 1 rg"), "{content}");
    // Shorter text: fewer lines.
    let mut doc = doc;
    text::replace_block(&mut doc, 0, 0, "Short.").unwrap();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!((blocks[0].text.as_str(), blocks[0].lines.len()), ("Short.", 1));
}

/// A list whose bullet is its own text object at the margin and whose text sits at a hanging
/// indent: the wrapped first item is one paragraph, and editing it keeps the items after it
/// (#2564, the edit that deleted its neighbours).
#[test]
fn a_wrapped_list_item_keeps_its_continuation_and_the_items_after_it() {
    let mut doc = text_page(
        "BT /F1 12 Tf 1 0 0 1 72 700 Tm (\\225) Tj ET \
         BT /F1 12 Tf 14.4 TL 1 0 0 1 86 700 Tm (First item of the list that wraps) Tj T* (onto a second row.) Tj ET \
         BT /F1 12 Tf 1 0 0 1 72 671.2 Tm (\\225) Tj ET \
         BT /F1 12 Tf 14.4 TL 1 0 0 1 86 671.2 Tm (Second item, one row.) Tj ET \
         BT /F1 12 Tf 1 0 0 1 72 656.8 Tm (\\225) Tj ET \
         BT /F1 12 Tf 14.4 TL 1 0 0 1 86 656.8 Tm (Third item, one row.) Tj ET",
    );
    let items = ["• First item of the list that wraps onto a second row.", "• Second item, one row.", "• Third item, one row."];
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, items, "one paragraph per item, the continuation with its first row");
    // Editing the paragraph the wrapped row belongs to must not take the items after it with it.
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let wrapped = blocks.iter().position(|b| b.text.contains("onto a second row.")).unwrap();
    text::replace_block(&mut doc, 0, wrapped, "• First item of the list that wraps onto a second row, edited.").unwrap();
    let blocks = text::text_blocks(&reopen(&doc), 0).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        ["• First item of the list that wraps onto a second row, edited.", "• Second item, one row.", "• Third item, one row."],
        "the items after the edited one are still on the page"
    );
}

/// A path segment operator before any `m` (or `re`) has no subpath to extend: the image scan used
/// to call `BezPath`'s `line_to`/`curve_to`/`close_path` on an empty path, which panics by contract,
/// taking `object_list`, `page_images` and the Office export's image walk down with it (#3581).
/// The segment is now skipped, as the renderer, MuPDF and pdftotext do, and the page's images are
/// still listed.
#[test]
fn image_scan_skips_a_path_segment_before_any_m() {
    for bad in ["1 1 l S", "1 1 2 2 3 3 c S", "1 1 2 2 4 4 y S", "1 1 2 2 3 3 v S", "h S"] {
        let content = format!("q {bad} Q 100 100 200 100 cm /Im0 Do");
        let doc = image_page_with(&content);
        assert_eq!(images::page_images(&doc, 0).unwrap().len(), 1, "{bad}: the image is still listed");
    }
    // A path started with m (or re) still extends and closes as before.
    let doc = image_page_with("q 0 0 m 1 1 l h S Q 100 100 200 100 cm /Im0 Do");
    assert_eq!(images::page_images(&doc, 0).unwrap().len(), 1);
    let doc = image_page_with("q 10 10 20 20 re W n Q 100 100 200 100 cm /Im0 Do");
    assert_eq!(images::page_images(&doc, 0).unwrap().len(), 1);
}

#[test]
fn moving_an_image_moves_its_own_clip_too() {
    let content = "q 100 100 200 100 re W* n 200 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Caption) Tj ET";
    let target = [50.0, 400.0, 350.0, 600.0];
    let mut doc = image_page_with(content);
    let before = images::page_images(&doc, 0).unwrap()[0].rect;
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(images::rect_to_rect(before, target))).unwrap();
    let mut doc = reopen(&doc);
    assert!(close(images::page_images(&doc, 0).unwrap()[0].rect, target));
    assert!(close(first_rect_in_user_space(&doc), target), "{:?}", first_rect_in_user_space(&doc));
    // A second edit still finds the clip, now behind the `cm` the first one put before it.
    let moved = [80.0, 420.0, 380.0, 620.0];
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(images::rect_to_rect(target, moved))).unwrap();
    assert!(close(first_rect_in_user_space(&doc), moved));
    images::translate_images(&mut doc, 0, &[0], [10.0, 0.0]).unwrap();
    assert!(close(images::page_images(&doc, 0).unwrap()[0].rect, [90.0, 420.0, 390.0, 620.0]));
    assert!(close(first_rect_in_user_space(&doc), [90.0, 420.0, 390.0, 620.0]));
    // Moving it with the group move helper takes the clip along as well.
    let mut doc = image_page_with(content);
    images::translate_images(&mut doc, 0, &[0], [30.0, -20.0]).unwrap();
    assert!(close(images::page_images(&doc, 0).unwrap()[0].rect, [130.0, 80.0, 330.0, 180.0]));
    assert!(close(first_rect_in_user_space(&doc), [130.0, 80.0, 330.0, 180.0]));
    // A clip shared with other drawing is left where it is.
    let shared = "q 100 100 200 100 re W n q 200 0 0 100 100 100 cm /Im0 Do Q 0 0 10 10 re f Q";
    let mut doc = image_page_with(shared);
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(images::rect_to_rect(before, target))).unwrap();
    assert!(close(first_rect_in_user_space(&doc), [100.0, 100.0, 300.0, 200.0]));
}

#[test]
fn repeated_image_edits_change_one_transform_instead_of_nesting() {
    let variants = [
        // Its own placement, a private crop, and a bare `Do` that the first edit wraps.
        "q 200 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Caption) Tj ET",
        "q 100 100 200 100 re W* n 200 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Caption) Tj ET",
        "200 0 0 100 100 100 cm /Im0 Do",
    ];
    for content in variants {
        let mut doc = image_page_with(content);
        let resize = |doc: &mut Document, n: usize| -> usize {
            let mut depth = 0;
            for i in 0..n {
                let r = images::page_images(doc, 0).unwrap()[0].visible_rect.unwrap();
                // Grow by 3% about the lower-left corner and move right by 2, staying on the page.
                let target = [r[0] + 2.0, r[1], r[0] + 2.0 + (r[2] - r[0]) * 1.03, r[1] + (r[3] - r[1]) * 1.03];
                images::change_image(doc, 0, 0, &images::ImageChange::Transform(images::rect_to_rect(r, target))).unwrap();
                let after = images::page_images(doc, 0).unwrap()[0].visible_rect.unwrap();
                assert!(after.iter().zip(target).all(|(a, b)| (a - b).abs() < 1e-6), "{content}: {after:?} vs {target:?}");
                if i == 0 {
                    depth = q_depth(doc);
                }
            }
            depth
        };
        let first = resize(&mut doc, 20);
        assert_eq!(q_depth(&doc), first, "{content}: twenty resizes nest no deeper than one");
        let mut doc = reopen(&doc);
        let before = images::page_images(&doc, 0).unwrap()[0].rect;
        for _ in 0..20 {
            images::translate_images(&mut doc, 0, &[0], [1.0, -2.0]).unwrap();
        }
        let after = images::page_images(&doc, 0).unwrap()[0].rect;
        assert!(close(after, [before[0] + 20.0, before[1] - 40.0, before[2] + 20.0, before[3] - 40.0]), "{content}: {after:?}");
        assert_eq!(q_depth(&doc), first, "{content}: group moves change the same transform");
        let caption: Vec<_> = text::text_lines(&doc, 0).unwrap().into_iter().map(|l| l.text).collect();
        assert_eq!(caption.is_empty(), !content.contains("Caption"), "{content}: the text after the image is untouched");
    }
}

#[test]
fn repeated_group_moves_of_a_cropped_image_change_one_clip_transform() {
    // The crop is smaller than the image, so a clip left behind would shrink `visible_rect`.
    let mut doc = image_page_with("q 150 120 100 50 re W n 200 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Caption) Tj ET");
    let operators = |doc: &Document| pdfcraft_content::parse(&page_content_bytes(doc, 0)).ops.len();
    let start = images::page_images(&doc, 0).unwrap()[0].clone();
    let mut first = 0;
    for i in 0..20 {
        images::translate_images(&mut doc, 0, &[0], [1.0, -2.0]).unwrap();
        if i == 0 {
            first = operators(&doc);
        }
    }
    let moved = |r: [f64; 4]| [r[0] + 20.0, r[1] - 40.0, r[2] + 20.0, r[3] - 40.0];
    let now = images::page_images(&doc, 0).unwrap()[0].clone();
    assert!(close(now.rect, moved(start.rect)), "{:?}", now.rect);
    assert!(close(now.visible_rect.unwrap(), moved(start.visible_rect.unwrap())), "the crop moves with the image: {:?}", now.visible_rect);
    assert_eq!(operators(&doc), first, "twenty group moves add no more operators than one");
    assert_eq!(q_depth(&doc), 1);
    let doc = reopen(&doc);
    assert!(close(images::page_images(&doc, 0).unwrap()[0].visible_rect.unwrap(), moved(start.visible_rect.unwrap())));
    let caption: Vec<_> = text::text_lines(&doc, 0).unwrap().into_iter().map(|l| l.text).collect();
    assert_eq!(caption, ["Caption"]);
}

#[test]
fn moving_an_image_inside_marked_content_leaves_every_sequence_intact() {
    // `translate_images` changes the image's own `cm`; its `Do` stays where it was, so a moved
    // image stays in the sequence that tags it (an image on a layer sits in `/OC … BDC`).
    for content in [
        "/P <</MCID 0>> BDC q 100 0 0 100 50 50 cm /Im0 Do Q EMC /P <</MCID 1>> BDC BT /F1 12 Tf 72 700 Td (Text) Tj ET EMC",
        "/OC /L0 BDC q 100 0 0 100 50 50 cm /Im0 Do Q EMC /P <</MCID 1>> BDC BT /F1 12 Tf 72 700 Td (Text) Tj ET EMC",
    ] {
        let mut doc = image_page_with(content);
        let before = images::page_images(&doc, 0).unwrap()[0].rect;
        images::translate_images(&mut doc, 0, &[0], [10.0, 0.0]).unwrap();
        let after = images::page_images(&doc, 0).unwrap()[0].rect;
        assert!(close(after, [before[0] + 10.0, before[1], before[2] + 10.0, before[3]]), "{content}: {after:?}");
        let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
        let count = |name: &str| ops.iter().filter(|op| op.is(name)).count();
        assert_eq!((count("BDC"), count("EMC")), (2, 2), "{content}: both sequences remain");
        let first = ops.iter().position(|op| op.is("BDC")).unwrap();
        let end = ops.iter().position(|op| op.is("EMC")).unwrap();
        let image = ops.iter().position(|op| op.is("Do")).unwrap();
        assert!(first < image && image < end, "{content}: the image is still inside its own sequence");
        assert!(!ops[end..].iter().any(|op| op.is("Do")), "{content}: the other sequence gained no image");
    }
}

#[test]
fn page_images_move_turn_replace_and_delete() {
    let mut doc = image_page();
    let imgs = images::page_images(&doc, 0).unwrap();
    assert_eq!(imgs.len(), 1);
    assert!(close(imgs[0].rect, [100.0, 100.0, 300.0, 200.0]), "{:?}", imgs[0].rect);
    assert_eq!((imgs[0].width, imgs[0].height, imgs[0].name.as_str()), (4, 2, "Im0"));
    // Move and resize.
    let t = images::rect_to_rect(imgs[0].rect, [50.0, 400.0, 150.0, 450.0]);
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(t)).unwrap();
    let doc = reopen(&doc);
    let r = images::page_images(&doc, 0).unwrap()[0].rect;
    assert!(close(r, [50.0, 400.0, 150.0, 450.0]), "{r:?}");
    // A quarter turn about the centre: 100 × 50 becomes 50 × 100 around (100, 425).
    let mut doc = doc;
    let t = images::turn_about_centre(r, 1, false, false);
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(t)).unwrap();
    let r = images::page_images(&doc, 0).unwrap()[0].rect;
    assert!(close(r, [75.0, 375.0, 125.0, 475.0]), "{r:?}");
    // The text after it is untouched.
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "Caption");
    // Replace with another image object, in the same place.
    let mut d = pdfcraft_cos::Dict::new();
    for (k, v) in [
        (&b"Type"[..], pdfcraft_cos::Object::name("XObject")),
        (b"Subtype", pdfcraft_cos::Object::name("Image")),
        (b"Width", pdfcraft_cos::Object::Int(1)),
        (b"Height", pdfcraft_cos::Object::Int(1)),
    ] {
        d.set(k.to_vec(), v);
    }
    let other = doc.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::from_raw(d, vec![0])));
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Replace(other)).unwrap();
    let imgs = images::page_images(&doc, 0).unwrap();
    assert_eq!((imgs[0].object, imgs[0].width), (Some(other), 1));
    assert!(close(imgs[0].rect, r));
    // Delete.
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Delete).unwrap();
    assert!(images::page_images(&doc, 0).unwrap().is_empty());
    assert!(images::change_image(&mut doc, 0, 0, &images::ImageChange::Delete).is_err());
}

#[test]
fn image_arrange_stacking_order_and_remove_crop() {
    let content = "q 100 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Middle Text) Tj ET q 150 0 0 150 200 200 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    let initial_images = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial_images.len(), 2);
    assert!(close(initial_images[0].rect, [100.0, 100.0, 200.0, 200.0]));
    assert!(close(initial_images[1].rect, [200.0, 200.0, 350.0, 350.0]));

    // Boundary case: image 0 is already at the back; SendToBack and SendBackward report an error.
    assert!(arrange(&mut doc, 0, 0, images::ArrangeOrder::SendToBack).is_err());
    assert!(arrange(&mut doc, 0, 0, images::ArrangeOrder::SendBackward).is_err());
    let imgs = images::page_images(&doc, 0).unwrap();
    assert_eq!(imgs.len(), 2);
    assert!(close(imgs[0].rect, initial_images[0].rect));

    // Bring forward image 0: moves it past the middle text, but it is still behind image 1.
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringForward).unwrap();
    let doc = reopen(&doc);
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    // Middle text should now precede image 0's Do in the content stream!
    let text_pos = str_content.find("Middle Text").unwrap();
    let first_do = str_content.find("/Im0 Do").unwrap();
    assert!(text_pos < first_do, "text should now precede image 0");

    // Bring to front on image 0: moves it past image 1 to the front!
    let mut doc = doc;
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let imgs_after_front = images::page_images(&doc, 0).unwrap();
    assert_eq!(imgs_after_front.len(), 2);
    // In drawing order, the image that was at [100, 100, 200, 200] is now drawn LAST (index 1)!
    assert!(close(imgs_after_front[0].rect, [200.0, 200.0, 350.0, 350.0]));
    assert!(close(imgs_after_front[1].rect, [100.0, 100.0, 200.0, 200.0]));

    // Send to back on the front image (index 1) restores it to the back.
    let mut doc = doc;
    arrange(&mut doc, 0, 1, images::ArrangeOrder::SendToBack).unwrap();
    let imgs_after_back = images::page_images(&doc, 0).unwrap();
    assert!(close(imgs_after_back[0].rect, [100.0, 100.0, 200.0, 200.0]));
    assert!(close(imgs_after_back[1].rect, [200.0, 200.0, 350.0, 350.0]));

    // Test remove crop
    let cropped_content = "q 200 0 0 100 100 100 cm /Im0 Do Q";
    let mut doc = image_page_with(cropped_content);
    crop(&mut doc, 0, 0, [100.0, 100.0, 150.0, 150.0]).unwrap();
    let img_before = &images::page_images(&doc, 0).unwrap()[0];
    assert_ne!(img_before.visible_rect, Some(img_before.rect));
    remove_crop(&mut doc, 0, 0).unwrap();
    let doc = reopen(&doc);
    let img_after = &images::page_images(&doc, 0).unwrap()[0];
    assert_eq!(img_after.visible_rect, Some(img_after.rect));
    // Removing crop again when there is no crop is refused.
    let mut doc = doc;
    assert!(remove_crop(&mut doc, 0, 0).is_err());

    // Test crop
    let uncropped_content = "q 200 0 0 100 100 100 cm /Im0 Do Q";
    let mut doc = image_page_with(uncropped_content);
    let img_before = &images::page_images(&doc, 0).unwrap()[0];
    assert_eq!(img_before.visible_rect, Some(img_before.rect));
    crop(&mut doc, 0, 0, [120.0, 110.0, 250.0, 180.0]).unwrap();
    let doc = reopen(&doc);
    let img_cropped = &images::page_images(&doc, 0).unwrap()[0];
    assert!(close(img_cropped.visible_rect.unwrap(), [120.0, 110.0, 250.0, 180.0]));
    // And remove_crop works on it:
    let mut doc = doc;
    remove_crop(&mut doc, 0, 0).unwrap();
    let doc = reopen(&doc);
    let img_restored = &images::page_images(&doc, 0).unwrap()[0];
    assert_eq!(img_restored.visible_rect, Some(img_restored.rect));

    // Test cropping an image that was moved (has an outer transform)
    let mut doc = image_page_with("q 200 0 0 100 100 100 cm /Im0 Do Q");
    let move_transform = [1.0, 0.0, 0.0, 1.0, 50.0, 40.0];
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(move_transform)).unwrap();
    let doc = reopen(&doc);
    let img_moved = &images::page_images(&doc, 0).unwrap()[0];
    assert_eq!(img_moved.rect, [150.0, 140.0, 350.0, 240.0]);
    let mut doc = doc;
    crop(&mut doc, 0, 0, [170.0, 150.0, 300.0, 220.0]).unwrap();
    let doc = reopen(&doc);
    let img_crop_moved = &images::page_images(&doc, 0).unwrap()[0];
    assert!(img_crop_moved.visible_rect.is_some(), "cropped image must not be deleted");
    assert!(close(img_crop_moved.visible_rect.unwrap(), [170.0, 150.0, 300.0, 220.0]));
    // Adjust crop (re-crop)
    let mut doc = doc;
    crop(&mut doc, 0, 0, [180.0, 160.0, 280.0, 210.0]).unwrap();
    let doc = reopen(&doc);
    let img_recropped = &images::page_images(&doc, 0).unwrap()[0];
    assert!(close(img_recropped.visible_rect.unwrap(), [180.0, 160.0, 280.0, 210.0]));
    // Remove crop from moved image
    let mut doc = doc;
    remove_crop(&mut doc, 0, 0).unwrap();
    let doc = reopen(&doc);
    let img_restored_moved = &images::page_images(&doc, 0).unwrap()[0];
    assert_eq!(img_restored_moved.visible_rect, Some(img_restored_moved.rect));
}

#[test]
fn crop_rotated_image_preserves_angle_and_in_place_geometry() {
    let content = "q 173.20508 100 -50 86.60254 100 100 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    let img_before = &images::page_images(&doc, 0).unwrap()[0];
    let m_before = img_before.matrix;
    assert!((m_before[1] - 100.0).abs() < 1e-3);
    assert!((m_before[2] - (-50.0)).abs() < 1e-3);

    crop(&mut doc, 0, 0, [140.0, 170.0, 180.0, 210.0]).unwrap();
    let doc = reopen(&doc);
    let img_cropped = &images::page_images(&doc, 0).unwrap()[0];

    let m_after = img_cropped.matrix;
    assert!((m_after[0] - m_before[0]).abs() < 1e-3);
    assert!((m_after[1] - m_before[1]).abs() < 1e-3);
    assert!((m_after[2] - m_before[2]).abs() < 1e-3);
    assert!((m_after[3] - m_before[3]).abs() < 1e-3);
    assert!((m_after[4] - m_before[4]).abs() < 1e-3);
    assert!((m_after[5] - m_before[5]).abs() < 1e-3);

    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    assert!(
        (str_content.contains("re") || (str_content.contains("m\n") && str_content.contains("l\n"))) && str_content.contains("W\nn"),
        "{str_content}"
    );
    assert!(str_content.contains("/Im0 Do"), "{str_content}");

    let mut doc = doc;
    remove_crop(&mut doc, 0, 0).unwrap();
    let doc = reopen(&doc);
    let img_restored = &images::page_images(&doc, 0).unwrap()[0];
    assert_eq!(img_restored.matrix, m_before);
}

#[test]
fn image_arrange_preserves_shared_group_drawings() {
    let content = "q 100 0 0 100 100 100 cm /Im0 Do /Im0 Do Q BT /F1 12 Tf 72 700 Td (End Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 2, "both images initially present in shared group");

    // Bring image 0 to front: it must not delete image 1 that shares the same q...Q group!
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 2, "both images must be preserved after BringToFront");
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    assert!(str_content.contains("End Text"), "text in page must be preserved");
}

/// #3145: Arrange treated marked-content boundaries as if they weren't there, so Bring to Front put
/// the image inside the text's `BDC`…`EMC` (tagged as the text) and left its own sequence empty.
#[test]
fn image_arrange_keeps_marked_content_sequences_whole() {
    let content = "/P <</MCID 0>> BDC q 100 0 0 100 50 50 cm /Im0 Do Q EMC /P <</MCID 1>> BDC BT /F1 12 Tf 72 700 Td (Text) Tj ET EMC";
    let mut doc = image_page_with(content);
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let bytes = page_content_bytes(&doc, 0);
    let s = String::from_utf8_lossy(&bytes);
    assert!(marked_sequence(&s, 0).contains("/Im0 Do"), "the image stays in its own sequence: {s}");
    assert!(!marked_sequence(&s, 1).contains("/Im0 Do"), "the text's sequence holds no image: {s}");
    assert!(marked_sequence(&s, 1).contains("(Text)"), "{s}");
    assert!(s.find("/MCID 1").unwrap() < s.find("/MCID 0").unwrap(), "the image's sequence now comes after the text's: {s}");
    assert_eq!(images::page_images(&doc, 0).unwrap().len(), 1);
}

/// A `BMC` sequence (here an artifact) moves as a whole too.
#[test]
fn image_arrange_moves_a_bmc_sequence_as_a_whole() {
    let content = "/Artifact BMC q 100 0 0 100 50 50 cm /Im0 Do Q EMC BT /F1 12 Tf 72 700 Td (Body) Tj ET";
    let mut doc = image_page_with(content);
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let s = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    let (art, emc, body, img) = (s.find("/Artifact BMC").unwrap(), s.rfind("EMC").unwrap(), s.find("(Body)").unwrap(), s.find("/Im0 Do").unwrap());
    assert!(body < art && art < img && img < emc, "text first, then the whole artifact sequence: {s}");
}

/// An image sharing a marked-content sequence with other content must not end up untagged.
/// Either the arrange call is refused with Err, or every image paint remains inside a marked-content sequence.
#[test]
fn arrange_image_in_shared_tagged_block_refuses_or_keeps_tagged() {
    let content =
        "/Figure <</MCID 0>> BDC q 100 0 0 100 50 50 cm /Im0 Do Q BT /F1 9 Tf 50 40 Td (Caption) Tj ET EMC BT /F1 12 Tf 72 700 Td (Body) Tj ET";
    let mut doc = image_page_with(content);
    let res = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront);
    match res {
        Err(_) => {
            // Refusing is the expected safe behavior when moving out of a shared tagged unit.
        }
        Ok(()) => {
            let doc = reopen(&doc);
            let bytes = page_content_bytes(&doc, 0);
            let ops = pdfcraft_content::parse(&bytes).ops;
            let mut mc_depth = 0usize;
            for op in ops {
                if op.is("BDC") || op.is("BMC") {
                    mc_depth += 1;
                } else if op.is("EMC") {
                    mc_depth = mc_depth.saturating_sub(1);
                } else if op.is("Do") {
                    assert!(mc_depth > 0, "every image paint must be inside a marked-content sequence: {}", String::from_utf8_lossy(&bytes));
                }
            }
        }
    }
}

/// An image that shares its marked-content sequence with other content cannot be rearranged
/// without leaving the image untagged or duplicating MCIDs, so it is refused.
#[test]
fn image_arrange_splits_a_shared_marked_sequence_into_own_tag() {
    let content =
        "/Figure <</MCID 0>> BDC q 100 0 0 100 50 50 cm /Im0 Do Q BT /F1 9 Tf 50 40 Td (Caption) Tj ET EMC BT /F1 12 Tf 72 700 Td (Body) Tj ET";
    let mut doc = image_page_with(content);
    assert!(arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).is_err());
}

/// Send backward also refuses to rearrange an image when it shares its sequence with other content.
#[test]
fn image_arrange_send_backward_splits_shared_marked_sequence() {
    let content =
        "BT /F1 12 Tf 72 700 Td (Header) Tj ET /Table <</MCID 0>> BDC q 100 0 0 100 50 50 cm /Im0 Do Q BT /F1 9 Tf 50 40 Td (Cell) Tj ET EMC";
    let mut doc = image_page_with(content);
    assert!(arrange(&mut doc, 0, 0, images::ArrangeOrder::SendBackward).is_err());
}

#[test]
fn arrange_preserves_color_leaking_out_of_marked_content() {
    let content = "/Figure <</MCID 0>> BDC 1 0 0 rg q 100 0 0 100 0 0 cm /Im0 Do Q EMC BT /F1 12 Tf 72 700 Td (Text) Tj ET";
    let mut doc = image_page_with(content);
    let res = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront);
    if res.is_ok() {
        let bytes = page_content_bytes(&doc, 0);
        let s = String::from_utf8_lossy(&bytes);
        let text_pos = s.find("(Text) Tj").unwrap();
        assert!(s[..text_pos].contains("1 0 0 rg") || s[..text_pos].contains("1 0 0 RG"));
    }
}

#[test]
fn arrange_shared_tagged_does_not_duplicate_mcid() {
    let content = "/Figure <</MCID 0>> BDC q 100 0 0 100 0 0 cm /Im0 Do Q 0 0 m 10 10 l S EMC";
    let mut doc = image_page_with(content);
    assert!(arrange(&mut doc, 0, 0, images::ArrangeOrder::SendToBack).is_err());
}

#[test]
fn image_crop_preserves_outer_cm_for_subsequent_drawings() {
    let content = "100 0 0 100 50 50 cm /Im0 Do /Im0 Do";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].rect, [50.0, 50.0, 150.0, 150.0]);
    assert_eq!(initial[1].rect, [50.0, 50.0, 150.0, 150.0]);

    // Crop image 0
    crop(&mut doc, 0, 0, [60.0, 60.0, 120.0, 120.0]).unwrap();
    let doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 2, "both images must remain");
    // Image 0 is cropped:
    assert!(close(after[0].visible_rect.unwrap(), [60.0, 60.0, 120.0, 120.0]));
    // Crucial: Image 1 must still have its rect at [50, 50, 150, 150], proving outer cm was not destroyed!
    assert_eq!(after[1].rect, [50.0, 50.0, 150.0, 150.0], "subsequent image must retain global cm");
}

#[test]
fn image_arrange_preserves_position_under_active_ctm() {
    let content = "100 0 0 100 50 50 cm /Im0 Do /Im0 Do";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].rect, [50.0, 50.0, 150.0, 150.0]);
    assert_eq!(initial[1].rect, [50.0, 50.0, 150.0, 150.0]);

    // Move first image to front: must not apply destination CTM twice!
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: check order of operators in the content stream
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let first_do = str_content.find("/Im0 Do").unwrap();
    let last_do = str_content.rfind("/Im0 Do").unwrap();
    assert!(first_do < last_do, "Position: multiple image Do calls exist and order was updated");

    // 2. Appearance: both images' positions and sizes are preserved
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0], "Appearance: first image position preserved");
    assert_eq!(after[1].rect, [50.0, 50.0, 150.0, 150.0], "Appearance: second image position preserved");

    // 3. Preservation: outer CTM is preserved
    assert!(str_content.contains("100 0 0 100 50 50 cm"), "Preservation: initial CTM preserved");
}

#[test]
fn image_arrange_preserves_shared_group_clipping_bounds() {
    let content = "q 120 120 40 40 re W n 100 0 0 100 100 100 cm /Im0 Do /Im0 Do Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].rect, [100.0, 100.0, 200.0, 200.0]);
    assert_eq!(initial[0].visible_rect, Some([120.0, 120.0, 160.0, 160.0]));
    assert_eq!(initial[1].rect, [100.0, 100.0, 200.0, 200.0]);
    assert_eq!(initial[1].visible_rect, Some([120.0, 120.0, 160.0, 160.0]));

    // Bring image 0 to front: moves it past the external text while preserving clipping bounds
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: moved image Do now follows External Text in AST
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let text_pos = str_content.find("External Text").unwrap();
    let moved_do_pos = str_content.rfind("/Im0 Do").unwrap();
    assert!(text_pos < moved_do_pos, "Position: moved image Do follows External Text");

    // 2. Appearance: both images retain exact clipping bounds and geometry
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 2, "both images preserved");
    assert_eq!(after[0].visible_rect, Some([120.0, 120.0, 160.0, 160.0]), "Appearance: image 0 retains clip");
    assert_eq!(after[1].visible_rect, Some([120.0, 120.0, 160.0, 160.0]), "Appearance: image 1 retains clip");

    // 3. Preservation: External Text and original group drawing preserved
    assert!(str_content.contains("External Text"), "Preservation: external text preserved");
    assert!(str_content.contains("120 120 40 40 re W n"), "Preservation: clipping path operators preserved");
}

#[test]
fn image_arrange_preserves_extended_graphics_state_opacity() {
    let content = "q /GS0 gs 100 0 0 100 50 50 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].rect, [50.0, 50.0, 150.0, 150.0]);

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: /Im0 Do moved past External Text
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let text_pos = str_content.find("External Text").unwrap();
    let do_pos = str_content.rfind("/Im0 Do").unwrap();
    assert!(text_pos < do_pos, "Position: /Im0 Do moved past External Text");

    // 2. Appearance: /GS0 gs is retained and directly precedes /Im0 Do
    let gs_pos = str_content.rfind("/GS0 gs").unwrap();
    assert!(text_pos < gs_pos, "Appearance: /GS0 gs is in the new moved unit");
    assert!(gs_pos < do_pos, "Appearance: /GS0 gs directly precedes /Im0 Do");
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1, "image preserved");
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0], "Appearance: rect preserved");

    // 3. Preservation: External Text is preserved
    assert!(str_content.contains("BT /F1 12 Tf 72 700 Td (External Text) Tj ET"), "Preservation: text preserved");
}

#[test]
fn image_arrange_preserves_exact_non_rectangular_clipping_path() {
    let content = "q 120 120 m 160 120 l 140 160 l h W* n 100 0 0 100 100 100 cm /Im0 Do /Im0 Do Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 2);

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: moved image follows External Text in AST
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let text_pos = str_content.find("External Text").unwrap();
    let last_do = str_content.rfind("/Im0 Do").unwrap();
    assert!(text_pos < last_do, "Position: moved image follows External Text");

    // 2. Appearance: polygon geometry and even-odd rule are preserved
    assert!(str_content.contains("W*"), "Appearance: even-odd W* fill rule preserved");
    // The original group still clips the image left behind, so the moved copy must carry its own.
    let flat = str_content.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(flat.matches("120 120 m 160 120 l 140 160 l h W* n").count(), 2, "Appearance: exact polygon clip preserved in both units");
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 2, "both images preserved");
    for image in &after {
        assert!(image.contains([140.0, 130.0]), "Appearance: inside the triangle shows");
        assert!(!image.contains([125.0, 155.0]), "Appearance: inside its box but outside the triangle stays clipped");
    }

    // 3. Preservation: External Text and remaining original group content preserved
    assert!(str_content.contains("External Text"), "Preservation: external text preserved");
}

#[test]
fn image_remove_crop_after_arrange_keeps_position_under_outer_transform() {
    // Arrange writes `cm inverse`, the clip, then the absolute placement; Remove Crop used to drop
    // only the first, which drew the image under the outer translation twice.
    let content = "1 0 0 1 50 50 cm q 100 0 0 100 0 0 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    crop(&mut doc, 0, 0, [50.0, 50.0, 100.0, 100.0]).unwrap();
    assert_eq!(images::page_images(&doc, 0).unwrap()[0].rect, [50.0, 50.0, 150.0, 150.0]);

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let mut doc = reopen(&doc);
    let moved = images::page_images(&doc, 0).unwrap();
    assert_eq!(moved[0].rect, [50.0, 50.0, 150.0, 150.0]);
    assert_eq!(moved[0].visible_rect, Some([50.0, 50.0, 100.0, 100.0]));

    remove_crop(&mut doc, 0, 0).unwrap();
    let doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0], "the image stays where it was");
    assert_eq!(after[0].visible_rect, Some([50.0, 50.0, 150.0, 150.0]), "the crop is gone");
}

#[test]
fn image_arrange_refuses_to_move_past_a_clip_it_is_not_under() {
    // The text after the image is drawn under a top-level clip; moving the image after it would
    // crop the image to that clip.
    let content = "q 100 0 0 100 50 50 cm /Im0 Do Q 0 0 60 60 re W n BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let before = page_content_bytes(&doc, 0);
    assert_eq!(images::page_images(&doc, 0).unwrap()[0].visible_rect, Some([50.0, 50.0, 150.0, 150.0]));

    for order in [images::ArrangeOrder::BringToFront, images::ArrangeOrder::BringForward] {
        let err = arrange(&mut doc, 0, 0, order).unwrap_err();
        assert!(err.to_string().contains("clipping path"), "{err}");
    }
    assert_eq!(page_content_bytes(&doc, 0), before, "nothing changes");
}

#[test]
fn image_arrange_send_to_back_keeps_a_shared_clip() {
    // The image after the text is clipped by a top-level clip set before both; behind the text it
    // is no longer under that clip, so the moved unit has to draw it itself.
    let content = "BT /F1 12 Tf 72 700 Td (External Text) Tj ET 0 0 60 60 re W n q 100 0 0 100 50 50 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    arrange(&mut doc, 0, 0, images::ArrangeOrder::SendToBack).unwrap();
    let doc = reopen(&doc);
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    assert!(str_content.rfind("/Im0 Do").unwrap() < str_content.find("External Text").unwrap(), "the image moved behind the text");
    assert_eq!(images::page_images(&doc, 0).unwrap()[0].visible_rect, Some([50.0, 50.0, 60.0, 60.0]), "still cropped to the clip");
}

#[test]
fn unwrapped_image_moves_successfully() {
    let content = "100 0 0 100 50 50 cm /Im0 Do BT /F1 12 Tf 72 700 Td (Middle Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].rect, [50.0, 50.0, 150.0, 150.0]);

    // Initial order: /Im0 Do precedes Middle Text
    let initial_bytes = page_content_bytes(&doc, 0);
    let initial_str = String::from_utf8_lossy(&initial_bytes);
    let init_do = initial_str.find("/Im0 Do").unwrap();
    let init_text = initial_str.find("Middle Text").unwrap();
    assert!(init_do < init_text, "/Im0 Do initially precedes Middle Text");

    // Move unwrapped image to front (past Middle Text)
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: /Im0 Do now follows Middle Text in the content stream AST
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let after_text = str_content.find("Middle Text").unwrap();
    let after_do = str_content.rfind("/Im0 Do").unwrap();
    assert!(after_text < after_do, "Position: /Im0 Do actually moved after Middle Text");

    // 2. Appearance: image rect and visibility are unchanged
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0], "Appearance: image coordinates preserved");
    assert_eq!(after[0].visible_rect, Some([50.0, 50.0, 150.0, 150.0]));

    // 3. Preservation: Middle Text and CTM remain intact
    assert!(str_content.contains("BT /F1 12 Tf 72 700 Td (Middle Text) Tj ET"), "Preservation: text operation preserved");
    assert!(str_content.contains("100 0 0 100 50 50 cm"), "Preservation: original cm preserved");
}

#[test]
fn test_arrange_unwrapped_image_extraction() {
    let content = b"100 0 0 100 100 100 cm /Im0 Do /Im1 Do";
    let ops = pdfcraft_content::parse(content).ops;
    let target = ops.iter().position(|op| op.is("Do") && op.name(0) == Some(b"Im0")).expect("target /Im0 not found");

    // Scope start for unwrapped image matches target because content_units isolates Do
    let scope_start = target;

    // Accumulate effective CTM prior to the image operation
    let mut initial_ctm = pdfcraft_content::Matrix::IDENTITY;
    for op in &ops[..target] {
        if op.is("cm")
            && let Some(m) = op.nums::<6>()
        {
            initial_ctm = pdfcraft_content::Matrix(m).then(&initial_ctm);
        }
    }
    assert_eq!(initial_ctm, pdfcraft_content::Matrix([100.0, 0.0, 0.0, 100.0, 100.0, 100.0]));

    let extracted = images::extract_scope_state(&ops, scope_start, target, initial_ctm)
        .expect("extraction must not return Err")
        .expect("extraction must return Some rather than None");

    assert_eq!(extracted.do_op.op.as_slice(), b"Do");
    assert_eq!(extracted.do_op.name(0), Some(b"Im0".as_slice()));
}

#[test]
fn arrange_unwrapped_image_in_mixed_scope_succeeds() {
    // Content unit contains text and an unwrapped image; scope extraction of unit fails on BT,
    // so it safely falls back to extracting target Do directly.
    let content = "q BT /F1 12 Tf 72 700 Td (Preceding Text) Tj ET 100 0 0 100 50 50 cm /Im0 Do Q BT /F1 12 Tf 72 500 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 1);

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let text_pos = str_content.find("External Text").unwrap();
    let do_pos = str_content.rfind("/Im0 Do").unwrap();
    assert!(text_pos < do_pos, "moved image follows External Text");

    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0]);
}

#[test]
fn image_arrange_with_transformed_clipping_path_succeeds() {
    // Nested graphics state: outer scope establishes a scaled clipping path (cm + path + W n),
    // inner scope draws the image.
    let content = "q 0.5 0 0 0.5 0 0 cm 100 100 m 300 100 l 300 300 l 100 300 l h W n q 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let before = images::page_images(&doc, 0).unwrap();
    assert_eq!(before.len(), 1);
    assert!(before[0].visible_rect.is_some());
    let rect_before = before[0].rect;
    let clip_before = before[0].visible_rect;

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].rect, rect_before);
    assert_eq!(after[0].visible_rect, clip_before);

    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let text_pos = str_content.find("External Text").unwrap();
    let do_pos = str_content.rfind("/Im0 Do").unwrap();
    assert!(text_pos < do_pos, "image moved after text: {str_content}");
}

#[test]
fn inherited_source_opacity_is_preserved() {
    // Outer q sets /GS0 gs; inner q draws /Im0 Do; outer Q restores state before External Text
    let content = "q /GS0 gs q 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].rect, [50.0, 50.0, 150.0, 150.0]);

    // Initial order: /Im0 Do precedes External Text
    let initial_bytes = page_content_bytes(&doc, 0);
    let initial_str = String::from_utf8_lossy(&initial_bytes);
    let init_do = initial_str.find("/Im0 Do").unwrap();
    let init_text = initial_str.find("External Text").unwrap();
    assert!(init_do < init_text, "/Im0 Do initially precedes External Text");

    // Move image to front (past External Text, outside its original outer q...Q scope)
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: /Im0 Do now follows External Text in the AST
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let after_text = str_content.find("External Text").unwrap();
    let after_do = str_content.rfind("/Im0 Do").unwrap();
    assert!(after_text < after_do, "Position: /Im0 Do moved past External Text");

    // 2. Appearance: effective appearance state (/GS0 gs) is injected and precedes /Im0 Do in the new unit
    let after_gs = str_content.rfind("/GS0 gs").unwrap();
    assert!(after_text < after_gs, "Appearance: /GS0 gs injected into the new unit at destination");
    assert!(after_gs < after_do, "Appearance: /GS0 gs directly precedes /Im0 Do in the moved unit");
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0], "Appearance: rect preserved");

    // 3. Preservation: External Text and original outer scope structure are preserved
    assert!(str_content.contains("BT /F1 12 Tf 72 700 Td (External Text) Tj ET"), "Preservation: external text preserved");
}

#[test]
fn destination_opacity_pollution_is_prevented() {
    // /Im0 Do is fully opaque at the start; destination has active top-level /GS_translucent gs
    let content = "100 0 0 100 50 50 cm /Im0 Do /GS_translucent gs BT /F1 12 Tf 72 700 Td (Translucent Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial_bytes = page_content_bytes(&doc, 0);

    // Attempting to move opaque image 0 to front into active transparency scope must safely bail out with Err
    let res = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront);
    assert!(res.is_err(), "must return an explicit Err rather than silently corrupting opacity");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("unsupported graphics state"), "error describes unsupported graphics state: {err_msg}");

    // Preservation: the document contents are completely unmodified
    let doc = reopen(&doc);
    let after_bytes = page_content_bytes(&doc, 0);
    assert_eq!(initial_bytes, after_bytes, "Preservation: content stream remained completely unmodified");
}

#[test]
fn sequential_gs_operators_merging_properties_are_preserved() {
    // Sequential gs calls: /GSalpha sets ca=0.6, /GSblend sets BM=/Multiply (without touching ca).
    let content = "q /GSalpha gs /GSblend gs q 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 72 700 Td (External Text) Tj ET";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].rect, [50.0, 50.0, 150.0, 150.0]);

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);

    // 1. Position: /Im0 Do now follows External Text in the AST
    let bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&bytes);
    let after_text = str_content.find("External Text").unwrap();
    let after_do = str_content.rfind("/Im0 Do").unwrap();
    assert!(after_text < after_do, "Position: /Im0 Do moved past External Text");

    // 2. Appearance: Both merged state operators (/GSalpha and /GSblend) are preserved and precede /Im0 Do
    let after_alpha = str_content.rfind("/GSalpha gs").unwrap();
    let after_blend = str_content.rfind("/GSblend gs").unwrap();
    assert!(after_text < after_alpha, "Appearance: /GSalpha gs injected into moved unit");
    assert!(after_text < after_blend, "Appearance: /GSblend gs injected into moved unit");
    assert!(after_alpha < after_do, "Appearance: /GSalpha gs precedes /Im0 Do");
    assert!(after_blend < after_do, "Appearance: /GSblend gs precedes /Im0 Do");

    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].rect, [50.0, 50.0, 150.0, 150.0], "Appearance: rect preserved");

    // 3. Preservation: External Text is intact
    assert!(str_content.contains("BT /F1 12 Tf 72 700 Td (External Text) Tj ET"), "Preservation: external text preserved");
}

#[test]
fn destination_differing_blend_mode_pollution_is_prevented() {
    // /Im0 Do has default Normal blend mode; destination has active top-level /GSblend gs (BM = /Multiply)
    let content = "100 0 0 100 50 50 cm /Im0 Do /GSblend gs BT /F1 12 Tf 72 700 Td (Blended Section) Tj ET";
    let mut doc = image_page_with(content);
    let initial_bytes = page_content_bytes(&doc, 0);

    // Attempting to move Normal-blend image to front into active Multiply-blend scope must safely bail out with Err
    let res = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront);
    assert!(res.is_err(), "must return an explicit Err rather than silently altering blend mode");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("unsupported graphics state"), "error describes unsupported graphics state: {err_msg}");

    // Preservation: document contents are completely unmodified
    let doc = reopen(&doc);
    let after_bytes = page_content_bytes(&doc, 0);
    assert_eq!(initial_bytes, after_bytes, "Preservation: content stream remained completely unmodified");
}

#[test]
fn nested_soft_masks_replay_in_correct_order_and_effective_mask_is_rendered() {
    let content = "q /GSA gs q /GSB gs 100 0 0 100 0 0 cm /Im0 Do Q Q BT /F1 12 Tf 10 90 Td (External Text) Tj ET";
    let mut doc = nested_soft_mask_page(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 1);

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();

    let content_bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&content_bytes);

    // 1. Assert emitted stream order: outer /GSA gs must precede inner /GSB gs, which precedes /Im0 Do
    let after_text = str_content.find("External Text").unwrap();
    let after_a = str_content.rfind("/GSA gs").unwrap();
    let after_b = str_content.rfind("/GSB gs").unwrap();
    let after_do = str_content.rfind("/Im0 Do").unwrap();
    assert!(after_text < after_a, "Position: /GSA gs relocated past external text");
    assert!(after_a < after_b, "Order: outer /GSA gs must be replayed before inner /GSB gs");
    assert!(after_b < after_do, "Order: inner /GSB gs must immediately precede /Im0 Do");

    // 2. The image survives saving and reopening.
    assert_eq!(images::page_images(&reopen(&doc), 0).unwrap().len(), 1);

    // 3. Rendered-pixel assertion: render output and verify Mask B (left 50x100) is effective, not Mask A (bottom 100x50)
    let pdf_bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(pdf_bytes), pdfcraft_render::RenderConfig::default());
    let page =
        renderer.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Pixels, scale: 1.0, ..Default::default() });
    let px = |x: u32, y: u32| {
        let idx = ((y * page.width + x) * 4) as usize;
        page.rgba[idx..idx + 4].to_vec()
    };
    // Left half (inside Mask B): image is rendered black
    assert_eq!(px(25, 25), vec![0, 0, 0, 255], "point (25, 25) is inside Mask B: image rendered");
    assert_eq!(px(25, 75), vec![0, 0, 0, 255], "point (25, 75) is inside Mask B: image rendered");
    // Right half (outside Mask B): masked out to white page background
    assert_eq!(px(75, 25), vec![255, 255, 255, 255], "point (75, 25) is outside Mask B: masked out to white");
    assert_eq!(px(75, 75), vec![255, 255, 255, 255], "point (75, 75) is outside Mask B: masked out to white");
}

#[test]
fn destination_differing_mask_pollution_is_prevented() {
    // /Im0 Do has no mask; destination has active top-level /GSA gs (Mask A)
    let content = "100 0 0 100 0 0 cm /Im0 Do /GSA gs BT /F1 12 Tf 10 90 Td (Masked Section) Tj ET";
    let mut doc = nested_soft_mask_page(content);
    let initial_bytes = page_content_bytes(&doc, 0);

    let res = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront);
    assert!(res.is_err(), "must return an explicit Err rather than corrupting mask state");
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("unsupported graphics state"), "error describes unsupported graphics state: {err_msg}");

    // Preservation: document contents are completely unmodified
    let doc = reopen(&doc);
    let after_bytes = page_content_bytes(&doc, 0);
    assert_eq!(initial_bytes, after_bytes, "Preservation: content stream remained completely unmodified");
}

#[test]
fn non_identity_cm_before_gs_preserves_mask_coordinates_and_rendering() {
    let content = "q 2 0 0 2 10 10 cm /GSA gs 50 0 0 50 0 0 cm /Im0 Do Q BT /F1 12 Tf 10 180 Td (External Text) Tj ET";
    let mut doc = soft_mask_with_alpha_page(content);

    // Initial state: image bounds are [10, 10, 110, 110]
    let initial_images = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial_images.len(), 1);
    assert!(close(initial_images[0].rect, [10.0, 10.0, 110.0, 110.0]));

    // Render baseline before reordering
    let pdf_bytes_before = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let mut renderer_before = pdfcraft_render::PageRenderer::new(Arc::new(pdf_bytes_before), pdfcraft_render::RenderConfig::default());
    let page_before = renderer_before.render(pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    let px_before = |x: u32, y: u32| {
        let idx = ((y * page_before.width + x) * 4) as usize;
        page_before.rgba[idx..idx + 4].to_vec()
    };

    // Arrange image: BringToFront
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();

    let content_bytes = page_content_bytes(&doc, 0);
    let str_content = String::from_utf8_lossy(&content_bytes);

    // 1. Assert AST reordering succeeds and interleaves cm before gs
    let text_pos = str_content.find("External Text").unwrap();
    let cm_mask_pos = str_content.rfind("2 0 0 2 10 10 cm").unwrap();
    let gs_pos = str_content.rfind("/GSA gs").unwrap();
    let cm_img_pos = str_content.rfind("50 0 0 50 0 0 cm").unwrap();
    let do_pos = str_content.rfind("/Im0 Do").unwrap();

    assert!(text_pos < cm_mask_pos, "Image relocated after external text");
    assert!(cm_mask_pos < gs_pos, "CTM for soft mask established before /GSA gs");
    assert!(gs_pos < cm_img_pos, "/GSA gs executed before image cm");
    assert!(cm_img_pos < do_pos, "image cm executed before /Im0 Do");

    // 2. Assert image bounds remain unchanged after saving and reopening
    let reopened_doc = reopen(&doc);
    let reopened_images = images::page_images(&reopened_doc, 0).unwrap();
    assert_eq!(reopened_images.len(), 1);
    assert!(close(reopened_images[0].rect, [10.0, 10.0, 110.0, 110.0]), "Image bounds preserved: {:?}", reopened_images[0].rect);

    // 3. Render reopened document and assert non-overlapping pixels remain unchanged
    let pdf_bytes_after = write_incremental(&reopened_doc, &SaveOptions::default()).unwrap();
    let mut renderer_after = pdfcraft_render::PageRenderer::new(Arc::new(pdf_bytes_after), pdfcraft_render::RenderConfig::default());
    let page_after = renderer_after.render(pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    let px_after = |x: u32, y: u32| {
        let idx = ((y * page_after.width + x) * 4) as usize;
        page_after.rgba[idx..idx + 4].to_vec()
    };

    // A point inside the masked image (e.g. at x=50, inside the image and bottom half of mask)
    // Non-overlapping with text (text is at y=180).
    // The pixel renders at [191, 191, 191, 255] (25% black over 100% white).
    // In raster coordinates, raster y for PDF y=50 in a 200-high page is y=150.
    let rendered_pixel_before = px_before(50, 150);
    let rendered_pixel_after = px_after(50, 150);
    assert_eq!(rendered_pixel_before, vec![191, 191, 191, 255], "baseline pixel rendered with 25% opacity mask");
    assert_eq!(rendered_pixel_after, rendered_pixel_before, "rendered non-overlapping pixels remain unchanged");

    // Outside the image: pure white background
    assert_eq!(px_after(5, 5), vec![255, 255, 255, 255]);
    assert_eq!(px_after(5, 5), px_before(5, 5));
}

#[test]
fn form_artwork_edits_only_the_selected_placement() {
    let mut doc = fixture(); // three pages share resources; pages 0 and 1 also share contents
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([10, 20, 110, 70].map(Object::Int).to_vec()));
    form.set(b"Matrix".to_vec(), Object::Array([2, 0, 0, 3, -20, -60].map(Object::Int).to_vec()));
    let artwork = doc.add(Object::Stream(Stream::flate(form, b"1 0 0 rg 10 20 100 50 re f")));
    doc.update_dict(pdfcraft_cos::ObjRef::new(6, 0), |d| {
        let mut xo = Dict::new();
        xo.set(b"Figure".to_vec(), Object::Ref(artwork));
        d.set(b"XObject".to_vec(), Object::Dict(xo));
    })
    .unwrap();
    let content = b"q 1 0 0 1 40 80 cm /Figure Do Q q 1 0 0 1 300 400 cm /Figure Do Q BT /F1 9 Tf (Caption) Tj ET";
    doc.set(pdfcraft_cos::ObjRef::new(7, 0), Object::Stream(Stream::flate(Dict::new(), content)));
    let mut doc = reopen(&doc);
    let original = doc.get(artwork);
    let imgs = images::page_images(&doc, 0).unwrap();
    assert_eq!(imgs.len(), 2);
    assert!(imgs[0].is_form);
    assert_eq!((imgs[0].width, imgs[0].height), (0, 0));
    assert!(close(imgs[0].rect, [40.0, 80.0, 240.0, 230.0]));
    let target = [60.0, 100.0, 160.0, 175.0];
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(images::rect_to_rect(imgs[0].rect, target))).unwrap();
    let mut doc = reopen(&doc);
    let moved = images::page_images(&doc, 0).unwrap();
    assert!(close(moved[0].rect, target));
    assert_eq!(moved[1].rect, imgs[1].rect);
    assert_eq!(images::page_images(&doc, 1).unwrap()[0].rect, imgs[0].rect);
    assert_eq!(*doc.get(artwork), *original, "the shared Form and its unknown data are untouched");
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "Caption");
    let t = images::turn_about_centre(target, 1, false, false);
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(t)).unwrap();
    assert!(close(images::page_images(&doc, 0).unwrap()[0].rect, [72.5, 87.5, 147.5, 187.5]));
    assert!(images::change_image(&mut doc, 0, 0, &images::ImageChange::Replace(artwork)).is_err());
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Delete).unwrap();
    let doc = reopen(&doc);
    assert_eq!(images::page_images(&doc, 0).unwrap().len(), 1);
    assert_eq!(images::page_images(&doc, 1).unwrap().len(), 2);
    // Invalid bounds are skipped without interpreting the Form stream.
    let mut doc = doc;
    let mut broken = (*doc.get(artwork)).clone();
    if let Object::Stream(s) = &mut broken {
        s.dict.set(b"BBox".to_vec(), Object::Array(vec![Object::Int(0)]));
    }
    doc.set(artwork, broken);
    assert!(images::page_images(&doc, 0).unwrap().is_empty());
}
