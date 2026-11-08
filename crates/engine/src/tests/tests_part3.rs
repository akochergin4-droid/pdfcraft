//! Unit tests of the engine (part 3 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn printing_lays_out_sheets_that_render() {
    let (s, id) = session_with(5);
    let settings = print::Settings {
        pages: print::select_pages(5, Some("2-5"), &[], print::Subset::All, false).unwrap(),
        layout: print::Layout::multiple(2),
        ..Default::default()
    };
    let bytes = s.print_pdf(id, &settings).unwrap();
    let mut s2 = Session::new();
    let id2 = s2.open("print.pdf", None, Arc::new(bytes), None).unwrap();
    assert_eq!(s2.get(id2).unwrap().info.pages.len(), 2, "four pages, two per sheet");
    let texts = page_texts(&s2, id2);
    assert!(texts[0].contains("Page 2") && texts[0].contains("Page 3") && texts[1].contains("Page 5"), "{texts:?}");
    // Printing honours the permissions.
    let p =
        Protection { open_password: Some("pw".into()), permissions_password: Some("owner".into()), printing: Printing::None, ..Default::default() };
    let (mut s3, id3) = session_with(1);
    s3.apply(id3, Edit::Protect(p)).unwrap();
    let saved = s3.save_bytes(id3).unwrap();
    let mut s4 = Session::new();
    let id4 = s4.open("locked.pdf", None, saved, Some("pw")).unwrap();
    assert_eq!(s4.print_pdf(id4, &print::Settings { pages: vec![0], ..Default::default() }), Err(EditError::NotPermitted("printing")));
}

#[test]
fn a_layer_that_is_off_does_not_print_and_one_that_is_on_does() {
    for (off, prints) in [(true, false), (false, true)] {
        let source = red_layer_pdf(off);
        assert_eq!(first_page_has_red(source.clone()), prints, "on screen (off: {off})");
        let mut s = Session::new();
        let id = s.open("layers.pdf", None, Arc::new(source), None).unwrap();
        let sheet = s.print_pdf(id, &print::Settings { pages: vec![0], ..Default::default() }).unwrap();
        assert_eq!(first_page_has_red(sheet), prints, "printed (layer off by default: {off})");
    }
}

#[test]
fn crop_and_duplicate_pages_show_in_the_viewer() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::SetPageBox { pages: vec![0], which: PageBox::Crop, spec: BoxSpec::Margins([10.0, 20.0, 30.0, 40.0]) }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.can_undo(), Some("Crop page"));
    assert_eq!(d.info.pages[0].crop, [10.0, 20.0, 170.0, 260.0]);
    assert_eq!((d.info.pages[0].width, d.info.pages[0].height), (160.0, 240.0), "the viewer shows the cropped size");
    assert_eq!(d.page_boxes()[1][1], [0.0, 0.0, 200.0, 300.0]);
    s.apply(id, Edit::DuplicatePages { pages: vec![1], at: None }).unwrap();
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2", "Page 2"]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Duplicate page"));
}

#[test]
fn headers_footers_watermarks_and_backgrounds_show_update_and_remove() {
    let (mut s, id) = session_with(2);
    let mut hf = HeaderFooter::default();
    hf.text[4] = "Page <<1>> of <<n>> · <<yyyy-mm-dd>>".into();
    s.apply(id, Edit::AddHeaderFooter { pages: vec![0, 1], settings: hf.clone(), replace: false }).unwrap();
    // The session clock is 2023-11-14.
    assert_eq!(page_texts(&s, id)[1], "Page 2\nPage 2 of 2 · 2023-11-14");
    assert_eq!(s.get(id).unwrap().marks(), [MarkKind::HeaderFooter]);
    hf.text[4] = "Draft".into();
    s.apply(id, Edit::AddHeaderFooter { pages: vec![0, 1], settings: hf, replace: true }).unwrap();
    assert_eq!(page_texts(&s, id)[0], "Page 1\nDraft");
    s.apply(
        id,
        Edit::AddWatermark { pages: vec![0], settings: Watermark { text: "SECRET".into(), ..Watermark::default() }, file: None, replace: false },
    )
    .unwrap();
    s.apply(
        id,
        Edit::AddBackground {
            pages: vec![1],
            settings: Background { color: [1.0, 0.9, 0.9], opacity: 1.0, ..Background::default() },
            replace: false,
            file: None,
        },
    )
    .unwrap();
    assert_eq!(s.get(id).unwrap().marks().len(), 3);
    assert!(page_texts(&s, id)[0].contains("SECRET"));
    s.apply(id, Edit::RemoveMarks { kind: MarkKind::HeaderFooter }).unwrap();
    s.apply(id, Edit::RemoveMarks { kind: MarkKind::Watermark }).unwrap();
    s.apply(id, Edit::RemoveMarks { kind: MarkKind::Background }).unwrap();
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2"]);
    assert!(s.get(id).unwrap().marks().is_empty());
    assert!(matches!(s.apply(id, Edit::RemoveMarks { kind: MarkKind::Watermark }), Err(EditError::Edit(_))));
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Remove background"));
}

/// Remove background says which pages keep their own white backdrop off (it couldn't be turned
/// back on safely), and says nothing when every backdrop comes back.
#[test]
fn remove_background_names_the_pages_whose_own_backdrop_stays_off() {
    let mut s = Session::new();
    // A tag whose hash doesn't match the bytes before the fill: something changed them since.
    let tag = format!("<< /Index 2 /Op (f) /Before <{}> >>", "00".repeat(32));
    let id = s.open("a.pdf", None, Arc::new(backdrop_pdf(Some(&tag))), None).unwrap();
    s.apply(id, Edit::RemoveMarks { kind: MarkKind::Background }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.edit_notices, [EditNotice::BackdropsLeftOff { pages: vec![0] }]);
    let said = d.edit_notices[0].to_string();
    assert!(said.starts_with("page 1 changed after the background was added"), "{said}");
    // An unrelated edit after it says nothing.
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    assert!(s.get(id).unwrap().edit_notices.is_empty());

    // Added and removed here: the backdrop comes back, and nothing is said.
    let id = s.open("b.pdf", None, Arc::new(backdrop_pdf(None)), None).unwrap();
    s.apply(id, Edit::RemoveMarks { kind: MarkKind::Background }).unwrap();
    let background = Background { color: [1.0, 0.0, 0.0], ..Background::default() };
    s.apply(id, Edit::AddBackground { pages: vec![0], settings: background, replace: false, file: None }).unwrap();
    assert_eq!(s.get(id).unwrap().page_backdrops(&[0]), [None], "replaced");
    s.apply(id, Edit::RemoveMarks { kind: MarkKind::Background }).unwrap();
    assert!(s.get(id).unwrap().edit_notices.is_empty());
    assert_eq!(s.get(id).unwrap().page_backdrops(&[0]), [Some(Backdrop::Fill)], "turned back on");
    assert_eq!(page_runs(&[0, 1, 2, 5, 7, 8]), "1-3, 6, 8-9");
    assert_eq!(page_runs(&[]), "");
}

/// Update previews its marks on the page without the ones it replaces: a yellow background
/// updated to blue isn't previewed as blue over yellow, nor a new watermark over the old one.
/// The other kinds of mark and the page's own content stay, every page is prepared at once (the
/// dialog steps through them), a document without that kind has nothing to prepare, and
/// preparing changes nothing in the document.
#[test]
fn update_previews_on_the_page_without_the_marks_it_replaces() {
    let (mut s, id) = session_with(2);
    let yellow = Background { color: [1.0, 1.0, 0.0], ..Background::default() };
    s.apply(id, Edit::AddBackground { pages: vec![0, 1], settings: yellow, replace: false, file: None }).unwrap();
    let red = Watermark { text: "SECRET".into(), color: [1.0, 0.0, 0.0], opacity: 1.0, ..Watermark::default() };
    s.apply(id, Edit::AddWatermark { pages: vec![0], settings: red, replace: false, file: None }).unwrap();
    let doc = s.get(id).unwrap();
    let (generation, dirty) = (doc.edit_generation(), doc.dirty);
    let px = |p: &pdfcraft_render::RenderedPage, x: u32, y: u32| -> [u8; 3] {
        p.rgba[(((300 - y) * p.width + x) * 4) as usize..][..3].try_into().unwrap()
    };
    let reds = |p: &pdfcraft_render::RenderedPage| p.rgba.as_chunks::<4>().0.iter().filter(|[r, g, b, _]| *r > 200 && *g < 80 && *b < 80).count();
    let dark = |p: &pdfcraft_render::RenderedPage| p.rgba.as_chunks::<4>().0.iter().filter(|[r, g, b, _]| *r < 100 && *g < 100 && *b < 100).count();
    assert_eq!(pixel(&s, id, 0, 5, 5)[..3], [255, 255, 0], "the page has the yellow background");

    let mut unmarked = doc.without_marks(MarkKind::Background).unwrap().expect("the pages have a background");
    let page = unmarked.render_page(0, 1.0).unwrap();
    assert_eq!(px(&page, 5, 5), [255, 255, 255], "without the background the page is white");
    assert!(reds(&page) > 50, "the watermark stays");
    assert!(dark(&page) > 10, "the page text stays");

    let page = doc.without_marks(MarkKind::Watermark).unwrap().expect("page 1 has a watermark").render_page(0, 1.0).unwrap();
    assert_eq!(reds(&page), 0, "without the watermark");
    assert_eq!(px(&page, 5, 5), [255, 255, 0], "the background stays");

    assert_eq!(px(&unmarked.render_page(1, 1.0).unwrap(), 5, 5), [255, 255, 255], "made once, for every page: page 2 is white too");
    assert_eq!(unmarked.parser_builds(), 1, "parsed once for both pages");
    assert!(doc.without_marks(MarkKind::HeaderFooter).unwrap().is_none(), "the document has no header");
    assert!(unmarked.render_page(9, 1.0).is_err(), "no page 10");
    let doc = s.get(id).unwrap();
    assert_eq!((doc.edit_generation(), doc.dirty, doc.can_undo()), (generation, dirty, Some("Add watermark")), "the document is unchanged");
}

#[test]
fn marker_indexes_match_after_open_and_save_when_requested() {
    let (mut source, source_id) = session_with(1);
    let mut header = HeaderFooter::default();
    header.text[0] = "Header".into();
    source.apply(source_id, Edit::AddHeaderFooter { pages: vec![0], settings: header, replace: false }).unwrap();
    source
        .apply(
            source_id,
            Edit::AddWatermark { pages: vec![0], settings: Watermark { text: "Draft".into(), ..Watermark::default() }, file: None, replace: false },
        )
        .unwrap();
    source
        .apply(
            source_id,
            Edit::AddBackground {
                pages: vec![0],
                settings: Background { color: [0.9, 0.9, 1.0], opacity: 1.0, ..Background::default() },
                replace: false,
                file: None,
            },
        )
        .unwrap();
    source
        .apply(
            source_id,
            Edit::AddText { page: 0, text: AddedText { rect: [20.0, 250.0, 180.0, 280.0], text: "Added note".into(), ..Default::default() } },
        )
        .unwrap();

    let bytes = source.save_bytes(source_id).unwrap();
    let reference = pdfcraft_cos::Document::open(bytes.clone()).unwrap();
    let expected_marks = pdfcraft_edit::marks_present(&reference);
    let expected_added = pdfcraft_edit::list_added(&reference);
    source.mark_saved(source_id, bytes.clone(), None).unwrap();
    let refreshed = source.get(source_id).unwrap();
    assert_eq!(refreshed.marks(), expected_marks);
    assert_eq!(refreshed.added(), expected_added);
    let mut reopened = Session::new();
    let reopened_id = reopened.open("marked.pdf", None, bytes, None).unwrap();
    let opened = reopened.get(reopened_id).unwrap();
    assert_eq!(opened.marks(), expected_marks);
    assert_eq!(opened.added(), expected_added);
    assert_eq!(opened.marks().len(), 3);
    assert!(opened.marks().contains(&MarkKind::HeaderFooter));
    assert!(opened.marks().contains(&MarkKind::Watermark));
    assert!(opened.marks().contains(&MarkKind::Background));
    assert_eq!(opened.added().len(), 1);
}

#[test]
fn secondary_indexes_load_once_and_follow_edit_scopes() {
    reset_secondary_scan_counts();
    let (mut session, id) = session_with(1);
    assert_eq!(secondary_scan_counts(), (0, 0), "open does not scan markers or links");

    {
        let doc = session.get(id).unwrap();
        assert!(doc.marks().is_empty());
        assert!(doc.added().is_empty());
        assert!(doc.added().is_empty());
        assert_eq!(secondary_scan_counts(), (1, 0), "marks and added content share one scan");
        assert!(doc.links().is_empty());
        assert!(doc.links().is_empty());
        assert_eq!(secondary_scan_counts(), (1, 1), "repeated reads reuse the link list");
    }
    metadata_and_comment_edits_keep_the_indexes(&mut session, id);
    link_edits_rebuild_only_the_link_list(&mut session, id);
    content_and_marker_edits_rebuild_the_markers(&mut session, id);
}

#[test]
fn export_images_and_text_follow_the_working_file() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: 90 }).unwrap();
    let doc = s.get(id).unwrap();
    let mut ex = export::Exporter::new(doc);
    let png = ex.png(1, 144.0).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // 200×300 pt rotated → 300×200 pt at 2 px/pt.
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
    assert_eq!((w, h), (600, 400));
    assert_eq!(ex.text_of(&[0, 1]).unwrap(), "Page 1\n\u{c}Page 2\n");
    assert!(ex.png(5, 72.0).is_err());
}

#[test]
fn create_and_reduce() {
    let mut s = Session::new();
    let blank = s.create_blank(612.0, 792.0, 2).unwrap();
    let id = s.open_new("Untitled.pdf", blank).unwrap();
    assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
    let text = s.create_from_text("notes", "hello\nworld").unwrap();
    let t = s.open_new("notes.pdf", text).unwrap();
    assert_eq!(page_texts(&s, t), ["hello\nworld"]);
    // A file with the same page imported twice shares nothing until reduced.
    let (mut s2, id2) = session_with(1);
    let src = Arc::new(fixture(1));
    s2.apply(
        id2,
        Edit::InsertPagesFrom {
            name: "x".into(),
            bytes: src.clone(),
            pages: None,
            at: 1,
            field_conflicts: pdfcraft_organize::FieldConflicts::Rename,
        },
    )
    .unwrap();
    let full = s2.save_full_bytes(id2).unwrap();
    let (reduced, ..) = s2.reduced_bytes(id2).unwrap();
    assert!(reduced.len() <= full.len(), "{} > {}", reduced.len(), full.len());
    let mut s3 = Session::new();
    let r = s3.open("r.pdf", None, reduced, None).unwrap();
    assert_eq!(page_texts(&s3, r), ["Page 1", "Page 1"]);
}

/// Discarding a category that holds nothing discards nothing; it used to end the whole
/// optimization with an error about redaction marks.
#[test]
fn optimize_discarding_an_empty_category_still_runs() {
    let (s, id) = session_with(2);
    let (bytes, report) = s.optimized_bytes(id, &optimize::Settings::default(), &[crate::Hidden::Bookmarks, crate::Hidden::Comments]).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(report.discarded.is_empty(), "{:?}", report.discarded);
    // An outline root with no entries is no bookmarks: it stays, as Remove Hidden Information,
    // which refuses then, leaves it.
    let outline = pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R /PageMode /UseOutlines >>")
        .obj("<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>")
        .obj("<< /Type /Page /Parent 2 0 R >>")
        .obj("<< /Type /Outlines /Count 0 >>")
        .build();
    let mut s = Session::new();
    let id = s.open("outline.pdf", None, Arc::new(outline), None).unwrap();
    let (bytes, report) = s.optimized_bytes(id, &optimize::Settings::default(), &[crate::Hidden::Bookmarks]).unwrap();
    assert!(report.discarded.is_empty(), "{:?}", report.discarded);
    let saved = pdfcraft_cos::Document::open(bytes).unwrap();
    let catalog = saved.get(saved.root().unwrap()).as_dict().cloned().unwrap();
    assert!(catalog.contains(b"Outlines") && catalog.contains(b"PageMode"), "{catalog:?}");
    // Beside an empty tick, one that holds something is discarded, and only that one.
    let noted = pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R >>")
        .obj("<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>")
        .obj("<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>")
        .obj("<< /Type /Annot /Subtype /Text /Rect [20 20 40 40] /Contents (internal note) >>")
        .build();
    let mut s = Session::new();
    let id = s.open("noted.pdf", None, Arc::new(noted), None).unwrap();
    let (_, report) = s.optimized_bytes(id, &optimize::Settings::default(), &[crate::Hidden::Bookmarks, crate::Hidden::Comments]).unwrap();
    assert_eq!(report.discarded, vec![(crate::Hidden::Comments, 1)]);
    // A field whose appearance draws text invisibly: flattening it brings that text onto the page,
    // which no scan before counts as hidden, and discarding hidden text takes it too.
    let form = pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] /DA (/F1 0 Tf 0 g) >> >>")
        .obj("<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>")
        .obj("<< /Type /Page /Parent 2 0 R /Annots [4 0 R] /Resources << /Font << /F1 6 0 R >> >> /Contents 7 0 R >>")
        .obj("<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (shown) /Rect [100 100 300 130] /P 3 0 R /F 4 /AP << /N 5 0 R >> >>")
        .stream(
            "/Type /XObject /Subtype /Form /BBox [0 0 200 30] /Resources << /Font << /F1 6 0 R >> >>",
            "/Tx BMC BT /F1 12 Tf 2 10 Td (shown) Tj ET BT /F1 12 Tf 3 Tr 2 20 Td (HIDDENSECRET) Tj ET EMC",
        )
        .obj("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        .stream("", "BT /F1 12 Tf 72 700 Td (Page text) Tj ET")
        .build();
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(form), None).unwrap();
    let (bytes, _) = s.optimized_bytes(id, &optimize::Settings::default(), &[crate::Hidden::FormFields, crate::Hidden::HiddenText]).unwrap();
    let saved = pdfcraft_cos::Document::open(bytes).unwrap();
    for (r, object) in saved.scan_objects() {
        if let pdfcraft_cos::Object::Stream(st) = &*object {
            let data = st.decoded().unwrap_or_default();
            assert!(!data.windows(12).any(|w| w == b"HIDDENSECRET"), "the hidden text stays in {r:?}");
        }
    }
}

#[test]
fn an_optimize_job_reports_its_stages_and_can_be_cancelled() {
    use crate::optimizer::OptimizeStage;
    let (s, id) = session_with(2);
    let settings = optimize::Settings::default();
    let mut stages = Vec::new();
    let (bytes, _) = s
        .optimize_job(id, &settings, &[])
        .unwrap()
        .run(|st| {
            stages.push(st);
            true
        })
        .unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(stages.first(), Some(&OptimizeStage::Discarding));
    assert_eq!(&stages[stages.len() - 3..], [OptimizeStage::CleaningUp, OptimizeStage::Merging, OptimizeStage::Writing]);
    let fractions: Vec<f32> = stages.iter().map(|st| st.fraction()).collect();
    assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "the bar never goes back: {fractions:?}");
    assert!(fractions.iter().all(|f| (0.0..=1.0).contains(f)));
    assert_eq!(
        OptimizeStage::Images { done: 0, total: 0 }.fraction(),
        OptimizeStage::Images { done: 5, total: 5 }.fraction(),
        "no images: no division by zero"
    );

    // Cancelled at the merge: nothing is written, the open document is untouched.
    let before = s.get(id).unwrap().bytes.clone();
    let r = s.optimize_job(id, &settings, &[]).unwrap().run(|st| st != OptimizeStage::Merging);
    assert!(matches!(r, Err(EditError::Cancelled)), "{r:?}");
    assert_eq!(s.get(id).unwrap().bytes, before);
}

#[test]
fn flattening_keeps_the_look_and_drops_the_objects() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, Arc::new(form_fixture()), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "name".into(), value: FieldValue::Text("Ada".into()) }).unwrap();
    s.apply(id, rect_comment(0, [80.0, 80.0, 120.0, 120.0])).unwrap();
    let red_before = pixel(&s, id, 0, 100, 100);
    s.apply(id, Edit::Flatten { comments: true, fields: true }).unwrap();
    let d = s.get(id).unwrap();
    assert!(d.info.annotations.is_empty() && d.form.is_empty(), "no comments or fields remain");
    assert_eq!(pixel(&s, id, 0, 100, 100), red_before, "the rectangle is now page content");
    assert_eq!(page_texts(&s, id), ["Ada"], "the field's text is now page content");
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().form.len(), 2);
}

/// A stamp whose reply is locked can still be dragged, so its layers are still prepared: the page
/// beneath is drawn on a copy, where locks don't count. Deleting the stamp is what they refuse.
#[test]
fn a_stamp_with_a_locked_reply_still_has_its_drag_layers() {
    let (mut s, id) = session_with(1);
    s.apply(id, fill_item(Shape::Mark { rect: [30.0, 60.0, 130.0, 66.0], mark: FillMark::Line }, "")).unwrap();
    s.apply(id, Edit::ReplyToAnnotation { page: 0, index: 0, text: "Approved".into(), author: "Bob".into() }).unwrap();
    let reply = s.get(id).unwrap().info.annotations.iter().find(|a| a.parent == Some((0, 0))).expect("the reply").index;
    s.apply(id, Edit::LockAnnotation { page: 0, index: reply, locked: true }).unwrap();
    let doc = s.get(id).unwrap();
    let background = doc.stamp_preview(0, 0).unwrap().expect("the line is a stamp").render_background(1.0).unwrap();
    let px = |p: &pdfcraft_render::RenderedPage, x: u32, y: u32| -> [u8; 4] {
        p.rgba[(((300 - y) * p.width + x) * 4) as usize..][..4].try_into().unwrap()
    };
    assert!((30..=130).all(|x| px(&background, x, 63) == [255, 255, 255, 255]), "the page beneath has no line");
    assert!(s.apply(id, Edit::DeleteAnnotation { page: 0, index: 0 }).is_err(), "the locked reply keeps the stamp");
}

/// A dragged stamp's layers: the page without it (its text and other comments kept) and the
/// stamp alone on a transparent page. Preparing them changes nothing in the document.
#[test]
fn stamp_preview_layers_split_the_stamp_from_its_page() {
    let (mut s, id) = session_with(1);
    s.apply(id, fill_item(Shape::Mark { rect: [30.0, 60.0, 130.0, 66.0], mark: FillMark::Line }, "")).unwrap();
    s.apply(id, rect_comment(0, [140.0, 40.0, 190.0, 90.0])).unwrap();
    let doc = s.get(id).unwrap();
    let (generation, dirty) = (doc.edit_generation(), doc.dirty);
    let preview = doc.stamp_preview(0, 0).unwrap().expect("the line is a stamp");
    let (stamp, background) = (preview.render_stamp(1.0).unwrap(), preview.render_background(1.0).unwrap());
    let px = |p: &pdfcraft_render::RenderedPage, x: u32, y: u32| -> [u8; 4] {
        p.rgba[(((300 - y) * p.width + x) * 4) as usize..][..4].try_into().unwrap()
    };
    let line = |x: u32, y: u32| (30..=130).contains(&x) && (59..=67).contains(&y);
    let (mut drawn, mut stray) = (0, Vec::new());
    for y in 1..300 {
        for x in 0..200 {
            match (line(x, y), px(&stamp, x, y)) {
                (true, [.., 255]) => drawn += 1,
                (false, v) if v != [0, 0, 0, 0] => stray.push((x, y)),
                _ => {}
            }
        }
    }
    assert!(drawn > 100, "the line is drawn opaque: {drawn} pixels");
    assert!(stray.is_empty(), "only the line is drawn on its layer: {:?}", &stray[..stray.len().min(8)]);
    assert!((30..=130).all(|x| px(&background, x, 63) == [255, 255, 255, 255]), "the page beneath has no line");
    assert!((138..=142).any(|x| px(&background, x, 65) != [255, 255, 255, 255]), "other comments stay");
    assert!((20..120).any(|x| (150..170).any(|y| px(&background, x, y)[0] < 128)), "the page text stays");
    assert!(doc.stamp_preview(0, 1).unwrap().is_none(), "a rectangle isn't a stamp");
    assert!(doc.stamp_preview(0, 9).unwrap().is_none());
    let doc = s.get(id).unwrap();
    assert_eq!((doc.edit_generation(), doc.dirty, doc.can_undo()), (generation, dirty, Some("Add rectangle")), "the document is unchanged");

    // Another app's comments without appearances: beneath a dragged stamp they are drawn as on
    // screen, and such a stamp has nothing to lay over the page.
    let bare = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R] >> endobj
4 0 obj << /Type /Annot /Subtype /Square /Rect [140 40 190 90] /C [1 0 0] >> endobj
5 0 obj << /Type /Annot /Subtype /Stamp /Rect [20 20 80 40] /Name /Draft >> endobj
trailer << /Root 1 0 R >>
%%EOF";
    let mut s = Session::new();
    let id = s.open("bare.pdf", None, Arc::new(bare.to_vec()), None).unwrap();
    s.apply(id, fill_item(Shape::Mark { rect: [30.0, 60.0, 130.0, 66.0], mark: FillMark::Line }, "")).unwrap();
    let doc = s.get(id).unwrap();
    let background = doc.stamp_preview(0, 2).unwrap().expect("the line is a stamp").render_background(1.0).unwrap();
    assert!((138..=142).any(|x| px(&background, x, 65)[..3] == [255, 0, 0]), "the square without an appearance is drawn beneath");
    assert!(doc.stamp_preview(0, 1).unwrap().is_none(), "a stamp without an appearance");
}

/// A moved comment's layer: the comment alone on a transparent page (no page text, no other
/// comment), drawn as on screen, another app's comment without an appearance too. Preparing it
/// changes nothing in the document.
#[test]
fn comment_layer_draws_the_comment_alone() {
    let (mut s, id) = session_with(1);
    s.apply(id, rect_comment(0, [140.0, 40.0, 190.0, 90.0])).unwrap();
    s.apply(id, rect_comment(0, [20.0, 20.0, 60.0, 60.0])).unwrap();
    let doc = s.get(id).unwrap();
    let (generation, dirty) = (doc.edit_generation(), doc.dirty);
    let layer = doc.comment_layer(0, &[0]).unwrap().expect("a rectangle").render(1.0).unwrap();
    let px = |p: &pdfcraft_render::RenderedPage, x: u32, y: u32| -> [u8; 4] {
        p.rgba[(((300 - y) * p.width + x) * 4) as usize..][..4].try_into().unwrap()
    };
    assert_eq!(px(&layer, 165, 65), [255, 0, 0, 255], "the rectangle is drawn");
    let near = |x: u32, y: u32| (137..=193).contains(&x) && (37..=93).contains(&y);
    let stray: Vec<(u32, u32)> =
        (1..300).flat_map(|y| (0..200).map(move |x| (x, y))).filter(|&(x, y)| !near(x, y) && px(&layer, x, y) != [0, 0, 0, 0]).collect();
    assert!(stray.is_empty(), "only the rectangle is on its layer: {:?}", &stray[..stray.len().min(8)]);
    assert!(doc.comment_layer(0, &[9]).unwrap().is_none(), "no such comment");
    let both = doc.comment_layer(0, &[0, 1]).unwrap().expect("both").render(1.0).unwrap();
    assert_eq!((px(&both, 165, 65), px(&both, 40, 40)), ([255, 0, 0, 255], [255, 0, 0, 255]), "moved together, drawn together");
    let doc = s.get(id).unwrap();
    assert_eq!((doc.edit_generation(), doc.dirty, doc.can_undo()), (generation, dirty, Some("Add rectangle")), "the document is unchanged");

    let bare = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /Annots [4 0 R] >> endobj
4 0 obj << /Type /Annot /Subtype /Square /Rect [140 40 190 90] /C [1 0 0] >> endobj
trailer << /Root 1 0 R >>
%%EOF";
    let mut s = Session::new();
    let id = s.open("bare.pdf", None, Arc::new(bare.to_vec()), None).unwrap();
    let layer = s.get(id).unwrap().comment_layer(0, &[0]).unwrap().expect("drawn as on screen").render(1.0).unwrap();
    assert!((138..=142).any(|x| px(&layer, x, 65)[..3] == [255, 0, 0]), "the square without an appearance is drawn");
}

#[test]
fn flattening_fill_and_sign_keeps_other_comments_fields_and_undoes() {
    let (mut s, id) = session_with(1);
    s.apply(id, fill_item(Shape::Typewriter { rect: [20.0, 200.0, 120.0, 220.0], font_size: 10.0 }, "Hello")).unwrap();
    s.apply(id, fill_item(Shape::Mark { rect: [140.0, 200.0, 160.0, 220.0], mark: FillMark::Check }, "")).unwrap();
    s.apply(id, fill_item(Shape::Signature { strokes: vec![vec![[10.0, 40.0], [40.0, 70.0], [70.0, 40.0]]] }, "")).unwrap();
    s.apply(id, fill_item(Shape::TypedSignature { rect: [90.0, 40.0, 150.0, 70.0], contours: vec![vec![[0.1, 0.2], [0.5, 0.9], [0.9, 0.2]]] }, ""))
        .unwrap();
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 8, 8);
        enc.set_color(png::ColorType::Rgba);
        let pixel = [40u8, 60, 80, 200];
        enc.write_header().unwrap().write_image_data(&pixel.repeat(64)).unwrap();
    }
    let image = SignatureImage::from_bytes(&png).unwrap();
    let info = s.get(id).unwrap().info.pages[0].clone();
    s.apply(id, image.edit(0, &info, [20.0, 120.0], false, "Ada").unwrap()).unwrap();
    s.apply(id, fill_item(Shape::TextMarkup { kind: Markup::Highlight, quads: vec![[20.0, 280.0, 80.0, 280.0, 20.0, 270.0, 80.0, 270.0]] }, "keep"))
        .unwrap();
    s.apply(id, fill_item(Shape::TextBox { rect: [20.0, 150.0, 120.0, 180.0], font_size: 12.0 }, "Note")).unwrap();
    s.apply(id, Edit::AddField { page: 0, rect: [20.0, 230.0, 120.0, 250.0], kind: NewField::Text { multiline: false }, name: Some("email".into()) })
        .unwrap();

    s.apply(id, Edit::FlattenFillSign).unwrap();
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Flatten Fill & Sign"));
    assert_eq!(comment_text(&s, id), ["Note", "keep"]);
    assert_eq!(s.get(id).unwrap().form.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["email"]);
    s.undo(id).unwrap();
    assert!(comment_text(&s, id).contains(&"Hello".to_string()), "undo restores the typewriter");

    s.apply(id, Edit::FlattenFillSign).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved.clone(), None).unwrap();
    let mut again = Session::new();
    let id2 = again.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(comment_text(&again, id2), ["Note", "keep"]);
    assert_eq!(again.get(id2).unwrap().form.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["email"]);
}

#[test]
fn flattening_fill_and_sign_does_nothing_when_there_are_none() {
    let (mut s, id) = session_with(1);
    s.apply(id, fill_item(Shape::TextMarkup { kind: Markup::Highlight, quads: vec![[20.0, 200.0, 80.0, 200.0, 20.0, 190.0, 80.0, 190.0]] }, "keep"))
        .unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    assert!(!s.get(id).unwrap().dirty);
    s.apply(id, Edit::FlattenFillSign).unwrap();
    assert!(!s.get(id).unwrap().dirty, "nothing was baked in, so the document stays saved");
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add highlight"), "no undo step for a flatten that changed nothing");
    assert_eq!(comment_text(&s, id), ["keep"]);
}

#[test]
fn flattening_fill_and_sign_is_refused_without_permission_to_change() {
    let (mut s, id) = session_with(1);
    s.apply(id, fill_item(Shape::Typewriter { rect: [20.0, 200.0, 120.0, 220.0], font_size: 10.0 }, "Hello")).unwrap();
    let p = Protection { printing: Printing::Low, changes: Changes::CommentFillSign, copy: false, ..protection(None, Some("boss")) };
    s.apply(id, Edit::Protect(p)).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let mut other = Session::new();
    let id2 = other.open("locked.pdf", None, saved, None).unwrap();
    let before = other.get(id2).unwrap().bytes.clone();
    assert_eq!(other.apply(id2, Edit::FlattenFillSign), Err(EditError::NotPermitted("changes to the document")));
    assert_eq!(other.get(id2).unwrap().bytes, before);
    assert!(!other.get(id2).unwrap().dirty);
    assert!(comment_text(&other, id2).contains(&"Hello".to_string()));
}

#[test]
fn flattening_fill_and_sign_on_a_signed_file_stays_incremental() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
    let opts = SignOptions { page: 0, rect: Some([20.0, 20.0, 160.0, 50.0]), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    s.mark_signed(id, signed.clone(), None).unwrap();
    s.apply(id, fill_item(Shape::Typewriter { rect: [20.0, 200.0, 120.0, 220.0], font_size: 10.0 }, "Hello")).unwrap();
    s.apply(id, Edit::FlattenFillSign).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert!(saved.starts_with(signed.as_ref()), "the signature's bytes stay an incremental update");
    assert!(!comment_text(&s, id).contains(&"Hello".to_string()));
}

#[test]
fn flattening_fill_and_sign_reopens_an_encrypted_file() {
    let (mut s, id) = session_with(1);
    s.apply(id, fill_item(Shape::Typewriter { rect: [20.0, 200.0, 120.0, 220.0], font_size: 10.0 }, "Hello")).unwrap();
    s.apply(id, Edit::Protect(protection(Some("secret"), None))).unwrap();
    s.apply(id, Edit::FlattenFillSign).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let mut again = Session::new();
    assert!(again.open("p.pdf", None, saved.clone(), None).is_err());
    let id2 = again.open("p.pdf", None, saved, Some("secret")).unwrap();
    assert!(!comment_text(&again, id2).contains(&"Hello".to_string()));
}

#[test]
fn pages_export_as_png_jpeg_and_tiff() {
    use crate::export::{Exporter, ImageFormat};
    let (s, id) = session_with(1);
    let mut ex = Exporter::new(s.get(id).unwrap());
    let png = ex.image(0, 72.0, ImageFormat::Png).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    let jpg = ex.image(0, 72.0, ImageFormat::Jpeg { quality: 85 }).unwrap();
    let decoded = image::load_from_memory_with_format(&jpg, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (200, 300), "the page at 72 dpi");
    let corner = decoded.to_rgb8().get_pixel(2, 2).0;
    assert!(corner.iter().all(|c| *c > 240), "white paper, not black: {corner:?}");
    let tif = ex.image(0, 144.0, ImageFormat::Tiff).unwrap();
    let decoded = image::load_from_memory_with_format(&tif, image::ImageFormat::Tiff).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (400, 600));
    let mut tags = tiff::decoder::Decoder::new(std::io::Cursor::new(&tif)).unwrap();
    let compression = tags.get_tag_u32(tiff::tags::Tag::Compression).unwrap();
    let predictor = tags.get_tag_u32(tiff::tags::Tag::Predictor).unwrap();
    assert_eq!((compression, predictor), (5, 1), "LZW, without a predictor");
}

/// Export ▸ Image draws several pages at once: each file is the one exporting that page alone
/// writes, handed over in the order asked for, whatever the number of pages drawn at once.
#[test]
fn pages_exported_at_once_are_the_files_each_page_exports_alone_in_order() {
    use crate::export::{Exporter, ImageFormat};
    let (s, id) = session_with(12);
    let pages = [3, 0, 11, 5, 5, 7, 1, 2, 10, 9, 8, 6, 4];
    for format in [ImageFormat::Png, ImageFormat::Jpeg { quality: 85 }, ImageFormat::Tiff] {
        let alone: Vec<(usize, Vec<u8>)> = pages.iter().map(|&p| (p, Exporter::new(s.get(id).unwrap()).image(p, 96.0, format).unwrap())).collect();
        for workers in [1, 2, 4, 8] {
            let mut got = Vec::new();
            let mut ex = Exporter::new(s.get(id).unwrap());
            ex.images_on(workers, &pages, 96.0, format, |p, img| {
                got.push((p, img));
                Ok(())
            })
            .unwrap();
            assert!(got == alone, "{} with {workers} workers", format.label());
        }
        let mut got = Vec::new();
        Exporter::new(s.get(id).unwrap())
            .images(&pages, 96.0, format, |p, img| {
                got.push((p, img));
                Ok(())
            })
            .unwrap();
        assert!(got == alone, "{}", format.label());
    }
}

/// A page that can't be exported, or a file that can't be written, stops the export there with
/// its error: every page before it is handed over, none after it.
#[test]
fn exporting_pages_at_once_stops_at_the_first_error() {
    use crate::export::{Exporter, ImageFormat};
    let (s, id) = session_with(6);
    for workers in [1, 4] {
        let mut ex = Exporter::new(s.get(id).unwrap());
        let mut got = Vec::new();
        let result = ex.images_on(workers, &[0, 1, 99, 2, 3], 72.0, ImageFormat::Png, |p, _| {
            got.push(p);
            Ok(())
        });
        assert_eq!(result, Err("page 100 does not exist".to_string()), "{workers} workers");
        assert_eq!(got, [0, 1], "{workers} workers");

        let mut got = Vec::new();
        let result = ex.images_on(workers, &[0, 1, 2, 3, 4, 5], 72.0, ImageFormat::Png, |p, _| {
            got.push(p);
            if p == 2 { Err("disk full".to_string()) } else { Ok(()) }
        });
        assert_eq!(result, Err("disk full".to_string()), "{workers} workers");
        assert_eq!(got, [0, 1, 2], "{workers} workers");
        // The exporter is still usable afterwards.
        assert!(ex.image(5, 72.0, ImageFormat::Png).is_ok());
    }
    // A panicking sink stops the workers rather than leaving them waiting for it.
    let mut ex = Exporter::new(s.get(id).unwrap());
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ex.images_on(4, &[0, 1, 2, 3, 4, 5], 72.0, ImageFormat::Png, |p, _| if p == 1 { panic!("sink") } else { Ok(()) })
    }));
    assert!(panicked.is_err());
}

/// An exported TIFF or JPEG says the resolution it was drawn at, so other programs show and print it
/// at the page's size. A TIFF said 1 × 1 with no unit, and a JPEG said nothing.
#[test]
fn exported_pages_record_the_resolution_they_were_drawn_at() {
    use crate::export::{Exporter, ImageFormat};
    let (s, id) = session_with(1);
    let mut ex = Exporter::new(s.get(id).unwrap());
    assert_eq!(tiff_resolution(&ex.image(0, 144.0, ImageFormat::Tiff).unwrap()), (2, 144.0, 144.0));
    let jpg = ex.image(0, 144.0, ImageFormat::Jpeg { quality: 85 }).unwrap();
    assert_eq!(&jpg[6..11], b"JFIF\0");
    assert_eq!(jpg[13..18], [1, 0, 144, 0, 144], "JFIF density in dots per inch");
    // pHYs counts pixels per metre: 72 dpi would read back as 72.009, and a page made from the PNG
    // (doc_create from images) would come out 0.01% small.
    let png = ex.image(0, 72.0, ImageFormat::Png).unwrap();
    assert!(png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap().info().pixel_dims.is_none(), "a PNG states no resolution");

    // A page too large for the renderer at the asked resolution says the one it was drawn at.
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let bytes = s.create_blank(14_400.0, 792.0, 1).unwrap();
    let id = s.open("wide.pdf", None, bytes, None).unwrap();
    let mut ex = Exporter::new(s.get(id).unwrap());
    let used = ex.dpi_used(0, 600.0);
    let (unit, x, y) = tiff_resolution(&ex.image(0, 600.0, ImageFormat::Tiff).unwrap());
    assert!(unit == 2 && (x - used).abs() < 0.01 && (y - used).abs() < 0.01, "{x} × {y}, drawn at {used}");
}

#[test]
fn added_text_and_images_render_and_stay_editable() {
    let (mut s, id) = session_with(1);
    let text = AddedText { rect: [20.0, 250.0, 180.0, 280.0], text: "Added note".into(), size: 12.0, ..AddedText::default() };
    s.apply(id, Edit::AddText { page: 0, text: text.clone() }).unwrap();
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add text"));
    assert!(page_texts(&s, id)[0].contains("Added note"), "the renderer's extractor sees page content");
    // A PNG, centred at its natural size.
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 20, 10);
        enc.set_color(png::ColorType::Rgb);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[0u8; 600]).unwrap();
    }
    s.apply(id, Edit::AddImage { page: 0, rect: None, name: "dot.png".into(), bytes: Arc::new(png) }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.added().len(), 2);
    assert_eq!(d.added()[1].content.rect(), [90.0, 145.0, 110.0, 155.0], "20×10 px at 72 dpi, centred on 200×300");
    let moved = d.added()[0].content.with_rect([30.0, 100.0, 190.0, 130.0]);
    s.apply(id, Edit::UpdateContent { page: 0, index: 0, content: moved }).unwrap();
    assert_eq!(s.get(id).unwrap().added()[0].content.rect()[3], 130.0);
    s.apply(id, Edit::DeleteContent { page: 0, index: 1 }).unwrap();
    assert_eq!(s.get(id).unwrap().added().len(), 1);
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().added().len(), 2);
}

/// Edit ▸ Copy and Paste of added content (#2636): a text box comes back with all its style, an
/// image with its picture and transforms, in the same document and in another.
#[test]
fn pasted_added_content_keeps_its_style_and_picture() {
    let (mut s, id) = session_with(2);
    let text = AddedText {
        rect: [20.0, 250.0, 180.0, 264.4],
        text: "Styled note".into(),
        family: FontFamily::Times,
        bold: true,
        italic: true,
        size: 12.0,
        color: [0.8, 0.1, 0.2],
        ink: None,
        align: TextAlign::Center,
        aligns: Vec::new(),
        char_spacing: 0.0,
    };
    s.apply(id, Edit::AddText { page: 0, text: text.clone() }).unwrap();
    let copied = s.get(id).unwrap().added()[0].content.clone();
    let moved = copied.with_rect([40.0, 100.0, 200.0, 114.4]);
    s.apply(id, Edit::PasteContent { page: 1, content: moved.clone(), file: None }).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Paste"));
    assert_eq!(doc.added()[1].page, 1);
    assert_eq!(doc.added()[1].content, moved, "every part of the style, in the new box");

    // An image, turned, flipped and cropped.
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 20, 10);
        enc.set_color(png::ColorType::Rgb);
        let px: Vec<u8> = (0..200).flat_map(|i| [(i % 256) as u8, 40, 200]).collect();
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 150.0, 150.0]), name: "p.png".into(), bytes: Arc::new(png) }).unwrap();
    let AddedContent::Image(mut img) = s.get(id).unwrap().added()[1].content.clone() else { panic!("the image is the page's second item") };
    (img.rotation, img.flip_h, img.crop) = (1, true, [0.1, 0.0, 0.2, 0.05]);
    s.apply(id, Edit::UpdateContent { page: 0, index: 1, content: AddedContent::Image(img.clone()) }).unwrap();
    let (ext, file) = s.get(id).unwrap().added_image_file(0, 1).unwrap();
    assert_eq!(ext, "png");
    let file = Some(("Pasted image.png".to_string(), Arc::new(file)));
    assert!(s.get(id).unwrap().added_image_file(0, 0).is_err(), "the first item is text");
    let pasted = AddedImage { rect: [60.0, 90.0, 160.0, 140.0], ..img.clone() };
    s.apply(id, Edit::PasteContent { page: 0, content: AddedContent::Image(pasted.clone()), file: file.clone() }).unwrap();
    let AddedContent::Image(got) = s.get(id).unwrap().added()[2].content.clone() else { panic!("an image was pasted") };
    assert_eq!(got, pasted, "the same picture object, box, turn, flip and crop");

    // Into another document: the picture comes along as a file of its own.
    let other = s.open("other.pdf", None, Arc::new(fixture(1)), None).unwrap();
    s.apply(other, Edit::PasteContent { page: 0, content: AddedContent::Image(pasted.clone()), file: file.clone() }).unwrap();
    s.apply(other, Edit::PasteContent { page: 0, content: copied.clone(), file: None }).unwrap();
    let d = s.get(other).unwrap();
    let AddedContent::Image(theirs) = &d.added()[0].content else { panic!("an image was pasted") };
    assert_eq!(AddedImage { image: img.image, ..theirs.clone() }, pasted, "the same box and transforms");
    assert_eq!(d.added_image_file(0, 0).unwrap().1, file.as_ref().unwrap().1.as_slice(), "the same picture");
    assert_eq!(d.added()[1].content, copied);
    // Pasted there again, it draws the first paste's picture object rather than storing a third.
    let first = theirs.image;
    s.apply(other, Edit::PasteContent { page: 0, content: AddedContent::Image(pasted.clone()), file: file.clone() }).unwrap();
    let AddedContent::Image(again) = s.get(other).unwrap().added()[2].content.clone() else { panic!("an image was pasted") };
    assert_eq!(again.image, first, "one picture object for both pastes");
    // The object holds another picture of the same size (another document's numbering, an undone
    // add): the file is embedded instead of drawing the wrong picture.
    let mut other_png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut other_png, 20, 10);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[7u8; 600]).unwrap();
    }
    let other_file = Some(("other.png".to_string(), Arc::new(other_png)));
    s.apply(id, Edit::PasteContent { page: 0, content: AddedContent::Image(pasted.clone()), file: other_file }).unwrap();
    let AddedContent::Image(fresh) = s.get(id).unwrap().added()[3].content.clone() else { panic!("an image was pasted") };
    assert_ne!(fresh.image, img.image, "a picture object of its own");
    assert_ne!(s.get(id).unwrap().added_image_file(0, 3).unwrap().1, file.as_ref().unwrap().1.as_slice());
    assert!(matches!(
        s.apply(id, Edit::PasteContent { page: 0, content: AddedContent::Image(pasted), file: None }),
        Err(EditError::Edit(pdfcraft_edit::EditError::Invalid(_)))
    ));
    // Four items on page 1 and the pasted text on page 2.
    assert_eq!(s.get(id).unwrap().added().len(), 5, "refused pastes add nothing");
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().added().len(), 4, "a paste is one undo step");
}

/// An added JPEG 2000 picture is copied as the file it was added from: pasted into its own
/// document it draws the same object, and into another one it is embedded once.
#[test]
fn an_added_jpeg_2000_image_is_copied_as_its_file() {
    // A JP2 file (30 × 20, three components) with a stub codestream, as Add Image takes it.
    let bx = |ty: &[u8], payload: &[u8]| [((payload.len() + 8) as u32).to_be_bytes().as_slice(), ty, payload].concat();
    let ihdr = [20u32.to_be_bytes().as_slice(), &30u32.to_be_bytes(), &[0, 3, 7, 7, 0, 0]].concat();
    let signature = [0, 0, 0, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A];
    let file = [signature.as_slice(), &bx(b"ftyp", b"jp2 \0\0\0\0jp2 "), &bx(b"jp2h", &bx(b"ihdr", &ihdr)), &bx(b"jp2c", &[0xFF, 0x4F, 0xFF, 0x51])]
        .concat();
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 110.0, 140.0]), name: "photo.jp2".into(), bytes: Arc::new(file.clone()) })
        .unwrap();
    let (ext, copied) = s.get(id).unwrap().added_image_file(0, 0).unwrap();
    assert_eq!((ext, &copied), ("jp2", &file), "the file as it was added");
    let AddedContent::Image(img) = s.get(id).unwrap().added()[0].content.clone() else { panic!("an image was added") };
    let copied = Arc::new(copied);
    let file = Some(("Pasted image.jp2".to_string(), copied.clone()));
    let pasted = AddedContent::Image(AddedImage { rect: [70.0, 80.0, 130.0, 120.0], ..img.clone() });
    s.apply(id, Edit::PasteContent { page: 0, content: pasted.clone(), file: file.clone() }).unwrap();
    assert!(matches!(&s.get(id).unwrap().added()[1].content, AddedContent::Image(i) if i.image == img.image), "the same picture object");
    let other = s.open("other.pdf", None, Arc::new(fixture(1)), None).unwrap();
    for _ in 0..2 {
        s.apply(other, Edit::PasteContent { page: 0, content: pasted.clone(), file: file.clone() }).unwrap();
    }
    let d = s.get(other).unwrap();
    let images: Vec<_> = d
        .added()
        .iter()
        .filter_map(|a| match &a.content {
            AddedContent::Image(i) => Some(i.image),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0], images[1], "embedded once");
    assert_eq!(d.added_image_file(0, 0).unwrap().1, copied.as_slice());
}

#[test]
fn a_batch_of_content_edits_refreshes_the_added_content_and_marks() {
    let (mut s, id) = session_with(1);
    let text = |t: &str, y: f64| AddedText { rect: [20.0, y, 180.0, y + 14.4], text: t.into(), ..AddedText::default() };
    let edits = vec![Edit::AddText { page: 0, text: text("One", 200.0) }, Edit::AddText { page: 0, text: text("Two", 150.0) }];
    s.apply(id, Edit::Batch { label: "Script".into(), edits }).unwrap();
    assert_eq!(s.get(id).unwrap().added().len(), 2);
    s.apply(id, Edit::Batch { label: "Script".into(), edits: vec![Edit::DeleteContent { page: 0, index: 0 }] }).unwrap();
    assert_eq!(s.get(id).unwrap().added().len(), 1);
    let watermark =
        Edit::AddWatermark { pages: vec![0], settings: Watermark { text: "Draft".into(), ..Watermark::default() }, file: None, replace: false };
    s.apply(id, Edit::Batch { label: "Script".into(), edits: vec![watermark] }).unwrap();
    assert_eq!(s.get(id).unwrap().marks().len(), 1);
}

#[test]
fn added_images_rotate_flip_and_crop_as_drawn() {
    // A 20×10 image: left half red, right half blue.
    let mut px = Vec::new();
    for _y in 0..10 {
        for x in 0..20 {
            px.extend_from_slice(if x < 10 { &[255, 0, 0] } else { &[0, 0, 255] });
        }
    }
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 20, 10);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    let (mut s, id) = session_with(1);
    // A 100×100 box at (50, 100) on the 200×300 page.
    s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 150.0, 200.0]), name: "rb.png".into(), bytes: Arc::new(png) }).unwrap();
    let colour_at = |s: &Session, x: u32, y_from_top: u32| -> [u8; 3] {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), Default::default());
        let out = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        let i = ((y_from_top * out.width + x) * 4) as usize;
        [out.rgba[i], out.rgba[i + 1], out.rgba[i + 2]]
    };
    let red = |c: [u8; 3]| c[0] > 200 && c[2] < 60;
    let blue = |c: [u8; 3]| c[2] > 200 && c[0] < 60;
    // Display y 100–200 is rows 100–200 from the top on a 300 pt page.
    assert!(red(colour_at(&s, 70, 150)) && blue(colour_at(&s, 130, 150)), "as placed: red left, blue right");
    let item = |s: &Session| s.get(id).unwrap().added()[0].content.clone();
    let AddedContent::Image(mut img) = item(&s) else { panic!() };
    img.rotation = 1;
    s.apply(id, Edit::UpdateContent { page: 0, index: 0, content: AddedContent::Image(img.clone()) }).unwrap();
    assert!(red(colour_at(&s, 100, 180)) && blue(colour_at(&s, 100, 120)), "a quarter turn left: red at the bottom");
    img.rotation = 0;
    img.flip_h = true;
    s.apply(id, Edit::UpdateContent { page: 0, index: 0, content: AddedContent::Image(img.clone()) }).unwrap();
    assert!(blue(colour_at(&s, 70, 150)) && red(colour_at(&s, 130, 150)), "flipped");
    img.flip_h = false;
    img.crop = [0.0, 0.0, 0.5, 0.0];
    s.apply(id, Edit::UpdateContent { page: 0, index: 0, content: AddedContent::Image(img.clone()) }).unwrap();
    assert!(red(colour_at(&s, 130, 150)), "the right half cropped away: red fills the box");
    // More than half from one side: only the blue right part is left.
    img.crop = [0.7, 0.0, 0.0, 0.0];
    s.apply(id, Edit::UpdateContent { page: 0, index: 0, content: AddedContent::Image(img) }).unwrap();
    assert!(blue(colour_at(&s, 53, 150)) && blue(colour_at(&s, 130, 150)), "70 % trimmed from the left: blue fills the box to its left edge");
}

#[test]
fn reload_reads_the_new_file_into_the_same_document() {
    let (mut s, id) = session_with(2);
    let other = s.open("other.pdf", None, Arc::new(fixture(1)), None).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let before = s.get(id).unwrap().edit_generation();
    s.reload(id, Arc::new(fixture(3))).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!((d.id, d.name.as_str()), (id, "fixture.pdf"), "same document, same name");
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2", "Page 3"], "the new version");
    assert!(!d.dirty && d.can_undo().is_none(), "nothing unsaved, nothing to undo");
    assert!(d.edit_generation() > before, "caches keyed by the generation see a change");
    assert_eq!(s.docs().iter().map(|d| d.id).collect::<Vec<_>>(), [id, other], "no new tab, same order");
    // A file that isn't a PDF (yet) leaves the document as it was.
    assert!(s.reload(id, Arc::new(b"%PDF-1.7 half written".to_vec())).is_err());
    assert_eq!(page_texts(&s, id).len(), 3);
    assert_eq!(s.docs().len(), 2);
}

/// #3443: reloading the file (it changed on disk) keeps how the reader shows it. Reading colours
/// reached the new renderer, not just the setting; hidden comments stay hidden. Each setting is
/// checked alone, so neither one rides on the other being changed.
#[test]
fn reload_keeps_reading_colours_and_hidden_comments() {
    let dark_share = |s: &Session, id: DocId| {
        let doc = s.get(id).unwrap();
        doc.renderer.set_queue(vec![pdfcraft_render::RenderRequest {
            page: 0,
            kind: pdfcraft_render::RequestKind::Pixels,
            scale: 1.0,
            ..Default::default()
        }]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let page = loop {
            if let Some(page) = doc.renderer.try_recv() {
                break page;
            }
            assert!(std::time::Instant::now() < deadline, "page render did not finish");
            std::thread::yield_now();
        };
        let (px, _) = page.rgba.as_chunks::<4>();
        px.iter().filter(|p| p[0] < 64 && p[1] < 64 && p[2] < 64).count() as f64 / px.len() as f64
    };
    let (mut s, id) = session_with(1);
    assert!(dark_share(&s, id) < 0.1, "a white page as the file draws it");
    assert!(s.set_reading_colors(id, ReadingColors::WHITE_ON_BLACK));
    assert!(dark_share(&s, id) > 0.9, "white on black");
    s.reload(id, Arc::new(fixture(2))).unwrap();
    assert_eq!(s.reading_colors(id), Some(ReadingColors::WHITE_ON_BLACK));
    assert!(dark_share(&s, id) > 0.9, "the reloaded page is drawn white on black too");
    assert!(!s.get(id).unwrap().comments_hidden());

    let (mut s, id) = session_with(1);
    assert!(s.set_hide_comments(id, true));
    s.reload(id, Arc::new(fixture(2))).unwrap();
    assert!(s.get(id).unwrap().comments_hidden(), "hidden comments stay hidden");
    assert_eq!(s.reading_colors(id), Some(ReadingColors::default()));
    assert!(dark_share(&s, id) < 0.1);
}

#[test]
fn revert_goes_back_to_the_saved_version() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::DeletePages { pages: vec![0] }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    s.apply(id, Edit::InsertBlankPage { at: 0, width: 200.0, height: 300.0 }).unwrap();
    assert!(s.get(id).unwrap().dirty);
    s.revert(id).unwrap();
    let d = s.get(id).unwrap();
    assert!(!d.dirty && d.can_undo().is_none() && d.can_redo().is_none());
    assert_eq!(page_texts(&s, id), ["Page 2"], "the saved state, not the opened one");
    assert_eq!(d.info.pages[0].rotation, 0);
}

#[test]
fn split_by_size_and_bookmarks_and_page_filters() {
    let (mut s, id) = session_with(6);
    // Every page alone is a few hundred bytes: a limit of about two pages gives three parts.
    let one = s.split(id, &pdfcraft_organize::SplitBy::PageCount(1)).unwrap()[0].2.len();
    let parts = s.split_by_size(id, one * 2 + one / 2).unwrap();
    assert!(parts.len() >= 2 && parts.len() <= 6, "{}", parts.len());
    assert_eq!(parts.last().unwrap().1, 6, "every page is in a part");
    assert_eq!(s.split_by_size(id, 1).unwrap().len(), 6, "pages larger than the limit stand alone");
    // Top-level bookmarks name the parts.
    s.apply(id, Edit::AddBookmark { parent: vec![], index: 0, title: "Intro".into(), page: 0, left: None, top: None }).unwrap();
    s.apply(id, Edit::AddBookmark { parent: vec![], index: 1, title: "Results".into(), page: 3, left: None, top: None }).unwrap();
    assert_eq!(s.bookmark_splits(id), [(0, "Intro".to_string()), (3, "Results".to_string())]);
    // Filters: page numbers 2, 4, 6 are even.
    let info = &s.get(id).unwrap().info;
    let all: Vec<usize> = (0..6).collect();
    assert_eq!(filter_pages(info, &all, PageParity::Even, PageOrientation::Both), [1, 3, 5]);
    assert_eq!(filter_pages(info, &all, PageParity::Odd, PageOrientation::Landscape), Vec::<usize>::new());
    assert_eq!(filter_pages(info, &all, PageParity::Odd, PageOrientation::Portrait), [0, 2, 4]);
}

#[test]
fn links_from_urls_and_link_edits() {
    // One page whose text contains a web address.
    let mut objs: Vec<Vec<u8>> =
        vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(), b"<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 300 200] >>".to_vec()];
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    objs.push(b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >> >>".to_vec());
    let body = b"BT /F1 12 Tf 20 100 Td (Visit www.example.org today) Tj ET";
    objs.push([format!("<< /Length {} >>\nstream\n", body.len()).into_bytes(), body.to_vec(), b"\nendstream".to_vec()].concat());
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new();
    let id = s.open("u.pdf", None, Arc::new(out), None).unwrap();
    let found = s.find_urls(id);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].2, "http://www.example.org");
    s.apply(id, Edit::AddLinks { links: found, style: LinkStyle::default() }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.links().len(), 1);
    assert_eq!(d.info.links.len(), 1, "the viewer follows it");
    assert!(s.find_urls(id).is_empty(), "already linked");
    // Remove all links and undo.
    s.apply(id, Edit::RemoveLinks { pages: None }).unwrap();
    assert!(s.get(id).unwrap().links().is_empty());
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Remove all links"));
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().links().len(), 1);
    // Link Properties and delete.
    let l = s.get(id).unwrap().links()[0].clone();
    s.apply(id, Edit::SetLink { page: 0, index: l.index, rect: None, action: Some(LinkAction::Page(0)), style: None }).unwrap();
    assert_eq!(s.get(id).unwrap().links()[0].action, LinkAction::Page(0));
    s.apply(id, Edit::DeleteLink { page: 0, index: l.index }).unwrap();
    assert!(s.get(id).unwrap().links().is_empty());
    assert!(s.apply(id, Edit::RemoveLinks { pages: None }).is_err(), "nothing left to remove");
}

#[test]
fn appearance_only_edit_preserves_gotor_action_semantics() {
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".to_vec(),
        b"<< /Type /Annot /Subtype /Link /Rect [10 10 80 30] /A << /S /GoToR /F (target.pdf) /D [2 /FitH 180] /NewWindow true >> >>".to_vec(),
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
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());

    let mut session = Session::new();
    let id = session.open("source.pdf", Some("/docs/source.pdf".into()), Arc::new(bytes), None).unwrap();
    let before = session.get(id).unwrap().info.links[0].target.clone();
    let mut style = session.get(id).unwrap().links()[0].style;
    style.visible = true;
    style.width = 2.0;
    session.apply(id, Edit::SetLink { page: 0, index: 0, rect: None, action: None, style: Some(style) }).unwrap();

    let after = &session.get(id).unwrap().info.links[0].target;
    assert_eq!(*after, before, "the GoToR file, destination and /NewWindow hint must remain unchanged");
    assert_eq!(session.get(id).unwrap().links()[0].style, style, "the appearance edit is applied");
}

#[test]
fn comments_lock_take_checkmarks_hide_and_summarize() {
    let (mut s, id) = session_with(2);
    s.apply(id, rect_comment(1, [80.0, 80.0, 120.0, 120.0])).unwrap();
    s.apply(id, Edit::ReplyToAnnotation { page: 1, index: 0, text: "Agreed".into(), author: "Ada".into() }).unwrap();
    s.apply(id, Edit::LockAnnotation { page: 1, index: 0, locked: true }).unwrap();
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Lock comment"));
    assert!(s.get(id).unwrap().info.annotations[0].locked);
    assert!(s.apply(id, Edit::MoveAnnotation { page: 1, index: 0, dx: 5.0, dy: 0.0 }).is_err(), "locked comments stay put");
    assert!(s.apply(id, Edit::DeleteAnnotation { page: 1, index: 0 }).is_err(), "and can't be deleted");
    s.apply(id, Edit::SetAnnotationContents { page: 1, index: 0, text: "Still editable".into() }).unwrap();
    s.apply(id, Edit::LockAnnotation { page: 1, index: 0, locked: false }).unwrap();
    s.apply(id, Edit::MoveAnnotation { page: 1, index: 0, dx: 5.0, dy: 0.0 }).unwrap();

    s.apply(id, Edit::MarkAnnotation { page: 1, index: 0, marked: true, author: "Ada".into() }).unwrap();
    let a = &s.get(id).unwrap().info.annotations;
    let mark = a.iter().find(|r| r.is_mark()).expect("a checkmark reply");
    assert_eq!(mark.state.as_deref(), Some("Marked"));

    // Hiding comments drops the rectangle but keeps the page.
    let px = pixel(&s, id, 1, 105, 100);
    assert!(px[0] > 200 && px[1] < 40, "drawn: {px:?}");
    assert!(s.set_hide_comments(id, true));
    assert!(!s.set_hide_comments(id, true));
    assert_eq!(pixel(&s, id, 1, 105, 100), [255, 255, 255, 255]);
    assert!(s.set_hide_comments(id, false));
    assert!(pixel(&s, id, 1, 105, 100)[1] < 40);

    let text = comment_summary("doc.pdf", &s.get(id).unwrap().info.annotations, SummarySort::Page);
    assert!(text.starts_with("Summary of Comments on doc.pdf"));
    assert!(text.contains("Page: 2\nNumber: 1  Author: Reviewer  Subject: Rectangle"), "{text}");
    assert!(text.contains("Still editable") && text.contains("    Agreed"), "{text}");
    assert!(!text.contains("Marked"), "checkmarks aren't listed: {text}");
    let pdf = s.summarize_comments(id, SummarySort::Author).unwrap();
    let n = s.open_new("summary.pdf", pdf).unwrap();
    assert!(!s.get(n).unwrap().info.pages.is_empty());
    assert_eq!(comment_summary("x", &[], SummarySort::Page), "Summary of Comments on x\n\nThis document has no comments.\n");
}

/// Replies thread by their `/IRT` reference, not `/NM`: PyMuPDF names the first comment on every
/// page `fitz-A0`, which used to file every reply under page 1's comment. An `/IRT` to the
/// annotation itself, or to a link (never listed as a comment), leaves it a comment of its own. Both readers (inspect on open, and the
/// comment list the engine refreshes after an edit) must agree.
#[test]
fn replies_thread_by_reference_when_names_repeat_across_pages() {
    let pdf = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots [5 0 R 8 0 R 9 0 R 10 0 R] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots [6 0 R 7 0 R] >> endobj
5 0 obj << /Type /Annot /Subtype /Text /Rect [20 140 40 160] /NM (fitz-A0) /T (Alpha) /Contents (COVER) >> endobj
6 0 obj << /Type /Annot /Subtype /Text /Rect [20 140 40 160] /NM (fitz-A0) /T (Beta) /Contents (SECOND) >> endobj
7 0 obj << /Type /Annot /Subtype /Text /Rect [20 100 40 120] /NM (fitz-A1) /IRT 6 0 R /T (Gamma) /Contents (ANSWER) >> endobj
8 0 obj << /Type /Annot /Subtype /Text /Rect [20 100 40 120] /NM (fitz-A1) /IRT 8 0 R /T (Delta) /Contents (SELF) >> endobj
9 0 obj << /Type /Annot /Subtype /Link /Rect [200 20 260 40] /Border [0 0 0] /Dest [3 0 R /Fit] >> endobj
10 0 obj << /Type /Annot /Subtype /Text /Rect [20 60 40 80] /NM (fitz-A2) /IRT 9 0 R /T (Echo) /Contents (LINKED) >> endobj
trailer << /Root 1 0 R >>
%%EOF";
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("fitz.pdf", None, Arc::new(pdf.to_vec()), None).expect("opens");
    let check = |s: &Session, replies: usize| {
        let all = &s.get(id).unwrap().info.annotations;
        let answer = all.iter().find(|a| a.contents.as_deref() == Some("ANSWER")).unwrap();
        assert_eq!(answer.parent, Some((1, 0)), "{answer:?}");
        let looped = all.iter().find(|a| a.contents.as_deref() == Some("SELF")).unwrap();
        assert!(!looped.is_reply(), "{looped:?}");
        let linked = all.iter().find(|a| a.contents.as_deref() == Some("LINKED")).unwrap();
        assert!(!linked.is_reply(), "{linked:?}");
        let text = comment_summary("fitz.pdf", all, SummarySort::Page);
        let (cover, second) = text.split_once("SECOND").unwrap();
        assert!(!cover.contains("ANSWER"), "page 1's comment has no replies: {text}");
        assert!(cover.contains("Number: 2  Author: Delta"), "the self-reply is a comment of its own: {text}");
        assert!(cover.contains("Number: 3  Author: Echo"), "so is the reply to a link: {text}");
        assert_eq!(second.matches("    Author:").count(), replies, "{text}");
    };
    check(&s, 1);
    s.apply(id, Edit::ReplyToAnnotation { page: 1, index: 0, text: "LATER".into(), author: "Delta".into() }).unwrap();
    check(&s, 2);
}

/// A comment listed through a reference chain (`/Annots [5 0 R]`, `5 0 obj 6 0 R`) keeps its
/// replies after an edit, whether their `/IRT` names the comment or the chain, as on open.
#[test]
fn replies_thread_through_a_reference_chain_after_an_edit() {
    let pdf = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots [5 0 R 7 0 R 8 0 R 9 0 R] >> endobj
5 0 obj 6 0 R endobj
6 0 obj << /Type /Annot /Subtype /Text /Rect [20 140 40 160] /NM (one) /Contents (CHAINED) >> endobj
7 0 obj << /Type /Annot /Subtype /Text /Rect [20 120 40 130] /IRT 6 0 R /Contents (TO THE COMMENT) >> endobj
8 0 obj << /Type /Annot /Subtype /Text /Rect [20 100 40 110] /IRT 5 0 R /Contents (THROUGH THE CHAIN) >> endobj
9 0 obj << /Type /Annot /Subtype /Text /Rect [20 60 40 80] /NM (two) /Contents (PLAIN) >> endobj
trailer << /Root 1 0 R >>
%%EOF";
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("chain.pdf", None, Arc::new(pdf.to_vec()), None).expect("opens");
    let check = |s: &Session| {
        let all = &s.get(id).unwrap().info.annotations;
        let comment = all.iter().find(|a| a.contents.as_deref() == Some("CHAINED")).expect("listed");
        assert_eq!((comment.page, comment.index, comment.is_reply()), (0, 0, false), "{all:?}");
        for reply in ["TO THE COMMENT", "THROUGH THE CHAIN"] {
            let a = all.iter().find(|a| a.contents.as_deref() == Some(reply)).expect("listed");
            assert_eq!((a.parent, a.in_reply_to.as_deref()), (Some((0, 0)), Some("one")), "{reply}: {all:?}");
        }
    };
    check(&s);
    s.apply(id, Edit::ReplyToAnnotation { page: 0, index: 3, text: "LATER".into(), author: "Delta".into() }).unwrap();
    check(&s);
}

/// #820: the generated summary includes replies to replies, indented one level deeper.
#[test]
fn comment_summary_includes_replies_to_replies() {
    let (mut s, id) = session_with(1);
    s.apply(id, rect_comment(0, [40.0, 40.0, 90.0, 90.0])).unwrap();
    s.apply(id, Edit::ReplyToAnnotation { page: 0, index: 0, text: "FIRST_REPLY".into(), author: "A".into() }).unwrap();
    s.apply(id, Edit::ReplyToAnnotation { page: 0, index: 1, text: "NESTED_REPLY".into(), author: "B".into() }).unwrap();
    let text = comment_summary("doc.pdf", &s.get(id).unwrap().info.annotations, SummarySort::Page);
    assert!(text.contains("FIRST_REPLY"), "{text}");
    assert!(text.contains("NESTED_REPLY"), "replies to replies are summarized: {text}");
    assert!(text.contains("        NESTED_REPLY"), "the nested reply is indented under its parent: {text}");
}

#[test]
fn reading_colors_set_and_invalidated() {
    let (mut s, id) = session_with(1);
    // Every open document holds a value; the default is Off.
    assert_eq!(s.reading_colors(id), Some(ReadingColors::default()));
    assert!(s.set_reading_colors(id, ReadingColors::WHITE_ON_BLACK));
    assert_eq!(s.reading_colors(id), Some(ReadingColors::WHITE_ON_BLACK));
    assert!(!s.set_reading_colors(id, ReadingColors::WHITE_ON_BLACK), "unchanged");
    assert!(s.set_reading_colors(id, ReadingColors::default()), "back to Off");
    assert_eq!(s.reading_colors(id), Some(ReadingColors::default()));
    // No such document.
    assert_eq!(s.reading_colors(DocId(u64::MAX)), None);
    assert!(!s.set_reading_colors(DocId(u64::MAX), ReadingColors::WHITE_ON_BLACK));
}

#[test]
fn signing_saving_trusting_and_commenting_afterwards() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    // After the test certificate's start date (2026-10-02).
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(2)), None).unwrap();
    assert!(s.get(id).unwrap().signatures.is_empty());
    let opts = SignOptions { page: 1, rect: Some([20.0, 20.0, 180.0, 60.0]), reason: Some("Approved".into()), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    s.mark_signed(id, signed, Some("/tmp/signed.pdf".into())).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.name, "signed.pdf");
    assert!(doc.is_signed() && !doc.dirty && doc.can_undo().is_none(), "signing can't be undone");
    let sig = &doc.signatures[0];
    assert_eq!((sig.status, sig.page, sig.date.as_deref()), (SignatureStatus::Unknown, Some(1), Some("D:20270115080000Z")));
    assert_eq!(s.save_full_bytes(id).unwrap_err(), EditError::Signed);
    // Trusting the signer makes it valid; a later comment is permitted.
    s.set_trusted_certificates(vec![digital_id.certificate.clone()]);
    assert_eq!(s.get(id).unwrap().signatures[0].status, SignatureStatus::Valid);
    s.apply(id, rect_comment(0, [80.0, 80.0, 120.0, 120.0])).unwrap();
    let sig = &s.get(id).unwrap().signatures[0];
    let comments = sign::Modification::Allowed(vec![sign::kind::COMMENTS]);
    assert_eq!((sig.status, sig.modification.clone()), (SignatureStatus::Valid, comments));
    // Saving keeps the signature (incremental).
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    assert_eq!(s.get(id).unwrap().signatures[0].status, SignatureStatus::Valid);
    // Edits that would need a full rewrite are refused, and change nothing.
    let before = s.get(id).unwrap().can_undo().map(str::to_owned);
    assert!(matches!(s.apply(id, Edit::Sanitize), Err(EditError::SignedRewrite(_))));
    assert!(!s.get(id).unwrap().dirty && s.get(id).unwrap().can_undo().map(str::to_owned) == before);
}

#[test]
fn validated_signatures_without_layers_keep_their_appearance() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    // After the test certificate's start date (2026-10-02).
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
    let opts = SignOptions { page: 0, rect: Some([20.0, 20.0, 180.0, 60.0]), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    s.mark_signed(id, signed, None).unwrap();
    s.set_trusted_certificates(vec![digital_id.certificate.clone()]);
    let doc = s.get(id).unwrap();
    let sig = &doc.signatures[0];
    assert_eq!(sig.status, SignatureStatus::Valid);
    // Our own flat appearance has no validation layers: nothing is substituted, and
    // the display copy carries no generated caption.
    let display = doc.display.clone();
    assert!(!display.windows(b"Signature valid".len()).any(|w| w == b"Signature valid"));
    // With nothing to draw, the display copy aliases the file exactly as before.
    assert!(Arc::ptr_eq(&doc.display, &doc.bytes));
}

#[test]
fn signed_pages_render_trust_save_and_reopen() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    // After the test certificate's start date (2026-10-02).
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
    let opts = SignOptions { page: 0, rect: Some([20.0, 20.0, 180.0, 60.0]), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    // The signed bytes open elsewhere too: validity is in the file, not the session.
    let mut t = Session::new().with_clock(|| 1_800_000_000);
    let tid = t.open("signed.pdf", None, signed.clone(), None).unwrap();
    assert!(t.get(tid).unwrap().signatures.iter().any(|x| x.signed && x.status == SignatureStatus::Unknown));
    s.mark_signed(id, signed, None).unwrap();
    // The signed page renders, from the file and from the display copy alike.
    for bytes in [s.get(id).unwrap().bytes.clone(), s.get(id).unwrap().display.clone()] {
        let mut renderer = pdfcraft_render::PageRenderer::new(bytes, s.get(id).unwrap().config.clone());
        let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        assert!(rendered.error.is_none(), "{:?}", rendered.error);
        assert!(rendered.width > 0 && rendered.height > 0);
    }
    // Trusting the signer makes it valid; saving keeps it valid across sessions.
    s.set_trusted_certificates(vec![digital_id.certificate.clone()]);
    assert_eq!(s.get(id).unwrap().signatures[0].status, SignatureStatus::Valid);
    let saved = s.save_bytes(id).unwrap();
    let mut u = Session::new().with_clock(|| 1_800_000_000);
    u.set_trusted_certificates(vec![digital_id.certificate.clone()]);
    let uid = u.open("saved.pdf", None, saved.clone(), None).unwrap();
    assert_eq!(u.get(uid).unwrap().signatures[0].status, SignatureStatus::Valid);
}

#[test]
fn signed_documents_round_trip_behind_a_user_password() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    // After the test certificate's start date (2026-10-02).
    let mut doc = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
    doc.set_encryption(&pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Aes128,
        user_password: "user",
        owner_password: "owner",
        permissions: -1,
        encrypt_metadata: true,
        seed: [7; 32],
    })
    .unwrap();
    let encrypted = Arc::new(pdfcraft_cos::write_full(&doc, &pdfcraft_cos::SaveOptions::default()).unwrap());
    // Wrong passwords refuse; the right one signs, validates, saves and reopens.
    let mut e = Session::new().with_clock(|| 1_800_000_000);
    assert!(e.open("enc.pdf", None, encrypted.clone(), Some("wrong")).is_err(), "wrong passwords refuse");
    let eid = e.open("enc.pdf", None, encrypted, Some("user")).unwrap();
    let signed = e.sign(eid, &digital_id, SignOptions { page: 0, rect: Some([20.0, 20.0, 180.0, 60.0]), ..SignOptions::default() }).unwrap();
    e.mark_signed(eid, signed, None).unwrap();
    e.set_trusted_certificates(vec![digital_id.certificate.clone()]);
    assert_eq!(e.get(eid).unwrap().signatures[0].status, SignatureStatus::Valid);
    let saved = e.save_bytes(eid).unwrap();
    let mut f = Session::new().with_clock(|| 1_800_000_000);
    f.set_trusted_certificates(vec![digital_id.certificate.clone()]);
    assert!(f.open("enc-saved.pdf", None, saved.clone(), Some("wrong")).is_err());
    let fid = f.open("enc-saved.pdf", None, saved.clone(), Some("user")).unwrap();
    assert_eq!(f.get(fid).unwrap().signatures[0].status, SignatureStatus::Valid);
}

#[test]
fn chamfered_signature_borders_survive_trust_exports_printing_and_save() {
    for raster_glyph in [false, true] {
        let mut doc = pdfcraft_cos::Document::open(Arc::new(layered_signable_fixture())).unwrap();
        signature_layer_ops(&mut doc, 12, b"0 G 1 w 10 1 m 90 1 l 99 10 l 99 90 l 90 99 l 10 99 l 1 90 l 1 10 l h S");
        signature_layer_ops(&mut doc, 10, b"0 G 2 w 0 0 50 30 re S");
        if raster_glyph {
            let mut image = pdfcraft_cos::Dict::new();
            image.set(b"Subtype", pdfcraft_cos::Object::name("Image"));
            image.set(b"Width", pdfcraft_cos::Object::Int(8));
            image.set(b"Height", pdfcraft_cos::Object::Int(8));
            image.set(b"ColorSpace", pdfcraft_cos::Object::name("DeviceRGB"));
            image.set(b"BitsPerComponent", pdfcraft_cos::Object::Int(8));
            let image = doc.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::from_raw(image, vec![0; 8 * 8 * 3])));
            let mut xobjects = pdfcraft_cos::Dict::new();
            xobjects.set(b"im", pdfcraft_cos::Object::Ref(image));
            xobjects.set(b"unused", pdfcraft_cos::Object::Ref(pdfcraft_cos::ObjRef::new(8, 0)));
            let mut resources = pdfcraft_cos::Dict::new();
            resources.set(b"XObject", pdfcraft_cos::Object::Dict(xobjects));
            doc.update_dict(pdfcraft_cos::ObjRef::new(10, 0), |d| d.set(b"Resources", pdfcraft_cos::Object::Dict(resources))).unwrap();
            signature_layer_ops(&mut doc, 10, b"q 20 0 0 20 5 5 cm /im Do Q");
        }
        assert_signature_artwork_survives_trust(doc, &["Not Verified"]);
    }
}

#[test]
fn mixed_signature_caption_details_survive_trust_exports_printing_and_save() {
    let mut doc = pdfcraft_cos::Document::open(Arc::new(layered_signable_fixture())).unwrap();
    signature_layer_ops(&mut doc, 11, b"BT /F1 5 Tf 2 27 Td (Alice Signer) Tj 0 -3 Td (alice@example.org) Tj 0 -3 Td (Signature Not Verified) Tj ET");
    assert_signature_artwork_survives_trust(doc, &["Alice Signer", "alice@example.org", "Not Verified"]);
}

/// Metadata edits retain the original caption; the UI can still show its verdict badge.
#[test]
fn a_metadata_edit_keeps_the_original_signature_appearance() {
    let (mut s, vid) = trusted_layered_session();
    s.apply(vid, Edit::SetInfo { key: "Title".into(), value: "Contract".into() }).unwrap();
    let doc = s.get(vid).unwrap();
    assert_eq!(doc.signatures[0].status, SignatureStatus::Valid);
    assert!(bytes_has(&doc.display, b"Not Verified"));
    assert!(!bytes_has(&doc.display, b"Signature valid"));
}
