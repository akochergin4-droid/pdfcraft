//! Unit tests of the engine (part 2 of 7).

#[allow(unused_imports)]
use super::*;

/// Table 22, bit 5: "Copy or otherwise extract text and graphics" (#568). Snapshots, exports and
/// Save Image As are refused, other permissions are untouched, and the owner password lifts it.
#[test]
fn copy_restricted_documents_refuse_extraction_unless_opened_by_the_owner() {
    let bytes = protected("", "owner", -1 ^ 0b1_0000); // everything but copying, no password to open
    let mut s = Session::new();
    let id = s.open("r.pdf", None, bytes.clone(), None).unwrap();
    let d = s.get(id).unwrap();
    assert!(!d.allows_copying());
    assert!(d.allows_printing() && d.allows_modification() && d.allows_assembly(), "only copying is withheld");
    assert_eq!(d.check_copying(), Err(EditError::NotPermitted("copying text and images")));
    assert!(d.page_image_file(0, 0).unwrap_err().contains("don't allow copying"));
    let extracting = ["edit.snapshot", "export.image", "export.text", "export.docx", "export.pptx", "export.html", "export.rtf", "export.all_images"];
    let enabled = |s: &Session, id, c| commands::is_enabled(commands::command(c).unwrap(), s, Some(id));
    for c in extracting {
        assert!(!enabled(&s, id, c), "{c} is disabled");
    }
    assert!(enabled(&s, id, "edit.find") && enabled(&s, id, "a11y.check"), "searching and accessibility don't copy");
    assert!(s.get(id).unwrap().autotag_preview(0).is_ok(), "like a11y.check, the auto-tag preview is allowed");
    let oid = s.open("r.pdf", None, bytes, Some("owner")).unwrap();
    assert!(s.get(oid).unwrap().allows_copying());
    assert!(extracting.iter().all(|c| enabled(&s, oid, c)));
    // Comparing reads both documents' text out, into the result, the report and the comments.
    let refused = || Err(EditError::NotPermitted("copying text and images"));
    assert_eq!((s.compare(id, oid).map(|_| ()), s.compare(oid, id).map(|_| ())), (refused(), refused()));
    assert_eq!(s.compare_report(oid, id).map(|_| ()), refused());
    assert!(s.compare(oid, oid).is_ok());
    assert_eq!(s.mark_differences(id, oid).map(|_| ()), refused());
    // Unencrypted documents allow everything.
    let (s, plain) = session_with(1);
    assert!(s.get(plain).unwrap().check_copying().is_ok() && extracting.iter().all(|c| enabled(&s, plain, c)));
}

/// Table 22, bit 3 (#2669): a document that forbids printing disables Print up front instead
/// of refusing once the dialog is confirmed. Low-resolution printing (bit 3 on, bit 12 off) is
/// still printing, and the owner password lifts the restriction.
#[test]
fn print_restricted_documents_disable_print_unless_opened_by_the_owner() {
    let enabled = |s: &Session, id, c| commands::is_enabled(commands::command(c).unwrap(), s, Some(id));
    let bytes = protected("", "owner", -1 ^ 0b100);
    let mut s = Session::new();
    let id = s.open("r.pdf", None, bytes.clone(), None).unwrap();
    assert!(!s.get(id).unwrap().allows_printing());
    assert!(!enabled(&s, id, "print.dialog"));
    assert!(enabled(&s, id, "export.text"), "only printing is withheld");
    let oid = s.open("r.pdf", None, bytes, Some("owner")).unwrap();
    assert!(enabled(&s, oid, "print.dialog"), "the owner may print");
    let low = s.open("low.pdf", None, protected("", "owner", -1 ^ (1 << 11)), None).unwrap();
    assert!(enabled(&s, low, "print.dialog"), "low-resolution printing is printing");
}

/// Table 22, bit 12 clear: printing is limited to a low-level representation. Whatever the
/// settings ask for, the print-ready PDF carries each page as a picture of at most 150 dpi, with
/// no text or vector artwork left; the owner password lifts the limit.
#[test]
fn low_resolution_printing_prints_pictures_of_at_most_150_dpi() {
    let bytes = protected("", "owner", -1 ^ (1 << 11));
    let mut s = Session::new();
    let low = s.open("low.pdf", None, bytes.clone(), None).unwrap();
    let width = f64::from(s.get(low).unwrap().info.pages[0].width);
    let actual = print::Settings { pages: vec![0], layout: print::Layout::Size(print::SizeMode::Actual), ..Default::default() };
    for raster in [None, Some(print::Raster { dpi: 600.0, grayscale: false })] {
        let printed = s.print_pdf(low, &print::Settings { raster, ..actual.clone() }).unwrap();
        let mut s2 = Session::new();
        let id2 = s2.open("print.pdf", None, Arc::new(printed.clone()), None).unwrap();
        assert!(page_texts(&s2, id2).iter().all(|t| t.trim().is_empty()), "{raster:?}: no text is left");
        let doc = pdfcraft_cos::Document::open(Arc::new(printed)).unwrap();
        let widths: Vec<i64> = doc
            .scan_objects()
            .filter_map(|(_, o)| match &*o {
                pdfcraft_cos::Object::Stream(st) if st.dict.name(b"Subtype") == Some(b"Image") => st.dict.int(b"Width"),
                _ => None,
            })
            .collect();
        assert!(!widths.is_empty() && widths.iter().all(|&w| w as f64 <= width * 150.0 / 72.0 + 1.0), "{raster:?}: {widths:?} px for {width} pt");
    }
    let owner = s.open("low.pdf", None, bytes, Some("owner")).unwrap();
    let printed = s.print_pdf(owner, &actual).unwrap();
    let mut s2 = Session::new();
    let id2 = s2.open("print.pdf", None, Arc::new(printed), None).unwrap();
    assert!(page_texts(&s2, id2).iter().any(|t| !t.trim().is_empty()), "the owner prints the pages themselves");
}

/// Low-resolution printing prints what the chosen content prints when the pages themselves are
/// printed: form fields only leaves out the page content and every comment, the document alone
/// leaves out the comments, and the document with stamps keeps only the stamps.
#[test]
fn low_resolution_printing_prints_only_the_chosen_content() {
    let mut cos = pdfcraft_cos::Document::open(Arc::new(print_content_pdf())).unwrap();
    cos.set_encryption(&pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Aes256,
        user_password: "",
        owner_password: "owner",
        permissions: -1 ^ (1 << 11),
        encrypt_metadata: true,
        seed: [7; 32],
    })
    .unwrap();
    let bytes = Arc::new(pdfcraft_cos::write_full(&cos, &Default::default()).unwrap());
    let mut s = Session::new();
    let low = s.open("low.pdf", None, bytes.clone(), None).unwrap();
    let owner = s.open("low.pdf", None, bytes, Some("owner")).unwrap();
    // Each band's centre, in pixels from the top of the sheet at 72 dpi, and its colour.
    let bands = [("content", 50, [0, 0, 255]), ("field", 150, [0, 255, 0]), ("stamp", 250, [255, 0, 0]), ("comment", 350, [255, 255, 0])];
    let all: &[&str] = &["content", "field", "stamp", "comment"];
    for (content, shown) in [
        (print::Content::DocumentAndMarkups, all),
        (print::Content::Document, &["content", "field"][..]),
        (print::Content::DocumentAndStamps, &["content", "field", "stamp"][..]),
        (print::Content::FormFieldsOnly, &["field"][..]),
    ] {
        let settings = print::Settings {
            pages: vec![0],
            paper: (100.0, 400.0),
            layout: print::Layout::Size(print::SizeMode::Actual),
            content,
            ..Default::default()
        };
        for (id, how) in [(owner, "pages"), (low, "pictures")] {
            let (w, h, rgba) = printed_pixels(s.print_pdf(id, &settings).unwrap());
            assert_eq!((w, h), (100, 400), "{content:?} as {how}");
            for (band, y, colour) in bands {
                let px = rgba.get((y * w as usize + 50) * 4..).and_then(|p| p.get(..3)).unwrap();
                let near = |c: [u8; 3]| px.iter().zip(c).all(|(&a, b)| a.abs_diff(b) < 40);
                let expected = shown.contains(&band);
                assert!(if expected { near(colour) } else { near([255; 3]) }, "{content:?} as {how}: {band} drawn {expected} expected, pixel {px:?}");
            }
        }
    }
}

#[test]
fn the_permissions_password_unlocks_an_open_document() {
    // Only a permissions password (#995, #1077): opens without asking. Changes are allowed,
    // security changes aren't.
    let bytes = protected("", "owner", 0b1100);
    let mut s = Session::new();
    let id = s.open("r.pdf", None, bytes, None).unwrap();
    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Before".into() }).unwrap();
    assert!(!s.get(id).unwrap().allows_security_change());
    assert!(matches!(s.apply(id, Edit::RemoveProtection), Err(EditError::NotPermitted(_))));
    for wrong in ["", "nope"] {
        assert_eq!(s.unlock(id, wrong), Err(EditError::WrongPassword), "{wrong:?}");
    }
    assert!(!s.get(id).unwrap().allows_security_change());
    s.unlock(id, "owner").unwrap();
    let d = s.get(id).unwrap();
    assert!(d.allows_security_change() && d.security_summary().unwrap().owner);
    assert!(d.dirty, "unlocking leaves the unsaved edit alone");
    assert_eq!(d.can_undo(), Some("Change Title"), "and adds no undo step");
    // Undo and redo don't bring the restrictions back.
    s.undo(id).unwrap();
    assert!(s.get(id).unwrap().allows_security_change(), "after undo");
    s.redo(id).unwrap();
    assert!(s.get(id).unwrap().allows_security_change(), "after redo");
    // Neither does saving.
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    assert!(s.get(id).unwrap().allows_security_change(), "after saving");
    s.apply(id, Edit::RemoveProtection).unwrap();
    let plain = s.save_bytes(id).unwrap();
    assert!(pdfcraft_cos::Document::open(plain).unwrap().security().is_none(), "security removed");
}

#[test]
fn remove_security_from_a_file_with_its_encrypt_dictionary_in_the_trailer() {
    // MuPDF writes /Encrypt inline in the trailer rather than as an object (#2661).
    let mut doc = pdfcraft_cos::Document::open_with_password(protected("user", "owner", -1), Some("owner")).unwrap();
    let r = doc.trailer().reference(b"Encrypt").unwrap();
    let dict = doc.get(r).as_dict().cloned().unwrap();
    doc.trailer_mut().set(b"Encrypt".to_vec(), pdfcraft_cos::Object::Dict(dict));
    let bytes = Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap());
    let mut s = Session::new();
    let id = s.open("m.pdf", None, bytes, Some("owner")).unwrap();
    s.apply(id, Edit::RemoveProtection).unwrap();
    assert!(!s.get(id).unwrap().info.encrypted);
    let plain = s.save_bytes(id).unwrap();
    assert!(pdfcraft_cos::Document::open(plain).unwrap().security().is_none(), "security removed");
}

#[test]
fn unlocking_needs_the_permissions_password_not_the_open_password() {
    let mut s = Session::new();
    let id = s.open("r.pdf", None, protected("open", "perms", 0b0100), Some("open")).unwrap();
    assert_eq!(s.unlock(id, "open"), Err(EditError::WrongPassword));
    s.unlock(id, "perms").unwrap();
    assert!(s.get(id).unwrap().allows_security_change());
    // Nothing to unlock: opened by the owner already, or not encrypted.
    s.unlock(id, "anything").unwrap();
    let plain = s.open("p.pdf", None, Arc::new(fixture(1)), None).unwrap();
    s.unlock(plain, "anything").unwrap();
}

#[test]
fn combining_protected_files_is_refused_clearly() {
    let s = Session::new();
    let err = s.combine(&[("a.pdf".into(), protected("pw", "o", -1)), ("b.pdf".into(), Arc::new(fixture(1)))]).unwrap_err();
    assert_eq!(err, EditError::Source("a.pdf: it is password-protected".into()));
    let err = s.combine(&[("c.pdf".into(), protected("", "o", 0b0100))]).unwrap_err();
    assert!(matches!(err, EditError::Source(m) if m.contains("don't allow copying pages")));
}

#[test]
fn owner_password_of_older_revisions_opens_the_viewer_too() {
    for alg in [pdfcraft_cos::Algorithm::Rc4_128, pdfcraft_cos::Algorithm::Aes128] {
        let mut doc = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
        doc.set_encryption(&pdfcraft_cos::NewEncryption {
            algorithm: alg,
            user_password: "u",
            owner_password: "o",
            permissions: 0,
            encrypt_metadata: true,
            seed: [1; 32],
        })
        .unwrap();
        let bytes = Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap());
        let mut s = Session::new();
        let id = s.open("x.pdf", None, bytes, Some("o")).unwrap_or_else(|e| panic!("{alg:?}: {e}"));
        let d = s.get(id).unwrap();
        assert!(d.security_summary().unwrap().owner && d.allows_modification(), "{alg:?}");
        assert_eq!(page_texts(&s, id), ["Page 1"], "{alg:?}: the renderer reads it");
    }
}

/// Render paths outside a session (the CLI's `info`, `text`, `render`) get the password the
/// renderer takes: the recovered user password for an R2–R4 owner password, else as given.
#[test]
fn render_password_for_recovers_the_user_password_of_older_revisions() {
    for alg in [pdfcraft_cos::Algorithm::Rc4_40, pdfcraft_cos::Algorithm::Rc4_128, pdfcraft_cos::Algorithm::Aes128, pdfcraft_cos::Algorithm::Aes256] {
        let mut doc = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
        doc.set_encryption(&pdfcraft_cos::NewEncryption {
            algorithm: alg,
            user_password: "u",
            owner_password: "o",
            permissions: 0,
            encrypt_metadata: true,
            seed: [1; 32],
        })
        .unwrap();
        let bytes = Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap());
        let owner = crate::render_password_for(bytes.clone(), Some("o"));
        let expected = if alg == pdfcraft_cos::Algorithm::Aes256 { "o" } else { "u" };
        assert_eq!(owner.as_deref(), Some(expected), "{alg:?}");
        assert!(pdfcraft_render::inspect(bytes.clone(), owner.as_deref()).is_ok(), "{alg:?}: the renderer opens it");
        assert_eq!(crate::render_password_for(bytes.clone(), Some("u")).as_deref(), Some("u"), "{alg:?}");
        assert_eq!(crate::render_password_for(bytes.clone(), Some("wrong")).as_deref(), Some("wrong"), "{alg:?}");
        assert_eq!(crate::render_password_for(bytes, None), None, "{alg:?}");
    }
}

#[test]
fn autosave_snapshots_only_changed_documents() {
    let (mut s, id) = session_with(2);
    assert!(!s.has_pending_autosave());
    assert!(s.autosave_snapshots().is_empty(), "clean documents are not snapshotted");
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    assert!(s.has_pending_autosave());
    assert!(s.has_pending_autosave(), "checking a deadline must not consume a snapshot");
    let snaps = s.autosave_snapshots();
    assert!(!s.has_pending_autosave());
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].bytes, s.get(id).unwrap().bytes, "the working file");
    assert!(s.autosave_snapshots().is_empty(), "nothing new since the last snapshot");
    s.apply(id, Edit::DeletePages { pages: vec![1] }).unwrap();
    assert_eq!(s.autosave_snapshots().len(), 1);
    s.undo(id).unwrap();
    assert!(s.get(id).unwrap().dirty, "the rotation is still unsaved");
    assert_eq!(s.autosave_snapshots().len(), 1, "undo that leaves unsaved work is a change");
    s.undo(id).unwrap();
    assert!(!s.get(id).unwrap().dirty, "undo back to the opened file is saved");
    assert!(s.autosave_snapshots().is_empty(), "a clean document needs no recovery");
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    assert!(s.autosave_snapshots().is_empty(), "saved documents need no recovery");
}

#[test]
fn autosave_snapshots_a_new_unsaved_document_once() {
    let (mut s, clean) = session_with(1);
    let id = s.open_new("Untitled.pdf", Arc::new(fixture(1))).unwrap();
    let snaps = s.autosave_snapshots();
    assert_eq!(snaps.len(), 1, "the new document is unsaved work");
    assert_eq!(snaps[0].doc, id);
    assert!(snaps.iter().all(|x| x.doc != clean), "the clean document stays out");
    assert!(s.autosave_snapshots().is_empty(), "nothing new since the last snapshot");
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    assert_eq!(s.autosave_snapshots().len(), 1, "a later edit is snapshotted again");
}

#[test]
fn recovered_documents_reopen_unsaved_at_their_original_path() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::DeletePages { pages: vec![1] }).unwrap();
    let snap = s.autosave_snapshots().remove(0);
    // A new session after a crash.
    let mut s2 = Session::new();
    let rid = s2.open(snap.name.clone(), None, snap.bytes.clone(), None).unwrap();
    s2.mark_recovered(rid, Some("/docs/report.pdf".into()));
    let d = s2.get(rid).unwrap();
    assert!(d.dirty);
    assert_eq!(d.path.as_deref(), Some("/docs/report.pdf"));
    assert_eq!(d.info.pages.len(), 1, "the edit survived");
    assert!(s2.autosave_snapshots().is_empty(), "already in the recovery store");
}

#[test]
fn bookmark_edits_show_in_the_viewer_undo_and_save() {
    let (mut s, id) = session_with(3);
    let titles = |s: &Session| outline_titles(&s.get(id).unwrap().info.outline);
    s.apply(id, Edit::AddBookmark { parent: vec![], index: 0, title: "Start".into(), page: 0, left: None, top: None }).unwrap();
    s.apply(id, Edit::AddBookmark { parent: vec![], index: 1, title: "End".into(), page: 2, left: None, top: None }).unwrap();
    s.apply(id, Edit::AddBookmark { parent: vec![1], index: 0, title: "Détail".into(), page: 1, left: None, top: None }).unwrap();
    // The inspector (an independent parser) sees the same tree and destinations.
    assert_eq!(titles(&s), ["Start→1", "End→3[Détail→2]"]);
    s.apply(id, Edit::MoveBookmark { from: vec![1, 0], to_parent: vec![], index: 0 }).unwrap();
    s.apply(id, Edit::RenameBookmark { path: vec![2], title: "Finish".into() }).unwrap();
    s.apply(id, Edit::SetBookmarkPage { path: vec![1], page: 1 }).unwrap();
    assert_eq!(titles(&s), ["Détail→2", "Start→2", "Finish→3"]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Set bookmark destination"));
    s.undo(id).unwrap();
    assert_eq!(titles(&s), ["Détail→2", "Start→1", "Finish→3"]);
    s.apply(id, Edit::DeleteBookmark { path: vec![0] }).unwrap();
    assert!(matches!(s.apply(id, Edit::DeleteBookmark { path: vec![7] }), Err(EditError::Bookmark(_))));

    let saved = s.save_bytes(id).unwrap();
    let mut again = Session::new();
    let id2 = again.open("again.pdf", None, saved, None).unwrap();
    assert_eq!(outline_titles(&again.get(id2).unwrap().info.outline), ["Start→1", "Finish→3"]);
}

#[test]
fn bookmarks_from_structure_nest_the_headings_and_undo() {
    let mut s = Session::new();
    let id = s.open("tagged.pdf", None, Arc::new(tagged_headings_fixture()), None).unwrap();
    s.apply(id, Edit::BookmarksFromStructure).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["Untitled→0[Intro→1[Details→2]]"]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("New bookmarks from structure"));
    s.undo(id).unwrap();
    assert!(s.get(id).unwrap().info.outline.is_empty());

    // Untagged: a clear error and nothing changes.
    let (mut s, id) = session_with(2);
    let err = s.apply(id, Edit::BookmarksFromStructure).unwrap_err();
    assert_eq!(err.to_string(), "this document has no tagged headings to make bookmarks from");
    assert!(s.get(id).unwrap().info.outline.is_empty());
}

/// #1693: the document's text styles, largest first, and bookmarks made from the chosen ones:
/// chapters at level 1, sections nested under the chapter before them. A two-line title is one
/// bookmark; a subset of the same font and a size set by the text matrix count as the style.
/// Each bookmark goes to its heading's top. One undo step; no style, or none in use, is refused.
#[test]
fn bookmarks_from_text_styles_nest_the_headings_and_undo() {
    let mut s = Session::new();
    let id = s.open("styled.pdf", None, Arc::new(styled_headings_fixture()), None).unwrap();
    let styles = s.get(id).unwrap().text_styles();
    let listed: Vec<(String, f64, bool, usize, usize)> =
        styles.iter().map(|u| (u.style.font.clone(), u.style.size, u.style.bold, u.lines, u.pages)).collect();
    assert_eq!(
        listed,
        [
            ("Helvetica-Bold".to_string(), 18.0, true, 3, 2),
            ("Helvetica-Bold".to_string(), 14.0, true, 5, 3),
            ("Helvetica".to_string(), 11.0, false, 3, 2),
        ]
    );
    assert_eq!(styles[0].sample, "Chapter One");
    let (chapter, section) = (styles[0].style.clone(), styles[1].style.clone());
    s.apply(id, Edit::BookmarksFromTextStyles { levels: vec![chapter.clone(), section.clone()] }).unwrap();
    let outline = s.get(id).unwrap().info.outline.clone();
    assert_eq!(
        outline_titles(&outline),
        ["Untitled→0[Chapter One→1[Section A→1],Chapter Two and its subtitle→2[Section B→2,Section C→2,Left part→3,Right part→3]]"]
    );
    let first = &outline[0].children[0];
    match first.view {
        pdfcraft_render::DestView::Xyz { left: None, top: Some(top), zoom: None } => {
            assert!((700.0..720.0).contains(&top), "the top of the heading's line: {top}")
        }
        ref v => panic!("{v:?}"),
    }
    assert_eq!(s.get(id).unwrap().can_undo(), Some("New bookmarks from text styles"));
    s.undo(id).unwrap();
    assert!(s.get(id).unwrap().info.outline.is_empty());
    // One level is enough; the sections are then left out.
    s.apply(id, Edit::BookmarksFromTextStyles { levels: vec![chapter] }).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["Untitled→0[Chapter One→1,Chapter Two and its subtitle→2]"]);
    s.undo(id).unwrap();
    // Nothing chosen, or a style no text is in: refused, nothing changes.
    let err = s.apply(id, Edit::BookmarksFromTextStyles { levels: vec![] }).unwrap_err();
    assert!(err.to_string().contains("at least one bookmark level"), "{err}");
    let unused = pdfcraft_engine_style(&section, 30.0);
    let err = s.apply(id, Edit::BookmarksFromTextStyles { levels: vec![unused] }).unwrap_err();
    assert!(err.to_string().contains("no text is set in the chosen styles"), "{err}");
    assert!(s.get(id).unwrap().info.outline.is_empty());
}

/// CodeRabbit on #4560: a line goes to the first level whose style it is in, so the same style
/// for two levels (or one within the size tolerance of the other) would leave the later level
/// empty. Refused at the edit, for the dialog and automation alike; nothing changes.
#[test]
fn bookmarks_from_text_styles_refuse_one_style_for_two_levels() {
    let mut s = Session::new();
    let id = s.open("styled.pdf", None, Arc::new(styled_headings_fixture()), None).unwrap();
    let styles = s.get(id).unwrap().text_styles();
    let (chapter, section) = (styles[0].style.clone(), styles[1].style.clone());
    let near = pdfcraft_engine_style(&section, section.size + 0.1);
    for levels in [vec![chapter.clone(), chapter.clone()], vec![chapter.clone(), section.clone(), near]] {
        let n = levels.len();
        let err = s.apply(id, Edit::BookmarksFromTextStyles { levels }).unwrap_err();
        assert!(err.to_string().contains(&format!("levels {} and {n} have the same text style", n - 1)), "{err}");
        assert!(s.get(id).unwrap().info.outline.is_empty());
    }
}

/// CodeRabbit on #4560: bookmark titles from text styles are the document's text, so a document
/// that forbids copying lists no styles and refuses the edit, as it refuses other extraction.
#[test]
fn bookmarks_from_text_styles_need_copying_permission() {
    let mut doc = pdfcraft_cos::Document::open(Arc::new(styled_headings_fixture())).unwrap();
    doc.set_encryption(&pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Aes256,
        user_password: "",
        owner_password: "owner",
        permissions: -1 ^ 0b1_0000,
        encrypt_metadata: true,
        seed: [7; 32],
    })
    .unwrap();
    let bytes = Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap());
    let mut s = Session::new();
    let id = s.open("r.pdf", None, bytes.clone(), None).unwrap();
    assert!(s.get(id).unwrap().allows_assembly() && !s.get(id).unwrap().allows_copying());
    assert!(s.get(id).unwrap().text_styles().is_empty(), "no samples of text that may not be copied");
    let style = text_style_bookmarks::TextStyle { font: "Helvetica-Bold".into(), size: 18.0, bold: true, italic: false };
    let err = s.apply(id, Edit::BookmarksFromTextStyles { levels: vec![style.clone()] }).unwrap_err();
    assert_eq!(err, EditError::NotPermitted("copying text and images"));
    let owner = s.open("r.pdf", None, bytes, Some("owner")).unwrap();
    assert!(!s.get(owner).unwrap().text_styles().is_empty());
    assert!(s.apply(owner, Edit::BookmarksFromTextStyles { levels: vec![style] }).is_ok(), "the owner may");
}

/// CodeRabbit on #4560: two headings in one style in two columns, the right one a little lower,
/// are two bookmarks, not one heading run on across the columns.
#[test]
fn headings_in_two_columns_are_two_bookmarks() {
    let page = "BT /FB 14 Tf 72 700 Td (Left heading) Tj ET BT /FB 14 Tf 320 690 Td (Right heading) Tj ET";
    let bytes = pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R >>")
        .obj("<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] /Resources << /Font << /FB 5 0 R >> >> >>")
        .obj("<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>")
        .stream("", page)
        .obj("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
        .build();
    let mut s = Session::new();
    let id = s.open("cols.pdf", None, Arc::new(bytes), None).unwrap();
    let style = s.get(id).unwrap().text_styles()[0].style.clone();
    s.apply(id, Edit::BookmarksFromTextStyles { levels: vec![style] }).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["Untitled→0[Left heading→1,Right heading→1]"]);
}

#[test]
fn number_pages_shows_in_the_viewer_and_undoes() {
    let (mut s, id) = session_with(5);
    let labels = |s: &Session| s.get(id).unwrap().info.pages.iter().map(|p| p.label.clone()).collect::<Vec<_>>();
    use pdfcraft_organize::LabelStyle;
    s.apply(id, Edit::NumberPages { from: 0, to: 1, style: LabelStyle::LowerRoman, prefix: String::new(), first: 1 }).unwrap();
    s.apply(id, Edit::NumberPages { from: 2, to: 4, style: LabelStyle::Decimal, prefix: "§".into(), first: 10 }).unwrap();
    // The inspector (lopdf-based, independent) formats them the same way.
    assert_eq!(labels(&s), ["i", "ii", "§10", "§11", "§12"]);
    s.undo(id).unwrap();
    assert_eq!(labels(&s), ["i", "ii", "3", "4", "5"]);
    assert!(s.apply(id, Edit::NumberPages { from: 3, to: 9, style: LabelStyle::Decimal, prefix: String::new(), first: 1 }).is_err());
}

#[test]
fn comments_are_added_drawn_threaded_and_undone() {
    let (mut s, id) = session_with(2);
    assert_eq!(pixel(&s, id, 1, 100, 100), [255, 255, 255, 255]);
    s.apply(id, rect_comment(1, [80.0, 80.0, 120.0, 120.0])).unwrap();
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add rectangle"));
    let px = pixel(&s, id, 1, 100, 100);
    assert!(px[0] > 200 && px[1] < 40 && px[2] < 40, "the rectangle is drawn: {px:?}");
    let a = &s.get(id).unwrap().info.annotations;
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].page, a[0].index, a[0].author.as_deref(), a[0].contents.as_deref()), (1, 0, Some("Reviewer"), Some("Look here")));
    assert_eq!(a[0].modified.as_deref().map(|m| m.contains("2023")), Some(true), "dated by the session clock");
    let nm = a[0].name.clone().expect("has an /NM");
    assert_eq!(nm.len(), 36);

    s.apply(id, Edit::ReplyToAnnotation { page: 1, index: 0, text: "Done".into(), author: "Ada".into() }).unwrap();
    s.apply(id, Edit::SetAnnotationStatus { page: 1, index: 0, state: ReviewState::Completed, author: "Ada".into() }).unwrap();
    let a = &s.get(id).unwrap().info.annotations;
    assert_eq!(a.len(), 3);
    assert!(a.iter().filter(|r| r.in_reply_to.as_deref() == Some(nm.as_str())).count() == 2);
    assert!(a.iter().any(|r| r.state.as_deref() == Some("Completed")));
    assert_ne!(a[1].name, a[2].name, "every comment gets its own id");

    s.apply(id, Edit::MoveAnnotation { page: 1, index: 0, dx: 50.0, dy: 0.0 }).unwrap();
    assert_eq!(pixel(&s, id, 1, 100, 100), [255, 255, 255, 255]);
    let px = pixel(&s, id, 1, 150, 100);
    assert!(px[0] > 200 && px[1] < 40, "moved: {px:?}");

    s.apply(id, Edit::DeleteAnnotation { page: 1, index: 0 }).unwrap();
    assert!(s.get(id).unwrap().info.annotations.is_empty(), "replies go with their parent");
    for _ in 0..4 {
        s.undo(id).unwrap();
    }
    assert_eq!(s.get(id).unwrap().info.annotations.len(), 1);
    let px = pixel(&s, id, 1, 100, 100);
    assert!(px[0] > 200 && px[1] < 40, "back where it was: {px:?}");
}

#[test]
fn highlight_multiplies_over_text() {
    let (mut s, id) = session_with(1);
    // "Page 1" is drawn at 20,150 in 24 pt Helvetica.
    let quad = [18.0, 172.0, 100.0, 172.0, 18.0, 145.0, 100.0, 145.0];
    s.apply(
        id,
        Edit::AddAnnotation(NewAnnotation {
            page: 0,
            shape: Shape::TextMarkup { kind: Markup::Highlight, quads: vec![quad] },
            style: Style { color: [1.0, 1.0, 0.0], ..Style::default() },
            contents: String::new(),
            author: String::new(),
        }),
    )
    .unwrap();
    let a = &s.get(id).unwrap().info.annotations[0];
    assert_eq!(a.subtype, "Highlight");
    assert_eq!(a.quads, vec![quad.map(|v| v as f32)]);
    // Background inside the quad turns yellow; text stays dark (multiply).
    let bg = pixel(&s, id, 0, 19, 170);
    assert!(bg[0] > 240 && bg[1] > 240 && bg[2] < 30, "{bg:?}");
}

#[test]
fn comment_permission_is_enforced() {
    let mut cos = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
    // Owner "own", empty user password, everything allowed except commenting (bit 6).
    let params = pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Aes256,
        user_password: "",
        owner_password: "own",
        permissions: !(1 << 5),
        encrypt_metadata: true,
        seed: [7; 32],
    };
    cos.set_encryption(&params).unwrap();
    let bytes = pdfcraft_cos::write_full(&cos, &pdfcraft_cos::SaveOptions::default()).unwrap();
    let mut s = Session::new();
    let id = s.open("locked.pdf", None, Arc::new(bytes), None).unwrap();
    let err = s.apply(id, rect_comment(0, [10.0, 10.0, 50.0, 50.0])).unwrap_err();
    assert_eq!(err, EditError::NotPermitted("comments"));
}

#[test]
fn saving_a_password_protected_document_rebases_on_it() {
    let bytes = protected("pw", "owner", -1);
    let mut s = Session::new();
    let id = s.open("x.pdf", None, bytes, Some("pw")).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved.clone(), None).expect("rebases on the encrypted file");
    assert!(!s.get(id).unwrap().dirty);
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: 90 }).unwrap();
    let again = s.save_bytes(id).unwrap();
    assert_eq!(&again[..saved.len()], &saved[..], "the next save appends");
}

#[test]
fn protect_with_an_open_password_then_save_reopen_and_undo() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::Protect(protection(Some("secret"), None))).unwrap();
    let d = s.get(id).unwrap();
    assert!(d.info.encrypted, "the working file is encrypted: {:?}", d.info.warnings);
    assert_eq!(d.security_summary().unwrap().method, "AES, 256-bit");
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2"], "still viewable in this session");
    let saved = s.save_bytes(id).unwrap();
    assert!(pdfcraft_cos::Document::open(saved.clone()).is_err(), "needs the password");
    s.mark_saved(id, saved.clone(), None).unwrap();
    // Further edits keep working and saving stays encrypted.
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let again = s.save_bytes(id).unwrap();
    let mut s2 = Session::new();
    assert!(s2.open("p.pdf", None, again.clone(), None).is_err());
    let id2 = s2.open("p.pdf", None, again, Some("secret")).unwrap();
    assert_eq!(s2.get(id2).unwrap().info.pages[0].rotation, 90);
    // Nothing is restricted without a permissions password: the opener may remove security.
    assert!(s2.get(id2).unwrap().allows_security_change());
    s2.apply(id2, Edit::RemoveProtection).unwrap();
    let plain = s2.save_bytes(id2).unwrap();
    assert!(pdfcraft_cos::Document::open(plain).is_ok());
}

#[test]
fn permissions_password_restricts_others_but_not_this_session() {
    let (mut s, id) = session_with(1);
    let p = Protection { printing: Printing::Low, changes: Changes::CommentFillSign, copy: false, ..protection(None, Some("boss")) };
    s.apply(id, Edit::Protect(p.clone())).unwrap();
    assert!(s.get(id).unwrap().allows_modification(), "the author keeps full rights");
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved.clone(), None).unwrap();
    assert!(s.get(id).unwrap().allows_modification(), "…also after saving (re-opened as owner)");
    // Someone else opens it without a password: restricted as chosen.
    let mut s2 = Session::new();
    let id2 = s2.open("r.pdf", None, saved.clone(), None).unwrap();
    let perm = s2.get(id2).unwrap().permissions().unwrap();
    assert!(perm.print() && !perm.print_high_quality() && perm.annotate() && perm.fill_forms() && !perm.copy() && !perm.modify());
    assert!(perm.extract_for_accessibility());
    assert!(!s2.get(id2).unwrap().allows_security_change());
    assert_eq!(s2.apply(id2, Edit::RemoveProtection), Err(EditError::NotPermitted("changing security")));
    assert!(s2.get(id2).unwrap().allows_annotation(), "commenting was allowed");
    // The owner password lifts everything.
    let id3 = s2.open("r.pdf", None, saved, Some("boss")).unwrap();
    assert!(s2.get(id3).unwrap().allows_security_change());
}

/// Converting to PDF/A takes the encryption off (PDF/A forbids it), so on an encrypted document
/// it needs the same rights as removing security, not just the right to make changes (#3921).
#[test]
fn convert_to_pdfa_needs_the_owners_rights_on_an_encrypted_document() {
    let pdfa = || Edit::ConvertPdfA { level: pdfcraft_preflight::Level::A2b };
    let refused = Err(EditError::NotPermitted("removing the encryption, which PDF/A forbids"));
    // Changes allowed; printing (bit 3) and copying (bit 5) withheld.
    let bytes = protected("pw", "owner", !((1 << 2) | (1 << 4)));
    let mut s = Session::new();
    let id = s.open("x.pdf", None, bytes.clone(), Some("pw")).unwrap();
    assert!(s.get(id).unwrap().allows_modification());
    assert_eq!(s.apply(id, pdfa()), refused);
    // Nor inside a batch.
    let batch = Edit::Batch { label: "x".into(), edits: vec![Edit::RotatePages { pages: vec![0], degrees: 90 }, pdfa()] };
    assert_eq!(s.apply(id, batch), refused);
    // Nothing was applied: no undo step, and saving gives back the encrypted original.
    assert!(s.get(id).unwrap().can_undo().is_none());
    assert_eq!(s.save_bytes(id).unwrap().as_slice(), bytes.as_slice());
    // The owner's password allows it, and the result is no longer encrypted.
    let id2 = s.open("x.pdf", None, bytes, Some("owner")).unwrap();
    s.apply(id2, pdfa()).unwrap();
    let saved = s.save_bytes(id2).unwrap();
    assert!(pdfcraft_cos::Document::open(saved).unwrap().trailer().get(b"Encrypt").is_none());
    // An unencrypted document needs nothing more.
    let (mut s, id) = session_with(1);
    s.apply(id, pdfa()).unwrap();
}

#[test]
fn combine_source_check_says_why_a_file_cannot_be_combined() {
    let (mut s, id) = session_with(1);
    let plain = s.get(id).unwrap().bytes.clone();
    assert_eq!(crate::combine_source_check(&plain, None), Ok(()));
    // Page assembly withheld (Changes::None) without a password to open it.
    let p = Protection { changes: Changes::None, ..protection(None, Some("boss")) };
    s.apply(id, Edit::Protect(p)).unwrap();
    let locked = s.save_bytes(id).unwrap();
    assert_eq!(crate::combine_source_check(&locked, None), Err(crate::SourceProblem::NotPermitted));
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::Protect(protection(Some("pw"), Some("owner")))).unwrap();
    let secret = s.save_bytes(id).unwrap();
    assert_eq!(crate::combine_source_check(&secret, None), Err(crate::SourceProblem::Password));
    assert_eq!(crate::combine_source_check(&secret, Some("nope")), Err(crate::SourceProblem::WrongPassword));
    // Its open password reads it, but its permissions withhold assembling pages: the owner's
    // password allows it.
    assert_eq!(crate::combine_source_check(&secret, Some("pw")), Err(crate::SourceProblem::NotPermitted));
    assert_eq!(crate::combine_source_check(&secret, Some("owner")), Ok(()));
    // The permissions password lifts the restriction on copying pages.
    assert_eq!(crate::combine_source_check(&locked, Some("boss")), Ok(()));
    // Combining with the passwords: the result opens without any.
    let s = Session::new();
    let sources = vec![("secret".to_string(), secret, None), ("locked".to_string(), locked, None)];
    assert!(s.combine_ranges(&sources).is_err(), "no passwords, no combining");
    let out = s.combine_unlocked(&sources, &[Some("owner"), Some("boss")]).unwrap();
    let combined = pdfcraft_cos::Document::open(out).unwrap();
    assert!(combined.permissions().is_none(), "the combined file is not encrypted");
    assert_eq!(pdfcraft_organize::page_count(&combined).unwrap(), 2);
    assert!(matches!(crate::combine_source_check(&Arc::new(b"not a pdf".to_vec()), None), Err(crate::SourceProblem::Unreadable(_))));
}

#[test]
fn protection_is_validated_undoable_and_never_logged() {
    let (mut s, id) = session_with(1);
    assert!(matches!(s.apply(id, Edit::Protect(protection(None, None))), Err(EditError::Protection(_))));
    assert!(matches!(s.apply(id, Edit::Protect(protection(Some("same"), Some("same")))), Err(EditError::Protection(_))));
    let rc4 = Protection { algorithm: pdfcraft_cos::Algorithm::Rc4_128, ..protection(Some("pässword"), None) };
    assert!(matches!(s.apply(id, Edit::Protect(rc4)), Err(EditError::Protection(_))));
    assert!(!format!("{:?}", Edit::Protect(protection(Some("hunter2"), Some("x")))).contains("hunter2"));
    s.apply(id, Edit::Protect(protection(Some("pw"), Some("owner")))).unwrap();
    assert!(s.get(id).unwrap().info.encrypted);
    s.undo(id).unwrap();
    assert!(!s.get(id).unwrap().info.encrypted, "undo removes the pending protection");
    assert!(s.get(id).unwrap().security_summary().is_none());
    s.redo(id).unwrap();
    assert!(s.get(id).unwrap().info.encrypted);
    assert_eq!(page_texts(&s, id), ["Page 1"]);
}

#[test]
fn passwords_that_encryption_would_make_empty_or_equal_are_refused() {
    use pdfcraft_cos::Algorithm::{Aes128, Rc4_128};
    use pdfcraft_cos::Auth::{Owner, User};
    let empty = "passwords can't be empty or made only of characters that passwords ignore, such as soft hyphens";
    let same = "the open password and the permissions password must be different (PDF readers treat these two as the same password)";
    // AES-256 keys on the SASLprep form of a password (soft hyphens dropped, then NFKC), RC4 and
    // AES-128 on its first 32 bytes. Written as typed, these would make a file that opens
    // without a password, or whose open password lifts the restrictions.
    let (a31, a32) = ("a".repeat(31), "a".repeat(32));
    let (long1, long2) = (format!("{a32}1"), format!("{a32}2"));
    // The 32-byte padding starts with "(": 31 characters and the same followed by "(" are one key.
    let padded = format!("{a31}(");
    let cases = [
        ("soft hyphen open password", protection(Some("\u{AD}"), None), empty),
        ("soft hyphen permissions password", protection(None, Some("\u{AD}")), empty),
        ("soft hyphen permissions password beside an open one", protection(Some("0"), Some("\u{AD}")), empty),
        ("fullwidth A, then A", protection(Some("\u{FF21}"), Some("A")), same),
        ("A, then fullwidth A", protection(Some("A"), Some("\u{FF21}")), same),
        ("fi ligature, then fi", protection(Some("\u{FB01}"), Some("fi")), same),
        ("fi, then fi ligature", protection(Some("fi"), Some("\u{FB01}")), same),
        ("RC4-128, same first 32 bytes", Protection { algorithm: Rc4_128, ..protection(Some(&long1), Some(&long2)) }, same),
        ("AES-128, same first 32 bytes", Protection { algorithm: Aes128, ..protection(Some(&long1), Some(&long2)) }, same),
        ("RC4-128, same once padded", Protection { algorithm: Rc4_128, ..protection(Some(&a31), Some(&padded)) }, same),
        ("AES-128, same once padded", Protection { algorithm: Aes128, ..protection(Some(&a31), Some(&padded)) }, same),
    ];
    let got: Vec<_> = cases.iter().map(|(case, p, _)| (*case, protect_and_reopen(p))).collect();
    let want: Vec<_> = cases.iter().map(|(case, _, message)| (*case, (Err(EditError::Protection(message.to_string())), None))).collect();
    assert_eq!(got, want, "each is refused, so no file is written");
    // Passwords that are empty or equal as typed keep their messages.
    let (mut s, id) = session_with(1);
    assert_eq!(s.apply(id, Edit::Protect(protection(Some(""), None))), Err(EditError::Protection("passwords can't be empty".into())));
    assert_eq!(
        s.apply(id, Edit::Protect(protection(Some("same"), Some("same")))),
        Err(EditError::Protection("the open password and the permissions password must be different".into()))
    );
    // Passwords that stay different are fine: only the permissions password gives owner rights.
    let distinct = [
        protection(Some("A"), Some("\u{FF21}\u{FF22}")),
        Protection { algorithm: Rc4_128, ..protection(Some(&format!("1{a32}")), Some(&format!("2{a32}"))) },
        Protection { algorithm: Aes128, ..protection(Some(&format!("1{a32}")), Some(&format!("2{a32}"))) },
    ];
    for p in &distinct {
        assert_eq!(protect_and_reopen(p), (Ok(()), Some([None, Some(User), Some(Owner)])), "{:?}", p.algorithm);
    }
}

#[test]
fn fresh_sessions_protecting_identical_documents_produce_different_salts() {
    // Characterization only: two sessions' salts differ. A per-call RandomState already did
    // that before Protect drew from the platform CSPRNG, so this does not prove the source.
    // `protect_fails_closed_when_randomness_is_unavailable` is the guard. It does not compile
    // against a Protect with no CSPRNG hook, and it fails closed when that hook errors or
    // returns zeros. Comparing the stored function with `getrandom::fill` is not reliable:
    // one function can have different addresses in different codegen units.
    let raw = Arc::new(fixture(1));
    let encrypt_bytes = |saved: Arc<Vec<u8>>, key: &[u8]| -> Vec<u8> {
        let doc = pdfcraft_cos::Document::open_with_password(saved, Some("secret")).unwrap();
        let enc = doc.trailer().get(b"Encrypt").map(|o| doc.resolve(o)).unwrap();
        enc.as_dict().unwrap().get(key).map(|o| doc.resolve(o)).unwrap().as_string().unwrap().bytes.clone()
    };
    let protect_once = |p: Protection| -> Arc<Vec<u8>> {
        let mut s = Session::new();
        let id = s.open("same.pdf", None, raw.clone(), None).unwrap();
        s.apply(id, Edit::Protect(p)).unwrap();
        s.save_bytes(id).unwrap()
    };
    let a = protect_once(protection(Some("secret"), None));
    let b = protect_once(protection(Some("secret"), None));
    let (u_a, u_b) = (encrypt_bytes(a.clone(), b"U"), encrypt_bytes(b.clone(), b"U"));
    let (o_a, o_b) = (encrypt_bytes(a.clone(), b"O"), encrypt_bytes(b.clone(), b"O"));
    let (ue_a, ue_b) = (encrypt_bytes(a.clone(), b"UE"), encrypt_bytes(b.clone(), b"UE"));
    let (oe_a, oe_b) = (encrypt_bytes(a, b"OE"), encrypt_bytes(b, b"OE"));
    assert_eq!((u_a.len(), u_b.len(), o_a.len(), o_b.len()), (48, 48, 48, 48));
    assert_ne!(&u_a[32..48], &u_b[32..48], "user validation and key salts must differ across fresh sessions");
    assert_ne!(&o_a[32..48], &o_b[32..48], "owner validation and key salts must differ across fresh sessions");
    assert_ne!(ue_a, ue_b, "wrapped user file keys must differ across fresh sessions");
    assert_ne!(oe_a, oe_b, "wrapped owner file keys must differ across fresh sessions");

    // Generated owner passwords in R4 (AES-128) must also come from CSPRNG entropy, even when the
    // document already carries a fixed /ID and only an open password is set.
    let mut with_id = pdfcraft_cos::Document::open(raw).unwrap();
    let fixed_id = pdfcraft_cos::Object::String(pdfcraft_cos::PdfString { bytes: b"0123456789abcdef".to_vec(), hex: true });
    with_id.trailer_mut().set(b"ID".to_vec(), pdfcraft_cos::Object::Array(vec![fixed_id.clone(), fixed_id]));
    let raw_with_id = Arc::new(pdfcraft_cos::write_full(&with_id, &Default::default()).unwrap());
    let protect_r4 = || -> Vec<u8> {
        let mut s = Session::new();
        let id = s.open("same.pdf", None, raw_with_id.clone(), None).unwrap();
        let p = Protection { algorithm: pdfcraft_cos::Algorithm::Aes128, ..protection(Some("secret"), None) };
        s.apply(id, Edit::Protect(p)).unwrap();
        encrypt_bytes(s.save_bytes(id).unwrap(), b"O")
    };
    assert_ne!(protect_r4(), protect_r4(), "generated owner passwords must differ across fresh sessions");
}

#[test]
fn protect_fails_closed_when_randomness_is_unavailable() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn fail_rng(_: &mut [u8]) -> Result<(), getrandom::Error> {
        Err(getrandom::Error::UNSUPPORTED)
    }
    fn zero_rng(out: &mut [u8]) -> Result<(), getrandom::Error> {
        out.fill(0);
        Ok(())
    }
    fn fail_second_rng(out: &mut [u8]) -> Result<(), getrandom::Error> {
        if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
            out.fill(0xA5);
            Ok(())
        } else {
            Err(getrandom::Error::UNSUPPORTED)
        }
    }
    CALLS.store(0, Ordering::SeqCst);
    for rng in [fail_rng, zero_rng, fail_second_rng] {
        let mut doc = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
        let mut cx = EditCtx::new(None, 0);
        cx.rng = rng;
        let err = run_edit(&mut doc, &Edit::Protect(protection(Some("secret"), None)), &mut cx);
        assert!(matches!(err, Err(EditError::Protection(_))), "Protect must fail closed when CSPRNG fails: {err:?}");
        assert!(doc.output_handler().is_none(), "document must stay unprotected when CSPRNG fails");
    }
}

/// #3926: a value typed into a text field with the Password flag stays in the open document but
/// is written to no file: Save, a full rewrite, Reduce File Size, a recovery snapshot, or a new
/// file made from its pages (Extract, Split, Combine). ISO
/// 32000-2 §12.7.5.3, Table 231, note to the Password flag: PDF processors should never store
/// the value in the PDF file.
#[test]
fn password_field_values_are_kept_in_memory_but_written_to_no_file() {
    let fixture = String::from_utf8(form_fixture()).unwrap().replace("/T (name)", "/T (name) /Ff 8192");
    // Offsets shift by the inserted text; the parser rebuilds the cross-reference table.
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, Arc::new(fixture.into_bytes()), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "name".into(), value: FieldValue::Text("hunter2".into()) }).unwrap();
    let field = s.get(id).unwrap().form.iter().find(|f| f.name == "name").cloned().unwrap();
    assert!(field.has(pdfcraft_forms::flags::PASSWORD));
    assert_eq!(field.value, vec!["hunter2".to_string()], "the open document keeps the value");
    assert!(appears_in_pdf(&s.get(id).unwrap().bytes, "hunter2"), "the working bytes do hold it (the test can see it)");

    let saved = s.save_bytes(id).unwrap();
    assert!(!appears_in_pdf(&saved, "hunter2"), "Save");
    // As Acrobat saves it (#3943 review): no default value, so an empty field whose appearance
    // no longer shows how long the typed password was.
    assert!(!appears_in_pdf(&saved, "*******"), "the typed length is not kept in the appearance");
    assert!(!appears_in_pdf(&s.save_full_bytes(id).unwrap(), "hunter2"), "a full rewrite");
    assert!(!appears_in_pdf(&s.reduced_bytes(id).unwrap().0, "hunter2"), "Reduce File Size");
    let snaps = s.autosave_snapshots();
    assert_eq!(snaps.len(), 1);
    assert!(!appears_in_pdf(&snaps[0].bytes, "hunter2"), "the recovery snapshot");
    // New files built from its pages too (#3943 review): Extract copies the fields of the pages it
    // takes, and Split writes each part the same way.
    let extracted = s.extract(id, &[0]).unwrap();
    assert!(!appears_in_pdf(&extracted, "hunter2"), "Extract Pages");
    assert!(!appears_in_pdf(&extracted, "*******"), "nor on the extracted page");
    for (_, _, part) in s.split_by_size(id, 1).unwrap() {
        assert!(!appears_in_pdf(&part, "hunter2"), "Split");
    }
    assert!(!appears_in_pdf(&s.combine(&[("form.pdf".into(), s.get(id).unwrap().bytes.clone())]).unwrap(), "hunter2"), "Combine");
    // Reopened, the field is empty, as in any reader.
    let mut t = Session::new();
    let reopened = t.open("saved.pdf", None, saved, None).unwrap();
    assert!(t.get(reopened).unwrap().form.iter().find(|f| f.name == "name").unwrap().value.iter().all(|v| v.is_empty()));
}

/// #3943 review: recording a save rebases editing on the saved file, which carries no Password
/// value; the typed value stays in the open document all the same, and a later save still omits it.
#[test]
fn a_typed_password_value_survives_save_and_mark_saved() {
    let fixture = String::from_utf8(form_fixture()).unwrap().replace("/T (name)", "/T (name) /Ff 8192");
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, Arc::new(fixture.into_bytes()), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "name".into(), value: FieldValue::Text("hunter2".into()) }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, Some("saved.pdf".into())).unwrap();
    let value = |s: &Session| s.get(id).unwrap().form.iter().find(|f| f.name == "name").unwrap().value.clone();
    assert_eq!(value(&s), ["hunter2"], "the open document keeps the value after Save");
    assert!(!s.get(id).unwrap().dirty, "restoring it is not an unsaved change");
    assert!(!appears_in_pdf(&s.save_bytes(id).unwrap(), "hunter2"), "the next save still omits it");
}

/// #3926 / #3943 review: signing writes the signed file from the same copy a save does, so a
/// typed Password-field value is not signed into the file either.
#[test]
fn a_signed_file_carries_no_password_field_value() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let fixture = String::from_utf8(form_fixture()).unwrap().replace("/T (name)", "/T (name) /Ff 8192");
    // After the test certificate's start date (2026-10-02).
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("form.pdf", None, Arc::new(fixture.into_bytes()), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "name".into(), value: FieldValue::Text("hunter2".into()) }).unwrap();
    assert!(appears_in_pdf(&s.get(id).unwrap().bytes, "hunter2"), "the working bytes hold it");
    let signed = s.sign(id, &digital_id, SignOptions { rect: None, ..SignOptions::default() }).unwrap();
    assert!(!appears_in_pdf(&signed, "hunter2"), "the signed file");
    assert!(!appears_in_pdf(&signed, "*******"), "nor the typed length");
}

/// #3944: a value typed into an XFA `<passwordEdit/>` field stays in the open document (both the
/// widget and the live XFA datasets, even across multiple edits and across Save + `mark_saved`),
/// but is written to no file — neither on the widget nor in the XFA datasets stream.
#[test]
fn xfa_password_field_values_are_not_saved_in_datasets_or_widgets() {
    let xfa_pdf = pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(""));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("xfa.pdf", None, Arc::new(xfa_pdf), None).unwrap();
    let pin_name = s.get(id).unwrap().form.iter().find(|f| f.name.contains("pin")).unwrap().name.clone();

    // Edit the password twice so an earlier edit's datasets stream could not linger in the overlay.
    s.apply(id, Edit::SetFieldValue { name: pin_name.clone(), value: FieldValue::Text("secret1".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: pin_name.clone(), value: FieldValue::Text("hunter2".into()) }).unwrap();

    let field = s.get(id).unwrap().form.iter().find(|f| f.name == pin_name).cloned().unwrap();
    assert!(field.has(pdfcraft_forms::flags::PASSWORD));
    assert_eq!(field.value, vec!["hunter2".to_string()], "the open document keeps the value");
    assert!(appears_in_pdf(&s.get(id).unwrap().bytes, "hunter2"), "the working bytes hold the live datasets");
    assert!(!appears_in_pdf(&s.get(id).unwrap().bytes, "secret1"), "the first edit's datasets stream was replaced in place");

    let saved = s.save_bytes(id).unwrap();
    for leak in ["hunter2", "secret1", "*******"] {
        assert!(!appears_in_pdf(&saved, leak), "Save leaked {leak}");
        assert!(!appears_in_pdf(&s.save_full_bytes(id).unwrap(), leak), "save_full_bytes leaked {leak}");
        assert!(!appears_in_pdf(&s.reduced_bytes(id).unwrap().0, leak), "reduced_bytes leaked {leak}");
    }
    let snaps = s.autosave_snapshots();
    assert_eq!(snaps.len(), 1);
    for leak in ["hunter2", "secret1", "*******"] {
        assert!(!appears_in_pdf(&snaps[0].bytes, leak), "autosave snapshot leaked {leak}");
    }

    // Reopening the saved file in a new session leaves the password field empty (it does not
    // repopulate from the XFA datasets on open).
    let mut t = Session::new();
    let reopened = t.open("saved.pdf", None, saved.clone(), None).unwrap();
    assert!(
        t.get(reopened).unwrap().form.iter().find(|f| f.name == pin_name).unwrap().value.iter().all(|v| v.is_empty()),
        "reopened XFA form must have an empty password field"
    );

    // Recording the save keeps the typed password in the open document (and its live datasets),
    // while a subsequent save still omits it.
    s.mark_saved(id, saved, Some("saved.pdf".into())).unwrap();
    assert_eq!(s.get(id).unwrap().form.iter().find(|f| f.name == pin_name).unwrap().value, ["hunter2"]);
    assert!(!s.get(id).unwrap().dirty);
    for leak in ["hunter2", "secret1", "*******"] {
        assert!(!appears_in_pdf(&s.save_bytes(id).unwrap(), leak), "save after mark_saved leaked {leak}");
    }

    // Undo past `mark_saved` and edit again: the datasets stream in the restored overlay is
    // overwritten in place rather than left behind.
    s.undo(id).unwrap();
    s.apply(id, Edit::SetFieldValue { name: pin_name.clone(), value: FieldValue::Text("secret3".into()) }).unwrap();
    let saved_after_undo = s.save_bytes(id).unwrap();
    for leak in ["hunter2", "secret1", "secret3", "*******"] {
        assert!(!appears_in_pdf(&saved_after_undo, leak), "save after undo leaked {leak}");
    }

    // A relayout (`rerender`) after typing a password frees the replaced generated widget and its
    // masked appearance stream so neither remains in the overlay when saving.
    let relayout_xml = pdfcraft_xfa::fixtures::fields_template("").replace(
        r#"<subform name="page1" layout="tb">"#,
        r#"<subform name="page1" layout="tb"><field name="grow" w="2in"><ui><textEdit multiLine="1"/></ui></field>"#,
    );
    let id2 = s.open("relayout.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&relayout_xml)), None).unwrap();
    let pin2 = s.get(id2).unwrap().form.iter().find(|f| f.name.contains("pin")).unwrap().name.clone();
    let grow2 = s.get(id2).unwrap().form.iter().find(|f| f.name.contains("grow")).unwrap().name.clone();
    s.apply(id2, Edit::SetFieldValue { name: pin2, value: FieldValue::Text("s3cr3t!".into()) }).unwrap();
    s.apply(id2, Edit::SetFieldValue { name: grow2, value: FieldValue::Text("line1\nline2\nline3\nline4\nline5".into()) }).unwrap();
    let saved_relayout = s.save_bytes(id2).unwrap();
    for leak in ["s3cr3t!", "*******"] {
        assert!(!appears_in_pdf(&saved_relayout, leak), "save after relayout leaked {leak}");
    }
}

/// A value its field's font can't draw is kept, and the edit that drew it reports it in
/// `edit_notices` (the interface shows it, tools return it): not a later unrelated edit, not a
/// failed one, and not a value that can be drawn. The form (an inline AcroForm here) asks other
/// viewers to redraw it.
#[test]
fn a_value_its_font_cannot_draw_is_reported_by_the_edit_that_drew_it() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, Arc::new(form_fixture()), None).unwrap();
    let fill = |s: &mut Session, name: &str, v: FieldValue| s.apply(id, Edit::SetFieldValue { name: name.into(), value: v });
    fill(&mut s, "name", FieldValue::Text("张三".into())).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.form[0].value, ["张三"]);
    let [EditNotice::Undrawn(n)] = d.edit_notices.as_slice() else { panic!("{:?}", d.edit_notices) };
    assert_eq!((n.field.as_deref(), n.page, n.ch, n.redrawn_elsewhere), (Some("name"), 0, '张', true));
    let said = d.edit_notices[0].to_string();
    assert!(said.contains("\"张\" (U+5F20)") && said.contains("other PDF viewers redraw"), "{said}");
    fill(&mut s, "ok", FieldValue::Check(true)).unwrap();
    assert!(s.get(id).unwrap().edit_notices.is_empty(), "an unrelated edit reports nothing");
    fill(&mut s, "name", FieldValue::Text("李四".into())).unwrap();
    assert_eq!(s.get(id).unwrap().edit_notices.len(), 1, "a new value is reported again");
    assert!(fill(&mut s, "missing", FieldValue::Text("x".into())).is_err());
    assert!(s.get(id).unwrap().edit_notices.is_empty(), "no stale notice after a failed edit");
    fill(&mut s, "name", FieldValue::Text("Ada".into())).unwrap();
    assert!(s.get(id).unwrap().edit_notices.is_empty());
    let saved = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    assert!(pdfcraft_forms::need_appearances(&saved));
}

#[test]
fn filling_a_form_shows_saves_and_undoes() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, Arc::new(form_fixture()), None).unwrap();
    assert_eq!(s.get(id).unwrap().form.len(), 2);
    s.apply(id, Edit::SetFieldValue { name: "name".into(), value: FieldValue::Text("Ada Lovelace".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "ok".into(), value: FieldValue::Check(true) }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.can_undo(), Some("Fill in ok"));
    assert_eq!(d.form[0].value, ["Ada Lovelace"]);
    assert_eq!(d.form[1].value, ["Yes"]);
    assert_eq!(page_texts(&s, id), ["Ada Lovelace"], "the new appearance shows the text");
    let err = s.apply(id, Edit::SetFieldValue { name: "missing".into(), value: FieldValue::Text("x".into()) }).unwrap_err();
    assert_eq!(err.to_string(), "there is no field named \"missing\"");
    s.apply(id, Edit::ResetForm { names: None }).unwrap();
    assert!(s.get(id).unwrap().form.iter().all(|f| f.value.is_empty()));
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().form[0].value, ["Ada Lovelace"]);
    let saved = s.save_bytes(id).unwrap();
    let mut s2 = Session::new();
    let id2 = s2.open("f.pdf", None, saved, None).unwrap();
    assert_eq!(s2.get(id2).unwrap().form[1].value, ["Yes"]);
}

#[test]
fn comment_edits_refresh_the_list_exactly_as_a_full_inspection_would() {
    let (mut s, id) = session_with(3);
    let add = |page: usize, shape: Shape, contents: &str| {
        Edit::AddAnnotation(NewAnnotation {
            page,
            shape,
            style: Style::default_for(&Shape::Rectangle { rect: [0.0; 4] }),
            contents: contents.into(),
            author: "Ada".into(),
        })
    };
    s.apply(id, add(0, Shape::Rectangle { rect: [10.0, 10.0, 60.0, 60.0] }, "box")).unwrap();
    s.apply(id, add(1, Shape::TextMarkup { kind: Markup::Highlight, quads: vec![[20.0, 170.0, 90.0, 170.0, 20.0, 150.0, 90.0, 150.0]] }, ""))
        .unwrap();
    s.apply(id, add(1, Shape::TextBox { rect: [10.0, 200.0, 150.0, 240.0], font_size: 11.0 }, "Text box")).unwrap();
    s.apply(id, add(2, Shape::Note { at: [100.0, 280.0], icon: NoteIcon::Comment }, "Sticky")).unwrap();
    s.apply(id, Edit::ReplyToAnnotation { page: 2, index: 0, text: "Reply".into(), author: "Bob".into() }).unwrap();
    s.apply(id, Edit::SetAnnotationStatus { page: 2, index: 0, state: ReviewState::Accepted, author: "Bob".into() }).unwrap();
    s.apply(id, Edit::MarkAnnotation { page: 2, index: 0, marked: true, author: "Bob".into() }).unwrap();
    s.apply(id, Edit::LockAnnotation { page: 1, index: 1, locked: true }).unwrap();
    s.apply(id, Edit::MoveAnnotation { page: 0, index: 0, dx: 5.0, dy: 5.0 }).unwrap();
    s.apply(id, Edit::DeleteAnnotation { page: 1, index: 0 }).unwrap();
    s.undo(id).unwrap();
    let d = s.get(id).unwrap();
    let full = pdfcraft_render::inspect(d.bytes.clone(), None).unwrap();
    assert_eq!(format!("{:?}", d.info.annotations), format!("{:?}", full.annotations));
    assert_eq!(d.info.file_size, full.file_size);
    assert_eq!(d.info.annotations.len(), 7);
    assert!(d.info.annotations.iter().any(|a| a.locked));
}

#[test]
fn form_edits_refresh_field_values_without_a_full_inspection() {
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(form_fixture()), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "name".into(), value: FieldValue::Text("Ada".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "ok".into(), value: FieldValue::Check(true) }).unwrap();
    let d = s.get(id).unwrap();
    let full = pdfcraft_render::inspect(d.bytes.clone(), None).unwrap();
    let values = |fs: &[pdfcraft_render::Field]| fs.iter().map(|f| (f.name.clone(), f.value.clone())).collect::<Vec<_>>();
    assert_eq!(values(&d.info.fields), values(&full.fields));
    assert_eq!(values(&d.info.fields), [("name".to_string(), Some("Ada".to_string())), ("ok".to_string(), Some("Yes".to_string()))]);
}

#[test]
fn form_refresh_keeps_first_value_when_field_names_repeat() {
    // Two widgets share one name; the refresh reads the first.
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 6 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R 6 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (dup) /V (first) /Rect [20 250 180 270] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (dup) /V (second) /Rect [20 220 180 240] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (other) /Rect [20 190 180 210] /P 3 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new();
    let id = s.open("dup.pdf", None, Arc::new(bytes), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "other".into(), value: FieldValue::Text("x".into()) }).unwrap();
    let dup = s.get(id).unwrap().info.fields.iter().find(|f| f.name == "dup").unwrap().value.clone();
    assert_eq!(dup.as_deref(), Some("first"));
}

#[test]
fn multiselect_choice_indices_stay_sorted_through_engine_save_and_reopen() {
    let (mut s, id) = session_with(1);
    s.apply(
        id,
        Edit::AddField {
            page: 0,
            rect: [20.0, 180.0, 180.0, 240.0],
            kind: NewField::List { options: vec!["Cheese".into(), "Ham".into(), "Olives".into()], multi: true },
            name: Some("toppings".into()),
        },
    )
    .unwrap();
    let chosen = vec!["Olives".to_string(), "Ham".to_string()];
    s.apply(id, Edit::SetFieldValue { name: "toppings".into(), value: FieldValue::Choice(chosen.clone()) }).unwrap();
    assert_eq!(s.get(id).unwrap().form[0].value, chosen, "the live engine keeps selection order");
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let all = pdfcraft_forms::fields(&cos);
    let list = all.iter().find(|field| field.name == "toppings").unwrap();
    let object = cos.get(list.obj);
    let indices = object.as_dict().unwrap().get(b"I").unwrap().as_array().unwrap();
    assert_eq!(indices.iter().map(|index| index.as_int().unwrap()).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(list.value, chosen, "sorted /I does not reorder /V");
    let reopened = s.open("reopened.pdf", None, saved, None).unwrap();
    assert_eq!(s.get(reopened).unwrap().form[0].value, chosen, "reopening keeps selection order");
}

#[test]
fn preparing_a_form_adds_renames_and_deletes_fields() {
    let (mut s, id) = session_with(1);
    assert!(s.get(id).unwrap().form.is_empty());
    let add = |kind: NewField, y: f64| Edit::AddField { page: 0, rect: [20.0, y, 180.0, y + 22.0], kind, name: None };
    s.apply(id, add(NewField::Text { multiline: false }, 250.0)).unwrap();
    s.apply(id, add(NewField::CheckBox, 200.0)).unwrap();
    s.apply(id, add(NewField::Text { multiline: false }, 150.0)).unwrap();
    let names = |s: &Session| s.get(id).unwrap().form.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&s), ["Text1", "Check Box1", "Text2"]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add field"));
    let props = FieldProps { name: Some("email".into()), tooltip: Some("Your e-mail".into()), required: Some(true), ..Default::default() };
    s.apply(id, Edit::SetFieldProps { name: "Text2".into(), props: Box::new(props) }).unwrap();
    s.apply(id, Edit::DeleteField { name: "Check Box1".into() }).unwrap();
    assert_eq!(names(&s), ["Text1", "email"]);
    s.apply(id, Edit::SetFieldValue { name: "email".into(), value: FieldValue::Text("ada@example.org".into()) }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let mut s2 = Session::new();
    let id2 = s2.open("f.pdf", None, saved, None).unwrap();
    let d = s2.get(id2).unwrap();
    assert_eq!(d.form.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Text1", "email"]);
    assert_eq!(d.form[1].value, ["ada@example.org"]);
    assert!(page_texts(&s2, id2).iter().any(|t| t.contains("ada@example.org")));
    s.undo(id).unwrap();
    s.undo(id).unwrap();
    assert_eq!(names(&s), ["Text1", "Check Box1", "email"]);
}

/// Setting the calculation order is one undoable edit that recalculates: values that ran too
/// early come out right at once, a script calculation included, and undo brings back the old
/// order and values together.
#[test]
fn setting_the_calculation_order_recalculates_and_undoes() {
    use form_scripts::{CalcOp, Calculate};
    let (mut s, id) = session_with(1);
    for (i, name) in ["qty", "sub", "total", "tax"].into_iter().enumerate() {
        let y = 250.0 - 40.0 * i as f64;
        let kind = NewField::Text { multiline: false };
        s.apply(id, Edit::AddField { page: 0, rect: [20.0, y, 180.0, y + 22.0], kind, name: Some(name.into()) }).unwrap();
    }
    let calculate = |name: &str, c: Calculate| Edit::SetFieldProps {
        name: name.into(),
        props: Box::new(FieldProps { calculate: Some(c), ..Default::default() }),
    };
    // Defined backwards: total, which reads sub, calculates first.
    s.apply(id, calculate("total", Calculate::Simple { op: CalcOp::Sum, fields: vec!["sub".into()] })).unwrap();
    s.apply(id, calculate("sub", Calculate::Notation("qty * 2".into()))).unwrap();
    let script = Some("event.value = this.getField(\"total\").value / 10;".to_string());
    s.apply(id, Edit::SetFieldScript { name: "tax".into(), event: "calculate".into(), script }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("5".into()) }).unwrap();
    let values = |s: &Session| {
        let form = &s.get(id).unwrap().form;
        ["sub", "total", "tax"].map(|n| form.iter().find(|f| f.name == n).and_then(|f| f.value.first().cloned()).unwrap_or_default())
    };
    assert_eq!(values(&s), ["10", "0", "0"], "total and tax ran before sub");
    let order = s.get(id).unwrap().calculation_order();
    assert_eq!(order.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["total", "sub", "tax"]);
    assert_eq!(order.fields[0].issues, [CalculationIssue::RunsTooEarly { field: "sub".into() }]);
    assert_eq!(order.fixed(), ["sub", "total", "tax"]);
    s.apply(id, Edit::SetCalculationOrder { names: order.fixed() }).unwrap();
    assert_eq!(values(&s), ["10", "10", "1"]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Set calculation order"));
    assert!(s.get(id).unwrap().calculation_order().fields.iter().all(|f| f.issues.is_empty()));
    s.undo(id).unwrap();
    assert_eq!(values(&s), ["10", "0", "0"]);
    assert_eq!(s.get(id).unwrap().calculation_order().fixed(), ["sub", "total", "tax"], "the old order is back");
    assert!(matches!(s.apply(id, Edit::SetCalculationOrder { names: vec!["qty".into()] }), Err(EditError::Form(_))));
}

/// Redaction removes text by the positions its own interpreter works out, and the renderer has
/// the last word: where the two disagree, a page that still shows a glyph mostly under a mark
/// fails the edit. The check is run here on a page nothing was removed from, as if the
/// interpreter had placed the word elsewhere. Only the overlay streams an apply added are left
/// out: a content stream of the input labelled like one (`/PCMark /Redaction`) is checked too.
#[test]
fn the_renderer_check_refuses_text_still_shown_under_a_mark() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let page = one_line_page_with("/Font << /F1 4 0 R >>", "/PCMark /Redaction", "BT /F1 16 Tf 20 150 Td (Public words then SECRET42) Tj ET");
    let id = s.open("shown.pdf", None, Arc::new(page), None).unwrap();
    mark_shown_text(&mut s, id, "SECRET42");
    let doc = s.get(id).unwrap();
    let mut cos = doc.editor.as_ref().unwrap().cos.clone();
    let marks = pdfcraft_redact::marks(&cos);
    let refused = |cos: &pdfcraft_cos::Document| {
        let shown = shown_under_redactions(cos, &marks, &[], &doc.info.pages, None);
        matches!(shown, Err(EditError::Redact(pdfcraft_redact::RedactError::Residue(n))) if n > 0)
    };
    assert!(refused(&cos));
    // The page's contents as an array, as they are once an apply has added its overlay.
    let page = pdfcraft_model::pages(&cos).swap_remove(0);
    let contents = page.dict.get(b"Contents").cloned().unwrap();
    cos.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), pdfcraft_cos::Object::Array(vec![contents]))).unwrap();
    assert!(refused(&cos), "the labelled input stream was not checked");
}

/// Inline image data `7FEI 1 0 0 1 200 0 cm` holds an `EI` the renderer ends the image at, so it
/// runs the `cm` and shows SECRET 200 pt to the right. Redaction reads the image the same way, so
/// a mark drawn over the word as shown takes it out of the file.
#[test]
fn text_after_an_inline_image_is_redacted_where_the_page_shows_it() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let page = one_line_page(
        "/Font << /F1 4 0 R >>",
        "BI /W 1 /H 1 /CS /G /BPC 8 /F /AHx ID 7FEI 1 0 0 1 200 0 cm EI BT /F1 10 Tf 10 100 Td (SECRET) Tj ET",
    );
    let id = s.open("inline.pdf", None, Arc::new(page), None).unwrap();
    mark_shown_text(&mut s, id, "SECRET");
    s.apply(id, Edit::ApplyRedactions { pages: None }).unwrap();
    assert!(!page_texts(&s, id).concat().contains("SECRET"));
    let saved = s.save_bytes(id).unwrap();
    let held = |d: &[u8]| d.windows(6).any(|w| w == b"SECRET");
    assert!(!held(&saved), "the saved file holds the redacted word");
    let reopened = pdfcraft_cos::Document::open(saved).unwrap();
    assert!(
        !reopened.scan_objects().any(|(_, o)| matches!(&*o, pdfcraft_cos::Object::Stream(st) if st.decoded().is_ok_and(|d| held(&d)))),
        "a saved stream holds the redacted word"
    );
}

/// A word whose font and size an ExtGState /Font entry (`gs`) sets is placed where the renderer
/// draws it, so a mark drawn over it as displayed takes it out of the file.
#[test]
fn a_word_in_a_font_set_by_an_extgstate_is_redacted() {
    let line = "Public words then SECRET42 and more";
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let by_gs = one_line_page("/ExtGState << /GS1 << /Type /ExtGState /Font [4 0 R 16] >> >>", &format!("BT /GS1 gs 20 150 Td ({line}) Tj ET"));
    let id = s.open("gs.pdf", None, Arc::new(by_gs), None).unwrap();
    mark_shown_text(&mut s, id, "SECRET42");
    s.apply(id, Edit::ApplyRedactions { pages: None }).unwrap();
    let shown = page_texts(&s, id).swap_remove(0);
    assert!(shown.starts_with("Public words then") && shown.ends_with("and more") && !shown.contains("SECRET42"), "{shown}");
    let saved = s.save_bytes(id).unwrap();
    let held = |d: &[u8]| d.windows(8).any(|w| w == b"SECRET42");
    assert!(!held(&saved), "the saved file holds the redacted word");
    let reopened = pdfcraft_cos::Document::open(saved).unwrap();
    assert!(
        !reopened.scan_objects().any(|(_, o)| matches!(&*o, pdfcraft_cos::Object::Stream(st) if st.decoded().is_ok_and(|d| held(&d)))),
        "a saved stream holds the redacted word"
    );
}

#[test]
fn redaction_marks_apply_for_good_and_undo() {
    let (mut s, id) = session_with(2);
    // "Page 1" at 24 pt from x 20: the "1" starts near x 82.7 (Helvetica widths).
    let shape =
        Shape::Redact { quads: vec![pdfcraft_annot::rect_quad([80.0, 140.0, 100.0, 180.0])], overlay: String::new(), look: Default::default() };
    let mark =
        Edit::AddAnnotation(NewAnnotation { page: 0, style: Style::default_for(&shape), shape, contents: String::new(), author: "Ada".into() });
    s.apply(id, mark).unwrap();
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add redaction mark"));
    assert_eq!(s.get(id).unwrap().redaction_marks(), 1);
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2"], "marking alone changes nothing");
    s.apply(id, Edit::ApplyRedactions { pages: None }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!((d.can_undo(), d.redaction_marks()), (Some("Apply redactions"), 0));
    // An independent extractor (the renderer's) no longer finds the "1".
    assert_eq!(page_texts(&s, id), ["Page", "Page 2"]);
    let saved = s.save_bytes(id).unwrap();
    assert!(!saved.windows(8).any(|w| w == b"(Page 1)"), "a full rewrite: the old revision is gone");
    assert!(saved.windows(8).any(|w| w == b"(Page 2)"));
    let mut s2 = Session::new();
    let id2 = s2.open("r.pdf", None, saved, None).unwrap();
    assert_eq!(page_texts(&s2, id2), ["Page", "Page 2"]);
    s.undo(id).unwrap();
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2"]);
    assert_eq!(s.apply(id, Edit::ClearRedactions).map(|_| s.get(id).unwrap().redaction_marks()), Ok(0));
    assert!(matches!(s.apply(id, Edit::ApplyRedactions { pages: None }), Err(EditError::Redact(_))));
}

/// Marks removed before they are applied, all at once or one by one, are not saved, nor are
/// replies to them: the file would otherwise record where they were, their overlay text and who
/// made them, and the page is left as it was.
#[test]
fn removed_redaction_marks_are_not_saved() {
    for clear in [true, false] {
        let (mut s, id) = session_with(1);
        mark_with_reply(&mut s, id);
        let removal = if clear { Edit::ClearRedactions } else { Edit::DeleteAnnotation { page: 0, index: 0 } };
        s.apply(id, removal).unwrap();
        assert_eq!(s.get(id).unwrap().redaction_marks(), 0);
        let saved = s.save_bytes(id).unwrap();
        for trace in MARK_TRACES {
            assert!(!saved.windows(trace.len()).any(|w| w == trace.as_bytes()), "clear: {clear}, {trace:?}");
        }
        let mut reopened = Session::new();
        let back = reopened.open("saved.pdf", None, saved.clone(), None).unwrap();
        assert_eq!(page_texts(&reopened, back), ["Page 1"]);
        let page = pdfcraft_model::pages(&reopened.get(back).unwrap().editor.as_ref().unwrap().cos).swap_remove(0);
        assert!(!page.dict.contains(b"Annots"), "clear: {clear}");

        // Undo past that save brings the mark back, under the number the saved file left free.
        s.mark_saved(id, saved, None).unwrap();
        s.undo(id).unwrap();
        let again = s.save_bytes(id).unwrap();
        let mut reopened = Session::new();
        let back = reopened.open("again.pdf", None, again.clone(), None).unwrap();
        let d = reopened.get(back).unwrap();
        assert!(d.info.warnings.is_empty() && d.editor.as_ref().unwrap().cos.repair_log().is_empty(), "{:?}", d.info.warnings);
        assert_eq!(d.redaction_marks(), 1, "clear: {clear}");
        let marks = pdfcraft_redact::marks(&d.editor.as_ref().unwrap().cos);
        assert_eq!(marks.first().map(|m| m.overlay.as_str()), Some("MARKED OVERLAY"));
    }
}

/// Removed marks are not in a signed file either, nor in one with revocation evidence embedded:
/// no later save could take them out of those.
#[test]
fn removed_redaction_marks_are_not_signed() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    // After the test certificate's start date (2026-10-02).
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
    mark_with_reply(&mut s, id);
    s.apply(id, Edit::ClearRedactions).unwrap();
    let signed = s.sign(id, &digital_id, SignOptions { rect: None, ..SignOptions::default() }).unwrap();
    let evidence = sign::dss::Evidence { certs: vec![digital_id.certificate.raw.clone()], ocsps: Vec::new(), crls: Vec::new() };
    let ltv = s.embed_ltv(id, &evidence).unwrap();
    for (what, out) in [("signed", signed), ("LTV", ltv)] {
        for trace in MARK_TRACES {
            assert!(!out.windows(trace.len()).any(|w| w == trace.as_bytes()), "{what}: {trace:?}");
        }
    }
}

#[test]
fn oversized_redaction_overlay_preserves_the_document_and_undo() {
    let (mut s, id) = session_with(1);
    let shape = Shape::Redact {
        quads: vec![pdfcraft_annot::rect_quad([0.0, 0.0, 16000.0, 1600.0])],
        overlay: "X".into(),
        look: pdfcraft_annot::OverlayLook { size: 2.0, repeat: true, ..Default::default() },
    };
    s.apply(
        id,
        Edit::AddAnnotation(NewAnnotation { page: 0, style: Style::default_for(&shape), shape, contents: String::new(), author: "Tester".into() }),
    )
    .unwrap();
    let before = s.get(id).unwrap().bytes.clone();
    assert!(matches!(s.apply(id, Edit::ApplyRedactions { pages: None }), Err(EditError::Redact(pdfcraft_redact::RedactError::OverlayLimit))));
    let doc = s.get(id).unwrap();
    assert_eq!(doc.bytes, before);
    assert_eq!(doc.redaction_marks(), 1);
    assert_eq!(doc.can_undo(), Some("Add redaction mark"));
    assert_eq!(page_texts(&s, id), ["Page 1"]);
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().redaction_marks(), 0);
    assert_eq!(page_texts(&s, id), ["Page 1"]);
}

/// #3046: a layer hidden in the Layers panel stays off on paper, as it is on screen; with the
/// layers untouched, the document's own configuration prints.
#[test]
fn printing_keeps_hidden_layers_hidden() {
    let content = b"/OC /A BDC 1 0 0 rg 10 10 80 80 re f EMC /OC /B BDC 0 1 0 rg 110 10 80 80 re f EMC";
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] /D << /Order [5 0 R 6 0 R] >> >> >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R /Resources << /Properties << /A 5 0 R /B 6 0 R >> >> >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(), content, b"\nendstream"].concat(),
        b"<< /Type /OCG /Name (Layer A) >>".to_vec(),
        b"<< /Type /OCG /Name (Layer B) >>".to_vec(),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(o);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());

    // Red and green pixels on the first printed sheet.
    let printed = |s: &Session, id: DocId| {
        let bytes = s.print_pdf(id, &print::Settings { pages: vec![0], ..Default::default() }).unwrap();
        let mut out = Session::new();
        let printed = out.open("print.pdf", None, Arc::new(bytes), None).unwrap();
        let page = render_page(&out, printed);
        let px = page.rgba.as_chunks::<4>().0;
        let red = px.iter().filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
        let green = px.iter().filter(|p| p[1] > 200 && p[0] < 60 && p[2] < 60).count();
        (red, green)
    };
    let mut s = Session::new();
    let id = s.open("layers.pdf", None, Arc::new(pdf), None).unwrap();
    let (red, green) = printed(&s, id);
    assert!(red > 100 && green > 100, "untouched layers both print: {red} red, {green} green");
    let a = s.get(id).unwrap().info.layers.iter().position(|l| l.name == "Layer A").unwrap();
    assert!(s.set_layer_visible(id, a, false));
    let (red, green) = printed(&s, id);
    assert_eq!(red, 0, "Layer A was hidden, so it doesn't print");
    assert!(green > 100, "Layer B still prints: {green} green");
    assert!(s.set_layer_visible(id, a, true));
    let (red, _) = printed(&s, id);
    assert!(red > 100, "shown again, Layer A prints again: {red} red");
}
