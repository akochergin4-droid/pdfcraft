//! Unit tests of the editing operations (part 7 of 7).

#[allow(unused_imports)]
use super::*;

/// Spaces after a row's last word are kept but take no part in its alignment: a justified row
/// still reaches the margin, and a centred or right-aligned one sits where it would without them.
#[test]
fn spaces_after_a_rows_last_word_do_not_move_its_alignment() {
    let shown = |text: &str, align: added::Align| {
        let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
        let style =
            text::BlockStyle { family: Some((added::Family::Helvetica, false, false)), align: Some(align), width: Some(300.0), ..Default::default() };
        text::rewrite_block(&mut doc, 0, 0, Some(text), &style).unwrap();
        String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned()
    };
    for (plain, trailing, align) in [
        ("aaa bbb ccc\nddd eee", "aaa bbb ccc  \nddd eee", added::Align::Justify),
        ("Title", "Title   ", added::Align::Center),
        ("Title", "Title   ", added::Align::Right),
    ] {
        let first = trailing.split('\n').next().unwrap();
        let expected = shown(plain, align);
        let actual = shown(trailing, align).replace(&format!("({first})"), &format!("({})", first.trim_end()));
        assert_eq!(actual, expected, "{trailing:?} {align:?}");
    }
}

/// A justified row runs from the paragraph's left edge to the margin. Word spacing reaches only
/// the spaces between its words: the spaces before its first word and after its last are left
/// out of a stretched row (as before #4199), so the reopened rows stay inside the paragraph and
/// can be edited again in place.
#[test]
fn a_justified_row_keeps_its_indent_and_ends_at_the_margin() {
    let style = text::BlockStyle {
        family: Some((added::Family::Helvetica, false, false)),
        align: Some(added::Align::Justify),
        width: Some(300.0),
        ..Default::default()
    };
    for typed in ["    Pack my box with five dozen jugs\nof liquor", "    Pack my box with five dozen jugs   \nof liquor"] {
        let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
        text::rewrite_block(&mut doc, 0, 0, Some(typed), &style).unwrap();
        let mut doc = reopen(&doc);
        let lines = text::text_lines(&doc, 0).unwrap();
        let first = lines.iter().find(|l| l.text.contains("Pack")).unwrap();
        assert_eq!(first.text, "Pack my box with five dozen jugs", "{typed:?}");
        assert!((first.rect[0] - 72.0).abs() < 0.5, "{typed:?}: {:?}", first.rect);
        assert!(lines.iter().all(|l| l.rect[0] >= 72.0 - 0.5 && l.rect[2] <= 372.5), "{typed:?}: {lines:?}");
        let blocks = text::text_blocks(&doc, 0).unwrap();
        assert_eq!(blocks.len(), 1, "{typed:?}: {blocks:?}");
        text::replace_block(&mut doc, 0, 0, "Edited again").unwrap();
        let again = text::text_lines(&reopen(&doc), 0).unwrap();
        assert!(again.iter().all(|l| l.rect[0] >= 0.0 && l.rect[2] <= 600.0), "{typed:?}: {again:?}");
    }
}

/// A justified paragraph whose first row was typed with an indent is edited again through the
/// styled path (a draft with a line break): its rows keep their scaling and stay one paragraph
/// inside its width.
#[test]
fn a_justified_paragraph_with_an_indent_edits_again_at_its_own_scale() {
    let style = text::BlockStyle {
        family: Some((added::Family::Helvetica, false, false)),
        align: Some(added::Align::Justify),
        width: Some(300.0),
        ..Default::default()
    };
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
    text::rewrite_block(&mut doc, 0, 0, Some("    aaa bbb ccc  \nddd"), &style).unwrap();
    let mut doc = reopen(&doc);
    let draft = text::text_blocks(&doc, 0).unwrap()[0].editor_text.replacen("ccc", "cc", 1);
    assert!(draft.contains('\n'), "{draft:?}");
    text::replace_block(&mut doc, 0, 0, &draft).unwrap();
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    let scales: Vec<&str> = content.lines().filter(|l| l.ends_with(" Tz")).collect();
    assert!(scales.iter().all(|l| *l == "100 Tz"), "{scales:?}: {content}");
    let doc = reopen(&doc);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!(lines.iter().all(|l| l.rect[0] >= 72.0 - 0.5 && l.rect[2] <= 372.5), "{lines:?}");
}

/// A paragraph drawn at `0 Tz` has no width to justify into: nothing is divided by it. The
/// writer writes a number that isn't finite as 0, so the test looks for the word spacing and
/// the move a division by zero would add, rather than for `NaN`.
#[test]
fn justifying_text_at_zero_horizontal_scaling_divides_by_nothing() {
    let mut doc = text_page("BT /F1 12 Tf 0 Tz 72 700 Td (Hello world) Tj ET");
    let style = text::BlockStyle { family: Some((added::Family::Helvetica, false, false)), align: Some(added::Align::Justify), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some("    aaa bbb\nccc"), &style).unwrap();
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    let block = content.split("ET").next().unwrap();
    assert_eq!(block.matches(" Tw").count(), 1, "only the paragraph's own 0 Tw: {content}");
    let first_row = block.split(" Tm").nth(1).and_then(|rest| rest.split(" Tj").next()).unwrap();
    assert!(!first_row.contains(" Td"), "no move before the first row: {content}");
}

/// The preview's box is the box of the rows Apply writes, measured to their last glyphs.
#[test]
fn a_drafts_box_is_the_box_apply_draws() {
    let doc = text_page("BT /F1 12 Tf 14 TL 72 700 Td [(First) -2000 (row here )] TJ T* (Second row ) Tj ET");
    let source = text::text_blocks(&doc, 0).unwrap().swap_remove(0);
    let draft = source.editor_text.replacen("Second", "Third", 1);
    let layout = text::text_edit_layout(&doc, 0, 0, &draft, &text::BlockStyle::default()).unwrap();
    let mut applied = doc.clone();
    text::replace_block(&mut applied, 0, 0, &draft).unwrap();
    let boxed = text::text_lines(&applied, 0)
        .unwrap()
        .into_iter()
        .map(|l| {
            let end = l.ink.iter().map(|r| r[2]).fold(f64::NEG_INFINITY, f64::max);
            [l.rect[0], l.rect[1], if end.is_finite() { end.min(l.rect[2]) } else { l.rect[2] }, l.rect[3]]
        })
        .reduce(|a, r| [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]);
    let (d, b) = (layout.draft_rect.unwrap(), boxed.unwrap());
    assert!(d.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01), "{draft:?}: preview {d:?}, applied {b:?}");
}

/// The draft's box is measured to its last glyph, not over the spaces typed after it.
#[test]
fn a_drafts_box_leaves_out_spaces_after_the_last_word() {
    let doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
    let layout = |text: &str| text::text_edit_layout(&doc, 0, 0, text, &text::BlockStyle::default()).unwrap().draft_rect;
    assert_eq!(layout("Hello there     "), layout("Hello there"));
}

/// A lone carriage return (an old Mac line ending) breaks the row, as `\n` and `\r\n` do.
#[test]
fn a_lone_carriage_return_breaks_an_edited_paragraph() {
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
    text::replace_block(&mut doc, 0, 0, "one\rtwo").unwrap();
    let lines: Vec<String> = text::text_lines(&reopen(&doc), 0).unwrap().into_iter().map(|l| l.text).collect();
    assert_eq!(lines, ["one", "two"]);
}

/// On the styled rewrite path too, `\r\n` is one row break and a lone `\r` another, and no
/// carriage return reaches the page: with no other formatting, and with a part set in red.
#[test]
fn carriage_returns_break_rows_on_the_styled_path() {
    let draft = "one\r\ntwo\rthree";
    let red = text::RangeStyle {
        range: 0..3,
        family: None,
        resource: None,
        size: None,
        color: Some([1.0, 0.0, 0.0]),
        underline: None,
        char_spacing: None,
        scale: None,
    };
    for style in [text::BlockStyle::default(), text::BlockStyle { ranges: vec![red], ..Default::default() }] {
        let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
        text::rewrite_block(&mut doc, 0, 0, Some(draft), &style).unwrap();
        let doc = reopen(&doc);
        let content = page_content_bytes(&doc, 0);
        assert!(!content.contains(&b'\r') && !content.windows(2).any(|w| w == b"\\r"), "{}", String::from_utf8_lossy(&content));
        let lines: Vec<String> = text::text_lines(&doc, 0).unwrap().into_iter().map(|l| l.text).collect();
        assert_eq!(lines, ["one", "two", "three"], "{style:?}");
    }
}

/// A subset with no space glyph shows each space as a positioning gap one space wide, so a run
/// of spaces still takes a run's width (#4199).
#[test]
fn a_run_of_spaces_in_a_subset_without_a_space_glyph_keeps_its_width() {
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj /F1 12 Tf ( ) Tj /F2 12 Tf (ba) Tj ET");
    assert_eq!(text::replace_block(&mut doc, 0, 0, "ab  ba   ab").unwrap().substituted, None);
    let content = String::from_utf8_lossy(&page_content_bytes(&reopen(&doc), 0)).into_owned();
    assert_eq!(content.matches("[-330] TJ").count(), 5, "{content}");
}

#[test]
fn image_exclusive_scope_resets_path_on_scope_exit() {
    // An earlier scope has a path operation closed by Q (with 'h' closepath).
    // The subsequent image in its own scope must cleanly extract and allow crop removal.
    let content = "q 0 0 m 10 10 l h Q q 200 0 0 100 100 100 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    crop(&mut doc, 0, 0, [100.0, 100.0, 250.0, 180.0]).unwrap();
    let before = images::page_images(&doc, 0).unwrap();
    assert_eq!(before.len(), 1);
    remove_crop(&mut doc, 0, 0).unwrap();
}

#[test]
fn content_units_does_not_swallow_subsequent_units_on_standalone_gs() {
    // /GS0 gs followed by Q and subsequent images must isolate each Do unit properly
    let content = "q /GS0 gs Q q 200 0 0 100 100 100 cm /Im0 Do Q q 10 0 0 10 300 300 cm /Im0 Do Q";
    let pieces = pdfcraft_content::Pieces::join(&[content.as_bytes()]);
    let ops = pieces.parse().ops;
    let units = images::content_units(&ops, 0, ops.len());
    assert_eq!(units.len(), 2);
}

#[test]
fn content_units_keep_each_top_level_inline_image_to_itself() {
    // The parser emits a whole inline image as one `BI` op; its unit must not run on to the end
    // (#3146). Inline images at the start, side by side and at the end are one unit each.
    let content = format!(
        "{} q 200 0 0 100 100 100 cm /Im0 Do Q {} {} q 10 0 0 10 300 300 cm /Im0 Do Q {}",
        inline_image('a'),
        inline_image('b'),
        inline_image('c'),
        inline_image('d')
    );
    let ops = pdfcraft_content::parse(content.as_bytes()).ops;
    let units = images::content_units(&ops, 0, ops.len());
    assert_eq!(units, [0..1, 1..5, 5..6, 6..7, 7..11, 11..12]);
}

#[test]
fn content_units_keep_a_nested_inline_image_inside_its_enclosing_unit() {
    // A `BI` inside `q`/`Q` or a marked-content sequence stays part of that sequence's unit.
    let content = format!("q {} Q /Span << /MCID 0 >> BDC {} EMC {}", inline_image('a'), inline_image('b'), inline_image('c'));
    let ops = pdfcraft_content::parse(content.as_bytes()).ops;
    let units = images::content_units(&ops, 0, ops.len());
    assert_eq!(units, [0..3, 3..6, 6..7]);
}

#[test]
fn send_backward_after_a_top_level_inline_image_moves_one_step() {
    // Before #3146 the inline image's unit swallowed everything after it, so SendBackward on a later
    // image acted like SendToBack: it must step behind the text only.
    let content = format!(
        "{} q 100 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Middle Text) Tj ET q 150 0 0 150 200 200 cm /Im0 Do Q",
        inline_image('x')
    );
    let mut doc = image_page_with(&content);
    let front = image_at(&doc, 200.0, 200.0);
    arrange(&mut doc, 0, front, images::ArrangeOrder::SendBackward).unwrap();
    let doc = reopen(&doc);
    assert_eq!(painted(&doc), ["BI x", "Do 100,100", "Do 200,200", "Tj"]);
}

#[test]
fn arrange_steps_an_image_past_one_inline_image_at_a_time() {
    // Before #3146 BringForward on B, inside the first inline image's unit, put it in front of
    // everything. One step forward is past the next inline image only, and one step back again
    // is behind that inline image only.
    let content = format!(
        "q 100 0 0 100 100 100 cm /Im0 Do Q {} q 150 0 0 150 200 200 cm /Im0 Do Q {} BT /F1 12 Tf 72 700 Td (Text) Tj ET q 50 0 0 50 300 300 cm /Im0 Do Q",
        inline_image('a'),
        inline_image('b')
    );
    let mut doc = image_page_with(&content);
    let b = image_at(&doc, 200.0, 200.0);
    arrange(&mut doc, 0, b, images::ArrangeOrder::BringForward).unwrap();
    let mut doc = reopen(&doc);
    assert_eq!(painted(&doc), ["Do 100,100", "BI a", "BI b", "Do 200,200", "Tj", "Do 300,300"]);
    let b = image_at(&doc, 200.0, 200.0);
    arrange(&mut doc, 0, b, images::ArrangeOrder::SendBackward).unwrap();
    let doc = reopen(&doc);
    assert_eq!(painted(&doc), ["Do 100,100", "BI a", "Do 200,200", "BI b", "Tj", "Do 300,300"]);
}

#[test]
fn arrange_around_inline_images_at_the_ends_of_the_stream() {
    // Before #3146 BringToFront on the last image after an inline image "succeeded" without moving
    // it. It is already in front, so it is refused and nothing changes.
    let content =
        format!("{} q 100 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Text) Tj ET q 150 0 0 150 200 200 cm /Im0 Do Q", inline_image('a'));
    let mut doc = image_page_with(&content);
    let before = page_content_bytes(&doc, 0);
    let front = image_at(&doc, 200.0, 200.0);
    assert!(arrange(&mut doc, 0, front, images::ArrangeOrder::BringToFront).is_err());
    assert!(arrange(&mut doc, 0, front, images::ArrangeOrder::BringForward).is_err());
    assert_eq!(page_content_bytes(&doc, 0), before);
    // Send to back goes behind the inline image that starts the stream.
    arrange(&mut doc, 0, front, images::ArrangeOrder::SendToBack).unwrap();
    let doc = reopen(&doc);
    assert_eq!(painted(&doc), ["Do 200,200", "BI a", "Do 100,100", "Tj"]);

    // Bring to front goes past the inline image that ends the stream.
    let content = format!("q 100 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Text) Tj ET {}", inline_image('z'));
    let mut doc = image_page_with(&content);
    let back = image_at(&doc, 100.0, 100.0);
    arrange(&mut doc, 0, back, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    assert_eq!(painted(&doc), ["Tj", "BI z", "Do 100,100"]);
}

#[test]
fn different_inline_soft_masks_have_distinct_identities() {
    let doc = Document::new_empty();
    let mut dict1 = Dict::default();
    let mut mask1 = Dict::default();
    mask1.set(b"Type".to_vec(), Object::Name(b"Mask1".to_vec()));
    dict1.set(b"SMask".to_vec(), Object::Dict(mask1));

    let mut dict2 = Dict::default();
    let mut mask2 = Dict::default();
    mask2.set(b"Type".to_vec(), Object::Name(b"Mask2".to_vec()));
    dict2.set(b"SMask".to_vec(), Object::Dict(mask2));

    let mut state1 = images::EffectiveGState::default();
    state1.merge_dict(&dict1, &doc, pdfcraft_content::Matrix::IDENTITY);

    let mut state2 = images::EffectiveGState::default();
    state2.merge_dict(&dict2, &doc, pdfcraft_content::Matrix::IDENTITY);

    assert!(state1.visually_differs(&state2), "two different inline soft masks must visually differ");
}

#[test]
fn is_entire_page_wrapped_with_nested_scopes() {
    let wrapped = "q 1 0 0 1 0 0 cm q 0 0 10 10 re f Q 200 0 0 100 100 100 cm /Im0 Do Q";
    let pieces = pdfcraft_content::Pieces::join(&[wrapped.as_bytes()]);
    let ops = pieces.parse().ops;
    assert!(images::is_entire_page_wrapped(&ops), "page wrapped in outer q...Q with inner nested q...Q is wrapped");

    let unwrapped = "q 1 0 0 1 0 0 cm Q q 200 0 0 100 100 100 cm /Im0 Do Q";
    let pieces2 = pdfcraft_content::Pieces::join(&[unwrapped.as_bytes()]);
    let ops2 = pieces2.parse().ops;
    assert!(!images::is_entire_page_wrapped(&ops2), "page with two separate top-level q...Q scopes is not wrapped");
}

#[test]
fn hostile_page_with_many_gs_and_nested_q_does_not_hang_or_oom() {
    use std::fmt::Write as _;
    let mut stream = String::new();
    for i in 0..10_000 {
        let _ = write!(stream, "/G{i} gs ");
    }
    for _ in 0..500 {
        stream.push_str("q ");
    }
    stream.push_str("100 0 0 100 0 0 cm /Im0 Do ");
    for _ in 0..500 {
        stream.push_str("Q ");
    }
    let doc = image_page_with(&stream);
    let start = std::time::Instant::now();
    let imgs = images::page_images(&doc, 0).unwrap();
    assert_eq!(imgs.len(), 1);
    assert!(start.elapsed().as_secs() < 5, "page_images on hostile 10k gs must finish in under 5s, took {:?}", start.elapsed());
}

#[test]
fn crop_image_with_leading_cm_in_scope_places_clip_before_internal_cm() {
    let content = "q 10 0 0 10 50 50 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    let before = images::page_images(&doc, 0).unwrap();
    assert_eq!(before.len(), 1);
    crop(&mut doc, 0, 0, [50.0, 50.0, 55.0, 55.0]).unwrap();
    let doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 1);
    assert!(after[0].visible_rect.is_some());
    let (v0, v1, v2, v3) =
        (after[0].visible_rect.unwrap()[0], after[0].visible_rect.unwrap()[1], after[0].visible_rect.unwrap()[2], after[0].visible_rect.unwrap()[3]);
    assert!((v0 - 50.0).abs() < 1e-3 && (v2 - 55.0).abs() < 1e-3, "visible rect must match crop coordinates: {:?}", after[0].visible_rect);
    assert!((v1 - 50.0).abs() < 1e-3 && (v3 - 55.0).abs() < 1e-3, "visible rect must match crop coordinates: {:?}", after[0].visible_rect);
}

#[test]
fn remove_crop_preserves_cm_before_extgstate() {
    let content = "q 2 0 0 2 0 0 cm /GS0 gs 0 0 100 100 re W n 10 0 0 10 50 50 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    let before = images::page_images(&doc, 0).unwrap();
    assert_eq!(before.len(), 1);
    crop(&mut doc, 0, 0, [100.0, 100.0, 110.0, 110.0]).unwrap();
    remove_crop(&mut doc, 0, 0).unwrap();
    let doc = reopen(&doc);
    let bytes = page_content_bytes(&doc, 0);
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("W n"), "the author clipping path must remain: {s}");
    assert!(!s.contains("PdfCraftImageCrop"), "the editor crop must be removed: {s}");
    // Verify that the initial cm appears before /GS0 gs
    let cm_pos = s.find("2 0 0 2 0 0 cm").expect("cm must be preserved");
    let gs_pos = s.find("/GS0 gs").expect("gs must be preserved");
    assert!(cm_pos < gs_pos, "cm must precede /GS0 gs so soft mask space is preserved: {s}");
}

/// Inside a text object `q`/`Q` save and restore the text and text line matrices (ISO 32000-2
/// §9.4.1, as corrected by the PDF Association's errata, issue 368), as the renderer does: the
/// text after `Q` is found, and stays, where it is drawn.
#[test]
fn q_inside_a_text_object_restores_the_text_matrices() {
    for (content, text, baseline) in [
        ("BT /F1 12 Tf 72 700 Td q 0 -100 Td Q (After) Tj ET", "After", [72.0, 700.0]),
        ("BT /F1 12 Tf 72 700 Td q 1 0 0 1 300 300 Tm Q (After) Tj ET", "After", [72.0, 700.0]),
        // `T*` moves from the restored line, not the one set inside q/Q.
        ("BT /F1 12 Tf 14 TL 72 700 Td q 0 -100 Td Q T* (Next) Tj ET", "Next", [72.0, 686.0]),
    ] {
        let mut doc = text_page(content);
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!((lines[0].text.as_str(), lines[0].baseline()), (text, baseline), "{content}");
        text::replace_line(&mut doc, 0, 0, "Edited").unwrap();
        let doc = reopen(&doc);
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!((lines[0].text.as_str(), lines[0].baseline()), ("Edited", baseline), "{content} after editing");
    }
}

/// Whole-paragraph alignment retains both soft wraps and typed hard breaks after save/reopen.
/// Selected-row alignment for existing PDF content is withheld until it can persist safely.
#[test]
fn wrapped_rows_keep_whole_paragraph_alignment_and_hard_break_markers() {
    let mut doc = text_page("BT /F1 10 Tf 12 TL 72 700 Td (Old) Tj ET");
    let style = text::BlockStyle { width: Some(60.0), align: Some(added::Align::Left), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some("One two three four five six\nTail"), &style).unwrap();
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!(lines.len() > 2, "the first logical row must wrap");
    let tail = lines.iter().find(|line| line.text == "Tail").unwrap();
    assert!((tail.rect[0] - 72.0).abs() < 1.0, "the second logical row keeps the paragraph alignment: {:?}", tail.rect);
    for line in lines.iter().filter(|line| line.text != "Tail") {
        assert!((line.rect[0] - 72.0).abs() < 1.0, "soft-wrapped rows stay left-aligned: {:?}", line.rect);
    }
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(content.contains("/PCSoft") && content.contains("/PCHard"), "both break kinds survive: {content}");
}

#[test]
fn hostile_soft_wrap_counts_fail_before_reflow_allocates() {
    for marker in [
        "/PCSoft << /N 9223372036854775807 >> BDC EMC",
        "/PCSoft << /N 16777217 >> BDC EMC",
        "/PCSoft << /N 8388608 >> BDC EMC /PCSoft << /N 8388609 >> BDC EMC",
    ] {
        let doc = text_page(&format!("BT /F1 12 Tf 20 700 Td (First) Tj {marker} 0 -14 Td (Second) Tj ET"));
        let error = text::text_lines(&doc, 0).unwrap_err();
        assert!(error.to_string().contains("too large"), "{marker}: {error}");
    }
}

#[test]
fn ordinary_soft_wrap_counts_keep_their_spaces() {
    for gap in [0, 1, 2, 8] {
        let doc = text_page(&format!("BT /F1 12 Tf 20 700 Td (First) Tj /PCSoft << /N {gap} >> BDC EMC 0 -14 Td (Second) Tj ET"));
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!(lines.len(), 2);
        let blocks = text::text_blocks(&doc, 0).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].reflow_text, format!("First{}Second", " ".repeat(gap)));
    }
}

#[test]
fn styled_existing_justification_is_refused_instead_of_silently_left_aligning() {
    let mut doc = mixed_region_page();
    let before = page_content_bytes(&doc, 0);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let style = text::BlockStyle { align: Some(added::Align::Justify), ..Default::default() };
    let error = text::rewrite_block(&mut doc, 0, 1, Some(&blocks[1].editor_text), &style).unwrap_err();
    assert!(error.to_string().contains("justification"));
    assert_eq!(page_content_bytes(&doc, 0), before);
    assert_eq!(page_content_bytes(&reopen(&doc), 0), before);
}

/// The OCR stamp path registers the plan's Type3 faces (a free `PCOcr…` name per stamp, so a
/// page that is OCR'd twice keeps both layers' fonts), and the stamped codes extract back
/// through the font's ToUnicode CMap.
#[test]
fn stamp_with_fonts_registers_the_planned_type3_faces() {
    if without_craft_fonts("stamp_with_fonts_registers_the_planned_type3_faces") {
        return;
    }
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let mut plan = pdfcraft_fonts::TextPlan::for_words(["日本"]);
    assert_eq!(plan.missing(), 0);
    stamp_with_fonts(&mut doc, 0, "OCR", &mut plan, |p| {
        let codes: Vec<u8> = p.encoding(0).iter().map(|&(_, c)| c).collect();
        let mut c = format!("BT 3 Tr /PCHelv 1 Tf\n/{} 1 Tf 100 700 Td ", p.fonts[0].name).into_bytes();
        c.extend_from_slice(&pdfcraft_fonts::literal(&codes));
        c.extend_from_slice(b" Tj ET");
        c
    })
    .unwrap();
    assert!(plan.fonts[0].name.starts_with("PCOcr"), "{:?}", plan.fonts[0].name);
    // The registered font carries a ToUnicode CMap; the engine's own reader decodes the run.
    let reopened = Document::open(Arc::new(write_incremental(&doc, &SaveOptions::default()).unwrap())).unwrap();
    let lines = text::text_lines(&reopened, 0).unwrap();
    assert!(lines.iter().any(|l| l.text.contains("日本")), "{lines:?}");
    // A second stamp on the same page takes the next free name; the first layer's font stands.
    let mut second = pdfcraft_fonts::TextPlan::for_words(["語"]);
    stamp_with_fonts(&mut doc, 0, "OCR", &mut second, |p| format!("BT 3 Tr /{} 1 Tf 100 600 Td (\u{1}) Tj ET", p.fonts[0].name).into_bytes())
        .unwrap();
    assert_eq!(second.fonts[0].name, format!("{}1", plan.fonts[0].name));
}

/// A Type3 face built for page 1 is reused on page 2, but page 2 already has a font under the
/// name page 1 gave it: page 2's own font keeps its name, and the shared face is registered on
/// page 2 under a free one that the plan carries for page 2's content. (The plan is written out
/// by hand, so this runs without the craft-fonts build input.)
#[test]
fn a_reused_type3_face_never_replaces_a_later_page_s_font_of_the_same_name() {
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 5 0 R >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 6 0 R /Resources << /Font << /PCOcr 7 0 R >> >> >>",
        &stream("", ""),
        &stream("", "BT /PCOcr 12 Tf 72 700 Td (own) Tj ET"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    ]);
    let glyph = pdfcraft_fonts::GlyphOutline { contours: vec![vec![[0.1, 0.0], [0.9, 0.0], [0.5, 0.8]]], width: 1.0, bbox: [0.1, 0.0, 0.9, 0.8] };
    let mut plan = pdfcraft_fonts::TextPlan::default();
    plan.fonts.push(pdfcraft_fonts::PlannedType3 { family: "Test", glyphs: vec![('日', glyph)], name: String::new(), object: None });
    let draw = |p: &pdfcraft_fonts::TextPlan| format!("BT 3 Tr /{} 1 Tf 100 600 Td (\u{1}) Tj ET", p.fonts[0].name).into_bytes();
    stamp_with_fonts(&mut doc, 0, "OCR", &mut plan, draw).unwrap();
    assert_eq!(plan.fonts[0].name, "PCOcr", "free on page 1");
    let face = plan.fonts[0].object.expect("built once");
    stamp_with_fonts(&mut doc, 1, "OCR", &mut plan, draw).unwrap();
    assert_ne!(plan.fonts[0].name, "PCOcr", "page 2 already has a PCOcr");
    let page2 = &pdfcraft_model::pages(&doc)[1];
    let res = doc.resolve(page2.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap());
    let fonts = fonts.as_dict().unwrap();
    assert_eq!(fonts.get(b"PCOcr").and_then(|o| o.as_ref()), Some(pdfcraft_cos::ObjRef::new(7, 0)), "page 2's own font keeps its name");
    assert_eq!(fonts.get(plan.fonts[0].name.as_bytes()).and_then(|o| o.as_ref()), Some(face), "the shared face, under a free name");
    assert_eq!(plan.fonts[0].object, Some(face), "still built once");
}

#[test]
fn planned_type3_charproc_has_valid_metrics_and_encloses_its_ink() {
    let glyph = pdfcraft_fonts::GlyphOutline {
        contours: vec![vec![[-0.1001, -0.2001], [0.9001, 1.1001]]],
        width: 0.8,
        bbox: [-0.1001, -0.2001, 0.9001, 1.1001],
    };
    let parsed = pdfcraft_content::parse(&text::glyph_path(&glyph));
    assert_eq!(parsed.skipped, 0);
    let metrics = &parsed.ops[0];
    assert!(metrics.is("d1"));
    assert_eq!(metrics.operands.len(), 6);
    assert_eq!((metrics.num(0), metrics.num(1)), (Some(800.0), Some(0.0)));
    assert_eq!((metrics.num(2), metrics.num(3), metrics.num(4), metrics.num(5)), (Some(-101.0), Some(-201.0), Some(901.0), Some(1101.0)));
}

#[test]
fn image_arrange_refuses_crossed_scope_boundaries_without_changes() {
    for content in ["/P <</MCID 0>> BDC q 10 0 0 10 0 0 cm /Im0 Do EMC Q /Im0 Do", "q /P <</MCID 0>> BDC /Im0 Do Q EMC /Im0 Do"] {
        let mut doc = image_page_with(content);
        let before = page_content_bytes(&doc, 0);
        assert!(arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).is_err(), "{content}");
        assert_eq!(page_content_bytes(&doc, 0), before);
    }
}

#[test]
fn removing_a_crop_preserves_author_clips_and_works_under_outer_clip() {
    for content in ["0 0 300 300 re W n q 10 0 0 10 50 50 cm /Im0 Do Q", "q 50 50 8 8 re W n 10 0 0 10 50 50 cm /Im0 Do Q"] {
        let mut doc = image_page_with(content);
        let original = images::page_images(&doc, 0).unwrap()[0].clone();
        crop(&mut doc, 0, 0, [52.0, 52.0, 55.0, 55.0]).unwrap();
        let mut doc = reopen(&doc);
        remove_crop(&mut doc, 0, 0).unwrap();
        let mut doc = reopen(&doc);
        let restored = images::page_images(&doc, 0).unwrap()[0].clone();
        assert_eq!(restored.rect, original.rect);
        assert_eq!(restored.visible_rect, original.visible_rect);
        assert_eq!(restored.contains([59.0, 59.0]), original.contains([59.0, 59.0]));
        assert!(remove_crop(&mut doc, 0, 0).is_err());
    }
}

#[test]
fn remove_crop_refuses_an_unmarked_author_clip_without_changes() {
    let mut doc = image_page_with("q 50 50 8 8 re W n 10 0 0 10 50 50 cm /Im0 Do Q");
    let before = page_content_bytes(&doc, 0);
    assert!(remove_crop(&mut doc, 0, 0).is_err());
    assert_eq!(page_content_bytes(&doc, 0), before);
}

#[test]
fn arrange_refuses_unrestorable_extended_print_state() {
    let mut unexpectedly_moved = Vec::new();
    for (key, value) in [
        ("TR", Object::name("Identity")),
        ("TR2", Object::name("Default")),
        ("OP", Object::Bool(true)),
        ("op", Object::Bool(true)),
        ("OPM", Object::Int(1)),
        ("BG", Object::name("Default")),
        ("BG2", Object::name("Default")),
        ("UCR", Object::name("Default")),
        ("UCR2", Object::name("Default")),
        ("HT", Object::name("Default")),
    ] {
        let mut doc = image_page_with("q 100 0 0 100 50 50 cm /Im0 Do Q /Print gs BT /F1 12 Tf 20 700 Td (Text) Tj ET");
        let page = pdfcraft_model::pages(&doc)[0].clone();
        let mut resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
        resources.set(
            b"ExtGState".to_vec(),
            Object::Dict(Dict::from_iter([(b"Print".to_vec(), Object::Dict(Dict::from_iter([(key.as_bytes().to_vec(), value)])))])),
        );
        doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
        let before = page_content_bytes(&doc, 0);
        match arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront) {
            Ok(()) => unexpectedly_moved.push(key),
            Err(error) => {
                assert!(error.to_string().contains("graphics state"), "{key}: {error}");
                assert_eq!(page_content_bytes(&doc, 0), before, "{key} refusal changes nothing");
            }
        }
    }
    assert!(unexpectedly_moved.is_empty(), "unrestorable destination state was accepted: {unexpectedly_moved:?}");
}

#[test]
fn arrange_keeps_implicit_device_spaces_before_sc_and_scn() {
    for (select, color) in [("0 g", "0.5 sc"), ("0 0 0 rg", "1 0 0 sc"), ("0 0 0 1 k", "0 1 1 0 scn")] {
        for stroke in [false, true] {
            let (select, color) = if stroke { (select.to_uppercase(), color.to_uppercase()) } else { (select.into(), color.into()) };
            for content in [
                format!("q {select} {color} q 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 20 700 Td (Text) Tj ET"),
                format!("q {select} q {color} 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 20 700 Td (Text) Tj ET"),
            ] {
                let mut doc = stencil_page_with(&content);
                arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
                let doc = reopen(&doc);
                let bytes = page_content_bytes(&doc, 0);
                let ops = pdfcraft_content::parse(&bytes).ops;
                let do_index = ops.iter().rposition(|op| op.is("Do")).unwrap();
                let start = ops[..do_index].iter().rposition(|op| op.is("q")).unwrap();
                let relocated = &ops[start..do_index];
                let selector = select.split_whitespace().last().unwrap();
                let color_op = color.split_whitespace().last().unwrap();
                let space_index = relocated.iter().position(|op| op.is(selector));
                let color_index = relocated.iter().position(|op| op.is(color_op));
                assert!(
                    space_index.is_some() && space_index < color_index,
                    "{select} must select its device space before {color}: {}",
                    String::from_utf8_lossy(&bytes)
                );
            }
        }
    }
}

#[test]
fn arrange_restores_initial_rendering_intent_from_ri_and_gs() {
    for selection in ["/Saturation ri", "/Print gs"] {
        let mut doc = image_page_with(&format!("q 100 0 0 100 50 50 cm /Im0 Do Q {selection} BT /F1 12 Tf 20 700 Td (Text) Tj ET"));
        let page = pdfcraft_model::pages(&doc)[0].clone();
        let mut resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
        resources.set(
            b"ExtGState".to_vec(),
            Object::Dict(Dict::from_iter([(b"Print".to_vec(), Object::Dict(Dict::from_iter([(b"RI".to_vec(), Object::name("Saturation"))])))])),
        );
        doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
        arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
        let doc = reopen(&doc);
        let bytes = page_content_bytes(&doc, 0);
        assert!(
            String::from_utf8_lossy(&bytes).contains("/RelativeColorimetric ri"),
            "default rendering intent must be restored after {selection}: {}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

#[test]
fn editor_crop_moves_with_repeated_group_translation() {
    let mut doc = image_page();
    let caption_rect = text::text_lines(&doc, 0).unwrap()[0].rect;
    crop(&mut doc, 0, 0, [150.0, 120.0, 250.0, 170.0]).unwrap();
    let start = images::page_images(&doc, 0).unwrap()[0].clone();
    let mut count = 0;
    for iteration in 0..20 {
        images::translate_images(&mut doc, 0, &[0], [1.0, -2.0]).unwrap();
        let ops = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops;
        if iteration == 0 {
            count = ops.len();
        }
        assert_eq!(ops.len(), count, "repeated moves fold the same transform");
    }
    let shifted = |r: [f64; 4]| [r[0] + 20.0, r[1] - 40.0, r[2] + 20.0, r[3] - 40.0];
    let doc = reopen(&doc);
    let after = &images::page_images(&doc, 0).unwrap()[0];
    assert!(close(after.rect, shifted(start.rect)));
    assert!(
        close(after.visible_rect.unwrap(), shifted(start.visible_rect.unwrap())),
        "the editor crop moves with its pixels: {:?}",
        after.visible_rect
    );
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "Caption");
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].rect, caption_rect);
}

#[test]
fn arranging_original_image_keeps_it_independent_of_added_items() {
    for update in [false, true] {
        let mut doc = image_page();
        let caption_rect = text::text_lines(&doc, 0).unwrap()[0].rect;
        add_content(
            &mut doc,
            0,
            &Content::Text(AddedText { rect: [20.0, 250.0, 250.0, 290.0], text: "Added caption".into(), ..AddedText::default() }),
        )
        .unwrap();
        arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
        let mut doc = reopen(&doc);
        assert_eq!(list_added(&doc).len(), 1, "the added item stays editable");
        if update {
            update_content(
                &mut doc,
                0,
                0,
                &Content::Text(AddedText { rect: [20.0, 250.0, 250.0, 290.0], text: "Updated caption".into(), ..AddedText::default() }),
            )
            .unwrap();
        } else {
            delete_content(&mut doc, 0, 0).unwrap();
        }
        let doc = reopen(&doc);
        assert_eq!(images::page_images(&doc, 0).unwrap().len(), 1, "editing/deleting an added item must preserve arranged original artwork");
        assert_eq!(images::page_images(&doc, 0).unwrap()[0].rect, [100.0, 100.0, 300.0, 200.0]);
        let text = text::text_lines(&doc, 0).unwrap().into_iter().map(|line| line.text).collect::<Vec<_>>();
        assert!(text.iter().any(|line| line == "Caption"), "the original caption survives");
        assert_eq!(text.iter().any(|line| line == "Updated caption"), update);
        assert_eq!(text::text_lines(&doc, 0).unwrap().iter().find(|line| line.text == "Caption").unwrap().rect, caption_rect);
    }
}

#[test]
fn arranging_nested_marked_image_refuses_without_losing_tags() {
    for marked in ["/Figure <</MCID 3>> BDC", "/Figure BMC"] {
        let mut doc = image_page_with(&format!(
            "q {marked} 100 0 0 100 50 50 cm /Im0 Do EMC BT /F1 12 Tf 20 700 Td (Text) Tj ET Q BT /F1 12 Tf 20 720 Td (Outside) Tj ET"
        ));
        let before = page_content_bytes(&doc, 0);
        let error = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap_err();
        assert!(error.to_string().contains("marked-content"), "{error}");
        assert_eq!(page_content_bytes(&doc, 0), before);
        assert_eq!(page_content_bytes(&reopen(&doc), 0), before);
    }
}

#[test]
fn editor_crop_group_move_keeps_an_outer_author_clip_fixed() {
    let mut doc = image_page_with("0 0 220 300 re W n q 200 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 10 250 Td (Caption) Tj ET");
    crop(&mut doc, 0, 0, [150.0, 120.0, 210.0, 170.0]).unwrap();
    let mut operators = 0;
    for i in 0..20 {
        images::translate_images(&mut doc, 0, &[0], [1.0, -2.0]).unwrap();
        let count = pdfcraft_content::parse(&page_content_bytes(&doc, 0)).ops.len();
        if i == 0 {
            operators = count;
        }
        assert_eq!(count, operators, "moves under an inherited clip must fold their transform");
    }
    let doc = reopen(&doc);
    let image = &images::page_images(&doc, 0).unwrap()[0];
    assert!(close(image.rect, [120.0, 60.0, 320.0, 160.0]));
    assert!(
        close(image.visible_rect.unwrap(), [170.0, 80.0, 220.0, 130.0]),
        "the editor crop moves but its author's outer clip stays fixed: {:?}",
        image.visible_rect
    );
    let bytes = page_content_bytes(&doc, 0);
    assert!(String::from_utf8_lossy(&bytes).starts_with("0 0 220 300 re W n"));
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "Caption");
}

#[test]
fn review_arrange_form_keeps_inherited_text_spacing() {
    for (spacing, later_spacing) in [(0, false), (12, false), (0, true)] {
        let initial = if later_spacing { String::new() } else { format!("BT {spacing} Tc ET") };
        let later = if later_spacing { "BT 12 Tc ET" } else { "" };
        let content = format!("q {initial} q 1 0 0 1 20 20 cm /Fm0 Do Q Q {later} q 0 0 1 rg 200 200 5 5 re f Q");
        let mut doc = form_page_with(&content);
        let form = pdfcraft_cos::ObjRef::new(6, 0);
        let dict = doc.get(form).as_dict().unwrap().clone();
        doc.set(form, Object::Stream(pdfcraft_cos::Stream::from_raw(dict, b"BT /F1 20 Tf 0 15 Td (ABC) Tj ET".to_vec())));
        let doc_before = reopen(&doc);
        let before = review_state_raster(&doc_before);
        assert!(before.as_chunks::<4>().0.iter().any(|p| p[0] == 0 && p[1] == 0 && p[2] == 0), "the Form actually paints text");
        arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
        let saved = reopen(&doc);
        assert_eq!(images::page_images(&saved, 0).unwrap()[0].rect, images::page_images(&doc_before, 0).unwrap()[0].rect);
        assert_eq!(review_state_raster(&saved), before, "inherited {spacing} Tc must survive arranging the Form and reopening");
    }
}

#[test]
fn review_repeated_gs_keeps_alpha_vs_luminosity_mask_chronology() {
    for states in ["/A gs", "/A gs /B gs", "/A gs /B gs /A gs"] {
        let mut doc = review_shared_mask_page(states, true, "0.25 g 0 0 300 300 re f");
        let before = review_state_raster(&reopen(&doc));
        assert!(before.as_chunks::<4>().0.iter().any(|p| p[0] == p[1] && p[1] == p[2] && p[0] < 230), "the masked image is visible");
        arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
        assert_eq!(review_state_raster(&reopen(&doc)), before, "mask application order must survive: {states}");
    }
}

#[test]
fn review_repeated_gs_keeps_last_mask_establishment_ctm() {
    for states in [
        "1 0 0 1 50 0 cm /A gs 1 0 0 1 -50 0 cm",
        "/A gs 1 0 0 1 50 0 cm /B gs 1 0 0 1 -50 0 cm",
        "/A gs 1 0 0 1 50 0 cm /B gs 1 0 0 1 -50 0 cm /A gs",
    ] {
        let mut doc = review_shared_mask_page(states, false, "0.25 g 0 0 60 300 re f");
        let before = review_state_raster(&reopen(&doc));
        arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
        assert_eq!(review_state_raster(&reopen(&doc)), before, "mask CTM must survive: {states}");
    }
}

#[test]
fn review_soft_masks_sharing_a_group_keep_distinct_configurations() {
    let doc = Document::new_empty();
    let mask = |subtype: &str| {
        Dict::from_iter([(
            b"SMask".to_vec(),
            Object::Dict(Dict::from_iter([(b"S".to_vec(), Object::name(subtype)), (b"G".to_vec(), Object::Ref(pdfcraft_cos::ObjRef::new(8, 0)))])),
        )])
    };
    let mut alpha = images::EffectiveGState::default();
    let mut luminosity = images::EffectiveGState::default();
    alpha.merge_dict(&mask("Alpha"), &doc, pdfcraft_content::Matrix::IDENTITY);
    luminosity.merge_dict(&mask("Luminosity"), &doc, pdfcraft_content::Matrix::IDENTITY);
    assert!(alpha.visually_differs(&luminosity), "a shared group does not make Alpha and Luminosity the same mask");
    for (key, value) in [
        (b"BC".to_vec(), Object::Array(vec![Object::Real(0.5)])),
        (
            b"TR".to_vec(),
            Object::Dict(Dict::from_iter([
                (b"FunctionType".to_vec(), Object::Int(2)),
                (b"Domain".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(1)])),
                (b"N".to_vec(), Object::Int(2)),
            ])),
        ),
    ] {
        let mut changed = mask("Luminosity");
        let Object::Dict(definition) = changed.get_mut(b"SMask").unwrap() else { panic!("mask dictionary") };
        definition.set(key, value);
        let mut state = images::EffectiveGState::default();
        state.merge_dict(&changed, &doc, pdfcraft_content::Matrix::IDENTITY);
        assert!(state.visually_differs(&luminosity), "mask backdrop and transfer function belong to its definition");
    }
}

#[test]
fn review_inside_scope_group_move_matches_direct_transform_under_outer_scale() {
    for scale in [1, 2] {
        let content = format!("{scale} 0 0 {scale} 0 0 cm q 40 0 0 40 10 10 cm /Im0 Do Q q 0 0 1 rg 75 75 5 5 re f Q");
        let mut cropped = image_page_with(&content);
        let keep = if scale == 2 { [30.0, 30.0, 70.0, 70.0] } else { [15.0, 15.0, 35.0, 35.0] };
        crop(&mut cropped, 0, 0, keep).unwrap();
        arrange(&mut cropped, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
        for offset in [[10.0, 0.0], [0.0, 10.0], [-10.0, 3.0]] {
            let mut grouped = reopen(&cropped);
            let mut direct = reopen(&cropped);
            let before = images::page_images(&grouped, 0).unwrap()[0].clone();
            images::translate_images(&mut grouped, 0, &[0], offset).unwrap();
            images::change_image(&mut direct, 0, 0, &images::ImageChange::Transform([1.0, 0.0, 0.0, 1.0, offset[0], offset[1]])).unwrap();
            let moved = |r: [f64; 4]| [r[0] + offset[0], r[1] + offset[1], r[2] + offset[0], r[3] + offset[1]];
            let saved = reopen(&grouped);
            let image = &images::page_images(&saved, 0).unwrap()[0];
            assert!(close(image.rect, moved(before.rect)), "scale {scale} offset {offset:?}: {:?}", image.rect);
            assert!(close(image.visible_rect.unwrap(), moved(before.visible_rect.unwrap())), "the crop follows the complete displacement");
            assert_eq!(
                review_state_raster(&saved),
                review_state_raster(&reopen(&direct)),
                "group and direct transforms must agree, preserving other artwork"
            );
        }
    }
}
