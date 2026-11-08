//! Unit tests of the engine (part 6 of 7).

#[allow(unused_imports)]
use super::*;

/// Every refresh path (full, comments, content, undo, redo) lets go of the previous working file
/// and still shows the current state.
#[test]
fn edits_let_go_of_the_previous_working_file() {
    let (mut s, id) = session_with(2);
    let shape = Shape::Rectangle { rect: [20.0, 20.0, 60.0, 60.0] };
    let comment = || {
        Edit::AddAnnotation(NewAnnotation {
            page: 0,
            style: Style::default_for(&shape),
            shape: shape.clone(),
            contents: String::new(),
            author: "Test".into(),
        })
    };
    let watermark =
        || Edit::AddWatermark { pages: vec![0], settings: Watermark { text: "Draft".into(), ..Watermark::default() }, file: None, replace: false };
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: 90 }).unwrap();
    // A full refresh, a comment refresh, a content refresh, then undo and redo.
    type Step<'a> = Box<dyn Fn(&mut Session) + 'a>;
    let steps: [(&str, Step<'_>); 5] = [
        ("rotate", Box::new(|s: &mut Session| s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap())),
        ("comment", Box::new(move |s: &mut Session| s.apply(id, comment()).unwrap())),
        ("watermark", Box::new(move |s: &mut Session| s.apply(id, watermark()).unwrap())),
        ("undo", Box::new(|s: &mut Session| drop(s.undo(id).unwrap()))),
        ("redo", Box::new(|s: &mut Session| drop(s.redo(id).unwrap()))),
    ];
    for (name, step) in steps {
        let (bytes, display) = {
            let doc = s.get(id).unwrap();
            (Arc::downgrade(&doc.bytes), Arc::downgrade(&doc.display))
        };
        step(&mut s);
        // The old renderer's workers let go of the file when they exit, just after.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while bytes.upgrade().is_some() || display.upgrade().is_some() {
            assert!(std::time::Instant::now() < deadline, "{name}: the previous working file is still held");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    // What is shown is still the current state.
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.annotations.len(), 1);
    let _ = shown(&s, id, 0);
}

#[test]
fn copy_image_finds_the_image_under_a_point_and_decodes_it_at_full_size() {
    let mut s = Session::new();
    let mut px = image::RgbaImage::new(3, 2);
    px.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    px.put_pixel(2, 1, image::Rgba([0, 0, 255, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(px.clone()).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let pic = s.create_from_images(&[("p.png".into(), png.into_inner())]).unwrap();
    let id = s.open("p.pdf", None, pic, None).unwrap();
    let d = s.get(id).unwrap();
    let r = d.page_images(0)[0].rect;
    assert_eq!(d.page_image_at(0, [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]), Some(0));
    assert_eq!(d.page_image_at(0, [r[2] + 10.0, r[3] + 10.0]), None);
    assert_eq!(d.page_image_at(7, [0.0, 0.0]), None);
    let (w, h, rgba) = d.page_image_rgba(0, 0).unwrap();
    assert_eq!((w, h), (3, 2));
    assert_eq!(rgba, px.into_raw());
    assert!(d.page_image_rgba(0, 1).is_err());

    // When the image is cropped, page_image_at only hit-tests the visible cropped region:
    let crop_keep = [r[0] + (r[2] - r[0]) * 0.25, r[1] + (r[3] - r[1]) * 0.25, r[0] + (r[2] - r[0]) * 0.75, r[1] + (r[3] - r[1]) * 0.75];
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Crop(crop_keep) }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.page_image_at(0, [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]), Some(0));
    assert_eq!(d.page_image_at(0, [r[0] + 0.1, r[1] + 0.1]), None, "cropped-away corner must not hit-test positive");
}

/// Copy Image: a JPEG drawn through grouped Form artwork (as Word, InDesign and scanners write
/// them), with a caption over it and an empty full-page Form on top.
#[test]
fn copy_image_finds_a_picture_inside_form_artwork_under_text() {
    let jpeg = jpeg_bytes();
    let content = b"q 100 0 0 100 50 50 cm /Fm1 Do Q BT /F1 12 Tf 60 100 Td (Caption over the picture) Tj ET /Fm2 Do".to_vec();
    let form = b"/Im1 Do".to_vec();
    let stream = |dict: &str, data: &[u8]| {
        let mut o = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        o.extend_from_slice(data);
        o.extend_from_slice(b"\nendstream");
        o
    };
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> /XObject << /Fm1 6 0 R /Fm2 9 0 R >> >> >>".to_vec(),
        stream("", &content),
        stream("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Im1 7 0 R /Tiny 8 0 R >> >>", &form),
        stream("/Type /XObject /Subtype /Image /Width 3 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode", &jpeg),
        stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8", &[255, 0, 0]),
        stream("/Type /XObject /Subtype /Form /BBox [0 0 200 300]", b""),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(out), None).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.page_images(0).len(), 2, "{:?}", d.page_images(0));
    // Under the caption, inside the picture: the Form holding it, not the empty Form on top.
    assert_eq!(d.page_image_at(0, [100.0, 100.0]), Some(0));
    assert_eq!(d.page_image_at(0, [10.0, 10.0]), None, "the empty full-page Form holds no picture");
    // The largest picture in the group, as the JPEG stored in the PDF.
    assert_eq!(d.page_image_copy_file(0, 0).unwrap(), ("jpg", jpeg));
    assert!(d.page_image_copy_file(0, 1).is_err());
}

/// Copy Image reads a JBIG2 image's `/JBIG2Globals`: here they hold the page information, and the
/// image's own stream only an extension segment decoders skip, so it has a picture only through
/// them. Its default pixel is black.
#[test]
fn copy_image_reads_the_globals_of_a_jbig2_picture() {
    let mut globals = vec![0, 0, 0, 0, 48, 0, 1, 0, 0, 0, 19];
    globals.extend_from_slice(&16u32.to_be_bytes());
    globals.extend_from_slice(&2u32.to_be_bytes());
    globals.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0]); // resolution, flags, striping
    let image = [0, 0, 0, 1, 62, 0, 1, 0, 0, 0, 0];
    let stream = |dict: &str, data: &[u8]| {
        let mut o = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        o.extend_from_slice(data);
        o.extend_from_slice(b"\nendstream");
        o
    };
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>".to_vec(),
        stream("", b"q 160 0 0 20 20 100 cm /Im1 Do Q"),
        stream(
            "/Type /XObject /Subtype /Image /Width 16 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode /DecodeParms << /JBIG2Globals 6 0 R >>",
            &image,
        ),
        stream("", &globals),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new();
    let id = s.open("scan.pdf", None, Arc::new(out), None).unwrap();
    let d = s.get(id).unwrap();
    let (w, h, rgba) = d.page_image_rgba(0, 0).unwrap();
    assert_eq!((w, h), (16, 2));
    assert!(rgba.chunks(4).all(|p| p == [0, 0, 0, 255]), "the page's default pixel, from the globals");
    // An Office export shares one allowance across its pictures. One past it is left out, and the
    // export says the page is missing something, as for a picture past its bytes.
    let (pages, truncated) = d.export_pages_within(true, 1 << 20, &mut pdfcraft_cos::ImageBudget::with_work(u64::MAX));
    assert_eq!((pages[0].images.len(), truncated.len()), (1, 0));
    let (pages, truncated) = d.export_pages_within(true, 1 << 20, &mut pdfcraft_cos::ImageBudget::with_work(0));
    assert!(pages[0].images.is_empty());
    assert_eq!(truncated, [0]);
}

/// An Office export leaves out a picture it can't encode, as it did. That isn't a page cut short:
/// only a picture past what the export may do is reported as one.
#[test]
fn an_office_export_doesnt_call_an_unreadable_picture_truncated() {
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
        2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >> endobj\n\
        3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >> endobj\n\
        4 0 obj << /Length 31 >> stream\nq 160 0 0 20 20 100 cm /Im1 Do Q\nendstream endobj\n\
        5 0 obj << /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /JPXDecode /Length 4 >> stream\nnone\nendstream endobj\n\
        trailer << /Root 1 0 R >>\n%%EOF"
        .to_vec();
    let mut s = Session::new();
    let id = s.open("picture.pdf", None, Arc::new(pdf), None).unwrap();
    let (pages, truncated) = s.get(id).unwrap().export_pages_within(true, 1 << 20, &mut pdfcraft_cos::ImageBudget::with_work(u64::MAX));
    assert!(pages[0].images.is_empty());
    assert_eq!(truncated, Vec::<usize>::new());
}

/// Copy Image takes a JBIG2 picture as its pixels and as a PNG file, behind another filter
/// (ASCIIHexDecode) too: Export's decoding, through `ImageBudget::decode`.
#[test]
fn copy_image_copies_a_jbig2_picture_behind_another_filter() {
    // 16 × 2: the left half of the top row black, the rest white.
    let bits = [0x00, 0xFF, 0xFF, 0xFF];
    let coded = pdfcraft_filters::encode_jbig2_generic(&bits, 16, 2).unwrap();
    let hex: Vec<u8> = coded.iter().flat_map(|b| format!("{b:02x}").into_bytes()).chain(*b">").collect();
    let stream = |dict: &str, data: &[u8]| {
        let mut o = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        o.extend_from_slice(data);
        o.extend_from_slice(b"\nendstream");
        o
    };
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>".to_vec(),
        stream("", b"q 160 0 0 20 20 100 cm /Im1 Do Q"),
        stream(
            "/Type /XObject /Subtype /Image /Width 16 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter [/ASCIIHexDecode /JBIG2Decode]",
            &hex,
        ),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut s = Session::new();
    let id = s.open("scan.pdf", None, Arc::new(out), None).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.page_image_at(0, [30.0, 115.0]), Some(0));
    assert_eq!(d.page_image_copy_file(0, 0).unwrap().0, "png");
    let (w, h, rgba) = d.page_image_rgba(0, 0).unwrap();
    assert_eq!((w, h), (16, 2));
    let grey: Vec<u8> = rgba.chunks(4).map(|px| px[0]).collect();
    let expected: Vec<u8> = (0..32).map(|i| if i < 8 { 0 } else { 255 }).collect();
    assert_eq!(grey, expected, "the picture, black as 0");
}

#[test]
fn a_recovered_page_tree_opens_unmodified_and_edits_the_pages_shown() {
    let mut s = Session::new();
    let id = s.open("damaged.pdf", None, Arc::new(fixture_without_page_tree()), None).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.pages.len(), 2);
    assert!(!doc.editor.as_ref().unwrap().cos.is_modified(), "the repair is a revision of the opened file, not an unsaved edit");
    assert!(doc.repair_log().iter().any(|l| l.contains("page tree is missing")), "{:?}", doc.repair_log());
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: 90 }).unwrap();
    assert_eq!(s.get(id).unwrap().info.pages.iter().map(|p| p.rotation).collect::<Vec<_>>(), [0, 90]);
}

#[test]
fn a_signed_document_rebuilt_from_object_headers_refuses_edits() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(2)), None).unwrap();
    let opts = SignOptions { page: 1, rect: Some([20.0, 20.0, 180.0, 60.0]), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    s.mark_signed(id, signed, None).unwrap();
    // A broken startxref: the file opens only by rebuilding from object headers, and any save
    // of it is a full rewrite.
    let mut bytes = s.get(id).unwrap().bytes.as_ref().clone();
    let sx = bytes.windows(10).rposition(|w| w == b"startxref\n").unwrap() + 10;
    let end = sx + bytes[sx..].iter().position(|b| !b.is_ascii_digit()).unwrap();
    bytes.splice(sx..end, b"1".iter().copied());
    let id = s.open("damaged.pdf", None, Arc::new(bytes), None).unwrap();
    assert!(s.get(id).unwrap().is_signed());
    assert!(s.get(id).unwrap().editor.as_ref().unwrap().cos.revisions().is_empty(), "the fixture must be rebuilt");
    assert!(matches!(s.apply(id, rect_comment(0, [80.0, 80.0, 120.0, 120.0])), Err(EditError::SignedRewrite(_))));
    assert!(!s.get(id).unwrap().dirty);
}

/// #1324: marking a figure decorative rewrites that figure only. The header, added text and an
/// unknown entry on the original stream stay attached to the page. This lives in the engine tests
/// because edit and a11y are the same layer and cannot depend on each other.
#[test]
fn marking_a_figure_decorative_keeps_other_page_streams() {
    let figure = "/Figure << /MCID 0 >> BDC 10 10 50 50 re f EMC";
    let header = "BT (HeaderSample) Tj ET";
    let added = "BT (AddedSample) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> /StructTreeRoot 7 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents [4 0 R 5 0 R 6 0 R] /StructParents 0 >>".into(),
        format!("<< /VendorKey 7 /Length {} >>\nstream\n{figure}\nendstream", figure.len()),
        format!("<< /PCMark /HeaderFooter /Length {} >>\nstream\n{header}\nendstream", header.len()),
        format!(
            "<< /PCMark /Added /PCAdded << /Kind /Text /Rect [70 70 150 100] /Text (AddedSample) /Size 12 >> /Length {} >>\nstream\n{added}\nendstream",
            added.len()
        ),
        "<< /Type /StructTreeRoot /K 8 0 R /ParentTree 10 0 R >>".into(),
        "<< /Type /StructElem /S /Document /P 7 0 R /K [9 0 R] >>".into(),
        "<< /Type /StructElem /S /Figure /P 8 0 R /Pg 3 0 R /K 0 >>".into(),
        "<< /Nums [0 [9 0 R]] >>".into(),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for offset in offsets {
        bytes.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut doc = pdfcraft_cos::Document::open(Arc::new(bytes)).unwrap();
    assert_eq!(pdfcraft_edit::scan_markers(&doc).marks, vec![MarkKind::HeaderFooter]);
    let before = pdfcraft_edit::scan_markers(&doc);
    assert_eq!(before.added.len(), 1);
    a11y::mark_decorative(&mut doc, pdfcraft_cos::ObjRef::new(9, 0)).unwrap();
    assert!(a11y::figures(&doc).is_empty());
    let page = &pdfcraft_model::pages(&doc)[0];
    let contents = page.dict.get(b"Contents").and_then(|c| c.as_array()).unwrap();
    assert_eq!(contents.len(), 3, "the other streams stay in the page's content list");
    let figure_ref = contents[0].as_ref().unwrap();
    assert_eq!(contents[1].as_ref(), Some(pdfcraft_cos::ObjRef::new(5, 0)));
    assert_eq!(contents[2].as_ref(), Some(pdfcraft_cos::ObjRef::new(6, 0)));
    let stream = |doc: &pdfcraft_cos::Document, n: u32| match &*doc.get(pdfcraft_cos::ObjRef::new(n, 0)) {
        pdfcraft_cos::Object::Stream(s) => s.clone(),
        other => panic!("object {n} is {other:?}"),
    };
    let figure = stream(&doc, figure_ref.num);
    assert_eq!(figure.dict.get(b"VendorKey").and_then(pdfcraft_cos::Object::as_int), Some(7));
    let drawn = String::from_utf8(figure.decoded().unwrap()).unwrap();
    assert!(drawn.contains("/Artifact BMC") && !drawn.contains("/MCID 0"), "{drawn}");
    let header = stream(&doc, 5);
    assert_eq!(header.dict.name(b"PCMark"), Some(&b"HeaderFooter"[..]));
    assert!(String::from_utf8(header.decoded().unwrap()).unwrap().contains("HeaderSample"));
    assert!(stream(&doc, 6).dict.get(b"PCAdded").is_some());
    let after = pdfcraft_edit::scan_markers(&doc);
    assert_eq!(after.marks, vec![MarkKind::HeaderFooter]);
    assert_eq!(after.added.len(), 1);
    assert_eq!(after.added[0].content, before.added[0].content);
    assert_eq!(pdfcraft_edit::remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap(), 1);
    let left = pdfcraft_model::pages(&doc)[0].dict.get(b"Contents").and_then(|c| c.as_array()).cloned().unwrap();
    let text: String = left
        .iter()
        .filter_map(|o| match &*doc.resolve(o) {
            pdfcraft_cos::Object::Stream(s) => String::from_utf8(s.decoded().unwrap()).ok(),
            _ => None,
        })
        .collect();
    assert!(!text.contains("HeaderSample"));
    assert_eq!(pdfcraft_edit::scan_markers(&doc).added[0].content, before.added[0].content);
    assert_eq!(stream(&doc, figure_ref.num).dict.get(b"VendorKey").and_then(pdfcraft_cos::Object::as_int), Some(7));
}

/// Print as image (Acrobat's Advanced Print Setup): the print-ready PDF carries each page as a
/// picture, the size of the page, with no text left to search; in grayscale the pictures are
/// grey. A page used on several sheets is rendered once.
#[test]
fn printing_as_image_sends_pictures_instead_of_pages() {
    let (s, id) = session_with(3);
    let page_size = (s.get(id).unwrap().info.pages[1].width as f64, s.get(id).unwrap().info.pages[1].height as f64);
    let settings = print::Settings {
        pages: vec![1, 2, 1],
        layout: print::Layout::Size(print::SizeMode::Actual),
        raster: Some(print::Raster { dpi: 100.0, grayscale: false }),
        ..Default::default()
    };
    let bytes = s.print_pdf(id, &settings).unwrap();
    let mut s2 = Session::new();
    let id2 = s2.open("print.pdf", None, Arc::new(bytes.clone()), None).unwrap();
    assert_eq!(s2.get(id2).unwrap().info.pages.len(), 3, "three sheets, one page each");
    assert!(page_texts(&s2, id2).iter().all(|t| t.trim().is_empty()), "pictures carry no text");
    let doc = pdfcraft_cos::Document::open(Arc::new(bytes)).unwrap();
    let images: Vec<pdfcraft_cos::Dict> = doc
        .scan_objects()
        .filter_map(|(_, o)| match &*o {
            pdfcraft_cos::Object::Stream(st) if st.dict.name(b"Subtype") == Some(b"Image") => Some(st.dict.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 2, "two distinct pages, each rendered once: {images:?}");
    // 100 dpi: the picture's pixels match the page size.
    let w = images.iter().map(|d| d.int(b"Width").unwrap_or(0)).max().unwrap();
    assert!((w as f64 - page_size.0 * 100.0 / 72.0).abs() <= 1.0, "{w} px for {} pt", page_size.0);
    assert!(images.iter().all(|d| d.name(b"ColorSpace") == Some(b"DeviceRGB")), "{images:?}");
    // Grayscale renders the pictures grey (the printer is not merely asked for monochrome).
    let grey = s.print_pdf(id, &print::Settings { raster: Some(print::Raster { dpi: 72.0, grayscale: true }), ..settings.clone() }).unwrap();
    let mut s3 = Session::new();
    let id3 = s3.open("grey.pdf", None, Arc::new(grey), None).unwrap();
    let mut ex = crate::export::Exporter::new(s3.get(id3).unwrap());
    let png = ex.png(0, 36.0).unwrap();
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(img.pixels().all(|p| p[0] == p[1] && p[1] == p[2]), "every pixel grey");
}

/// #1936: a JPEG logo on a cover page reaches the Word file as the unchanged JPEG, both where it
/// sits clear of the title (inline) and where it overlaps it (a positioned page).
#[test]
fn exporting_a_jpeg_logo_to_word_keeps_the_image() {
    for (logo_y, overlaps) in [(650, false), (560, true)] {
        let pdf = jpeg_cover_pdf(logo_y);
        let mut s = Session::new();
        let id = s.open("c.pdf", None, Arc::new(pdf), None).unwrap();
        let d = s.get(id).unwrap();
        let pages = d.export_pages();
        assert_eq!((pages[0].images.len(), pages[0].images[0].ext), (1, "jpg"), "overlap {overlaps}");
        let jpeg = pages[0].images[0].bytes.clone();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
        let docx = d.export_office(compare::OfficeFormat::Docx);
        assert!(docx.windows(21).any(|w| w == b"word/media/image1.jpg"), "overlap {overlaps}");
        assert!(docx.windows(jpeg.len()).any(|w| w == &jpeg[..]), "the JPEG is stored unchanged (overlap {overlaps})");
    }
}

/// Cropping an existing page image (Acrobat's crop handles on a picture) shows only the chosen
/// box, the pixels staying where they are: `visible_rect` says what shows, the full placement is
/// unchanged, and a later move carries the crop along.
#[test]
fn cropping_an_existing_image_clips_it_and_moves_with_it() {
    let mut s = Session::new();
    let mut px = image::RgbaImage::new(40, 40);
    for (x, y, p) in px.enumerate_pixels_mut() {
        *p = if x < 20 {
            image::Rgba([255, 0, 0, 255])
        } else if y < 20 {
            image::Rgba([0, 255, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        };
    }
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(px).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let pic = s.create_from_images(&[("p.png".into(), png.into_inner())]).unwrap();
    let id = s.open("p.pdf", None, pic, None).unwrap();
    let full = s.get(id).unwrap().page_images(0)[0].rect;
    let (w, h) = (full[2] - full[0], full[3] - full[1]);
    // Keep the right half, upper part.
    let keep = [full[0] + w / 2.0, full[1] + h / 2.0, full[2], full[3]];
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Crop(keep) }).unwrap();
    let d = s.get(id).unwrap();
    assert_eq!(d.can_undo(), Some("Crop image"));
    let im = &d.page_images(0)[0];
    assert_eq!(im.rect, full, "the placement is not scaled");
    let v = im.visible_rect.expect("clipped");
    assert!(v.iter().zip(keep.iter()).all(|(a, b)| (a - b).abs() < 0.01), "{v:?} vs {keep:?}");
    // What shows: the kept quarter is green, the cropped-away quarters are paper.
    let (pw, ph, rgba) = {
        let mut ex = crate::export::Exporter::new(d);
        let png = ex.png(0, 72.0).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        (img.width(), img.height(), img)
    };
    let at = |fx: f64, fy: f64| rgba.get_pixel((pw as f64 * fx) as u32, (ph as f64 * fy) as u32).0;
    let page_h = d.info.pages[0].height as f64;
    let page_w = d.info.pages[0].width as f64;
    // Fractions of the page from the top-left: the kept box is the top-right quarter of the image.
    let fx = |x: f64| x / page_w;
    let fy = |y: f64| 1.0 - y / page_h;
    assert_eq!(at(fx(full[0] + 0.75 * w), fy(full[1] + 0.75 * h))[1], 255, "kept green");
    assert_eq!(at(fx(full[0] + 0.25 * w), fy(full[1] + 0.25 * h)), [255, 255, 255, 255], "cropped-away red is gone");
    // A move takes the crop along (down and left, so the kept part stays on the page).
    let to = [full[0] - 10.0, full[1] - 10.0, full[2] - 10.0, full[3] - 10.0];
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move(to) }).unwrap();
    let im = &s.get(id).unwrap().page_images(0)[0];
    let v = im.visible_rect.expect("still clipped");
    let moved_keep = [keep[0] - 10.0, keep[1] - 10.0, keep[2] - 10.0, keep[3] - 10.0];
    assert!(v.iter().zip(moved_keep.iter()).all(|(a, b)| (a - b).abs() < 0.01), "{v:?} vs {moved_keep:?}");
    // Cropping again replaces the earlier crop, so the picture can grow back (Remove crop).
    let full_now = s.get(id).unwrap().page_images(0)[0].rect;
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Crop(full_now) }).unwrap();
    let im = &s.get(id).unwrap().page_images(0)[0];
    let v = im.visible_rect.expect("visible");
    let page = [0.0, 0.0, page_w, page_h];
    let shown = [full_now[0].max(page[0]), full_now[1].max(page[1]), full_now[2].min(page[2]), full_now[3].min(page[3])];
    assert!(v.iter().zip(shown.iter()).all(|(a, b)| (a - b).abs() < 0.01), "{v:?} vs {shown:?}");
    // No area, or a box off the image, is refused.
    assert!(s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Crop([10.0, 10.0, 10.0, 50.0]) }).is_err());
}

#[test]
fn a_phone_photo_used_as_a_custom_stamp_is_embedded_upright() {
    let mut session = Session::new();
    let blank = session.create_blank(300.0, 400.0, 1).unwrap();
    let id = session.open_new("form.pdf", blank).unwrap();
    // A click places the stamp at its natural size: 30 x 20 pt as stored, 20 x 30 pt upright.
    // Named "Signature" so `signature_image` finds its picture.
    let file = MarkFile { name: "photo.jpg".into(), bytes: Arc::new(phone_jpeg()), page: 0 };
    let stamp = Edit::AddCustomStamp { page: 0, rect: [100.0, 100.0, 100.0, 100.0], name: "Signature".into(), file, author: "Ada".into() };
    session.apply(id, stamp).unwrap();
    let cos = &session.get(id).unwrap().editor.as_ref().unwrap().cos;
    let picture = pdfcraft_annot::signature_image(cos, 0, 0).unwrap().unwrap();
    let obj = cos.get(picture);
    let pdfcraft_cos::Object::Stream(s) = &*obj else { panic!() };
    assert_eq!((s.dict.int(b"Width"), s.dict.int(b"Height")), (Some(20), Some(60)));
    let rect = pdfcraft_annot::summaries(cos)[0].rect;
    assert!(((rect[2] - rect[0]) - 20.0).abs() < 0.01 && ((rect[3] - rect[1]) - 30.0).abs() < 0.01, "{rect:?}");
}

/// Opening a paragraph and applying its edit read the page's text blocks; after an edit they are
/// still read once per document state, and undo and redo never get another state's blocks.
#[test]
fn text_blocks_stay_cached_after_an_edit_and_follow_undo_and_redo() {
    let (mut session, id) = session_with(1);
    let blocks = |s: &Session| s.get(id).unwrap().editor.as_ref().unwrap().text_blocks(0).unwrap();
    let before = blocks(&session);
    assert_eq!(before[0].editor_text, "Page 1");
    let edit = Edit::EditTextBlock { page: 0, block: 0, text: "Edited".into(), style: pdfcraft_edit::BlockStyle::default() };
    session.apply(id, edit).unwrap();
    let edited = blocks(&session);
    assert_eq!(edited[0].editor_text, "Edited");
    assert!(Arc::ptr_eq(&edited, &blocks(&session)), "the edited page is grouped once");
    session.undo(id).unwrap();
    assert_eq!(blocks(&session)[0].editor_text, "Page 1");
    session.redo(id).unwrap();
    let redone = blocks(&session);
    assert_eq!(redone[0].editor_text, "Edited");
    assert!(Arc::ptr_eq(&redone, &blocks(&session)));
}

/// #4199, #3797: the spaces typed into a paragraph are all there after saving and reopening.
#[test]
fn edited_text_keeps_every_space_typed_through_save_and_reopen() {
    let (mut s, id) = session_with(1);
    let typed = "  A   b  c";
    s.apply(id, Edit::EditTextBlock { page: 0, block: 0, text: typed.into(), style: pdfcraft_edit::BlockStyle::default() }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let reopened = s.open("saved.pdf", None, saved, None).unwrap();
    let blocks = s.get(reopened).unwrap().editor.as_ref().unwrap().text_blocks(0).unwrap();
    assert_eq!(blocks[0].editor_text, typed);
}

/// A new document, or one restored from recovery, has never been written where it belongs: undo
/// back to the bytes it was opened from must not make it clean, or closing it would discard the
/// whole document without asking. Saving makes it clean; Revert of a saved one too.
#[test]
fn undo_never_makes_an_unsaved_document_clean() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let new = s.open_new("Combined.pdf", Arc::new(fixture(2))).unwrap();
    s.apply(new, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    s.undo(new).unwrap();
    assert!(s.get(new).unwrap().dirty, "a new document is unsaved until it is saved");
    assert_eq!(s.autosave_snapshots().len(), 1, "and still needs recovery");
    s.revert(new).unwrap();
    assert!(s.get(new).unwrap().dirty, "Revert goes back to the created bytes, which aren't saved anywhere");

    let recovered = s.open("Report.pdf", None, Arc::new(fixture(2)), None).unwrap();
    s.mark_recovered(recovered, Some("Report.pdf".into()));
    s.apply(recovered, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    s.undo(recovered).unwrap();
    assert!(s.get(recovered).unwrap().dirty, "the recovered work was never saved");

    let saved = s.save_bytes(new).unwrap();
    s.mark_saved(new, saved, Some("Combined.pdf".into())).unwrap();
    assert!(!s.get(new).unwrap().dirty);
    s.apply(new, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    s.undo(new).unwrap();
    assert!(!s.get(new).unwrap().dirty, "once saved, undo back to the saved state is clean");
}

#[test]
fn undo_and_redo_roll_back_on_a_refresh_panic_or_error() {
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let (bytes_90, gen_90) = {
        let d = s.get(id).unwrap();
        (d.bytes.clone(), d.generation)
    };

    // 1. A panic during undo's refresh catches the unwind and rolls back cos, undo/redo, info, and working bytes.
    panic_on_refresh(1);
    let err = s.undo(id).unwrap_err();
    assert_eq!(err, EditError::Reopen("injected refresh panic".into()));
    {
        let d = s.get(id).unwrap();
        let ed = d.editor.as_ref().unwrap();
        assert_eq!(d.info.pages[0].rotation, 90);
        assert_eq!(d.bytes, bytes_90);
        assert_eq!(d.generation, gen_90);
        assert!(d.dirty);
        assert!(ed.cos.is_modified());
        assert_eq!(ed.undo.len(), 1);
        assert!(ed.redo.is_empty());
    }
    let _ = shown(&s, id, 0);

    // 2. If the rollback refresh also panics, `show_released` restores the renderer so the page still draws.
    panic_on_refresh(2);
    assert_eq!(s.undo(id), Err(EditError::Reopen("injected refresh panic".into())));
    assert_eq!(s.get(id).unwrap().editor.as_ref().unwrap().undo.len(), 1);
    let _ = shown(&s, id, 0);

    // Undo now succeeds cleanly.
    assert_eq!(s.undo(id).unwrap().as_str(), "Rotate page");
    let (bytes_0, gen_0) = {
        let d = s.get(id).unwrap();
        assert_eq!(d.info.pages[0].rotation, 0);
        assert!(!d.dirty);
        (d.bytes.clone(), d.generation)
    };

    // 3. A panic during redo's refresh catches the unwind and rolls back to the undone state.
    panic_on_refresh(1);
    let err = s.redo(id).unwrap_err();
    assert_eq!(err, EditError::Reopen("injected refresh panic".into()));
    {
        let d = s.get(id).unwrap();
        let ed = d.editor.as_ref().unwrap();
        assert_eq!(d.info.pages[0].rotation, 0);
        assert_eq!(d.bytes, bytes_0);
        assert_eq!(d.generation, gen_0);
        assert!(!d.dirty);
        assert!(!ed.cos.is_modified());
        assert!(ed.undo.is_empty());
        assert_eq!(ed.redo.len(), 1);
    }
    let _ = shown(&s, id, 0);

    // 4. If redo's rollback refresh also panics, `show_released` restores the renderer.
    panic_on_refresh(2);
    assert_eq!(s.redo(id), Err(EditError::Reopen("injected refresh panic".into())));
    assert_eq!(s.get(id).unwrap().editor.as_ref().unwrap().redo.len(), 1);
    let _ = shown(&s, id, 0);

    // Redo now succeeds cleanly.
    assert_eq!(s.redo(id).unwrap().as_str(), "Rotate page");
    assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 90);
    rolls_back_metadata_scope(&mut s, id);
    rolls_back_protection_keys(&mut s, id);
}

#[test]
fn a_bookmark_added_at_a_point_goes_there_through_undo_and_save() {
    use pdfcraft_render::DestView;
    let (mut s, id) = session_with(3);
    let view = |s: &Session, i: usize| s.get(id).unwrap().info.outline.get(i).map(|o| (o.page, o.view));
    let add = |s: &mut Session, title: &str, page: usize, left: Option<f64>, top: Option<f64>| {
        s.apply(id, Edit::AddBookmark { parent: vec![], index: usize::MAX, title: title.into(), page, left, top })
    };
    add(&mut s, "Plain", 0, None, None).unwrap();
    add(&mut s, "Section", 1, None, Some(500.5)).unwrap();
    add(&mut s, "Table", 2, Some(72.0), Some(300.0)).unwrap();
    // The page top (no position) is what it always was; the others carry their place.
    assert_eq!(view(&s, 0), Some((Some(0), DestView::Xyz { left: None, top: None, zoom: None })));
    assert_eq!(view(&s, 1), Some((Some(1), DestView::Xyz { left: None, top: Some(500.5), zoom: None })));
    assert_eq!(view(&s, 2), Some((Some(2), DestView::Xyz { left: Some(72.0), top: Some(300.0), zoom: None })));

    // A position that is not a number fails the edit, which then changes nothing.
    for (left, top) in [(Some(f64::NAN), None), (None, Some(f64::INFINITY))] {
        assert!(matches!(add(&mut s, "Bad", 0, left, top), Err(EditError::Bookmark(_))), "{left:?} {top:?}");
    }
    assert_eq!(s.get(id).unwrap().info.outline.len(), 3);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Add bookmark"));

    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().info.outline.len(), 2);
    s.redo(id).unwrap();
    let saved = s.save_bytes(id).unwrap();
    let mut again = Session::new();
    let id2 = again.open("again.pdf", None, saved, None).unwrap();
    let outline = &again.get(id2).unwrap().info.outline;
    assert_eq!(outline[2].view, DestView::Xyz { left: Some(72.0), top: Some(300.0), zoom: None });
    assert_eq!(outline[1].view, DestView::Xyz { left: None, top: Some(500.5), zoom: None });
}

/// #3470: a filled form whose writer left the appearances to the viewer (`/NeedAppearances`):
/// field A has an empty appearance, B none. The page as displayed (and printed) shows both
/// values, and a comment without an appearance beside them still gets its own; the file itself
/// is not changed, so saving writes it as it was.
#[test]
fn fields_left_to_the_viewer_are_displayed_from_their_values() {
    let pdf = |need: bool| {
        let flag = if need { "/NeedAppearances true " } else { "" };
        format!(
            "%PDF-1.7\n\
             1 0 obj << /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R >> endobj\n\
             2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
             3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Annots [6 0 R 7 0 R 10 0 R] >> endobj\n\
             10 0 obj << /Type /Annot /Subtype /Square /Rect [360 20 390 50] /C [1 0 0] /BS << /W 4 >> >> endobj\n\
             5 0 obj << /Fields [6 0 R 7 0 R] {flag}/DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >> endobj\n\
             6 0 obj << /Type /Annot /Subtype /Widget /FT /Tx /T (a) /V (MMMMMMMM) /DA (/Helv 12 Tf 0 g) /Rect [50 120 350 140] /F 4 /P 3 0 R /AP << /N 9 0 R >> >> endobj\n\
             7 0 obj << /Type /Annot /Subtype /Widget /FT /Tx /T (b) /V (MMMMMMMM) /DA (/Helv 12 Tf 0 g) /Rect [50 60 350 80] /F 4 /P 3 0 R >> endobj\n\
             8 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> endobj\n\
             9 0 obj << /Type /XObject /Subtype /Form /BBox [0 0 300 20] /Length 4 >> stream\nq\nQ\nendstream endobj\n\
             trailer << /Root 1 0 R >>\n%%EOF"
        )
        .into_bytes()
    };
    let red = |s: &Session, id: DocId| {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.display.clone(), doc.config.clone());
        let page = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        page.rgba.as_chunks::<4>().0.iter().filter(|p| p[0] > 200 && p[1] < 80 && p[2] < 80).count()
    };
    // Dark pixels inside a field's box (user-space rectangle, page drawn at scale 1).
    let ink = |s: &Session, id: DocId, y0: u32, y1: u32| {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.display.clone(), doc.config.clone());
        let page = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        let (px, _) = page.rgba.as_chunks::<4>();
        (200 - y1..200 - y0)
            .flat_map(|y| (55..345).map(move |x| (x, y)))
            .filter(|&(x, y)| px.get((y * page.width + x) as usize).is_some_and(|p| p[0] < 128))
            .count()
    };
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(pdf(true)), None).unwrap();
    let (a, b) = (ink(&s, id, 122, 138), ink(&s, id, 62, 78));
    assert!(a > 100 && b > 100, "both values are shown: a {a}, b {b}");
    assert!(red(&s, id) > 100, "the comment is drawn too");
    // The file is saved as it was: B still has no appearance, A its empty one.
    let saved = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    let fields = pdfcraft_forms::fields(&saved);
    let widget = |name: &str| saved.get(fields.iter().find(|f| f.name == name).unwrap().widgets[0].obj);
    assert!(widget("b").as_dict().is_some_and(|d| !d.contains(b"AP")), "B has no appearance");
    let n = widget("a").as_dict().and_then(|d| d.get(b"AP").and_then(|ap| saved.resolve(ap).as_dict().and_then(|ap| ap.reference(b"N")))).unwrap();
    let pdfcraft_cos::Object::Stream(n) = &*saved.get(n) else { panic!("A's appearance is a stream") };
    assert_eq!(n.decoded().unwrap().trim_ascii_end(), b"q\nQ", "A keeps its empty appearance");
    // Typing in one field redraws that field only, not the whole form, on each keystroke.
    let builds = |s: &Session| s.get(id).unwrap().editor.as_ref().unwrap().field_appearances.builds();
    let before = builds(&s);
    s.apply(id, Edit::SetFieldValue { name: "a".into(), value: pdfcraft_forms::FieldValue::Text("Typed".into()) }).unwrap();
    assert_eq!(builds(&s), before + 1);

    // Without the flag, the empty appearance is the file's own, and only B is drawn.
    let id = s.open("form.pdf", None, Arc::new(pdf(false)), None).unwrap();
    let (a, b) = (ink(&s, id, 122, 138), ink(&s, id, 62, 78));
    assert!(a == 0 && b > 100, "a {a}, b {b}");
}

#[test]
#[ignore = "manual performance measurement: 3,129 fields, 190 scripts"]
fn acroform_interaction_benchmark() {
    let bytes = Arc::new(synthetic_acroform(3129, 190));
    // Optional local export for comparing CLI build profiles. Never overwrite an input.
    if let Some(path) = std::env::var_os("PDFCRAFT_BENCH_EXPORT") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
        file.write_all(&bytes).unwrap();
    }
    let mut s = Session::new();
    let id = s.open("synthetic-large-form.pdf", None, bytes, None).unwrap();
    let value = |s: &Session, name: &str| s.get(id).unwrap().form.iter().find(|f| f.name == name).unwrap().value.clone();
    for (name, v) in [
        ("ranks", FieldValue::Text("1".into())),
        ("trained", FieldValue::Check(true)),
        ("ranks", FieldValue::Text("2".into())),
        ("trained", FieldValue::Check(false)),
        ("ranks", FieldValue::Text("0".into())),
    ] {
        let started = std::time::Instant::now();
        s.apply(id, Edit::SetFieldValue { name: name.into(), value: v }).unwrap();
        eprintln!("BENCH {name}: {:.3} ms", started.elapsed().as_secs_f64() * 1000.0);
        assert!(s.take_js_output(id).errors.is_empty());
    }
    assert_eq!(value(&s, "bonus"), ["0"]);
    assert_eq!(value(&s, "field191"), ["189"]);
    assert_eq!(value(&s, "field3128"), ["0"]);
}

#[test]
fn checkbox_and_rank_changes_recalculate_ordered_dependents() {
    let mut s = Session::new();
    let id = s.open("skills.pdf", None, Arc::new(synthetic_acroform(8, 4)), None).unwrap();
    for (name, input, bonus) in [
        ("trained", FieldValue::Check(true), "0"),
        ("ranks", FieldValue::Text("1".into()), "4"),
        ("trained", FieldValue::Check(false), "1"),
        ("ranks", FieldValue::Text("2".into()), "2"),
        ("trained", FieldValue::Check(true), "5"),
        ("ranks", FieldValue::Text("0".into()), "0"),
    ] {
        s.apply(id, Edit::SetFieldValue { name: name.into(), value: input }).unwrap();
        let fields = &s.get(id).unwrap().form;
        let value = |name: &str| fields.iter().find(|f| f.name == name).unwrap().value.clone();
        assert_eq!(value("bonus"), [bonus]);
        assert_eq!(value("field5"), [(bonus.parse::<u32>().unwrap() + 3).to_string()]);
        assert_eq!(value("field7"), ["0"]);
        assert!(s.take_js_output(id).errors.is_empty());
    }
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().form.iter().find(|f| f.name == "bonus").unwrap().value, ["5"]);
    let saved = s.save_bytes(id).unwrap();
    let reopened = s.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(s.get(reopened).unwrap().form.iter().find(|f| f.name == "bonus").unwrap().value, ["5"]);
    let appearance = |s: &Session| {
        let doc = s.get(id).unwrap();
        let widget = doc.form.iter().find(|f| f.name == "bonus").unwrap().widgets.first().unwrap().obj;
        doc.editor.as_ref().unwrap().cos.get(widget).as_dict().unwrap().get(b"AP").cloned()
    };
    let before = appearance(&s);
    assert!(before.is_some());
    s.apply(id, Edit::SetFieldValue { name: "field7".into(), value: FieldValue::Text("9".into()) }).unwrap();
    assert_eq!(appearance(&s), before, "unchanged calculated values keep their appearance streams");
}

#[test]
fn dynamic_calculation_reads_and_side_effects_reach_later_fields() {
    use pdfcraft_cos::{Dict, Object, PdfString};
    let mut cos = pdfcraft_cos::Document::open(Arc::new(synthetic_acroform(8, 4))).unwrap();
    let fields = pdfcraft_forms::fields(&cos);
    let bonus = fields.iter().find(|f| f.name == "bonus").unwrap();
    let selector = fields.iter().find(|f| f.name == "field7").unwrap();
    cos.update_dict(selector.obj, |d| d.set(b"V".to_vec(), PdfString::text("ranks"))).unwrap();
    let mut a = Dict::new();
    a.set(b"S".to_vec(), Object::name("JavaScript"));
    a.set(b"JS".to_vec(), PdfString::text(
        "var key = getField('field7').value; var v = Number(getField(key).value); getField('field6').value = v * 2; getField('field6').readonly = true; event.value = v + 1;"
    ));
    let mut aa = Dict::new();
    aa.set(b"C".to_vec(), Object::Dict(a));
    cos.update_dict(bonus.obj, |d| d.set(b"AA".to_vec(), Object::Dict(aa))).unwrap();
    let bytes = pdfcraft_cos::write_incremental(&cos, &Default::default()).unwrap();
    let mut s = Session::new();
    let id = s.open("dynamic.pdf", None, Arc::new(bytes), None).unwrap();
    s.apply(id, Edit::SetFieldValue { name: "ranks".into(), value: FieldValue::Text("4".into()) }).unwrap();
    let fields = &s.get(id).unwrap().form;
    let field = |name: &str| fields.iter().find(|f| f.name == name).unwrap();
    assert_eq!(field("bonus").value, ["5"]);
    assert_eq!(field("field5").value, ["8"], "later calculations see the ordered result");
    assert_eq!(field("field6").value, ["8"], "side-effect value applied");
    assert!(field("field6").read_only(), "side-effect property applied");
    assert_eq!(field("field7").value, ["ranks"]);
    assert!(s.take_js_output(id).errors.is_empty());
}

#[test]
fn scripted_display_changes_survive_warm_cache_and_new_edit() {
    use pdfcraft_forms::{FieldEvent, Scripts};
    let mut cos = pdfcraft_cos::Document::open(Arc::new(synthetic_acroform(2, 0))).unwrap();
    for (display, flags) in [(1, 6), (3, 36), (2, 0), (0, 4)] {
        let fields = pdfcraft_forms::fields(&cos);
        let target = fields.iter().find(|f| f.name == "ranks").unwrap();
        let other = fields.iter().find(|f| f.name == "trained").unwrap();
        let old_display = crate::js::field_state(&cos, other).display;
        let mut runner = crate::js::JsRunner::new(&cos, "synthetic.pdf");
        // Populate the display cache before a script writes the same field.
        let warm = runner.run(FieldEvent::Calculate, "event.value = getField('trained').display;", target, "", &fields);
        assert_eq!(warm.value, old_display.to_string());
        let result = runner.run(
            FieldEvent::Calculate,
            &format!("getField('trained').display = {display}; event.value = getField('trained').display;"),
            target,
            "",
            &fields,
        );
        assert_eq!(result.value, display.to_string());
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes.first().unwrap().display, Some(display));
        pdfcraft_forms::apply_script_changes(&mut cos, &result.changes, &mut pdfcraft_forms::NoScripts).unwrap();
        assert_eq!(cos.get(other.widgets.first().unwrap().obj).as_dict().unwrap().int(b"F").unwrap() & 38, flags);
        // Within an edit the original implementation reads an immutable pre-edit COS snapshot.
        let same_edit = runner.run(FieldEvent::Calculate, "event.value = getField('trained').display;", target, "", &fields);
        assert_eq!(same_edit.value, old_display.to_string());
        // The next edit must rebuild its cache from the newly written annotation flags.
        let mut next = crate::js::JsRunner::new(&cos, "synthetic.pdf");
        let read = next.run(FieldEvent::Calculate, "event.value = getField('trained').display;", target, "", &pdfcraft_forms::fields(&cos));
        assert_eq!(read.value, display.to_string());
        assert!(read.changes.is_empty());
        assert!(runner.output.errors.is_empty());
        assert!(next.output.errors.is_empty());
    }
}

/// #2992: a page coloured with Edit a PDF ▸ Background exports to Word with that page colour,
/// behind its text.
#[test]
fn exporting_to_word_keeps_the_page_background() {
    let mut s = Session::new();
    let text = s.create_from_text("t", "Blue text on teal").unwrap();
    let id = s.open("t.pdf", None, text, None).unwrap();
    assert_eq!(s.get(id).unwrap().export_pages()[0].background, None, "no background yet");
    s.apply(
        id,
        Edit::AddBackground {
            pages: vec![0],
            settings: Background { color: [0.0, 0.5, 0.5], opacity: 1.0, ..Background::default() },
            replace: false,
            file: None,
        },
    )
    .unwrap();
    let d = s.get(id).unwrap();
    let page = &d.export_pages()[0];
    assert!(page.background.is_some_and(|c| c.iter().zip([0.0, 0.5, 0.5]).all(|(a, b)| (a - b).abs() < 1e-6)), "{:?}", page.background);
    assert!(page.blocks.iter().any(|b| b.text.contains("Blue text")), "the text is still there");
    let docx = d.export_office(compare::OfficeFormat::Docx);
    assert!(docx.windows(b"word/settings.xml".len()).any(|w| w == b"word/settings.xml"), "Word is told to show the page colour");
}

#[test]
fn exporting_office_keeps_dotted_leader_entries_as_paragraphs() {
    let mut content = String::new();
    for (title, page, y) in [("Art.1. Scope", "3", 200), ("Art.2. Eligibility", "7", 180), ("Art.3. Application", "12", 160)] {
        content.push_str(&format!(
            "BT /F1 12 Tf 72 {y} Td ({title}) Tj ET \
             BT /F1 12 Tf 250 {y} Td (....................) Tj ET \
             BT /F1 12 Tf 500 {y} Td ({page}) Tj ET\n"
        ));
    }
    let pdf = one_line_page("/Font << /F1 4 0 R >>", &content);
    let mut session = Session::new();
    let id = session.open("contents.pdf", None, Arc::new(pdf), None).unwrap();
    let doc = session.get(id).unwrap();
    let html = String::from_utf8(doc.export_office(compare::OfficeFormat::Html)).unwrap();
    assert!(!html.contains("<table>"), "contents entries are paragraphs: {html}");
    let entries = ["Art.1. Scope .................... 3", "Art.2. Eligibility .................... 7", "Art.3. Application .................... 12"];
    let positions: Vec<_> = entries.iter().map(|entry| html.find(&format!("<p>{entry}</p>")).expect("one paragraph per entry")).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "reading order: {html}");
}

/// A date typed in another of an XFA date field's edit patterns is kept in its first: yymmdd and
/// yyyymd are rewritten as yyyymmdd, a date none of them reads is refused, and the data holds the date.
#[test]
fn a_date_typed_by_another_of_an_xfa_fields_edit_patterns_is_kept_in_its_first() {
    let mut s = Session::new();
    let id = s
        .open("dates.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::date_alternatives_template())), None)
        .expect("opens");
    let value = |s: &Session| s.get(id).unwrap().form.iter().find(|f| f.name == "leave").unwrap().value.clone();
    s.apply(id, Edit::SetFieldValue { name: "leave".into(), value: FieldValue::Text("261018".into()) }).expect("yymmdd is one of its patterns");
    assert_eq!(value(&s), ["20261018"]);
    s.apply(id, Edit::SetFieldValue { name: "leave".into(), value: FieldValue::Text("2026118".into()) }).expect("so is yyyymd");
    assert_eq!(value(&s), ["20261108"]);
    let refused = s.apply(id, Edit::SetFieldValue { name: "leave".into(), value: FieldValue::Text("2618".into()) });
    assert!(refused.is_err_and(|e| e.to_string().contains("yyyymmdd")), "none of its patterns reads it");
    assert_eq!(value(&s), ["20261108"]);
    // The data holds the date.
    let cos = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    let data = pdfcraft_xfa::parse_datasets(&pdfcraft_xfa::read_packets(&cos).unwrap().unwrap().xdp).expect("a datasets packet");
    assert_eq!(data.text_at(&pdfcraft_xfa::som_to_path("form[0].leave[0]")), Some("2026-11-08"));
}

/// A date the field's own scripts refuse is not tried again in another of an XFA date field's
/// patterns: only a date its format refuses is, which happens before any script runs, so the
/// scripts run once. This validate script counts its runs in `name` and accepts on its second.
#[test]
fn a_date_its_validate_script_refuses_is_not_tried_again() {
    let mut s = Session::new();
    let id = s
        .open("dates.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::date_alternatives_template())), None)
        .expect("opens");
    let script = Some("var n = Number(getField('name').value || 0) + 1; getField('name').value = String(n); event.rc = n >= 2;".to_string());
    s.apply(id, Edit::SetFieldScript { name: "leave".into(), event: "validate".into(), script }).unwrap();
    let value = |s: &Session, n: &str| s.get(id).unwrap().form.iter().find(|f| f.name == n).unwrap().value.clone();
    // Its format reads 18OCT26, and so does its DDMMMYY pattern.
    let refused = s.apply(id, Edit::SetFieldValue { name: "leave".into(), value: FieldValue::Text("18OCT26".into()) });
    assert!(refused.is_err(), "the validate script refused it, once");
    assert!(value(&s, "name").is_empty(), "nothing the refused run changed is kept: {:?}", value(&s, "name"));
}

/// An XFA button's click script may show the attachments panel; a text field's exit script
/// asking the same is noted on the console and ignored.
#[test]
fn an_xfa_button_may_show_a_side_panel_and_other_events_may_not() {
    let xdp = r##"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
<subform name="form" layout="tb">
 <pageSet><pageArea name="front"><contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/><medium short="8.5in" long="11in"/></pageArea></pageSet>
 <field name="files" w="2in" h="0.3in"><ui><button/></ui><caption><value><text>Attachments</text></value></caption>
  <event activity="click"><script contentType="application/x-javascript">app.execMenuItem("ShowHideFileAttachment");</script></event></field>
 <field name="note" w="2in" h="0.3in"><ui><textEdit/></ui>
  <event activity="exit"><script contentType="application/x-javascript">app.execMenuItem("ShowHideFileAttachment");</script></event></field>
</subform>
</template>
</xdp:xdp>
"##;
    let mut s = Session::new();
    let id = s.open("files.pdf", None, Arc::new(pdfcraft_xfa::fixtures::shell(xdp)), None).expect("opens");
    s.take_js_output(id);
    s.apply(id, Edit::XfaEvent { som: "form[0].files[0]".into(), activity: "click".into() }).unwrap();
    let out = s.take_js_output(id);
    assert_eq!(out.requests, [pdfcraft_js::Request::ShowPanel(pdfcraft_js::Panel::Attachments)], "{out:?}");
    s.apply(id, Edit::SetFieldValue { name: "note".into(), value: FieldValue::Text("x".into()) }).unwrap();
    let out = s.take_js_output(id);
    assert!(out.requests.is_empty(), "only a click shows a panel: {out:?}");
    assert!(out.console.iter().any(|c| c.contains("show a side panel")), "{:?}", out.console);
}

/// A validate script that asks for Save As and a side panel gets neither, with a console note
/// for each; a button's script may still open Save As.
#[test]
fn a_fields_own_script_may_not_save_as_or_show_a_panel() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("order.pdf", None, Arc::new(scripted_form()), None).unwrap();
    s.apply(
        id,
        Edit::SetFieldScript {
            name: "qty".into(),
            event: "validate".into(),
            script: Some("app.execMenuItem('SaveAs'); app.execMenuItem('ShowHideFileAttachment');".into()),
        },
    )
    .unwrap();
    s.take_js_output(id);
    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("4".into()) }).unwrap();
    let out = s.take_js_output(id);
    assert!(out.requests.is_empty(), "a validate script runs unasked: {out:?}");
    assert_eq!(out.console.iter().filter(|c| c.contains("only a button may")).count(), 2, "{:?}", out.console);
    // A button's script may.
    let o = s.run_javascript(id, "app.execMenuItem('SaveAs');", Some("go")).unwrap();
    assert_eq!(o.requests, [js::Request::SaveAs]);
}

/// The allowance is the export's, not each picture's: a page of many small pictures stops where it
/// runs out, the rest are left out and counted, and the page is listed, which is what the export's
/// warning goes by. A few large pictures are well within it, and all come through.
#[test]
fn a_page_of_many_pictures_stops_at_the_exports_allowance() {
    let export = |pdf: Vec<u8>, work: &mut pdfcraft_cos::ImageBudget| {
        let mut s = Session::new();
        let id = s.open("pictures.pdf", None, Arc::new(pdf), None).unwrap();
        s.get(id).unwrap().export_pages_within(true, 1 << 30, work)
    };
    let mut one = pdfcraft_cos::ImageBudget::with_work(u64::MAX);
    export(page_of_pictures(1, 2), &mut one);
    let cost = u64::MAX - one.work_left();
    assert!(cost > 0);

    let mut work = pdfcraft_cos::ImageBudget::with_work(100 * cost);
    let (pages, truncated) = export(page_of_pictures(10_000, 2), &mut work);
    assert_eq!(pages[0].images.len(), 100);
    assert_eq!(truncated, vec![0]);
    assert_eq!(work.refused(), 9_900);

    let mut s = Session::new();
    let id = s.open("large.pdf", None, Arc::new(page_of_pictures(3, 1_000)), None).unwrap();
    let (pages, truncated) = s.get(id).unwrap().export_pages_checked(true, 1 << 30);
    assert_eq!(pages[0].images.len(), 3);
    assert_eq!(truncated, Vec::<usize>::new());
}

/// A page whose forms go past the reading limit and whose picture goes past the image budget is
/// listed once among the truncated pages.
#[test]
fn a_page_truncated_for_forms_and_pictures_is_listed_once() {
    let depth = compare::MAX_FORM_DEPTH + 1;
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 200] >>".into(),
        String::new(),
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n\0\x20\x40\x7f\nendstream".into(),
        "<< /Length 34 >>\nstream\nq 10 0 0 10 0 0 cm /Im Do Q /D1 Do\nendstream".into(),
    ];
    // Forms D1..D{depth}, objects 6 onwards, each drawing the next.
    let xobjects: String = (1..=depth).map(|i| format!("/D{i} {} 0 R ", 5 + i)).collect();
    let resources = format!("<< /XObject << /Im 4 0 R {xobjects}>> >>");
    for i in 1..=depth {
        let body = if i == depth { "q Q".to_string() } else { format!("/D{} Do", i + 1) };
        objs.push(format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources {resources} /Length {} >>\nstream\n{body}\nendstream",
            body.len()
        ));
    }
    objs[2] = format!("<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources {resources} >>");
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
    let id = s.open("both.pdf", None, Arc::new(bytes), None).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.export_pages_checked(false, 0).1, [0], "the forms alone truncate the page");
    let (exported, _) = doc.export_pages_checked(true, 1 << 20);
    assert_eq!(exported[0].images.len(), 1, "the picture exports within a sufficient budget");
    let (pages, truncated) = doc.export_pages_checked(true, 0);
    assert!(pages[0].images.is_empty(), "the picture is past the budget");
    assert_eq!(truncated, [0]);
}

/// Moving an added image, then rotating it, must keep the moved position.
#[test]
fn added_image_move_then_rotate_keeps_position() {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
    }
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([20.0, 20.0, 60.0, 60.0]), name: "p.png".into(), bytes: Arc::new(png) }).unwrap();
    let rect = |s: &Session| s.get(id).unwrap().page_images(0)[0].rect;
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move([120.0, 120.0, 160.0, 160.0]) }).unwrap();
    let moved = rect(&s);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Rotate(1) }).unwrap();
    let rotated = rect(&s);
    let c = |r: [f64; 4]| ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (a, b) = (c(moved), c(rotated));
    assert!((a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0, "rotate moved the centre from {a:?} to {b:?}");
}

/// Transforming an added image, then rotating it, must keep the moved position.
#[test]
fn added_image_transform_then_rotate_keeps_position() {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
    }
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([20.0, 20.0, 60.0, 60.0]), name: "p.png".into(), bytes: Arc::new(png) }).unwrap();
    let rect = |s: &Session| s.get(id).unwrap().page_images(0)[0].rect;
    // Translate by (100, 100): matrix [1, 0, 0, 1, 100, 100]
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([1.0, 0.0, 0.0, 1.0, 100.0, 100.0]) }).unwrap();
    let transformed = rect(&s);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Rotate(1) }).unwrap();
    let rotated = rect(&s);
    let c = |r: [f64; 4]| ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (a, b) = (c(transformed), c(rotated));
    assert!((a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0, "rotate moved the centre from {a:?} to {b:?}");
}

/// Moving an added image, then rotating by degrees, must keep the moved position.
#[test]
fn added_image_move_then_rotate_by_keeps_position() {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
    }
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([20.0, 20.0, 60.0, 60.0]), name: "p.png".into(), bytes: Arc::new(png) }).unwrap();
    let rect = |s: &Session| s.get(id).unwrap().page_images(0)[0].rect;
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move([120.0, 120.0, 160.0, 160.0]) }).unwrap();
    let moved = rect(&s);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::RotateBy(90.0) }).unwrap();
    let rotated = rect(&s);
    let c = |r: [f64; 4]| ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (a, b) = (c(moved), c(rotated));
    assert!((a.0 - b.0).abs() < 1.0 && (a.1 - b.1).abs() < 1.0, "rotate_by moved the centre from {a:?} to {b:?}");
}

/// Arranging an added image moves it into the document stream and converts it to an ordinary
/// page image (dropping the PCAdded marker), so no empty shell remains in list_added.
#[test]
fn added_image_arrange_converts_to_ordinary_image() {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
    }
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([20.0, 20.0, 60.0, 60.0]), name: "p1.png".into(), bytes: Arc::new(png.clone()) }).unwrap();
    s.apply(id, Edit::AddImage { page: 0, rect: Some([100.0, 100.0, 200.0, 200.0]), name: "p2.png".into(), bytes: Arc::new(png) }).unwrap();
    assert_eq!(s.get(id).unwrap().page_images(0).len(), 2);
    assert_eq!(s.get(id).unwrap().added().len(), 2);
    s.apply(id, Edit::EditPageImage { page: 0, index: 1, change: ImageEdit::Arrange(ArrangeOrder::SendToBack) }).unwrap();
    assert_eq!(s.get(id).unwrap().page_images(0).len(), 2);
    assert_eq!(s.get(id).unwrap().added().len(), 1, "arranged image is converted and no longer an added-item shell");
}

#[test]
fn added_image_crop_then_move_keeps_crop() {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
    }
    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([20.0, 20.0, 100.0, 100.0]), name: "p.png".into(), bytes: Arc::new(png) }).unwrap();
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Crop([30.0, 30.0, 70.0, 70.0]) }).unwrap();
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move([50.0, 50.0, 130.0, 130.0]) }).unwrap();
    let img = s.get(id).unwrap().page_images(0)[0].clone();
    assert_eq!(img.visible_rect, Some([60.0, 60.0, 100.0, 100.0]));
}

#[test]
fn added_image_move_and_transform_on_rotated_and_user_unit_pages() {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
    }
    let png_bytes = Arc::new(png);

    let cases = [(90, 1.0), (180, 1.0), (270, 1.0), (0, 2.0), (90, 2.0)];

    for (rotate, user_unit) in cases {
        let pdf = fixture_with_rotation_and_user_unit(rotate, user_unit);
        let mut s = Session::new().with_clock(|| 1_700_000_000);
        let id = s.open("test.pdf", None, Arc::new(pdf), None).expect("opens");

        s.apply(id, Edit::AddImage { page: 0, rect: Some([10.0, 10.0, 50.0, 50.0]), name: "img.png".into(), bytes: png_bytes.clone() }).unwrap();

        // Target user-space rectangle R
        let r = [30.0, 40.0, 80.0, 90.0];
        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move(r) }).unwrap();

        let painted = s.get(id).unwrap().page_images(0)[0].rect;
        for i in 0..4 {
            assert!(
                (painted[i] - r[i]).abs() < 1e-3,
                "Move: rotate={rotate}, user_unit={user_unit}: painted[{i}] = {}, expected {}",
                painted[i],
                r[i]
            );
        }

        let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
        let added = pdfcraft_edit::list_added(&cos);
        assert_eq!(added.len(), 1);
        let stream_obj = match &*cos.get(added[0].obj) {
            pdfcraft_cos::Object::Stream(s) => s.clone(),
            other => panic!("expected stream, got {other:?}"),
        };
        let pc_rect = stream_obj
            .dict
            .get(b"PCAdded")
            .and_then(|o| cos.resolve(o).as_dict().cloned())
            .and_then(|d| d.get(b"Rect").cloned())
            .and_then(|o| cos.resolve(&o).as_array().cloned())
            .unwrap();
        let pc_box: [f64; 4] =
            [pc_rect[0].as_f64().unwrap(), pc_rect[1].as_f64().unwrap(), pc_rect[2].as_f64().unwrap(), pc_rect[3].as_f64().unwrap()];
        let content = String::from_utf8(stream_obj.decoded().unwrap()).unwrap();
        let re_idx = content.find(" re W n").expect("re W n in content stream");
        let prefix = &content[..re_idx];
        let nums: Vec<f64> =
            prefix.split_whitespace().rev().take(4).collect::<Vec<_>>().into_iter().rev().map(|val| val.parse::<f64>().unwrap()).collect();
        assert_eq!(nums.len(), 4);
        let (rx, ry, rw, rh) = (nums[0], nums[1], nums[2], nums[3]);
        assert!((rx - pc_box[0]).abs() < 1e-3, "re x {rx} != pc_box[0] {}", pc_box[0]);
        assert!((ry - pc_box[1]).abs() < 1e-3, "re y {ry} != pc_box[1] {}", pc_box[1]);
        assert!((rw - (pc_box[2] - pc_box[0])).abs() < 1e-3, "re w {rw} != {}", pc_box[2] - pc_box[0]);
        assert!((rh - (pc_box[3] - pc_box[1])).abs() < 1e-3, "re h {rh} != {}", pc_box[3] - pc_box[1]);

        // Transform test: user-space translation by (10, 10)
        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([1.0, 0.0, 0.0, 1.0, 10.0, 10.0]) }).unwrap();

        let expected_transformed = [r[0] + 10.0, r[1] + 10.0, r[2] + 10.0, r[3] + 10.0];
        let painted_transformed = s.get(id).unwrap().page_images(0)[0].rect;
        for i in 0..4 {
            assert!(
                (painted_transformed[i] - expected_transformed[i]).abs() < 1e-3,
                "Transform: rotate={rotate}, user_unit={user_unit}: painted[{i}] = {}, expected {}",
                painted_transformed[i],
                expected_transformed[i]
            );
        }
    }
}

#[test]
fn replace_on_added_image_replaces_xobject_and_undo_restores() {
    let make_png = |rgb: [u8; 3]| {
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            let data = [rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2]];
            enc.write_header().unwrap().write_image_data(&data).unwrap();
        }
        Arc::new(png)
    };
    let png1 = make_png([255, 0, 0]);
    let png2 = make_png([0, 0, 255]);

    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([20.0, 20.0, 60.0, 60.0]), name: "img1.png".into(), bytes: png1 }).unwrap();

    let old_obj = s.get(id).unwrap().page_images(0)[0].object.expect("initial image object ref");

    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Replace { name: "img2.png".into(), bytes: png2 } }).unwrap();

    let new_obj = s.get(id).unwrap().page_images(0)[0].object.expect("new image object ref");
    assert_ne!(new_obj, old_obj, "after Replace the page references the new XObject");

    s.undo(id).unwrap();

    let restored_obj = s.get(id).unwrap().page_images(0)[0].object.expect("restored image object ref");
    assert_eq!(restored_obj, old_obj, "undo restores the old XObject");

    // Also test replacement on an image with active crop (promoted path)
    let png3 = make_png([0, 255, 0]);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Crop([25.0, 25.0, 55.0, 55.0]) }).unwrap();
    let crop_obj = s.get(id).unwrap().page_images(0)[0].object.expect("crop image object ref");
    assert_eq!(crop_obj, old_obj);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Replace { name: "img3.png".into(), bytes: png3 } }).unwrap();
    let promoted_new_obj = s.get(id).unwrap().page_images(0)[0].object.expect("promoted replace object ref");
    assert_ne!(promoted_new_obj, crop_obj);
    s.undo(id).unwrap();
    assert_eq!(s.get(id).unwrap().page_images(0)[0].object.expect("undo promoted replace"), crop_obj);
}

#[test]
fn mirroring_transform_on_added_image_flips_and_non_finite_is_refused() {
    let make_png = |rgb: [u8; 3]| {
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            let data = [rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2]];
            enc.write_header().unwrap().write_image_data(&data).unwrap();
        }
        Arc::new(png)
    };
    let png1 = make_png([255, 0, 0]);

    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([0.0, 0.0, 80.0, 40.0]), name: "img1.png".into(), bytes: png1 }).unwrap();

    let initial_img = s.get(id).unwrap().page_images(0)[0].clone();
    assert_eq!(initial_img.rect, [0.0, 0.0, 80.0, 40.0]);
    assert!(initial_img.matrix[0] > 0.0, "initially unflipped");

    // Mirroring transform: [-1.0, 0.0, 0.0, 1.0, 80.0, 0.0]
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([-1.0, 0.0, 0.0, 1.0, 80.0, 0.0]) }).unwrap();

    let flipped_img = s.get(id).unwrap().page_images(0)[0].clone();
    assert!(flipped_img.matrix[0] < 0.0, "horizontal mirror flips matrix x-axis, got matrix: {:?}", flipped_img.matrix);

    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let added = pdfcraft_edit::list_added(&cos);
    assert_eq!(added.len(), 1);
    let AddedContent::Image(ai) = &added[0].content else { panic!("expected added image") };
    assert!(ai.flip_h, "PCAdded record must have flip_h = true");

    // Mirroring horizontally again flips it back
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([-1.0, 0.0, 0.0, 1.0, 80.0, 0.0]) }).unwrap();
    let unflipped_img = s.get(id).unwrap().page_images(0)[0].clone();
    assert!(unflipped_img.matrix[0] > 0.0, "double horizontal mirror restores positive x-axis");
    let cos2 = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let AddedContent::Image(ai2) = &pdfcraft_edit::list_added(&cos2)[0].content else { panic!("expected added image") };
    assert!(!ai2.flip_h, "PCAdded record must have flip_h = false");

    // Mirroring vertically: [1.0, 0.0, 0.0, -1.0, 0.0, 40.0]
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([1.0, 0.0, 0.0, -1.0, 0.0, 40.0]) }).unwrap();
    let v_flipped_img = s.get(id).unwrap().page_images(0)[0].clone();
    assert!(v_flipped_img.matrix[3] < 0.0, "vertical mirror flips matrix y-axis, got matrix: {:?}", v_flipped_img.matrix);
    let cos3 = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let AddedContent::Image(ai3) = &pdfcraft_edit::list_added(&cos3)[0].content else { panic!("expected added image") };
    assert!(ai3.flip_v, "PCAdded record must have flip_v = true");

    // Non-finite transform must be refused with error
    let res = s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]) });
    assert!(res.is_err(), "NaN transform must be refused");

    let res_inf = s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([1.0, 0.0, 0.0, f64::INFINITY, 0.0, 0.0]) });
    assert!(res_inf.is_err(), "infinite transform must be refused");

    let res_move = s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move([f64::NAN, 20.0, 100.0, 60.0]) });
    assert!(res_move.is_err(), "NaN position must be refused");
}

#[test]
fn transform_on_added_image_folds_quarter_turns_and_promotes_non_axis_aligned() {
    let make_png = |rgb: [u8; 3]| {
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            let data = [rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2], rgb[0], rgb[1], rgb[2]];
            enc.write_header().unwrap().write_image_data(&data).unwrap();
        }
        Arc::new(png)
    };
    let png1 = make_png([0, 128, 255]);

    let (mut s, id) = session_with(1);
    s.apply(id, Edit::AddImage { page: 0, rect: Some([100.0, 100.0, 200.0, 150.0]), name: "img.png".into(), bytes: png1 }).unwrap();

    // 1. A pure 90° clockwise rotation matrix about the image centre:
    let rot_90_cw = pdfcraft_edit::turn_about_centre([100.0, 100.0, 200.0, 150.0], 1, false, false);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform(rot_90_cw) }).unwrap();

    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let added = pdfcraft_edit::list_added(&cos);
    assert_eq!(added.len(), 1, "stays as PCAdded image");
    let AddedContent::Image(ai) = &added[0].content else { panic!("expected added image") };
    assert_eq!(ai.rotation, 3, "quarter turn clockwise folds into ai.rotation = 3 (270° CCW)");
    // Centre is (150, 125); rotated width=50, height=100 -> [125, 75, 175, 175]
    assert!((ai.rect[0] - 125.0).abs() < 1e-4);
    assert!((ai.rect[1] - 75.0).abs() < 1e-4);
    assert!((ai.rect[2] - 175.0).abs() < 1e-4);
    assert!((ai.rect[3] - 175.0).abs() < 1e-4);

    // 2. A pure 90° counter-clockwise rotation matrix:
    let rot_90_ccw = pdfcraft_edit::turn_about_centre(ai.rect, 3, false, false);
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform(rot_90_ccw) }).unwrap();

    let cos2 = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    let AddedContent::Image(ai2) = &pdfcraft_edit::list_added(&cos2)[0].content else { panic!("expected added image") };
    assert_eq!(ai2.rotation, 0, "quarter turn counter-clockwise restores ai.rotation = 0");
    assert!((ai2.rect[0] - 100.0).abs() < 1e-4);
    assert!((ai2.rect[1] - 100.0).abs() < 1e-4);
    assert!((ai2.rect[2] - 200.0).abs() < 1e-4);
    assert!((ai2.rect[3] - 150.0).abs() < 1e-4);

    // Arbitrary transforms promote the added image, just like RotateBy.
    let initial_matrix = s.get(id).unwrap().page_images(0)[0].matrix;
    let rot_45 = pdfcraft_edit::turn_by_degrees([100.0, 100.0, 200.0, 150.0], 45.0).unwrap();
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform(rot_45) }).unwrap();
    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    assert!(pdfcraft_edit::list_added(&cos).is_empty(), "arbitrary transform promotes the image");
    assert_eq!(s.get(id).unwrap().page_images(0).len(), 1);
    let expected = pdfcraft_content::Matrix(initial_matrix).then(&pdfcraft_content::Matrix(rot_45)).0;
    let actual = s.get(id).unwrap().page_images(0)[0].matrix;
    assert!(actual.into_iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3), "{actual:?} != {expected:?}");
    s.undo(id).unwrap();
    let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
    assert_eq!(pdfcraft_edit::list_added(&cos).len(), 1, "undo restores added-image editability");
    s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform([1.0, 0.5, 0.0, 1.0, 0.0, 0.0]) }).unwrap();
    assert_eq!(s.get(id).unwrap().page_images(0).len(), 1);
}
