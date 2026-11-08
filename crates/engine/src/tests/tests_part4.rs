//! Unit tests of the engine (part 4 of 7).

#[allow(unused_imports)]
use super::*;

/// An unchanged document never reaches the writer, so the edit is what makes this the
/// place the display copy would leak if saving ever read it.
#[test]
fn saving_after_an_edit_keeps_the_baked_appearance() {
    let (mut s, vid) = trusted_layered_session();
    s.apply(vid, Edit::SetInfo { key: "Title".into(), value: "Contract".into() }).unwrap();
    let saved = s.save_bytes(vid).unwrap();
    assert!(bytes_has(&saved, b"Not Verified"), "the writer keeps the baked appearance: {saved:?}");
    assert!(!bytes_has(&saved, b"Signature valid"), "the validated state never reaches the file");
}

#[test]
fn real_layered_signatures_keep_their_original_appearance() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let id = sign::pkcs12::open(&p12, "test").unwrap();
    let anchor = id.chain.first().cloned().unwrap_or_else(|| id.certificate.clone());
    let date = "D:20261002120000+01'00'";
    let mut doc = pdfcraft_cos::Document::open(Arc::new(layered_signable_fixture())).unwrap();
    let signed = Arc::new(sign_prepared(&mut doc, &id, date));
    check_layered_round_trip(signed, None, &anchor);
}

#[test]
fn real_layered_signatures_round_trip_behind_a_user_password() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let id = sign::pkcs12::open(&p12, "test").unwrap();
    let anchor = id.chain.first().cloned().unwrap_or_else(|| id.certificate.clone());
    let date = "D:20261002120000+01'00'";
    let mut doc = pdfcraft_cos::Document::open(Arc::new(layered_signable_fixture())).unwrap();
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
    let mut doc = pdfcraft_cos::Document::open_with_password(encrypted, Some("user")).unwrap();
    let signed = Arc::new(sign_prepared(&mut doc, &id, date));
    let mut probe = Session::new().with_clock(|| 1_800_000_000);
    assert!(probe.open("enc.pdf", None, signed.clone(), Some("wrong")).is_err(), "wrong passwords refuse");
    // Encrypted appearance bytes remain unchanged through trust and save/reopen.
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let vid = s.open("enc.pdf", None, signed.clone(), Some("user")).unwrap();
    assert_eq!(s.get(vid).unwrap().signatures[0].status, SignatureStatus::Unknown);
    s.set_trusted_certificates(vec![anchor.clone()]);
    let doc = s.get(vid).unwrap();
    assert_eq!(doc.signatures[0].status, SignatureStatus::Valid);
    assert!(Arc::ptr_eq(&doc.display, &doc.bytes), "encrypted signature artwork remains original");
    let saved = s.save_bytes(vid).unwrap();
    let mut t = Session::new().with_clock(|| 1_800_000_000);
    t.set_trusted_certificates(vec![anchor.clone()]);
    assert!(t.open("enc-saved.pdf", None, saved.clone(), Some("wrong")).is_err());
    let tid = t.open("enc-saved.pdf", None, saved.clone(), Some("user")).unwrap();
    assert_eq!(t.get(tid).unwrap().signatures[0].status, SignatureStatus::Valid);
}

#[test]
#[ignore = "timing probe"]
fn probe_edit_latency_on_a_large_signed_document() {
    // ~120 MB: one page whose content stream is large.
    let big: Vec<u8> = (0..120_000_000u32).map(|i| b"0123456789 "[(i % 11) as usize]).collect();
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offs = vec![];
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", big.len() + 3).into_bytes(), b"%  ".to_vec(), big, b"\nendstream".to_vec()].concat(),
    ];
    for (i, o) in objs.iter().enumerate() {
        offs.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", i + 1).bytes());
        pdf.extend(o);
        pdf.extend(b"\nendobj\n");
    }
    let x = pdf.len();
    pdf.extend(b"xref\n0 5\n0000000000 65535 f \n");
    for o in offs {
        pdf.extend(format!("{o:010} 00000 n \n").bytes());
    }
    pdf.extend(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n").bytes());
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let t = std::time::Instant::now();
    let id = s.open("big.pdf", None, Arc::new(pdf), None).unwrap();
    eprintln!("open: {:?}", t.elapsed());
    let t = std::time::Instant::now();
    s.apply(id, rect_comment(0, [5.0, 10.0, 50.0, 50.0])).unwrap();
    eprintln!("comment edit before signing: {:?}", t.elapsed());
    s.undo(id).unwrap();
    let t = std::time::Instant::now();
    let signed = s.sign(id, &digital_id, SignOptions { rect: None, ..SignOptions::default() }).unwrap();
    eprintln!("sign: {:?}", t.elapsed());
    let t = std::time::Instant::now();
    s.mark_signed(id, signed, None).unwrap();
    eprintln!("mark_signed (reopen + validate): {:?}", t.elapsed());
    for k in 0..3 {
        let t = std::time::Instant::now();
        s.apply(id, rect_comment(0, [10.0 + k as f64, 10.0, 50.0, 50.0])).unwrap();
        eprintln!("comment edit {k}: {:?}", t.elapsed());
    }
}

#[test]
fn backgrounds_and_watermarks_from_files() {
    let (mut s, id) = session_with(1);
    // A source PDF whose second page is solid blue.
    let blue = {
        let mut src = Session::new();
        let sid = src.open("src.pdf", None, Arc::new(fixture(2)), None).unwrap();
        src.apply(
            sid,
            Edit::AddBackground {
                pages: vec![1],
                settings: Background { color: [0.0, 0.0, 1.0], ..Background::default() },
                replace: false,
                file: None,
            },
        )
        .unwrap();
        src.save_bytes(sid).unwrap()
    };
    let file = MarkFile { name: "src.pdf".into(), bytes: blue, page: 1 };
    s.apply(id, Edit::AddBackground { pages: vec![0], settings: Background::default(), replace: false, file: Some(file) }).unwrap();
    let corner = pixel(&s, id, 0, 3, 3);
    assert!(corner[2] > 200 && corner[0] < 40, "the blue page fills the background: {corner:?}");
    // The page's own text still draws on top (at "Page 1", 20,150).
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add background"));
    // A red image watermark at half the page, on top, unrotated.
    let red = MarkFile { name: "stamp.bmp".into(), bytes: Arc::new(bmp(4, 4, [0, 0, 255])), page: 0 };
    let wm = Watermark { rotation: 0.0, opacity: 1.0, scale: 0.5, ..Watermark::default() };
    s.apply(id, Edit::AddWatermark { pages: vec![0], settings: wm, replace: false, file: Some(red) }).unwrap();
    let centre = pixel(&s, id, 0, 100, 150);
    assert!(centre[0] > 200 && centre[2] < 40, "the red picture sits in the middle: {centre:?}");
    let edge = pixel(&s, id, 0, 10, 150);
    assert!(edge[2] > 200, "outside it the background shows: {edge:?}");
    // A bad file is refused clearly.
    let junk = MarkFile { name: "x.png".into(), bytes: Arc::new(b"not an image".to_vec()), page: 0 };
    assert!(s.apply(id, Edit::AddBackground { pages: vec![0], settings: Background::default(), replace: true, file: Some(junk) }).is_err());
}

/// Scan & OCR ▸ Recognize text on a page that is only a picture of text (needs the models:
/// `cargo xtask models`; skipped without them).
#[test]
fn recognize_text_reads_the_euro_sign() {
    if !ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let text = s.create_from_text("t", "Total amount due: €250 by Friday").unwrap();
    let id = s.open("text.pdf", None, text, None).unwrap();
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, 150.0).unwrap();
    let scan = s.create_from_images(&[("scan.png".into(), png)]).unwrap();
    let id = s.open("scan.pdf", None, scan, None).unwrap();
    s.recognize_text(id, &[], ocr::OcrSettings::default()).unwrap();
    let text = page_texts(&s, id)[0].clone();
    // The model was trained with € where the ocrs crate's alphabet has a second E.
    assert!(text.contains("€250"), "{text}");
}

#[test]
fn synthetic_ocr_scan_keeps_the_original_page_size_without_models() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let (id, expected) = synthetic_ocr_scan(&mut s);
    let page = &s.get(id).unwrap().info.pages[0];
    // Rounding to whole raster pixels may change a page edge by at most one pixel.
    let tolerance = 72.0 / SYNTHETIC_SCAN_DPI;
    for (actual, expected) in [page.width, page.height].into_iter().zip(expected) {
        assert!(f64::from((actual - expected).abs()) <= tolerance, "page edge {actual} pt, expected {expected} pt");
    }
    assert_eq!(page_texts(&s, id), [""], "the scan has no text until OCR runs");
}

#[test]
fn recognize_text_makes_a_scanned_page_searchable() {
    if !ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let (id, _) = synthetic_ocr_scan(&mut s);
    assert_eq!(page_texts(&s, id), [""], "a picture has no text");

    let found = s.recognize_text(id, &[], ocr::OcrSettings::default()).unwrap();
    assert_eq!(found.len(), 1);
    let text = page_texts(&s, id)[0].to_lowercase();
    for w in ["quick", "brown", "fox", "lazy"] {
        assert!(text.contains(w), "{text}");
    }
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Recognize text"));
    // The text sits over the words: "quick" is left of "lazy" and on the same line.
    let words = &found[0].words;
    let q = words.iter().find(|w| w.text.to_lowercase().contains("quick")).unwrap();
    let l = words.iter().find(|w| w.text.to_lowercase().contains("lazy")).unwrap();
    let top = |w: &ocr::PlacedWord| w.origin[1] + w.up[1];
    // OCR boxes can differ by a few pixels at 300 DPI and still sit on the same line.
    let line_tolerance = q.up[1].abs().max(l.up[1].abs()) / 4.0;
    assert!(q.origin[0] < l.origin[0] && (top(q) - top(l)).abs() < line_tolerance, "{q:?} {l:?}");

    // A second pass skips the page: it has text now.
    let again = s.recognize_text(id, &[], ocr::OcrSettings::default()).unwrap();
    assert!(again[0].skipped.is_some());
}

/// #1028: a page with its own text and a picture of text. By default it is skipped, as before;
/// with `skip_text_pages: false` the picture is read and the page's own text is not added again.
#[test]
fn recognize_text_reads_a_picture_on_a_page_that_has_text() {
    if !ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let text = s.create_from_text("t", "The quick brown fox jumps over the lazy dog.").unwrap();
    let id = s.open("text.pdf", None, text, None).unwrap();
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, 150.0).unwrap();
    let scan = s.create_from_images(&[("scan.png".into(), png)]).unwrap();
    let id = s.open("scan.pdf", None, scan, None).unwrap();
    // Real text in the empty lower part of the picture.
    let (w, h) = {
        let p = &s.get(id).unwrap().info.pages[0];
        (p.width as f64, p.height as f64)
    };
    let note = AddedText { rect: [w * 0.1, h * 0.75, w * 0.9, h * 0.85], text: "Invoice reference 4711".into(), size: 18.0, ..Default::default() };
    s.apply(id, Edit::AddText { page: 0, text: note }).unwrap();
    let count = |s: &Session, word: &str| page_texts(s, id)[0].to_lowercase().matches(word).count();
    assert_eq!((count(&s, "invoice"), count(&s, "brown")), (1, 0));

    let skipped = s.recognize_text(id, &[], ocr::OcrSettings::default()).unwrap();
    assert!(skipped[0].skipped.is_some(), "pages with text are still skipped by default");
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add text"), "nothing was added");

    let hybrid = ocr::OcrSettings { skip_text_pages: false, ..Default::default() };
    let found = s.recognize_text(id, &[], hybrid.clone()).unwrap();
    assert_eq!(found[0].skipped, None);
    assert!(found[0].already_on_page >= 2, "the page's own words were recognised and left out: {:?}", found[0]);
    let added = found[0].text().to_lowercase();
    assert!(added.contains("brown") && added.contains("lazy") && !added.contains("invoice"), "{added}");
    assert_eq!((count(&s, "invoice"), count(&s, "brown")), (1, 1), "{:?}", page_texts(&s, id));
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Recognize text"));

    // Reading the page again adds nothing: every word is there now.
    let again = s.recognize_text(id, &[], hybrid).unwrap();
    assert!(again[0].words.is_empty(), "{:?}", again[0]);
    assert_eq!((count(&s, "invoice"), count(&s, "brown")), (1, 1));
}

/// Edit reads a scan that only has comments on it, not a page with real text and a picture
/// (`native_text_only`): a text box over the picture is not the page's text, page content is.
#[test]
fn native_text_only_ignores_comments_but_not_page_text() {
    if !ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let text = s.create_from_text("t", "The quick brown fox jumps over the lazy dog.").unwrap();
    let id = s.open("text.pdf", None, text, None).unwrap();
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, 150.0).unwrap();
    let scan = s.create_from_images(&[("scan.png".into(), png)]).unwrap();
    let comment_only = |s: &mut Session| {
        let id = s.open("scan.pdf", None, scan.clone(), None).unwrap();
        let (w, h) = {
            let p = &s.get(id).unwrap().info.pages[0];
            (p.width as f64, p.height as f64)
        };
        let shape = pdfcraft_annot::Shape::TextBox { rect: [w * 0.1, h * 0.75, w * 0.9, h * 0.85], font_size: 18.0 };
        s.apply(
            id,
            Edit::AddAnnotation(NewAnnotation {
                page: 0,
                style: pdfcraft_annot::Style::default_for(&shape),
                shape,
                contents: "Invoice reference 4711".into(),
                author: "Test".into(),
            }),
        )
        .unwrap();
        id
    };
    let native = ocr::OcrSettings { native_text_only: true, ..Default::default() };

    // A comment over the picture: the default run counts it as text and skips the page, the
    // native-text run reads the picture.
    let id = comment_only(&mut s);
    let skipped = s.recognize_text(id, &[], ocr::OcrSettings::default()).unwrap();
    assert!(skipped[0].skipped.is_some(), "a comment is text for the default run");
    let found = s.recognize_text(id, &[], native.clone()).unwrap();
    assert_eq!(found[0].skipped, None, "{:?}", found[0]);
    assert!(found[0].text().to_lowercase().contains("brown"), "{:?}", found[0]);

    // Page content over the picture is the page's own text: skipped, whatever the setting.
    let id = comment_only(&mut s);
    let (w, h) = {
        let p = &s.get(id).unwrap().info.pages[0];
        (p.width as f64, p.height as f64)
    };
    let note = AddedText { rect: [w * 0.1, h * 0.6, w * 0.9, h * 0.7], text: "Page text".into(), size: 18.0, ..Default::default() };
    s.apply(id, Edit::AddText { page: 0, text: note }).unwrap();
    let skipped = s.recognize_text(id, &[], native).unwrap();
    assert!(skipped[0].skipped.is_some(), "{:?}", skipped[0]);
}

/// AcroForm field scripts validate, calculate and format through edits, and a button script's
/// changes, messages and requests come back.
#[test]
fn form_javascript_validates_calculates_and_formats() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("order.pdf", None, Arc::new(scripted_form()), None).unwrap();
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).unwrap().value.clone();
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("4".into()) }).unwrap();
    assert_eq!(value(&s, "total"), ["10"], "calculated by the script");
    assert!(page_texts(&s, id)[0].contains("USD 10.00"), "{:?}", page_texts(&s, id));
    let out = s.take_js_output(id);
    assert!(out.console.contains(&"times".to_string()), "{out:?}");

    let err = s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("500".into()) }).unwrap_err();
    assert_eq!(err.to_string(), "At most 100");
    assert_eq!(value(&s, "qty"), ["4"]);

    // A button script changes fields in one undoable step, and asks for a page.
    let o =
        s.run_javascript(id, "getField('price').value = 3; getField('total').readonly = true; app.alert('Done'); this.print();", Some("go")).unwrap();
    assert_eq!(o.alerts, [crate::js::Message::plain("Done")]);
    assert_eq!(o.requests, [js::Request::Print]);
    assert_eq!(value(&s, "total"), ["12"], "recalculated after the change");
    assert!(s.get(id).unwrap().form.iter().find(|f| f.name == "total").unwrap().read_only());
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Run JavaScript"));

    // JavaScript off: scripts don't run.
    s.set_javascript(false);
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("500".into()) }).unwrap();
    assert_eq!(value(&s, "total"), ["12"]);
    assert!(s.run_javascript(id, "1", None).is_err());
}

/// XFA forms check `xfa_installed` and `xfa_version` in a document script that runs before
/// each of their scripts, and alert when either is missing.
#[test]
fn scripts_in_an_xfa_form_see_the_xfa_plugin_installed() {
    let check = "typeof xfa_installed + ' ' + typeof xfa_version + ' ' + xfa_version";
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let xfa = s.open("form.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template(3))), None).unwrap();
    let o = s.run_javascript(xfa, check, None).unwrap();
    assert_eq!((o.error, o.result.as_deref()), (None, Some("boolean number 3.5")));
    let plain = s.open("order.pdf", None, Arc::new(scripted_form()), None).unwrap();
    let o = s.run_javascript(plain, check, None).unwrap();
    assert_eq!((o.error, o.result.as_deref()), (None, Some("undefined number 3.5")));
}

/// #3940: a document-level script that defines the helper and then calls a method PdfCraft lacks
/// (an Acrobat form copying a button icon at open) left every calculated field blank. The
/// helper stays defined, the form calculates, and the error is reported once.
#[test]
fn a_document_script_that_throws_still_lets_the_form_calculate() {
    let helpers =
        "function times\\(a, b\\) { return a * b; } this.getField\\('go'\\).buttonSetIcon\\(this.getField\\('go'\\).buttonGetIcon\\(\\)\\);";
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("order.pdf", None, Arc::new(scripted_form_with(helpers)), None).unwrap();
    let _ = s.take_js_output(id);
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("4".into()) }).unwrap();
    let total = s.get(id).unwrap().form.iter().find(|f| f.name == "total").unwrap().value.clone();
    assert_eq!(total, ["10"], "calculated through the helper the failing script defined");
    let out = s.take_js_output(id);
    assert_eq!(out.errors.len(), 1, "reported once, not once per script: {out:?}");
    assert!(out.errors[0].starts_with("document script 1: TypeError"), "{out:?}");
    // The console runs too, and says which document script failed.
    let o = s.run_javascript(id, "times(2, 3)", None).unwrap();
    assert_eq!((o.error, o.result.as_deref(), o.doc_errors.len()), (None, Some("6"), 1));
}

#[test]
fn active_xfa_does_not_run_pdf_container_scripts_but_keeps_requested_alerts() {
    let template = pdfcraft_xfa::fixtures::scripted_template().replace(
        r#"xfa.host.messageBox("Hello " + qty.rawValue);"#,
        r#"xfa.host.messageBox("Hello " + qty.rawValue); xfa.host.print(1, "0", "0", 0, 0, 0, 0, 0);"#,
    );
    let mut cos = pdfcraft_cos::Document::open(Arc::new(pdfcraft_xfa::fixtures::shell(&template))).unwrap();
    let fallback = "app.alert('This PDF form requires a newer version of Adobe Acrobat');";
    pdfcraft_forms::set_document_script(&mut cos, "viewerFallback", Some(fallback)).unwrap();
    let bytes = Arc::new(pdfcraft_cos::write_full(&cos, &Default::default()).unwrap());
    let mut s = Session::new();
    let id = s.open("xfa.pdf", None, bytes, None).unwrap();
    assert!(s.get(id).unwrap().xfa.is_some());
    assert!(s.take_js_output(id).alerts.is_empty());
    let out = s.run_javascript(id, "app.alert('Console message');", None).unwrap();
    assert!(out.error.is_none(), "{out:?}");
    assert_eq!(out.alerts, [js::Message::plain("Console message")]);
    assert_eq!(s.get(id).unwrap().document_scripts(), [("viewerFallback".into(), fallback.into())]);
    let out = s.run_javascript(id, "", Some("hello")).unwrap();
    assert!(out.error.is_none(), "{out:?}");
    assert_eq!(out.alerts, [js::Message::plain("Hello 2")]);
    assert_eq!(out.requests, [js::Request::Print]);
    // The same context rule applies to format/validate/calculate through JsRunner.
    s.apply(id, Edit::SetFieldScript { name: "qty".into(), event: "validate".into(), script: Some("app.alert('Field message');".into()) }).unwrap();
    s.take_js_output(id);
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("3".into()) }).unwrap();
    assert_eq!(s.take_js_output(id).alerts, [js::Message::plain("Field message")]);
    // Saved XFA keeps its container scripts for inspection, but does not execute them.
    let saved = s.get(id).unwrap().bytes.clone();
    let reopened = s.open("saved.pdf", None, saved, None).unwrap();
    assert!(s.run_javascript(reopened, "", None).unwrap().alerts.is_empty());
    // Making the form static restores the ordinary PDF context; undo restores XFA's.
    s.apply(id, Edit::XfaToStatic).unwrap();
    assert_eq!(s.run_javascript(id, "", None).unwrap().alerts.len(), 1);
    s.undo(id).unwrap();
    assert!(s.run_javascript(id, "", None).unwrap().alerts.is_empty());
}

#[test]
fn active_xfa_without_template_scripts_does_not_initialize_pdf_fallback_scripts() {
    let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
<subform name="form" layout="tb"><pageSet><pageArea name="page">
<contentArea x="36pt" y="36pt" w="540pt" h="720pt"/><medium short="612pt" long="792pt"/>
</pageArea></pageSet><field name="text" w="200pt" h="30pt"><ui><textEdit/></ui></field></subform></template>"#;
    let mut s = Session::new();
    let id = s.open("no-scripts.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(template)), None).unwrap();
    assert!(s.get(id).unwrap().xfa.is_some());
    assert!(s.get(id).unwrap().xfa_template.is_none());
    s.apply(id, Edit::SetDocumentScript { name: "fallback".into(), script: Some("throw new Error('PDF fallback ran');".into()) }).unwrap();
    let out = s.run_javascript(id, "app.alert('Console message');", None).unwrap();
    assert!(out.error.is_none(), "{out:?}");
    assert_eq!(out.alerts, [js::Message::plain("Console message")]);
}

#[test]
fn inactive_xfa_and_ordinary_pdfs_keep_document_helpers_and_alerts() {
    for bytes in [
        fixture(1),
        pdfcraft_xfa::fixtures::static_shell("<form1><page1><name>Test</name></page1></form1>"),
        pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template()),
    ] {
        let mut cos = pdfcraft_cos::Document::open(Arc::new(bytes)).unwrap();
        pdfcraft_forms::set_document_script(&mut cos, "helper", Some("function twice(n) { return n * 2; } app.alert('PDF message');")).unwrap();
        // The raw dynamic shell has not been laid out: XFA's presence alone is not enough.
        let runner = js::JsRunner::new(&cos, "control.pdf");
        let out = pdfcraft_js::run("twice(3)", &pdfcraft_js::Event::doc("Console"), &runner.doc, &[], &runner.doc_scripts, Default::default());
        assert!(out.error.is_none(), "{out:?}");
        assert_eq!(out.result.as_deref(), Some("6"));
        assert_eq!(out.alerts, [js::Message::plain("PDF message")]);
    }
}

#[test]
fn edit_history_does_not_replay_field_script_side_effects() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("order.pdf", None, Arc::new(scripted_form()), None).unwrap();
    let value = |s: &Session, name: &str| s.get(id).unwrap().form.iter().find(|f| f.name == name).unwrap().value.clone();
    let initial_qty = value(&s, "qty");
    let initial_total = value(&s, "total");
    assert!(s.take_js_output(id).is_empty());

    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("4".into()) }).unwrap();
    assert_eq!(value(&s, "qty"), ["4"]);
    assert_eq!(value(&s, "total"), ["10"]);
    let output = s.take_js_output(id);
    assert!(output.console.contains(&"times".to_string()), "{output:?}");
    assert_eq!(output.console.iter().filter(|line| *line == "times").count(), 1, "{output:?}");

    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Report".into() }).unwrap();
    assert!(s.take_js_output(id).is_empty(), "a metadata edit does not rerun field scripts");

    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().info.title, None);
    assert_eq!(value(&s, "qty"), ["4"]);
    assert!(s.take_js_output(id).is_empty(), "undoing metadata does not rerun field scripts");

    s.redo(id).unwrap();
    assert_eq!(s.get(id).unwrap().info.title.as_deref(), Some("Report"));
    assert_eq!(value(&s, "qty"), ["4"]);
    assert!(s.take_js_output(id).is_empty(), "redoing metadata does not rerun field scripts");

    s.undo(id).unwrap();
    s.undo(id).unwrap();
    assert_eq!(value(&s, "qty"), initial_qty);
    assert_eq!(value(&s, "total"), initial_total);
    assert!(s.take_js_output(id).is_empty(), "undoing the scripted edit does not replay its side effect");

    s.redo(id).unwrap();
    assert_eq!(value(&s, "qty"), ["4"]);
    assert_eq!(value(&s, "total"), ["10"]);
    assert!(s.take_js_output(id).is_empty(), "redoing the scripted edit restores its snapshot without replay");
}

#[test]
fn detecting_fields_on_a_printed_form() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let text = s.create_from_text("t", "Name: ______________________\n\nEmail address: ____________________\n\nPlain text without blanks.").unwrap();
    let id = s.open("paper.pdf", None, text, None).unwrap();
    let found = s.detect_fields(id, &[]);
    let names: Vec<&str> = found.iter().map(|(_, c)| c.name.as_str()).collect();
    assert_eq!(names, ["Name", "Email address"], "{found:?}");
    let added = s.auto_detect_fields(id, &[]).unwrap();
    assert_eq!(added, ["Name", "Email address"]);
    let d = s.get(id).unwrap();
    assert_eq!(d.form.len(), 2);
    assert_eq!(d.can_undo(), Some("Detect form fields"));
    // The field sits on the blank: right of the label, at its height.
    let f = &d.form[0];
    assert!(f.widgets[0].rect[0] > 72.0 + 20.0, "{:?}", f.widgets[0].rect);
    assert!(s.detect_fields(id, &[]).is_empty(), "nothing left to detect");
}

/// #294, #2831: a detected field sits on a line or box the page already draws, so it gets no border
/// and no fill that would cover the printed form. A field drawn by hand keeps the defaults.
#[test]
fn detected_fields_have_no_border_or_fill() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("paper.pdf", None, Arc::new(paper_form()), None).unwrap();
    let added = s.auto_detect_fields(id, &[]).unwrap();
    let d = s.get(id).unwrap();
    let kinds: Vec<pdfcraft_forms::FieldKind> = d.form.iter().map(|f| f.kind).collect();
    assert!(kinds.contains(&pdfcraft_forms::FieldKind::Text) && kinds.contains(&pdfcraft_forms::FieldKind::CheckBox), "{kinds:?}");
    for name in &added {
        let look = d.field_look(name).unwrap();
        assert_eq!((look.border, look.fill), (None, None), "{name}");
    }
    assert_eq!(d.can_undo(), Some("Detect form fields"), "still one step");

    s.apply(id, Edit::AddField { page: 0, rect: [200.0, 10.0, 300.0, 30.0], kind: NewField::Text { multiline: false }, name: Some("drawn".into()) })
        .unwrap();
    let look = s.get(id).unwrap().field_look("drawn").unwrap();
    assert!(look.border.is_some() && look.fill.is_some(), "{look:?}");
}

/// Fields on a scanned paper form are found from its lines and boxes, wherever the page turns
/// them, and named from the labels when text recognition is installed.
#[test]
fn detecting_fields_on_a_scanned_form() {
    use pdfcraft_forms::detect::Kind::{CheckBox, Text};
    for scan in [Scan::Straight, Scan::Sideways, Scan::Noisy, Scan::Skewed] {
        let mut s = Session::new().with_clock(|| 1_700_000_000);
        let id = s.open("scan.pdf", None, scanned_form(scan), None).unwrap();
        assert_eq!(page_texts(&s, id), [""], "{scan:?}: a picture has no text");
        let info = s.get(id).unwrap().info.pages[0].clone();
        assert_eq!((info.width.round(), info.height.round()), (420.0, 300.0), "{scan:?}");
        // Where a field shows on the page, in points from the bottom-left as displayed.
        let shown = |r: [f64; 4]| {
            let a = info.user_to_view(r[0] as f32, r[1] as f32);
            let b = info.user_to_view(r[2] as f32, r[3] as f32);
            [a[0].min(b[0]), 300.0 - a[1].max(b[1]), a[0].max(b[0]), 300.0 - a[1].min(b[1])].map(f64::from)
        };
        let found = s.detect_fields(id, &[]);
        let got: Vec<(pdfcraft_forms::detect::Kind, [f64; 4])> = found.iter().map(|(_, c)| (c.kind.clone(), shown(c.rect))).collect();
        // In reading order: the rule after "Name:", the box below "Anschrift", the "Telefon"
        // box, two check boxes.
        let want = [
            (Text, [80.0, 229.0, 380.0, 245.0]),
            (Text, [31.0, 151.0, 379.0, 189.0]),
            (Text, [31.0, 91.0, 379.0, 119.0]),
            (CheckBox, [30.0, 50.0, 40.0, 60.0]),
            (CheckBox, [150.0, 50.0, 160.0, 60.0]),
        ];
        assert_eq!(got.len(), want.len(), "{scan:?}: {got:?}");
        for ((kind, r), (want_kind, w)) in got.iter().zip(&want) {
            assert_eq!(kind, want_kind, "{scan:?}: {got:?}");
            // The labelled box's top is wherever the label ends; everything else is close. A
            // degree askew, a 350 pt line climbs 6 pt, and the field stays inside the box.
            let tol = if matches!(scan, Scan::Skewed) { [6.5; 4] } else { [2.5, 2.5, 2.5, if w[3] == 189.0 { 6.0 } else { 2.5 }] };
            assert!(r.iter().zip(w).zip(tol).all(|((a, b), t)| (a - b).abs() <= t), "{scan:?}: {r:?}, want {w:?}");
        }
        let names: Vec<&str> = found.iter().map(|(_, c)| c.name.as_str()).collect();
        if ocr::available() {
            // (Askew, recognition may read "Haupt" and "Zweig" as one phrase.)
            assert!(names.contains(&"Name") && names.contains(&"Telefon") && names.iter().any(|n| n.starts_with("Haupt")), "{scan:?}: {names:?}");
        } else {
            // Without text recognition the fields get the default names.
            assert!(names.iter().all(|n| n.starts_with("Text") || n.starts_with("Check Box")), "{scan:?}: {names:?}");
        }

        // Added as one step, where they were found; on the turned page each field turns with
        // it, so typing reads upright.
        let added = s.auto_detect_fields(id, &[]).unwrap();
        assert_eq!(added.len(), want.len());
        let d = s.get(id).unwrap();
        assert_eq!(d.can_undo(), Some("Detect form fields"));
        let turn = if matches!(scan, Scan::Sideways) { 90 } else { 0 };
        for (_, c) in &found {
            let f = d.form.iter().find(|f| f.name == c.name).unwrap();
            assert_eq!(f.widgets[0].rotation, turn, "{scan:?}: {}", f.name);
            let r = f.widgets[0].rect;
            assert!(r.iter().zip(c.rect).all(|(a, b)| (a - b).abs() < 0.01), "{scan:?}: {r:?} moved from {:?}", c.rect);
        }
        assert!(s.detect_fields(id, &[]).is_empty(), "{scan:?}: nothing left to detect");
    }
}

#[test]
fn detecting_comb_boxes_adds_one_comb_field_per_row() {
    // The rows from #1887: five 18 x 22 cells and ten 16 x 20 cells, each a stroked `re`.
    let mut body = String::from("BT /F1 10 Tf 40 205 Td (Postcode A) Tj ET BT /F1 10 Tf 40 105 Td (Postcode B) Tj ET\n");
    for i in 0..5 {
        body.push_str(&format!("{} 200 18 22 re S\n", 120 + i * 18));
    }
    for i in 0..10 {
        body.push_str(&format!("{} 100 16 20 re S\n", 120 + i * 16));
    }
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("comb.pdf", None, Arc::new(one_line_page("/Font << /F1 4 0 R >>", &body)), None).unwrap();
    let found: Vec<(String, pdfcraft_forms::detect::Kind)> = s.detect_fields(id, &[]).into_iter().map(|(_, c)| (c.name, c.kind)).collect();
    assert_eq!(
        found,
        [
            ("Postcode A".to_string(), pdfcraft_forms::detect::Kind::Comb { cells: 5 }),
            ("Postcode B".to_string(), pdfcraft_forms::detect::Kind::Comb { cells: 10 })
        ]
    );
    s.auto_detect_fields(id, &[]).unwrap();
    let d = s.get(id).unwrap();
    let combs: Vec<(&str, FormFieldKind, bool, Option<usize>)> =
        d.form.iter().map(|f| (f.name.as_str(), f.kind, f.has(pdfcraft_forms::flags::COMB), f.max_len)).collect();
    assert_eq!(combs, [("Postcode A", FormFieldKind::Text, true, Some(5)), ("Postcode B", FormFieldKind::Text, true, Some(10))]);
    assert_eq!(d.can_undo(), Some("Detect form fields"));
}

#[test]
fn comparing_two_versions_of_a_document() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let v1 = s.create_from_text("t", "The contract starts on Monday.\n\nPayment is due within thirty days.\n\nSigned by both parties.").unwrap();
    let v2 = s
        .create_from_text("t", "The contract starts on Tuesday.\n\nPayment is due within thirty days of invoice.\n\nSigned by both parties.")
        .unwrap();
    let old = s.open("v1.pdf", None, v1, None).unwrap();
    let new = s.open("v2.pdf", None, v2, None).unwrap();
    let c = s.compare(old, new).unwrap();
    let got: Vec<(compare::Kind, &str, &str)> = c.changes.iter().map(|x| (x.kind, x.old.text.as_str(), x.new.text.as_str())).collect();
    assert_eq!(got, [(compare::Kind::Replaced, "Monday.", "Tuesday."), (compare::Kind::Replaced, "days.", "days of invoice.")], "{got:?}");
    assert!(s.compare(old, old).unwrap().identical());
    // limited to pages of the new file: both changes are on the first
    assert_eq!(s.compare_pages(old, new, &[]).unwrap().changes.len(), 2);
    assert_eq!(s.compare_pages(old, new, &[0]).unwrap().changes.len(), 2);
    assert!(s.compare_pages(old, new, &[1]).unwrap().identical());

    let report = s.compare_report(old, new).unwrap();
    let rid = s.open("report.pdf", None, report, None).unwrap();
    let text = page_texts(&s, rid).join(" ");
    assert!(text.contains("2 changes: 2 replaced") && text.contains("Tuesday."), "{text}");

    // Visually, only the changed words' places differ (and the page is otherwise the same).
    let vis = s.compare_visual(old, new, 72.0).unwrap();
    assert!(!vis.is_empty() && vis.iter().all(|(p, r)| *p == 0 && r[3] > 600.0), "{vis:?}");
    assert!(s.compare_visual(old, old, 72.0).unwrap().is_empty());

    assert_eq!(s.mark_differences(old, new).unwrap(), 2);
    let d = s.get(new).unwrap();
    assert_eq!(d.can_undo(), Some("Mark differences"));
    let marks: Vec<_> = d.info.annotations.iter().filter(|a| a.author.as_deref() == Some("Compare")).collect();
    assert_eq!(marks.len(), 2);
    assert!(marks[0].contents.as_deref().unwrap().starts_with("Replaced: \"Monday.\""));
}

#[test]
fn actions_run_their_steps_on_files() {
    let s = Session::new();
    let src = s.create_from_text("t", "Quarterly numbers").unwrap();
    let built = actions::builtin();
    let numbers = built.iter().find(|a| a.name == "Add Page Numbers").unwrap();
    let r = actions::run_on(numbers, "q.pdf", src.clone(), |_, _| {}).unwrap();
    let mut s2 = Session::new();
    let id = s2.open("out.pdf", None, r.bytes, None).unwrap();
    assert!(page_texts(&s2, id)[0].contains("Page 1 of 1"), "{:?}", page_texts(&s2, id));
    assert_eq!(r.log, ["Add footer"]);

    let custom = actions::Action {
        name: "Mine".into(),
        description: String::new(),
        steps: vec![
            actions::Step::from_id("set_title", "Q3").unwrap(),
            actions::Step::AddWatermark("DRAFT".into()),
            actions::Step::ReduceFileSize,
            actions::Step::FlattenComments,
        ],
        builtin: false,
    };
    let mut steps = Vec::new();
    let r = actions::run_on(&custom, "q.pdf", src, |i, n| steps.push((i, n))).unwrap();
    assert_eq!(steps, [(0, 4), (1, 4), (2, 4), (3, 4)]);
    assert_eq!(r.log.len(), 4, "{:?}", r.log);
    let id = s2.open("out2.pdf", None, r.bytes, None).unwrap();
    let d = s2.get(id).unwrap();
    assert_eq!(d.info.title.as_deref(), Some("Q3"));
    assert!(page_texts(&s2, id)[0].contains("DRAFT"));
    assert!(actions::run_on(&custom, "bad.pdf", Arc::new(b"nope".to_vec()), |_, _| {}).is_err());
}

/// A removal step that fails (here: a signed file can't be rewritten without its hidden data)
/// fails the action, so the file is not passed on as cleaned.
#[test]
fn actions_fail_when_hidden_information_could_not_be_removed() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(2)), None).unwrap();
    s.apply(id, Edit::SetInfo { key: "Author".into(), value: "Private Name".into() }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    let signed = s.sign(id, &digital_id, SignOptions { page: 1, ..SignOptions::default() }).unwrap();
    let built = actions::builtin();
    let sanitize = built.iter().find(|a| a.name == "Sanitize Documents").unwrap();
    let e = actions::run_on(sanitize, "signed.pdf", signed, |_, _| {}).map(|r| r.log).unwrap_err();
    assert!(e.starts_with("Sanitize document:"), "{e}");
}

#[test]
fn exporting_office_files_keeps_images() {
    let mut s = Session::new();
    let text = s.create_from_text("t", "Picture page").unwrap();
    let id = s.open("t.pdf", None, text, None).unwrap();
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, 20.0).unwrap();
    let pic = s.create_from_images(&[("p.png".into(), png)]).unwrap();
    let pid = s.open("p.pdf", None, pic, None).unwrap();
    let d = s.get(pid).unwrap();
    assert_eq!(d.export_pages()[0].images.len(), 1);
    let docx = d.export_office(compare::OfficeFormat::Docx);
    assert!(docx.windows(16).any(|w| w == b"word/media/image"));
    let pptx = d.export_office(compare::OfficeFormat::Pptx);
    assert!(pptx.windows(15).any(|w| w == b"ppt/media/image"));
    if let Ok(dir) = std::env::var("PDFCRAFT_EXPORT_DIR") {
        std::fs::write(format!("{dir}/pic.docx"), &docx).unwrap();
    }
    assert!(String::from_utf8(d.export_office(compare::OfficeFormat::Html)).unwrap().contains("data:image/png;base64,"));
}

/// #314: a page whose content is drawn through a form XObject exports its text (the file from
/// the report: no xref table, wrong stream lengths).
#[test]
fn exporting_office_files_reads_form_xobjects() {
    let pdf = b"%PDF-1.4
1 0 obj <</Type/Catalog/Pages 2 0 R>> endobj
2 0 obj <</Type/Pages/Kids[3 0 R]/Count 1>> endobj
3 0 obj <</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]/Resources<</XObject<</Fm1 7 0 R>>>>/Contents 5 0 R>> endobj
4 0 obj <</Type/Font/Subtype/Type1/BaseFont/Helvetica/Encoding/WinAnsiEncoding>> endobj
5 0 obj <</Length 22>> stream
q 1 0 0 1 0 0 cm /Fm1 Do Q
endstream endobj
7 0 obj <</Type/XObject/Subtype/Form/BBox[0 0 595 842]/Resources<</Font<</F1 4 0 R>>>>/Length 60>> stream
BT /F1 18 Tf 72 760 Td (Hello from a test invoice) Tj ET
endstream endobj
trailer <</Root 1 0 R>>
%%EOF
";
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(pdf.to_vec()), None).unwrap();
    let d = s.get(id).unwrap();
    let blocks: Vec<String> = d.export_pages()[0].blocks.iter().map(|b| b.text.clone()).collect();
    assert_eq!(blocks, ["Hello from a test invoice"]);
    assert!(String::from_utf8(d.export_office(compare::OfficeFormat::Html)).unwrap().contains("Hello from a test invoice"));
    // Edit text still offers only what the page's own streams show.
    assert!(d.text_blocks(0).is_empty());
}

#[test]
fn exporting_office_files_keeps_text_colour() {
    // #526: a dark green heading came out black in Word.
    let content = "BT /F1 24 Tf 0.05 0.23 0.18 rg 72 700 Td (Annual Report) Tj ET \
BT /F1 11 Tf 0 g 72 650 Td (Black body text that is long enough to be the body size.) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new();
    let id = s.open("c.pdf", None, Arc::new(pdf), None).unwrap();
    let d = s.get(id).unwrap();
    let html = String::from_utf8(d.export_office(compare::OfficeFormat::Html)).unwrap();
    assert!(html.contains("style=\"color:#0D3B2E\">Annual Report"), "{html}");
    let pptx = d.export_office(compare::OfficeFormat::Pptx);
    assert!(pptx.starts_with(b"PK"));
    assert!(pptx.windows(b"ppt/slides/slide1.xml".len()).any(|w| w == b"ppt/slides/slide1.xml"));
    let blocks = &d.export_pages()[0].blocks;
    assert!(blocks.iter().any(|b| b.text == "Annual Report" && b.color == [0.05, 0.23, 0.18]), "{blocks:?}");
}

#[test]
fn office_export_positions_follow_the_displayed_crop_rotation_and_units() {
    use pdfcraft_cos::{Document as CosDocument, ObjRef, Object, SaveOptions, write_full};
    let mut pdf = CosDocument::open(Arc::new(fixture(1))).unwrap();
    let mut page = pdf.get(ObjRef::new(4, 0)).as_dict().unwrap().clone();
    page.set("CropBox", Object::Array([10, 20, 190, 280].map(Object::Int).to_vec()));
    page.set("Rotate", Object::Int(90));
    page.set("UserUnit", Object::Int(2));
    pdf.set(ObjRef::new(4, 0), page);
    let mut session = Session::new();
    let id = session.open("turned.pdf", None, Arc::new(write_full(&pdf, &SaveOptions::default()).unwrap()), None).unwrap();
    let doc = session.get(id).unwrap();
    let pages = doc.export_pages();
    let b = &pages[0].blocks[0];
    assert!((b.rotation + 90.0).abs() < 0.01, "clockwise page rotation: {b:?}");
    assert!((b.rect[0] - 248.0).abs() < 0.01, "crop and UserUnit applied: {b:?}");
    assert_eq!(b.size, 48.0);
    assert_eq!((pages[0].width, pages[0].height), (520.0, 360.0));
}

#[test]
fn guard_turns_a_panic_into_an_error() {
    assert_eq!(guard(|| 7), Ok(7));
    assert_eq!(guard(|| -> u8 { panic!("boom") }), Err("boom".to_string()));
    let n = 3;
    assert_eq!(guard(|| -> u8 { panic!("page {n} is bad") }), Err("page 3 is bad".to_string()));
}

#[test]
fn measurement_edits_are_transactional_and_honour_annotation_permissions() {
    let mut session = Session::default();
    let id = session.open("plan.pdf", None, Arc::new(fixture(1)), None).unwrap();
    let m = measure::NewMeasurement {
        page: 0,
        kind: measure::Kind::Distance,
        points: vec![[10.0, 20.0], [40.0, 60.0]],
        scale: measure::Scale::new(0.1, "m", 2).unwrap(),
        style: Style::default(),
        label: "Wall".into(),
        author: "Tester".into(),
    };
    session.apply(id, Edit::AddMeasurement(m.clone())).unwrap();
    let before = session.get(id).unwrap().bytes.clone();
    assert_eq!(session.get(id).unwrap().measurements().unwrap().measurements[0].reading.label, "5.00 m");
    let mut invalid = m.clone();
    invalid.points[1][0] = f64::INFINITY;
    assert!(session.apply(id, Edit::AddMeasurement(invalid)).is_err());
    assert_eq!(session.get(id).unwrap().bytes, before);
    session.undo(id).unwrap();
    assert!(session.get(id).unwrap().measurements().unwrap().measurements.is_empty());
    session.redo(id).unwrap();
    assert_eq!(session.get(id).unwrap().bytes, before);
    let p = pdfcraft_cos::Permissions { bits: 0, owner: false, revision: 3 };
    assert_eq!(check_permission(&Edit::AddMeasurement(m), &p), Err(EditError::NotPermitted("comments")));
    assert_eq!(
        check_permission(
            &Edit::SetMeasurementScale { page: 0, bbox: [0.0, 0.0, 100.0, 100.0], name: "Detail".into(), scale: measure::Scale::default() },
            &p
        ),
        Err(EditError::NotPermitted("comments"))
    );
}

/// A laid-out XFA field that doesn't scroll (`hScrollPolicy="off"`) opens as a text field with
/// DoNotScroll, and its text box says what fits it: what the editor takes no more than.
#[test]
fn an_xfa_field_that_doesnt_scroll_opens_with_do_not_scroll_and_a_text_box() {
    let xdp = r##"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="form" layout="tb">
<field name="code" w="1in" h="0.3in"><ui><textEdit hScrollPolicy="off"/></ui><font typeface="Arial" size="10pt"/></field>
<field name="note" w="1in" h="0.3in"><ui><textEdit/></ui><font typeface="Arial" size="10pt"/></field>
</subform></template></xdp:xdp>"##;
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(xdp)), None).expect("opens");
    let doc = s.get(id).unwrap();
    let field = |end: &str| doc.form.iter().find(|f| f.name.ends_with(end)).unwrap_or_else(|| panic!("no field {end}"));
    assert!(field("code").has(field_flags::DO_NOT_SCROLL) && !field("note").has(field_flags::DO_NOT_SCROLL));
    let tb = doc.field_text_box(&field("code").name, 0).expect("a text box");
    assert!(tb.fits("A1B2") && !tb.fits(&"W".repeat(40)), "a 1-inch field at 10 pt");
    assert!(doc.field_text_box("no such field", 0).is_none() && doc.field_text_box(&field("code").name, 5).is_none());
}

/// A round XFA check button opens drawn round: the appearance its laid-out widget gets draws a
/// circle, the on state with its mark in it, while a square one keeps its box.
#[test]
fn round_xfa_check_buttons_open_drawn_round() {
    let xdp = r##"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="form" layout="tb">
<field name="round" w="1in" h="0.3in"><ui><checkButton shape="round"/></ui><items><integer>1</integer><integer>0</integer></items></field>
<field name="square" w="1in" h="0.3in"><ui><checkButton/></ui><items><integer>1</integer><integer>0</integer></items></field>
</subform></template></xdp:xdp>"##;
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(xdp)), None).expect("opens");
    let cos = s.get(id).unwrap().editor.as_ref().unwrap().cos.clone();
    let all = pdfcraft_forms::fields(&cos);
    // The decoded appearance of `name`'s widget in `state`.
    let drawn = |name: &str, state: &[u8]| {
        let w = all.iter().find(|f| f.name.ends_with(name)).unwrap_or_else(|| panic!("no field {name}")).widgets[0].obj;
        let wd = cos.get(w).as_dict().cloned().unwrap();
        let n = cos.resolve(wd.get(b"AP").unwrap().as_dict().unwrap().get(b"N").unwrap());
        let r = n.as_dict().unwrap().reference(state).unwrap_or_else(|| panic!("{name} has no {} state", String::from_utf8_lossy(state)));
        let pdfcraft_cos::Object::Stream(st) = &*cos.get(r) else { panic!("a stream") };
        String::from_utf8_lossy(&st.decoded().unwrap()).into_owned()
    };
    for state in [&b"Off"[..], b"1"] {
        let round = drawn("round", state);
        assert!(round.contains(" c S") && !round.contains(" re S"), "the round one's border is a circle: {round}");
        assert!(drawn("square", state).contains(" re S"), "the square one keeps its box");
    }
}

#[test]
fn a_dynamic_xfa_form_is_saved_without_the_usage_rights_its_layout_voided() {
    let xdp = pdfcraft_xfa::fixtures::template(3);
    let original = Arc::new(pdfcraft_xfa::fixtures::usage_rights_shell(&xdp, "", false));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, original.clone(), None).expect("opens");
    let doc = s.get(id).unwrap();
    let warnings = &doc.xfa.as_ref().expect("laid out from the template").warnings;
    assert!(warnings.iter().any(|w| w.contains("usage rights")), "the user is told: {warnings:?}");
    assert!(!doc.dirty, "laying out is still not an edit");
    s.apply(id, Edit::SetFieldValue { name: "familyName".into(), value: FieldValue::Text("Singh".into()) }).unwrap();
    let incremental = s.save_bytes(id).unwrap();
    assert!(incremental.starts_with(&original[..]), "still an incremental save");
    // Saved either way, the file no longer carries them, and keeps what was filled in.
    for saved in [incremental, s.save_full_bytes(id).unwrap()] {
        let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
        let catalog = cos.get(cos.root().unwrap()).as_dict().cloned().unwrap();
        assert!(!catalog.contains(b"Perms"), "{catalog:?}");
        let again = s.open("saved.pdf", None, saved, None).unwrap();
        let d2 = s.get(again).unwrap();
        assert_eq!(d2.form.iter().find(|f| f.name == "familyName").unwrap().value, vec!["Singh".to_string()]);
        let warnings = &d2.xfa.as_ref().expect("the layout is remembered").warnings;
        assert!(!warnings.iter().any(|w| w.contains("usage rights")), "nothing left to say: {warnings:?}");
    }
}

#[test]
fn dynamic_xfa_forms_are_laid_out_on_open_filled_and_saved_incrementally() {
    let original = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template(3)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, original.clone(), None).expect("opens");
    let doc = s.get(id).unwrap();
    let x = doc.xfa.as_ref().expect("laid out from the template");
    assert_eq!((x.pages, x.fields), (2, 14), "{x:?}");
    assert!(x.warnings.is_empty(), "{:?}", x.warnings);
    assert_eq!(doc.info.pages.len(), 2, "the placeholder page is gone");
    assert_eq!(doc.info.xfa, Some(pdfcraft_render::Xfa::Dynamic), "still an XFA form for Adobe's viewers");
    assert!(!doc.dirty, "laying out is not an edit");
    assert!(doc.bytes.starts_with(&original[..]), "the layout is an appended revision; the original bytes stay");
    let names: Vec<&str> = doc.form.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"familyName") && names.contains(&"answer") && names.contains(&"what2"), "{names:?}");
    let family = doc.form.iter().find(|f| f.name == "familyName").unwrap();
    assert_eq!((family.tooltip.as_deref(), family.max_len), (Some("Your family name"), Some(30)));
    let answer = doc.form.iter().find(|f| f.name == "answer").unwrap();
    assert_eq!(answer.widgets.iter().filter_map(|w| w.on_state.clone()).collect::<Vec<_>>(), ["Y", "N"]);
    assert!(page_texts(&s, id)[0].contains("Sample Form"), "{:?}", page_texts(&s, id));
    // Fill it in and save: one more incremental revision on top.
    s.apply(id, Edit::SetFieldValue { name: "familyName".into(), value: FieldValue::Text("Singh".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "answer".into(), value: FieldValue::Radio(Some("N".into())) }).unwrap();
    assert!(s.get(id).unwrap().dirty);
    let saved = s.save_bytes(id).unwrap();
    assert!(saved.starts_with(&original[..]));
    // The datasets packet carries the values too, for Adobe's viewers.
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let data = pdfcraft_xfa::parse_datasets(&pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp).expect("a datasets packet");
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form[0].head[0].familyName[0]")), Some("Singh"));
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form[0].head[0].answer[0]")), Some("N"));
    let id2 = s.open("saved.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    assert_eq!(d2.info.pages.len(), 2, "a saved form is not laid out a second time");
    assert_eq!(d2.xfa.as_ref().map(|x| (x.pages, x.fields)), Some((2, 14)), "the layout is remembered");
    assert_eq!(d2.form.iter().find(|f| f.name == "familyName").unwrap().value, vec!["Singh".to_string()]);
    assert_eq!(d2.form.iter().find(|f| f.name == "answer").unwrap().value, vec!["N".to_string()]);
}

#[test]
fn a_signed_xfa_signature_box_shows_the_picture_the_file_holds() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("signed.pdf", None, Arc::new(pdfcraft_xfa::fixtures::signed_shell()), None).expect("opens");
    let doc = s.get(id).unwrap();
    assert_eq!(doc.xfa.as_ref().map(|x| (x.pages, x.fields)), Some((1, 2)));
    let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
    let out = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(out.error.is_none(), "{:?}", out.error);
    let at = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * out.width + x) * 4) as usize;
        out.rgba[i..i + 4].try_into().unwrap()
    };
    // The signed box (3in by 1in from half an inch in) is filled with the signature's blue
    // picture; the unsigned one below it is not.
    assert_eq!(at(36 + 108, 36 + 36), [0, 0, 255, 255]);
    assert_ne!(at(36 + 108, 36 + 72 + 36), [0, 0, 255, 255]);
}

#[test]
fn an_xfa_form_without_a_template_opens_with_its_placeholder_and_a_warning() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell("<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\"></xdp:xdp>"));
    let mut s = Session::new();
    let id = s.open("form.pdf", None, bytes, None).expect("opens");
    let doc = s.get(id).unwrap();
    assert!(doc.xfa.is_none());
    assert_eq!(doc.info.pages.len(), 1);
    assert!(doc.info.warnings.iter().any(|w| w.contains("could not be laid out") && w.contains("no template")), "{:?}", doc.info.warnings);
}

#[test]
fn dynamic_xfa_forms_open_with_the_values_and_rows_of_their_data() {
    let data = "<form><head><familyName>Kaur</familyName><born>2001-02-03</born><answer>Y</answer></head><table><row><what0>a</what0></row><row><what0>b</what0></row><row><what0>c</what0></row></table></form>";
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::template_with_data(1, data)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, bytes, None).expect("opens");
    let doc = s.get(id).unwrap();
    let value = |n: &str| doc.form.iter().find(|f| f.name == n).unwrap_or_else(|| panic!("no field {n}")).value.clone();
    assert_eq!(value("familyName"), vec!["Kaur".to_string()]);
    assert_eq!(value("born"), vec!["2001-02-03".to_string()]);
    assert_eq!(value("answer"), vec!["Y".to_string()]);
    assert_eq!(value("what0_3"), vec!["c".to_string()], "three rows from the data: {:?}", doc.form.iter().map(|f| &f.name).collect::<Vec<_>>());
    assert!(!doc.dirty);
}

/// #3180: an XFA form with no fields whose configuration says `dynamicRender` is `forbidden` is a
/// static form: its pages are the form. Opening it used to lay the template out into new pages
/// and write them as a revision, so even a save with no change replaced the pages.
#[test]
fn an_xfa_form_without_fields_that_forbids_dynamic_rendering_keeps_its_pages() {
    // The fixture's own config packet gains `acrobat/acrobat7/dynamicRender`.
    let template = pdfcraft_xfa::fixtures::template(1);
    let forbidden = "<acrobat><acrobat7><dynamicRender>forbidden</dynamicRender></acrobat7></acrobat><present>";
    let with_config = template.replacen("<present>", forbidden, 1);
    assert_ne!(with_config, template, "the fixture has a config packet");
    // The fixture shell without /NeedsRendering (blanked in place, so the offsets stay right).
    let shell = |xdp: &str| {
        let bytes = pdfcraft_xfa::fixtures::shell(xdp);
        let at = bytes.windows(20).position(|w| w == b"/NeedsRendering true").expect("the fixture sets it");
        let mut bytes = bytes;
        bytes.splice(at..at + 20, *b"                    ");
        Arc::new(bytes)
    };
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let original = shell(&with_config);
    let id = s.open("static.pdf", None, original.clone(), None).expect("opens");
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.xfa, Some(pdfcraft_render::Xfa::Static));
    assert!(doc.xfa.is_none(), "nothing was laid out");
    assert_eq!(doc.bytes[..], original[..], "no revision was appended on open");
    assert_eq!(doc.info.pages.len(), 1);
    // The same form without the config is still taken to be dynamic: laid out from the template
    // into an appended revision.
    let plain = shell(&template);
    let id = s.open("dynamic.pdf", None, plain.clone(), None).expect("opens");
    let doc = s.get(id).unwrap();
    assert!(doc.xfa.is_some(), "laid out from the template");
    assert!(doc.bytes.len() > plain.len() && doc.bytes.starts_with(&plain[..]), "the layout is a revision");
}

/// A static XFA form's fields take their values from the datasets on opening, and edits are
/// written back there.
#[test]
fn static_xfa_forms_take_their_values_from_the_datasets_and_write_them_back() {
    let original = Arc::new(pdfcraft_xfa::fixtures::static_shell("<form1><page1><name>Ada</name><agree>1</agree></page1></form1>"));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("static.pdf", None, original.clone(), None).expect("opens");
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.xfa, Some(pdfcraft_render::Xfa::Static));
    assert!(doc.xfa.is_none(), "nothing was laid out");
    assert_eq!(doc.form.iter().find(|f| f.name == "form1[0].page1[0].name[0]").unwrap().value, vec!["Ada".to_string()]);
    assert_eq!(doc.form.iter().find(|f| f.name == "form1[0].page1[0].agree[0]").unwrap().value, vec!["1".to_string()]);
    assert!(!doc.dirty && doc.bytes.starts_with(&original[..]), "the values are an appended revision");
    // Rewriting the fields from the XFA data is recorded, not silent.
    let note = doc.info.warnings.iter().find(|w| w.contains("taken from this form's XFA data")).expect("a warning");
    assert!(note.starts_with("2 form field values") && note.contains("form1[0].page1[0].name[0]"), "{note}");
    s.apply(id, Edit::SetFieldValue { name: "form1[0].page1[0].name[0]".into(), value: FieldValue::Text("Grace".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "form1[0].page1[0].agree[0]".into(), value: FieldValue::Check(false) }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let data = pdfcraft_xfa::parse_datasets(&pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp).unwrap();
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form1[0].page1[0].name[0]")), Some("Grace"));
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form1[0].page1[0].agree[0]")), Some(""), "unchecked, with no off value");
    // Reopening agrees with itself: nothing to apply, nothing dirty.
    let id2 = s.open("again.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    assert_eq!(d2.form.iter().find(|f| f.name == "form1[0].page1[0].name[0]").unwrap().value, vec!["Grace".to_string()]);
    assert!(!d2.dirty);
    assert!(!d2.info.warnings.iter().any(|w| w.contains("taken from this form's XFA data")), "nothing was rewritten");
}

/// A static XFA form whose values are only in its datasets takes every one on opening, as
/// filling the fields one at a time would leave it: values, appearances and the note; a
/// read-only field keeps its own value.
#[test]
fn a_static_xfa_form_takes_many_data_values_as_one_by_one() {
    let n = 60;
    let original = Arc::new(pdfcraft_xfa::fixtures::static_shell_many(n));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("many.pdf", None, original.clone(), None).expect("opens");
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.xfa, Some(pdfcraft_render::Xfa::Static));
    assert!(!doc.dirty && doc.bytes.starts_with(&original[..]), "the values are an appended revision");
    // The read-only first field keeps its own (empty) value, and is not named.
    let note = doc.info.warnings.iter().find(|w| w.contains("taken from this form's XFA data")).expect("a warning");
    assert!(note.starts_with("59 form field values") && note.contains("form1[0].page1[0].f1[0]") && note.ends_with("and 54 more"), "{note}");
    assert!(!note.contains("f0[0]"), "{note}");
    // The same values filled one at a time.
    let mut one_by_one = pdfcraft_cos::Document::open(original).unwrap();
    for i in 0..n {
        let r = pdfcraft_forms::set_value(&mut one_by_one, &format!("form1[0].page1[0].f{i}[0]"), &FieldValue::Text(format!("v{i}")));
        assert_eq!(r.is_err(), i == 0, "field {i}: {r:?}");
    }
    let opened = &doc.editor.as_ref().unwrap().cos;
    let shown = |d: &pdfcraft_cos::Document| -> Vec<(String, Vec<String>, String)> {
        pdfcraft_forms::fields(d)
            .into_iter()
            .map(|f| {
                let w = f.widgets.first().map(|w| w.obj).unwrap();
                let ap = d.get(w).as_dict().and_then(|w| w.get(b"AP").cloned()).map(|a| d.resolve(&a));
                let n = ap.and_then(|ap| ap.as_dict().and_then(|a| a.get(b"N").cloned())).map(|n| d.resolve(&n));
                let look = match n.as_deref() {
                    Some(pdfcraft_cos::Object::Stream(st)) => String::from_utf8_lossy(&st.decoded().unwrap()).into_owned(),
                    _ => String::new(),
                };
                (f.name, f.value, look)
            })
            .collect()
    };
    let got = shown(opened);
    assert_eq!(got.len(), n);
    assert_eq!(got, shown(&one_by_one));
    assert!(got[0].1.is_empty());
    assert!(got.iter().enumerate().skip(1).all(|(i, (_, v, ap))| *v == [format!("v{i}")] && ap.contains(&format!("(v{i})"))));
}

/// Opening a static XFA form whose datasets fill thousands of fields took minutes: each value
/// read the whole form again (twice), so the time grew with the square of the fields. Eight
/// times the fields now takes at most about eight times as long (it took about fifty).
///
/// Each round opens the small form and then the large one, so a busy spell on a shared machine
/// slows both, and the best round's ratio is checked. Timing every small opening before the
/// large ones let such a spell fall on the large ones alone.
#[test]
fn opening_a_static_xfa_form_grows_linearly_with_its_data_values() {
    let small = Arc::new(pdfcraft_xfa::fixtures::static_shell_many(100));
    let large = Arc::new(pdfcraft_xfa::fixtures::static_shell_many(800));
    let time = |bytes: &Arc<Vec<u8>>| {
        let mut s = Session::new();
        let start = std::time::Instant::now();
        s.open("many.pdf", None, bytes.clone(), None).expect("opens");
        start.elapsed()
    };
    // The first openings also pay for what the process sets up once.
    time(&small);
    time(&large);
    let rounds: Vec<_> = (0..5).map(|_| (time(&small), time(&large))).collect();
    let ratio = |(s, l): &(std::time::Duration, std::time::Duration)| l.as_secs_f64() / s.as_secs_f64().max(1e-6);
    let best = rounds.iter().map(ratio).fold(f64::INFINITY, f64::min);
    assert!(best < 16.0, "100 and 800 fields, by round: {rounds:?} (best {best:.1}x)");
}

/// A date goes into the XFA data in the field's data picture (`<bind><picture>`), as other
/// viewers write and read it, and comes back from there when the form is opened again. It used
/// to be written as an ISO date whatever the picture said.
#[test]
fn xfa_dates_are_written_and_read_back_in_their_data_picture() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::bound_date_template("")));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, bytes, None).expect("opens");
    s.apply(id, Edit::SetFieldValue { name: "due".into(), value: FieldValue::Text("10/18/2026".into()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "when".into(), value: FieldValue::Text("10/18/2026".into()) }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let data = pdfcraft_xfa::parse_datasets(&pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp).unwrap();
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form[0].page[0].due[0]")), Some("20261018"), "in the field's data picture");
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form[0].page[0].when[0]")), Some("2026-10-18"), "ISO without one");
    let id2 = s.open("again.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    let value = |n: &str| d2.form.iter().find(|f| f.name == n).unwrap_or_else(|| panic!("no field {n}")).value.clone();
    assert_eq!(value("due"), vec!["10/18/2026".to_string()]);
    assert_eq!(value("when"), vec!["10/18/2026".to_string()]);
}

/// Same-named date fields are each written in their own data picture, and a data picture
/// without a day keeps the ISO date.
#[test]
fn each_xfa_date_field_is_written_in_its_own_data_picture_and_a_partial_one_keeps_iso() {
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::sibling_date_template("")));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, bytes, None).expect("opens");
    // Two siblings named `due` (laid out as `due` and `due_2`) and a field bound `YYYYMM`.
    for name in ["due", "due_2", "month"] {
        s.apply(id, Edit::SetFieldValue { name: name.into(), value: FieldValue::Text("10/18/2026".into()) }).unwrap();
    }
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let data = pdfcraft_xfa::parse_datasets(&pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp).unwrap();
    let at = |som: &str| data.text_at(&pdfcraft_xfa::som_to_path(som)).map(str::to_string);
    assert_eq!(at("form[0].page[0].due[0]").as_deref(), Some("20261018"), "the first sibling's data picture");
    assert_eq!(at("form[0].page[0].due[1]").as_deref(), Some("18102026"), "the second sibling's own");
    // A data picture without a day would drop the one the field shows: the data keeps ISO.
    assert_eq!(at("form[0].page[0].month[0]").as_deref(), Some("2026-10-18"));
    let id2 = s.open("again.pdf", None, saved, None).unwrap();
    let d2 = s.get(id2).unwrap();
    for name in ["due", "due_2", "month"] {
        let f = d2.form.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}"));
        assert_eq!(f.value, vec!["10/18/2026".to_string()], "{name} reads back");
    }
}

/// Data no field binds to (other namespaces, history) survives an edit and a save.
#[test]
fn xfa_data_no_field_binds_to_survives_an_edit_and_save() {
    // Regression: editing a field replaced the whole data element with the fields' values.
    let data = concat!(
        "<form1><page1><name>Ada</name><agree>1</agree><extra xmlns:my=\"urn:my\"><my:note id=\"7\">keep</my:note></extra></page1>",
        "<history><entry><when>2020</when></entry></history></form1><my:other xmlns:my=\"urn:my\"><my:x>unbound</my:x></my:other>"
    );
    let original = Arc::new(pdfcraft_xfa::fixtures::static_shell(data));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("static.pdf", None, original, None).expect("opens");
    s.apply(id, Edit::SetFieldValue { name: "form1[0].page1[0].name[0]".into(), value: FieldValue::Text("Grace".into()) }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let xdp = pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp;
    for kept in [
        "<extra xmlns:my=\"urn:my\"><my:note id=\"7\">keep</my:note></extra>",
        "<history><entry><when>2020</when></entry></history>",
        "<my:other xmlns:my=\"urn:my\"><my:x>unbound</my:x></my:other>",
    ] {
        assert!(xdp.contains(kept), "lost {kept:?}: {xdp}");
    }
    let data = pdfcraft_xfa::parse_datasets(&xdp).unwrap();
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form1[0].page1[0].name[0]")), Some("Grace"));
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form1[0].page1[0].agree[0]")), Some("1"));
    // A value that can't be written is reported on the document.
    let original = Arc::new(pdfcraft_xfa::fixtures::static_shell("<form1><page1><name><b>rich</b></name></page1></form1>"));
    let id = s.open("rich.pdf", None, original, None).expect("opens");
    s.apply(id, Edit::SetFieldValue { name: "form1[0].page1[0].name[0]".into(), value: FieldValue::Text("plain".into()) }).unwrap();
    let doc = s.get(id).unwrap();
    assert!(doc.info.warnings.iter().any(|w| w.contains("structured content")), "{:?}", doc.info.warnings);
    assert!(doc.xfa_warnings.iter().any(|w| w.contains("structured content")));
}

/// An XFA form's choice lists, signature and picture fields open as their kinds of field, take
/// values that scripts read, and are saved into the datasets.
#[test]
fn xfa_choice_lists_signature_and_picture_fields_open_fill_script_and_save() {
    let data = "<form><page1><country>Japan</country><langs>English\nSpanish</langs></page1></form>";
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("fields.pdf", None, bytes, None).expect("opens");
    let field = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).cloned().unwrap_or_else(|| panic!("no field {n}"));
    use pdfcraft_forms::FieldKind as K;
    // The forms layer sees a combo box with its options and the data's saved value, a
    // multi-select list with both picks, an editable combo, a password field and a signature.
    let country = field(&s, "country");
    assert_eq!((country.kind, country.value.clone()), (K::Combo, vec!["JP".to_string()]));
    assert_eq!(country.options, vec![("CA".into(), "Canada".into()), ("FR".into(), "France".into()), ("JP".into(), "Japan".into())]);
    let langs = field(&s, "langs");
    assert_eq!(langs.kind, K::List);
    assert!(langs.has(pdfcraft_forms::flags::MULTI_SELECT));
    assert_eq!(langs.value, vec!["English".to_string(), "Spanish".to_string()]);
    assert!(field(&s, "other").has(pdfcraft_forms::flags::EDIT));
    assert!(field(&s, "pin").has(pdfcraft_forms::flags::PASSWORD));
    // The signature field is named by its SOM path, as another viewer finds a signature.
    assert_eq!(field(&s, "form[0].page1[0].sign[0]").kind, K::Signature);
    // Scripts read a choice list's saved value.
    assert_eq!(field(&s, "summary").value, vec!["Country: JP".to_string()], "{:?}", s.take_js_output(id));
    // Choosing another item recalculates; the list takes several values; a typed value is kept
    // in the editable list and refused in the fixed one.
    s.apply(id, Edit::SetFieldValue { name: "country".into(), value: FieldValue::Choice(vec!["CA".into()]) }).unwrap();
    assert_eq!(field(&s, "summary").value, vec!["Country: CA".to_string()]);
    s.apply(id, Edit::SetFieldValue { name: "langs".into(), value: FieldValue::Choice(vec!["French".into(), "Spanish".into()]) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "other".into(), value: FieldValue::Text("typed".into()) }).unwrap();
    assert!(s.apply(id, Edit::SetFieldValue { name: "country".into(), value: FieldValue::Choice(vec!["Mars".into()]) }).is_err());
    // Saved: the datasets hold the saved values, several on separate lines; reopened, the
    // fields and the calculation are what they were.
    let saved = s.save_bytes(id).unwrap();
    let cos = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    let d = pdfcraft_xfa::data_of(&cos).unwrap();
    assert_eq!(d.text_at(&pdfcraft_xfa::som_to_path("form[0].page1[0].country[0]")), Some("CA"));
    assert_eq!(d.text_at(&pdfcraft_xfa::som_to_path("form[0].page1[0].langs[0]")), Some("French\nSpanish"));
    assert_eq!(d.text_at(&pdfcraft_xfa::som_to_path("form[0].page1[0].other[0]")), Some("typed"));
    let id2 = s.open("again.pdf", None, saved.clone(), None).unwrap();
    let d2 = s.get(id2).unwrap();
    let f2 = |n: &str| d2.form.iter().find(|f| f.name == n).unwrap().value.clone();
    assert_eq!(f2("country"), vec!["CA".to_string()]);
    assert_eq!(f2("langs"), vec!["French".to_string(), "Spanish".to_string()]);
    assert_eq!(f2("summary"), vec!["Country: CA".to_string()]);
    // The PNG of the image field and the GIF of the draw are pictures on the page.
    assert_eq!(s.get(id).unwrap().page_images(0).len(), 2);
    // Filled in by another viewer since: saved values, several on separate lines, and shown
    // text (an older viewer's), are taken on reopen; shown text that means the current value
    // is not a change.
    let mut cos = pdfcraft_cos::Document::open(saved).unwrap();
    pdfcraft_xfa::write_data_value(&mut cos, &pdfcraft_xfa::som_to_path("form[0].page1[0].langs[0]"), "English\nSpanish").unwrap();
    pdfcraft_xfa::write_data_value(&mut cos, &pdfcraft_xfa::som_to_path("form[0].page1[0].country[0]"), "Canada").unwrap();
    let edited = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).unwrap());
    let id3 = s.open("elsewhere.pdf", None, edited, None).unwrap();
    let d3 = s.get(id3).unwrap();
    let f3 = |n: &str| d3.form.iter().find(|f| f.name == n).unwrap().value.clone();
    assert_eq!(f3("langs"), vec!["English".to_string(), "Spanish".to_string()]);
    assert_eq!(f3("country"), vec!["CA".to_string()]);
    let taken: Vec<&String> = d3.info.warnings.iter().filter(|w| w.contains("another viewer")).collect();
    assert_eq!(taken.len(), 1, "{:?}", d3.info.warnings);
    assert!(taken[0].contains("langs") && !taken[0].contains("country"), "{}", taken[0]);
}

/// A form's data exports, imports and takes pictures through the session, and what it holds
/// comes back the same.
#[test]
fn xfa_data_exports_imports_and_pictures_round_trip_through_the_session() {
    use pdfcraft_xfa::som_to_path;
    let data = "<form><page1><country>JP</country><langs>English\nSpanish</langs></page1></form>";
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("fields.pdf", None, bytes, None).expect("opens");
    let field = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).cloned().unwrap_or_else(|| panic!("no field {n}"));
    // Export: XML is the datasets document, XDP wraps it; the fields' current values are in it.
    s.apply(id, Edit::SetFieldValue { name: "country".into(), value: FieldValue::Choice(vec!["CA".into()]) }).unwrap();
    let xml = String::from_utf8(s.export_data(id, DataFormat::Xml, false, true).unwrap()).unwrap();
    assert!(xml.starts_with("<?xml") && xml.contains("<form") && xml.contains("<country>CA</country>") && !xml.contains("<xfa:datasets"), "{xml}");
    let xdp = String::from_utf8(s.export_data(id, DataFormat::Xdp, false, true).unwrap()).unwrap();
    assert!(xdp.contains("<xdp:xdp") && xdp.contains("<country>CA</country>") && xdp.contains("href=\"fields.pdf\""), "{xdp}");
    // Import: the data file's values replace the form's; the calculation runs; undo brings
    // the old values back.
    let file = "<form><page1><country>FR</country><langs>French</langs><pin>1234</pin></page1></form>";
    s.apply(id, Edit::ImportData { name: "data.xml".into(), bytes: Arc::new(file.as_bytes().to_vec()) }).unwrap();
    assert_eq!(field(&s, "country").value, vec!["FR".to_string()]);
    assert_eq!(field(&s, "langs").value, vec!["French".to_string()]);
    assert_eq!(field(&s, "pin").value, vec!["1234".to_string()]);
    assert_eq!(field(&s, "summary").value, vec!["Country: FR".to_string()], "{:?}", s.take_js_output(id));
    s.undo(id).unwrap();
    assert_eq!(field(&s, "country").value, vec!["CA".to_string()]);
    s.redo(id).unwrap();
    assert_eq!(field(&s, "country").value, vec!["FR".to_string()]);
    // A file in another encoding, and one with a long comment before its root, import too.
    let latin1: Vec<u8> =
        "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><form><page1><other>Société</other></page1></form>".chars().map(|c| c as u32 as u8).collect();
    s.apply(id, Edit::ImportData { name: "l1.xml".into(), bytes: Arc::new(latin1) }).unwrap();
    assert_eq!(field(&s, "other").value, vec!["Société".to_string()]);
    let commented = format!("<!-- {} --><form><page1><other>after</other></page1></form>", "x".repeat(5000));
    s.apply(id, Edit::ImportData { name: "c.xml".into(), bytes: Arc::new(commented.into_bytes()) }).unwrap();
    assert_eq!(field(&s, "other").value, vec!["after".to_string()]);
    let utf16: Vec<u8> = "<form><page1><other>wide</other></page1></form>".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    s.apply(id, Edit::ImportData { name: "w.xml".into(), bytes: Arc::new([&[0xFF, 0xFE][..], &utf16].concat()) }).unwrap();
    assert_eq!(field(&s, "other").value, vec!["wide".to_string()]);
    let shift_jis = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><form/>".to_vec();
    assert!(s.apply(id, Edit::ImportData { name: "sj.xml".into(), bytes: Arc::new(shift_jis) }).is_err());
    // A data file for another form is refused.
    assert!(s.apply(id, Edit::ImportData { name: "o.xml".into(), bytes: Arc::new(b"<other><x>1</x></other>".to_vec()) }).is_err());
    assert_eq!(field(&s, "other").value, vec!["wide".to_string()]);
    // XFDF still goes the usual way on an XFA form.
    let xfdf =
        "<?xml version=\"1.0\"?><xfdf xmlns=\"http://ns.adobe.com/xfdf/\"><fields><field name=\"pin\"><value>9999</value></field></fields></xfdf>";
    s.apply(id, Edit::ImportData { name: "d.xfdf".into(), bytes: Arc::new(xfdf.as_bytes().to_vec()) }).unwrap();
    assert_eq!(field(&s, "pin").value, vec!["9999".to_string()]);
    assert_eq!(field(&s, "country").value, vec!["FR".to_string()]);
    // A picture chosen for the image field goes into the data and onto the page.
    let png = pdfcraft_xfa::fixtures::tiny_png();
    assert_eq!(field(&s, "photo").button, Some(pdfcraft_forms::af::ButtonAction::ImportIcon), "clicking it picks a picture");
    s.apply(id, Edit::SetFieldImage { name: "photo".into(), image: Arc::new(pdfcraft_xfa::fixtures::TINY_GIF.to_vec()) }).unwrap();
    let cos = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    let d = pdfcraft_xfa::data_of(&cos).unwrap();
    assert!(d.text_at(&som_to_path("form[0].page1[0].photo[0]")).unwrap().starts_with("R0lGODlh"));
    assert!(pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp.contains("xfa:contentType=\"image/gif\""));
    assert_eq!(s.get(id).unwrap().page_images(0).len(), 2, "the field's picture (now the GIF) and the logo");
    // Only pictures the form can show; only image fields.
    for (data, declared) in [(pdfcraft_xfa::fixtures::tiny_bmp(false), "image/bmp"), (pdfcraft_xfa::fixtures::tiny_tiff(false), "image/tiff")] {
        s.apply(id, Edit::SetFieldImage { name: "photo".into(), image: Arc::new(data) }).unwrap();
        let cos = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
        assert!(pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp.contains(&format!("xfa:contentType=\"{declared}\"")));
    }
    assert!(s.apply(id, Edit::SetFieldImage { name: "photo".into(), image: Arc::new(b"BM\0\0\0\0".to_vec()) }).is_err());
    assert!(s.apply(id, Edit::SetFieldImage { name: "country".into(), image: Arc::new(pdfcraft_xfa::fixtures::TINY_GIF.to_vec()) }).is_err());
    // One datasets stream per edit, however many steps it took.
    let before = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap().object_numbers().len();
    s.apply(id, Edit::SetFieldImage { name: "photo".into(), image: Arc::new(pdfcraft_xfa::fixtures::TINY_GIF.to_vec()) }).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "pin".into(), value: FieldValue::Text("1".into()) }).unwrap();
    let after = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    let datasets = after
        .object_numbers()
        .into_iter()
        .filter(|n| matches!(&*after.get(pdfcraft_cos::ObjRef { num: *n, generation: 0 }), pdfcraft_cos::Object::Stream(st) if st.decoded().is_ok_and(|b| b.starts_with(b"<xfa:datasets"))))
        .count();
    assert!(datasets <= 3, "{datasets} datasets streams (one per edit at most), {before} objects before");
    s.apply(id, Edit::SetFieldImage { name: "photo".into(), image: Arc::new(png) }).unwrap();
    let cos = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    assert!(pdfcraft_xfa::data_of(&cos).unwrap().text_at(&som_to_path("form[0].page1[0].photo[0]")).unwrap().starts_with("iVBOR"));
    // Reopened: the picture and the imported values are what they were.
    let id2 = s.open("again.pdf", None, s.save_bytes(id).unwrap(), None).unwrap();
    let d2 = s.get(id2).unwrap();
    assert_eq!(d2.form.iter().find(|f| f.name == "country").unwrap().value, vec!["FR".to_string()]);
    assert_eq!(d2.page_images(0).len(), 2);
}

/// Importing data into a static form replaces every value, emptying the fields the file
/// doesn't name, and pictures chosen for it reach its data.
#[test]
fn xfa_data_import_on_a_static_form_replaces_every_value_and_pictures_reach_its_data() {
    let original = Arc::new(pdfcraft_xfa::fixtures::static_shell("<form1><page1><name>Ada</name><agree>1</agree></page1></form1>"));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("static.pdf", None, original, None).expect("opens");
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name.ends_with(n)).map(|f| f.value.clone()).unwrap_or_default();
    assert_eq!(value(&s, "name[0]"), vec!["Ada".to_string()]);
    assert_eq!(value(&s, "agree[0]"), vec!["1".to_string()]);
    // The file names only `name`: `agree` is emptied too, since the data was replaced.
    s.apply(id, Edit::ImportData { name: "d.xml".into(), bytes: Arc::new(b"<form1><page1><name>Bob</name></page1></form1>".to_vec()) }).unwrap();
    assert_eq!(value(&s, "name[0]"), vec!["Bob".to_string()]);
    assert!(value(&s, "agree[0]").is_empty(), "{:?}", value(&s, "agree[0]"));
    let cos = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    let d = pdfcraft_xfa::data_of(&cos).unwrap();
    assert_eq!(d.text_at(&pdfcraft_xfa::som_to_path("form1[0].page1[0].name[0]")), Some("Bob"));
    assert!(d.text_at(&pdfcraft_xfa::som_to_path("form1[0].page1[0].agree[0]")).is_none_or(str::is_empty));
    let xml = String::from_utf8(s.export_data(id, DataFormat::Xml, false, true).unwrap()).unwrap();
    assert!(xml.contains("<name>Bob</name>") && !xml.contains("<agree>1</agree>"), "{xml}");
}
