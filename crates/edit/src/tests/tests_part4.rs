//! Unit tests of the editing operations (part 4 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn paragraphs_take_new_formatting() {
    let mut doc = text_page("BT /F1 10 Tf 12 TL 100 700 Td (One two three four five six seven) Tj T* (eight nine ten eleven twelve) Tj ET");
    let before = text::text_blocks(&doc, 0).unwrap()[0].clone();
    let style = text::BlockStyle {
        family: Some((added::Family::Times, true, false)),
        size: Some(12.0),
        color: Some([1.0, 0.0, 0.0]),
        align: Some(added::Align::Center),
        ..Default::default()
    };
    text::rewrite_block(&mut doc, 0, 0, None, &style).unwrap();
    let doc = reopen(&doc);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(blocks[0].text, before.text, "same words");
    assert_eq!((blocks[0].base_font.as_str(), blocks[0].size), ("Times-Bold", 12.0));
    // Centred in the paragraph's width.
    let mid = (before.rect[0] + before.rect[2]) / 2.0;
    for i in &blocks[0].lines {
        let r = lines[*i].rect;
        assert!(((r[0] + r[2]) / 2.0 - mid).abs() < 2.0, "line {:?} centred on {mid}", lines[*i].text);
    }
    assert!(String::from_utf8_lossy(&page_content_bytes(&doc, 0)).contains("1 0 0 rg"));
    // Right alignment keeps lines flush with the right edge.
    let mut doc = text_page("BT /F1 10 Tf 12 TL 100 700 Td (One two three four five six seven) Tj T* (eight nine ten eleven twelve) Tj ET");
    let right = text::text_blocks(&doc, 0).unwrap()[0].rect[2];
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { align: Some(added::Align::Right), ..Default::default() }).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    for l in &lines {
        assert!((l.rect[2] - right).abs() < 1.0, "{:?} ends at {} not {right}", l.text, l.rect[2]);
    }
}

#[test]
fn a_one_row_title_keeps_its_centre_when_it_grows_and_when_it_wraps() {
    // A centred one-row title (its centre is at x ≈ 306 on the 600 pt page) above a body
    // left-aligned at x = 72. Growing it must not drag it to the right.
    let src = "BT /F1 18 Tf 1 0 0 1 252.477 700 Tm (Centered title) Tj ET\n\
               BT /F1 12 Tf 1 0 0 1 72 640 Tm (Body text below the heading starts here and runs) Tj ET\n\
               BT /F1 12 Tf 1 0 0 1 72 625.6 Tm (on for a second row of ordinary words.) Tj ET";
    let mut doc = text_page(src);
    let before = text::text_blocks(&doc, 0).unwrap()[0].clone();
    let centre = (before.rect[0] + before.rect[2]) / 2.0;
    text::rewrite_block(&mut doc, 0, 0, Some("Centered title with a few more words"), &text::BlockStyle::default()).unwrap();
    let doc = reopen(&doc);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(blocks[0].text, "Centered title with a few more words");
    let r = lines[blocks[0].lines[0]].rect;
    assert!(((r[0] + r[2]) / 2.0 - centre).abs() < 1.0, "the grown title stays centred on {centre}, its centre is {}", (r[0] + r[2]) / 2.0);
    // Long enough to have to wrap: every row stays centred on the same centre too.
    let mut doc = text_page(src);
    text::rewrite_block(
        &mut doc,
        0,
        0,
        Some("Centered title with many more words added so that the single row has to become much longer"),
        &text::BlockStyle::default(),
    )
    .unwrap();
    let doc = reopen(&doc);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!(blocks[0].lines.len() > 1, "the long title wraps into rows: {:?}", blocks[0].text);
    for i in &blocks[0].lines {
        let r = lines[*i].rect;
        assert!(((r[0] + r[2]) / 2.0 - centre).abs() < 2.0, "row {:?} stays centred on {centre}", lines[*i].text);
    }
}

#[test]
fn a_new_paragraph_colour_does_not_spill_into_the_text_after_it() {
    let mut doc = text_page("BT /F1 10 Tf 72 700 Td (First paragraph) Tj 0 -40 Td (Second paragraph) Tj ET");
    let style = text::BlockStyle { color: Some([1.0, 0.0, 0.0]), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, None, &style).unwrap();
    let doc = reopen(&doc);
    // The fill colour in force where the second paragraph is shown (q/Q nest it).
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let (mut fill, mut stack) = (String::from("0 g"), Vec::new());
    for op in &ops {
        match op.op.as_slice() {
            b"q" => stack.push(fill.clone()),
            b"Q" => fill = stack.pop().unwrap_or_default(),
            b"g" | b"rg" | b"k" => fill = String::from_utf8_lossy(&pdfcraft_content::serialize_ops(std::slice::from_ref(op))).trim().to_string(),
            b"Tj" if op.operands.first().and_then(|o| o.as_string()).is_some_and(|s| s.to_text() == "Second paragraph") => break,
            _ => {}
        }
    }
    assert_eq!(fill, "0 g", "{ops:?}");
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[1].text, "Second paragraph");
}

#[test]
fn a_rewritten_paragraph_keeps_its_place_in_a_shared_text_object() {
    // Three paragraphs in one BT … ET: rewriting the second keeps the order (paragraph numbers
    // and reading order) and every paragraph's position.
    let src = "BT /F1 10 Tf 72 700 Td (First paragraph) Tj 0 -40 Td (Second paragraph) Tj 0 -40 Td (Third paragraph) Tj ET";
    let mut doc = text_page(src);
    let before = text::text_blocks(&doc, 0).unwrap();
    text::replace_block(&mut doc, 0, 1, "Edited second").unwrap();
    let doc = reopen(&doc);
    let after = text::text_blocks(&doc, 0).unwrap();
    let texts: Vec<&str> = after.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["First paragraph", "Edited second", "Third paragraph"]);
    assert_eq!(after[0].rect, before[0].rect);
    assert_eq!(after[2].rect, before[2].rect);
    assert!((after[1].rect[0] - before[1].rect[0]).abs() < 0.01 && (after[1].rect[1] - before[1].rect[1]).abs() < 0.01, "{:?}", after[1].rect);
}

#[test]
fn recolouring_a_paragraph_in_a_shared_text_object_keeps_order_and_nesting() {
    // Splitting the text object (to keep the order) and q … Q (to keep the colour in) together:
    // q/Q stay outside text objects, and the paragraph after it keeps the original colour.
    let src = "BT /F1 10 Tf 72 700 Td (First paragraph) Tj 0 -40 Td (Second paragraph) Tj 0 -40 Td (Third paragraph) Tj ET";
    let mut doc = text_page(src);
    let style = text::BlockStyle { color: Some([1.0, 0.0, 0.0]), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 1, None, &style).unwrap();
    let doc = reopen(&doc);
    let texts: Vec<String> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["First paragraph", "Second paragraph", "Third paragraph"]);
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    let (mut in_text, mut depth, mut red) = (false, 0usize, Vec::new());
    for op in &ops {
        match op.op.as_slice() {
            b"BT" => in_text = true,
            b"ET" => in_text = false,
            b"q" | b"Q" => {
                assert!(!in_text, "q/Q inside a text object: {ops:?}");
                depth = if op.is("q") { depth + 1 } else { depth.saturating_sub(1) };
            }
            b"rg" => red.push(depth),
            b"Tj" if op.operands.first().and_then(|o| o.as_string()).is_some_and(|s| s.to_text() == "Third paragraph") => {
                assert!(red.iter().all(|d| *d > depth), "red still in force for the third paragraph: {ops:?}");
            }
            _ => {}
        }
    }
    assert!(!red.is_empty(), "{ops:?}");
}

#[test]
fn justify_underline_and_spacing() {
    let src = "BT /F1 10 Tf 12 TL 100 700 Td (One two three four five six seven) Tj T* (eight nine ten eleven twelve) Tj T* (end) Tj ET";
    // Justified: every line but the last reaches the right edge.
    let mut doc = text_page(src);
    let right = text::text_blocks(&doc, 0).unwrap()[0].rect[2];
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { align: Some(added::Align::Justify), ..Default::default() }).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    for l in &lines[..lines.len() - 1] {
        assert!((l.rect[2] - right).abs() < 1.0, "{:?} ends at {} not {right}", l.text, l.rect[2]);
    }
    // Double line spacing, underlined.
    let mut doc = text_page(src);
    let style = text::BlockStyle { line_spacing: Some(2.0), underline: Some(true), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, None, &style).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!((lines[0].origin_baseline() - lines[1].origin_baseline() - 20.0).abs() < 0.01);
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains(" l\nS\n") || content.contains(" l S"), "{content}");
    // Character spacing widens, horizontal scale narrows.
    let mut doc = text_page("BT /F1 10 Tf 100 700 Td (Wide text) Tj ET");
    let w0 = text::text_lines(&doc, 0).unwrap()[0].rect;
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { char_spacing: Some(2.0), ..Default::default() }).unwrap();
    let w1 = text::text_lines(&doc, 0).unwrap()[0].rect;
    assert!((w1[2] - w1[0]) - (w0[2] - w0[0]) > 15.0, "{w0:?} → {w1:?}");
    let mut doc = text_page("BT /F1 10 Tf 100 700 Td (Wide text) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { scale: Some(50.0), ..Default::default() }).unwrap();
    let w2 = text::text_lines(&doc, 0).unwrap()[0].rect;
    assert!(((w2[2] - w2[0]) - (w0[2] - w0[0]) / 2.0).abs() < 1.0, "{w0:?} → {w2:?}");
}

#[test]
fn a_line_drawn_twice_is_replaced_everywhere() {
    // Fake bold: a line drawn twice at the same spot (each copy its own text object). The two
    // copies group apart, but editing one replaces both — a surviving copy would show the old
    // text under the new.
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello) Tj ET BT /F1 12 Tf 72 700 Td (Hello) Tj ET");
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2, "the copies group apart");
    text::replace_block(&mut doc, 0, 0, "Hello world").unwrap();
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Hello world"], "one line, replaced");
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert_eq!(content.matches("(Hello").count(), 1, "the second copy's operators are gone: {content}");
    // Copies in one text object (the matrix re-issued at the same spot) go too.
    let mut doc = text_page("BT /F1 12 Tf 1 0 0 1 72 700 Tm (Hi) Tj 1 0 0 1 72.3 700 Tm (Hi) Tj ET");
    text::replace_line(&mut doc, 0, 0, "Hey").unwrap();
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Hey"], "one line, replaced");
    // A neighbouring line is not coincident and stays (40 pt spacing groups apart at 12 pt).
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello) Tj 0 -40 Td (world) Tj ET");
    text::replace_block(&mut doc, 0, 0, "Hello there").unwrap();
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Hello there", "world"]);
}

#[test]
fn replacing_a_line_keeps_a_distinct_nearby_line() {
    // 12 pt lines 6 pt apart: the glyph boxes overlap by more than half the smaller box,
    // but the baselines are different lines (#1316).
    let src = "BT /F1 12 Tf 1 0 0 1 72 700 Tm (Alpha) Tj 0 -6 Td (Beta) Tj ET";
    let listed = |doc: &Document| text::text_lines(doc, 0).unwrap().into_iter().map(|l| l.text).collect::<Vec<_>>();
    for paragraph in [false, true] {
        let mut doc = text_page(src);
        assert_eq!(listed(&doc), ["Alpha", "Beta"]);
        assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
        if paragraph {
            text::rewrite_block(&mut doc, 0, 0, Some("ALPHA"), &text::BlockStyle::default()).unwrap();
        } else {
            text::replace_line(&mut doc, 0, 0, "ALPHA").unwrap();
        }
        assert_eq!(listed(&doc), ["ALPHA", "Beta"]);
        assert!(String::from_utf8_lossy(&page_content_bytes(&doc, 0)).contains("(Beta)"));
        for bytes in [write_incremental(&doc, &SaveOptions::default()).unwrap(), write_full(&doc, &SaveOptions::default()).unwrap()] {
            let back = Document::open(Arc::new(bytes)).unwrap();
            assert!(back.repair_log().is_empty());
            assert_eq!(listed(&back), ["ALPHA", "Beta"]);
        }
    }
    // The same word on the next line is still a different line.
    let mut same = text_page("BT /F1 12 Tf 1 0 0 1 72 700 Tm (Alpha) Tj 0 -6 Td (Alpha) Tj ET");
    text::replace_line(&mut same, 0, 0, "ALPHA").unwrap();
    assert_eq!(listed(&same), ["ALPHA", "Alpha"]);
    // Ordinary leading, and a paragraph far enough to group apart, keep the neighbour.
    let mut spaced = text_page("BT /F1 12 Tf 1 0 0 1 72 700 Tm (Alpha) Tj 0 -14.4 Td (Beta) Tj ET");
    text::replace_line(&mut spaced, 0, 0, "ALPHA").unwrap();
    assert_eq!(listed(&spaced), ["ALPHA", "Beta"]);
    let mut apart = text_page("BT /F1 12 Tf 1 0 0 1 72 700 Tm (Alpha) Tj 0 -40 Td (Beta) Tj ET");
    text::rewrite_block(&mut apart, 0, 0, Some("ALPHA"), &text::BlockStyle::default()).unwrap();
    assert_eq!(listed(&apart), ["ALPHA", "Beta"]);
    // A second copy at the same position is still one replacement.
    for paragraph in [false, true] {
        let mut duplicate = text_page("BT /F1 12 Tf 1 0 0 1 72 700 Tm (Alpha) Tj ET BT /F1 12 Tf 1 0 0 1 72 700 Tm (Alpha) Tj ET");
        if paragraph {
            text::rewrite_block(&mut duplicate, 0, 0, Some("ALPHA"), &text::BlockStyle::default()).unwrap();
        } else {
            text::replace_line(&mut duplicate, 0, 0, "ALPHA").unwrap();
        }
        assert_eq!(listed(&duplicate), ["ALPHA"]);
    }
}

#[test]
fn editing_text_keeps_an_overlapping_watermark_and_covered_paragraphs() {
    let listed = |doc: &Document| text::text_lines(doc, 0).unwrap().into_iter().map(|l| l.text).collect::<Vec<_>>();
    // #1756: a rotated watermark's bounding box covers the middle of the page, and a body line
    // whose baseline happens to sit near the watermark's starting y is still unrelated text.
    for rotation in [35.0, 45.0, 0.0] {
        let mut base = text_page("BT /F1 14 Tf 60 700 Td (Heading) Tj ET");
        add_watermark(
            &mut base,
            &[0],
            &Watermark { text: "TEST WATERMARK".into(), font_size: 48.0, rotation, opacity: 0.25, ..Watermark::default() },
            false,
        )
        .unwrap();
        let wm_y = text::text_lines(&base, 0).unwrap().into_iter().find(|l| l.text == "TEST WATERMARK").unwrap().origin_baseline();
        let mut doc = text_page(&format!("BT /F1 14 Tf 1 0 0 1 60 {wm_y:.2} Tm (Body paragraph one with several words here) Tj ET"));
        add_watermark(
            &mut doc,
            &[0],
            &Watermark { text: "TEST WATERMARK".into(), font_size: 48.0, rotation, opacity: 0.25, ..Watermark::default() },
            false,
        )
        .unwrap();
        let line_idx = text::text_lines(&doc, 0).unwrap().iter().position(|l| l.text.starts_with("Body")).unwrap();
        let mut line_edited = doc.clone();
        text::replace_line(&mut line_edited, 0, line_idx, "Edited body line").unwrap();
        assert_eq!(listed(&reopen(&line_edited)), ["Edited body line", "TEST WATERMARK"], "rotation {rotation}");

        let block_idx = text::text_blocks(&doc, 0).unwrap().iter().position(|b| b.text.starts_with("Body")).unwrap();
        let mut preview = doc.clone();
        text::clear_block(&mut preview, 0, block_idx).unwrap();
        assert_eq!(listed(&preview), ["TEST WATERMARK"], "clear_block keeps the watermark at rotation {rotation}");

        text::rewrite_block(&mut doc, 0, block_idx, Some("Edited body paragraph"), &text::BlockStyle::default()).unwrap();
        assert_eq!(listed(&reopen(&doc)), ["Edited body paragraph", "TEST WATERMARK"], "rotation {rotation}");
    }

    // #1368: dragging one paragraph onto another paragraph's baseline and moving it on again
    // leaves the covered paragraph intact.
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (First paragraph) Tj ET BT /F1 12 Tf 72 600 Td (Second paragraph) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { offset: Some([0.0, -100.0]), ..Default::default() }).unwrap();
    let second = text::text_blocks(&doc, 0).unwrap().iter().position(|b| b.text == "Second paragraph");
    assert!(second.is_some(), "the covered paragraph stays on the page: {:?}", listed(&doc));
    let first = text::text_blocks(&doc, 0).unwrap().iter().position(|b| b.text == "First paragraph").unwrap();
    text::rewrite_block(&mut doc, 0, first, None, &text::BlockStyle { offset: Some([0.0, -100.0]), ..Default::default() }).unwrap();
    assert_eq!(listed(&reopen(&doc)), ["First paragraph", "Second paragraph"]);

    // Even when a large horizontal watermark shows the same word as a body line at the same y,
    // or a rotated 12 pt line crosses a horizontal 12 pt line of the same word at the same origin,
    // editing one keeps the other.
    let mut same_word = text_page("BT /F1 12 Tf 1 0 0 1 200 400 Tm (DRAFT) Tj ET BT /F1 48 Tf 1 0 0 1 150 400 Tm (DRAFT) Tj ET");
    text::replace_line(&mut same_word, 0, 0, "FINAL").unwrap();
    let after = text::text_lines(&reopen(&same_word), 0).unwrap();
    assert_eq!(after.iter().map(|l| (&l.text[..], l.size as i32)).collect::<Vec<_>>(), [("FINAL", 12), ("DRAFT", 48)]);

    let mut crossed = text_page("BT /F1 12 Tf 1 0 0 1 100 500 Tm (Crossed) Tj ET BT /F1 12 Tf 0.707 0.707 -0.707 0.707 100 500 Tm (Crossed) Tj ET");
    text::replace_line(&mut crossed, 0, 0, "Horizontal").unwrap();
    assert_eq!(listed(&reopen(&crossed)), ["Horizontal", "Crossed"]);

    // An invisible OCR text layer (`3 Tr`) on the same baseline and size as a visible line is
    // still removed when the visible line is edited, even if OCR misread a letter.
    let mut ocr = text_page("BT /F1 12 Tf 0 Tr 1 0 0 1 72 700 Tm (Hello world) Tj ET BT /F1 12 Tf 3 Tr 1 0 0 1 72 700 Tm (Rello world) Tj ET");
    text::replace_line(&mut ocr, 0, 0, "Goodbye world").unwrap();
    assert_eq!(listed(&reopen(&ocr)), ["Goodbye world"]);
}

#[test]
fn paragraphs_move_and_rewrap_to_a_new_width() {
    let para = "BT /F1 10 Tf 12 TL 100 700 Td (One two three four five six seven) Tj T* (eight nine ten eleven twelve) Tj ET";
    let close = |a: f64, b: f64| (a - b).abs() < 0.5;
    // Moved 50 right and 200 down, same words and wrapping.
    let mut doc = text_page(para);
    let before = text::text_blocks(&doc, 0).unwrap()[0].clone();
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { offset: Some([50.0, -200.0]), ..Default::default() }).unwrap();
    let doc = reopen(&doc);
    let after = text::text_blocks(&doc, 0).unwrap()[0].clone();
    assert_eq!((after.text.as_str(), after.lines.len()), (before.text.as_str(), before.lines.len()));
    assert!(close(after.rect[0], before.rect[0] + 50.0) && close(after.rect[1], before.rect[1] - 200.0), "{:?} → {:?}", before.rect, after.rect);
    // Drawn at half scale: the move is still in page space.
    let mut doc = text_page(&format!("q 0.5 0 0 0.5 0 0 cm {} Q", para.replace("100 700 Td", "200 1400 Td")));
    let before = text::text_blocks(&doc, 0).unwrap()[0].clone();
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { offset: Some([30.0, 40.0]), ..Default::default() }).unwrap();
    let after = text::text_blocks(&reopen(&doc), 0).unwrap()[0].clone();
    assert!(close(after.rect[0], before.rect[0] + 30.0) && close(after.rect[1], before.rect[1] + 40.0), "{:?} → {:?}", before.rect, after.rect);
    // A narrower box rewraps into more lines that fit it; a wider one into fewer.
    let mut doc = text_page(para);
    let left = text::text_blocks(&doc, 0).unwrap()[0].rect[0];
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { width: Some(80.0), ..Default::default() }).unwrap();
    let narrow = text::text_blocks(&reopen(&doc), 0).unwrap()[0].clone();
    assert!(narrow.lines.len() > 2, "{narrow:?}");
    assert!(narrow.rect[2] <= left + 80.5, "{narrow:?}");
    let mut doc = text_page(para);
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { width: Some(400.0), ..Default::default() }).unwrap();
    assert_eq!(text::text_blocks(&reopen(&doc), 0).unwrap()[0].lines.len(), 1);
    // Untrusted numbers (the automation tools pass them through) are refused, not drawn.
    let mut doc = text_page(para);
    for style in [
        text::BlockStyle { offset: Some([f64::NAN, 0.0]), ..Default::default() },
        text::BlockStyle { offset: Some([0.0, f64::INFINITY]), ..Default::default() },
        text::BlockStyle { width: Some(f64::NAN), ..Default::default() },
    ] {
        assert!(text::rewrite_block(&mut doc, 0, 0, None, &style).is_err(), "{style:?}");
    }
    // A zero width is clamped to one character's width rather than looping or vanishing.
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { width: Some(0.0), ..Default::default() }).unwrap();
    assert_eq!(text::text_blocks(&reopen(&doc), 0).unwrap().iter().map(|b| b.text.split_whitespace().count()).sum::<usize>(), 12);
}

#[test]
fn the_graphics_state_carries_over_between_content_streams() {
    let close = |a: f64, b: f64| (a - b).abs() < 0.5;
    // Text at (1000, 2000) in a space scaled by 0.12: on the page at (120, 240), 12 pt.
    let mut doc = split_page("0.12 0 0 0.12 0 0 cm", "BT /F1 100 Tf 1000 2000 Td (Site plan) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(close(l.rect[0], 120.0) && l.rect[1] > 230.0 && l.rect[3] < 260.0, "{:?}", l.rect);
    assert!(close(l.size, 12.0), "{}", l.size);
    // Editing keeps the paragraph where it was, at its size; moving it moves it in page space.
    text::rewrite_block(&mut doc, 0, 0, Some("Site plan, rev. B"), &text::BlockStyle { offset: Some([10.0, -20.0]), ..Default::default() }).unwrap();
    let after = text::text_blocks(&reopen(&doc), 0).unwrap();
    assert_eq!(after[0].text, "Site plan, rev. B");
    assert!(close(after[0].rect[0], 130.0) && close(after[0].rect[1], l.rect[1] - 20.0), "{:?}", after[0].rect);
    assert!(close(after[0].size, 12.0), "{}", after[0].size);
    // A `q` left open in one stream and closed in the next still restores the state.
    let doc = split_page("q 0.5 0 0 0.5 0 0 cm", "Q BT /F1 10 Tf 100 100 Td (Footpath) Tj ET");
    let l = &text::text_lines(&doc, 0).unwrap()[0];
    assert!(close(l.rect[0], 100.0) && close(l.size, 10.0), "{l:?}");
}

#[test]
fn arrange_and_crop_image_in_second_content_stream_succeeds() {
    let first = "q 100 0 0 100 50 50 cm /Im0 Do Q";
    let second = "q 100 0 0 100 200 200 cm /Im1 Do Q";
    let mut doc = two_stream_image_page(first, second);
    let before = images::page_images(&doc, 0).unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].name, "Im0");
    assert_eq!(before[1].name, "Im1");

    // Arrange second image (located in the second content stream)
    arrange(&mut doc, 0, 1, images::ArrangeOrder::SendToBack).unwrap();
    let after_arrange = images::page_images(&reopen(&doc), 0).unwrap();
    assert_eq!(after_arrange.len(), 2);

    // Crop image in second stream
    crop(&mut doc, 0, 0, [210.0, 210.0, 290.0, 290.0]).unwrap();
    let after_crop = images::page_images(&reopen(&doc), 0).unwrap();
    assert_eq!(after_crop.len(), 2);
    assert_eq!(after_crop[0].name, "Im1");
    assert!(close(after_crop[0].visible_rect.unwrap(), [210.0, 210.0, 290.0, 290.0]));
    assert_eq!(after_crop[1].name, "Im0");
    assert_eq!(after_crop[1].visible_rect, Some(after_crop[1].rect));
}

/// The report: Edit text showed "בדיוק" (exactly) reversed, because the line was read in the
/// order the page draws it.
#[test]
fn hebrew_lines_read_in_typing_order() {
    let doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET BT /F1 12 Tf 300 600 Td (EDCBA HDGF) Tj ET");
    let lines: Vec<String> = text::text_lines(&doc, 0).unwrap().into_iter().map(|l| l.text).collect();
    assert_eq!(lines, ["בדיוק", "שלום בדיוק"]);
    let blocks: Vec<String> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(blocks, ["בדיוק", "שלום בדיוק"]);
}

/// Text the page's own font can show is written back in drawing order, right-aligned to where
/// the line ended, and reads back as typed.
#[test]
fn hebrew_edits_are_drawn_right_to_left_and_keep_their_right_edge() {
    let mut doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET");
    let before = text::text_lines(&doc, 0).unwrap()[0].rect;
    // One line: the bytes are the visual order of the typed text.
    assert_eq!(text::replace_line(&mut doc, 0, 0, "שלום בדיוק").unwrap().substituted, None);
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains("(EDCBA HDGF)"), "{content}");
    assert_eq!(text::text_lines(&reopen(&doc), 0).unwrap()[0].text, "שלום בדיוק");

    // A paragraph: shorter text stays right-aligned at the old right edge.
    let mut doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET");
    assert_eq!(text::replace_block(&mut doc, 0, 0, "דוב").unwrap().substituted, None);
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains("(ADB)"), "{content}");
    let doc = reopen(&doc);
    let line = text::text_lines(&doc, 0).unwrap().swap_remove(0);
    assert_eq!(line.text, "דוב");
    assert!((line.rect[2] - before[2]).abs() < 0.5, "right edge {} → {}", before[2], line.rect[2]);
    assert!(line.rect[0] > before[0] + 1.0, "shorter text starts further right: {:?} vs {before:?}", line.rect);

    // Longer text grows to the left, not off the right edge.
    let mut doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET");
    text::replace_block(&mut doc, 0, 0, "שלום בדיוק שלום").unwrap();
    let doc = reopen(&doc);
    let line = text::text_lines(&doc, 0).unwrap().swap_remove(0);
    assert_eq!(line.text, "שלום בדיוק שלום");
    assert!((line.rect[2] - before[2]).abs() < 0.5, "right edge {} → {}", before[2], line.rect[2]);
    assert!(line.rect[0] < before[0] - 1.0, "longer text grows left: {:?} vs {before:?}", line.rect);

    // An alignment asked for still wins.
    let mut doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET");
    let left = text::BlockStyle { align: Some(Align::Left), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some("דוב"), &left).unwrap();
    let line = text::text_lines(&reopen(&doc), 0).unwrap().swap_remove(0);
    assert!((line.rect[0] - before[0]).abs() < 0.5, "left-aligned: {:?} vs {before:?}", line.rect);
}

/// A generated Type 3 font numbers its glyphs from 1, so code 32 is a character, not the space:
/// justified rows are stretched after each space, never with word spacing.
#[test]
fn justified_text_in_the_type3_fallback_stretches_its_spaces() {
    let src = "BT /F1 12 Tf 14 TL 100 700 Td (Some words to fill a row of text) Tj T* (and another one below it) Tj T* (end) Tj ET";
    let mut doc = text_page(src);
    let rect = text::text_blocks(&doc, 0).unwrap()[0].rect;
    // More than 32 different characters, so one of them gets code 32.
    let typed = "日本語の 文章を 書きます 東京 大阪 京都 名古屋 札幌 福岡 神戸 横浜 仙台 広島 千葉 新潟 静岡 熊本 岡山 金沢";
    let style = text::BlockStyle { align: Some(added::Align::Justify), ..Default::default() };
    let rewritten = text::rewrite_block(&mut doc, 0, 0, Some(typed), &style);
    if pdfcraft_fonts::document_japanese_font().is_none() {
        assert!(rewritten.is_err());
        eprintln!("built without a craft-fonts Japanese face: the Type 3 path is checked with CRAFT_FONTS_DIR");
        return;
    }
    rewritten.unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!(lines.len() > 2, "{lines:?}");
    for l in &lines[..lines.len() - 1] {
        assert!((l.rect[2] - rect[2]).abs() < 1.0, "{:?} spans {:?}, not {rect:?}", l.text, l.rect);
    }
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    assert!(ops.iter().any(|o| o.is("TJ")), "{ops:?}");
    assert!(ops.iter().filter(|o| o.is("Tw")).all(|o| o.operands.first().and_then(|n| n.as_f64()) == Some(0.0)), "{ops:?}");
}

/// Letters the page's font lacks are drawn with the Hebrew face from craft-fonts; a build
/// without it says so and changes nothing. Hebrew points are refused, not misdrawn.
#[test]
fn hebrew_letters_the_page_font_lacks_use_the_hebrew_face() {
    let mut doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET");
    let before = page_content_bytes(&doc, 0);
    let pointed = text::replace_line(&mut doc, 0, 0, "שָׁלוֹם עוֹלָם");
    let Err(EditError::Invalid(msg)) = pointed else { panic!("expected a clear error, got {pointed:?}") };
    assert!(msg.contains("niqqud"), "{msg}");
    assert_eq!(page_content_bytes(&doc, 0), before);

    // ע and ה aren't in the page's font.
    let typed = "שלום עולם, עמוד 12 (PDF)";
    let line = text::replace_line(&mut doc, 0, 0, typed);
    if pdfcraft_fonts::document_hebrew_font(false).is_none() {
        let Err(EditError::Invalid(msg)) = line else { panic!("expected a clear error, got {line:?}") };
        assert!(msg.contains("Hebrew fallback font") && msg.contains("CRAFT_FONTS_DIR"), "{msg}");
        assert_eq!(page_content_bytes(&doc, 0), before);
        eprintln!("built without a craft-fonts Hebrew face: the Type 3 path is checked with CRAFT_FONTS_DIR");
        return;
    }
    assert_eq!(line.unwrap().substituted.as_deref(), Some("Noto Sans Hebrew Type3"));
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains("/PCHe"), "{content}");
    let reopened = reopen(&doc);
    assert_eq!(text::text_lines(&reopened, 0).unwrap()[0].text, typed);
    // Every glyph has an outline, so nothing is drawn as a box.
    let p = pdfcraft_model::pages(&reopened).swap_remove(0);
    let res = reopened.resolve(p.dict.get(b"Resources").unwrap());
    let fonts = reopened.resolve(res.as_dict().unwrap().get(b"Font").unwrap());
    let font = reopened.resolve(fonts.as_dict().unwrap().get(b"PCHe").unwrap());
    let procs = reopened.resolve(font.as_dict().unwrap().get(b"CharProcs").unwrap());
    let procs = procs.as_dict().unwrap();
    assert_eq!(procs.iter().count(), typed.chars().collect::<std::collections::HashSet<_>>().len());
    for (_, glyph) in procs.iter() {
        let pdfcraft_cos::Object::Stream(s) = &*reopened.resolve(glyph) else { panic!("glyph stream") };
        let path = String::from_utf8(s.decoded().unwrap()).unwrap();
        assert!(path.starts_with(|c: char| c.is_ascii_digit()) && path.contains(" d1"), "{path}");
    }

    // A whole paragraph too, right-aligned.
    let mut doc = hebrew_page("BT /F1 12 Tf 300 700 Td (EDCBA) Tj ET");
    assert_eq!(text::replace_block(&mut doc, 0, 0, "עברית מלאה").unwrap().substituted.as_deref(), Some("Noto Sans Hebrew Type3"));
    assert_eq!(text::text_blocks(&reopen(&doc), 0).unwrap()[0].text, "עברית מלאה");
}

/// #2013: the dialog asks `first_undrawable` before offering OK, so it must refuse exactly what
/// applying refuses: a Cyrillic footer was previewed (in the interface font) and then failed.
#[test]
fn header_and_watermark_drawability_matches_what_applying_refuses() {
    let footer = |t: &str| HeaderFooter {
        text: [String::new(), String::new(), String::new(), String::new(), String::new(), t.into()],
        ..HeaderFooter::default()
    };
    for (text, refused) in [
        ("Страница <<1>> из 1 / Page <<1>> of 1", Some('С')),
        ("TEST-001 <<Page 1 of n>>", None),
        ("Café — 5€ ™ <<Bates Number#6#1#ACME-#>>", None),
        ("Left\r\nRight", None),
        ("A\rB", Some('\r')),
        ("line one\nΕλληνικά", Some('Ε')),
    ] {
        let hf = footer(text);
        assert_eq!(hf.first_undrawable(), refused, "{text:?}");
        let mut doc = fixture();
        let applied = add_header_footer(&mut doc, &[0], &hf, false, &cx());
        match refused {
            Some(c) => assert_eq!(applied.unwrap_err().to_string(), undrawable_message(c), "{text:?}"),
            None => applied.unwrap(),
        }
        let wm = Watermark { text: text.into(), ..Watermark::default() };
        assert_eq!(wm.first_undrawable(), refused, "{text:?}");
        let applied = add_watermark(&mut fixture(), &[0], &wm, false);
        assert_eq!(applied.err().map(|e| e.to_string()), refused.map(undrawable_message), "{text:?}");
    }
}

/// #125: page-content tools refuse text the standard fonts can't draw instead of writing `?`,
/// and leave the document untouched.
#[test]
fn page_text_the_standard_fonts_cant_draw_is_refused() {
    let mut doc = fixture();
    let before: Vec<Vec<String>> = (0..3).map(|p| streams(&reopen(&doc), p)).collect();
    let text = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "日本語のテキスト".into(), ..AddedText::default() };
    let err = add_content(&mut doc, 0, &Content::Text(text)).unwrap_err().to_string();
    assert!(err.contains("\"日\" (U+65E5)"), "{err}");
    let hf = HeaderFooter {
        text: ["Ελληνικά".into(), String::new(), String::new(), String::new(), String::new(), String::new()],
        ..HeaderFooter::default()
    };
    assert!(add_header_footer(&mut doc, &[0, 1], &hf, true, &cx()).unwrap_err().to_string().contains("U+0395"));
    let wm = Watermark { text: "机密".into(), ..Watermark::default() };
    assert!(add_watermark(&mut doc, &[0], &wm, true).unwrap_err().to_string().contains("U+673A"));
    let after: Vec<Vec<String>> = (0..3).map(|p| streams(&reopen(&doc), p)).collect();
    assert_eq!(before, after, "a refused edit changes nothing");
    // Western European text, including what only WinAnsi (not Latin-1) has, still works.
    let text = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "Café — 5€ ™".into(), ..AddedText::default() };
    add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap();
    // Retyping existing text is refused the same way, and the whole document is left as it was.
    let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let retyped = AddedText { text: "Café 中".into(), ..text.clone() };
    assert!(update_content(&mut doc, 0, 0, &Content::Text(retyped)).unwrap_err().to_string().contains("U+4E2D"));
    assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), saved, "a refused update changes nothing");
    // A carriage return is checked where it would be drawn: added text splits on LF only, so
    // CRLF would leave a `?`; headers and watermarks split CRLF into lines, but not a lone CR.
    let crlf = AddedText { text: "A\r\nB".into(), ..text };
    assert!(add_content(&mut doc, 0, &Content::Text(crlf)).unwrap_err().to_string().contains("U+000D"));
    let hf = |t: &str| HeaderFooter {
        text: [t.into(), String::new(), String::new(), String::new(), String::new(), String::new()],
        ..HeaderFooter::default()
    };
    add_header_footer(&mut doc, &[0], &hf("Left\r\n<<Bates Number#6#1#ACME-#>>"), true, &cx()).unwrap();
    assert!(add_header_footer(&mut doc, &[0], &hf("A\rB"), true, &cx()).unwrap_err().to_string().contains("U+000D"));
    // Arabic is shaped with the craft-fonts Arabic face (#403): drawable when that face is built
    // in, refused (never written as `?`) when it isn't. Text left to the standard font still counts.
    let arabic = |text: &str| AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: text.into(), ..AddedText::default() };
    // U+061C is in the Arabic block but is a direction mark that's dropped, not shaped: it never
    // makes text drawable, nor (with the face) undrawable.
    assert!(add_content(&mut doc, 0, &Content::Text(arabic("\u{061C}中"))).is_err());
    if pdfcraft_fonts::arabic_has('م') {
        add_content(&mut doc, 0, &Content::Text(arabic("مرحبا Hello"))).unwrap();
        add_content(&mut doc, 0, &Content::Text(arabic("\u{061C}Hello"))).unwrap();
        for text in ["مرحبا 中", "\u{061C}中"] {
            let err = add_content(&mut doc, 0, &Content::Text(arabic(text))).unwrap_err().to_string();
            assert!(err.contains("U+4E2D"), "{text:?}: {err}");
        }
    } else {
        // #403's own refusal says what's missing; the Add-text editor still keeps the draft.
        let err = add_content(&mut doc, 0, &Content::Text(arabic("مرحبا Hello"))).unwrap_err().to_string();
        assert!(err.contains("CRAFT_FONTS_DIR"), "{err}");
        assert_eq!(first_undrawable(&arabic("مرحبا Hello")), Some('م'));
        // An existing item may still take Arabic (#403 keeps it movable), but nothing else that
        // the standard font would write as `?`.
        let n = list_added(&doc).iter().filter(|a| a.page == 0).count();
        add_content(&mut doc, 0, &Content::Text(arabic("x"))).unwrap();
        let err = update_content(&mut doc, 0, n, &Content::Text(arabic("ب中"))).unwrap_err().to_string();
        assert!(err.contains("U+4E2D"), "{err}");
        update_content(&mut doc, 0, n, &Content::Text(arabic("ب"))).unwrap();
    }
}

#[test]
fn fill_sign_flatten_bakes_marks_keeps_other_comments_and_does_not_reuse_pcfl0() {
    use pdfcraft_annot::{FillMark, Markup, Meta, NewAnnotation, Shape, Style, add_annotation, summaries};
    let mut doc = fixture();
    assert_eq!(flatten_fill_sign(&mut doc, &[0]).unwrap(), 0);
    assert!(!doc.is_modified(), "nothing to flatten leaves the file alone");

    let meta = Meta { date: None, id: "x".into() };
    let add = |doc: &mut Document, shape: Shape, contents: &str| {
        let style = Style::default_for(&shape);
        add_annotation(doc, &NewAnnotation { page: 0, shape, style, contents: contents.into(), author: "a".into() }, &meta).unwrap()
    };
    add(&mut doc, Shape::Rectangle { rect: [10.0, 10.0, 80.0, 40.0] }, "box");
    assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), 1);
    let kept = xobject(&doc, 0, b"PCFl0").expect("the first flatten's XObject");

    add(&mut doc, Shape::Typewriter { rect: [20.0, 500.0, 140.0, 520.0], font_size: 10.0 }, "Hello");
    doc.update_dict(annot_at(&doc, 0, 0), |d| {
        d.remove(b"PCFillSign");
    })
    .unwrap();
    add(&mut doc, Shape::Mark { rect: [100.0, 100.0, 120.0, 120.0], mark: FillMark::Check }, "");
    add(&mut doc, Shape::Signature { strokes: vec![vec![[10.0, 50.0], [40.0, 80.0], [70.0, 50.0]]] }, "");
    doc.update_dict(annot_at(&doc, 0, 2), |d| d.set(b"Subj".to_vec(), pdfcraft_cos::PdfString::text("Pencil"))).unwrap();
    add(&mut doc, Shape::TypedSignature { rect: [20.0, 40.0, 90.0, 70.0], contours: vec![vec![[0.1, 0.2], [0.5, 0.9], [0.9, 0.2]]] }, "");
    add(&mut doc, Shape::TextMarkup { kind: Markup::Highlight, quads: vec![[20.0, 400.0, 80.0, 400.0, 20.0, 390.0, 80.0, 390.0]] }, "keep");
    add(&mut doc, Shape::TextBox { rect: [20.0, 300.0, 140.0, 340.0], font_size: 12.0 }, "Note");
    add(&mut doc, Shape::Ink { strokes: vec![vec![[200.0, 200.0], [220.0, 220.0], [240.0, 200.0]]] }, "pencil");
    let old = add(&mut doc, Shape::Signature { strokes: vec![vec![[300.0, 50.0], [330.0, 80.0], [360.0, 50.0]]] }, "");
    doc.update_dict(annot_at(&doc, 0, old), |d| {
        d.remove(b"PCFillSign");
    })
    .unwrap();
    let hidden = add(&mut doc, Shape::Typewriter { rect: [20.0, 600.0, 140.0, 620.0], font_size: 10.0 }, "secret");
    doc.update_dict(annot_at(&doc, 0, hidden), |d| d.set(b"F".to_vec(), Object::Int(2))).unwrap();

    let n = flatten_fill_sign(&mut doc, &[0]).unwrap();
    assert_eq!(n, 5, "text, check, both signatures and the typed signature; the hidden one stays");
    assert_eq!(xobject(&doc, 0, b"PCFl0"), Some(kept), "an existing PCFl0 is not replaced");
    let newest = streams(&doc, 0).last().unwrap().clone();
    assert!(newest.contains("/PCFl1 Do") && !newest.contains("/PCFl0 Do"), "{newest}");

    let doc = reopen(&doc);
    let mut left: Vec<String> = summaries(&doc).iter().map(|s| s.contents.clone().unwrap_or_default()).collect();
    left.sort();
    assert_eq!(left, ["Note", "keep", "pencil", "secret"]);
    assert!(summaries(&doc).iter().any(|s| s.intent.as_deref() == Some("FreeTextTypeWriter")), "the hidden typewriter stays");
    assert_eq!(summaries(&doc).iter().filter(|s| s.intent.as_deref() == Some("FreeTextTypeWriter")).count(), 1);
}

/// #314: text and images drawn by (nested) form XObjects are read for export, placed through
/// every form matrix and set in each form's own fonts; editing still sees only the page's text.
#[test]
fn reading_follows_form_xobjects() {
    let page = stream("", "q 1 0 0 1 10 0 cm /Fm1 Do Q BT /F1 12 Tf 72 100 Td (Page text) Tj ET");
    let fm1 = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 600 800] /Matrix [1 0 0 1 0 50] /Resources << /Font << /F1 7 0 R >> /XObject << /Fm2 8 0 R >> >>",
        "BT /F1 18 Tf 72 700 Td (Hello from a test invoice) Tj ET /Fm2 Do",
    );
    let fm2 = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 300 400] /Matrix [2 0 0 2 0 0] /Resources << /Font << /F2 9 0 R >> /XObject << /Im1 10 0 R >> >>",
        "BT /F2 10 Tf 50 100 Td (Nested) Tj ET q 20 0 0 10 5 5 cm /Im1 Do Q",
    );
    let image = stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8", "A");
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 5 0 R >> /XObject << /Fm1 6 0 R >> >> /Contents 4 0 R >>",
        &page,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
        &fm1,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        &fm2,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Bold >>",
        &image,
    ]);
    let close = |a: f64, b: f64| (a - b).abs() < 0.01;

    let blocks = text::reading_blocks(&doc, 0).unwrap();
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["Hello from a test invoice", "Nested", "Page text"]);
    let (hello, nested, own) = (&blocks[0], &blocks[1], &blocks[2]);
    // Hello: (72, 700) moved by the form's (0, 50) and the page's (10, 0).
    assert_eq!(hello.base_font, "Helvetica", "the form's own /F1, not the page's");
    assert!(close(hello.rect[0], 82.0) && close(hello.size, 18.0), "{hello:?}");
    // Nested: (50, 100) doubled by Fm2, then Fm1 and the page: (110, 250), 20 pt.
    assert_eq!(nested.base_font, "Times-Bold");
    assert!(nested.bold);
    assert!(close(nested.rect[0], 110.0) && close(nested.size, 20.0), "{nested:?}");
    assert_eq!(own.base_font, "Courier");

    let images = images::reading_images(&doc, 0).unwrap();
    assert_eq!(images.len(), 1, "{images:?}");
    let r = images[0].rect;
    assert!(close(r[0], 20.0) && close(r[1], 60.0) && close(r[2], 60.0) && close(r[3], 80.0), "{r:?}");
    assert_eq!((images[0].width, images[0].height), (1, 1));

    // Editing works on the page's own streams only, unchanged.
    let editable: Vec<String> = text::text_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(editable, ["Page text"]);
    // The page draws one figure of its own (Fm1, edited as a whole); the image inside Fm2 is not
    // a page image.
    let own = images::page_images(&doc, 0).unwrap();
    assert_eq!(own.len(), 1, "{own:?}");
    assert!(own[0].is_form && own[0].object == Some(pdfcraft_cos::ObjRef::new(6, 0)), "{own:?}");
}

/// A page's `/XObject` and a form's `/Resources` are read wherever they are stored, and a form with
/// none of its own, or with something else there, uses the ones it was drawn with (PDF 1.1 files).
/// Each of these pages shows one line of text and one picture through one form.
#[test]
fn reading_follows_forms_however_their_resources_are_stored() {
    let body = "BT /F1 12 Tf 72 700 Td (In the form) Tj ET q 20 0 0 10 5 5 cm /Im Do Q";
    let (font, direct) = ("/Font << /F1 5 0 R >>", "/XObject << /Fm 6 0 R /Im 7 0 R >>");
    for (page, form) in [
        (format!("<< {font} {direct} >>"), ""),
        (format!("<< {font} /XObject 9 0 R >>"), ""),
        (format!("<< /Font 10 0 R {direct} >>"), ""),
        (format!("<< {font} {direct} >>"), "/Resources 99 0 R"),
        (format!("<< {font} {direct} >>"), "/Resources 12"),
        ("<< /XObject << /Fm 6 0 R >> >>".to_string(), "/Resources 8 0 R"),
        ("<< /XObject 9 0 R >>".to_string(), "/Resources << /Font << /F1 5 0 R >> /XObject 9 0 R >>"),
    ] {
        let doc = build(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            &format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources {page} /Contents 4 0 R >>"),
            &stream("", "/Fm Do"),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
            &stream(&format!("/Type /XObject /Subtype /Form /BBox [0 0 600 800] {form}"), body),
            &stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8", "A"),
            "<< /Font << /F1 5 0 R >> /XObject 9 0 R >>",
            "<< /Fm 6 0 R /Im 7 0 R >>",
            "<< /F1 5 0 R >>",
        ]);
        let blocks = text::reading_blocks(&doc, 0).unwrap();
        let texts: Vec<(&str, &str)> = blocks.iter().map(|b| (b.text.as_str(), b.base_font.as_str())).collect();
        assert_eq!(texts, [("In the form", "Courier")], "{page} {form}");
        assert_eq!(images::reading_images(&doc, 0).unwrap().len(), 1, "{page} {form}");
    }
}

/// A chain of forms, each drawing the next twenty times, twelve deep: within the depth cap but
/// 20^11 visits without a budget. Reading stops at the page's visit budget instead of hanging.
#[test]
fn form_xobjects_that_fan_out_stop_at_the_visit_budget() {
    let font = "/Font << /F1 5 0 R >>";
    let levels = 12;
    let mut objs = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 5 0 R >> /XObject << /N 6 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        stream("", "/N Do"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    for level in 0..levels {
        let next = 7 + level;
        objs.push(if level + 1 == levels {
            stream(&format!("/Subtype /Form /BBox [0 0 9 9] /Resources << {font} >>"), "BT /F1 10 Tf 10 10 Td (leaf) Tj ET")
        } else {
            stream(&format!("/Subtype /Form /BBox [0 0 9 9] /Resources << {font} /XObject << /N {next} 0 R >> >>"), &"/N Do ".repeat(20))
        });
    }
    let refs: Vec<&str> = objs.iter().map(String::as_str).collect();
    let doc = build(&refs);
    let start = std::time::Instant::now();
    let _ = text::reading_blocks(&doc, 0).unwrap();
    assert!(images::reading_images(&doc, 0).unwrap().is_empty());
    assert!(start.elapsed() < std::time::Duration::from_secs(30), "took {:?}", start.elapsed());
}

/// #3120: export reads text from forms as deep and as often as the renderer draws them, and says
/// when a limit left some out: a text form drawn 5 000 times (past the old budget of 4 096) or
/// forms nested 13 deep (past the old 12) lost the form drawn after them, silently.
#[test]
fn reading_follows_forms_as_far_as_the_renderer_and_says_where_it_stops() {
    // Covers "Secret" with a black box and writes "TAIL": drawn last, so a reader that stops
    // early misses it.
    const TAIL: &str = "0 g 70 692 90 30 re f BT /F1 24 Tf 72 600 Td (TAIL) Tj ET";
    let tail = |doc: &Document| {
        let read = text::reading_blocks_checked(doc, 0).unwrap();
        (read.blocks.iter().any(|b| b.text.contains("TAIL")), read.truncated)
    };
    let form = |name: &str, body: &str| (name.to_string(), body.to_string());

    let rows =
        |n: usize| page_with_forms(&format!("{}/C Do", "/R Do ".repeat(n)), &[form("R", "BT /F1 6 Tf 300 100 Td (row) Tj ET"), form("C", TAIL)]);
    assert_eq!(tail(&rows(5_000)), (true, false));

    // The innermost form also draws an image: an image at the depth limit is read, and leaves
    // nothing out.
    let chain = |depth: usize| {
        let mut forms: Vec<_> = (1..depth).map(|i| form(&format!("D{i}"), &format!("/D{} Do", i + 1))).collect();
        forms.push(form(&format!("D{depth}"), &format!("{TAIL} q 20 0 0 10 5 5 cm /Im Do Q")));
        page_with_forms("/D1 Do", &forms)
    };
    assert_eq!(tail(&chain(13)), (true, false));
    // The renderer's limit, written out so the test does not move with the constant.
    assert_eq!(tail(&chain(50)), (true, false));
    assert_eq!(tail(&chain(51)), (false, true));
    let deepest = chain(text::MAX_FORM_DEPTH);
    assert_eq!(tail(&deepest), (true, false));
    assert_eq!(images::reading_images(&deepest, 0).unwrap().len(), 1);
    let deep = chain(text::MAX_FORM_DEPTH + 1);
    assert_eq!(tail(&deep), (false, true));
    assert!(images::reading_images(&deep, 0).unwrap().is_empty());

    // A wrapper form A draws a dot form B `n` times, then C. A, the dots and C use the page's
    // budget up exactly; one dot more leaves C unread.
    let fanout =
        |n: usize| page_with_forms("/A Do", &[form("A", &format!("{}/C Do", "/B Do ".repeat(n))), form("B", "0 g 10 10 1 1 re f"), form("C", TAIL)]);
    assert_eq!(tail(&fanout(text::MAX_FORM_VISITS - 2)), (true, false));
    let many = fanout(text::MAX_FORM_VISITS - 1);
    assert_eq!(tail(&many), (false, true));
    assert!(images::reading_images(&many, 0).unwrap().is_empty());
    // Without the flag, reading is unchanged.
    let blocks: Vec<String> = text::reading_blocks(&many, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(blocks, ["Secret"]);
}

/// #3120: with the renderer's limits, forms that each draw the next a hundred times, 12 levels
/// deep (100^11 forms) or past the depth cap, still stop at the page's budget quickly, and say
/// that text was left out.
#[test]
fn forms_that_fan_out_a_hundred_ways_stop_quickly_and_say_so() {
    for levels in [12, text::MAX_FORM_DEPTH + 1] {
        let mut forms: Vec<(String, String)> = (1..levels).map(|i| (format!("N{i}"), format!("/N{} Do ", i + 1).repeat(100))).collect();
        forms.push((format!("N{levels}"), "BT /F1 10 Tf 10 10 Td (leaf) Tj ET".to_string()));
        let doc = page_with_forms("/N1 Do", &forms);
        let start = std::time::Instant::now();
        assert!(text::reading_blocks_checked(&doc, 0).unwrap().truncated, "{levels} levels");
        assert!(images::reading_images(&doc, 0).unwrap().is_empty());
        assert!(start.elapsed() < std::time::Duration::from_secs(30), "{levels} levels took {:?}", start.elapsed());
    }
}

/// Forms that draw themselves or each other, have no resources, a malformed matrix, or are
/// missing are read once (or skipped) without panicking or looping.
#[test]
fn hostile_form_xobjects_are_read_once() {
    let page = stream("", "/Loop Do /A Do /NoRes Do /Bad Do /Missing Do /F1 Do");
    let font = "/Font << /F1 5 0 R >>";
    let lp = stream(
        &format!("/Subtype /Form /BBox [0 0 9 9] /Resources << {font} /XObject << /Loop 6 0 R >> >>"),
        "/Loop Do BT /F1 10 Tf 10 10 Td (once) Tj ET",
    );
    let a = stream(
        &format!("/Subtype /Form /BBox [0 0 9 9] /Resources << {font} /XObject << /B 8 0 R >> >>"),
        "/B Do BT /F1 10 Tf 10 100 Td (from a) Tj ET",
    );
    let b = stream(
        &format!("/Subtype /Form /BBox [0 0 9 9] /Resources << {font} /XObject << /A 7 0 R >> >>"),
        "/A Do BT /F1 10 Tf 10 200 Td (from b) Tj ET",
    );
    let nores = stream("/Subtype /Form /BBox [0 0 9 9]", "BT /F1 10 Tf 10 300 Td (inherited) Tj ET");
    let bad = stream(&format!("/Subtype /Form /BBox [0 0 9 9] /Matrix [1 0 0] /Resources << {font} >>"), "BT /F1 10 Tf 10 400 Td (bad matrix) Tj ET");
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 5 0 R >> /XObject << /Loop 6 0 R /A 7 0 R /NoRes 9 0 R /Bad 10 0 R >> >> /Contents 4 0 R >>",
        &page,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        &lp,
        &a,
        &b,
        &nores,
        &bad,
    ]);
    let texts: Vec<String> = text::reading_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(texts, ["once", "from b", "from a", "inherited", "bad matrix"]);
    assert!(images::reading_images(&doc, 0).unwrap().is_empty());
}

#[test]
fn added_thai_text_wraps_clusters_and_survives_reopen() {
    let mut doc = fixture();
    let t = AddedText {
        rect: [20.0, 100.0, 250.0, 300.0], text: "สวัสดี น้ำ ผู้ใช้ ปู่ ๑๒๓ Hello 123".into(), size: 16.0, ..AddedText::default()
    };
    if pdfcraft_fonts::document_thai_font().is_none() {
        assert_eq!(first_undrawable(&t), Some('ส'));
        let before = list_added(&doc);
        let err = add_content(&mut doc, 0, &Content::Text(t)).unwrap_err().to_string();
        assert!(err.contains("Thai") && err.contains("CRAFT_FONTS_DIR"), "{err}");
        assert_eq!(list_added(&doc), before);
        return;
    }
    assert_eq!(first_undrawable(&t), None);
    add_content(&mut doc, 0, &Content::Text(t.clone())).unwrap();
    let reopened = reopen(&doc);
    assert_eq!(list_added(&reopened)[0].content, Content::Text(AddedText { rect: super::added::text_rect(&t), ..t.clone() }));
    let narrow = AddedText { rect: [0.0, 0.0, 16.0, 200.0], text: "กิ่กิ่กิ่".into(), ..t.clone() };
    assert_eq!(super::added::lines(&narrow), ["กิ่", "กิ่", "กิ่"]);
    update_content(&mut doc, 0, 0, &Content::Text(AddedText { text: "แก้ไข น้ำ".into(), ..t.clone() })).unwrap();
    assert!(!own(&doc, 0).is_empty());
    let bad = Content::Text(AddedText { text: "ไทย 日".into(), ..t });
    let before = list_added(&doc);
    assert!(update_content(&mut doc, 0, 0, &bad).is_err());
    assert_eq!(list_added(&doc), before);
    delete_content(&mut doc, 0, 0).unwrap();
    assert!(list_added(&doc).is_empty());
    assert!(arabic_font_names(&doc, 0).is_empty());
}

#[test]
fn group_moves_preserve_glyph_bytes_and_unselected_text_state() {
    for content in [
        "BT /F1 12 Tf 1.3 Tc 2 Tw 80 Tz 14 TL 1 0.2 -0.1 1 50 600 Tm (A\\101) Tj 1 0 0 rg (Neighbour) Tj T* (Next) Tj ET",
        "BT /F1 12 Tf 14 TL 50 600 Td (Before) Tj 0 0 1 rg (Quoted) ' 1 0 0 rg (After) Tj ET",
        "BT /F1 12 Tf 14 TL 50 600 Td (Before) Tj 0 0 1 rg 2 1 (Quoted) \" 1 0 0 rg [(After) -120 (wards)] TJ T* (Last) Tj ET",
        "q 1.2 0.3 -0.1 0.9 20 30 cm BT /F1 12 Tf 50 600 Td [(A) -120 (B)] TJ 1 0 0 rg (After) Tj ET Q %keep-this-comment\n",
        "BT /F1 12.3456789 Tf 81.23456789 Tz 0.123456789 Ts 14 TL 50 600 Td (Before) Tj 0 0 1 rg 2.123456789 1.987654321 (Quoted) \" 1 0 0 rg (After) Tj ET",
    ] {
        let mut doc = text_page(content);
        let before = text::text_lines(&doc, 0).unwrap();
        let blocks = text::text_blocks(&doc, 0).unwrap();
        let index = blocks.iter().position(|b| b.text.contains("Quoted")).unwrap_or(0);
        let selected: Vec<_> = blocks[index].lines.iter().map(|i| before[*i].text.clone()).collect();
        let glyph_bytes = |doc: &Document| {
            pdfcraft_content::parse(&page_content_bytes(doc, 0))
                .ops
                .iter()
                .flat_map(|op| {
                    op.operands
                        .iter()
                        .flat_map(|o| match o {
                            Object::String(s) => vec![s.bytes.clone()],
                            Object::Array(a) => a.iter().filter_map(|v| v.as_string().map(|s| s.bytes.clone())).collect(),
                            _ => Vec::new(),
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let original_bytes = glyph_bytes(&doc);
        let target = ObjectTarget { kind: ObjectKind::Text, index };
        move_objects(&mut doc, 0, &[target], [35.0, -27.0]).unwrap();
        let doc = reopen(&doc);
        assert_eq!(glyph_bytes(&doc), original_bytes, "original codes and kerning strings survive");
        let after = text::text_lines(&doc, 0).unwrap();
        assert_eq!(after.len(), before.len(), "{content}");
        for old in before {
            let new = after.iter().find(|l| l.text == old.text).unwrap();
            let expected = if selected.contains(&old.text) {
                [old.rect[0] + 35.0, old.rect[1] - 27.0, old.rect[2] + 35.0, old.rect[3] - 27.0]
            } else {
                old.rect
            };
            assert!(new.rect.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-6), "{}: {:?} != {:?}", old.text, new.rect, expected);
            assert_eq!((new.font.clone(), new.color), (old.font, old.color));
        }
        if content.contains("%keep-this-comment") {
            assert!(String::from_utf8_lossy(&page_content_bytes(&doc, 0)).contains("%keep-this-comment"));
        }
    }
}

#[test]
fn group_moves_preserve_small_offsets_under_large_transforms() {
    let mut doc = image_page();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let content = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"q 100000 0 0 100000 100 100 cm /Im0 Do Q q 10000 0 0 10000 0 0 cm BT /F1 0.00123456789 Tf 0.02 0.06 Td (Small) Tj 1 0 0 rg (Neighbour) Tj ET Q",
    )));
    doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(content))).unwrap();
    let before = editable_objects(&doc, 0).unwrap();
    let targets = before.iter().filter(|o| o.text.as_deref() != Some("Neighbour")).map(|o| o.target).collect::<Vec<_>>();
    assert_eq!(targets.len(), 2);
    let image_before = images::page_images(&doc, 0).unwrap()[0].rect;
    let offset = [0.00123456789, -0.00987654321];
    move_objects(&mut doc, 0, &targets, offset).unwrap();
    let reopened = reopen(&doc);
    let image_after = images::page_images(&reopened, 0).unwrap()[0].clone();
    assert!(
        close(image_after.rect, [image_before[0] + offset[0], image_before[1] + offset[1], image_before[2] + offset[0], image_before[3] + offset[1]]),
        "full image placement preserves sub-point movement"
    );
    let after = editable_objects(&reopened, 0).unwrap();
    for old in before {
        let new = after.iter().find(|o| o.target == old.target).unwrap();
        let [dx, dy] = if targets.contains(&old.target) { offset } else { [0.0, 0.0] };
        if old.target.kind == crate::ObjectKind::Image {
            // The full placement moves; the visible right/top edges stay at the page crop.
            assert_eq!(Some(new.rect), image_after.visible_rect);
        } else {
            assert!(close(new.rect, [old.rect[0] + dx, old.rect[1] + dy, old.rect[2] + dx, old.rect[3] + dy]), "{old:?}: {new:?}");
        }
    }
}

#[test]
fn mixed_object_inventory_does_not_select_added_content_twice() {
    let mut doc = image_page();
    add_content(&mut doc, 0, &Content::Text(AddedText { text: "Added label".into(), rect: [50.0, 350.0, 200.0, 380.0], ..AddedText::default() }))
        .unwrap();
    let before = editable_objects(&doc, 0).unwrap();
    assert_eq!(before.len(), 3, "one native paragraph, one native image, one added text");
    let targets = before.iter().map(|o| o.target).collect::<Vec<_>>();
    let image = images::page_images(&doc, 0).unwrap()[0].object.unwrap();
    let original_image = doc.get(image).clone();
    move_objects(&mut doc, 0, &targets, [20.0, -15.0]).unwrap();
    let doc = reopen(&doc);
    let after = editable_objects(&doc, 0).unwrap();
    assert_eq!(after.len(), before.len());
    for old in before {
        let new = after.iter().find(|o| o.target == old.target).unwrap();
        assert!(close(new.rect, [old.rect[0] + 20.0, old.rect[1] - 15.0, old.rect[2] + 20.0, old.rect[3] - 15.0]));
        assert_eq!(new.text, old.text);
    }
    assert_eq!(doc.get(image).clone(), original_image, "pixel data is shared, not recompressed");
}

#[test]
fn group_move_refusals_leave_the_object_graph_untouched() {
    let mut doc = image_page();
    let target = editable_objects(&doc, 0).unwrap()[0].target;
    let original = pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap();
    for (targets, offset) in [
        (vec![], [1.0, 2.0]),
        (vec![target, target], [1.0, 2.0]),
        (vec![target, ObjectTarget { kind: ObjectKind::Image, index: usize::MAX }], [1.0, 2.0]),
        (vec![target], [f64::NAN, 2.0]),
        (vec![target], [f64::INFINITY, 2.0]),
        (vec![target], [0.0, 0.0]),
        (vec![target], [1e300, 2.0]),
        (vec![target; MAX_MOVE_OBJECTS + 1], [1.0, 2.0]),
    ] {
        assert!(move_objects(&mut doc, 0, &targets, offset).is_err());
        assert_eq!(pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap(), original);
    }
    let mut clipped = text_page("BT /F1 12 Tf 50 600 Td (Ordinary) Tj ET BT /F1 12 Tf 4 Tr 50 400 Td (Clipping) Tj ET");
    let targets = editable_objects(&clipped, 0).unwrap().iter().map(|o| o.target).collect::<Vec<_>>();
    let original = pdfcraft_cos::write_full(&clipped, &SaveOptions::default()).unwrap();
    assert!(move_objects(&mut clipped, 0, &targets, [10.0, 5.0]).unwrap_err().to_string().contains("clipping"));
    assert_eq!(pdfcraft_cos::write_full(&clipped, &SaveOptions::default()).unwrap(), original);
}

#[test]
fn added_and_native_objects_move_together_on_cropped_rotated_user_unit_pages() {
    for rotation in [0, 90, 180, 270] {
        let mut doc = image_page();
        let page = pdfcraft_model::pages(&doc)[0].obj;
        doc.update_dict(page, |d| {
            d.set(b"CropBox".to_vec(), Object::Array([10, 20, 500, 750].map(Object::Int).to_vec()));
            d.set(b"Rotate".to_vec(), Object::Int(rotation));
            d.set(b"UserUnit".to_vec(), Object::Int(2));
        })
        .unwrap();
        add_content(&mut doc, 0, &Content::Text(AddedText { text: "Added".into(), rect: [60.0, 100.0, 180.0, 125.0], ..AddedText::default() }))
            .unwrap();
        let before = editable_objects(&doc, 0).unwrap();
        move_objects(&mut doc, 0, &before.iter().map(|o| o.target).collect::<Vec<_>>(), [12.0, -8.0]).unwrap();
        let after = editable_objects(&reopen(&doc), 0).unwrap();
        for (a, b) in before.iter().zip(after) {
            assert!(
                close(b.rect, [a.rect[0] + 12.0, a.rect[1] - 8.0, a.rect[2] + 12.0, a.rect[3] - 8.0]),
                "rotation {rotation}: {:?} -> {:?}",
                a.rect,
                b.rect
            );
        }
    }
}

#[test]
fn group_moves_keep_split_stream_tokens_and_shared_form_resources() {
    let mut doc = split_streams_page();
    let before = text::text_lines(&doc, 0).unwrap();
    let objects: Vec<_> = editable_objects(&doc, 0).unwrap().iter().map(|o| o.target).collect();
    move_objects(&mut doc, 0, &objects, [30.0, -20.0]).unwrap();
    let doc = reopen(&doc);
    let after = text::text_lines(&doc, 0).unwrap();
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.text, b.text);
        assert!(close(b.rect, [a.rect[0] + 30.0, a.rect[1] - 20.0, a.rect[2] + 30.0, a.rect[3] - 20.0]));
    }
    let joined = page_content_bytes(&doc, 0);
    let ops = pdfcraft_content::parse(&joined).ops;
    assert_eq!(ops.iter().find(|o| o.is("BDC")).unwrap().operands.len(), 2);
    assert!(ops.iter().any(|o| o.is("EMC")));
    // Two pages share a content stream and a Form; only one placement on one page moves.
    let mut doc = fixture();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 60, 30].map(Object::Int).to_vec()));
    form.set(b"PrivateData".to_vec(), Object::String(pdfcraft_cos::PdfString::literal(b"retain this".to_vec())));
    let artwork = doc.add(Object::Stream(Stream::flate(form, b"1 0 0 rg 0 0 60 30 re f")));
    doc.update_dict(pdfcraft_cos::ObjRef::new(6, 0), |d| {
        let mut xo = Dict::new();
        xo.set(b"Figure".to_vec(), Object::Ref(artwork));
        d.set(b"XObject".to_vec(), Object::Dict(xo));
    })
    .unwrap();
    doc.set(
        pdfcraft_cos::ObjRef::new(7, 0),
        Object::Stream(Stream::flate(
            Dict::new(),
            b"q 1 0 0 1 40 80 cm /Figure Do Q q 1 0 0 1 300 400 cm /Figure Do Q BT /F1 9 Tf 40 160 Td (Caption) Tj ET",
        )),
    );
    let original_form = doc.get(artwork);
    let source_page = page_content_bytes(&doc, 1);
    let images = images::page_images(&doc, 0).unwrap();
    move_objects(
        &mut doc,
        0,
        &[ObjectTarget { kind: ObjectKind::Image, index: 0 }, ObjectTarget { kind: ObjectKind::Text, index: 0 }],
        [15.0, -10.0],
    )
    .unwrap();
    let after = images::page_images(&doc, 0).unwrap();
    assert!(close(after[0].rect, [55.0, 70.0, 115.0, 100.0]));
    assert_eq!(after[1].rect, images[1].rect);
    assert_eq!(*doc.get(artwork), *original_form);
    assert_eq!(page_content_bytes(&doc, 1), source_page);
}

#[test]
fn justified_text_in_a_composite_font_fills_each_row_with_adjustments_after_its_spaces() {
    // Two-byte codes, so word spacing (single-byte code 32 only) can't stretch the spaces.
    let hex = |s: &str| s.bytes().map(|b| format!("00{b:02x}")).collect::<String>();
    let src = format!(
        "BT /F1 10 Tf 12 TL 100 700 Td <{}> Tj T* <{}> Tj T* <{}> Tj ET",
        hex("One two three four five six seven"),
        hex("eight nine ten eleven twelve"),
        hex("and then the end of it")
    );
    let mut doc = text_page(&src);
    let unicode = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"1 begincodespacerange <0000> <ffff> endcodespacerange 1 beginbfrange <0020> <007e> <0020> endbfrange",
    )));
    let font = Dict::from_iter([
        (b"Type".to_vec(), Object::name("Font")),
        (b"Subtype".to_vec(), Object::name("Type0")),
        (b"BaseFont".to_vec(), Object::name("SyntheticCID")),
        (b"Encoding".to_vec(), Object::name("Identity-H")),
        (b"ToUnicode".to_vec(), Object::Ref(unicode)),
        (
            b"DescendantFonts".to_vec(),
            Object::Array(vec![Object::Dict(Dict::from_iter([
                (b"Subtype".to_vec(), Object::name("CIDFontType2")),
                (b"DW".to_vec(), Object::Int(500)),
            ]))]),
        ),
    ]);
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let resources = Dict::from_iter([(b"Font".to_vec(), Object::Dict(Dict::from_iter([(b"F1".to_vec(), Object::Dict(font))])))]);
    doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
    let before = text::text_blocks(&doc, 0).unwrap()[0].clone();
    let right = before.rect[2];
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { align: Some(added::Align::Justify), ..Default::default() }).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!(lines.len() > 2, "{lines:?}");
    for l in &lines[..lines.len() - 1] {
        assert!((l.rect[2] - right).abs() < 1.0, "{:?} ends at {} not {right}", l.text, l.rect[2]);
    }
    // The source font shows the text, with its spaces, and the text reads as before, rewrapped.
    let flat = |s: &str| s.replace('\n', " ");
    assert_eq!(flat(&text::text_blocks(&doc, 0).unwrap()[0].editor_text), flat(&before.editor_text));
    let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
    assert!(ops.iter().all(|o| !o.is("Tf") || o.operands.first().and_then(|n| n.as_name()) == Some(b"F1".as_slice())), "{ops:?}");
    assert!(ops.iter().any(|o| o.is("TJ")), "{ops:?}");
    assert!(ops.iter().filter(|o| o.is("Tw")).all(|o| o.operands.first().and_then(|n| n.as_f64()) == Some(0.0)), "{ops:?}");
}

#[test]
fn group_moves_preserve_encoded_glyphs_and_refuse_vertical_cmaps() {
    let mut doc = text_page("BT /F1 12 Tf 40 400 Td <004100420043> Tj ET");
    let mut font = Dict::from_iter([
        (b"Type".to_vec(), Object::name("Font")),
        (b"Subtype".to_vec(), Object::name("Type0")),
        (b"BaseFont".to_vec(), Object::name("SyntheticCID")),
        (b"Encoding".to_vec(), Object::name("Identity-H")),
    ]);
    let unicode = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"1 begincodespacerange <0000> <ffff> endcodespacerange 3 beginbfchar <0041> <0041> <0042> <0042> <0043> <0043> endbfchar",
    )));
    font.set(b"ToUnicode".to_vec(), Object::Ref(unicode));
    font.set(
        b"DescendantFonts".to_vec(),
        Object::Array(vec![Object::Dict(Dict::from_iter([(b"Subtype".to_vec(), Object::name("CIDFontType2")), (b"DW".to_vec(), Object::Int(600))]))]),
    );
    // No external font asset: this synthetic font is enough to exercise raw CID bytes.
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let mut resources = Dict::new();
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Dict(font.clone()));
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(resources.clone()))).unwrap();
    let original = text::text_lines(&doc, 0).unwrap();
    assert!(!original.is_empty());
    move_objects(&mut doc, 0, &[ObjectTarget { kind: ObjectKind::Text, index: 0 }], [25.0, 15.0]).unwrap();
    let encoded =
        |doc: &Document| pdfcraft_content::parse(&page_content_bytes(doc, 0)).ops.into_iter().find(|o| o.is("Tj")).unwrap().operands[0].clone();
    assert_eq!(encoded(&doc).as_string().unwrap().bytes, [0, 65, 0, 66, 0, 67]);
    let after = text::text_lines(&doc, 0).unwrap();
    assert_eq!(original[0].text, after[0].text);
    for encoding in [
        Object::name("Identity-V"),
        Object::Stream(Stream::flate(Dict::new(), b"/WMode 1 def")),
        Object::Stream(Stream::flate(Dict::from_iter([(b"WMode".to_vec(), Object::Int(1))]), b"")),
    ] {
        let mut f = font.clone();
        f.set(b"Encoding".to_vec(), encoding);
        let mut fonts = Dict::new();
        fonts.set(b"F1".to_vec(), Object::Dict(f));
        resources.set(b"Font".to_vec(), Object::Dict(fonts));
        doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(resources.clone()))).unwrap();
        let before = pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap();
        assert!(move_objects(&mut doc, 0, &[ObjectTarget { kind: ObjectKind::Text, index: 0 }], [5.0, 5.0]).is_err());
        assert_eq!(pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap(), before);
    }
}

#[test]
fn moving_added_content_after_page_rotation_preserves_streams_and_resources() {
    let mut doc = text_page("BT /F1 12 Tf 40 400 Td (Native) Tj ET");
    added::add_content(
        &mut doc,
        0,
        &Content::Text(added::AddedText { text: "Added label".into(), rect: [50.0, 300.0, 200.0, 330.0], ..Default::default() }),
    )
    .unwrap();
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let resources = pdfcraft_model::pages(&doc)[0].dict.get(b"Resources").cloned();
    let item = added::list_added(&doc)[0].clone();
    let source = stream_bytes(&doc, &Object::Ref(item.obj)).unwrap();
    let content_after_cm = |bytes: &[u8]| {
        let ops = pdfcraft_content::parse(bytes).ops;
        bytes[ops[1].span.end..].to_vec()
    };
    for rotation in [90, 180, 270, 0] {
        doc.update_dict(p, |d| d.set(b"Rotate".to_vec(), Object::Int(rotation))).unwrap();
        let before_lines = text::text_lines(&doc, 0).unwrap();
        let before_objects = editable_objects(&doc, 0).unwrap();
        let a = before_objects.iter().find(|o| o.target.kind == ObjectKind::Added).unwrap();
        // The logical added box must contain the actual source glyph geometry after rotation.
        let label = before_lines.iter().find(|l| l.text == "Added label").unwrap();
        assert!(
            label.rect[0] >= a.rect[0] - 0.01
                && label.rect[2] <= a.rect[2] + 0.01
                && label.rect[1] >= a.rect[1] - 0.01
                && label.rect[3] <= a.rect[3] + 0.01
        );
        move_objects(&mut doc, 0, &[a.target], [13.0, -17.0]).unwrap();
        let after_lines = text::text_lines(&doc, 0).unwrap();
        for (old, new) in before_lines.iter().zip(&after_lines) {
            let delta = if old.text == "Added label" { [13.0, -17.0] } else { [0.0, 0.0] };
            assert_eq!(old.text, new.text);
            assert!(close(new.rect, [old.rect[0] + delta[0], old.rect[1] + delta[1], old.rect[2] + delta[0], old.rect[3] + delta[1]]));
        }
        let current = added::list_added(&doc)[0].clone();
        let bytes = stream_bytes(&doc, &Object::Ref(current.obj)).unwrap();
        assert_eq!(content_after_cm(&bytes), content_after_cm(&source), "all drawing bytes after the placement, including glyphs, remain identical");
        assert_eq!(pdfcraft_model::pages(&doc)[0].dict.get(b"Resources").cloned(), resources);
        doc = reopen(&doc);
    }
}

#[test]
fn group_moves_of_shared_added_streams_change_only_the_selected_page() {
    let mut doc = fixture();
    added::add_content(
        &mut doc,
        0,
        &Content::Text(added::AddedText { text: "Shared added label".into(), rect: [40.0, 400.0, 240.0, 420.0], ..Default::default() }),
    )
    .unwrap();
    let pages = pdfcraft_model::pages(&doc);
    let contents = pages[0].dict.get(b"Contents").cloned().unwrap();
    doc.update_dict(pages[1].obj, |d| d.set(b"Contents".to_vec(), contents)).unwrap();
    let doc = reopen(&doc);
    let mut doc = doc;
    let other = page_content_bytes(&doc, 1);
    let before = editable_objects(&doc, 0).unwrap();
    let target = before.iter().find(|o| o.target.kind == ObjectKind::Added).unwrap().target;
    move_objects(&mut doc, 0, &[target], [15.0, -20.0]).unwrap();
    assert_eq!(page_content_bytes(&doc, 1), other, "a shared stream is copied on write");
    assert_ne!(added::list_added(&doc).iter().find(|a| a.page == 0).unwrap().obj, added::list_added(&doc).iter().find(|a| a.page == 1).unwrap().obj);
}

#[test]
fn group_moves_handle_added_content_under_inherited_graphics_state() {
    let mut doc = text_page("2 0 0 2 10 20 cm BT /F1 12 Tf 40 250 Td (Native) Tj ET");
    added::add_content(
        &mut doc,
        0,
        &Content::Text(added::AddedText { text: "Added label".into(), rect: [50.0, 100.0, 200.0, 130.0], ..Default::default() }),
    )
    .unwrap();
    let before = text::text_lines(&doc, 0).unwrap();
    let target = editable_objects(&doc, 0).unwrap().iter().find(|o| o.target.kind == ObjectKind::Added).unwrap().target;
    move_objects(&mut doc, 0, &[target], [30.0, -20.0]).unwrap();
    let after = text::text_lines(&doc, 0).unwrap();
    for (old, new) in before.iter().zip(&after) {
        let delta = if old.text == "Added label" { [30.0, -20.0] } else { [0.0, 0.0] };
        assert!(close(new.rect, [old.rect[0] + delta[0], old.rect[1] + delta[1], old.rect[2] + delta[0], old.rect[3] + delta[1]]));
    }
}

#[test]
fn group_selection_refuses_incomplete_content_and_resource_budget_overruns() {
    for data in [
        format!("{} BT /F1 12 Tf 40 400 Td (Target) Tj ET", "q ".repeat(1025)),
        format!("{} BT /F1 12 Tf 40 400 Td (Target) Tj ET", "n ".repeat(100001)),
    ] {
        let mut doc = text_page(&data);
        let before = pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap();
        assert!(editable_objects(&doc, 0).is_err());
        assert!(move_objects(&mut doc, 0, &[ObjectTarget { kind: ObjectKind::Text, index: 0 }], [10.0, 10.0]).is_err());
        assert_eq!(pdfcraft_cos::write_full(&doc, &SaveOptions::default()).unwrap(), before);
    }
    let mut doc = text_page("BT /F1 12 Tf 40 400 Td (Target) Tj ET");
    let page = pdfcraft_model::pages(&doc)[0].obj;
    doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(pdfcraft_cos::ObjRef::new(999999, 0)))).unwrap();
    assert!(editable_objects(&doc, 0).is_err(), "a missing stream cannot produce a partial inventory");
    let mut filter = Dict::new();
    filter.set(b"Filter".to_vec(), Object::name("NotAFilter"));
    let broken = doc.add(Object::Stream(Stream::from_raw(filter, vec![1, 2, 3])));
    doc.update_dict(page, |d| d.set(b"Contents".to_vec(), Object::Ref(broken))).unwrap();
    assert!(editable_objects(&doc, 0).is_err(), "an undecodable stream is refused");
}

/// Invisible text moves on top only when something painted after it covers it: an XObject drawn
/// over its box (an image, or a form holding one), or any inline image or shading. An image
/// clipped away entirely, or drawn elsewhere, does not move it.
#[test]
fn revealed_text_moves_on_top_only_when_painted_over() {
    let on_top_of = |doc: Document| {
        let block = text::text_blocks(&doc, 0).unwrap().remove(0);
        let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
        let at = ops.iter().position(|o| o.is("BT")).unwrap();
        revealed_under_paint(&doc, 0, block.invisible, block.rect, &ops, at)
    };
    let on_top = |content: &str| on_top_of(image_page_with(content));
    let scan = "q 200 0 0 100 100 100 cm /Im0 Do Q";
    assert!(on_top(&format!("BT /F1 12 Tf 3 Tr 110 150 Td (Under) Tj ET {scan}")), "the scan covers it");
    assert!(!on_top(&format!("BT /F1 12 Tf 3 Tr 400 600 Td (Away) Tj ET {scan}")), "an image elsewhere");
    assert!(!on_top(&format!("{scan} BT /F1 12 Tf 3 Tr 110 150 Td (Over) Tj ET")), "the scan is drawn first");
    assert!(!on_top(&format!("BT /F1 12 Tf 0 Tr 110 150 Td (Shown) Tj ET {scan}")), "visible text stays where it is");
    assert!(!on_top("BT /F1 12 Tf 3 Tr 110 150 Td (Path) Tj ET 0 0 600 800 re f"), "a filled path does not move it");
    assert!(on_top("BT /F1 12 Tf 3 Tr 400 600 Td (Shade) Tj ET /Sh sh"), "a shading after it");
    assert!(on_top("BT /F1 12 Tf 3 Tr 400 600 Td (Inline) Tj ET BI /W 1 /H 1 /CS /G /BPC 8 ID x EI"), "an inline image after it");
    let under = "BT /F1 12 Tf 3 Tr 110 150 Td (Under) Tj ET";
    let elsewhere = "q 50 0 0 50 400 600 cm /Im0 Do Q";
    assert!(!on_top(&format!("{under} q 0 0 10 10 re W n {scan} Q")), "the scan is clipped away entirely");
    assert!(on_top(&format!("{under} {elsewhere} {scan}")), "two images, the second over it");
    assert!(on_top(&format!("{under} {scan} {elsewhere}")), "two images, the first over it");
    assert!(!on_top(&format!("{under} {elsewhere} {elsewhere}")), "two images, neither over it");
    let in_form = |content: &str| {
        let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm0 7 0 R >> >> >>";
        let image = stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8", "A");
        let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Im0 6 0 R >> >>", "/Im0 Do");
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
            page.into(),
            stream("", content),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
            image,
            form,
        ];
        on_top_of(build(&objs.iter().map(String::as_str).collect::<Vec<_>>()))
    };
    assert!(in_form(&format!("{under} q 200 0 0 100 100 100 cm /Fm0 Do Q")), "a form holding the scan, over it");
    assert!(!in_form(&format!("{under} q 50 0 0 50 400 600 cm /Fm0 Do Q")), "a form holding an image elsewhere");
}

/// Revealed OCR text is checked where the rewrite lands, not only where it was: an image drawn
/// after it over just the moved or grown text still lifts the text on top, on both rewrite paths.
#[test]
fn an_ocr_rewrite_goes_on_top_of_paint_over_where_it_lands() {
    let scan = "q 200 0 0 100 100 100 cm /Im0 Do Q";
    let moved = text::BlockStyle { offset: Some([-290.0, -450.0]), ..Default::default() };
    let sized = text::BlockStyle { size: Some(24.0), ..Default::default() };
    let spaced = text::BlockStyle { line_spacing: Some(3.0), ..Default::default() };
    let range = text::RangeStyle {
        range: 0..2,
        family: None,
        resource: None,
        size: Some(120.0),
        color: None,
        underline: None,
        char_spacing: None,
        scale: None,
    };
    let enlarged = text::BlockStyle { ranges: vec![range], ..Default::default() };
    let above = "q 200 0 0 60 100 200 cm /Im0 Do Q";
    let cases = [
        ("BT /F1 12 Tf 3 Tr 400 600 Td (Away) Tj ET", "Moved", &moved),
        ("BT /F1 12 Tf 3 Tr 400 600 Td (Away) Tj ET", "Moved\nTwice", &moved),
        ("BT /F1 12 Tf 3 Tr 40 150 Td (Hi) Tj ET", "Hello there, this line now runs well past the scan's left edge", &text::BlockStyle::default()),
        ("BT /F1 12 Tf 3 Tr 40 150 Td (Hi) Tj ET", "Hello there,\nthis line now runs well past the scan's left edge", &text::BlockStyle::default()),
        ("BT /F1 12 Tf 3 Tr 110 230 Td (Above) Tj ET", "One\nTwo\nThree\nFour\nFive\nSix", &text::BlockStyle::default()),
    ];
    let low = "q 200 0 0 30 100 300 cm /Im0 Do Q";
    let cases = cases.into_iter().map(|(c, e, s)| (c, e, s, scan)).chain([
        ("BT /F1 12 Tf 3 Tr 110 500 Td (Above) Tj ET", "One\nTwo\nThree\nFour\nFive\nSix", &sized, low),
        ("BT /F1 12 Tf 3 Tr 110 500 Td (Above) Tj ET", "One\nTwo\nThree\nFour\nFive\nSix", &spaced, low),
        ("BT /F1 12 Tf 3 Tr 110 150 Td (Hi there) Tj ET", "Hi there", &enlarged, above),
    ]);
    for (content, edit, style, scan) in cases {
        let mut doc = image_page_with(&format!("{content} {scan}"));
        let before = text::text_blocks(&doc, 0).unwrap().remove(0);
        let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
        let at = ops.iter().position(|o| o.is("BT")).unwrap();
        assert!(!revealed_under_paint(&doc, 0, true, before.rect, &ops, at), "{edit:?}: the old box alone is clear of the scan");
        text::rewrite_block(&mut doc, 0, 0, Some(edit), style).unwrap();
        let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
        let scan_at = ops.iter().position(|o| o.is("Do")).unwrap();
        let shows = ["Tj", "TJ", "'", "\""];
        assert!(ops.iter().skip(scan_at).any(|o| shows.iter().any(|s| o.is(s))), "{edit:?}: the rewrite is drawn over the scan");
    }
}
