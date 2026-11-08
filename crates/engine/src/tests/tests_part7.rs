//! Unit tests of the engine (part 7 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn test_added_image_transform_reflections_with_rotation() {
    let make_asymmetric_png = || {
        let mut px = Vec::with_capacity(20 * 20 * 3);
        for y in 0..20 {
            for x in 0..20 {
                let color = match (x < 10, y < 10) {
                    (true, true) => [255, 0, 0],     // TL: Red
                    (false, true) => [0, 255, 0],    // TR: Green
                    (true, false) => [0, 0, 255],    // BL: Blue
                    (false, false) => [255, 255, 0], // BR: Yellow
                };
                px.extend_from_slice(&color);
            }
        }
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 20, 20);
            enc.set_color(png::ColorType::Rgb);
            enc.write_header().unwrap().write_image_data(&px).unwrap();
        }
        Arc::new(png)
    };

    let colour_at = |s: &Session, id: DocId, x: u32, y_from_top: u32| -> [u8; 3] {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), Default::default());
        let out = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        let i = ((y_from_top * out.width + x) * 4) as usize;
        [out.rgba[i], out.rgba[i + 1], out.rgba[i + 2]]
    };

    let is_red = |c: [u8; 3]| c[0] > 200 && c[1] < 50 && c[2] < 50;
    let is_green = |c: [u8; 3]| c[0] < 50 && c[1] > 200 && c[2] < 50;
    let is_blue = |c: [u8; 3]| c[0] < 50 && c[1] < 50 && c[2] > 200;
    let is_yellow = |c: [u8; 3]| c[0] > 200 && c[1] > 200 && c[2] < 50;

    let check_quadrants =
        |s: &Session, id: DocId, tl: fn([u8; 3]) -> bool, tr: fn([u8; 3]) -> bool, bl: fn([u8; 3]) -> bool, br: fn([u8; 3]) -> bool, label: &str| {
            let c_tl = colour_at(s, id, 75, 125);
            let c_tr = colour_at(s, id, 125, 125);
            let c_bl = colour_at(s, id, 75, 175);
            let c_br = colour_at(s, id, 125, 175);
            assert!(tl(c_tl), "{label} TL mismatch: got {:?}", c_tl);
            assert!(tr(c_tr), "{label} TR mismatch: got {:?}", c_tr);
            assert!(bl(c_bl), "{label} BL mismatch: got {:?}", c_bl);
            assert!(br(c_br), "{label} BR mismatch: got {:?}", c_br);
        };

    // --- CASE 1: 90° clockwise rotation (ImageEdit::Rotate(1), ai.rotation = 3) ---
    // Unrotated: TL=Red, TR=Green, BL=Blue, BR=Yellow.
    // After 90° CW: TL=Blue, TR=Red, BL=Yellow, BR=Green.
    {
        // 1a: Flip Horizontal after 90° rotation
        let (mut s, id) = session_with(1);
        s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 150.0, 200.0]), name: "img.png".into(), bytes: make_asymmetric_png() })
            .unwrap();
        check_quadrants(&s, id, is_red, is_green, is_blue, is_yellow, "initial");

        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Rotate(1) }).unwrap();
        check_quadrants(&s, id, is_blue, is_red, is_yellow, is_green, "90° CW");

        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Flip { horizontal: true } }).unwrap();
        let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
        let AddedContent::Image(ai) = &pdfcraft_edit::list_added(&cos)[0].content else { panic!() };
        assert_eq!(ai.rotation, 3);
        assert!(!ai.flip_h, "at 90° rotation, horizontal flip toggles flip_v, not flip_h");
        assert!(ai.flip_v, "at 90° rotation, horizontal flip toggles flip_v");
        // Flipping horizontally on screen swaps left & right columns: TL=Red, TR=Blue, BL=Green, BR=Yellow
        check_quadrants(&s, id, is_red, is_blue, is_green, is_yellow, "90° CW + flip horizontal");
    }

    {
        // 1b: Flip Vertical after 90° rotation
        let (mut s, id) = session_with(1);
        s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 150.0, 200.0]), name: "img.png".into(), bytes: make_asymmetric_png() })
            .unwrap();
        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Rotate(1) }).unwrap();

        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Flip { horizontal: false } }).unwrap();
        let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
        let AddedContent::Image(ai) = &pdfcraft_edit::list_added(&cos)[0].content else { panic!() };
        assert_eq!(ai.rotation, 3);
        assert!(ai.flip_h, "at 90° rotation, vertical flip toggles flip_h");
        assert!(!ai.flip_v, "at 90° rotation, vertical flip toggles flip_h, not flip_v");
        // Flipping vertically on screen swaps top & bottom rows: TL=Yellow, TR=Green, BL=Blue, BR=Red
        check_quadrants(&s, id, is_yellow, is_green, is_blue, is_red, "90° CW + flip vertical");
    }

    // --- CASE 2: 270° clockwise rotation (ImageEdit::Rotate(3), ai.rotation = 1) ---
    // Unrotated: TL=Red, TR=Green, BL=Blue, BR=Yellow.
    // After 270° CW: TL=Green, TR=Yellow, BL=Red, BR=Blue.
    {
        // 2a: Flip Horizontal after 270° rotation
        let (mut s, id) = session_with(1);
        s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 150.0, 200.0]), name: "img.png".into(), bytes: make_asymmetric_png() })
            .unwrap();
        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Rotate(3) }).unwrap();
        check_quadrants(&s, id, is_green, is_yellow, is_red, is_blue, "270° CW");

        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Flip { horizontal: true } }).unwrap();
        let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
        let AddedContent::Image(ai) = &pdfcraft_edit::list_added(&cos)[0].content else { panic!() };
        assert_eq!(ai.rotation, 1);
        assert!(!ai.flip_h, "at 270° rotation, horizontal flip toggles flip_v, not flip_h");
        assert!(ai.flip_v, "at 270° rotation, horizontal flip toggles flip_v");
        // Flipping horizontally on screen swaps left & right columns: TL=Yellow, TR=Green, BL=Blue, BR=Red
        check_quadrants(&s, id, is_yellow, is_green, is_blue, is_red, "270° CW + flip horizontal");
    }

    {
        // 2b: Flip Vertical after 270° rotation
        let (mut s, id) = session_with(1);
        s.apply(id, Edit::AddImage { page: 0, rect: Some([50.0, 100.0, 150.0, 200.0]), name: "img.png".into(), bytes: make_asymmetric_png() })
            .unwrap();
        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Rotate(3) }).unwrap();

        s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Flip { horizontal: false } }).unwrap();
        let cos = pdfcraft_cos::Document::open(s.get(id).unwrap().bytes.clone()).unwrap();
        let AddedContent::Image(ai) = &pdfcraft_edit::list_added(&cos)[0].content else { panic!() };
        assert_eq!(ai.rotation, 1);
        assert!(ai.flip_h, "at 270° rotation, vertical flip toggles flip_h");
        assert!(!ai.flip_v, "at 270° rotation, vertical flip toggles flip_h, not flip_v");
        // Flipping vertically on screen swaps top & bottom rows: TL=Red, TR=Blue, BL=Green, BR=Yellow
        check_quadrants(&s, id, is_red, is_blue, is_green, is_yellow, "270° CW + flip vertical");
    }
}

/// ISO 32000-2 Table 192, 12.7.5.2.3 and Table 228 (Comb): a check box, a push button, a comb
/// field the file leaves to the viewer. The page as displayed and printed as an image shows the
/// check box's caption symbol in its text colour, the button's grey face and the comb's cells in
/// the border colour, and saving writes the file as it was.
#[test]
fn buttons_and_combs_left_to_the_viewer_are_displayed() {
    let pdf = b"%PDF-1.7\n\
        1 0 obj << /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >> endobj\n\
        2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
        3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Annots [6 0 R 7 0 R 8 0 R] >> endobj\n\
        4 0 obj << /Fields [6 0 R 7 0 R 8 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >> endobj\n\
        5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> endobj\n\
        6 0 obj << /Type /Annot /Subtype /Widget /FT /Btn /T (k) /V /Yes /AS /Yes /DA (/ZaDb 0 Tf 1 0 0 rg) /MK << /CA (8) /BC [0 0 0] >> /Rect [20 150 60 190] /F 4 /P 3 0 R >> endobj\n\
        7 0 obj << /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (o) /DA (/Helv 12 Tf 0 g) /MK << /CA (Press) /BG [0.8 0.8 0.8] /BC [0 0 0] >> /BS << /W 2 /S /B >> /Rect [100 150 220 190] /F 4 /P 3 0 R >> endobj\n\
        8 0 obj << /Type /Annot /Subtype /Widget /FT /Tx /Ff 16777216 /MaxLen 4 /T (d) /V (AB) /DA (/Helv 12 Tf 0 g) /MK << /BC [0 0 1] >> /Rect [20 80 220 110] /F 4 /P 3 0 R >> endobj\n\
        trailer << /Root 1 0 R >>\n%%EOF"
        .to_vec();
    let render = |bytes: Arc<Vec<u8>>| {
        let mut r = pdfcraft_render::PageRenderer::new(bytes, pdfcraft_render::RenderConfig::default());
        r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() })
    };
    // Pixels of the page (drawn at scale 1, 400 x 200) in user-space box x0..x1, y0..y1 that `keep`.
    let count = |page: &pdfcraft_render::RenderedPage, (x0, y0, x1, y1): (u32, u32, u32, u32), keep: &dyn Fn(&[u8; 4]) -> bool| {
        let (px, _) = page.rgba.as_chunks::<4>();
        (200 - y1..200 - y0)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| px.get((y * page.width + x) as usize).is_some_and(keep))
            .count()
    };
    let red = |p: &[u8; 4]| p[0] > 200 && p[1] < 80 && p[2] < 80;
    let grey = |p: &[u8; 4]| (180..=230).contains(&p[0]) && p[0] == p[1] && p[1] == p[2];
    // Bluish: a 1 pt line on a pixel edge is half blue on each side.
    let blue = |p: &[u8; 4]| p[2] > 200 && p[0] < 180 && p[1] < 180 && p[0] + 40 < p[2];
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(pdf), None).unwrap();
    let shown = render(s.get(id).unwrap().display.clone());
    assert!(count(&shown, (24, 154, 56, 186), &red) > 60, "the check box's red cross");
    assert!(count(&shown, (104, 154, 216, 186), &grey) > 1000, "the push button's grey face");
    // The comb's three inner lines at x = 70, 120 and 170, between its cells.
    for x in [70, 120, 170] {
        assert!(count(&shown, (x - 1, 84, x + 1, 106), &blue) >= 20, "a cell line at {x}");
    }
    assert_eq!(count(&shown, (95, 84, 97, 106), &blue), 0, "no line inside a cell");
    // Print as Image: the page as a picture, at 72 dpi so it is the size of the page here.
    let raster = pdfcraft_print::Raster { dpi: 72.0, grayscale: false };
    let pictures = crate::print_raster::rasterize(s.get(id).unwrap(), &[0], &raster, pdfcraft_print::Content::Document).unwrap();
    let printed = render(Arc::new(pictures));
    assert!(count(&printed, (24, 154, 56, 186), &red) > 60, "the check box prints");
    assert!(count(&printed, (104, 154, 216, 186), &grey) > 1000, "and the push button");
    // The file is saved as it was: no widget has an appearance.
    let saved = pdfcraft_cos::Document::open(s.save_bytes(id).unwrap()).unwrap();
    for f in pdfcraft_forms::fields(&saved) {
        assert!(saved.get(f.widgets[0].obj).as_dict().is_some_and(|d| !d.contains(b"AP")), "{} has no appearance", f.name);
    }
}

/// ISO 32000-2 Tables 166 and 183, drawn on the page: a square without an appearance whose
/// `/Border [0 0 3 [6 3]]` names its width before a dash array is stroked 3 pt wide and dashed
/// (it was 1 pt and solid), and a caret with `/Sy /P` shows a ¶ to its right.
#[test]
fn legacy_border_dashes_and_the_caret_paragraph_symbol_are_drawn() {
    let original = Arc::new(pdf_with_annots(&[
        b"<< /Type /Annot /Subtype /Square /Rect [20 180 120 280] /Border [0 0 3 [6 3]] /C [1 0 0] >>",
        b"<< /Type /Annot /Subtype /Caret /Rect [20 20 80 80] /Sy /P /C [0 0 1] >>",
    ]));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("noap.pdf", None, original, None).expect("opens");
    let p = shown(&s, id, 0);
    let px = |x: u32, y: u32| p.rgba[((y * p.width + x) * 4) as usize..][..4].to_vec();
    // y down: the bottom edge's stroke is centred 1.5 pt inside /Rect, on row 118.
    let row: Vec<Vec<u8>> = (24..116).map(|x| px(x, 118)).collect();
    let red = row.iter().filter(|c| c[0] > 200 && c[1] < 80).count();
    let white = row.iter().filter(|c| c[1] > 200).count();
    assert!(red > 40 && white > 15, "3 pt and dashed: {red} red, {white} white");
    // The ¶ sits in the right part of the caret's /Rect, where the caret no longer reaches.
    let blue = (250..270).flat_map(|y| (68..80).map(move |x| (x, y))).filter(|&(x, y)| {
        let c = px(x, y);
        c[2] > 150 && c[0] < 100
    });
    assert!(blue.count() > 20, "the pilcrow is drawn beside the caret");
}

/// Import comments with a PDF chosen copies that PDF's comments in one undoable step, and refuses
/// a PDF that has none.
#[test]
fn importing_a_pdf_copies_its_comments() {
    let mut s = Session::new();
    let id = s.open("doc.pdf", None, Arc::new(pdf_with_annots(&[])), None).unwrap();
    let review = pdf_with_annots(&[
        b"<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (from the review) /P 3 0 R >>",
        b"<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /A << /S /URI /URI (https://example.com/) >> /P 3 0 R >>",
    ]);
    s.apply(id, Edit::ImportData { name: "review.pdf".into(), bytes: Arc::new(review.clone()) }).unwrap();
    let contents = |s: &Session| s.get(id).unwrap().info.annotations.iter().map(|a| a.contents.clone()).collect::<Vec<_>>();
    assert_eq!(contents(&s), [Some("from the review".to_string())], "the comment, not the link");
    assert_eq!(s.undo(id).unwrap().to_string(), "Import review.pdf");
    assert!(s.get(id).unwrap().info.annotations.is_empty());

    // Bytes before the header, as the reader allows them.
    let prefixed = [b"\r\n  \n".as_slice(), &review].concat();
    s.apply(id, Edit::ImportData { name: "prefixed.pdf".into(), bytes: Arc::new(prefixed) }).unwrap();
    assert_eq!(contents(&s), [Some("from the review".to_string())]);
    s.undo(id).unwrap();

    let links = pdf_with_annots(&[b"<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /P 3 0 R >>"]);
    let err = s.apply(id, Edit::ImportData { name: "links.pdf".into(), bytes: Arc::new(links.clone()) }).unwrap_err();
    assert_eq!(err.to_string(), pdfcraft_xfdf::DataError::NothingImported.to_string());
    // A PDF that needs a password is refused with that reason, with or without bytes before its header.
    let mut locked = pdfcraft_cos::Document::open(Arc::new(review)).unwrap();
    locked
        .set_encryption(&pdfcraft_cos::NewEncryption {
            algorithm: pdfcraft_cos::Algorithm::Aes256,
            user_password: "u",
            owner_password: "o",
            permissions: 0,
            encrypt_metadata: true,
            seed: [1; 32],
        })
        .unwrap();
    let locked = pdfcraft_cos::write_full(&locked, &Default::default()).unwrap();
    for bytes in [locked.clone(), [b"\n".as_slice(), &locked].concat()] {
        let err = s.apply(id, Edit::ImportData { name: "locked.pdf".into(), bytes: Arc::new(bytes) }).unwrap_err();
        assert_eq!(err.to_string(), EditError::Source("locked.pdf: it is password-protected".into()).to_string());
    }

    // An XFDF file that mentions the PDF header is still read as XFDF.
    let xfdf = r#"<?xml version="1.0"?><!-- exported from a file starting %PDF-1.7 --><xfdf xmlns="http://ns.adobe.com/xfdf/"><annots><text page="0" name="n1" rect="10,10,30,30"><contents>from the data file</contents></text></annots></xfdf>"#;
    s.apply(id, Edit::ImportData { name: "notes.xfdf".into(), bytes: Arc::new(xfdf.as_bytes().to_vec()) }).unwrap();
    assert_eq!(contents(&s), [Some("from the data file".to_string())]);
}

/// An XFA form imports the comments of a PDF with binary bytes before its header, instead of
/// refusing it as a data file in an unknown encoding.
#[test]
fn an_xfa_form_imports_comments_from_a_pdf_with_binary_bytes_before_its_header() {
    let data = "<form><page1><country>JP</country></page1></form>";
    let bytes = Arc::new(pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(data)));
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("fields.pdf", None, bytes, None).expect("opens");
    let review = pdf_with_annots(&[b"<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (from the review) /P 3 0 R >>"]);
    let prefixed = [b"\xC0\xC1\xF5\n".as_slice(), &review].concat();
    s.apply(id, Edit::ImportData { name: "prefixed.pdf".into(), bytes: Arc::new(prefixed) }).unwrap();
    let contents: Vec<_> = s.get(id).unwrap().info.annotations.iter().map(|a| a.contents.clone()).collect();
    assert!(contents.contains(&Some("from the review".to_string())), "{contents:?}");
}

/// ISO 32000-2 Table 224 (`/NeedAppearances`) and Table 192 (`/MK /CA`): a form that leaves its
/// appearances to the viewer shows its check boxes and radio buttons drawn from `/MK`, over the
/// filled squares the file carries: a checked box its check mark (ZapfDingbats `4`), an unchecked
/// one its empty border, a radio button its dot (`l`), on screen and printed as an image. Without
/// `/NeedAppearances` the file's own squares show. Saving writes the file as it was.
#[test]
fn need_appearances_buttons_are_displayed_from_their_captions() {
    let pdf = |need: bool| {
        let flag = if need { "/NeedAppearances true " } else { "" };
        format!(
            "%PDF-1.7\n\
             1 0 obj << /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >> endobj\n\
             2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
             3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Annots [6 0 R 7 0 R 8 0 R] >> endobj\n\
             4 0 obj << /Fields [6 0 R 7 0 R 8 0 R] {flag}/DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >> endobj\n\
             5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> endobj\n\
             6 0 obj << /Type /Annot /Subtype /Widget /FT /Btn /T (on) /V /Yes /AS /Yes /DA (/Helv 0 Tf 0 g) /MK << /BC [0 0 0] /CA (4) >> /Rect [20 100 60 140] /F 4 /P 3 0 R /AP << /N << /Yes 9 0 R /Off 9 0 R >> >> >> endobj\n\
             7 0 obj << /Type /Annot /Subtype /Widget /FT /Btn /T (off) /V /Off /AS /Off /DA (/Helv 0 Tf 0 g) /MK << /BC [0 0 0] /CA (4) >> /Rect [100 100 140 140] /F 4 /P 3 0 R /AP << /N << /Yes 9 0 R /Off 9 0 R >> >> >> endobj\n\
             8 0 obj << /Type /Annot /Subtype /Widget /FT /Btn /Ff 32768 /T (radio) /V /B /AS /B /DA (/Helv 0 Tf 0 g) /MK << /BC [0 0 0] /CA (l) >> /Rect [180 100 220 140] /F 4 /P 3 0 R /AP << /N << /B 9 0 R /Off 9 0 R >> >> >> endobj\n\
             9 0 obj << /Type /XObject /Subtype /Form /BBox [0 0 40 40] /Length 18 >> stream\n0 g 4 4 32 32 re f\nendstream endobj\n\
             trailer << /Root 1 0 R >>\n%%EOF"
        )
        .into_bytes()
    };
    let render = |bytes: Arc<Vec<u8>>| {
        let mut r = pdfcraft_render::PageRenderer::new(bytes, pdfcraft_render::RenderConfig::default());
        r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() })
    };
    // Dark pixels of the page (drawn at scale 1, 400 x 200) in the user-space box x0..x1, y0..y1.
    let ink = |page: &pdfcraft_render::RenderedPage, (x0, y0, x1, y1): (u32, u32, u32, u32)| {
        let (px, _) = page.rgba.as_chunks::<4>();
        (200 - y1..200 - y0)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| px.get((y * page.width + x) as usize).is_some_and(|p| p[0] < 128 && p[1] < 128 && p[2] < 128))
            .count()
    };
    // In a widget whose lower-left corner is (x, y): its whole inside, the top-left of its inside
    // (where a check mark leaves the box empty), and the lower-left corner (outside a dot).
    let inside = |x: u32, y: u32| (x + 6, y + 6, x + 34, y + 34);
    let top_left = |x: u32, y: u32| (x + 6, y + 26, x + 14, y + 34);
    let corner = |x: u32, y: u32| (x + 5, y + 5, x + 9, y + 9);
    let mut s = Session::new();
    let id = s.open("form.pdf", None, Arc::new(pdf(true)), None).unwrap();
    let shown = render(s.get(id).unwrap().display.clone());
    let raster = pdfcraft_print::Raster { dpi: 72.0, grayscale: false };
    let pictures = crate::print_raster::rasterize(s.get(id).unwrap(), &[0], &raster, pdfcraft_print::Content::Document).unwrap();
    let printed = render(Arc::new(pictures));
    for (page, how) in [(&shown, "shown"), (&printed, "printed")] {
        let (check, check_corner) = (ink(page, inside(20, 100)), ink(page, top_left(20, 100)));
        assert!(check > 100 && check < 600, "{how}: a check mark, not the 28 x 28 square: {check}");
        assert_eq!(check_corner, 0, "{how}: the check mark leaves the top left empty");
        assert_eq!(ink(page, inside(100, 100)), 0, "{how}: the unchecked box is empty");
        assert!(ink(page, (100, 100, 140, 102)) > 30, "{how}: and keeps its border");
        let dot = ink(page, (188, 108, 212, 132));
        assert!(dot > 300 && ink(page, corner(180, 100)) == 0, "{how}: the radio button's dot: {dot}");
    }
    let saved = s.save_bytes(id).unwrap();
    assert!(saved.windows(18).any(|w| w == b"0 g 4 4 32 32 re f"), "the file keeps its own appearances");

    // Without /NeedAppearances the file's filled squares are shown.
    let id = s.open("plain.pdf", None, Arc::new(pdf(false)), None).unwrap();
    let shown = render(s.get(id).unwrap().display.clone());
    for x in [20, 100, 180] {
        assert!(ink(&shown, inside(x, 100)) > 700, "the file's square at {x}");
    }
}

/// Copy and paste: the copies of a comment and its reply land on the asked-for page of another
/// document under names of their own (nothing replaced; a second paste gives a second set),
/// undo takes a paste away as one step, and a clipboard without comments is refused.
#[test]
fn pasting_copied_comments_adds_copies_that_undo_as_one_step() {
    let (mut s, id) = session_with(2);
    s.apply(id, rect_comment(1, [80.0, 80.0, 120.0, 120.0])).unwrap();
    s.apply(id, Edit::ReplyToAnnotation { page: 1, index: 0, text: "Done".into(), author: "Ada".into() }).unwrap();
    let xfdf = s.export_comments(id, &[(1, 0)]).unwrap();
    assert!(xfdf.contains("<square") && xfdf.contains("inreplyto="), "{xfdf}");
    let other = s.open("other.pdf", None, Arc::new(fixture(2)), None).expect("opens");
    s.apply(other, Edit::PasteComments { xfdf: xfdf.clone(), page: 0, offset: None }).unwrap();
    let a = s.get(other).unwrap().info.annotations.clone();
    assert_eq!(a.len(), 2, "{a:?}");
    assert!(a.iter().all(|x| x.page == 0), "on the asked-for page: {a:?}");
    let parent = a.iter().find(|x| x.in_reply_to.is_none()).unwrap();
    let reply = a.iter().find(|x| x.in_reply_to.is_some()).unwrap();
    assert_eq!(reply.in_reply_to, parent.name, "the reply follows its copied parent");
    assert_ne!(parent.name, s.get(id).unwrap().info.annotations[0].name, "a name of its own");
    assert_eq!(parent.contents.as_deref(), Some("Look here"));
    assert_eq!(s.get(other).unwrap().can_undo(), Some("Paste comments"));
    s.apply(other, Edit::PasteComments { xfdf: xfdf.clone(), page: 1, offset: Some([30.0, -10.0]) }).unwrap();
    let a = s.get(other).unwrap().info.annotations.clone();
    assert_eq!(a.len(), 4, "a second set");
    let placed = a.iter().find(|x| x.page == 1 && x.in_reply_to.is_none()).unwrap();
    assert!((placed.rect[0] - 110.0).abs() < 0.5 && (placed.rect[1] - 70.0).abs() < 0.5, "moved by the offset: {:?}", placed.rect);
    assert_eq!(comment_count(&xfdf), 1);
    assert_eq!(comments_bounds(&xfdf).map(|b| b.map(|v| v.round())), Some([80.0, 80.0, 120.0, 120.0]));
    s.undo(other).unwrap();
    assert_eq!(s.get(other).unwrap().info.annotations.len(), 2);
    assert!(s.apply(other, Edit::PasteComments { xfdf: "hello".into(), page: 0, offset: None }).is_err(), "no comments in it");
    assert!(s.apply(other, Edit::PasteComments { xfdf, page: 9, offset: None }).is_err(), "no such page");
    assert_eq!(s.get(other).unwrap().info.annotations.len(), 2, "a refused paste changes nothing");
    assert_eq!(s.get(id).unwrap().info.annotations.len(), 2, "the source is untouched");
}

#[test]
fn repeated_backdrop_artwork_survives_background_undo_redo_and_both_save_modes() {
    let white = "1 g";
    let rectangle = "0 0 100 100 re f";
    let artwork = "q 0 g 0.25 0 0 0.25 20 20 cm";
    let raw = format!(
        "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents [4 0 R 5 0 R 6 0 R 5 0 R 7 0 R] >> endobj\n\
         4 0 obj << /Length {} >> stream\n{white}\nendstream endobj\n\
         5 0 obj << /Length {} >> stream\n{rectangle}\nendstream endobj\n\
         6 0 obj << /Length {} >> stream\n{artwork}\nendstream endobj\n\
         7 0 obj << /Length 1 >> stream\nQ\nendstream endobj\ntrailer << /Root 1 0 R >>\n%%EOF",
        white.len(),
        rectangle.len(),
        artwork.len()
    );
    // Start with a conventional xref, so the first save can exercise incremental output.
    let source = pdfcraft_cos::Document::open(Arc::new(raw.into_bytes())).unwrap();
    let bytes = pdfcraft_cos::write_full(&source, &pdfcraft_cos::SaveOptions::default()).unwrap();
    let mut s = Session::new();
    let id = s.open("shared-backdrop.pdf", None, Arc::new(bytes), None).unwrap();
    let black = |s: &Session, id| {
        let doc = s.get(id).unwrap();
        let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
        let out = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!((out.width, out.height), (100, 100));
        // PDF point (30, 30), inside the later black square, in top-down raster coordinates.
        let i = ((70 * out.width + 30) * 4) as usize;
        let rgba = &out.rgba[i..i + 4];
        assert!(rgba[0] < 20 && rgba[1] < 20 && rgba[2] < 20, "later artwork survives: {rgba:?}");
    };
    black(&s, id);
    s.apply(
        id,
        Edit::AddBackground { pages: vec![0], settings: Background { color: [1.0, 0.0, 0.0], ..Background::default() }, replace: false, file: None },
    )
    .unwrap();
    black(&s, id);
    s.undo(id).unwrap();
    black(&s, id);
    s.redo(id).unwrap();
    black(&s, id);
    let saved = s.save_bytes(id).unwrap();
    let reopened = s.open("incremental.pdf", None, saved.clone(), None).unwrap();
    black(&s, reopened);
    let doc = pdfcraft_cos::Document::open(saved).unwrap();
    let full = pdfcraft_cos::write_full(&doc, &pdfcraft_cos::SaveOptions::default()).unwrap();
    let rewritten = s.open("full.pdf", None, Arc::new(full), None).unwrap();
    black(&s, rewritten);
    s.apply(rewritten, Edit::RemoveMarks { kind: MarkKind::Background }).unwrap();
    black(&s, rewritten);
}

#[test]
fn saving_after_deleting_pages_rewrites_the_file_without_them() {
    let (mut s, id) = session_with(6);
    let original = s.save_bytes(id).unwrap();
    s.apply(id, Edit::DeletePages { pages: vec![1, 2, 3, 4] }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert!(saved.len() < original.len(), "the file shrank: {} -> {}", original.len(), saved.len());
    assert!(!saved.starts_with(original.as_slice()), "a full rewrite, not an update appended to the old file");
    assert!(!contains(&saved, b"Page 3"), "a deleted page's text is not left in an earlier revision");
    let mut again = Session::new();
    let id2 = again.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(page_texts(&again, id2), ["Page 1", "Page 6"]);
}

#[test]
fn saving_after_deleting_pages_from_a_signed_file_stays_incremental() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(3)), None).unwrap();
    let opts = SignOptions { page: 0, rect: Some([20.0, 20.0, 160.0, 50.0]), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    s.mark_signed(id, signed.clone(), None).unwrap();
    s.apply(id, Edit::DeletePages { pages: vec![2] }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert!(saved.starts_with(signed.as_ref()), "the signed bytes stay an incremental update");
    assert!(s.get(id).unwrap().is_signed());
}

#[test]
fn saving_after_a_batch_that_deletes_pages_rewrites_the_file() {
    let (mut s, id) = session_with(6);
    let original = s.save_bytes(id).unwrap();
    let batch = Edit::Batch {
        label: "Delete and rotate".into(),
        edits: vec![
            Edit::Batch { label: "Delete pages".into(), edits: vec![Edit::DeletePages { pages: vec![1, 2, 3, 4] }] },
            Edit::RotatePages { pages: vec![0], degrees: 90 },
        ],
    };
    s.apply(id, batch).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert!(!saved.starts_with(original.as_slice()), "a full rewrite, not an update appended to the old file");
    assert!(!contains(&saved, b"Page 3"), "a deleted page's text is not left in an earlier revision");
}

#[test]
fn saving_without_deleting_pages_stays_incremental() {
    let (mut s, id) = session_with(3);
    let original = s.save_bytes(id).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert!(saved.starts_with(original.as_slice()), "an edit that deletes nothing is still appended");
}

#[test]
fn original_image_edit_does_not_match_an_added_sibling_of_the_same_xobject() {
    let mut document = pdfcraft_cos::Document::open(Arc::new(jpeg_cover_pdf(650))).unwrap();
    let original = pdfcraft_edit::page_images(&document, 0).unwrap()[0].clone();
    let added = AddedImage::new([20.0, 30.0, 80.0, 90.0], original.object.unwrap());
    pdfcraft_edit::add_content(&mut document, 0, &AddedContent::Image(added.clone())).unwrap();
    let bytes = pdfcraft_cos::write_full(&document, &pdfcraft_cos::SaveOptions::default()).unwrap();
    let mut session = Session::new();
    let id = session.open("shared-image.pdf", None, Arc::new(bytes), None).unwrap();
    let before_added = session.get(id).unwrap().added()[0].content.clone();
    session.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Move([40.0, 50.0, 140.0, 150.0]) }).unwrap();
    let doc = session.get(id).unwrap();
    assert_eq!(doc.added()[0].content, before_added, "matching an original Do must not select the added sibling's metadata");
    assert_eq!(doc.page_images(0)[0].rect, [40.0, 50.0, 140.0, 150.0]);
}

#[test]
fn image_transform_engine_refuses_singular_matrices_without_mutation() {
    let mut png = Vec::new();
    let mut enc = png::Encoder::new(&mut png, 1, 1);
    enc.set_color(png::ColorType::Rgb);
    enc.write_header().unwrap().write_image_data(&[100, 150, 200]).unwrap();
    let mut s = Session::new();
    let bytes = s.create_from_images(&[("pixel.png".into(), png)]).unwrap();
    let id = s.open("image.pdf", None, bytes, None).unwrap();
    for matrix in [[0.0, 0.0, 0.0, 1.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0 + 1e-12, 0.0, 0.0], [1e308, 0.0, 0.0, 1e308, 0.0, 0.0]] {
        let before = s.get(id).unwrap().bytes.clone();
        assert!(s.apply(id, Edit::EditPageImage { page: 0, index: 0, change: ImageEdit::Transform(matrix) }).is_err());
        assert_eq!(s.get(id).unwrap().bytes, before);
    }
}

/// Optimize has JBIG2 images share a symbol dictionary only where redaction can store each again
/// without it. Of four pages of the same scan, the two in DeviceGray and the image mask share one;
/// the one in a DeviceN of one colourant, which redaction doesn't store again, is coded alone.
/// Redacting the first page then stores the second and the mask again without the dictionary,
/// which leaves the file.
#[test]
fn redacting_an_optimized_jbig2_page_stores_the_rest_of_its_group_again() {
    use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, Stream, write_full};
    let (w, h) = (600usize, 480usize);
    // Lines of 26 letters, each 32 rows of a run from a stem down its left side, of 2 to 24 pixels,
    // the same to the pixel wherever it is, picked and spaced at random.
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as usize
    };
    let letters: Vec<Vec<usize>> = (0..26).map(|_| (0..16).map(|_| 1 + next() % 12).collect()).collect();
    let stride = w.div_ceil(8);
    let mut bits = vec![0xFFu8; stride * h];
    for top in (8..h - 40).step_by(44) {
        let mut x = 8 + next() % 8;
        while x + 32 < w {
            let letter = &letters[next() % 26];
            for (row, run) in letter.iter().enumerate() {
                for (px, py) in (x..x + 2 * run).flat_map(|px| [(px, top + 2 * row), (px, top + 2 * row + 1)]) {
                    bits[py * stride + px / 8] &= !(0x80 >> (px % 8));
                }
            }
            x += 2 * letter.iter().max().unwrap_or(&1) + 4 + next() % 5;
        }
    }
    let mut doc = Document::new_empty();
    let pages = doc.root().and_then(|r| doc.get(r).as_dict().and_then(|d| d.reference(b"Pages"))).unwrap();
    let device_n = Object::Array(vec![Object::name("DeviceN"), Object::Array(vec![Object::name("Black")]), Object::name("DeviceGray"), {
        let mut tint = Dict::new();
        tint.set(b"FunctionType".to_vec(), Object::Int(2));
        tint.set(b"Domain".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(1)]));
        tint.set(b"N".to_vec(), Object::Int(1));
        Object::Dict(tint)
    }]);
    let mut images: Vec<ObjRef> = Vec::new();
    let grey = || ("ColorSpace", Object::name("DeviceGray"));
    for (k, (key, value)) in [grey(), grey(), ("ColorSpace", device_n), ("ImageMask", Object::Bool(true))].into_iter().enumerate() {
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::name("XObject"));
        d.set(b"Subtype".to_vec(), Object::name("Image"));
        d.set(b"Width".to_vec(), Object::Int(w as i64));
        d.set(b"Height".to_vec(), Object::Int(h as i64));
        d.set(b"BitsPerComponent".to_vec(), Object::Int(1));
        d.set(key.as_bytes().to_vec(), value);
        let image = doc.add(Object::Stream(Stream::flate(d, &bits)));
        images.push(image);
        let content = doc.add(Object::Stream(Stream::from_raw(Dict::new(), b"q 144 0 0 115.2 36 600 cm /Im Do Q".to_vec())));
        let mut xobjects = Dict::new();
        xobjects.set(b"Im".to_vec(), Object::Ref(image));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xobjects));
        let mut page = Dict::new();
        page.set(b"Type".to_vec(), Object::name("Page"));
        page.set(b"Parent".to_vec(), Object::Ref(pages));
        page.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
        page.set(b"Resources".to_vec(), Object::Dict(resources));
        page.set(b"Contents".to_vec(), Object::Ref(content));
        if k == 0 {
            let mut mark = Dict::new();
            mark.set(b"Type".to_vec(), Object::name("Annot"));
            mark.set(b"Subtype".to_vec(), Object::name("Redact"));
            mark.set(b"Rect".to_vec(), Object::Array(vec![40.into(), 610.into(), 100.into(), 650.into()]));
            let mark = doc.add(Object::Dict(mark));
            page.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(mark)]));
        }
        let page = doc.add(Object::Dict(page));
        doc.update_dict(pages, |d| {
            let mut kids = d.get(b"Kids").and_then(|k| k.as_array().cloned()).unwrap_or_default();
            kids.push(Object::Ref(page));
            d.set(b"Count".to_vec(), Object::Int(kids.len() as i64));
            d.set(b"Kids".to_vec(), Object::Array(kids));
        })
        .unwrap();
    }
    let mono = pdfcraft_optimize::MonoSettings {
        compression: pdfcraft_optimize::MonoCompression::Jbig2,
        downsample: false,
        ..pdfcraft_optimize::MonoSettings::REDUCE
    };
    pdfcraft_optimize::optimize(&mut doc, &pdfcraft_optimize::Settings { mono, ..Default::default() }).unwrap();
    let globals = |doc: &Document, image: ObjRef| match &*doc.get(image) {
        Object::Stream(s) => s.dict.get(b"DecodeParms").and_then(Object::as_dict).and_then(|d| d.reference(b"JBIG2Globals")),
        _ => None,
    };
    let shared = globals(&doc, images[0]);
    assert!(shared.is_some() && globals(&doc, images[1]) == shared && globals(&doc, images[3]) == shared, "DeviceGray and the mask share");
    assert_eq!(globals(&doc, images[2]), None, "the DeviceN page is coded alone");
    pdfcraft_redact::apply(&mut doc, None).unwrap();
    let saved = Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap();
    let named = saved.scan_objects().filter(|(_, o)| matches!(&**o, Object::Stream(s) if saved.jbig2_globals_ref(s).is_some())).count();
    assert_eq!(named, 0, "no image names the dictionary, which left with them");
}

#[test]
fn fill_sign_multiline_text_survives_engine_save_reopen_and_render() {
    let (mut session, id) = rotated_blank(0);
    let text = "\nAda\n\nLovelace\n";
    let shape = Shape::Typewriter { rect: text_box_at(0, [20.0, 250.0], 120.0, 66.0).unwrap(), font_size: 10.0 };
    let style = Style::default_for(&shape);
    session.apply(id, Edit::AddAnnotation(NewAnnotation { page: 0, shape, style, contents: text.into(), author: "Test".into() })).unwrap();
    assert_eq!(session.get(id).unwrap().info.annotations[0].contents.as_deref(), Some(text), "engine keeps outer typewriter line breaks");
    let before = render_page(&session, id);
    assert!(before.rgba.as_chunks::<4>().0.iter().any(|p| p[..3] == [0, 0, 0]), "the appearance contains ink");
    let bytes = session.save_bytes(id).unwrap();
    let mut reopened = Session::new();
    let again = reopened.open("multiline.pdf", None, bytes, None).unwrap();
    assert_eq!(reopened.get(again).unwrap().info.annotations[0].contents.as_deref(), Some(text), "reopening preserves all blank lines");
    assert_eq!(before.rgba, render_page(&reopened, again).rgba, "the saved appearance is identical");
}

/// A whole tree goes in as one undoable step, and a refused tree changes nothing.
#[test]
fn imported_bookmarks_undo_in_one_step() {
    let (mut s, id) = session_with(2);
    let node = |title: &str, page, children| pdfcraft_organize::OutlineNode {
        title: title.into(),
        page,
        view: None,
        color: None,
        italic: false,
        bold: false,
        open: true,
        children,
    };
    let bookmarks = vec![node("Intro", Some(0), vec![node("Detail", Some(5), vec![])]), node("End", Some(1), vec![])];
    s.apply(id, Edit::ImportBookmarks { parent: vec![], index: 0, bookmarks: bookmarks.clone(), replace: false }).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["Intro→1[Detail→0]", "End→2"]);
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Import bookmarks"));
    assert_eq!(s.outline_tree(id, &[]).unwrap().unwrap().nodes[0].title, "Intro");
    s.undo(id).unwrap();
    assert!(s.get(id).unwrap().info.outline.is_empty());
    s.apply(id, Edit::ImportBookmarks { parent: vec![], index: 0, bookmarks: bookmarks.clone(), replace: false }).unwrap();
    s.apply(id, Edit::ImportBookmarks { parent: vec![], index: 0, bookmarks: vec![node("New", Some(1), vec![])], replace: true }).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["New→2"]);
    s.undo(id).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["Intro→1[Detail→0]", "End→2"]);
    s.undo(id).unwrap();
    let err = s.apply(id, Edit::ImportBookmarks { parent: vec![4], index: 0, bookmarks, replace: false }).unwrap_err();
    assert!(err.to_string().contains("no bookmark"), "{err}");
    assert!(s.get(id).unwrap().info.outline.is_empty());
}
