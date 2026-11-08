//! Unit tests of the editing operations (part 6 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn new_marks_follow_clean_streams_ending_in_comments() {
    let mut doc = isolation_fixture(&[("Added", "q 50 Tz Q % no newline")]);
    stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    assert_eq!(probe_states(&doc).len(), 1, "the new mark must not be swallowed by a comment");
    assert!(probe_states(&doc)[0].is_empty());
}

#[test]
fn removing_marks_does_not_strip_arbitrary_wrap_tagged_content() {
    let mut doc = isolation_fixture(&[("Wrap", "q /PCProbe Do Q"), ("HeaderFooter", "q Q")]);
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap(), 1);
    assert_eq!(probe_states(&doc).len(), 1);
}

#[test]
fn removing_marks_strips_nested_trailing_wrapper_pairs() {
    let mut doc = isolation_fixture(&[
        ("Wrap", "q %PdfCraft\n"),
        ("Wrap", "q %PdfCraft\n"),
        ("", "2 0 0 2 0 0 cm"),
        ("Wrap", "Q %PdfCraft\n"),
        ("Wrap", "Q %PdfCraft\n"),
        ("HeaderFooter", "q Q"),
    ]);
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap(), 1);
    assert_eq!(streams(&doc, 0), ["2 0 0 2 0 0 cm"]);
}

#[test]
fn updating_marks_on_unreadable_content_does_not_nest_wrappers() {
    for data in ["q 50 Tz", "Q 50 Tz", "BT"] {
        let mut doc = isolation_fixture(&[("", data)]);
        let page = page_list(&doc).remove(0);
        for _ in 0..3 {
            place_tagged(&mut doc, &page, MarkKind::HeaderFooter.tag(), b"q /PCProbe Do Q".to_vec(), false).unwrap();
            assert_eq!(streams(&doc, 0).len(), 4, "{data}: {:?}", streams(&doc, 0));
            assert_eq!(remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap(), 1);
            assert_eq!(streams(&doc, 0), [data], "{data}: the original list comes back");
        }
    }
}

#[test]
fn removing_marks_strips_wrappers_that_closed_open_state() {
    let mut doc = isolation_fixture(&[("", "q 2 0 0 2 0 0 cm")]);
    stamp(&mut doc, 0, "Probe", b"q /PCProbe Do Q\n".to_vec()).unwrap();
    assert!(probe_states(&doc).last().unwrap().is_empty());
    // Now balanced, the page reads cleanly: the close (with its extra `Q`) still pairs.
    let page = page_list(&doc).remove(0);
    place_tagged(&mut doc, &page, MarkKind::Watermark.tag(), b"q /PCProbe Do Q\n".to_vec(), false).unwrap();
    assert_eq!(streams(&doc, 0).len(), 5, "no second wrapper: {:?}", streams(&doc, 0));
    let mut doc = isolation_fixture(&[("", "q 2 0 0 2 0 0 cm")]);
    let page = page_list(&doc).remove(0);
    place_tagged(&mut doc, &page, MarkKind::Watermark.tag(), b"q /PCProbe Do Q\n".to_vec(), false).unwrap();
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::Watermark).unwrap(), 1);
    assert_eq!(streams(&doc, 0), ["q 2 0 0 2 0 0 cm"]);
}

/// #3087: flattening comments leaves alone what isn't a comment: links, and the non-markup
/// multimedia and print-production annotations (3D, Screen, Movie, RichMedia, PrinterMark,
/// TrapNet, Watermark), which it used to bake into the page and delete with their data.
/// Flattening fields only leaves them alone too; real comments are still flattened.
#[test]
fn flattening_comments_keeps_annotations_that_are_not_comments() {
    let kept = ["Widget", "Link", "3D", "Screen", "Movie", "RichMedia", "PrinterMark", "TrapNet", "Watermark"];
    let flattened = ["Text", "Square", "FileAttachment"];
    let all: Vec<&str> = flattened.iter().chain(kept.iter()).copied().collect();
    let form = "<< /Type /XObject /Subtype /Form /BBox [0 0 40 20] /Length 26 >>\nstream\n0 0 1 rg 0 0 40 20 re f\nendstream";
    let annots: Vec<String> = all
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!("<< /Type /Annot /Subtype /{s} /Rect [{x} 700 {r} 720] /AP << /N 4 0 R >> /Contents ({s}) >>", x = 20 + 50 * i, r = 60 + 50 * i)
        })
        .collect();
    let refs: Vec<String> = (0..all.len()).map(|i| format!("{} 0 R", 6 + i)).collect();
    let page = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << >> /Contents 5 0 R /Annots [{}] >>", refs.join(" "));
    let content = stream("", " ");
    let mut objs: Vec<&str> = vec!["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", &page, form, &content];
    objs.extend(annots.iter().map(String::as_str));
    let left = |doc: &Document| -> Vec<String> {
        let p = &pdfcraft_model::pages(doc)[0];
        let Some(a) = p.dict.get(b"Annots").map(|a| doc.resolve(a)) else { return Vec::new() };
        a.as_array()
            .unwrap()
            .iter()
            .filter_map(|o| doc.resolve(o).as_dict().and_then(|d| d.name(b"Subtype")).map(|s| String::from_utf8_lossy(s).into_owned()))
            .collect()
    };
    let mut doc = build(&objs);
    assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), flattened.len());
    assert_eq!(left(&reopen(&doc)), kept, "only the comments were flattened");
    let mut doc = build(&objs);
    assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 1, "the widget");
    let without_widget: Vec<&str> = all.iter().copied().filter(|s| *s != "Widget").collect();
    assert_eq!(left(&reopen(&doc)), without_widget, "flattening fields touches no annotation that isn't a widget");
}

/// A footnote marker (`Ts` text rise, a smaller `Tf` size) does not split the paragraph around it
/// into separate editable zones (#3252): on `main` this read as three blocks — the text up to the
/// marker, the marker alone, and the text after it — because the row-joining baseline tolerance
/// was scaled by the smaller of the two runs' sizes, far tighter than a typical superscript's rise.
#[test]
fn a_superscript_footnote_marker_stays_in_its_paragraphs_row() {
    let doc = text_page(
        "BT /F1 12 Tf 72 700 Td (and A.) Tj 0 -14 Td (Edward Siecienski) Tj 4 Ts /F1 7.2 Tf (3) Tj 0 Ts /F1 12 Tf (. More words continue the paragraph right after the marker.) Tj ET",
    );
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 4, "the marker is its own line, raised and smaller: {lines:?}");
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 1, "one paragraph, not split at the marker: {blocks:?}");
    assert_eq!(blocks[0].rows.len(), 2, "two visual rows, the second carrying the marker");
    assert_eq!(blocks[0].text, "and A. Edward Siecienski3. More words continue the paragraph right after the marker.", "{:?}", blocks[0]);
}

/// Rewriting text that is a clipping path destroyed the artwork it masked: the glyphs went, the
/// mask with them, and the fill behind flooded its own rectangle, in a file that still opened
/// without a warning (#3429). Photoshop exports draw every text object this way. The rewrite is
/// refused instead, as `clear_block` already refuses a preview.
#[test]
fn rewriting_clipping_path_text_is_refused_for_every_clipping_mode() {
    for mode in [4, 5, 6, 7] {
        let before = text::text_lines(&clipping_text_page(mode), 0).unwrap();
        assert_eq!(before.len(), 1, "mode {mode}: the mask is one line");

        let mut doc = clipping_text_page(mode);
        let line = text::replace_line(&mut doc, 0, 0, "EDITED");
        assert!(matches!(&line, Err(EditError::Invalid(m)) if m.contains("clipping path")), "mode {mode}: {line:?}");

        let mut doc = clipping_text_page(mode);
        let block = text::rewrite_block(&mut doc, 0, 0, Some("EDITED"), &BlockStyle::default());
        assert!(matches!(&block, Err(EditError::Invalid(m)) if m.contains("clipping path")), "mode {mode}: {block:?}");

        // Refused means untouched: the mask is still there to shape what is painted through it.
        let after = text::text_lines(&doc, 0).unwrap();
        assert_eq!(after.len(), 1, "mode {mode}");
        assert_eq!(after[0].text, "MASK", "mode {mode}");
    }
}

/// The modes that paint ink are still editable: only 4 to 7 add to the clip path.
#[test]
fn painting_modes_are_still_rewritten() {
    for mode in [0, 1, 2] {
        let mut doc = clipping_text_page(mode);
        assert!(text::replace_line(&mut doc, 0, 0, "EDITED").is_ok(), "mode {mode}");
        assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "EDITED", "mode {mode}");
    }
}

/// A line's runs need not share a rendering mode: nothing in the join requires it, so a line that
/// starts as ink can continue as a mask. The refusal reads the whole line, not the mode its first
/// run set -- otherwise the rewrite passed the guard and took the mask away with the ink.
#[test]
fn a_line_that_starts_as_ink_and_continues_as_a_mask_is_refused() {
    let content = "q\nBT /F1 48 Tf 40 60 Td 0 Tr (TITLE) Tj 7 Tr (MASK) Tj ET\n1 0 0 rg 0 0 300 150 re f\nQ\n";
    let before = text::text_lines(&text_page(content), 0).unwrap();
    assert_eq!(before.len(), 1, "the two runs are one line: {:?}", before.iter().map(|l| &l.text).collect::<Vec<_>>());
    assert_eq!(before[0].text, "TITLEMASK", "and the mask is the second half of it");

    let mut doc = text_page(content);
    let line = text::replace_line(&mut doc, 0, 0, "EDITED");
    assert!(matches!(&line, Err(EditError::Invalid(m)) if m.contains("clipping path")), "{line:?}");
    let block = text::rewrite_block(&mut doc, 0, 0, Some("EDITED"), &BlockStyle::default());
    assert!(matches!(&block, Err(EditError::Invalid(m)) if m.contains("clipping path")), "{block:?}");
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "TITLEMASK", "refused means untouched");
}

/// Another drawing of the line on the same baseline goes with it, so that the replacement is all
/// that shows -- and that copy can be the mask while the line being replaced is ink. Dropping it
/// destroys the artwork as surely as rewriting the mask itself, so this is refused too.
#[test]
fn a_clipping_copy_on_the_same_baseline_is_refused() {
    let content = "q\nBT 0 Tr /F1 48 Tf 40 60 Td (MASK) Tj ET\nBT 7 Tr /F1 48 Tf 40 60 Td (MASK) Tj ET\n1 0 0 rg 0 0 300 150 re f\nQ\n";
    let before = text::text_lines(&text_page(content), 0).unwrap();
    assert_eq!(before.len(), 2, "two drawings of the same line");

    let mut doc = text_page(content);
    let line = text::replace_line(&mut doc, 0, 0, "EDITED");
    assert!(matches!(&line, Err(EditError::Invalid(m)) if m.contains("clipping path")), "{line:?}");
    let block = text::rewrite_block(&mut doc, 0, 0, Some("EDITED"), &BlockStyle::default());
    assert!(matches!(&block, Err(EditError::Invalid(m)) if m.contains("clipping path")), "{block:?}");
    assert_eq!(text::text_lines(&doc, 0).unwrap().len(), 2, "refused means untouched");
}

/// #2512: a session's drafts are laid out from the document as it was opened, never from each
/// other, and its commit writes what a one-shot rewrite writes, whatever was laid out before.
#[test]
fn a_text_session_commits_what_a_rewrite_writes_whatever_it_laid_out_before() {
    let page = "BT /F1 12 Tf 14 TL 72 700 Td (The first row of a paragraph) Tj T* (and its second row here.) Tj ET";
    let bold = BlockStyle { bold: Some(true), ..Default::default() };
    let drafts: [(&str, &BlockStyle); 4] = [
        ("The first row of a paragraph\nand its second row here, with more words to wrap it onto a third row", &BlockStyle::default()),
        ("Short", &bold),
        ("The first row\nsecond", &BlockStyle::default()),
        ("The first row of a paragraph\nand its second row here.", &bold),
    ];
    let doc = text_page(page);
    let session = text::TextSession::open(&doc, 0, 0).unwrap();
    assert_eq!(session.source().editor_text, "The first row of a paragraph\nand its second row here.");
    for (text, style) in drafts {
        let fresh = text::text_edit_layout(&text_page(page), 0, 0, text, style).unwrap();
        assert_eq!(session.layout(text, style).unwrap(), fresh, "{text:?} is laid out from the opened page");
    }
    let (text, style) = drafts[0];
    let mut by_session = doc.clone();
    session.commit(&mut by_session, Some(text), style).unwrap();
    let mut by_rewrite = text_page(page);
    text::rewrite_block(&mut by_rewrite, 0, 0, Some(text), style).unwrap();
    assert_eq!(
        pdfcraft_cos::write_full(&by_session, &SaveOptions::default()).unwrap(),
        pdfcraft_cos::write_full(&by_rewrite, &SaveOptions::default()).unwrap(),
        "the commit writes what a rewrite writes"
    );
    // A document changed since the session opened isn't written to.
    let before = pdfcraft_cos::write_full(&by_session, &SaveOptions::default()).unwrap();
    let refused = session.commit(&mut by_session, Some("Again"), &BlockStyle::default());
    assert!(matches!(&refused, Err(EditError::Invalid(m)) if m.contains("changed while it was being edited")), "{refused:?}");
    assert_eq!(pdfcraft_cos::write_full(&by_session, &SaveOptions::default()).unwrap(), before);
    assert!(session.is_current(&doc) && !session.is_current(&by_session));
}

/// #3115: a row drawn in a hidden layer, at 30 % opacity or at 80 % horizontal scale keeps that
/// state when another row of the same paragraph is edited; the edited row doesn't take it on.
#[test]
fn editing_a_row_keeps_the_layer_opacity_and_scale_of_the_rows_around_it() {
    let row1 = "BT /F1 12 Tf 72 700 Td (Hello world, this is the first line) Tj ET\n";
    let row2 = "BT /F1 12 Tf 72 686 Td (and the second) Tj ET";
    let pages = [
        (format!("{row1}/OC /L1 BDC {row2} EMC"), (true, false, 100.0)),
        (format!("{row1}q /GS0 gs {row2} Q"), (false, true, 100.0)),
        ("BT /F1 12 Tf 72 700 Td (Hello world, this is the first line) Tj 80 Tz 0 -14 Td (and the second) Tj ET".to_string(), (false, false, 80.0)),
        (format!("{row1}{row2}"), (false, false, 100.0)),
    ];
    for (n, (content, (oc, gs, tz))) in pages.iter().enumerate() {
        let mut doc = row_state_page(content);
        let blocks = text::text_blocks(&doc, 0).unwrap();
        let block = blocks.iter().position(|b| b.text.starts_with("Hello")).unwrap();
        let text = blocks[block].text.replacen("Hello", "Jello", 1);
        text::replace_block(&mut doc, 0, block, &text).unwrap();
        let shown = shown_states(&reopen(&doc));
        let find = |needle: &str| shown.iter().filter(|s| s.0.contains(needle)).cloned().collect::<Vec<_>>();
        let first = find("Jello");
        assert_eq!(first.len(), 1, "page {}: {shown:?}", n + 1);
        assert_eq!((first[0].1, first[0].2, first[0].3), (false, false, 100.0), "page {}: the edited row keeps its own state: {shown:?}", n + 1);
        let second = find("and the second");
        assert_eq!(second.len(), 1, "page {}: {shown:?}", n + 1);
        assert_eq!((second[0].1, second[0].2, second[0].3), (*oc, *gs, *tz), "page {}: the other row keeps its state: {shown:?}", n + 1);
    }
    // The control: rows in the same state are still one paragraph.
    let blocks = text::text_blocks(&row_state_page(&pages[3].0), 0).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "Hello world, this is the first line and the second");
}

/// What keeps rows apart is the graphics state they are drawn in, not the `gs` operators
/// before them: a repeated `gs` without q/Q, or another name with the same settings, is still one
/// paragraph, and a `gs` that changes the opacity or puts it back is not.
#[test]
fn rows_under_repeated_or_equal_gs_states_stay_one_paragraph() {
    let row1 = "BT /F1 12 Tf 72 700 Td (Hello world, this is the first line) Tj ET\n";
    let row2 = "BT /F1 12 Tf 72 686 Td (and the second) Tj ET";
    let paragraphs = |content: String| text::text_blocks(&row_state_page(&content), 0).unwrap().len();
    assert_eq!(paragraphs(format!("/GS0 gs {row1}/GS0 gs {row2}")), 1, "the same name twice");
    assert_eq!(paragraphs(format!("/GS0 gs {row1}/GS0 gs /GS0 gs {row2}")), 1, "the same name three times");
    assert_eq!(paragraphs(format!("/GS0 gs {row1}/GS1 gs {row2}")), 1, "two names, the same settings");
    assert_eq!(paragraphs(format!("/GS2 gs {row1}{row2}")), 1, "settings that are the defaults");
    assert_eq!(paragraphs(format!("{row1}/GS0 gs {row2}")), 2, "another opacity");
    assert_eq!(paragraphs(format!("/GS0 gs {row1}/GS2 gs {row2}")), 2, "the opacity put back");
}

#[test]
fn vertical_text_lines_are_columns_and_editing_one_keeps_its_neighbour() {
    let doc = vertical_columns_page();
    let lines = text::text_lines(&doc, 0).unwrap();
    let first = lines.iter().find(|l| l.baseline()[0] == 500.0).unwrap();
    let r = first.rect;
    // Six glyphs, each 20 pt wide and advancing 20 pt down, centred on x = 500.
    assert!((r[0] - 490.0).abs() < 0.5 && (r[2] - 510.0).abs() < 0.5, "{r:?}");
    assert!((r[1] - 600.0).abs() < 0.5 && (r[3] - 720.0).abs() < 0.5, "{r:?}");
    for edit in [0, 1] {
        let mut doc = vertical_columns_page();
        let lines = text::text_lines(&doc, 0).unwrap();
        if edit == 0 {
            let line = lines.iter().position(|l| l.baseline()[0] == 500.0).unwrap();
            text::replace_line(&mut doc, 0, line, "ABCDEA").unwrap();
        } else {
            let blocks = text::text_blocks(&doc, 0).unwrap();
            let block = blocks.iter().position(|b| b.lines.iter().any(|&i| lines[i].baseline()[0] == 500.0)).unwrap();
            assert_eq!(blocks[block].lines.len(), 1, "{blocks:?}");
            text::replace_block(&mut doc, 0, block, "ABCDEA").unwrap();
        }
        let after = text::text_lines(&doc, 0).unwrap();
        let texts: Vec<(f64, &str)> = after.iter().map(|l| (l.baseline()[0], l.text.as_str())).collect();
        assert!(texts.contains(&(500.0, "ABCDEA")), "{texts:?}");
        assert!(texts.contains(&(470.0, "ABCDEF")), "{texts:?}");
        assert!(texts.contains(&(300.0, "ABCD")), "{texts:?}");
    }
}

#[test]
fn text_whose_cmap_writing_mode_cannot_be_read_is_not_edited() {
    let mut doc = vertical_columns_page();
    make_cmap_cyclic(&mut doc);
    let error = text::replace_line(&mut doc, 0, 0, "ABCDEA").unwrap_err();
    assert!(error.to_string().contains("cyclic"), "{error}");
}

#[test]
fn a_cyclic_cmap_falls_back_to_horizontal_and_only_refuses_its_own_lines() {
    let mut doc = vertical_page("BT /V1 20 Tf 100 700 Td <00410042> Tj ET\nBT /F1 12 Tf 100 600 Td (Plain) Tj ET");
    make_cmap_cyclic(&mut doc);
    // The page is still listed: the broken font's text as horizontal, the rest untouched.
    let lines = text::text_lines(&doc, 0).unwrap();
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["AB", "Plain"], "{texts:?}");
    assert!(lines[0].rect[2] - lines[0].rect[0] > 30.0 && lines[0].rect[3] - lines[0].rect[1] < 30.0, "{:?}", lines[0].rect);
    assert_eq!(text::reading_blocks(&doc, 0).unwrap().len(), 2);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    // Only the line in the broken font is refused.
    let error = text::replace_line(&mut doc.clone(), 0, 0, "BA").unwrap_err();
    assert!(error.to_string().contains("cyclic"), "{error}");
    let block = text::text_blocks(&doc, 0).unwrap().iter().position(|b| b.text == "AB").unwrap();
    let error = text::replace_block(&mut doc.clone(), 0, block, "BA").unwrap_err();
    assert!(error.to_string().contains("cyclic"), "{error}");
    text::replace_line(&mut doc, 0, 1, "Changed").unwrap();
    let texts: Vec<String> = text::text_lines(&doc, 0).unwrap().into_iter().map(|l| l.text).collect();
    assert_eq!(texts, ["AB", "Changed"]);
}

#[test]
fn vertical_text_drawn_one_glyph_per_tj_is_one_line() {
    // One text object, a `Tj` per glyph; and a column with a text object per glyph.
    let doc = vertical_page(
        "BT /V1 20 Tf 500 720 Td <0041> Tj <0042> Tj <0043> Tj ET\n\
         BT /V1 20 Tf 1 0 0 1 470 720 Tm <0041> Tj ET\nBT /V1 20 Tf 1 0 0 1 470 700 Tm <0042> Tj ET\nBT /V1 20 Tf 1 0 0 1 470 680 Tm <0043> Tj ET",
    );
    let lines = text::text_lines(&doc, 0).unwrap();
    let columns: Vec<(f64, &str)> = lines.iter().map(|l| (l.baseline()[0], l.text.as_str())).collect();
    assert_eq!(columns, [(500.0, "ABC"), (470.0, "ABC")], "{columns:?}");
    for line in &lines {
        let r = line.rect;
        assert!((r[1] - 660.0).abs() < 0.5 && (r[3] - 720.0).abs() < 0.5, "{r:?}");
    }
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
}

/// Two genuinely stacked rows of ordinary body text must not join into one row just because the
/// first is narrow enough to put the second's start within the #3252 tolerance band: here the
/// second row starts 1.4pt to the left of where the first ends (so the two overlap a little,
/// as compressed text or a tight column can) and 5pt below its baseline — inside the widened 50%
/// tolerance but outside the original 20% one. They stay two rows (and two paragraphs).
#[test]
fn stacked_rows_that_merely_overlap_do_not_join_into_one_row() {
    let doc = text_page("BT /F1 12 Tf 72 700 Td (A compressed line of body text here) Tj 190 -5 Td (Second stacked line of body text) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 2);
    let gap = lines[1].rect[0] - lines[0].rect[2];
    assert!((-2.4..0.0).contains(&gap), "the rows overlap a little, within the old tolerance band: {gap}");
    let baseline_shift = lines[0].rect[3] - lines[1].rect[3];
    assert!((2.4..6.0).contains(&baseline_shift), "between the old and new vertical tolerance: {baseline_shift}");
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 2, "two separate rows stay two paragraphs, not one: {blocks:?}");
}

/// Two equal-size rows with no horizontal overlap must not join into one row either (#3252):
/// the wider tolerance is for a run distinctly smaller or larger than its neighbour (a
/// super/subscript), not for two ordinary body-text rows that happen to sit close enough
/// vertically — here a 5pt baseline shift, inside the widened 50% tolerance but outside the
/// original 20% one, with the second row starting just past where the first ends.
#[test]
fn equal_size_rows_need_the_tighter_tolerance_even_without_overlap() {
    let doc = text_page("BT /F1 12 Tf 72 700 Td (Short line one) Tj 75 -5 Td (Second line two here) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].size, lines[1].size, "same size: no super/subscript here");
    let gap = lines[1].rect[0] - lines[0].rect[2];
    assert!(gap >= 0.0, "no horizontal overlap: {gap}");
    let baseline_shift = lines[0].rect[3] - lines[1].rect[3];
    assert!((2.4..6.0).contains(&baseline_shift), "between the old and new vertical tolerance: {baseline_shift}");
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 2, "two separate rows stay two paragraphs, not one: {blocks:?}");
}

/// A `TJ` with a surplus array operand (`[(FIRSTARR)] [(LASTARR)] TJ`) is malformed, but the
/// operator still takes the one immediately before it, as the renderer, MuPDF and pdftotext do
/// (#3569) -- not the first, which this reader took before, showing a word the page never draws.
#[test]
fn a_tj_with_a_surplus_array_reads_the_last_one() {
    let doc = text_page("BT /F1 24 Tf 20 100 Td [(FIRSTARR)] [(LASTARR)] TJ ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "LASTARR", "the operand immediately before TJ, as the page draws: {lines:?}");
}

/// A producer wrote `0.0001 Tc` on one word of a line (Word through macOS Quartz). That spacing
/// can't be seen, so the word stays on its line; spacing that can be seen still keeps runs apart.
#[test]
fn a_word_with_invisible_extra_spacing_stays_on_its_line() {
    let page = |spacing: &str| {
        text_page(&format!(
            "BT /F1 12 Tf 1 0 0 1 72 700 Tm (for company ) Tj ET \
             BT /F1 12 Tf 1 0 0 1 141.36 700 Tm (A) Tj ET \
             BT {spacing} Tc /F1 12 Tf 1 0 0 1 149.364 700 Tm (EROSOL) Tj ET"
        ))
    };
    let blocks = text::text_blocks(&page("0.0001"), 0).unwrap();
    assert_eq!(blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["for company AEROSOL"]);

    let mut doc = page("0.0001");
    text::rewrite_block(&mut doc, 0, 0, Some("for company AEROSOL LLC"), &BlockStyle::default()).unwrap();
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["for company AEROSOL LLC"]);
    assert!((lines[0].rect[0] - 72.0).abs() < 0.01, "{:?}", lines[0].rect);

    let blocks = text::text_blocks(&page("0.5"), 0).unwrap();
    assert_eq!(blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["for company A", "EROSOL"]);
}

/// #3573: past the 256-level marked-content cap every `EMC` must still close its own sequence,
/// so `/ActualText` nested that deep is neither dropped nor applied to too little text.
#[test]
fn actual_text_pairs_past_the_marked_content_cap() {
    let span = "/Span <</ActualText (REPLACED)>> BDC BT /F1 24 Tf 72 700 Td (abc) Tj ET EMC";
    let outer = "/Span <</ActualText (OUTER)>> BDC BT /F1 24 Tf 72 700 Td /Y BMC (abc) Tj EMC (def) Tj ET EMC";
    for (content, expected) in [
        (span.to_string(), "REPLACED"),
        (format!("{}{span}{}", "/X BMC ".repeat(256), " EMC".repeat(256)), "REPLACED"),
        (format!("{}{span}{}", "/X BMC ".repeat(1000), " EMC".repeat(1000)), "REPLACED"),
        (outer.to_string(), "OUTER"),
        (format!("{}{outer}{}", "/X BMC ".repeat(255), " EMC".repeat(255)), "OUTER"),
    ] {
        let doc = text_page(&content);
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), [expected]);
        let reading: Vec<String> = text::reading_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
        assert_eq!(reading, [expected]);
    }
}

/// #2992: page background detection follows the first full-page fill under text and ignores
/// partial, translucent, clipped, or unsupported fills.
#[test]
fn a_page_painted_first_with_a_full_page_fill_has_that_background() {
    let gs = "/ExtGState << /Half << /ca 0.5 >> /Mul << /BM /Multiply >> /Full << /ca 1 /SMask /None >> >>";
    let mut doc = fixture();
    assert_eq!(page_background(&doc, 0).unwrap(), None, "text first");
    add_background(&mut doc, &[0, 1], &Background { color: [0.0, 0.5, 0.5], opacity: 1.0, ..Background::default() }, false).unwrap();
    let doc = reopen(&doc);
    let teal = page_background(&doc, 0).unwrap().expect("Background's fill");
    assert!(teal.iter().zip([0.0, 0.5, 0.5]).all(|(a, b)| (a - b).abs() < 1e-6), "{teal:?}");
    assert!(page_background(&doc, 1).unwrap().is_some(), "on a turned page too");
    assert_eq!(page_background(&doc, 2).unwrap(), None, "a page with no content");
    assert!(page_background(&doc, 9).is_err());

    assert_page_background_fill_cases(gs);
    assert_page_background_transform_and_clip_cases();
}

#[test]
fn reading_keeps_dotted_leader_entries_whole_and_in_order() {
    let mut content = String::new();
    for (title, page, y) in [("Art.1. Scope", "3", 700), ("Art.2. Eligibility", "7", 680), ("Art.3. Application", "12", 660)] {
        content.push_str(&format!(
            "BT /F1 12 Tf 72 {y} Td ({title}) Tj ET \
             BT /F1 12 Tf 250 {y} Td (....................) Tj ET \
             BT /F1 12 Tf 500 {y} Td ({page}) Tj ET\n"
        ));
    }
    content.push_str("BT /F1 12 Tf 72 640 Td (Appendix) Tj ET BT /F1 12 Tf 250 640 Td (\\205\\205\\205) Tj ET BT /F1 12 Tf 500 640 Td (iv) Tj ET\n");
    content.push_str("BT /F1 12 Tf 72 600 Td (Ordinary body text.) Tj ET BT /F1 12 Tf 72 582 Td (Still the same paragraph.) Tj ET");
    let doc = text_page(&content);
    let blocks: Vec<_> = text::reading_blocks(&doc, 0).unwrap().into_iter().map(|b| b.text).collect();
    assert_eq!(
        blocks,
        [
            "Art.1. Scope .................... 3",
            "Art.2. Eligibility .................... 7",
            "Art.3. Application .................... 12",
            "Appendix ……… iv",
            "Ordinary body text. Still the same paragraph.",
        ]
    );
}

/// A `cm` inside a BDC…EMC unit applies to later content; arranging the unit must not shift anything.
#[test]
fn arrange_keeps_cm_leaking_out_of_marked_content() {
    let mut doc = regression_page("/Figure <</MCID 0>> BDC 1 0 0 1 50 50 cm q 100 0 0 100 0 0 cm /Im0 Do Q EMC q 10 0 0 10 200 200 cm /Im0 Do Q");
    let before = regression_rects(&doc);
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    assert_eq!(before, regression_rects(&reopen(&doc)), "other content moved");
}

/// Cropping a Form XObject to 100×100 keeps a 100×100 visible area (or is refused) — never 1×1.
#[test]
fn crop_form_xobject_keeps_requested_area() {
    let mut doc = regression_page("q 1 0 0 1 100 100 cm /Fm0 Do Q");
    if crop(&mut doc, 0, 0, [100.0, 100.0, 200.0, 200.0]).is_err() {
        return; // refusing to crop vector art is also acceptable
    }
    let after = images::page_images(&reopen(&doc), 0).unwrap()[0].clone();
    let v = after.visible_rect.unwrap_or(after.rect);
    assert!((v[2] - v[0] - 100.0).abs() < 1.0 && (v[3] - v[1] - 100.0).abs() < 1.0, "expected 100x100 visible, got {v:?}");
}

/// A gs naming a missing ExtGState must not be read as 50% opacity and block the reorder.
#[test]
fn arrange_tolerates_missing_extgstate() {
    let mut doc = regression_page("q /GSmissing gs 100 0 0 100 50 50 cm /Im0 Do Q /GSone gs q 10 0 0 10 200 200 cm /Im0 Do Q");
    let r = arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront);
    assert!(r.is_ok(), "{r:?}");
}

/// Bring to front on the image already in front must not report a change it didn't make.
#[test]
fn noop_arrange_is_not_ok_without_change() {
    let mut doc = regression_page("q 10 0 0 10 200 200 cm /Im0 Do Q q 100 0 0 100 50 50 cm /Im0 Do Q");
    let before = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let r = arrange(&mut doc, 0, 1, images::ArrangeOrder::BringToFront);
    let after = write_incremental(&doc, &SaveOptions::default()).unwrap();
    assert!(r.is_err() || before != after, "Ok with no change");
}

/// Cropping a turned image keeps the requested upright box in user space, rather than expanding
/// to the bounding box of the turned image coordinates.
#[test]
fn crop_turned_image_preserves_requested_area() {
    let mut doc = regression_page("q 70.71 70.71 -70.71 70.71 300 100 cm /Im0 Do Q");
    let req = [270.0, 150.0, 330.0, 200.0];
    crop(&mut doc, 0, 0, req).expect("crop succeeds");
    let after = &images::page_images(&reopen(&doc), 0).unwrap()[0];
    let v = after.visible_rect.expect("visible rect exists");
    assert!((v[0] - req[0]).abs() < 1.0, "x0: expected {}, got {}", req[0], v[0]);
    assert!((v[1] - req[1]).abs() < 1.0, "y0: expected {}, got {}", req[1], v[1]);
    assert!((v[2] - req[2]).abs() < 1.0, "x1: expected {}, got {}", req[2], v[2]);
    assert!((v[3] - req[3]).abs() < 1.0, "y1: expected {}, got {}", req[3], v[3]);
}

/// Text set larger than the widest page PDF allows (14 400 pt) panicked when its paragraph was given
/// a width: the width is clamped between the font size and 14 400 pt, and `f64::clamp` panics when
/// its minimum is the larger. The width is now capped at 14 400 pt whatever the font size: from the
/// file (`Tf`, or a small `Tf` scaled up by `Tm`) or asked for (the automation tools pass it through).
#[test]
fn width_on_a_paragraph_set_larger_than_a_page_is_capped_not_a_panic() {
    let wide = text::BlockStyle { width: Some(200.0), ..Default::default() };
    let huge = text::BlockStyle { size: Some(20_000.0), width: Some(200.0), ..Default::default() };
    for (content, style) in [
        ("BT /F2 20000 Tf 72 700 Td (ab) Tj ET", &wide),
        ("BT /F2 1 Tf 20000 0 0 20000 72 700 Tm (ab) Tj ET", &wide),
        ("BT /F2 12 Tf 72 700 Td (ab) Tj ET", &huge),
    ] {
        let mut doc = text_page(content);
        text::rewrite_block(&mut doc, 0, 0, None, style).unwrap();
    }
}

#[test]
fn added_text_is_wrapped_once_per_write() {
    let mut doc = fixture();
    let lines: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
    let text = AddedText { rect: [72.0, 100.0, 300.0, 700.0], text: lines.join("\n"), size: 10.0, align: Align::Justify, ..AddedText::default() };
    added::WRAP_CALLS.with(|c| c.set(0));
    add_content(&mut doc, 0, &Content::Text(text)).unwrap();
    let calls = added::WRAP_CALLS.with(std::cell::Cell::get);
    assert!(calls <= 4, "the text was wrapped {calls} times for 40 lines");
}

/// Character spacing on added text (#4245) is the PDF's `Tc`, kept in `/PCAdded` so the item
/// reopens with it; an item written without it reads 0.
#[test]
fn added_text_character_spacing_is_drawn_and_reopens() {
    let mut doc = fixture();
    let plain = AddedText { rect: [72.0, 600.0, 400.0, 700.0], text: "Approved".into(), size: 10.0, ..AddedText::default() };
    add_content(&mut doc, 0, &Content::Text(plain.clone())).unwrap();
    add_content(&mut doc, 0, &Content::Text(AddedText { char_spacing: 2.5, ..plain })).unwrap();
    let doc = reopen(&doc);
    let spacings: Vec<f64> = list_added(&doc).iter().map(|a| if let Content::Text(t) = &a.content { t.char_spacing } else { f64::NAN }).collect();
    assert_eq!(spacings, [0.0, 2.5]);
    let page = streams(&doc, 0);
    let tc: Vec<bool> = page.iter().filter(|s| s.contains("(Approved) Tj")).map(|s| s.contains("2.5 Tc")).collect();
    assert_eq!(tc, [false, true], "{page:?}");
}

/// Spacing widens every measure the layout takes: the widest line and wrapping.
#[test]
fn added_text_character_spacing_widens_its_layout() {
    let plain = AddedText { rect: [0.0, 0.0, 1000.0, 100.0], text: "Approved".into(), size: 10.0, ..AddedText::default() };
    let spaced = AddedText { char_spacing: 3.0, ..plain.clone() };
    // After each of the 8 letters, like `Tc`.
    assert!((added::text_width(&spaced) - added::text_width(&plain) - 24.0).abs() < 1e-9);
    let w = added::text_width(&plain);
    let narrow = |t: &AddedText| AddedText { text: "Approved Approved".into(), rect: [0.0, 0.0, 2.0 * w + 5.0, 100.0], ..t.clone() };
    assert_eq!(added::lines(&narrow(&plain)).len(), 1);
    assert_eq!(added::lines(&narrow(&spaced)).len(), 2, "the spaced words no longer fit one line");
}

/// A field tree whose every node lists its child twice is pruned in work linear in its nodes, not
/// in its paths, and a node listing itself is visited once.
#[test]
fn flattening_prunes_a_field_tree_with_repeated_kids_once_per_node() {
    let levels = 16;
    let page = stream("", " ");
    // 5: the flattened widget; 6…: the chain; then the hidden widget at its end, on the page.
    let leaf = 6 + levels;
    let fields = format!("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R {} 0 R] >> >>", leaf + 2);
    let annots = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R {leaf} 0 R] >>");
    let mut objs: Vec<String> = vec![
        fields,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        annots,
        page,
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T (shown) /Rect [10 10 150 40] /F 4 /AP << /N {} 0 R >> >>", leaf + 1),
    ];
    for i in 0..levels {
        let next = 7 + i;
        objs.push(format!("<< /T (n{i}) /Kids [{next} 0 R {next} 0 R] >>"));
    }
    objs.push("<< /Type /Annot /Subtype /Widget /FT /Tx /T (hidden) /Rect [10 90 150 120] /F 2 >>".into());
    objs.push("<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream".into());
    objs.push(format!("<< /T (loop) /Kids [{} 0 R] >>", leaf + 2));
    let objs: Vec<&str> = objs.iter().map(String::as_str).collect();
    let mut doc = build(&objs);
    flatten::KEEP_CALLS.with(|c| c.set(0));
    assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 1);
    let calls = flatten::KEEP_CALLS.with(std::cell::Cell::get);
    assert!(calls <= 2 * (levels + 4), "{calls} field-tree visits");
    let root = doc.root().unwrap();
    let af = doc.resolve(doc.get(root).as_dict().unwrap().get(b"AcroForm").unwrap());
    let left = doc.resolve(af.as_dict().unwrap().get(b"Fields").unwrap()).as_array().unwrap().len();
    assert_eq!(left, 2, "the chain to the hidden widget and the self-listing node stay");
}

/// A field reached first at the end of a long chain has its kids cut off by the depth limit, so
/// they survive there. A shorter path to the same field still looks at its kids: an off-page widget
/// under it is taken out when every field is flattened.
#[test]
fn a_field_reached_again_by_a_shorter_path_is_pruned_there() {
    let chain = 64;
    let page = stream("", " ");
    // 7…: the chain; then the shared field, its off-page widget, and the short path to it.
    let shared = 7 + chain;
    let fields = format!("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 7 0 R {} 0 R] >> >>", shared + 2);
    let mut objs: Vec<String> = vec![
        fields,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R] >>".into(),
        page,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (shown) /Rect [10 10 150 40] /F 4 /AP << /N 6 0 R >> >>".into(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream".into(),
    ];
    for i in 0..chain {
        objs.push(format!("<< /T (c{i}) /Kids [{} 0 R] >>", 8 + i));
    }
    objs.push(format!("<< /T (shared) /Kids [{} 0 R] >>", shared + 1));
    objs.push("<< /Type /Annot /Subtype /Widget /FT /Tx /T (orphan) /Rect [10 50 150 80] /F 4 >>".into());
    objs.push(format!("<< /T (short) /Kids [{shared} 0 R] >>"));
    let objs: Vec<&str> = objs.iter().map(String::as_str).collect();
    let mut doc = build(&objs);
    assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 1);
    let shared_ref = pdfcraft_cos::ObjRef::new(u32::try_from(shared).unwrap(), 0);
    let kids = doc.resolve(doc.get(shared_ref).as_dict().unwrap().get(b"Kids").unwrap()).as_array().unwrap().len();
    assert_eq!(kids, 0, "the off-page widget is pruned along the short path");
}

/// A ToUnicode destination is repeated for every glyph that shows its code: a page whose text
/// would run past the extraction budget is refused, not built in memory.
#[test]
fn page_text_past_the_budget_is_refused() {
    let cmap = stream("", &format!("1 beginbfchar <41> <{}> endbfchar", "0041".repeat(256)));
    let page = |shown: &str| {
        let content = stream("", &format!("BT /F1 12 Tf 10 10 Td ({shown}) Tj ET"));
        build(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
            &content,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode 6 0 R >>",
            &cmap,
        ])
    };
    let doc = page(&"A".repeat(100_000));
    assert!(matches!(text_lines(&doc, 0), Err(EditError::Invalid(_))), "25 MB of text from a 100 kB page");
    let doc = page("AA");
    assert_eq!(text_lines(&doc, 0).unwrap()[0].text.len(), 512, "a page within the budget still reads");
}

#[test]
fn repeated_content_references_share_one_decode_budget() {
    // One 1 MiB stream (a kilobyte on disk) named 300 times by the page's /Contents.
    let doc = page_with(|doc| {
        let spaces = doc.add(Stream::flate(Dict::new(), &vec![b' '; 1 << 20]));
        (Object::Array(vec![Object::Ref(spaces); 300]), Dict::new())
    });
    match text_lines(&doc, 0) {
        Ok(lines) => panic!("read {} lines from more content than a page may decode", lines.len()),
        Err(e) => assert!(matches!(e, EditError::Invalid(_)), "{e}"),
    }
}

#[test]
fn reading_images_stops_at_the_page_record_limit() {
    // 20 draws of a form drawing a form 20 times that draws an image 20 times: 8,000 images
    // per level-two form, 160,000 in all, from a few hundred bytes.
    let doc = page_with(|doc| {
        let image: Dict = [
            (b"Subtype".to_vec(), Object::Name(b"Image".to_vec())),
            (b"Width".to_vec(), Object::Int(1)),
            (b"Height".to_vec(), Object::Int(1)),
            (b"ColorSpace".to_vec(), Object::Name(b"DeviceGray".to_vec())),
            (b"BitsPerComponent".to_vec(), Object::Int(8)),
        ]
        .into_iter()
        .collect();
        let mut inner = doc.add(Stream::from_raw(image, vec![0]));
        let draws = "/X Do ".repeat(20);
        let resources = |inner: pdfcraft_cos::ObjRef| -> Dict {
            let xobjects: Dict = [(b"X".to_vec(), Object::Ref(inner))].into_iter().collect();
            [(b"XObject".to_vec(), Object::Dict(xobjects))].into_iter().collect()
        };
        for _ in 0..3 {
            let mut d = resources(inner);
            d = [(b"Subtype".to_vec(), Object::Name(b"Form".to_vec())), (b"Resources".to_vec(), Object::Dict(d))].into_iter().collect();
            inner = doc.add(Stream::from_raw(d, draws.clone().into_bytes()));
        }
        let page = doc.add(Stream::from_raw(Dict::new(), draws.into_bytes()));
        (Object::Ref(page), resources(inner))
    });
    if let Ok(images) = crate::reading_images(&doc, 0) {
        assert!(images.len() <= crate::images::MAX_PAGE_IMAGES, "{} images", images.len());
    }
}

#[test]
fn reading_spends_one_decode_budget_on_content_and_forms() {
    // 130 MiB of content and a 130 MiB form (a kilobyte each on disk): each fits a page's budget,
    // together they don't, so reading leaves the form out and says so.
    let doc = page_with(|doc| {
        let mut body = b"%".to_vec();
        body.resize(130 << 20, b'x');
        let form: Dict = [(b"Subtype".to_vec(), Object::Name(b"Form".to_vec()))].into_iter().collect();
        let form = doc.add(Stream::flate(form, &body));
        let mut content = b"/X Do\n%".to_vec();
        content.resize(130 << 20, b'x');
        let content = doc.add(Stream::flate(Dict::new(), &content));
        let xobjects: Dict = [(b"X".to_vec(), Object::Ref(form))].into_iter().collect();
        (Object::Ref(content), [(b"XObject".to_vec(), Object::Dict(xobjects))].into_iter().collect())
    });
    assert!(crate::text::reading_blocks_checked(&doc, 0).unwrap().truncated);
}

/// A page that can't take flattened content on top (marked content nested past the depth budget)
/// refuses the whole flatten: the pages before it keep their comments and content.
#[test]
fn a_flatten_refused_on_a_later_page_changes_nothing() {
    let deep = stream("", &"/Artifact BMC\n".repeat(10_001));
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R] >>",
        &stream("", " "),
        "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] /F 4 /AP << /N 9 0 R >> >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 7 0 R /Annots [8 0 R] >>",
        &deep,
        "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] /F 4 /AP << /N 9 0 R >> >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 40 40] /Length 0 >>\nstream\n\nendstream",
    ]);
    let first = pdfcraft_cos::ObjRef::new(3, 0);
    let before = (*doc.get(first)).clone();
    assert!(matches!(flatten(&mut doc, &[0, 1], true, false), Err(EditError::Invalid(_))));
    assert_eq!((*doc.get(first)).clone(), before, "the first page keeps its comment and content");
}

#[test]
fn arrange_preserves_clip_leaked_out_of_marked_unit() {
    let content = "/Figure <</MCID 0>> BDC 0 0 300 300 re W n q 10 0 0 10 50 50 cm /Im0 Do Q EMC q 10 0 0 10 400 400 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    let initial = images::page_images(&doc, 0).unwrap();
    assert_eq!(initial.len(), 2);
    assert!(initial[0].visible_rect.is_some());
    assert!(initial[1].visible_rect.is_none(), "second image is fully clipped initially");

    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let after = images::page_images(&doc, 0).unwrap();
    assert_eq!(after.len(), 2);
    let img1 = after.iter().find(|img| (img.rect[0] - 400.0).abs() < 1e-3).expect("second image exists");
    assert!(img1.visible_rect.is_none(), "second image must stay clipped after BringToFront");

    // Conflicting leaking transform with leaking clip must be refused
    let content_cm_clip =
        "/Figure <</MCID 0>> BDC 2 0 0 2 0 0 cm 0 0 300 300 re W n q 10 0 0 10 50 50 cm /Im0 Do Q EMC q 10 0 0 10 400 400 cm /Im0 Do Q";
    let mut doc3 = image_page_with(content_cm_clip);
    let res3 = arrange(&mut doc3, 0, 0, images::ArrangeOrder::BringToFront);
    assert!(res3.is_err(), "moving marked unit with both leaking cm and leaking clip must be refused");
}

#[test]
fn arrange_preserves_leaked_color_and_text_state() {
    let content = "/Figure <</MCID 0>> BDC /DeviceRGB cs 1 0 0 sc /F1 14 Tf 2 Tc q 10 0 0 10 50 50 cm /Im0 Do Q EMC 0 0 10 10 re f q 10 0 0 10 200 200 cm /Im0 Do Q";
    let mut doc = image_page_with(content);
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let doc = reopen(&doc);
    let bytes = page_content_bytes(&doc, 0);
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("1 0 0 sc"), "sc must be preserved in content stream: {s}");
    assert!(s.contains("/F1 14 Tf"), "Tf must be preserved in content stream: {s}");
    assert!(s.contains("2 Tc"), "Tc must be preserved in content stream: {s}");

    // Pixel test: 0 0 10 10 re f still fills red [255, 0, 0] after arrange
    let pdf_bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(pdf_bytes), pdfcraft_render::RenderConfig::default());
    let page =
        renderer.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Pixels, scale: 1.0, ..Default::default() });
    let idx = (((page.height - 5) * page.width + 5) * 4) as usize;
    assert_eq!(page.rgba[idx..idx + 3], [255, 0, 0], "point (5, 5 from bottom) inside 10x10 rect fills red");
}

#[test]
fn arrange_preserves_inherited_drawing_state_for_stencil_and_form() {
    // 1. Stencil mask test:
    // Page has: q 1 0 0 rg q 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 20 700 Td (Text) Tj ET
    // Non-overlapping stencil and text.
    let content = "q 1 0 0 rg q 100 0 0 100 50 50 cm /Im0 Do Q Q BT /F1 12 Tf 20 700 Td (Text) Tj ET";
    let mut doc = stencil_page_with(content);

    // Render before move:
    let pdf_before = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let mut renderer_before = pdfcraft_render::PageRenderer::new(Arc::new(pdf_before), pdfcraft_render::RenderConfig::default());
    let page_before = renderer_before.render(pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    let stencil_pixel_idx = (((page_before.height - 75) * page_before.width + 75) * 4) as usize;
    assert_eq!(page_before.rgba[stencil_pixel_idx..stencil_pixel_idx + 3], [255, 0, 0], "stencil starts red");

    // Bring to front
    arrange(&mut doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();

    // Save and reopen
    let doc = reopen(&doc);

    // Verify content stream has inherited color
    let bytes = page_content_bytes(&doc, 0);
    let s = String::from_utf8_lossy(&bytes);
    assert!(s.contains("1 0 0 rg"), "inherited 1 0 0 rg must be replayed in moved unit: {s}");

    // Render after reopen and verify stencil remains red:
    let pdf_after = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let mut renderer_after = pdfcraft_render::PageRenderer::new(Arc::new(pdf_after), pdfcraft_render::RenderConfig::default());
    let page_after = renderer_after.render(pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    assert_eq!(page_after.rgba[stencil_pixel_idx..stencil_pixel_idx + 3], [255, 0, 0], "stencil stays red after BringToFront and reopen");

    // 2. Form XObject test:
    // Page has: q 0 1 0 rg q 1 0 0 1 50 50 cm /Fm0 Do Q Q BT /F1 12 Tf 20 700 Td (Text) Tj ET
    let form_content = "q 0 1 0 rg q 1 0 0 1 50 50 cm /Fm0 Do Q Q BT /F1 12 Tf 20 700 Td (Text) Tj ET";
    let mut form_doc = form_page_with(form_content);

    let form_pdf_before = write_incremental(&form_doc, &SaveOptions::default()).unwrap();
    let mut form_renderer_before = pdfcraft_render::PageRenderer::new(Arc::new(form_pdf_before), pdfcraft_render::RenderConfig::default());
    let form_page_before = form_renderer_before.render(pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    let form_pixel_idx = (((form_page_before.height - 75) * form_page_before.width + 75) * 4) as usize;
    assert_eq!(form_page_before.rgba[form_pixel_idx..form_pixel_idx + 3], [0, 255, 0], "form starts green");

    arrange(&mut form_doc, 0, 0, images::ArrangeOrder::BringToFront).unwrap();
    let form_doc = reopen(&form_doc);

    let form_pdf_after = write_incremental(&form_doc, &SaveOptions::default()).unwrap();
    let mut form_renderer_after = pdfcraft_render::PageRenderer::new(Arc::new(form_pdf_after), pdfcraft_render::RenderConfig::default());
    let form_page_after = form_renderer_after.render(pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    assert_eq!(form_page_after.rgba[form_pixel_idx..form_pixel_idx + 3], [0, 255, 0], "form stays green after BringToFront and reopen");
}
/// Existing-row alignment is explicitly refused without mutating the paragraph.
#[test]
fn one_row_of_a_paragraph_takes_its_own_alignment_is_refused_without_changing_the_document() {
    for align in [added::Align::Left, added::Align::Center, added::Align::Right, added::Align::Justify] {
        let mut doc = text_page("BT /F1 10 Tf 12 TL 72 700 Td (First row) Tj T* (Second row) Tj ET");
        let before = page_content_bytes(&doc, 0);
        let blocks = text::text_blocks(&doc, 0).unwrap();
        let style = text::BlockStyle { aligns: vec![None, Some(align)], ..Default::default() };
        let error = text::rewrite_block(&mut doc, 0, 0, Some(&blocks[0].editor_text), &style).unwrap_err();
        assert!(error.to_string().contains("selected-row alignment"));
        assert_eq!(page_content_bytes(&doc, 0), before);
        assert_eq!(page_content_bytes(&reopen(&doc), 0), before);
    }
}

/// Refusal preserves all existing rows and their saved content stream.
#[test]
fn rows_without_their_own_alignment_keep_their_operators_is_refused_without_changing_the_document() {
    for align in [added::Align::Left, added::Align::Center, added::Align::Right, added::Align::Justify] {
        let mut doc = text_page("BT /F1 10 Tf 12 TL 72 700 Td (First row) Tj T* (Second row) Tj ET");
        let before = page_content_bytes(&doc, 0);
        let blocks = text::text_blocks(&doc, 0).unwrap();
        let style = text::BlockStyle { aligns: vec![None, Some(align)], ..Default::default() };
        let error = text::rewrite_block(&mut doc, 0, 0, Some(&blocks[0].editor_text), &style).unwrap_err();
        assert!(error.to_string().contains("selected-row alignment"));
        assert_eq!(page_content_bytes(&doc, 0), before);
        assert_eq!(page_content_bytes(&reopen(&doc), 0), before);
    }
}

/// Mixed-style rows also refuse unsupported per-row intent before writing.
#[test]
fn one_row_of_a_styled_region_takes_its_own_alignment_is_refused_without_changing_the_document() {
    for align in [added::Align::Left, added::Align::Center, added::Align::Right, added::Align::Justify] {
        let mut doc = mixed_region_page();
        let before = page_content_bytes(&doc, 0);
        let blocks = text::text_blocks(&doc, 0).unwrap();
        let style = text::BlockStyle { aligns: vec![None, Some(align)], ..Default::default() };
        let error = text::rewrite_block(&mut doc, 0, 1, Some(&blocks[1].editor_text), &style).unwrap_err();
        assert!(error.to_string().contains("selected-row alignment"));
        assert_eq!(page_content_bytes(&doc, 0), before);
        assert_eq!(page_content_bytes(&reopen(&doc), 0), before);
    }
}

/// Added text: each paragraph is laid out with its own alignment, a justified paragraph's last
/// line isn't stretched, and the alignments survive saving and reopening.
#[test]
fn added_text_paragraphs_keep_their_own_alignments() {
    let mut doc = fixture();
    let text = AddedText {
        rect: [72.0, 600.0, 372.0, 700.0],
        text: "Left line\nRight line\nCentred line".into(),
        size: 12.0,
        aligns: vec![None, Some(Align::Right), Some(Align::Center)],
        ..AddedText::default()
    };
    assert_eq!(text.paragraph_align(1), Align::Right);
    add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap();
    let doc = reopen(&doc);
    let Content::Text(t) = &list_added(&doc)[0].content else { panic!() };
    assert_eq!(t.aligns, text.aligns, "saved with the item");
    let page = streams(&doc, 0).join("\n");
    let x_of = |s: &str| -> f64 {
        let at = page.find(&format!("({s}) Tj")).unwrap();
        let tm = page[..at].rfind(" Tm").unwrap();
        let nums: Vec<f64> = page[..tm].rsplit("1 0 0 1 ").next().unwrap().split_whitespace().filter_map(|v| v.parse().ok()).collect();
        nums[0]
    };
    let w = |s: &str| Family::Helvetica.width(s, 12.0, false, false);
    assert!((x_of("Left line") - 72.0).abs() < 0.01);
    assert!((x_of("Right line") - (372.0 - w("Right line"))).abs() < 0.01);
    assert!((x_of("Centred line") - (72.0 + (300.0 - w("Centred line")) / 2.0)).abs() < 0.01);
    // A paragraph's own alignment the same as the box's is no entry at all.
    let tidy = paragraph_align::tidy(vec![Some(Align::Left), Some(Align::Right)], Align::Left);
    assert_eq!(tidy, [None, Some(Align::Right)]);
}

/// With character spacing, a paragraph aligned on its own is placed by its spaced width: a
/// right-aligned line still ends at the box's right edge, and a centred one sits in the middle.
#[test]
fn added_text_paragraphs_align_by_their_spaced_width() {
    let mut doc = fixture();
    let text = AddedText {
        rect: [72.0, 600.0, 372.0, 700.0],
        text: "Left line\nRight line\nCentred line".into(),
        size: 12.0,
        aligns: vec![None, Some(Align::Right), Some(Align::Center)],
        char_spacing: 2.0,
        ..AddedText::default()
    };
    add_content(&mut doc, 0, &Content::Text(text)).unwrap();
    let page = streams(&reopen(&doc), 0).join("\n");
    assert!(page.contains("2 Tc"), "{page}");
    let x_of = |s: &str| -> f64 {
        let at = page.find(&format!("({s}) Tj")).unwrap();
        let tm = page[..at].rfind(" Tm").unwrap();
        let nums: Vec<f64> = page[..tm].rsplit("1 0 0 1 ").next().unwrap().split_whitespace().filter_map(|v| v.parse().ok()).collect();
        nums[0]
    };
    let w = |s: &str| Family::Helvetica.width(s, 12.0, false, false) + 2.0 * s.chars().count() as f64;
    assert!((x_of("Left line") - 72.0).abs() < 0.01);
    assert!((x_of("Right line") - (372.0 - w("Right line"))).abs() < 0.01);
    assert!((x_of("Centred line") - (72.0 + (300.0 - w("Centred line")) / 2.0)).abs() < 0.01);
}

/// A one-row title over a body column: an alignment picked for it is measured across the column.
/// Left puts it on the column's left edge, and centring it again leaves it where it is, rather
/// than aligning it in its own box (a no-op) or toward the page's margin (moving it right each
/// time). It is aligned as the editor aligns a selected row, in whichever paragraph holds it.
#[test]
fn a_lone_row_aligns_across_the_column_it_sits_in() {
    let mut doc = text_page(
        "BT /F1 24 Tf 1 0 0 1 250 700 Tm (Title) Tj ET          BT /F1 10 Tf 12 TL 72 650 Td (The body of the page runs across the whole column here.) Tj T* (It has a second row that is a little shorter.) Tj ET",
    );
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks[0].text, "Title");
    let column = [blocks[1].rect[0], blocks[1].rect[2]];
    let title = |doc: &Document| text::text_lines(doc, 0).unwrap().into_iter().find(|l| l.text == "Title").unwrap().rect;
    let align = |doc: &mut Document, a: added::Align| {
        let lines = text::text_lines(doc, 0).unwrap();
        let line = lines.iter().position(|l| l.text == "Title").unwrap();
        let blocks = text::text_blocks(doc, 0).unwrap();
        let (index, block) = blocks.iter().enumerate().find(|(_, b)| b.lines.contains(&line)).unwrap();
        // The title is the paragraph's first row.
        let style = text::BlockStyle { align: Some(a), ..Default::default() };
        text::rewrite_block(doc, 0, index, Some(&block.editor_text), &style).unwrap();
    };
    align(&mut doc, added::Align::Center);
    let centred = title(&doc);
    assert!(((centred[0] + centred[2]) / 2.0 - (column[0] + column[1]) / 2.0).abs() < 1.0, "centred in the column: {centred:?} vs {column:?}");
    align(&mut doc, added::Align::Center);
    assert!((title(&doc)[0] - centred[0]).abs() < 0.5, "centring again leaves it where it is: {:?} vs {centred:?}", title(&doc));
    align(&mut doc, added::Align::Left);
    assert!((title(&doc)[0] - column[0]).abs() < 0.5, "left: back on the column's edge, {:?} vs {column:?}", title(&doc));
    align(&mut doc, added::Align::Right);
    assert!((title(&doc)[2] - column[1]).abs() < 1.0, "right: on the column's right edge, {:?} vs {column:?}", title(&doc));
}

/// An edited paragraph's text has a line break between its source rows (`editor_text`), as the
/// paragraph editor sends it. Those are the rows of one paragraph, not paragraph ends: justified
/// in a font of its own (the single-style rewrite), every row but the last still fills the width.
#[test]
fn a_justified_paragraph_from_the_editor_fills_its_rows() {
    let mut doc =
        text_page("BT /F1 10 Tf 12 TL 100 700 Td (One two three four five six seven) Tj T* (eight nine ten eleven twelve) Tj T* (end) Tj ET");
    let block = text::text_blocks(&doc, 0).unwrap()[0].clone();
    assert_eq!(block.editor_text.matches('\n').count(), 2);
    let style = text::BlockStyle { align: Some(added::Align::Justify), family: Some((Family::Helvetica, true, false)), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some(&block.editor_text), &style).unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    let row = |t: &str| lines.iter().find(|l| l.text.starts_with(t)).unwrap().rect;
    let right = row("One")[2];
    assert!((row("eight")[2] - right).abs() < 1.0, "the second row is stretched: {:?} vs {right}", row("eight"));
    assert!(row("end")[2] < right - 20.0, "the last row isn't");
}

/// An alignment for a row the draft doesn't have names no row: a one-row title given one for a
/// second row only stays where it is, not moved to its column's edge. Nor does a row of the draft
/// with no alignment of its own move because another row has one (CodeRabbit on 648dd428).
#[test]
fn a_title_moves_to_its_column_only_for_an_alignment_of_its_own() {
    let page = "BT /F1 24 Tf 1 0 0 1 250 700 Tm (Title) Tj ET          BT /F1 10 Tf 12 TL 72 650 Td (The body of the page runs across the whole column here.) Tj T* (It has a second row that is a little shorter.) Tj ET";
    let title = |doc: &Document| text::text_lines(doc, 0).unwrap().into_iter().find(|l| l.text == "Title").unwrap().rect;
    let before = title(&text_page(page));
    for draft in ["Title", "Title\nSub"] {
        let mut doc = text_page(page);
        let style = text::BlockStyle { aligns: vec![None, Some(added::Align::Right)], ..Default::default() };
        assert!(text::rewrite_block(&mut doc, 0, 0, Some(draft), &style).is_err());
        assert!((title(&doc)[0] - before[0]).abs() < 0.5, "{draft:?}: the title stays put, {:?} vs {before:?}", title(&doc));
    }
}

/// A column whose rows are several runs (a second style on the same baseline) reaches the right
/// edge of its whole rows, not of the run at the left edge: a title wider than that first run is
/// still held by the column and aligns to it (CodeRabbit on b9940d7d).
#[test]
fn a_column_is_measured_by_its_whole_rows_not_its_first_runs() {
    let page = "BT /F1 18 Tf 1 0 0 1 100 672 Tm (A longer title) Tj ET                 BT /F1 10 Tf 12 TL 50 610 Td (The first column runs to here.) Tj ET                 BT /F1 11 Tf 1 0 0 rg 1 0 0 1 185 610 Tm (and keeps going on.) Tj ET                 BT /F1 10 Tf 12 TL 50 598 Td (A second row of it.) Tj ET";
    let blocks = text::text_blocks(&text_page(page), 0).unwrap();
    let body = blocks.iter().find(|b| b.text.starts_with("The first")).expect("the body is a block");
    assert!(body.rows.first().is_some_and(|row| row.len() >= 2), "the body's first row is several runs: {:?}", body.rows);
    let lines = text::text_lines(&text_page(page), 0).unwrap();
    let column_right =
        body.rows.iter().filter_map(|row| row.iter().filter_map(|i| lines.get(*i)).map(|l| l.rect[2]).reduce(f64::max)).fold(f64::MIN, f64::max);
    let mut doc = text_page(page);
    let index = blocks.iter().position(|b| b.text == "A longer title").expect("the title is a block of its own");
    let style = text::BlockStyle { align: Some(added::Align::Right), ..Default::default() };
    text::rewrite_block(&mut doc, 0, index, Some("A longer title"), &style).unwrap();
    let title = text::text_lines(&doc, 0).unwrap().into_iter().find(|l| l.text == "A longer title").unwrap().rect;
    assert!((title[2] - column_right).abs() < 1.0, "right: on the whole row's right edge, {title:?} vs {column_right}");
}

/// A row with no column of its own is aligned in its own box, but that box is not the width its new
/// text wraps at: changing the text together with an alignment keeps a single row on one row, as
/// it does without the alignment (CodeRabbit on b9940d7d).
#[test]
fn an_isolated_row_edited_with_an_alignment_does_not_wrap_at_its_old_width() {
    for align in [added::Align::Center, added::Align::Right] {
        let mut doc = text_page("BT /F1 18 Tf 1 0 0 1 100 700 Tm (Title) Tj ET");
        let longer = "A much longer title for the page";
        let style = text::BlockStyle { align: Some(align), ..Default::default() };
        text::rewrite_block(&mut doc, 0, 0, Some(longer), &style).unwrap();
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), [longer], "{align:?}: the title stays on one row");
    }
}

/// A heading across the whole page sits closer to the title than the title's own column does: the
/// title is still aligned in its own column, not across the heading's page-wide frame (FredApps's
/// review of 43f848d5). Centre and right both go by the column.
#[test]
fn a_lone_row_aligns_in_its_own_column_not_a_closer_full_width_heading() {
    let page = "BT /F1 12 Tf 1 0 0 1 50 700 Tm (A heading that runs across both columns of the page, well past the first column, to the far margin.) Tj ET                 BT /F1 18 Tf 1 0 0 1 120 672 Tm (Title) Tj ET                 BT /F1 10 Tf 12 TL 50 610 Td (The first column runs to here.) Tj T* (A second row of it.) Tj ET                 BT /F1 10 Tf 12 TL 320 610 Td (The second column has more rows.) Tj T* (One.) Tj T* (Two.) Tj T* (Three.) Tj ET";
    let blocks = text::text_blocks(&text_page(page), 0).unwrap();
    let column = blocks.iter().find(|b| b.text.starts_with("The first")).unwrap().rect;
    let heading = blocks.iter().find(|b| b.text.starts_with("A heading")).unwrap().rect;
    let title_block = blocks.iter().find(|b| b.text == "Title").expect("the title is a block of its own").rect;
    let gap = |other: [f64; 4]| (other[1] - title_block[3]).max(title_block[1] - other[3]).max(0.0);
    assert!(gap(heading) < gap(column), "the heading is the nearer block: {} vs {}", gap(heading), gap(column));
    assert!(heading[2] - heading[0] > 2.0 * (column[2] - column[0]), "the heading is page-wide: {heading:?} vs {column:?}");
    let title_after = |align: added::Align| {
        let mut doc = text_page(page);
        let blocks = text::text_blocks(&doc, 0).unwrap();
        let index = blocks.iter().position(|b| b.text == "Title").unwrap();
        let style = text::BlockStyle { align: Some(align), ..Default::default() };
        text::rewrite_block(&mut doc, 0, index, Some("Title"), &style).unwrap();
        text::text_lines(&doc, 0).unwrap().into_iter().find(|l| l.text == "Title").unwrap().rect
    };
    let centred = title_after(added::Align::Center);
    assert!(((centred[0] + centred[2]) / 2.0 - (column[0] + column[2]) / 2.0).abs() < 1.0, "centred in its own column: {centred:?} vs {column:?}");
    let right = title_after(added::Align::Right);
    assert!((right[2] - column[2]).abs() < 1.0, "right: on its own column's right edge, {right:?} vs {column:?}");
}

/// The same on a page of two columns, where the other column has more rows: a title over the
/// first column is aligned across the first column, not its own box.
#[test]
fn a_lone_row_aligns_across_its_own_column_of_two() {
    let mut doc = text_page(
        "BT /F1 18 Tf 1 0 0 1 120 700 Tm (Title) Tj ET \
         BT /F1 10 Tf 12 TL 50 660 Td (The first column runs to here.) Tj T* (A second row of it.) Tj ET \
         BT /F1 10 Tf 12 TL 320 660 Td (The second column has more rows.) Tj T* (One.) Tj T* (Two.) Tj T* (Three.) Tj T* (Four.) Tj ET",
    );
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let column = blocks.iter().find(|b| b.text.starts_with("The first")).unwrap().rect;
    let index = blocks.iter().position(|b| b.text == "Title").unwrap();
    let style = text::BlockStyle { align: Some(added::Align::Left), ..Default::default() };
    text::rewrite_block(&mut doc, 0, index, Some("Title"), &style).unwrap();
    let title = text::text_lines(&doc, 0).unwrap().into_iter().find(|l| l.text == "Title").unwrap().rect;
    assert!((title[0] - column[0]).abs() < 0.5, "left: on the first column's edge, {title:?} vs {column:?}");
}

/// A one-row title in two colours is rewritten by the styled-region path, which is given the
/// same column frame: centred twice, it stays centred in the column instead of moving right.
#[test]
fn a_lone_row_of_two_styles_aligns_across_its_column() {
    let mut doc = text_page(
        "BT /F1 18 Tf 1 0 0 1 250 700 Tm (Big ) Tj 1 0 0 rg (Title) Tj ET \
         BT 0 g /F1 10 Tf 12 TL 72 650 Td (The body of the page runs across the whole column here.) Tj T* (It has a second row that is a little shorter.) Tj ET",
    );
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks[0].text, "Big Title");
    let column = [blocks[1].rect[0], blocks[1].rect[2]];
    let title = |doc: &Document| {
        let lines = text::text_lines(doc, 0).unwrap();
        let block = text::text_blocks(doc, 0).unwrap().into_iter().find(|b| b.text == "Big Title").unwrap();
        block.lines.iter().map(|i| lines[*i].rect).reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]).unwrap()
    };
    let centre = |doc: &mut Document| {
        let blocks = text::text_blocks(doc, 0).unwrap();
        let (index, block) = blocks.iter().enumerate().find(|(_, b)| b.text == "Big Title").unwrap();
        let style = text::BlockStyle { align: Some(added::Align::Center), ..Default::default() };
        text::rewrite_block(doc, 0, index, Some(&block.editor_text), &style).unwrap();
    };
    centre(&mut doc);
    let centred = title(&doc);
    assert!(((centred[0] + centred[2]) / 2.0 - (column[0] + column[1]) / 2.0).abs() < 1.0, "centred in the column: {centred:?} vs {column:?}");
    centre(&mut doc);
    assert!((title(&doc)[0] - centred[0]).abs() < 0.5, "centring again leaves it where it is: {:?} vs {centred:?}", title(&doc));
}

#[test]
fn a_titles_column_ignores_distant_full_width_headers_and_footers() {
    for (title_x, column_prefix) in [(95, "The first"), (365, "The second")] {
        for mixed in [false, true] {
            for title_y in [710, 580] {
                let title = if mixed { "(Big ) Tj 1 0 0 rg (Title) Tj" } else { "(Title) Tj" };
                let name = if mixed { "Big Title" } else { "Title" };
                let mut doc = text_page(&format!(
                    "BT /F1 10 Tf 12 TL 50 780 Td (A page-wide header extending well beyond either body column on this page and continuing all the way to the right margin.) Tj T* (A repeated page-wide heading across both columns and continuing to the right margin of the page.) Tj ET \
                 BT /F1 18 Tf 1 0 0 1 {title_x} {title_y} Tm {title} ET \
                 BT 0 g /F1 10 Tf 12 TL 50 660 Td (The first column runs to here.) Tj T* (A second row of it.) Tj ET \
                 BT /F1 10 Tf 12 TL 320 660 Td (The second column runs to here.) Tj T* (Another row.) Tj ET \
                 BT /F1 10 Tf 50 40 Td (A page-wide footer extending well beyond either body column on this page and continuing all the way to the right margin.) Tj ET"
                ));
                let rect = |doc: &Document, prefix: &str| {
                    let blocks = text::text_blocks(doc, 0).unwrap();
                    blocks.iter().find(|b| b.text.starts_with(prefix)).unwrap_or_else(|| panic!("missing {prefix:?} in {blocks:?}")).rect
                };
                let column = rect(&doc, column_prefix);
                let header = rect(&doc, "A page-wide header");
                let footer = rect(&doc, "A page-wide footer");
                assert!(header[2] > column[2] + 20.0 && footer[2] > column[2] + 20.0);
                for align in [added::Align::Center, added::Align::Center, added::Align::Right, added::Align::Left] {
                    let blocks = text::text_blocks(&doc, 0).unwrap();
                    let (index, block) = blocks.iter().enumerate().find(|(_, b)| b.text == name).unwrap();
                    let style = text::BlockStyle { align: Some(align), ..Default::default() };
                    text::rewrite_block(&mut doc, 0, index, Some(&block.editor_text), &style).unwrap();
                    doc = reopen(&doc);
                    let title = rect(&doc, name);
                    let error = match align {
                        added::Align::Center => ((title[0] + title[2]) - (column[0] + column[2])).abs() / 2.0,
                        added::Align::Right => (title[2] - column[2]).abs(),
                        _ => (title[0] - column[0]).abs(),
                    };
                    assert!(error < 1.0, "{align:?}, mixed={mixed}, title_y={title_y}: {title:?} must use {column:?}");
                    assert_eq!(rect(&doc, "A page-wide header"), header);
                    assert_eq!(rect(&doc, "A page-wide footer"), footer);
                    assert_eq!(rect(&doc, column_prefix), column);
                }
            }
        }
    }
}

/// Rewriting a paragraph drew it before its text object, restating only the font, spacing and
/// fill: a `1 Tr 1 0 0 RG 2 w` set inside `BT` was lost, and red outline text came back filled
/// black, even with the text unchanged (#3508). Set before `BT` (the control), it already worked.
#[test]
fn a_rewritten_paragraph_keeps_the_rendering_mode_stroke_and_width_set_inside_its_text_object() {
    let outline = |text: &str| Painted { text: text.into(), render_mode: 1, stroke: vec!["1 0 0 RG".into()], line_width: 2.0, gs: Vec::new() };
    for content in ["BT /F1 72 Tf 1 Tr 1 0 0 RG 2 w 40 400 Td (OUTLINE) Tj ET\n", "1 Tr 1 0 0 RG 2 w BT /F1 72 Tf 40 400 Td (OUTLINE) Tj ET\n"] {
        assert_eq!(painted_shows(&text_page(content)), [outline("OUTLINE")], "the fixture: {content}");
        for new in ["OUTLINE", "OUTLINED"] {
            let mut doc = text_page(content);
            text::rewrite_block(&mut doc, 0, 0, Some(new), &BlockStyle::default()).unwrap();
            let doc = reopen(&doc);
            assert_eq!(painted_shows(&doc), [outline(new)], "{content} -> {new}: {}", streams(&doc, 0).join("\n"));
        }
    }
}

/// The same for transparency: `/GS1 gs` (`/ca 0.2`) inside `BT` was not in force for the
/// rewritten paragraph, so 20 % blue text came back opaque (#3509).
#[test]
fn a_rewritten_paragraph_keeps_the_extgstate_selected_inside_its_text_object() {
    for content in ["BT /GS1 gs /F1 72 Tf 0 0 1 rg 40 400 Td (FAINT) Tj ET\n", "/GS1 gs BT /F1 72 Tf 0 0 1 rg 40 400 Td (FAINT) Tj ET\n"] {
        for new in ["FAINT", "FAINTER"] {
            let mut doc = faint_page(content);
            text::rewrite_block(&mut doc, 0, 0, Some(new), &BlockStyle::default()).unwrap();
            let doc = reopen(&doc);
            let shows = painted_shows(&doc);
            assert_eq!(shows.len(), 1, "{content} -> {new}: {shows:?}");
            assert_eq!(
                (shows[0].text.as_str(), shows[0].gs.as_slice()),
                (new, ["GS1".to_string()].as_slice()),
                "{content} -> {new}: {}",
                streams(&doc, 0).join("\n")
            );
        }
    }
}

/// A paragraph of differently coloured runs goes through the styled rewrite, which draws each
/// run in its own text object: each keeps the state its source set inside `BT`.
#[test]
fn a_styled_rewrite_keeps_the_paint_state_set_inside_the_text_object() {
    let content = "BT /GS1 gs 1 Tr 1 0 0 RG 2 w /F1 72 Tf 40 400 Td (FAI) Tj 0 0 1 rg (NT) Tj ET\n";
    let mut doc = faint_page(content);
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].rows[0].len(), 2, "two runs on one row");
    text::rewrite_block(&mut doc, 0, 0, Some("FAINTS"), &BlockStyle::default()).unwrap();
    let doc = reopen(&doc);
    let shows = painted_shows(&doc);
    let joined = streams(&doc, 0).join("\n");
    assert_eq!(shows.iter().map(|s| s.text.as_str()).collect::<String>(), "FAINTS", "{joined}");
    for s in &shows {
        assert_eq!((s.render_mode, s.stroke.as_slice(), s.line_width), (1, ["1 0 0 RG".to_string()].as_slice(), 2.0), "{s:?}: {joined}");
        assert_eq!(s.gs, ["GS1"], "{s:?}: {joined}");
    }
}

/// A `gs` has no inverse, so one run's ExtGState stayed in force for the runs the styled rewrite
/// drew after it. Two ExtGStates at the same opacity paint the same (see `Paint`), so their runs
/// share a paragraph, but either may set what that doesn't compare (a dash, a line cap): each run
/// is drawn with only the ones its source selected.
#[test]
fn a_styled_rewrite_does_not_carry_one_runs_extgstate_into_the_next() {
    // "FAI" ends at 40 + 72 * (0.611 + 0.667 + 0.278) = 152.032; "NT" is blue, so another run.
    let content = "q BT /GS3 gs /F1 72 Tf 40 400 Td (FAI) Tj ET Q BT /GS1 gs /F1 72 Tf 0 0 1 rg 152.032 400 Td (NT) Tj ET
";
    for new in ["FAINT", "FAINTS"] {
        let mut doc = faint_page(content);
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].rows[0].len(), 2, "two runs on one row");
        text::rewrite_block(&mut doc, 0, 0, Some(new), &BlockStyle::default()).unwrap();
        let doc = reopen(&doc);
        let shows = painted_shows(&doc);
        let joined = streams(&doc, 0).join(
            "
",
        );
        assert_eq!(shows.iter().map(|s| s.text.as_str()).collect::<String>(), new, "{joined}");
        let gs: Vec<_> = shows.iter().map(|s| (s.text.as_str(), s.gs.as_slice())).collect();
        assert_eq!(gs.first(), Some(&("FAI", ["GS3".to_string()].as_slice())), "{gs:?}: {joined}");
        assert!(gs.iter().skip(1).all(|(_, gs)| *gs == ["GS1"]), "{gs:?}: {joined}");
    }
}

/// An ExtGState's `/LW` sets the line width as `w` does: `2 w /GS2 gs` (`/LW 4`) outlines at 4,
/// and the rewrite replayed `/GS2 gs` and then `2 w`. `/GS2 gs 1 w` outlines at 1, and the
/// rewrite replayed only `/GS2 gs`, since 1 is the initial width.
#[test]
fn a_rewritten_paragraph_keeps_the_line_width_an_extgstate_set() {
    for (content, width) in [
        ("BT /F1 72 Tf 1 Tr 2 w /GS2 gs 40 400 Td (OUTLINE) Tj ET\n", 4.0),
        ("BT /F1 72 Tf 1 Tr /GS2 gs 2 w 40 400 Td (OUTLINE) Tj ET\n", 2.0),
        ("BT /F1 72 Tf 1 Tr /GS2 gs 1 w 40 400 Td (OUTLINE) Tj ET\n", 1.0),
    ] {
        assert_eq!(painted_shows(&faint_page(content))[0].line_width, width, "the fixture: {content}");
        for new in ["OUTLINE", "OUTLINED"] {
            let mut doc = faint_page(content);
            text::rewrite_block(&mut doc, 0, 0, Some(new), &BlockStyle::default()).unwrap();
            let doc = reopen(&doc);
            let shows = painted_shows(&doc);
            assert_eq!(
                shows.iter().map(|s| (s.text.as_str(), s.render_mode, s.line_width)).collect::<Vec<_>>(),
                [(new, 1, width)],
                "{content} -> {new}: {}",
                streams(&doc, 0).join("\n")
            );
        }
    }
}

/// A line is rewritten in the state of its first run, so outline text partway through a text
/// object came back filled: "FAI" filled and "NT" a red outline came back all filled. The change
/// of `Tr` now starts a run. (A `gs` partway already starts a paragraph; see `Paint`.)
#[test]
fn a_rendering_mode_partway_through_a_line_starts_a_run_of_its_own() {
    let content = "BT /F1 72 Tf 40 400 Td (FAI) Tj 1 Tr 1 0 0 RG 2 w (NT) Tj ET
";
    for new in ["FAINT", "FAINTS"] {
        let mut doc = faint_page(content);
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].rows[0].len(), 2, "two runs on one row");
        text::rewrite_block(&mut doc, 0, 0, Some(new), &BlockStyle::default()).unwrap();
        let doc = reopen(&doc);
        let shows = painted_shows(&doc);
        let joined = streams(&doc, 0).join(
            "
",
        );
        assert_eq!(shows.iter().map(|s| s.text.as_str()).collect::<String>(), new, "{joined}");
        let modes: Vec<_> = shows.iter().map(|s| (s.text.as_str(), s.render_mode, s.stroke.as_slice(), s.line_width)).collect();
        assert_eq!(modes.first().map(|m| (m.0, m.1)), Some(("FAI", 0)), "{modes:?}: {joined}");
        let outline = ["1 0 0 RG".to_string()];
        assert!(modes.iter().skip(1).all(|m| (m.1, m.2, m.3) == (1, outline.as_slice(), 2.0)), "{modes:?}: {joined}");
    }
}

/// Two ExtGStates at the same opacity paint text the same (see `Paint`), but one of them dashes
/// its outline: a dashed outline after a solid one in the same text object joined its line and
/// came back solid, drawn with the first run's selections. A different dash now starts a run.
#[test]
fn a_dash_partway_through_an_outlined_line_starts_a_run_of_its_own() {
    let content = "BT /GS3 gs /F1 72 Tf 1 Tr 1 0 0 RG 2 w 40 400 Td (FAI) Tj /GS4 gs (NT) Tj ET
";
    for new in ["FAINT", "FAINTS"] {
        let mut doc = faint_page(content);
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].rows[0].len(), 2, "two runs on one row");
        text::rewrite_block(&mut doc, 0, 0, Some(new), &BlockStyle::default()).unwrap();
        let doc = reopen(&doc);
        let shows = painted_shows(&doc);
        let joined = streams(&doc, 0).join(
            "
",
        );
        assert_eq!(shows.iter().map(|s| s.text.as_str()).collect::<String>(), new, "{joined}");
        let gs: Vec<_> = shows.iter().map(|s| (s.text.as_str(), s.gs.as_slice())).collect();
        assert_eq!(gs.first(), Some(&("FAI", ["GS3".to_string()].as_slice())), "{gs:?}: {joined}");
        assert!(gs.iter().skip(1).all(|(_, gs)| gs.last().is_some_and(|g| g == "GS4")), "{gs:?}: {joined}");
    }
}

/// The dash and cap are compared as the operators set them too: `d` or `J` partway through an
/// outlined line starts a run, and a malformed one (too long, not numbers, out of range) is ignored.
#[test]
fn a_dash_or_cap_operator_partway_through_an_outlined_line_starts_a_run() {
    let long = format!("[{}] 0 d", "1 ".repeat(40));
    for (change, runs) in [("[6 3] 0 d", 2), ("1 J", 2), (long.as_str(), 1), ("[(a) 3] 0 d", 1), ("[6 3] /x d", 1), ("7 J", 1), ("J", 1)] {
        let content = format!("BT /F1 72 Tf 1 Tr 1 0 0 RG 2 w 40 400 Td (FAI) Tj {change} (NT) Tj ET\n");
        let mut doc = faint_page(&content);
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].rows[0].len(), runs, "{change}");
        text::rewrite_block(&mut doc, 0, 0, Some("FAINTS"), &BlockStyle::default()).unwrap();
        let shows = painted_shows(&reopen(&doc));
        assert_eq!(shows.iter().map(|s| s.text.as_str()).collect::<String>(), "FAINTS", "{change}");
    }
}

/// #4199, #3797: every space typed into a paragraph is saved, not folded into one.
#[test]
fn an_edited_paragraph_keeps_every_space_typed() {
    for typed in ["Hello  world", "Hello     world   end", "  indented and trailing  "] {
        let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello world) Tj ET");
        assert_eq!(text::replace_block(&mut doc, 0, 0, typed).unwrap().substituted, None);
        let doc = reopen(&doc);
        let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
        assert!(content.contains(&format!("({typed}) Tj")), "{typed:?}: {content}");
        assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, typed);
    }
}
