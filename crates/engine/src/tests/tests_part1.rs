//! Unit tests of the engine (part 1 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn prepared_open_does_not_mutate_the_session_until_commit() {
    fn assert_send<T: Send>() {}
    assert_send::<crate::PreparedDocument>();

    let mut session = Session::new();
    let prepared = session.prepare_open("prepared.pdf", None, Arc::new(fixture(3)), None).expect("prepares");
    assert!(session.get(DocId(1)).is_none());

    let id = session.commit_open(prepared);
    assert_eq!(id, DocId(1));
    assert_eq!(session.get(id).expect("committed document").info.pages.len(), 3);
}

#[test]
fn custom_image_stamp_keeps_displayed_orientation_on_rotated_pages() {
    let colours = [[240, 20, 20, 255], [20, 180, 20, 255], [20, 20, 240, 255], [230, 180, 20, 255]];
    let image = image::RgbaImage::from_fn(80, 40, |x, y| image::Rgba(colours[usize::from(y >= 20) * 2 + usize::from(x >= 40)]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let bytes = Arc::new(png.into_inner());
    for degrees in [0, 90, 180, 270] {
        let (mut session, id) = session_with(1);
        session.apply(id, Edit::RotatePages { pages: vec![0], degrees }).unwrap();
        let at = session.get(id).unwrap().info.pages[0].view_to_user(100.0, 100.0).map(f64::from);
        session
            .apply(
                id,
                Edit::AddCustomStamp {
                    page: 0,
                    rect: [at[0], at[1], at[0], at[1]],
                    name: "Original quadrants".into(),
                    file: MarkFile { name: "quadrants.png".into(), bytes: bytes.clone(), page: 0 },
                    author: "Test".into(),
                },
            )
            .unwrap();
        let check = |session: &Session, points: [(u32, u32); 4]| {
            let doc = session.get(id).unwrap();
            let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
            let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
            assert!(rendered.error.is_none(), "{:?}", rendered.error);
            for ((x, y), colour) in points.into_iter().zip(colours) {
                let offset = ((y * rendered.width + x) * 4) as usize;
                assert_eq!(&rendered.rgba[offset..offset + 4], &colour, "page rotation {degrees}; displayed point ({x}, {y})");
            }
        };
        let points = [(80, 90), (120, 90), (80, 110), (120, 110)];
        check(&session, points);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let rect = session.get(id).unwrap().info.annotations[0].rect;
        let a = info.user_to_view(rect[0], rect[1]);
        let b = info.user_to_view(rect[2], rect[3]);
        let bounds = [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])];
        for (actual, expected) in bounds.into_iter().zip([60.0, 80.0, 140.0, 120.0]) {
            assert!((actual - expected).abs() < 0.0001, "displayed bounds {bounds:?}");
        }
        session.apply(id, Edit::StyleAnnotation { page: 0, index: 0, color: None, opacity: Some(1.0), width: None, endings: None }).unwrap();
        check(&session, points);
        let resized = info.view_rect_to_user([40.0, 70.0, 160.0, 130.0]).map(f64::from);
        session.apply(id, Edit::ResizeAnnotation { page: 0, index: 0, rect: resized }).unwrap();
        let resized_points = [(70, 85), (130, 85), (70, 115), (130, 115)];
        check(&session, resized_points);
        session.undo(id).unwrap();
        check(&session, points);
        session.redo(id).unwrap();
        check(&session, resized_points);
        let from = info.view_to_user(100.0, 100.0);
        let to = info.view_to_user(110.0, 115.0);
        session.apply(id, Edit::MoveAnnotation { page: 0, index: 0, dx: f64::from(to[0] - from[0]), dy: f64::from(to[1] - from[1]) }).unwrap();
        check(&session, [(80, 100), (140, 100), (80, 130), (140, 130)]);
        session.undo(id).unwrap();
        check(&session, resized_points);
        session.redo(id).unwrap();
        check(&session, [(80, 100), (140, 100), (80, 130), (140, 130)]);
    }
}

/// #1398: a Sign Here tag points left as displayed, on every page rotation, including after save.
#[test]
fn rubber_stamps_point_left_as_displayed_on_rotated_pages() {
    let (w, h) = StampKind::SignHere.size();
    for degrees in [0, 90, 180, 270] {
        let (mut session, id) = rotated_blank(degrees);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let rect = displayed_rect(&info, 100.0, 40.0, w, h);
        let shape = Shape::Stamp { rect, stamp: StampKind::SignHere, by: None };
        session
            .apply(
                id,
                Edit::AddAnnotation(NewAnnotation {
                    page: 0,
                    style: Style::default_for(&shape),
                    shape,
                    contents: String::new(),
                    author: "Test".into(),
                }),
            )
            .unwrap();
        let check = |session: &Session, id: DocId| {
            let rendered = render_page(session, id);
            let doc = session.get(id).unwrap();
            let info = doc.info.pages[0].clone();
            let stamp = doc.info.annotations[0].rect;
            let a = info.user_to_view(stamp[0], stamp[1]);
            let b = info.user_to_view(stamp[2], stamp[3]);
            let (left, top) = (a[0].min(b[0]), a[1].min(b[1]));
            let (right, bottom) = (a[0].max(b[0]), a[1].max(b[1]));
            let scale = 2.0;
            let at = |x: f32, y: f32| stamp_pixel(&rendered, (x * scale) as u32, (y * scale) as u32);
            // The tag's point is the left edge. The flat end fills the right edge.
            let tip = at(left + 4.0, (top + bottom) / 2.0);
            let outside = at(left + 4.0, top + 4.0);
            let end = at(right - 4.0, top + 4.0);
            assert!(tip[0] > 180 && tip[2] < 80, "rotation {degrees}: tip {tip:?} should be the orange point");
            assert!(outside.iter().all(|c| *c > 240), "rotation {degrees}: above the point {outside:?} should be paper");
            assert!(end[0] > 180 && end[2] < 80, "rotation {degrees}: flat end {end:?} should be orange");
        };
        check(&session, id);
        let saved = session.save_bytes(id).unwrap();
        let mut reopened = Session::new().with_clock(|| 1_700_000_000);
        let again = reopened.open("drawing.pdf", None, saved, None).unwrap();
        check(&reopened, again);
    }
}

/// #2583, #1869: a sticky note placed on a turned page is drawn upright, with its top-left as
/// displayed where it was placed (the corner its `/Rect` keeps, as the NoRotate flag it is saved
/// with asks), exactly as on an unturned page, including after save. The comment list's
/// `shown_rect`, which the UI hit-tests, is that square.
#[test]
fn sticky_notes_stay_upright_where_they_are_placed_on_rotated_pages() {
    // The displayed 20 × 20 pt square whose top-left is (100, 40), in device pixels at scale 2.
    let crop = |rendered: &pdfcraft_render::RenderedPage| -> Vec<[u8; 4]> {
        (80..120).flat_map(|y| (200..240).map(move |x| (x, y))).map(|(x, y)| stamp_pixel(rendered, x, y)).collect()
    };
    let mut upright = None;
    for degrees in [0, 90, 180, 270] {
        let (mut session, id) = rotated_blank(degrees);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let [x, y] = info.view_to_user(100.0, 40.0);
        let shape = Shape::Note { at: [f64::from(x), f64::from(y)], icon: NoteIcon::Comment };
        let style = Style::default_for(&shape);
        session.apply(id, Edit::AddAnnotation(NewAnnotation { page: 0, style, shape, contents: "Turned".into(), author: "Test".into() })).unwrap();
        let mut check = |session: &Session, id: DocId| {
            let doc = session.get(id).unwrap();
            let info = &doc.info.pages[0];
            let note = &doc.info.annotations[0];
            assert!(note.no_rotate, "rotation {degrees}: notes are saved NoRotate");
            let shown = note.shown_rect(info.rotation);
            let (a, b) = (info.user_to_view(shown[0], shown[1]), info.user_to_view(shown[2], shown[3]));
            let shown = [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])];
            assert!(shown.iter().zip([100.0, 40.0, 120.0, 60.0]).all(|(v, w)| (v - w).abs() < 0.01), "rotation {degrees}: shown at {shown:?}");
            let icon = crop(&render_page(session, id));
            let reference = upright.get_or_insert_with(|| icon.clone());
            assert!(reference.iter().filter(|c| c.iter().any(|v| *v < 200)).count() > 100, "the note's icon is drawn");
            let off = icon.iter().zip(reference.iter()).filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 48)).count();
            assert!(off <= 8, "rotation {degrees}: {off} of the icon's {} pixels differ from the unturned page's", icon.len());
        };
        check(&session, id);
        let saved = session.save_bytes(id).unwrap();
        let mut reopened = Session::new().with_clock(|| 1_700_000_000);
        let again = reopened.open("drawing.pdf", None, saved, None).unwrap();
        check(&reopened, again);
    }
}

/// #2392: Fill & Sign marks are drawn as displayed on every page rotation, including after save:
/// a checkmark's long stroke rises to the right, and a line runs across.
#[test]
fn fill_sign_marks_are_upright_as_displayed_on_rotated_pages() {
    for degrees in [0, 90, 180, 270] {
        let (mut session, id) = rotated_blank(degrees);
        let info = session.get(id).unwrap().info.pages[0].clone();
        // Displayed boxes: a 40 pt checkmark centred at (50, 50), an 80 × 8 pt line at (130, 50).
        for (mark, vx, w, h) in [(FillMark::Check, 50.0, 40.0, 40.0), (FillMark::Line, 130.0, 80.0, 8.0)] {
            let shape = Shape::Mark { rect: displayed_rect(&info, vx, 50.0, w, h), mark };
            session
                .apply(
                    id,
                    Edit::AddAnnotation(NewAnnotation {
                        page: 0,
                        style: Style::default_for(&shape),
                        shape,
                        contents: String::new(),
                        author: "Test".into(),
                    }),
                )
                .unwrap();
        }
        let check = |session: &Session, id: DocId| {
            let rendered = render_page(session, id);
            // Dark pixels in a displayed box, rendered at scale 2.
            let ink = |x0: u32, y0: u32, x1: u32, y1: u32| -> Vec<(u32, u32)> {
                (y0 * 2..y1 * 2)
                    .flat_map(|y| (x0 * 2..x1 * 2).map(move |x| (x, y)))
                    .filter(|&(x, y)| stamp_pixel(&rendered, x, y)[..3].iter().all(|c| *c < 128))
                    .collect()
            };
            let tick = ink(30, 30, 70, 70);
            assert!(!tick.is_empty(), "rotation {degrees}: no checkmark drawn");
            let top = tick.iter().min_by_key(|p| p.1).unwrap();
            let bottom = tick.iter().max_by_key(|p| p.1).unwrap();
            // The long stroke ends at the top right; the short one meets it low, left of centre.
            assert!(top.0 > bottom.0 + 20, "rotation {degrees}: highest ink {top:?} should be right of the lowest {bottom:?}");
            assert!(bottom.0 < 100 && top.0 > 110, "rotation {degrees}: checkmark {bottom:?} .. {top:?} is not upright");
            let line = ink(85, 40, 175, 60);
            assert!(!line.is_empty(), "rotation {degrees}: no line drawn");
            let (x0, x1) = (line.iter().map(|p| p.0).min().unwrap(), line.iter().map(|p| p.0).max().unwrap());
            let (y0, y1) = (line.iter().map(|p| p.1).min().unwrap(), line.iter().map(|p| p.1).max().unwrap());
            assert!((x1 - x0) > 10 * (y1 - y0 + 1), "rotation {degrees}: line {x0}..{x1} x {y0}..{y1} should run across");
        };
        check(&session, id);
        let saved = session.save_bytes(id).unwrap();
        let mut reopened = Session::new().with_clock(|| 1_700_000_000);
        let again = reopened.open("drawing.pdf", None, saved, None).unwrap();
        check(&reopened, again);
    }
}

/// Fill & Sign text and text boxes read across as displayed on turned pages (scanned forms are
/// often stored sideways with `/Rotate`), including after save.
#[test]
fn typed_text_reads_across_as_displayed_on_rotated_pages() {
    for degrees in [0, 90, 180, 270] {
        let (mut session, id) = rotated_blank(degrees);
        let info = session.get(id).unwrap().info.pages[0].clone();
        // Displayed top-left of the text at (40, 60): a wide first line, then one letter below
        // its start.
        let at = info.view_to_user(40.0, 60.0);
        let at = [f64::from(at[0]), f64::from(at[1])];
        let rect = text_box_at(degrees, at, 120.0, 2.0 * 12.0 * 1.2 + 6.0).unwrap();
        let shape = Shape::Typewriter { rect, font_size: 12.0 };
        let style = Style::default_for(&shape);
        session
            .apply(
                id,
                Edit::AddAnnotation(NewAnnotation {
                    page: 0,
                    style,
                    shape,
                    contents: "MMMMMMMM
I"
                    .into(),
                    author: "Test".into(),
                }),
            )
            .unwrap();
        let check = |session: &Session, id: DocId| {
            let rendered = render_page(session, id);
            let dark = |x: u32, y: u32| stamp_pixel(&rendered, x, y)[..3].iter().all(|c| *c < 128);
            let ink: Vec<(u32, u32)> =
                (0..rendered.height).flat_map(|y| (0..rendered.width).map(move |x| (x, y))).filter(|&(x, y)| dark(x, y)).collect();
            assert!(!ink.is_empty(), "rotation {degrees}: no text drawn");
            let (x0, x1) = (ink.iter().map(|p| p.0).min().unwrap(), ink.iter().map(|p| p.0).max().unwrap());
            let (y0, y1) = (ink.iter().map(|p| p.1).min().unwrap(), ink.iter().map(|p| p.1).max().unwrap());
            // Rendered at scale 2: the text starts at the clicked point and runs to the right.
            assert!((x1 - x0) > 2 * (y1 - y0), "rotation {degrees}: ink {x0}..{x1} x {y0}..{y1} should be wide, not tall");
            assert!(x0.abs_diff(80) < 16 && y0.abs_diff(120) < 16, "rotation {degrees}: text starts at ({x0}, {y0}), not near (80, 120)");
            // The lone "I" is on the second line under the start of the first, not upside down
            // (above the end).
            let mid = (y0 + y1) / 2;
            let lower: Vec<u32> = ink.iter().filter(|p| p.1 > mid + 2).map(|p| p.0).collect();
            assert!(!lower.is_empty() && lower.iter().all(|x| *x < x0 + (x1 - x0) / 4), "rotation {degrees}: second line is not under the start");
        };
        check(&session, id);
        let saved = session.save_bytes(id).unwrap();
        let mut reopened = Session::new().with_clock(|| 1_700_000_000);
        let again = reopened.open("drawing.pdf", None, saved, None).unwrap();
        check(&reopened, again);
    }
}

/// #1398: an explicit rectangle keeps the caller's box and turns the picture upright.
#[test]
fn explicit_image_stamps_keep_displayed_orientation_on_rotated_pages() {
    let bytes = quadrant_png();
    for degrees in [0, 90, 180, 270] {
        let (mut session, id) = rotated_blank(degrees);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let rect = displayed_rect(&info, 100.0, 100.0, 80.0, 40.0);
        session
            .apply(
                id,
                Edit::AddCustomStamp {
                    page: 0,
                    rect,
                    name: "Original quadrants".into(),
                    file: MarkFile { name: "quadrants.png".into(), bytes: bytes.clone(), page: 0 },
                    author: "Test".into(),
                },
            )
            .unwrap();
        assert_upright_quadrants(&session, id, degrees);
        session.apply(id, Edit::StyleAnnotation { page: 0, index: 0, color: None, opacity: Some(1.0), width: None, endings: None }).unwrap();
        assert_upright_quadrants(&session, id, degrees);
    }
}

/// A visible digital signature reads upright as the page is displayed on every page rotation:
/// its picture, alone in the box, shows its quadrants where they are in the image.
#[test]
fn visible_signatures_keep_displayed_orientation_on_rotated_pages() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let picture = image::load_from_memory(&quadrant_png()).unwrap().to_rgba8();
    let graphic = sign::Graphic::Image { width: picture.width(), height: picture.height(), rgba: Arc::new(picture.into_raw()) };
    let bare = sign::Appearance { name: false, date: false, reason: false, location: false, distinguished_name: false, labels: false };
    for degrees in [0, 90, 180, 270] {
        // After the test certificate's start date (2026-10-02).
        let mut session = Session::new().with_clock(|| 1_800_000_000);
        let blank = session.create_blank(200.0, 300.0, 1).unwrap();
        let id = session.open_new("contract.pdf", blank).unwrap();
        session.apply(id, Edit::RotatePages { pages: vec![0], degrees }).unwrap();
        let info = session.get(id).unwrap().info.pages[0].clone();
        // The picture's proportions (2:1) as displayed, centred at (100, 100).
        let rect = displayed_rect(&info, 100.0, 100.0, 80.0, 40.0);
        let opts = SignOptions { page: 0, rect: Some(rect), graphic: Some(graphic.clone()), appearance: bare.clone(), ..SignOptions::default() };
        let signed = session.sign(id, &digital_id, opts).unwrap();
        let reopened = session.open("signed.pdf", None, signed, None).unwrap();
        assert_upright_quadrants(&session, reopened, degrees);
    }
}

/// #1398: a PDF page used as a stamp stays upright, at its natural size and in an explicit box.
#[test]
fn pdf_page_stamps_keep_displayed_orientation_on_rotated_pages() {
    let bytes = Arc::new(half_red_page());
    for (natural, degrees) in [true, false].into_iter().flat_map(|natural| [0, 90, 180, 270].map(|degrees| (natural, degrees))) {
        let (mut session, id) = rotated_blank(degrees);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let at = info.view_to_user(100.0, 100.0).map(f64::from);
        let rect = if natural { [at[0], at[1], at[0], at[1]] } else { displayed_rect(&info, 100.0, 100.0, 80.0, 40.0) };
        session
            .apply(
                id,
                Edit::AddCustomStamp {
                    page: 0,
                    rect,
                    name: "Half red".into(),
                    file: MarkFile { name: "half.pdf".into(), bytes: bytes.clone(), page: 0 },
                    author: "Test".into(),
                },
            )
            .unwrap();
        let doc = session.get(id).unwrap();
        let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
        let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        assert!(rendered.error.is_none(), "{:?}", rendered.error);
        let left = stamp_pixel(&rendered, 80, 100);
        let right = stamp_pixel(&rendered, 120, 100);
        assert!(left[0] > 200 && left[1] < 40, "rotation {degrees}, natural {natural}: left {left:?} should be red");
        assert!(right.iter().all(|c| *c > 240), "rotation {degrees}, natural {natural}: right {right:?} should be paper");
    }
}

/// #4077: a PDF page stamped into a box of another shape is fitted inside it, keeping its shape
/// and centred, and stays vector: a form XObject, not a picture of it.
#[test]
fn pdf_page_stamps_fit_inside_an_explicit_box() {
    let bytes = Arc::new(half_red_page());
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let blank = session.create_blank(595.0, 842.0, 1).unwrap();
    let id = session.open_new("text.pdf", blank).unwrap();
    // 483 pt wide (a text column) and 300 pt tall: the 80 × 40 page is 483 × 241.5, centred.
    let stamp = |rect| Edit::AddCustomStamp {
        page: 0,
        rect,
        name: "Diagram".into(),
        file: MarkFile { name: "diagram.pdf".into(), bytes: bytes.clone(), page: 0 },
        author: "Test".into(),
    };
    session.apply(id, stamp([56.0, 400.0, 539.0, 700.0])).unwrap();
    assert_eq!(session.get(id).unwrap().info.annotations[0].rect, [56.0, 429.25, 539.0, 670.75]);
    // A box too tall and narrow: the width limits it, and it is centred vertically.
    session.apply(id, stamp([100.0, 100.0, 140.0, 300.0])).unwrap();
    assert_eq!(session.get(id).unwrap().info.annotations[1].rect, [100.0, 190.0, 140.0, 210.0]);

    let doc = pdfcraft_cos::Document::open(session.save_bytes(id).unwrap()).unwrap();
    let page = &pdfcraft_model::pages(&doc)[0];
    let annots = doc.resolve(page.dict.get(b"Annots").unwrap());
    let first = annots.as_array().unwrap()[0].as_ref().unwrap();
    let annot = doc.get(first);
    let annot = annot.as_dict().unwrap();
    let pdfcraft_cos::Object::Stream(picture) = doc.get(annot.reference(b"PCPicture").unwrap()).as_ref().clone() else {
        panic!("the picture is a stream")
    };
    assert_eq!(picture.dict.name(b"Subtype"), Some(&b"Form"[..]), "the page is kept as a form XObject");
    let ap = doc.resolve(annot.get(b"AP").unwrap());
    let pdfcraft_cos::Object::Stream(normal) = doc.get(ap.as_dict().unwrap().reference(b"N").unwrap()).as_ref().clone() else {
        panic!("an appearance stream")
    };
    let bbox: Vec<f64> = normal.dict.get(b"BBox").unwrap().as_array().unwrap().iter().filter_map(pdfcraft_cos::Object::as_f64).collect();
    assert_eq!(bbox, [56.0, 429.25, 539.0, 670.75]);
    let content = String::from_utf8(normal.decoded().unwrap().to_vec()).unwrap();
    assert!(content.contains("/Pic Do") && !content.contains("BI"), "{content}");
}

/// #1297: a 3D model on a page keeps the whole box as its rectangle and draws a PDF poster as a
/// form XObject fitted inside it, keeping its shape and centred.
#[test]
fn a_3d_model_gets_a_fitted_vector_poster_and_a_file_that_is_not_one_is_refused() {
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let blank = session.create_blank(595.0, 842.0, 1).unwrap();
    let id = session.open_new("model.pdf", blank).unwrap();
    let mut prc = b"PRC".to_vec();
    prc.extend_from_slice(&[1; 64]);
    let add = |model: Vec<u8>| Edit::AddModel3d {
        page: 0,
        rect: [56.0, 400.0, 539.0, 700.0],
        model: Arc::new(model),
        poster: Some(MarkFile { name: "poster.pdf".into(), bytes: Arc::new(half_red_page()), page: 0 }),
        activation: Activation3d::default(),
        name: "Bracket".into(),
    };
    assert!(session.apply(id, add(b"%PDF-1.7 not a model".to_vec())).is_err());
    session.apply(id, add(prc)).unwrap();

    let doc = pdfcraft_cos::Document::open(session.save_bytes(id).unwrap()).unwrap();
    let page = &pdfcraft_model::pages(&doc)[0];
    let annots = doc.resolve(page.dict.get(b"Annots").unwrap());
    let [only] = annots.as_array().unwrap().as_slice() else { panic!("the refused model added nothing: {annots:?}") };
    let annot = doc.get(only.as_ref().unwrap());
    let annot = annot.as_dict().unwrap();
    assert_eq!(annot.name(b"Subtype"), Some(&b"3D"[..]));
    let rect: Vec<f64> = annot.get(b"Rect").unwrap().as_array().unwrap().iter().filter_map(pdfcraft_cos::Object::as_f64).collect();
    assert_eq!(rect, [56.0, 400.0, 539.0, 700.0], "the whole box, not the fitted picture");
    assert_eq!(doc.resolve(annot.get(b"3DA").unwrap()).as_dict().unwrap().name(b"A"), Some(&b"XA"[..]));
    let ap = doc.resolve(annot.get(b"AP").unwrap());
    let pdfcraft_cos::Object::Stream(normal) = doc.get(ap.as_dict().unwrap().reference(b"N").unwrap()).as_ref().clone() else {
        panic!("an appearance stream")
    };
    let pic = doc.resolve(normal.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let pic = doc.resolve(pic.get(b"XObject").unwrap()).as_dict().unwrap().reference(b"Pic").unwrap();
    let pdfcraft_cos::Object::Stream(picture) = doc.get(pic).as_ref().clone() else { panic!("the picture is a stream") };
    assert_eq!(picture.dict.name(b"Subtype"), Some(&b"Form"[..]), "the page is kept as a form XObject");
    // The 80 × 40 page in the 483 × 300 box: 483 × 241.5 from y 429.25.
    let content = String::from_utf8(normal.decoded().unwrap().to_vec()).unwrap();
    assert!(content.contains(" 56 429.25 cm /Pic Do"), "{content}");
}

/// A PDF poster whose page has no area is refused rather than drawn as nothing.
#[test]
fn a_3d_poster_page_without_area_is_refused() {
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let blank = session.create_blank(595.0, 842.0, 1).unwrap();
    let id = session.open_new("model.pdf", blank).unwrap();
    let flat = String::from_utf8(half_red_page()).unwrap().replace("[0 0 80 40]", "[0 0 80 0 ]");
    let add = Edit::AddModel3d {
        page: 0,
        rect: [56.0, 400.0, 539.0, 700.0],
        model: Arc::new(b"PRC rest".to_vec()),
        poster: Some(MarkFile { name: "flat.pdf".into(), bytes: Arc::new(flat.into_bytes()), page: 0 }),
        activation: Activation3d::default(),
        name: "Bracket".into(),
    };
    assert!(matches!(session.apply(id, add), Err(EditError::Source(_))));
    assert!(session.get(id).unwrap().info.annotations.is_empty(), "nothing was added");
}

/// Adding a 3D model is a comment change: refused without the comments permission, and only the
/// comments are rebuilt after it.
#[test]
fn a_3d_model_needs_the_comments_permission_and_changes_the_comments() {
    let edit = Edit::AddModel3d {
        page: 0,
        rect: [0.0, 0.0, 100.0, 100.0],
        model: Arc::new(b"PRC rest".to_vec()),
        poster: None,
        activation: Activation3d::default(),
        name: String::new(),
    };
    let p = pdfcraft_cos::Permissions { bits: 0, owner: false, revision: 3 };
    assert_eq!(check_permission(&edit, &p), Err(EditError::NotPermitted("comments")));
    assert_eq!(crate::history::scope_of(&edit), crate::history::Scope::Comments);
}

/// A file with its title and author only in XMP shows them in the document information and in
/// Document Properties ▸ Description (#2373).
#[test]
fn document_information_stated_only_in_xmp_is_shown() {
    let mut cos = pdfcraft_cos::Document::open(Arc::new(fixture(1))).unwrap();
    let xmp = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
               <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
               <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Quarterly report</rdf:li></rdf:Alt></dc:title>\
               <dc:creator><rdf:Seq><rdf:li>Ada</rdf:li></rdf:Seq></dc:creator></rdf:Description></rdf:RDF></x:xmpmeta>";
    let mut dict = pdfcraft_cos::Dict::new();
    dict.set(b"Type".to_vec(), pdfcraft_cos::Object::name("Metadata"));
    dict.set(b"Subtype".to_vec(), pdfcraft_cos::Object::name("XML"));
    let m = cos.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::from_raw(dict, xmp.as_bytes().to_vec())));
    let root = cos.root().unwrap();
    cos.update_dict(root, |c| c.set(b"Metadata".to_vec(), pdfcraft_cos::Object::Ref(m))).unwrap();
    let bytes = pdfcraft_cos::write_full(&cos, &pdfcraft_cos::SaveOptions::default()).unwrap();
    let mut s = Session::new();
    let id = s.open("xmp.pdf", None, Arc::new(bytes), None).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!((doc.info.title.as_deref(), doc.info.author.as_deref(), doc.info.subject.as_deref()), (Some("Quarterly report"), Some("Ada"), None));
    assert_eq!(doc.info_value("Title").as_deref(), Some("Quarterly report"));
    assert_eq!(doc.info_value("Author").as_deref(), Some("Ada"));

    // A full refresh (any structural edit, undo, redo) reads /Info again and must not lose them.
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!((doc.info.title.as_deref(), doc.info.author.as_deref()), (Some("Quarterly report"), Some("Ada")), "after an edit");
    s.undo(id).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!((doc.info.title.as_deref(), doc.info.author.as_deref()), (Some("Quarterly report"), Some("Ada")), "after undo");
}

#[test]
fn edits_update_view_data_and_mark_dirty() {
    let (mut s, id) = session_with(3);
    assert!(s.get(id).unwrap().editable());
    assert!(!s.get(id).unwrap().dirty);
    s.apply(id, Edit::DeletePages { pages: vec![1] }).unwrap();
    let doc = s.get(id).unwrap();
    assert!(doc.dirty);
    assert_eq!(doc.info.pages.len(), 2, "inspection refreshed");
    assert_eq!(doc.can_undo(), Some("Delete page"));
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 3"], "renderer refreshed");
}

/// A font of an Adobe CJK collection that a PDF does not embed is drawn with an installed face on
/// the viewer's pool only. An export of the same page, which can be saved, has nothing drawn with
/// an installed font (AGENTS.md §1.2, §1.4). Skipped where no such face is installed.
#[test]
fn installed_cjk_fonts_draw_in_the_viewer_but_not_in_an_export() {
    let content = "BT /F1 40 Tf 5 15 Td <7E419AD45B57> Tj ET";
    let pdf = format!(
        "%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 60] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length {} >> stream
{content}
endstream endobj
5 0 obj << /Type /Font /Subtype /Type0 /BaseFont /MSung-Light /Encoding /UniCNS-UCS2-H /DescendantFonts [6 0 R] >> endobj
6 0 obj << /Type /Font /Subtype /CIDFontType0 /BaseFont /MSung-Light /CIDSystemInfo << /Registry (Adobe) /Ordering (CNS1) /Supplement 2 >> /FontDescriptor 7 0 R /DW 1000 >> endobj
7 0 obj << /Type /FontDescriptor /FontName /MSung-Light /Flags 6 /FontBBox [0 -141 1000 859] /ItalicAngle 0 /Ascent 859 /Descent -141 /CapHeight 700 /StemV 80 >> endobj
trailer << /Root 1 0 R >>
%%EOF",
        content.len()
    );
    let mut s = Session::new();
    let id = s.open("cns1.pdf", None, Arc::new(pdf.into_bytes()), None).unwrap();
    let doc = s.get(id).unwrap();
    doc.renderer.set_queue(vec![pdfcraft_render::RenderRequest {
        page: 0,
        kind: pdfcraft_render::RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    }]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let viewer = loop {
        if let Some(page) = doc.renderer.try_recv() {
            assert!(page.error.is_none(), "{:?}", page.error);
            break page;
        }
        assert!(std::time::Instant::now() < deadline, "page render did not finish");
        std::thread::yield_now();
    };
    let inked = |rgba: &[u8]| rgba.as_chunks::<4>().0.iter().filter(|c| c[0] < 128).count();
    // Whether this machine has a face that can draw the page at all, asked of a renderer with the
    // fallback switched on by hand, so a viewer pool that lost it fails below instead of skipping.
    let probe = pdfcraft_render::PageRenderer::new(doc.display.clone(), pdfcraft_render::RenderConfig { system_fonts: true, ..Default::default() })
        .render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(probe.error.is_none(), "{:?}", probe.error);
    if inked(&probe.rgba) < 300 {
        eprintln!("no installed Traditional Chinese face (or PDFCRAFT_SYSTEM_FONTS=0): skipped");
        return;
    }
    assert!(inked(&viewer.rgba) > 300, "the viewer's pool draws it with the installed face ({} dark pixels)", inked(&viewer.rgba));
    let png = export::Exporter::new(doc).png(0, 72.0).unwrap();
    let exported = image::load_from_memory(&png).unwrap().into_rgba8();
    assert!(inked(exported.as_raw()) < 50, "an export draws nothing with an installed font ({} dark pixels)", inked(exported.as_raw()));

    // Reload rebuilds the pool when retaining a view preference. That path must keep
    // the display-only fallback too; otherwise Chinese disappears after a file refresh.
    let bytes = doc.bytes.clone();
    assert!(s.set_hide_comments(id, true));
    s.reload(id, bytes).unwrap();
    let doc = s.get(id).unwrap();
    assert!(doc.comments_hidden());
    doc.renderer.set_queue(vec![pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() }]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let reloaded = loop {
        if let Some(page) = doc.renderer.try_recv() {
            assert!(page.error.is_none(), "{:?}", page.error);
            break page;
        }
        assert!(std::time::Instant::now() < deadline, "reloaded page render did not finish");
        std::thread::yield_now();
    };
    assert!(inked(&reloaded.rgba) > 300, "reload must retain visible Chinese text");
}

#[test]
fn metadata_edit_keeps_the_renderer_without_changing_page_content() {
    let (mut s, id) = session_with(1);
    let request = pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Pixels, scale: 1.0, ..Default::default() };
    let render = |session: &Session| {
        let doc = session.get(id).unwrap();
        doc.renderer.set_queue(vec![request]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(page) = doc.renderer.try_recv() {
                assert!(page.error.is_none(), "{:?}", page.error);
                return (doc.renderer.stats(), page.rgba);
            }
            assert!(std::time::Instant::now() < deadline, "page render did not finish");
            std::thread::yield_now();
        }
    };

    let (before, before_pixels) = render(&s);
    assert_eq!(before.page_interpretations, 1);
    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Report".into() }).unwrap();
    assert_eq!(s.get(id).unwrap().info_value("Title").as_deref(), Some("Report"));
    assert_eq!(s.get(id).unwrap().renderer.stats().page_interpretations, 1, "a metadata edit keeps the renderer and its page interpretation");
    let (after, after_pixels) = render(&s);
    assert_eq!(after.page_interpretations, 2, "the same renderer serves the second request, so its count continues");
    assert_eq!(before_pixels, after_pixels, "document properties do not change page pixels");
}

#[test]
fn metadata_edits_and_history_reuse_the_render_pool() {
    let (mut s, id) = session_with(1);
    let pixels = shown(&s, id, 0).rgba;
    let doc = s.get(id).unwrap();
    let stats = doc.renderer.stats();
    let display = doc.display.clone();
    assert_eq!(stats.page_interpretations, 1);

    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Report".into() }).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.title.as_deref(), Some("Report"));
    assert_eq!(doc.renderer.stats(), stats);
    assert!(Arc::ptr_eq(&doc.display, &display));
    assert_eq!(pixels.as_ref().len(), 200 * 300 * 4);

    s.undo(id).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.title, None);
    assert_eq!(doc.renderer.stats(), stats);
    assert!(Arc::ptr_eq(&doc.display, &display));

    s.redo(id).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.title.as_deref(), Some("Report"));
    assert_eq!(doc.renderer.stats(), stats);
    assert!(Arc::ptr_eq(&doc.display, &display));
}

#[test]
fn field_and_comment_edits_still_rebuild_the_render_pool() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("form.pdf", None, Arc::new(scripted_form()), None).unwrap();
    let _ = shown(&s, id, 0);
    assert_eq!(s.get(id).unwrap().renderer.stats().page_interpretations, 1);

    s.apply(id, Edit::SetFieldValue { name: "qty".into(), value: FieldValue::Text("4".into()) }).unwrap();
    assert_eq!(s.get(id).unwrap().renderer.stats().page_interpretations, 0);
    let _ = shown(&s, id, 0);
    assert_eq!(s.get(id).unwrap().renderer.stats().page_interpretations, 1);

    let shape = Shape::Rectangle { rect: [20.0, 20.0, 60.0, 60.0] };
    s.apply(
        id,
        Edit::AddAnnotation(NewAnnotation { page: 0, style: Style::default_for(&shape), shape, contents: String::new(), author: "Test".into() }),
    )
    .unwrap();
    assert_eq!(s.get(id).unwrap().renderer.stats().page_interpretations, 0);
}

#[test]
fn undo_and_redo_restore_exact_states() {
    let (mut s, id) = session_with(3);
    let original = s.get(id).unwrap().bytes.clone();
    s.apply(id, Edit::MovePages { pages: vec![2], to: 0 }).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 90);
    assert_eq!(s.undo(id).unwrap().as_str(), "Rotate page");
    assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 0);
    assert_eq!(s.undo(id).unwrap().as_str(), "Move page");
    assert_eq!(s.get(id).unwrap().bytes, original, "undoing everything returns the original bytes");
    assert_eq!(s.undo(id), Err(EditError::NothingToUndo));
    assert_eq!(s.redo(id).unwrap().as_str(), "Move page");
    assert_eq!(page_texts(&s, id), ["Page 3", "Page 1", "Page 2"]);
    // A new edit clears the redo stack.
    s.apply(id, Edit::InsertBlankPage { at: 0, width: 612.0, height: 792.0 }).unwrap();
    assert_eq!(s.redo(id), Err(EditError::NothingToRedo));
    assert_eq!(s.get(id).unwrap().info.pages.len(), 4);
}

#[test]
fn failed_edit_changes_nothing() {
    let (mut s, id) = session_with(2);
    let before = s.get(id).unwrap().bytes.clone();
    let err = s.apply(id, Edit::DeletePages { pages: vec![0, 1] }).unwrap_err();
    assert_eq!(err, EditError::Organize(pdfcraft_organize::OrganizeError::WouldRemoveAllPages));
    let doc = s.get(id).unwrap();
    assert!(!doc.dirty);
    assert_eq!(doc.can_undo(), None);
    assert_eq!(doc.bytes, before);
}

#[test]
fn save_is_incremental_stamps_mod_date_and_rebases() {
    let (mut s, id) = session_with(2);
    let original = s.get(id).unwrap().bytes.clone();
    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Quarterly report".into() }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert_eq!(&saved[..original.len()], &original[..], "incremental save keeps the original bytes");
    let text = String::from_utf8_lossy(&saved[original.len()..]);
    assert!(text.contains("/ModDate (D:20231114221320Z)"), "mod date stamped: {text}");
    s.mark_saved(id, saved.clone(), Some("/tmp/out/report.pdf".into())).unwrap();
    let doc = s.get(id).unwrap();
    assert!(!doc.dirty);
    assert_eq!(doc.name, "report.pdf");
    assert_eq!(doc.info_value("Title").as_deref(), Some("Quarterly report"));
    assert_eq!(doc.info.title.as_deref(), Some("Quarterly report"));
    // Saving again without edits writes the same bytes; a later edit appends just one revision.
    assert_eq!(s.save_bytes(id).unwrap(), saved);
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: -90 }).unwrap();
    let second = s.save_bytes(id).unwrap();
    assert_eq!(&second[..saved.len()], &saved[..]);
    let reopened = pdfcraft_cos::Document::open(second).unwrap();
    assert_eq!(reopened.revisions().len(), 3);
}

/// #1937: a file whose cross-reference data had to be rebuilt saved as its original bytes when
/// nothing was edited, so the damage Document Properties reported as repaired was still in the
/// file. Save writes the rebuilt file in full; reopened, it needs no repair.
#[test]
fn a_repaired_file_is_rewritten_by_save_even_without_edits() {
    let good = fixture(2);
    // Every offset in the table is off by the inserted line: the chain reads but leads nowhere.
    let mut shifted = b"%PDF-1.7\n%shifted\n".to_vec();
    shifted.extend_from_slice(&good[b"%PDF-1.7\n".len()..]);
    // No cross-reference table at all.
    let mut lost = good.clone();
    lost.truncate(good.windows(9).rposition(|w| w == b"startxref").unwrap());
    lost.extend_from_slice(b"startxref\n999999\n%%EOF\n");
    for (name, damaged) in [("shifted", shifted), ("lost", lost)] {
        let mut s = Session::new().with_clock(|| 1_700_000_000);
        let id = s.open("broken.pdf", Some("/tmp/broken.pdf".into()), Arc::new(damaged.clone()), None).unwrap();
        let doc = s.get(id).unwrap();
        assert!(doc.repair_log().iter().any(|l| l.contains("rebuilt cross-reference index")), "{name}: {:?}", doc.repair_log());
        assert_eq!(doc.info.pages.len(), 2, "{name}");
        assert!(!doc.dirty, "{name}: a repair is not an edit");
        let saved = s.save_bytes(id).unwrap();
        assert_ne!(&saved[..], &damaged[..], "{name}: the damaged bytes are not what Save writes");
        let reopened = pdfcraft_cos::Document::open(saved.clone()).unwrap();
        assert!(reopened.repair_log().is_empty(), "{name}: {:?}", reopened.repair_log());
        assert_eq!(reopened.revisions().len(), 1, "{name}");
        s.mark_saved(id, saved.clone(), None).unwrap();
        let doc = s.get(id).unwrap();
        assert!(doc.repair_log().is_empty(), "{name}: {:?}", doc.repair_log());
        assert_eq!(doc.info.pages.len(), 2, "{name}");
        // Sound now: saving again without edits writes the same bytes.
        assert_eq!(s.save_bytes(id).unwrap(), saved, "{name}");
    }
}

/// A damaged file that is signed keeps its bytes on a Save without edits: rewriting it in full
/// to put the repair on disk would invalidate its signatures without a word.
#[test]
fn a_signed_repaired_file_keeps_its_bytes_when_saved_without_edits() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    // After the test certificate's start date (2026-10-02).
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(1)), None).unwrap();
    let opts = SignOptions { page: 0, rect: Some([20.0, 20.0, 180.0, 60.0]), ..SignOptions::default() };
    let signed = s.sign(id, &digital_id, opts).unwrap();
    // A trailing `startxref` that points nowhere: the signed revision's bytes are untouched, but
    // the file's cross-reference data has to be rebuilt.
    let mut damaged = signed.to_vec();
    damaged.extend_from_slice(b"\nstartxref\n999999\n%%EOF\n");
    let mut t = Session::new().with_clock(|| 1_800_000_000);
    let tid = t.open("damaged.pdf", Some("/tmp/damaged.pdf".into()), Arc::new(damaged.clone()), None).unwrap();
    let doc = t.get(tid).unwrap();
    assert!(doc.repair_log().iter().any(|l| l.contains("rebuilt cross-reference index")), "{:?}", doc.repair_log());
    assert!(doc.is_signed());
    assert_eq!(&t.save_bytes(tid).unwrap()[..], &damaged[..], "a signed file is not rewritten without edits");
}

#[test]
fn undo_still_works_after_save() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::DeletePages { pages: vec![0] }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved, None).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    s.undo(id).unwrap();
    let doc = s.get(id).unwrap();
    assert!(!doc.dirty, "undo back to the saved file is clean");
    assert_eq!(doc.info.pages.len(), 1);
    assert_eq!(doc.info.pages[0].rotation, 0);
    s.undo(id).unwrap();
    let doc = s.get(id).unwrap();
    assert!(doc.dirty, "undoing past a save makes the document dirty again");
    assert_eq!(doc.info.pages.len(), 2);
}

#[test]
fn undo_to_the_opened_file_is_not_dirty() {
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let opened = Arc::new(fixture(2));
    let id = session.open("existing.pdf", None, opened.clone(), None).unwrap();
    assert!(!session.get(id).unwrap().dirty);
    assert!(session.autosave_snapshots().is_empty());

    session.apply(id, Edit::DeletePages { pages: vec![0] }).unwrap();
    assert_eq!(session.get(id).unwrap().info.pages.len(), 1);
    session.undo(id).unwrap();
    let document = session.get(id).unwrap();
    assert!(!document.dirty, "the opened bytes are the saved version");
    assert_eq!(document.bytes, opened);
    assert_eq!(document.info.pages.len(), 2);
    assert!(session.autosave_snapshots().is_empty(), "nothing unsaved to recover");

    session.redo(id).unwrap();
    assert!(session.get(id).unwrap().dirty);
    session.undo(id).unwrap();
    assert!(!session.get(id).unwrap().dirty);
}

#[test]
fn save_after_undo_retains_the_latest_saved_revision() {
    let (mut session, document) = session_with(3);
    session.apply(document, Edit::DeletePages { pages: vec![1] }).unwrap();
    let deleted = session.save_bytes(document).unwrap();
    session.mark_saved(document, deleted, None).unwrap();
    session.apply(document, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let rotated = session.save_bytes(document).unwrap();
    session.mark_saved(document, rotated.clone(), None).unwrap();
    let rotated_revisions = pdfcraft_cos::Document::open(rotated.clone()).unwrap().revisions().len();

    session.undo(document).unwrap();
    session.apply(document, Edit::SetInfo { key: "Title".into(), value: "Report".into() }).unwrap();
    let saved = session.save_bytes(document).unwrap();
    assert!(saved.starts_with(rotated.as_slice()), "the saved rotation stays in the file");
    let reopened = pdfcraft_cos::Document::open(saved.clone()).unwrap();
    assert!(reopened.revisions().len() > rotated_revisions, "the title is a newer revision, not a rewrite of the earlier one");
    assert_eq!(pdfcraft_organize::info(&reopened, "Title").as_deref(), Some("Report"));

    session.mark_saved(document, saved, None).unwrap();
    let doc = session.get(document).unwrap();
    assert_eq!(page_texts(&session, document), ["Page 1", "Page 3"]);
    assert_eq!(doc.info.pages[0].rotation, 0, "the undone rotation is the current page");
    assert_eq!(doc.info_value("Title").as_deref(), Some("Report"));
}

#[test]
fn revert_preserves_the_saved_pages_after_undo() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::DeletePages { pages: vec![0] }).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let saved_bytes = s.save_bytes(id).unwrap();
    s.mark_saved(id, saved_bytes, None).unwrap();
    s.undo(id).unwrap();
    assert!(s.get(id).unwrap().dirty);
    assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 0);

    s.revert(id).unwrap();
    let d = s.get(id).unwrap();
    assert!(!d.dirty && d.can_undo().is_none() && d.can_redo().is_none());
    assert_eq!(page_texts(&s, id), ["Page 2"], "the last saved pages, not the file as opened");
    assert_eq!(d.info.pages[0].rotation, 90);
}

#[test]
fn full_save_is_readable_and_smaller_after_deletes() {
    let (mut s, id) = session_with(5);
    let original = s.save_bytes(id).unwrap();
    s.apply(id, Edit::DeletePages { pages: vec![0, 1, 2] }).unwrap();
    let full = s.save_full_bytes(id).unwrap();
    assert!(full.len() < original.len());
    let mut s2 = Session::new();
    let id2 = s2.open("full.pdf", None, full, None).unwrap();
    assert_eq!(page_texts(&s2, id2), ["Page 4", "Page 5"]);
}

#[test]
fn layer_choices_survive_edits() {
    // No layers in the fixture: just make sure refresh keeps working with an empty list and
    // that unknown layers are rejected.
    let (mut s, id) = session_with(1);
    assert!(!s.set_layer_visible(id, 0, false));
    s.apply(id, Edit::InsertBlankPage { at: 1, width: 100.0, height: 100.0 }).unwrap();
    assert_eq!(s.get(id).unwrap().info.pages.len(), 2);
}

#[test]
fn set_layer_actions_toggle_in_order_and_keep_radio_groups() {
    use pdfcraft_render::LayerOp::{Off, On, Toggle};
    let layer = |n: u32, visible: bool| Layer { id: (n, 0), name: format!("L{n}"), visible, initial: visible };
    let run = |changes: &[(LayerOp, u32)], preserve_rb: bool| {
        // L1 and L2 are a radio-button group; L3 is on its own.
        let mut layers = vec![layer(1, true), layer(2, false), layer(3, true)];
        let changes: Vec<_> = changes.iter().map(|&(op, n)| (op, (n, 0))).collect();
        let changed = apply_layer_state(&mut layers, &[vec![(1, 0), (2, 0)]], &changes, preserve_rb);
        (changed, layers.iter().map(|l| l.visible).collect::<Vec<_>>())
    };
    // Turning a layer on turns off the rest of its group, unless the action says not to.
    assert_eq!(run(&[(On, 2)], true), (true, vec![false, true, true]));
    assert_eq!(run(&[(On, 2)], false), (true, vec![true, true, true]));
    // A toggle that turns a layer on does the same.
    assert_eq!(run(&[(Toggle, 2)], true), (true, vec![false, true, true]));
    // Turning a layer off leaves its group alone.
    assert_eq!(run(&[(Off, 1)], true), (true, vec![false, false, true]));
    // Each toggle sees the state the changes before it left: twice is no change.
    assert_eq!(run(&[(Toggle, 3), (Toggle, 3)], true), (false, vec![true, false, true]));
    // Later changes win, and groups that aren't layers are skipped.
    assert_eq!(run(&[(On, 1), (On, 2), (Off, 99)], true), (true, vec![false, true, true]));
    // What counts is the end state: on and back off again is no change.
    assert_eq!(run(&[(On, 2), (On, 1), (Off, 99)], true), (false, vec![true, false, true]));
}

#[test]
fn set_layer_actions_change_what_renders() {
    // A red left half on layer Red (on) and a green right half on layer Green (off); the two
    // are a radio-button group.
    let content = "/OC /R BDC 1 0 0 rg 0 0 50 100 re f EMC /OC /G BDC 0 1 0 rg 50 0 50 100 re f EMC";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] /D << /OFF [6 0 R] /RBGroups [[5 0 R 6 0 R]] >> >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources << /Properties << /R 5 0 R /G 6 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /OCG /Name (Red) >>".into(),
        "<< /Type /OCG /Name (Green) >>".into(),
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
    let id = s.open("layers.pdf", None, Arc::new(pdf), None).unwrap();
    // The colours at the middle of the left and right halves, as the document draws now.
    let halves = |s: &Session| {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
        let p = r.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Pixels, scale: 1.0, ..Default::default() });
        let at = |x: usize| {
            let i = (50 * p.width as usize + x) * 4;
            p.rgba[i..i + 3].to_vec()
        };
        (at(25), at(75))
    };
    let (red, green, white) = (vec![255, 0, 0], vec![0, 255, 0], vec![255, 255, 255]);
    assert_eq!(halves(&s), (red.clone(), white.clone()));
    // Showing Green hides Red, its radio-button partner.
    assert!(s.set_layer_state(id, &[(LayerOp::On, (6, 0))], true));
    let visible: Vec<_> = s.get(id).unwrap().info.layers.iter().map(|l| (l.name.as_str(), l.visible)).collect();
    assert_eq!(visible, [("Red", false), ("Green", true)]);
    assert_eq!(halves(&s), (white.clone(), green));
    // Running it again changes nothing.
    assert!(!s.set_layer_state(id, &[(LayerOp::On, (6, 0))], true));
    // Turning Green off brings nothing back on.
    assert!(s.set_layer_state(id, &[(LayerOp::Off, (6, 0))], true));
    assert_eq!(halves(&s), (white.clone(), white));
    assert!(!s.set_layer_state(DocId(999), &[(LayerOp::On, (5, 0))], true));
}

#[test]
fn reset_layers_returns_to_the_files_visibility() {
    // Red (left half, on) and Green (right half, off) are a radio-button group.
    let content = "/OC /R BDC 1 0 0 rg 0 0 50 100 re f EMC /OC /G BDC 0 1 0 rg 50 0 50 100 re f EMC";
    let pdf = pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] /D << /OFF [6 0 R] /RBGroups [[5 0 R 6 0 R]] >> >> >>")
        .obj("<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
        .obj("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources << /Properties << /R 5 0 R /G 6 0 R >> >> >>")
        .stream("", content)
        .obj("<< /Type /OCG /Name (Red) >>")
        .obj("<< /Type /OCG /Name (Green) >>")
        .build();
    // A page drawing on layer Blue, which that file has off.
    let other = pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >> >>")
        .obj("<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
        .obj("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources << /Properties << /B 5 0 R >> >> >>")
        .stream("", "/OC /B BDC 0 0 1 rg 0 0 10 10 re f EMC")
        .obj("<< /Type /OCG /Name (Blue) >>")
        .build();
    let mut s = Session::new();
    let id = s.open("layers.pdf", None, Arc::new(pdf), None).unwrap();
    let shown = |s: &Session| s.get(id).unwrap().info.layers.iter().map(|l| (l.name.clone(), l.visible)).collect::<Vec<_>>();
    let left = |s: &Session| {
        let doc = s.get(id).unwrap();
        let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
        let p = r.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Pixels, scale: 1.0, ..Default::default() });
        let i = (50 * p.width as usize + 25) * 4;
        p.rgba[i..i + 3].to_vec()
    };
    assert!(s.set_layer_state(id, &[(LayerOp::On, (6, 0))], true));
    let edit = Edit::InsertPagesFrom {
        name: "b.pdf".into(),
        bytes: Arc::new(other),
        pages: None,
        at: 1,
        field_conflicts: pdfcraft_organize::FieldConflicts::Rename,
    };
    s.apply(id, edit).unwrap();
    let blue = s.get(id).unwrap().info.layers.iter().position(|l| l.name == "Blue").unwrap();
    assert!(s.set_layer_visible(id, blue, true));
    assert_eq!(shown(&s), [("Red".into(), false), ("Green".into(), true), ("Blue".into(), true)]);
    assert_eq!(left(&s), [255, 255, 255]);
    // Every layer, the inserted one too, goes back to the file's default configuration.
    assert!(s.reset_layers(id));
    assert_eq!(shown(&s), [("Red".into(), true), ("Green".into(), false), ("Blue".into(), false)]);
    assert_eq!(left(&s), [255, 0, 0]);
    assert!(!s.reset_layers(id), "already as the file opens");
    // What the eye sees is not an edit: Undo still undoes the insert.
    assert_eq!(s.get(id).unwrap().can_undo(), Some("Insert pages from b.pdf"));
    assert!(!s.reset_layers(DocId(999)));
}

#[test]
fn edits_to_unknown_documents_are_rejected() {
    let (mut s, _) = session_with(1);
    assert_eq!(s.apply(DocId(999), Edit::DeletePages { pages: vec![0] }), Err(EditError::NoDocument));
    assert_eq!(s.undo(DocId(999)), Err(EditError::NoDocument));
    assert!(s.save_bytes(DocId(999)).is_err());
}

#[test]
fn batch_is_one_undo_step_and_all_or_nothing() {
    let (mut s, id) = session_with(3);
    let batch = Edit::Batch {
        label: "Change properties".into(),
        edits: vec![Edit::SetInfo { key: "Title".into(), value: "T".into() }, Edit::SetInfo { key: "Author".into(), value: "A".into() }],
    };
    s.apply(id, batch).unwrap();
    assert_eq!(s.get(id).unwrap().info_value("Author").as_deref(), Some("A"));
    assert_eq!(s.undo(id).unwrap().as_str(), "Change properties");
    assert_eq!(s.get(id).unwrap().info_value("Title"), None);
    assert_eq!(s.get(id).unwrap().can_undo(), None, "one step");
    // A failing member rolls back the whole batch.
    let bad = Edit::Batch { label: "x".into(), edits: vec![Edit::RotatePages { pages: vec![0], degrees: 90 }, Edit::DeletePages { pages: vec![9] }] };
    assert!(s.apply(id, bad).is_err());
    assert_eq!(s.get(id).unwrap().info.pages[0].rotation, 0);
}

#[test]
fn insert_blank_between_pages_keeps_both_neighbours() {
    let (mut s, id) = session_with(2);
    s.apply(id, Edit::InsertBlankPage { at: 1, width: 200.0, height: 300.0 }).unwrap();
    assert_eq!(page_texts(&s, id), ["Page 1", "", "Page 2"]);
}

#[test]
fn combine_extract_split_and_insert_from_file() {
    let (mut s, id) = session_with(3);
    let other = Arc::new(fixture(2));
    // Combine: the first file then the second.
    let combined = s.combine(&[("a.pdf".into(), Arc::new(fixture(3))), ("b.pdf".into(), other.clone())]).unwrap();
    let cid = s.open_new("Combined.pdf", combined).unwrap();
    assert_eq!(page_texts(&s, cid), ["Page 1", "Page 2", "Page 3", "Page 1", "Page 2"]);
    assert!(s.get(cid).unwrap().dirty, "a new document starts unsaved");
    assert_eq!(s.get(cid).unwrap().info.outline.len(), 2, "one bookmark per file");
    // Extract pages 3 and 1 into a new document.
    let ex = s.extract(id, &[2, 0]).unwrap();
    let eid = s.open_new("Extract.pdf", ex).unwrap();
    assert_eq!(page_texts(&s, eid), ["Page 3", "Page 1"]);
    // Split every 2 pages.
    let parts = s.split(id, &pdfcraft_organize::SplitBy::PageCount(2)).unwrap();
    assert_eq!(parts.iter().map(|(a, b, _)| (*a, *b)).collect::<Vec<_>>(), [(1, 2), (3, 3)]);
    // Insert pages from a file, undoably, into the open document.
    s.apply(
        id,
        Edit::InsertPagesFrom {
            name: "b.pdf".into(),
            bytes: other,
            pages: Some(vec![1]),
            at: 1,
            field_conflicts: pdfcraft_organize::FieldConflicts::Rename,
        },
    )
    .unwrap();
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2", "Page 2", "Page 3"]);
    assert_eq!(s.undo(id).unwrap().as_str(), "Insert pages from b.pdf");
    assert_eq!(page_texts(&s, id).len(), 3);
    // Garbage sources fail cleanly.
    let bad = s.apply(
        id,
        Edit::InsertPagesFrom {
            name: "junk.pdf".into(),
            bytes: Arc::new(b"nope".to_vec()),
            pages: None,
            at: 0,
            field_conflicts: pdfcraft_organize::FieldConflicts::Rename,
        },
    );
    assert!(matches!(bad, Err(EditError::Source(_))));
}

/// Inserting pages of a file whose form names a field as the open document's: the incoming
/// Duplicating a page copies the document's own field: the copy keeps its name (Duplicate is
/// not Combine), and nothing about renaming lands in the repair log.
#[test]
fn duplicating_a_page_with_a_field_keeps_the_field_name() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("a.pdf", None, Arc::new(form_with_field("Text1")), None).unwrap();
    s.apply(id, Edit::DuplicatePages { pages: vec![0], at: None }).unwrap();
    let doc = s.get(id).unwrap();
    assert_eq!(doc.info.pages.len(), 2);
    assert!(doc.form.iter().all(|f| f.name == "Text1"), "{:?}", doc.form.iter().map(|f| &f.name).collect::<Vec<_>>());
    assert!(!doc.repair_log().iter().any(|l| l.contains("Text1-1")), "{:?}", doc.repair_log());
}

/// field is renamed to keep the two separate, the repair log says so (warning that scripts
/// were not rewritten), the undo step's label counts the renames, and undo puts the names back.
#[test]
fn insert_from_file_renames_colliding_fields_and_notes_it() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("a.pdf", None, Arc::new(form_with_field("Text1")), None).unwrap();
    s.apply(
        id,
        Edit::InsertPagesFrom {
            name: "b.pdf".into(),
            bytes: Arc::new(form_with_field("Text1")),
            pages: None,
            at: 1,
            field_conflicts: pdfcraft_organize::FieldConflicts::Rename,
        },
    )
    .unwrap();
    let doc = s.get(id).unwrap();
    let names: Vec<&str> = doc.form.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Text1", "Text1-1"]);
    assert!(
        doc.repair_log().iter().any(|l| l.contains("Text1 → Text1-1") && l.contains("were not rewritten")),
        "the note names the rename and the scripts caveat: {:?}",
        doc.repair_log()
    );
    let label = s.undo(id).unwrap();
    assert_eq!(label.as_str(), "Insert pages from b.pdf — 1 field renamed");
    let doc = s.get(id).unwrap();
    let names: Vec<&str> = doc.form.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Text1"], "undo restores the single original field");
}

/// A combine reports the fields it renamed — the tool result tells callers what their scripts
/// must call the field now — and honours a caller's choice to merge instead.
#[test]
fn combine_reports_renamed_fields_and_can_merge() {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let sources: Vec<CombineSource> =
        vec![("a.pdf".into(), Arc::new(form_with_field("Text1")), None), ("b.pdf".into(), Arc::new(form_with_field("Text1")), None)];
    let out = s.combine_unlocked_reporting(&sources, &[], pdfcraft_organize::FieldConflicts::Rename).unwrap();
    assert_eq!(out.renamed_fields, [("Text1".to_string(), "Text1-1".to_string())]);
    let cid = s.open_new("Combined.pdf", out.bytes).unwrap();
    let doc = s.get(cid).unwrap();
    let names: Vec<&str> = doc.form.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Text1", "Text1-1"]);
    let merged = s.combine_unlocked_reporting(&sources, &[], pdfcraft_organize::FieldConflicts::Merge).unwrap();
    assert!(merged.renamed_fields.is_empty(), "merge renames nothing");
}

/// The Combine tab's pre-flight over byte sources: what combining them, in order, would rename
/// in each. A file that cannot be opened previews as nothing (its row already shows the problem).
#[test]
fn combine_field_conflicts_previews_over_bytes() {
    let s = Session::new().with_clock(|| 1_700_000_000);
    let sources: Vec<CombineSource> = vec![
        ("a.pdf".into(), Arc::new(form_with_field("Text1")), None),
        ("b.pdf".into(), Arc::new(form_with_field("Text1")), None),
        ("junk.pdf".into(), Arc::new(b"nope".to_vec()), None),
    ];
    let preview = s.combine_field_conflicts(&sources, &[]);
    assert_eq!(preview, vec![vec![], vec![("Text1".to_string(), "Text1-1".to_string())], vec![]]);
}

#[test]
fn mixed_files_convert_and_combine_in_order() {
    let mut s = Session::new();
    let pdf = Arc::new(fixture(2));
    let (kind, same) = s.convert_to_pdf("a.pdf", &pdf).unwrap();
    assert_eq!(kind, SourceKind::Pdf);
    assert!(Arc::ptr_eq(&same, &pdf), "a PDF is passed through untouched");
    let (kind, image) = s.convert_to_pdf("scan.jpg", &Arc::new(jpeg_bytes())).unwrap();
    assert_eq!(kind, SourceKind::Image);
    let (kind, text) = s.convert_to_pdf("notes.txt", &Arc::new(b"hello".to_vec())).unwrap();
    assert_eq!(kind, SourceKind::Text);
    let combined = s.combine_ranges(&[("notes".into(), text, None), ("a".into(), pdf, Some("2".into())), ("scan".into(), image, None)]).unwrap();
    let id = s.open_new("Combined.pdf", combined).unwrap();
    assert_eq!(page_texts(&s, id), ["hello", "Page 2", ""]);
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["notes→1", "a→2", "scan→3"]);
}

#[test]
fn utf16_and_code_page_text_files_convert_to_their_text() {
    let mut s = Session::new();
    // Notepad's "Unicode": UTF-16 little endian with a byte order mark.
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend("Grüße aus Köln".encode_utf16().flat_map(u16::to_le_bytes));
    let (_, pdf) = s.convert_to_pdf("notes.txt", &Arc::new(utf16)).unwrap();
    let id = s.open_new("notes.pdf", pdf).unwrap();
    assert_eq!(page_texts(&s, id), ["Grüße aus Köln"]);
    // A UTF-8 byte order mark is not text; Windows-1252 bytes are not replacement characters.
    let (_, pdf) = s.convert_to_pdf("menu.txt", &Arc::new(b"\xEF\xBB\xBFCaf\xC3\xA9".to_vec())).unwrap();
    let id = s.open_new("menu.pdf", pdf).unwrap();
    assert_eq!(page_texts(&s, id), ["Café"]);
    let (_, pdf) = s.convert_to_pdf("old.txt", &Arc::new(b"Cr\xE8me br\xFBl\xE9e".to_vec())).unwrap();
    let id = s.open_new("old.pdf", pdf).unwrap();
    assert_eq!(page_texts(&s, id), ["Crème brûlée"]);
}

#[test]
fn a_file_split_around_another_combines_in_order_with_one_bookmark() {
    let mut s = Session::new();
    let (a, b) = (Arc::new(fixture(3)), Arc::new(fixture(2)));
    // a's pages 1–2, then b, then a's page 3: one file (group 7) split around another.
    let sources: Vec<CombineSource> = vec![("a".into(), a.clone(), Some("1-2".into())), ("b".into(), b, None), ("a".into(), a, Some("3".into()))];
    let combined = s.combine_grouped(&sources, &[7, 9, 7], &[]).unwrap();
    let id = s.open_new("Combined.pdf", combined).unwrap();
    assert_eq!(page_texts(&s, id), ["Page 1", "Page 2", "Page 1", "Page 2", "Page 3"]);
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["a→1", "b→3"], "one bookmark for a, at its first page");
    // Without groups each source is a file of its own, as combine_ranges.
    let (a, b) = (Arc::new(fixture(3)), Arc::new(fixture(2)));
    let sources: Vec<CombineSource> = vec![("a".into(), a.clone(), Some("1".into())), ("b".into(), b, None), ("a".into(), a, Some("3".into()))];
    let combined = s.combine_grouped(&sources, &[], &[]).unwrap();
    let id = s.open_new("Combined 2.pdf", combined).unwrap();
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["a→1", "b→2", "a→4"]);
    // A bad range is refused with the file's name.
    let bad: Vec<CombineSource> = vec![("a".into(), Arc::new(fixture(1)), Some("9".into()))];
    assert!(matches!(s.combine_grouped(&bad, &[1], &[]), Err(EditError::Print(e)) if e.starts_with("a:")));
}

#[test]
fn create_multiple_reads_each_source_only_after_the_previous_one_is_released() {
    let mut s = Session::new();
    let mut earlier: Vec<std::sync::Weak<Vec<u8>>> = Vec::new();
    let texts = ["Alpha", "Beta", "Gamma"];
    let sources = texts.iter().map(|text| {
        // The combine has finished with every earlier source, so their bytes are gone by now.
        assert!(earlier.iter().all(|w| w.upgrade().is_none()), "a source is released before the next is read");
        let (_, pdf) = s.convert_to_pdf(&format!("{text}.txt"), &Arc::new(text.as_bytes().to_vec())).unwrap();
        earlier.push(Arc::downgrade(&pdf));
        Ok::<CombineSource, EditError>((text.to_string(), pdf, None))
    });
    let combined = s.combine_each(sources).unwrap().unwrap();
    assert!(earlier.iter().all(|w| w.upgrade().is_none()));
    let id = s.open_new("Combined.pdf", combined).unwrap();
    assert_eq!(page_texts(&s, id), ["Alpha", "Beta", "Gamma"]);
    assert_eq!(outline_titles(&s.get(id).unwrap().info.outline), ["Alpha→1", "Beta→2", "Gamma→3"]);
}

#[test]
fn create_multiple_gives_the_bytes_of_combine_ranges_and_refuses_a_bad_source() {
    // The two combines are compared byte for byte, and each stamps the clock: pin it.
    let s = Session::new().with_clock(|| 1_700_000_000);
    let (a, b) = (Arc::new(fixture(3)), Arc::new(fixture(2)));
    let sources: Vec<CombineSource> =
        vec![("a".into(), a.clone(), Some("1-2".into())), ("b".into(), b, None), ("a".into(), a.clone(), Some("3".into()))];
    let by_ranges = s.combine_ranges(&sources).unwrap();
    let streamed = s.combine_each(sources.into_iter().map(Ok::<_, EditError>)).unwrap().unwrap();
    assert_eq!(*streamed, *by_ranges);
    // A bad range, or a file that isn't a PDF, stops the combine with an error naming that source.
    let bad_range: Vec<Result<CombineSource, EditError>> = vec![Ok(("a".into(), a.clone(), None)), Ok(("c".into(), a.clone(), Some("9".into())))];
    assert!(matches!(s.combine_each(bad_range), Err(EditError::Print(e)) if e.starts_with("c:")));
    let junk: Vec<Result<CombineSource, EditError>> = vec![Ok(("junk".into(), Arc::new(b"not a pdf".to_vec()), None))];
    assert!(matches!(s.combine_each(junk), Err(EditError::Source(m)) if m.starts_with("junk:")));
    // Nothing to combine is `None`, not an empty document.
    assert!(matches!(s.combine_each(Vec::<Result<CombineSource, EditError>>::new()), Ok(None)));
}

#[test]
fn a_page_with_a_form_field_shown_twice_keeps_one_field() {
    // Regression (found by review): the repeated page copied the whole field again, so the
    // result had two fields named "shared", filled in separately.
    use pdfcraft_cos::{Dict, Document, Object, PdfString};
    let mut doc = Document::open(Arc::new(fixture(2))).unwrap();
    let pages: Vec<_> = pdfcraft_organize::pages(&doc).unwrap().into_iter().map(|p| p.obj).collect();
    let field = doc.add(Object::Null);
    let widgets: Vec<_> = pages
        .iter()
        .map(|p| {
            let mut w = Dict::new();
            w.set(b"Type".to_vec(), Object::name("Annot"));
            w.set(b"Subtype".to_vec(), Object::name("Widget"));
            w.set(b"Rect".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(100), Object::Int(20)]));
            w.set(b"Parent".to_vec(), Object::Ref(field));
            w.set(b"P".to_vec(), Object::Ref(*p));
            doc.add(w)
        })
        .collect();
    let mut f = Dict::new();
    f.set(b"FT".to_vec(), Object::name("Tx"));
    f.set(b"T".to_vec(), Object::String(PdfString::text("shared")));
    f.set(b"V".to_vec(), Object::String(PdfString::text("Ada")));
    f.set(b"Kids".to_vec(), Object::Array(widgets.iter().copied().map(Object::Ref).collect()));
    doc.set(field, Object::Dict(f));
    for (p, w) in pages.iter().zip(&widgets) {
        doc.update_dict(*p, |d| d.set(b"Annots".to_vec(), Object::Array(vec![Object::Ref(*w)]))).unwrap();
    }
    let mut form = Dict::new();
    form.set(b"Fields".to_vec(), Object::Array(vec![Object::Ref(field)]));
    doc.update_dict(doc.root().unwrap(), |d| d.set(b"AcroForm".to_vec(), Object::Dict(form))).unwrap();
    let bytes = Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap());
    // Pages 1, 2, then page 1 again: one file split around nothing but itself.
    let s = Session::new();
    let sources: Vec<CombineSource> = vec![("form".into(), bytes.clone(), Some("1-2".into())), ("form".into(), bytes, Some("1".into()))];
    let out = s.combine_grouped(&sources, &[1, 1], &[]).unwrap();
    let mut out = Document::open(out).unwrap();
    pdfcraft_forms::set_value(&mut out, "shared", &pdfcraft_forms::FieldValue::Text("Grace".into())).unwrap();
    let fields = pdfcraft_forms::fields(&out);
    assert!(
        !fields.is_empty() && fields.iter().all(|f| f.name == "shared" && f.value == ["Grace"]),
        "one field, one value: {:?}",
        fields.iter().map(|f| (&f.name, &f.value)).collect::<Vec<_>>()
    );
    let catalog = out.get(out.root().unwrap()).as_dict().cloned().unwrap();
    let form = out.resolve(catalog.get(b"AcroForm").unwrap()).as_dict().cloned().unwrap();
    let listed = form.get(b"Fields").and_then(|f| f.as_array()).cloned().unwrap();
    assert_eq!(listed.len(), 1, "the field isn't copied again for the repeated page");
    // Its widgets: one on each of the three pages, each belonging to its own page.
    let out_pages: Vec<_> = pdfcraft_organize::pages(&out).unwrap().into_iter().map(|p| p.obj).collect();
    let kids = out.resolve(&listed[0]).as_dict().and_then(|f| f.get(b"Kids").and_then(|k| k.as_array()).cloned()).unwrap();
    assert_eq!(kids.len(), 3);
    let mut on: Vec<_> = kids.iter().map(|k| out.resolve(k).as_dict().and_then(|w| w.reference(b"P")).unwrap()).collect();
    on.sort();
    let mut expected = out_pages.clone();
    expected.sort();
    assert_eq!(on, expected, "a widget per page, on its own page");
}

#[test]
fn files_that_cannot_be_converted_are_refused_clearly() {
    let s = Session::new();
    let err = s.convert_to_pdf("report.docx", &Arc::new(b"PK\x03\x04".to_vec())).unwrap_err();
    assert_eq!(err, EditError::Source("report.docx: this file type can't be converted; use a PDF, an image or a .txt file".into()));
    assert!(matches!(s.convert_to_pdf("empty", &Arc::new(Vec::new())), Err(EditError::Source(_))));
    // Damaged inputs of a known type fail with an error, not a panic.
    assert!(s.convert_to_pdf("bad.png", &Arc::new(b"\x89PNG\r\n\x1a\nnope".to_vec())).is_err());
    assert!(s.convert_to_pdf("bad.jpg", &Arc::new(vec![0xFF, 0xD8, 0xFF])).is_err());
    assert!(s.convert_to_pdf("bad.pdf", &Arc::new(b"%PDF-1.7 nope".to_vec())).is_err());
    let err = s.convert_to_pdf("locked.pdf", &protected("pw", "o", -1)).unwrap_err();
    assert_eq!(err, EditError::Source("locked.pdf: it is password-protected".into()));
}

/// Combine files (#569): images and text become PDFs; PDFs pass through unread, even protected
/// ones (Combine asks for their password) and ones named .pdf without a header.
#[test]
fn files_for_combine_are_made_into_pdfs_and_pdfs_pass_through() {
    let s = Session::new();
    let locked = protected("pw", "o", -1);
    assert!(Arc::ptr_eq(&s.as_pdf("locked.pdf", &locked).unwrap(), &locked));
    let headless = Arc::new(b"1 0 obj << >> endobj".to_vec());
    assert!(Arc::ptr_eq(&s.as_pdf("old.PDF", &headless).unwrap(), &headless));
    assert!(s.as_pdf("scan.jpg", &Arc::new(jpeg_bytes())).unwrap().starts_with(b"%PDF-"));
    assert!(s.as_pdf("notes.txt", &Arc::new(b"hello".to_vec())).unwrap().starts_with(b"%PDF-"));
    let err = s.as_pdf("report.docx", &Arc::new(b"PK\x03\x04".to_vec())).unwrap_err();
    assert!(matches!(err, EditError::Source(m) if m.starts_with("report.docx: this file type can't be converted")));
}

#[test]
fn encrypted_documents_open_edit_and_save_encrypted() {
    let bytes = protected("pw", "owner", -1);
    let mut s = Session::new();
    assert!(s.open("x.pdf", None, bytes.clone(), None).is_err(), "needs a password");
    let id = s.open("x.pdf", None, bytes.clone(), Some("pw")).unwrap();
    assert!(s.get(id).unwrap().editable(), "{:?}", s.get(id).unwrap().read_only_reason);
    assert_eq!(s.get(id).unwrap().security_summary().unwrap().method, "AES, 256-bit");
    assert!(s.get(id).unwrap().info.encrypted, "reported as encrypted");
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    let saved = s.save_bytes(id).unwrap();
    assert_eq!(&saved[..bytes.len()], &bytes[..], "incremental");
    assert!(pdfcraft_cos::Document::open(saved.clone()).is_err(), "still protected after saving");
    let mut s2 = Session::new();
    let id2 = s2.open("x.pdf", None, saved, Some("pw")).unwrap();
    assert_eq!(s2.get(id2).unwrap().info.pages[0].rotation, 90);
}

#[test]
fn restricted_documents_refuse_changes_unless_opened_by_the_owner() {
    let bytes = protected("", "owner", 0b0100); // print only, no password to open
    let mut s = Session::new();
    let id = s.open("r.pdf", None, bytes.clone(), None).unwrap();
    let d = s.get(id).unwrap();
    assert!(!d.allows_assembly() && !d.allows_modification());
    let summary = d.security_summary().unwrap();
    assert!(!summary.owner && summary.permissions.print() && !summary.permissions.copy());
    assert_eq!(s.apply(id, Edit::DeletePages { pages: vec![0] }), Err(EditError::NotPermitted("page changes")));
    assert_eq!(s.apply(id, Edit::SetInfo { key: "Title".into(), value: "x".into() }), Err(EditError::NotPermitted("changes to the document")));
    assert!(matches!(s.extract(id, &[0]), Err(EditError::NotPermitted(_))));
    assert!(!s.get(id).unwrap().dirty);
    // The owner password lifts the restrictions.
    let oid = s.open("r.pdf", None, bytes, Some("owner")).unwrap();
    assert!(s.get(oid).unwrap().allows_assembly());
    s.apply(oid, Edit::DeletePages { pages: vec![0] }).unwrap();
}
