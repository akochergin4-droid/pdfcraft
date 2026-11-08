//! Unit tests of the editing operations (part 1 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn an_image_turns_by_any_angle_about_its_centre_and_saves_the_matrix() {
    // 100,100..300,200 is 200 x 100 around (200, 150).
    let mut doc = image_page();
    let r0 = images::page_images(&doc, 0).unwrap()[0].rect;
    let t = images::turn_by_degrees(r0, 30.0).unwrap();
    images::change_image(&mut doc, 0, 0, &images::ImageChange::Transform(t)).unwrap();
    let doc = reopen(&doc);
    let r = images::page_images(&doc, 0).unwrap()[0].rect;
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let (w, h) = (200.0 * c + 100.0 * s, 200.0 * s + 100.0 * c);
    assert!(close(r, [200.0 - w / 2.0, 150.0 - h / 2.0, 200.0 + w / 2.0, 150.0 + h / 2.0]), "{r:?}");
    // 90 degrees is the quarter turn; whole turns and non-finite angles are harmless.
    let q = images::turn_about_centre(r0, 1, false, false);
    let d = images::turn_by_degrees(r0, 90.0).unwrap();
    assert!(q.iter().zip(&d).all(|(a, b)| (a - b).abs() < 1e-9), "{q:?} {d:?}");
    let full = images::turn_by_degrees(r0, 720.0).unwrap();
    assert!(full.iter().zip([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).all(|(a, b)| (a - b).abs() < 1e-9));
    assert!(images::turn_by_degrees(r0, f64::NAN).is_none() && images::turn_by_degrees(r0, f64::INFINITY).is_none());
}

#[test]
fn an_added_image_crop_keeps_some_of_each_axis() {
    use crate::added::AddedImage;
    assert!(AddedImage::crop_valid([0.7, 0.0, 0.0, 0.95]));
    assert!(AddedImage::crop_valid([0.5, 0.45, 0.45, 0.5]));
    // 0.05 + 0.9 rounds to just above 0.95: still the 5 % kept, accepted and left as it is.
    assert!(AddedImage::crop_valid([0.05, 0.0, 0.9, 0.0]));
    assert_eq!(AddedImage::clamp_crop([0.05, 0.0, 0.9, 0.0]), [0.05, 0.0, 0.9, 0.0]);
    assert!(!AddedImage::crop_valid([0.06, 0.0, 0.9, 0.0]), "more than 95 % across");
    assert!(!AddedImage::crop_valid([0.6, 0.0, 0.4, 0.0]), "nothing left across");
    assert!(!AddedImage::crop_valid([-0.1, 0.0, 0.0, 0.0]));
    assert!(!AddedImage::crop_valid([f64::NAN, 0.0, 0.0, 0.0]));
    assert_eq!(AddedImage::clamp_crop([0.7, 0.0, 0.0, 0.0]), [0.7, 0.0, 0.0, 0.0], "a valid crop stays as it is");
    // From a file: too much across keeps the two sides' proportion; nonsense trims nothing.
    let [l, b, r, t] = AddedImage::clamp_crop([0.6, f64::INFINITY, 0.6, -1.0]);
    assert!((l - 0.475).abs() < 1e-12 && (r - 0.475).abs() < 1e-12 && b == 0.0 && t == 0.0);
    // Sides beyond the whole image keep their proportion too, and huge ones don't overflow.
    let [l, _, r, _] = AddedImage::clamp_crop([1.5, 0.0, 0.5, 0.0]);
    assert!((l - 0.7125).abs() < 1e-12 && (r - 0.2375).abs() < 1e-12, "{l} {r}");
    let [l, _, r, _] = AddedImage::clamp_crop([f64::MAX, 0.0, f64::MAX, 0.0]);
    assert!((l - 0.475).abs() < 1e-12 && (r - 0.475).abs() < 1e-12, "{l} {r}");
    let [l, _, r, _] = AddedImage::clamp_crop([1.5, 0.0, 0.0, 0.0]);
    assert!((l - 0.95).abs() < 1e-12 && r == 0.0, "one side alone is capped at the limit");
}

#[test]
fn tokens_expand() {
    let c = cx();
    assert_eq!(expand("Page <<1>> of <<n>>", 3, 10, 0, &c), "Page 3 of 10");
    assert_eq!(expand("<<1 of n>> · <<Page 1>> · <<1/n>>", 2, 5, 0, &c), "2 of 5 · Page 2 · 2/5");
    assert_eq!(expand("<<m/d/yyyy>> <<yyyy-mm-dd>> <<mmmm d, yyyy>>", 1, 1, 0, &c), "10/1/2026 2026-10-01 October 1, 2026");
    assert_eq!(expand("<<yyyy/mm/dd>> <<yyyy/m/d>>", 1, 1, 0, &c), "2026/10/01 2026/10/1");
    assert_eq!(expand("<<dd.mm.yyyy>> <<d.m.yyyy>> <<dd.mm.yy>> <<d.m.yy>>", 1, 1, 0, &c), "01.10.2026 1.10.2026 01.10.26 1.10.26");
    assert_eq!(expand("<<Bates Number#6#100#ABC#-X>>", 1, 1, 104, &c), "ABC000104-X");
    assert_eq!(expand("keep <<unknown>> and <<unclosed", 1, 1, 0, &c), "keep <<unknown>> and <<unclosed");
    assert_eq!(bates_start("x <<Bates Number#6#100#A#B>>"), Some(100));
}

#[test]
fn header_and_footer_are_drawn_in_display_space_and_wrap_the_original_content() {
    let mut doc = fixture();
    let hf = HeaderFooter {
        text: ["Left".into(), String::new(), "<<Page 1 of n>>".into(), String::new(), "Confidential (draft)".into(), String::new()],
        ..HeaderFooter::default()
    };
    add_header_footer(&mut doc, &[0, 1, 2], &hf, false, &cx()).unwrap();
    let doc = reopen(&doc);
    let s0 = streams(&doc, 0);
    // q-wrapper, original, Q-wrapper, header/footer.
    assert_eq!(s0.len(), 4, "{s0:?}");
    // The close starts on a new line, so a comment the original ends in can't swallow it.
    assert_eq!((s0[0].as_str(), s0[2].as_str()), ("q %PdfCraft\n", "\nQ %PdfCraft\n"));
    let mark = &s0[3];
    assert!(mark.contains("/PCMark /HeaderFooter") && mark.contains("(Page 1 of 3) Tj") && mark.contains("(Confidential \\(draft\\)) Tj"), "{mark}");
    assert!(mark.contains("1 0 0 1 0 0 cm"), "upright page: identity");
    // The rotated page maps display space onto its rotated crop box.
    let s1 = streams(&doc, 1);
    assert!(s1.last().unwrap().contains("0 1 -1 0 600 0 cm") && s1.last().unwrap().contains("(Page 2 of 3)"), "{s1:?}");
    // A page without content gets just the mark; its inherited resources are copied, not changed.
    assert_eq!(streams(&doc, 2).len(), 1);
    let p2 = &pdfcraft_model::pages(&doc)[2];
    let res = doc.resolve(p2.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap());
    assert!(fonts.as_dict().unwrap().contains(b"PCHelv") && fonts.as_dict().unwrap().contains(b"F1"));
    let shared = doc.get(pdfcraft_cos::ObjRef::new(6, 0));
    let shared_fonts = doc.resolve(shared.as_dict().unwrap().get(b"Font").unwrap());
    assert!(!shared_fonts.as_dict().unwrap().contains(b"PCHelv"), "the shared dictionary is untouched");
    assert_eq!(marks_present(&doc), [MarkKind::HeaderFooter]);
}

#[test]
fn header_and_footer_are_drawn_in_the_chosen_standard_face() {
    // #3754: Times or Courier instead of Helvetica. The content names the face, the page's fonts
    // gain it, and right-aligned text is placed by that face's widths (not Helvetica's).
    let text = "Wide words";
    for (font, resource, base) in [(added::Family::Times, "PCTimes", "Times-Roman"), (added::Family::Courier, "PCCour", "Courier")] {
        let mut doc = fixture();
        let hf = HeaderFooter {
            text: [String::new(), String::new(), text.into(), String::new(), String::new(), String::new()],
            font,
            ..HeaderFooter::default()
        };
        add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
        let doc = reopen(&doc);
        let mark = streams(&doc, 0).last().cloned().unwrap();
        assert!(mark.contains(&format!("/{resource} 8 Tf")), "{base}: {mark}");
        let x = 600.0 - 72.0 - font.width(text, 8.0, false, false);
        assert!((x - (600.0 - 72.0 - pdfcraft_fonts::helvetica_width(text, 8.0))).abs() > 1.0, "{base}: the faces differ in width");
        assert!(mark.contains(&format!("1 0 0 1 {} ", n(x))), "{base}: right-aligned by its own widths: {mark}");
        let page = &pdfcraft_model::pages(&doc)[0];
        let res = doc.resolve(page.dict.get(b"Resources").unwrap());
        let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap()).as_dict().cloned().unwrap();
        let face = doc.resolve(fonts.get(resource.as_bytes()).unwrap()).as_dict().cloned().unwrap();
        assert_eq!(face.name(b"BaseFont"), Some(base.as_bytes()), "{base}");
    }
    // The default is still Helvetica, and only Helvetica is added.
    let mut doc = fixture();
    let hf =
        HeaderFooter { text: [text.into(), String::new(), String::new(), String::new(), String::new(), String::new()], ..HeaderFooter::default() };
    add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
    let mark = streams(&reopen(&doc), 0).last().cloned().unwrap();
    assert!(mark.contains("/PCHelv 8 Tf") && !mark.contains("/PCTimes") && !mark.contains("/PCCour"), "{mark}");
}

#[test]
fn a_header_never_takes_over_a_font_name_the_page_already_draws_with() {
    // From review of #3756: a page that already draws a different font as /PCTimes keeps it; the
    // header's Times goes under a free name, and an update reuses that name, not a third one.
    let mut doc = fixture();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let mut fonts = Dict::new();
    let mut theirs = Dict::new();
    theirs.set(b"Type".to_vec(), Object::name("Font"));
    theirs.set(b"Subtype".to_vec(), Object::name("Type1"));
    theirs.set(b"BaseFont".to_vec(), Object::name("Helvetica-Bold"));
    fonts.set(b"PCTimes".to_vec(), Object::Dict(theirs));
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(page, |d| d.set(b"Resources".to_vec(), Object::Dict(res))).unwrap();
    let hf = HeaderFooter {
        text: ["Serif".into(), String::new(), String::new(), String::new(), String::new(), String::new()],
        font: added::Family::Times,
        ..HeaderFooter::default()
    };
    let base_of = |doc: &Document, name: &[u8]| {
        let fonts = page_fonts(doc, &pdfcraft_model::pages(doc)[0]);
        fonts.get(name).map(|f| doc.resolve(f)).and_then(|f| f.as_dict().and_then(|d| d.name(b"BaseFont")).map(<[u8]>::to_vec))
    };
    add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
    let doc = reopen(&doc);
    assert_eq!(base_of(&doc, b"PCTimes").as_deref(), Some(&b"Helvetica-Bold"[..]), "the page's own font is untouched");
    assert_eq!(base_of(&doc, b"PCTimes_1").as_deref(), Some(&b"Times-Roman"[..]));
    let mark = streams(&doc, 0).last().cloned().unwrap();
    assert!(mark.contains("/PCTimes_1 8 Tf"), "{mark}");
    // Updating the header finds its own name again.
    let mut doc = doc;
    add_header_footer(&mut doc, &[0], &hf, true, &cx()).unwrap();
    let doc = reopen(&doc);
    assert!(streams(&doc, 0).last().unwrap().contains("/PCTimes_1 8 Tf"));
    assert_eq!(base_of(&doc, b"PCTimes_2"), None, "no new name on update");
    assert_eq!(base_of(&doc, b"PCTimes").as_deref(), Some(&b"Helvetica-Bold"[..]));
    // The same face with more to it (here a /ToUnicode map) is the page's own too: kept whole.
    let mut doc = fixture();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let map = doc.add(Object::Stream(pdfcraft_cos::Stream::flate(Dict::new(), b"/CIDInit /ProcSet findresource begin")));
    let mut theirs = Dict::new();
    for (k, v) in [("Type", "Font"), ("Subtype", "Type1"), ("BaseFont", "Times-Roman"), ("Encoding", "WinAnsiEncoding")] {
        theirs.set(k.as_bytes().to_vec(), Object::name(v));
    }
    theirs.set(b"ToUnicode".to_vec(), Object::Ref(map));
    let mut fonts = Dict::new();
    fonts.set(b"PCTimes".to_vec(), Object::Dict(theirs));
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(page, |d| d.set(b"Resources".to_vec(), Object::Dict(res))).unwrap();
    add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
    let doc = reopen(&doc);
    let fonts = page_fonts(&doc, &pdfcraft_model::pages(&doc)[0]);
    let kept = doc.resolve(fonts.get(b"PCTimes").unwrap()).as_dict().cloned().unwrap();
    assert!(kept.contains(b"ToUnicode"), "the page's map is kept");
    assert!(streams(&doc, 0).last().unwrap().contains("/PCTimes_1 8 Tf"));
}

#[test]
fn header_start_number_that_would_wrap_is_refused_without_drawing() {
    // u32::MAX is a valid start_number on its own, but there is no room for a second page
    // number after it. The old code wrapped the second page to zero; now the whole run is
    // refused and the document keeps no header at all.
    let mut doc = fixture();
    let hf = HeaderFooter {
        text: [String::new(), String::new(), "<<1>>".into(), String::new(), String::new(), String::new()],
        start_number: u32::MAX,
        ..HeaderFooter::default()
    };
    let err = add_header_footer(&mut doc, &[0, 1], &hf, false, &cx()).unwrap_err();
    assert!(err.to_string().contains("leaves no room"), "{err}");
    assert!(marks_present(&doc).is_empty(), "nothing is drawn when the run is refused");
    // A single page still fits the start number itself.
    add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
    assert_eq!(marks_present(&doc).len(), 1);
}

#[test]
fn header_bates_start_that_would_overflow_is_refused_without_drawing() {
    // A Bates start of u64::MAX has no room for a second page. The run must be refused up front
    // instead of overflowing (a panic in debug builds, a wrap to 0 in release builds).
    let mut doc = fixture();
    let hf = HeaderFooter {
        text: [String::new(), String::new(), "<<Bates Number#6#18446744073709551615#A#B>>".into(), String::new(), String::new(), String::new()],
        ..HeaderFooter::default()
    };
    let err = add_header_footer(&mut doc, &[0, 1], &hf, false, &cx()).unwrap_err();
    assert!(err.to_string().contains("Bates"), "{err}");
    assert!(marks_present(&doc).is_empty(), "nothing is drawn when the run is refused");
    // A single page still fits the start number itself.
    add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
    assert_eq!(marks_present(&doc).len(), 1);
}

#[test]
fn replace_and_remove_restore_the_original_content() {
    let mut doc = fixture();
    let original = streams(&doc, 0);
    let mut hf = HeaderFooter::default();
    hf.text[1] = "First".into();
    add_header_footer(&mut doc, &[0], &hf, false, &cx()).unwrap();
    hf.text[1] = "Second".into();
    add_header_footer(&mut doc, &[0], &hf, true, &cx()).unwrap();
    let s = streams(&doc, 0);
    assert_eq!(s.iter().filter(|x| x.contains("/PCMark")).count(), 1, "replaced, not added");
    assert!(s.last().unwrap().contains("(Second)"));
    add_watermark(&mut doc, &[0], &Watermark { text: "DRAFT".into(), ..Watermark::default() }, false).unwrap();
    add_background(&mut doc, &[0], &Background { color: [1.0, 1.0, 0.9], opacity: 1.0, ..Background::default() }, false).unwrap();
    assert_eq!(marks_present(&doc).len(), 3);
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap(), 1);
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::Watermark).unwrap(), 1);
    let s = streams(&doc, 0);
    assert!(s[0].contains("/PCMark /Background"), "the background stays behind: {s:?}");
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::Background).unwrap(), 1);
    assert_eq!(streams(&reopen(&doc), 0), original, "back to exactly the original content");
}

#[test]
fn shrink_scales_the_original_content_into_the_band_between_header_and_footer() {
    let mut doc = fixture();
    // 8 pt text, margins 36 top and bottom: the header takes 36 + 8 (+ 4 clearance) at the top,
    // the footer 36 + 6.4 (its capitals above the baseline, + 4) at the bottom.
    add_header_footer(&mut doc, &[0], &shrinking("ACME", "<<Bates Number#6#1##>>"), false, &cx()).unwrap();
    let doc = reopen(&doc);
    let s = streams(&doc, 0);
    // q-wrapper, shrink, original, Q-wrapper, header/footer.
    assert_eq!(s.len(), 5, "{s:?}");
    assert_eq!((s[0].as_str(), s[3].as_str()), ("q %PdfCraft\n", "\nQ %PdfCraft\n"));
    assert!(s[2].contains("(Hi) Tj"), "the original content follows the shrink: {s:?}");
    let m = first_cm(&s[1]);
    let k = (800.0 - 48.0 - 46.4) / 800.0;
    assert!((m[0] - k).abs() < 1e-3 && (m[3] - k).abs() < 1e-3 && m[1] == 0.0 && m[2] == 0.0, "uniform scale {k}: {m:?}");
    // The page's corners land on the band, centred across.
    assert!(near(map_point(m, 0.0, 0.0), ((600.0 - 600.0 * k) / 2.0, 46.4)), "{m:?}");
    assert!(near(map_point(m, 600.0, 800.0), ((600.0 + 600.0 * k) / 2.0, 800.0 - 48.0)), "{m:?}");
    assert!(s[4].contains("(000001) Tj"), "{}", s[4]);
}

#[test]
fn shrink_follows_the_display_space_of_a_rotated_page() {
    let mut doc = fixture();
    // Page 2 is rotated 90°: shown 800 wide and 600 high. A footer alone keeps the top.
    add_header_footer(&mut doc, &[1], &shrinking("", "<<1>>"), false, &cx()).unwrap();
    let s = streams(&doc, 1);
    let m = first_cm(&s[1]);
    let page = &pdfcraft_model::pages(&doc)[1];
    let v = page.view_matrix(&doc);
    let k = (600.0 - 46.4) / 600.0;
    // Display (x, y) is user v·(x, y): the shrunk content's display corners.
    let to_user = |x: f64, y: f64| map_point(v, x, y);
    assert!(near(map_point(m, to_user(0.0, 0.0).0, to_user(0.0, 0.0).1), to_user((800.0 - 800.0 * k) / 2.0, 46.4)), "{m:?}");
    assert!(near(map_point(m, to_user(800.0, 600.0).0, to_user(800.0, 600.0).1), to_user((800.0 + 800.0 * k) / 2.0, 600.0)), "{m:?}");
}

#[test]
fn shrink_is_undone_by_remove_and_not_doubled_by_update() {
    let mut doc = fixture();
    let original = streams(&doc, 0);
    add_background(&mut doc, &[0], &Background::default(), false).unwrap();
    add_header_footer(&mut doc, &[0], &shrinking("First", ""), false, &cx()).unwrap();
    add_header_footer(&mut doc, &[0], &shrinking("Second", ""), true, &cx()).unwrap();
    let s = streams(&doc, 0);
    assert_eq!(s.iter().filter(|x| x.contains(" cm") && !x.contains("/PCMark")).count(), 2, "one shrink besides the original's own cm: {s:?}");
    assert!(s[0].contains("/PCMark /Background"), "the background stays behind, full size: {s:?}");
    // Update without shrinking: back to full size.
    let mut plain = shrinking("Third", "");
    plain.shrink = false;
    add_header_footer(&mut doc, &[0], &plain, true, &cx()).unwrap();
    assert_eq!(streams(&doc, 0).len(), 5, "background, wrapper, original, wrapper, header: {:?}", streams(&doc, 0));
    add_header_footer(&mut doc, &[0], &shrinking("Fourth", ""), true, &cx()).unwrap();
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::HeaderFooter).unwrap(), 2, "the header and its shrink");
    assert_eq!(remove_marks(&mut doc, &[0], MarkKind::Background).unwrap(), 1);
    assert_eq!(streams(&reopen(&doc), 0), original, "back to exactly the original content");
}

#[test]
fn shrink_with_no_room_left_is_refused_without_drawing() {
    let mut doc = fixture();
    let mut hf = shrinking("Top", "Bottom");
    hf.margins = [400.0, 400.0, 72.0, 72.0];
    let err = add_header_footer(&mut doc, &[0, 1], &hf, false, &cx()).unwrap_err();
    assert!(err.to_string().contains("no room"), "{err}");
    assert!(!doc.is_modified());
    // A page without content has nothing to shrink: it just gets the header.
    let mut doc = fixture();
    add_header_footer(&mut doc, &[2], &shrinking("Top", ""), false, &cx()).unwrap();
    assert_eq!(streams(&doc, 2).len(), 1);
}

#[test]
fn watermarks_rotate_fade_and_can_go_behind() {
    let mut doc = fixture();
    let wm = Watermark { text: "CONFIDENTIAL\nDo not copy".into(), opacity: 0.3, rotation: 45.0, behind: true, ..Watermark::default() };
    add_watermark(&mut doc, &[0], &wm, false).unwrap();
    let s = streams(&doc, 0);
    assert!(s[0].contains("/PCMark /Watermark"), "behind: first");
    assert!(s[0].contains("/PCGS30 gs") && s[0].contains("0.707 0.707 -0.707 0.707 300 400 cm"), "{}", s[0]);
    assert!(s[0].contains("(CONFIDENTIAL) Tj") && s[0].contains("(Do not copy) Tj"));
    assert_eq!(s.len(), 2, "no wrapper needed for content behind");
}

#[test]
fn a_second_background_is_drawn_over_the_first() {
    // #3591: the new background went in front of the old one in /Contents, so the old colour
    // was painted over it and stayed.
    let mut doc = fixture();
    add_background(&mut doc, &[0], &Background { color: [0.0, 0.0, 1.0], opacity: 1.0, ..Background::default() }, false).unwrap();
    add_background(&mut doc, &[0], &Background { color: [0.0, 1.0, 0.0], opacity: 1.0, ..Background::default() }, false).unwrap();
    let s = streams(&doc, 0);
    assert_eq!(s.len(), 3, "{s:?}");
    assert!(s[0].contains("0 0 1 rg") && s[1].contains("0 1 0 rg"), "the newer background is painted last: {s:?}");
    assert!(s[2].contains("(Hi) Tj"), "both stay behind the content: {s:?}");
}

#[test]
fn backgrounds_stay_below_watermarks_behind_the_content() {
    // Behind the content, backgrounds come first, in the order added, and then watermarks, so a
    // watermark added after a background shows and a later background doesn't cover it.
    let mut doc = fixture();
    add_background(&mut doc, &[0], &Background { color: [0.0, 0.0, 1.0], opacity: 1.0, ..Background::default() }, false).unwrap();
    add_watermark(&mut doc, &[0], &Watermark { text: "DRAFT".into(), behind: true, ..Watermark::default() }, false).unwrap();
    add_background(&mut doc, &[0], &Background { color: [0.0, 1.0, 0.0], opacity: 1.0, ..Background::default() }, false).unwrap();
    let s = streams(&doc, 0);
    assert_eq!(s.len(), 4, "{s:?}");
    assert!(s[0].contains("0 0 1 rg") && s[1].contains("0 1 0 rg"), "backgrounds first, oldest first: {s:?}");
    assert!(s[2].contains("/PCMark /Watermark") && s[3].contains("(Hi) Tj"), "then the watermark, then the content: {s:?}");
}

#[test]
fn a_watermark_behind_goes_under_one_on_top_of_a_page_without_content() {
    // With no content of its own, the page's watermark on top is its first stream, which looks
    // like a behind-mark; a watermark added behind later must still be painted under it.
    let mut doc = fixture();
    add_watermark(&mut doc, &[2], &Watermark { text: "TOP".into(), ..Watermark::default() }, false).unwrap();
    add_watermark(&mut doc, &[2], &Watermark { text: "BEHIND".into(), behind: true, ..Watermark::default() }, false).unwrap();
    let s = streams(&doc, 2);
    assert_eq!(s.len(), 2, "{s:?}");
    assert!(s[0].contains("(BEHIND) Tj") && s[1].contains("(TOP) Tj"), "the watermark behind is painted first: {s:?}");
}

#[test]
fn watermark_offset_moves_the_centre_right_and_up() {
    let mut doc = fixture();
    let wm = Watermark { text: "DRAFT".into(), rotation: 0.0, offset: [-100.0, 50.0], ..Watermark::default() };
    add_watermark(&mut doc, &[0], &wm, false).unwrap();
    let s = streams(&doc, 0);
    let mark = s.iter().find(|c| c.contains("/PCMark /Watermark")).expect("the watermark stream");
    assert!(mark.contains("1 0 0 1 200 450 cm"), "{mark}");
}

#[test]
fn a_watermark_offset_that_is_not_finite_is_refused() {
    for offset in [[f64::NAN, 0.0], [0.0, f64::INFINITY], [f64::NEG_INFINITY, 0.0]] {
        let mut doc = fixture();
        let wm = Watermark { text: "DRAFT".into(), offset, ..Watermark::default() };
        assert!(matches!(add_watermark(&mut doc, &[0], &wm, false), Err(EditError::Invalid(_))), "{offset:?}");
        assert!(!doc.is_modified());
    }
}

#[test]
fn invalid_requests_change_nothing() {
    let mut doc = fixture();
    assert!(matches!(add_header_footer(&mut doc, &[0], &HeaderFooter::default(), false, &cx()), Err(EditError::Invalid(_))));
    assert!(matches!(add_watermark(&mut doc, &[0], &Watermark::default(), false), Err(EditError::Invalid(_))));
    assert_eq!(
        add_background(&mut doc, &[9], &Background { color: [1.0; 3], opacity: 1.0, ..Background::default() }, false),
        Err(EditError::NoSuchPage(9))
    );
    assert_eq!(EditError::NoSuchPage(9).to_string(), "page 10 does not exist", "the message counts pages from 1 (#1550)");
    assert!(!doc.is_modified());
}

#[test]
fn flattening_draws_appearances_into_the_page_and_removes_the_comments() {
    use pdfcraft_annot::{Meta, NewAnnotation, NoteIcon, Shape, Style, add_annotation, add_reply};
    let mut doc = fixture();
    let meta = Meta { date: None, id: "x".into() };
    let add = |doc: &mut Document, shape: Shape| {
        let style = Style::default_for(&shape);
        add_annotation(doc, &NewAnnotation { page: 0, shape, style, contents: "c".into(), author: "a".into() }, &meta).unwrap()
    };
    add(&mut doc, Shape::Rectangle { rect: [10.0, 10.0, 110.0, 60.0] });
    let note = add(&mut doc, Shape::Note { at: [200.0, 700.0], icon: NoteIcon::Comment });
    add_reply(&mut doc, 0, note, "reply", "b", &meta).unwrap();
    let before = streams(&doc, 0);
    let n = flatten(&mut doc, &[0], true, false).unwrap();
    assert_eq!(n, 2, "the rectangle and the note icon are drawn; the reply has nothing to draw");
    let doc = reopen(&doc);
    let p = &pdfcraft_model::pages(&doc)[0];
    assert!(!p.dict.contains(b"Annots"), "comments, pop-up and reply are gone");
    let s = streams(&doc, 0);
    assert_eq!(s.len(), before.len() + 3, "wrapped original + flattened content: {s:?}");
    let flat = s.last().unwrap();
    assert!(flat.contains("/PCFl0 Do") && flat.contains("/PCFl1 Do"), "{flat}");
    // The rectangle's appearance is drawn at its rectangle (bbox = rect, identity mapping).
    assert!(flat.contains("q 1 0 0 1 0 0 cm /PCFl0 Do Q"), "{flat}");
    // The note's 20×20 icon box is mapped onto its rect at (200, 680).
    assert!(flat.contains("1 0 0 1 200 680 cm /PCFl1 Do"), "{flat}");
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let xo = doc.resolve(res.as_dict().unwrap().get(b"XObject").unwrap());
    assert!(xo.as_dict().unwrap().contains(b"PCFl1"));
    assert!(marks_present(&doc).is_empty(), "flattened content is not a removable mark");
}

/// #2368: a widget with /NoRotate on a rotated page is shown upright, turned with the page's
/// /Rotate about its rectangle's upper-left corner (§12.5.3, as MuPDF and Poppler draw it).
/// Flattening drew its appearance at the rectangle as it is, so the widget turned sideways and
/// moved; a neighbour without the flag is unchanged, and so is everything on an unrotated page.
#[test]
fn flattening_keeps_a_no_rotate_widget_upright_on_a_rotated_page() {
    let page = stream("", " ");
    let form = "<< /Type /XObject /Subtype /Form /BBox [0 0 100 50] /Length 15 >>\nstream\n0 0 100 50 re f\nendstream";
    for (rotate, plain, upright) in [
        (0, "q 1 0 0 1 100 500 cm /PCFl0 Do Q", "q 1 0 0 1 100 500 cm /PCFl1 Do Q"),
        (90, "q 1 0 0 1 100 500 cm /PCFl0 Do Q", "q 0 1 -1 0 150 550 cm /PCFl1 Do Q"),
        (180, "q 1 0 0 1 100 500 cm /PCFl0 Do Q", "q -1 0 0 -1 100 600 cm /PCFl1 Do Q"),
        (270, "q 1 0 0 1 100 500 cm /PCFl0 Do Q", "q 0 -1 1 0 50 550 cm /PCFl1 Do Q"),
    ] {
        let widget =
            |flags: i64| format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T (f{flags}) /Rect [100 500 200 550] /F {flags} /AP << /N 7 0 R >> >>");
        let mut doc = build(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R] >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Rotate {rotate} /Resources << >> /Contents 6 0 R /Annots [4 0 R 5 0 R] >>"
            ),
            &widget(4),
            &widget(4 | 16),
            &page,
            form,
        ]);
        assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 2);
        let doc = reopen(&doc);
        let s = streams(&doc, 0);
        let flat = s.last().unwrap();
        assert!(flat.contains(plain), "/Rotate {rotate}: {flat}");
        assert!(flat.contains(upright), "/Rotate {rotate}: {flat}");
    }
}

/// #1317: a cloudy square this builder cannot draw stays on the page. A solid square, a hidden
/// cloudy square and a cloudy square that already has an appearance keep their previous results.
#[test]
fn flattening_keeps_a_cloudy_square_it_cannot_draw() {
    let page = stream("", " ");
    let form = "<< /Type /XObject /Subtype /Form /BBox [100 600 200 650] /Length 28 >>\nstream\n1 0 0 RG 100 600 100 50 re S\nendstream";
    let case = |extra: &str, drawn: usize, left: bool| {
        let annot = format!("<< /Type /Annot /Subtype /Square /Rect [100 600 200 650] /C [1 0 0] /Contents (Synthetic comment) {extra} >>");
        let mut doc = build(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << >> /Contents 5 0 R /Annots [4 0 R] >>",
            &annot,
            &page,
            form,
        ]);
        let before = streams(&doc, 0);
        assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), drawn);
        let doc = reopen(&doc);
        let p = &pdfcraft_model::pages(&doc)[0];
        assert_eq!(p.dict.contains(b"Annots"), left);
        if drawn == 0 {
            assert_eq!(streams(&doc, 0), before, "nothing was drawn in place of the comment");
        }
    };
    case("/BE << /S /C /I 2 >>", 0, true);
    case("/BE << /S /S >>", 1, false);
    case("/BE << /S /C /I 2 >> /F 2", 0, true);
    case("/BE << /S /C /I 2 >> /AP << /N 6 0 R >>", 1, false);
}

/// #805: a supported note with no stored appearance is still drawn by viewers. Flattening must
/// generate that appearance and bake it into the page, not delete a comment that shows an icon.
#[test]
fn flattening_keeps_a_note_whose_appearance_is_missing() {
    let page = stream("", " ");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (Synthetic note) /Name /Comment /Open false /F 0 >>",
    ]);
    assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), 1, "the note icon is drawn");
    let doc = reopen(&doc);
    let p = &pdfcraft_model::pages(&doc)[0];
    assert!(!p.dict.contains(b"Annots"), "the note is gone once its icon is baked in");
    let flat = streams(&doc, 0).pop().unwrap();
    assert!(flat.contains("q 1 0 0 1 10 10 cm /PCFl0 Do Q"), "{flat}");
}

/// #3175: a note without an appearance is drawn as a fixed-size icon at the upper-left corner of
/// its `/Rect`. A rectangle with no width (LaTeX `pdfcomment`) used to scale the icon to nothing
/// while flattening still counted it as drawn and deleted the note.
#[test]
fn flattening_a_note_with_a_zero_width_rect_draws_a_fixed_size_icon() {
    let page = stream("", " ");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R 6 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /Text /Rect [50 60 50 72] /Contents (Zero width) /Name /Comment /F 4 >>",
        "<< /Type /Annot /Subtype /Text /Rect [10 20 150 120] /Contents (Large) /F 4 >>",
    ]);
    assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), 2, "both icons are drawn");
    let doc = reopen(&doc);
    let flat = streams(&doc, 0).pop().unwrap();
    assert!(flat.contains("q 1 0 0 1 50 52 cm /PCFl0 Do Q"), "{flat}");
    assert!(flat.contains("q 1 0 0 1 10 100 cm /PCFl1 Do Q"), "{flat}");
}

/// #1470: a form field whose widget is on no page (left behind by page extraction or combining)
/// has nothing to draw, but flattening every form field still takes it out of `/AcroForm /Fields`,
/// or other software keeps treating the flattened file as a form. A hidden widget on a page is
/// left alone, as before.
#[test]
fn flattening_fields_drops_the_ones_on_no_page() {
    let page = stream("", " ");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R 8 0 R] >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R 7 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (on_page) /Rect [10 10 150 40] /F 4 /AP << /N 10 0 R >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (orphan) /Rect [10 50 150 80] /F 4 /AP << /N 10 0 R >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (hidden) /Rect [10 90 150 120] /F 2 /AP << /N 10 0 R >> >>",
        "<< /FT /Tx /T (parent) /Kids [9 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /Rect [10 130 150 160] /F 4 /AP << /N 10 0 R >> >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream",
    ]);
    assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 1, "only the field on the page is drawn");
    let doc = reopen(&doc);
    let root = doc.root().unwrap();
    let af = doc.resolve(doc.get(root).as_dict().unwrap().get(b"AcroForm").unwrap());
    let fields = doc.resolve(af.as_dict().unwrap().get(b"Fields").unwrap());
    let left: Vec<pdfcraft_cos::ObjRef> = fields.as_array().unwrap().iter().filter_map(|f| f.as_ref()).collect();
    assert_eq!(left, [pdfcraft_cos::ObjRef::new(7, 0)], "only the hidden field on the page stays");
}

/// #1794: a form field whose widget has no usable appearance (none stored, or none for its
/// selected state) has nothing to draw. Flattening used to remove it anyway, losing its value
/// with no ink in its place. It stays, in the page and in the form, like a comment that can't be
/// drawn; a field with an appearance is still baked in and removed.
#[test]
fn flattening_keeps_a_valued_widget_it_cannot_draw() {
    let page = stream("", " ");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R] >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R 6 0 R 7 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Ada) /Rect [10 10 150 40] /F 4 >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /V /Yes /AS /Yes /Rect [10 50 30 70] /F 4 /AP << /N << /Off 8 0 R >> >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (drawn) /V (Bob) /Rect [10 90 150 120] /F 4 /AP << /N 8 0 R >> >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream",
    ]);
    assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 1, "only the field with an appearance is drawn");
    let doc = reopen(&doc);
    let refs = |o: &Object| -> Vec<u32> { doc.resolve(o).as_array().unwrap().iter().filter_map(|f| f.as_ref()).map(|r| r.num).collect() };
    let p = &pdfcraft_model::pages(&doc)[0];
    assert_eq!(refs(p.dict.get(b"Annots").unwrap()), [5, 6], "the undrawable widgets stay on the page");
    let root = doc.root().unwrap();
    let af = doc.resolve(doc.get(root).as_dict().unwrap().get(b"AcroForm").unwrap());
    assert_eq!(refs(af.as_dict().unwrap().get(b"Fields").unwrap()), [5, 6], "and in the form");
    let value = |n: u32| doc.get(pdfcraft_cos::ObjRef::new(n, 0)).as_dict().unwrap().get(b"V").cloned();
    assert_eq!(value(5), Some(Object::String(pdfcraft_cos::PdfString::literal(b"Ada".to_vec()))));
    assert_eq!(value(6), Some(Object::name("Yes")));
}

/// #4017: a widget with no appearance whose field has no value (none, or an empty string) has
/// nothing to lose, so flattening removes it instead of leaving a fillable field behind. One whose
/// value it inherits from its parent field still stays (#1794).
#[test]
fn flattening_removes_an_empty_widget_it_cannot_draw() {
    let page = stream("", " ");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R 8 0 R] >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R 6 0 R 7 0 R 9 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (no_value) /Rect [10 10 150 40] /F 4 >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (empty) /V () /Rect [10 50 150 80] /F 4 >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (drawn) /Rect [10 90 150 120] /F 4 /AP << /N 10 0 R >> >>",
        "<< /FT /Tx /T (parent) /V (Ada) /Kids [9 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /Parent 8 0 R /Rect [10 130 150 160] /F 4 >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream",
    ]);
    assert_eq!(flatten(&mut doc, &[0], false, true).unwrap(), 1, "only the field with an appearance is drawn");
    let doc = reopen(&doc);
    let refs = |o: &Object| -> Vec<u32> { doc.resolve(o).as_array().unwrap().iter().filter_map(|f| f.as_ref()).map(|r| r.num).collect() };
    let p = &pdfcraft_model::pages(&doc)[0];
    assert_eq!(refs(p.dict.get(b"Annots").unwrap()), [9], "only the widget with an inherited value stays on the page");
    let root = doc.root().unwrap();
    let af = doc.resolve(doc.get(root).as_dict().unwrap().get(b"AcroForm").unwrap());
    assert_eq!(refs(af.as_dict().unwrap().get(b"Fields").unwrap()), [8], "and its field in the form");
}

/// #4017: when the walk up the parent fields reaches its limit before finding a value, the widget
/// stays: an unverified value must not be removed. Here `/V` sits on the 64th ancestor.
#[test]
fn flattening_keeps_a_widget_whose_value_is_past_the_parent_limit() {
    let page = stream("", " ");
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [69 0 R] >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R] >>".into(),
        page,
        "<< /Type /Annot /Subtype /Widget /Parent 6 0 R /Rect [10 10 150 40] /F 4 >>".into(),
    ];
    objs.extend((6..69).map(|n| format!("<< /Parent {} 0 R /Kids [{} 0 R] >>", n + 1, n - 1)));
    objs.push("<< /FT /Tx /T (deep) /V (Ada) /Kids [68 0 R] >>".into());
    let mut doc = build(&objs.iter().map(String::as_str).collect::<Vec<_>>());
    flatten(&mut doc, &[0], false, true).unwrap();
    let doc = reopen(&doc);
    let annots = pdfcraft_model::pages(&doc)[0].dict.get(b"Annots").map(|a| doc.resolve(a).as_array().map_or(0, Vec::len));
    assert_eq!(annots, Some(1), "the widget stays");
}

/// #1794: a reply this builder cannot draw stays on the page even when the comment it replies to
/// is flattened; the comment's pop-up still goes with it.
#[test]
fn flattening_keeps_an_undrawn_reply_to_a_flattened_comment() {
    let page = stream("", " ");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << >> /Contents 4 0 R /Annots [5 0 R 6 0 R 7 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /Square /Rect [100 600 200 650] /C [1 0 0] /Contents (Synthetic comment) /AP << /N 8 0 R >> /Popup 7 0 R >>",
        "<< /Type /Annot /Subtype /Square /Rect [100 500 200 550] /C [1 0 0] /Contents (Synthetic reply) /BE << /S /C /I 2 >> /IRT 5 0 R >>",
        "<< /Type /Annot /Subtype /Popup /Rect [300 600 400 700] /Parent 5 0 R >>",
        "<< /Type /XObject /Subtype /Form /BBox [100 600 200 650] /Length 28 >>\nstream\n1 0 0 RG 100 600 100 50 re S\nendstream",
    ]);
    assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), 1, "the comment is drawn; the cloudy reply cannot be");
    let doc = reopen(&doc);
    let p = &pdfcraft_model::pages(&doc)[0];
    let left: Vec<u32> = doc.resolve(p.dict.get(b"Annots").unwrap()).as_array().unwrap().iter().filter_map(|f| f.as_ref()).map(|r| r.num).collect();
    assert_eq!(left, [6], "the undrawn reply stays; the comment and its pop-up are gone");
}

/// Flattening never makes a `?` permanent: it refuses, changing nothing, a text box without an
/// appearance whose text Helvetica can't draw (another program's), and a comment or form field
/// whose appearance PdfCraft drew with `?` (marked `PCUndrawn`). A comment another program drew
/// with a font that has those characters (an appearance, no mark) flattens as before.
#[test]
fn flattening_refuses_what_would_bake_in_question_marks() {
    let page = stream("", " ");
    let refused = |annot: &str, comments: bool, fields: bool, what: &str| {
        let mut doc = build(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R] >>",
            &page,
            annot,
            "<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream",
        ]);
        let before = write_incremental(&doc, &SaveOptions::default()).unwrap();
        let err = flatten(&mut doc, &[0], comments, fields).unwrap_err().to_string();
        assert!(err.contains(what) && err.contains("page 1") && err.contains("\"欢\" (U+6B22)") && err.contains("nothing was flattened"), "{err}");
        assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), before, "a refused flatten changes nothing");
        assert!(pdfcraft_model::pages(&doc)[0].dict.contains(b"Annots"), "the annotation is still there");
    };
    refused(
        "<< /Type /Annot /Subtype /FreeText /Rect [10 10 150 40] /DA (0 g /Helv 12 Tf) /Contents <FEFF6B228FCE> /F 4 >>",
        true,
        false,
        "a comment",
    );
    refused(
        "<< /Type /Annot /Subtype /FreeText /Rect [10 10 150 40] /DA (0 g /Helv 12 Tf) /Contents <FEFF6B228FCE> /F 4 /AP << /N 6 0 R >> /PCUndrawn <FEFF6B22> >>",
        true,
        false,
        "a comment",
    );
    refused(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V <FEFF6B228FCE> /Rect [10 10 150 40] /F 4 /AP << /N 6 0 R >> /PCUndrawn <FEFF6B22> >>",
        false,
        true,
        "a form field",
    );
    // Drawn by another program, with a font that has 欢: no mark, so it flattens.
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R /Annots [5 0 R] >>",
        &page,
        "<< /Type /Annot /Subtype /FreeText /Rect [10 10 150 40] /DA (0 g /Helv 12 Tf) /Contents <FEFF6B228FCE> /F 4 /AP << /N 6 0 R >> >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 140 30] /Length 0 >>\nstream\n\nendstream",
    ]);
    assert_eq!(flatten(&mut doc, &[0], true, false).unwrap(), 1);
}

/// Display space is in points, so on a `/UserUnit 2` page an item's box maps to half as many
/// user-space units. Items record the `/UserUnit` they were written for; one without it was
/// written when display space was in user-space units, and reads back scaled to points.
#[test]
fn added_items_follow_the_page_user_unit() {
    let mut doc = fixture();
    let page = pdfcraft_model::pages(&doc)[2].obj;
    doc.update_dict(page, |d| d.set(b"UserUnit".to_vec(), Object::Int(2))).unwrap();
    let text = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "Scaled".into(), size: 14.0, ..AddedText::default() };
    add_content(&mut doc, 2, &Content::Text(text.clone())).unwrap();
    add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap();
    let doc = reopen(&doc);
    let all = list_added(&doc);
    let on = |page: usize| all.iter().find(|a| a.page == page).unwrap();
    let Content::Text(t) = &on(2).content else { panic!() };
    assert_eq!((t.rect[0], t.rect[2], t.rect[3], t.size), (72.0, 300.0, 700.0, 14.0), "read back as written");
    assert!(streams(&doc, 2).iter().any(|s| s.starts_with("q 0.5 0 0 0.5 0 0 cm")), "display points to user space");
    let recorded = |doc: &Document, a: &Added| {
        doc.get(a.obj).as_dict().and_then(|d| d.get(b"PCAdded").cloned()).and_then(|p| p.as_dict().and_then(|p| p.get(b"UserUnit").cloned()))
    };
    assert!(recorded(&doc, on(0)).is_none(), "a page without /UserUnit records nothing");
    assert_eq!(recorded(&doc, on(2)).and_then(|u| u.as_f64()), Some(2.0));
    // The same item as an earlier version wrote it: no record, box and size in user-space units.
    let mut legacy = doc.clone();
    let Object::Stream(mut s) = (*legacy.get(on(2).obj)).clone() else { panic!() };
    let mut params = s.dict.get(b"PCAdded").and_then(|p| p.as_dict().cloned()).unwrap();
    params.remove(b"UserUnit");
    s.dict.set(b"PCAdded".to_vec(), Object::Dict(params));
    legacy.set(on(2).obj, Object::Stream(s));
    let Content::Text(old) = &list_added(&legacy).into_iter().find(|a| a.page == 2).unwrap().content else { panic!() };
    assert_eq!((old.rect[0], old.rect[2], old.rect[3], old.size), (144.0, 600.0, 1400.0, 28.0), "scaled to points");
}

#[test]
fn combined_marker_scan_matches_the_independent_scans() {
    let empty = build(&["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", "<< /Type /Page /Parent 2 0 R >>"]);
    assert_marker_scan_matches_legacy(&empty, "empty page", &[], 0);
    assert_marker_scan_matches_legacy(&fixture(), "ordinary streams and a page without contents", &[], 0);

    for (tag, kind) in [("HeaderFooter", MarkKind::HeaderFooter), ("Watermark", MarkKind::Watermark), ("Background", MarkKind::Background)] {
        let mut doc = fixture();
        let marker = tagged_stream(&mut doc, tag, None);
        set_page_contents(&mut doc, marker, None);
        assert_marker_scan_matches_legacy(&doc, tag, &[kind], 0);
    }

    let mut all_marks = fixture();
    let background = tagged_stream(&mut all_marks, "Background", None);
    let watermark = tagged_stream(&mut all_marks, "Watermark", None);
    let header = tagged_stream(&mut all_marks, "HeaderFooter", None);
    set_page_contents(
        &mut all_marks,
        Object::Array(vec![background.clone(), watermark.clone(), header.clone(), watermark, header, background]),
        None,
    );
    assert_marker_scan_matches_legacy(&all_marks, "all mark kinds repeated", &[MarkKind::Background, MarkKind::Watermark, MarkKind::HeaderFooter], 0);

    let mut single_reference = fixture();
    let marker = tagged_stream(&mut single_reference, "Watermark", None);
    set_page_contents(&mut single_reference, marker, None);
    assert_marker_scan_matches_legacy(&single_reference, "contents as one reference", &[MarkKind::Watermark], 0);

    let mut text_only = fixture();
    let text = tagged_stream(&mut text_only, "Added", Some(Object::Dict(added_text_params())));
    set_page_contents(&mut text_only, text, None);
    let text_scan = assert_marker_scan_matches_legacy(&text_only, "one added text item", &[], 1);
    assert!(matches!(&text_scan.added[0].content, Content::Text(text) if text.text == "Added text"));

    let mut indirect_params = fixture();
    let params = indirect_params.add(Object::Dict(added_text_params()));
    let text = tagged_stream(&mut indirect_params, "Added", Some(Object::Ref(params)));
    set_page_contents(&mut indirect_params, text, None);
    let indirect_scan = assert_marker_scan_matches_legacy(&indirect_params, "indirect PCAdded dictionary", &[], 1);
    assert!(matches!(&indirect_scan.added[0].content, Content::Text(text) if text.text == "Added text"));

    let mut image_only = fixture();
    let image = image_only.add(Object::Stream(Stream::from_raw(Dict::new(), vec![0])));
    let image_item = tagged_stream(&mut image_only, "Added", Some(Object::Dict(added_image_params(image))));
    set_page_contents(&mut image_only, image_item, None);
    let image_scan = assert_marker_scan_matches_legacy(&image_only, "one added image item", &[], 1);
    assert!(matches!(&image_scan.added[0].content, Content::Image(item) if item.image == image));

    let mut interleaved = fixture();
    let text = tagged_stream(&mut interleaved, "Added", Some(Object::Dict(added_text_params())));
    let watermark = tagged_stream(&mut interleaved, "Watermark", None);
    let image = interleaved.add(Object::Stream(Stream::from_raw(Dict::new(), vec![0])));
    let image_item = tagged_stream(&mut interleaved, "Added", Some(Object::Dict(added_image_params(image))));
    let header = tagged_stream(&mut interleaved, "HeaderFooter", None);
    set_page_contents(&mut interleaved, Object::Array(vec![text, watermark, image_item, header]), None);
    let interleaved_scan =
        assert_marker_scan_matches_legacy(&interleaved, "marks and added content interleaved", &[MarkKind::Watermark, MarkKind::HeaderFooter], 2);
    assert!(matches!(&interleaved_scan.added[0].content, Content::Text(_)));
    assert!(matches!(&interleaved_scan.added[1].content, Content::Image(_)));

    let mut array_contents = fixture();
    let background = tagged_stream(&mut array_contents, "Background", None);
    let array_ref = array_contents.add(Object::Array(vec![background]));
    set_page_contents(&mut array_contents, Object::Ref(array_ref), None);
    assert_marker_scan_matches_legacy(&array_contents, "contents as an indirect array", &[MarkKind::Background], 0);

    let mut malformed = fixture();
    let indirect_mark = malformed.add(Object::Dict(tagged_dictionary("HeaderFooter", None)));
    let missing_added = tagged_stream(&mut malformed, "Added", None);
    let invalid_added = tagged_stream(&mut malformed, "Added", Some(Object::String(pdfcraft_cos::PdfString::text("bad params"))));
    let unknown = tagged_stream(&mut malformed, "FutureKind", None);
    let direct_watermark = Object::Dict(tagged_dictionary("Watermark", None));
    let direct_added = Object::Dict(tagged_dictionary("Added", Some(Object::Dict(added_text_params()))));
    set_page_contents(
        &mut malformed,
        Object::Array(vec![
            Object::Null,
            Object::Int(7),
            Object::name("not-a-stream"),
            direct_watermark,
            Object::Ref(indirect_mark),
            missing_added,
            invalid_added,
            unknown,
            direct_added,
        ]),
        None,
    );
    assert_marker_scan_matches_legacy(
        &malformed,
        "malformed, non-stream, unknown and direct entries",
        &[MarkKind::Watermark, MarkKind::HeaderFooter],
        0,
    );

    let mut shared = fixture();
    let added = tagged_stream(&mut shared, "Added", Some(Object::Dict(added_text_params())));
    let mark = tagged_stream(&mut shared, "Background", None);
    set_page_contents(&mut shared, Object::Array(vec![added.clone(), mark.clone(), added.clone(), mark]), None);
    let shared_scan = assert_marker_scan_matches_legacy(&shared, "repeated shared stream references", &[MarkKind::Background], 2);
    assert_eq!(shared_scan.added[0].obj, shared_scan.added[1].obj);

    let mut user_unit = fixture();
    let added = tagged_stream(&mut user_unit, "Added", Some(Object::Dict(added_text_params())));
    set_page_contents(&mut user_unit, added, Some(Object::Int(2)));
    let unit_scan = assert_marker_scan_matches_legacy(&user_unit, "non-1.0 UserUnit", &[], 1);
    let Content::Text(text) = &unit_scan.added[0].content else { panic!() };
    assert_eq!(text.rect, [20.0, 40.0, 220.0, 80.0]);
    assert_eq!(text.size, 24.0);
}

#[test]
fn added_text_and_images_are_page_content_that_stays_editable() {
    let mut doc = fixture();
    let text = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "Approved by Ada\nSecond line".into(), size: 14.0, ..AddedText::default() };
    assert_eq!(add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap(), 0);
    // An 1×1 gray image, placed on the rotated page.
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), Object::name("XObject"));
    d.set(b"Subtype".to_vec(), Object::name("Image"));
    d.set(b"Width".to_vec(), Object::Int(1));
    d.set(b"Height".to_vec(), Object::Int(1));
    d.set(b"ColorSpace".to_vec(), Object::name("DeviceGray"));
    d.set(b"BitsPerComponent".to_vec(), Object::Int(8));
    let img = doc.add(Object::Stream(Stream::from_raw(d, vec![128])));
    add_content(&mut doc, 1, &Content::Image(AddedImage::new([10.0, 10.0, 110.0, 60.0], img))).unwrap();
    let doc2 = reopen(&doc);
    let all = list_added(&doc2);
    assert_eq!(all.len(), 2);
    let Content::Text(t) = &all[0].content else { panic!() };
    assert_eq!((t.text.as_str(), t.size), ("Approved by Ada\nSecond line", 14.0));
    assert_eq!(t.rect, [72.0, 700.0 - 2.0 * 14.0 * 1.2, 300.0, 700.0], "the box height follows the two lines");
    let page0 = streams(&doc2, 0).join("\n");
    assert!(page0.contains("(Approved by Ada) Tj") && page0.contains("(Second line) Tj") && page0.contains("/PCFHelvetica 14 Tf"), "{page0}");
    // The rotated page draws in display space: the view matrix comes first.
    let page1 = streams(&doc2, 1).join("\n");
    assert!(page1.contains("q 0 1 -1 0 600 0 cm") && page1.contains(&format!("/PCImg{} Do", img.num)), "{page1}");
    // Edit: move, restyle, retype; then delete.
    let mut doc = doc2;
    let moved = AddedText {
        rect: [100.0, 500.0, 300.0, 520.0],
        text: "Approved".into(),
        bold: true,
        family: Family::Times,
        align: Align::Right,
        color: [1.0, 0.0, 0.0],
        ..text
    };
    update_content(&mut doc, 0, 0, &Content::Text(moved)).unwrap();
    let page0 = streams(&doc, 0).join("\n");
    assert!(page0.contains("/PCFTimesBold 14 Tf 1 0 0 rg") && !page0.contains("Second line"), "{page0}");
    assert!(
        update_content(&mut doc, 1, 0, &Content::Text(AddedText { text: "x".into(), rect: [0.0, 0.0, 50.0, 10.0], ..AddedText::default() })).is_err(),
        "kinds don't change"
    );
    delete_content(&mut doc, 0, 0).unwrap();
    assert_eq!(list_added(&doc).len(), 1);
    assert!(!streams(&doc, 0).join("").contains("Approved"));
    assert!(add_content(&mut doc, 0, &Content::Text(AddedText { rect: [0.0, 0.0, 100.0, 10.0], ..AddedText::default() })).is_err(), "empty text");
    // Page marks ignore added items.
    assert!(marks_present(&doc).is_empty());
}

/// #6440: added text in a print file can be gray or CMYK, painted with `g` / `k`, so its black
/// stays 100 K. The ink survives save and reopen; another colour drops it for good.
#[test]
fn gray_and_cmyk_added_text_writes_g_and_k_until_it_is_recoloured() {
    use crate::added::Ink;
    let item_stream = |doc: &Document, i: usize| {
        let obj = list_added(doc)[i].obj;
        let Object::Stream(s) = &*doc.get(obj) else { panic!("an item is a stream") };
        String::from_utf8_lossy(&s.decoded().unwrap()).into_owned()
    };
    let mut doc = fixture();
    let base = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "Proof".into(), size: 14.0, ..AddedText::default() };
    let mut black = base.clone();
    black.set_ink(Some(Ink::Cmyk([0.0, 0.0, 0.0, 1.0])));
    assert_eq!(black.color, [0.0; 3], "100 K looks black on screen");
    let mut gray = AddedText { rect: [72.0, 400.0, 300.0, 500.0], ..base.clone() };
    gray.set_ink(Some(Ink::Gray(0.25)));
    add_content(&mut doc, 0, &Content::Text(black)).unwrap();
    add_content(&mut doc, 0, &Content::Text(gray)).unwrap();
    add_content(&mut doc, 0, &Content::Text(AddedText { rect: [72.0, 200.0, 300.0, 300.0], ..base.clone() })).unwrap();

    let doc = reopen(&doc);
    let (k, g, rgb) = (item_stream(&doc, 0), item_stream(&doc, 1), item_stream(&doc, 2));
    assert!(k.contains("/PCFHelvetica 14 Tf 0 0 0 1 k\n") && !k.contains("rg"), "{k}");
    assert!(g.contains("/PCFHelvetica 14 Tf 0.25 g\n") && !g.contains("rg"), "{g}");
    assert!(rgb.contains("/PCFHelvetica 14 Tf 0 0 0 rg\n"), "RGB text paints as before: {rgb}");
    let inks: Vec<Option<Ink>> = list_added(&doc).iter().map(|a| if let Content::Text(t) = &a.content { t.painted_ink() } else { None }).collect();
    assert_eq!(inks, [Some(Ink::Cmyk([0.0, 0.0, 0.0, 1.0])), Some(Ink::Gray(0.25)), None]);

    // Another colour drops the ink, and picking the ink's look again later doesn't bring it back;
    // picking that look while the ink is there keeps it.
    let Content::Text(cmyk) = list_added(&doc)[0].content.clone() else { panic!() };
    let mut same = cmyk.clone();
    same.set_color([0.0; 3]);
    assert_eq!(same, cmyk, "black on 100 K stays 100 K");
    let mut recoloured = cmyk.clone();
    recoloured.set_color([1.0, 0.0, 0.0]);
    assert_eq!(recoloured.ink, None);
    recoloured.set_color([0.0; 3]);
    assert_eq!((recoloured.ink, recoloured.painted_ink()), (None, None), "the ink stays gone");

    // Set directly, a colour paints in RGB too, and the item reads back without the ink: what a
    // draft with its stale ink dropped compares equal to.
    let mut doc = doc;
    let red = AddedText { color: [1.0, 0.0, 0.0], ..cmyk };
    update_content(&mut doc, 0, 0, &Content::Text(red.clone())).unwrap();
    let stream = item_stream(&doc, 0);
    assert!(stream.contains("1 0 0 rg") && !stream.contains(" k\n"), "{stream}");
    let mut settled = red;
    settled.drop_stale_ink();
    for doc in [&doc, &reopen(&doc)] {
        let Content::Text(back) = list_added(doc)[0].content.clone() else { panic!() };
        // The stored box is the text's own height.
        assert_eq!(back, AddedText { rect: back.rect, ..settled.clone() });
    }

    // CMYK looks as ISO 32000-2 §10.4.2.5 maps it to RGB; an ink out of range is refused.
    assert_eq!(Ink::Cmyk([0.5, 0.0, 1.0, 0.25]).rgb(), [0.25, 0.75, 0.0]);
    let mut wrong = base;
    wrong.ink = Some(Ink::Gray(1.5));
    assert!(add_content(&mut doc, 0, &Content::Text(wrong)).is_err());
}

/// #6440: a file edited by hand paints its added text in RGB unless the stored gray or CMYK is
/// in range, the right length and still the look of `/Color`; nothing in it panics.
#[test]
fn a_hand_edited_ink_reads_back_only_when_it_is_the_stored_colours_look() {
    use crate::added::Ink;
    let mut doc = fixture();
    add_content(&mut doc, 0, &Content::Text(AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "Proof".into(), ..AddedText::default() })).unwrap();
    let obj = list_added(&doc)[0].obj;
    let real = |v: &[f64]| Object::Array(v.iter().map(|x| Object::Real(*x)).collect());
    // A `/CMYK` far longer than four numbers, as a hostile file might store it: an ink first.
    let long = real(&[0.0, 0.0, 0.0, 1.0].repeat(50_000));
    let long_ref = Object::Ref(doc.add(long.clone()));
    let ink_ref = Object::Ref(doc.add(real(&[0.0, 0.0, 0.0, 1.0])));
    let read_text = |entries: &[(&[u8], Object)]| {
        let mut edited = doc.clone();
        let Object::Stream(mut s) = (*edited.get(obj)).clone() else { panic!("an item is a stream") };
        let mut params = s.dict.get(b"PCAdded").and_then(|p| p.as_dict().cloned()).unwrap();
        for (key, value) in entries {
            params.set(key.to_vec(), value.clone());
        }
        s.dict.set(b"PCAdded".to_vec(), Object::Dict(params));
        edited.set(obj, Object::Stream(s));
        let Content::Text(t) = list_added(&edited)[0].content.clone() else { panic!() };
        t
    };
    let read = |entries: &[(&[u8], Object)]| read_text(entries).ink;
    // As PDFCraft writes it: the ink stands.
    assert_eq!(read(&[(b"Color", real(&[0.5; 3])), (b"Gray", Object::Real(0.5))]), Some(Ink::Gray(0.5)));
    assert_eq!(read(&[(b"Color", real(&[0.0; 3])), (b"CMYK", real(&[0.0, 0.0, 0.0, 1.0]))]), Some(Ink::Cmyk([0.0, 0.0, 0.0, 1.0])));
    assert_eq!(read(&[(b"Color", real(&[0.0; 3])), (b"CMYK", ink_ref)]), Some(Ink::Cmyk([0.0, 0.0, 0.0, 1.0])), "by reference");
    // Finer than it paints, it reads back as painted (`0.5 g`), its look with it.
    let fine = read_text(&[(b"Color", real(&[0.50049; 3])), (b"Gray", Object::Real(0.50049))]);
    assert_eq!((fine.ink, fine.color), (Some(Ink::Gray(0.5)), [0.5; 3]));
    for (what, entries) in [
        ("a colour the ink isn't the look of", vec![(&b"Color"[..], real(&[1.0, 0.0, 0.0])), (b"CMYK", real(&[0.0, 0.0, 0.0, 1.0]))]),
        ("three CMYK numbers", vec![(b"Color", real(&[0.0; 3])), (b"CMYK", real(&[0.0, 0.0, 1.0]))]),
        ("five CMYK numbers", vec![(b"Color", real(&[0.0; 3])), (b"CMYK", real(&[0.0, 0.0, 0.0, 1.0, 0.0]))]),
        ("a long CMYK", vec![(b"Color", real(&[0.0; 3])), (b"CMYK", long.clone())]),
        ("a long CMYK by reference", vec![(b"Color", real(&[0.0; 3])), (b"CMYK", long_ref.clone())]),
        ("gray a long array", vec![(b"Color", real(&[0.0; 3])), (b"Gray", long.clone())]),
        ("CMYK not an array", vec![(b"Color", real(&[0.0; 3])), (b"CMYK", Object::Real(1.0))]),
        ("gray out of range", vec![(b"Color", real(&[1.0; 3])), (b"Gray", Object::Real(2.0))]),
        ("gray not a number", vec![(b"Color", real(&[0.0; 3])), (b"Gray", Object::name("Black"))]),
        (
            "a CMYK entry not a number",
            vec![
                (b"Color", real(&[0.0; 3])),
                (b"CMYK", Object::Array(vec![Object::Real(0.0), Object::name("Bad"), Object::Real(0.0), Object::Real(1.0)])),
            ],
        ),
        (
            "gray not a number beside a CMYK",
            vec![(b"Color", real(&[0.0; 3])), (b"Gray", Object::name("Black")), (b"CMYK", real(&[0.0, 0.0, 0.0, 1.0]))],
        ),
        ("gray NaN", vec![(b"Color", real(&[0.0; 3])), (b"Gray", Object::Real(f64::NAN))]),
        ("CMYK infinite", vec![(b"Color", real(&[0.0; 3])), (b"CMYK", real(&[0.0, 0.0, 0.0, f64::INFINITY]))]),
    ] {
        assert_eq!(read(&entries), None, "{what}");
    }
}

/// #6440: an ink finer than the thousandth `g` and `k` are written to is kept as painted, so the
/// item, its stored `/CMYK` and what it reads back as are what the page shows.
#[test]
fn an_ink_is_kept_as_it_is_painted() {
    use crate::added::Ink;
    let mut doc = fixture();
    let mut t = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "Proof".into(), size: 14.0, ..AddedText::default() };
    t.set_ink(Some(Ink::Cmyk([0.12345, 0.00049, 0.0, 0.99951])));
    let painted = Ink::Cmyk([0.123, 0.0, 0.0, 1.0]);
    assert_eq!((t.ink, t.color), (Some(painted), painted.rgb()));
    add_content(&mut doc, 0, &Content::Text(t)).unwrap();
    let doc = reopen(&doc);
    let item = &list_added(&doc)[0];
    let Object::Stream(s) = &*doc.get(item.obj) else { panic!("an item is a stream") };
    let stream = String::from_utf8_lossy(&s.decoded().unwrap()).into_owned();
    assert!(
        stream.contains(
            "/PCFHelvetica 14 Tf 0.123 0 0 1 k
"
        ),
        "{stream}"
    );
    let stored = s
        .dict
        .get(b"PCAdded")
        .and_then(|p| p.as_dict())
        .and_then(|p| p.get(b"CMYK"))
        .and_then(Object::as_array)
        .map(|a| a.iter().filter_map(Object::as_f64).collect::<Vec<_>>());
    assert_eq!(stored, Some(vec![0.123, 0.0, 0.0, 1.0]));
    let Content::Text(back) = &item.content else { panic!() };
    assert_eq!(back.painted_ink(), Some(painted));
}

/// #4464: a page left with one added item draws that item and nothing else.
#[test]
fn an_isolated_item_is_all_that_is_left_on_its_page() {
    let mut doc = fixture();
    let first = AddedText { rect: [72.0, 600.0, 300.0, 700.0], text: "First".into(), size: 14.0, ..AddedText::default() };
    let second = AddedText { rect: [72.0, 400.0, 300.0, 500.0], text: "Second".into(), ..first.clone() };
    add_content(&mut doc, 0, &Content::Text(first)).unwrap();
    add_content(&mut doc, 0, &Content::Text(second)).unwrap();
    let before = streams(&doc, 0).join("\n");
    assert!(before.contains("(First) Tj") && before.contains("(Second) Tj"), "{before}");
    assert!(isolate_content(&mut doc, 0, 2).is_err(), "no third item");
    assert!(isolate_content(&mut doc, 9, 0).is_err(), "no such page");
    let page = pdfcraft_model::pages(&doc).swap_remove(0);
    doc.update_dict(page.obj, |d| d.set(b"Annots".to_vec(), Object::Array(Vec::new()))).unwrap();
    isolate_content(&mut doc, 0, 1).unwrap();
    let after = streams(&doc, 0).join("\n");
    assert!(after.contains("(Second) Tj") && !after.contains("(First) Tj"), "{after}");
    let page = pdfcraft_model::pages(&doc).swap_remove(0);
    assert!(page.dict.get(b"Annots").is_none(), "no annotations are drawn over it");
    assert_eq!(list_added(&doc).len(), 1);
}

/// Edit ▸ Paste (#2636): where a pasted item lands.
#[test]
fn a_paste_lands_where_the_copy_was_offset_per_paste_and_stays_on_the_page() {
    use crate::added::{PASTE_STEP, paste_rect};
    let page = [300.0, 400.0];
    let rect = [40.0, 300.0, 140.0, 320.0];
    assert_eq!(paste_rect(rect, page, 0, None), rect, "in place");
    let d = 2.0 * PASTE_STEP;
    assert_eq!(paste_rect(rect, page, 2, None), [40.0 + d, 300.0 - d, 140.0 + d, 320.0 - d], "two steps down and right");
    assert_eq!(paste_rect(rect, page, 1, Some([10.0, 50.0])), [10.0, 30.0, 110.0, 50.0], "its top-left corner at the point");
    assert_eq!(paste_rect(rect, page, 0, Some([290.0, 5.0])), [200.0, 0.0, 300.0, 20.0], "pulled back onto the page");
    assert_eq!(paste_rect([0.0, 0.0, 500.0, 600.0], page, 0, Some([10.0, 10.0])), [0.0, -200.0, 500.0, 400.0], "too large: left and top on the page");
    assert_eq!(paste_rect([140.0, 320.0, 40.0, 300.0], page, 0, None), rect, "a reversed box is normalized");
    assert_eq!(paste_rect(rect, [f64::NAN, 400.0], 1, None), rect, "a page without a size leaves the box alone");
    assert_eq!(paste_rect(rect, page, 1, Some([f64::INFINITY, 5.0])), rect, "so does a point that isn't a number");
    assert_eq!(paste_rect(rect, page, u32::MAX, None), [200.0, 0.0, 300.0, 20.0], "many pastes stop at the page's corner");
}

/// A paste steps on past exact copies of the item (its original, earlier pastes), and only those.
#[test]
fn a_paste_steps_past_copies_of_itself_only() {
    use crate::added::{PASTE_STEP, paste_place};
    let page = [300.0, 400.0];
    let text = |rect, t: &str| Content::Text(AddedText { rect, text: t.into(), ..AddedText::default() });
    let item = text([40.0, 300.0, 140.0, 314.4], "Note");
    let step = |n: f64| [40.0 + n * PASTE_STEP, 300.0 - n * PASTE_STEP, 140.0 + n * PASTE_STEP, 314.4 - n * PASTE_STEP];
    assert_eq!(paste_place([], &item, page), step(0.0), "an empty page: where it was");
    assert_eq!(paste_place([&item], &item, page), step(1.0), "beside its original");
    let first = item.with_rect(step(1.0));
    assert_eq!(paste_place([&item, &first], &item, page), step(2.0), "beside the last copy");
    assert_eq!(paste_place([&first], &item, page), step(0.0), "its place is free again (the original was cut)");
    let other = text(step(0.0), "Other text");
    assert_eq!(paste_place([&other], &item, page), step(0.0), "a different item in the way doesn't count");
    // A page full of copies at its corner: the search ends.
    let corner = item.with_rect([200.0, 0.0, 300.0, 14.4]);
    let stuck = paste_place([&corner], &item.with_rect([200.0, 0.0, 300.0, 14.4]), page);
    assert_eq!(stuck, [200.0, 0.0, 300.0, 14.4]);
    // An image pasted into another document draws its picture from an object of that document:
    // still a copy, so the next paste steps past it.
    let image = Content::Image(AddedImage { crop: [0.1, 0.0, 0.0, 0.0], ..AddedImage::new(step(0.0), pdfcraft_cos::ObjRef::new(7, 0)) });
    let pasted = Content::Image(AddedImage { crop: [0.1, 0.0, 0.0, 0.0], ..AddedImage::new(step(0.0), pdfcraft_cos::ObjRef::new(42, 0)) });
    assert_eq!(paste_place([&pasted], &image, page), step(1.0), "a copy whose picture is stored anew");
    let turned = Content::Image(AddedImage { rotation: 1, ..AddedImage::new(step(0.0), pdfcraft_cos::ObjRef::new(7, 0)) });
    assert_eq!(paste_place([&turned], &image, page), step(0.0), "the same picture turned or cropped otherwise isn't a copy");
}

#[test]
fn added_arabic_text_is_shaped_in_display_order_and_stays_searchable() {
    let mut doc = fixture();
    let text =
        AddedText { rect: [72.0, 600.0, 400.0, 700.0], text: "مرحبا World".into(), size: 14.0, align: Align::Right, ..AddedText::default() };
    if pdfcraft_fonts::document_arabic_font().is_none() {
        eprintln!("skipping the Arabic drawing checks: built without a craft-fonts Arab face (set CRAFT_FONTS_DIR)");
        let err = add_content(&mut doc, 0, &Content::Text(text)).unwrap_err();
        assert!(matches!(&err, EditError::Invalid(m) if m.contains("CRAFT_FONTS_DIR")), "{err:?}");
        assert!(!doc.is_modified(), "nothing is written when the font is missing");
        return;
    }
    add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap();
    // A second Arabic item on the same page keeps its own fonts.
    add_content(&mut doc, 0, &Content::Text(AddedText { text: "شكرا".into(), rect: [72.0, 500.0, 400.0, 520.0], ..text.clone() })).unwrap();
    let doc = reopen(&doc);
    let added = list_added(&doc);
    let Content::Text(t) = &added[0].content else { panic!() };
    assert_eq!(t.text, "مرحبا World", "the logical text is kept for editing");
    let s = streams(&doc, 0);
    let item = s.iter().find(|s| s.contains("(World) Tj")).unwrap();
    // A right-to-left paragraph: "World" is drawn left of the Arabic word.
    let x_before = |s: &str, at: usize| -> f64 {
        let tm = s[..at].rfind("1 0 0 1 ").unwrap() + 8;
        s[tm..].split(' ').next().unwrap().parse().unwrap()
    };
    let latin_x = x_before(item, item.find("(World) Tj").unwrap());
    let arabic_at = item.find("/PCAr").unwrap();
    let arabic_x = x_before(item, item[arabic_at..].find(" Tj").unwrap() + arabic_at);
    assert!(latin_x < arabic_x, "{item}");
    // The Arabic glyphs come from Type 3 fonts whose ToUnicode maps give back each letter.
    let p = &pdfcraft_model::pages(&doc)[0];
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap()).as_dict().unwrap().clone();
    let arabic: Vec<_> = fonts.iter().filter(|(k, _)| k.starts_with(b"PCAr")).map(|(_, v)| doc.resolve(v).as_dict().unwrap().clone()).collect();
    assert_eq!(arabic.len(), 2, "one font per item");
    let mut mapped = String::new();
    for f in &arabic {
        assert_eq!(f.name(b"Subtype"), Some(&b"Type3"[..]));
        mapped += &String::from_utf8_lossy(&stream_bytes(&doc, f.get(b"ToUnicode").unwrap()).unwrap());
    }
    for letter in ["0645", "0631", "062D", "0628", "0627", "0634", "0643"] {
        assert!(mapped.contains(&format!("<{letter}>")), "{letter} in {mapped}");
    }
}

#[test]
fn added_hebrew_text_is_drawn_right_to_left_and_stays_searchable() {
    let mut doc = fixture();
    let text = AddedText {
        rect: [72.0, 600.0, 400.0, 700.0],
        text: "שלום (עולם) 2026 World".into(),
        size: 14.0,
        align: Align::Right,
        ..AddedText::default()
    };
    if pdfcraft_fonts::document_hebrew_font(false).is_none() {
        eprintln!("skipping the Hebrew drawing checks: built without a craft-fonts Hebr face (set CRAFT_FONTS_DIR)");
        assert_eq!(first_undrawable(&text), Some('ש'));
        let err = add_content(&mut doc, 0, &Content::Text(text)).unwrap_err();
        assert!(matches!(&err, EditError::Invalid(m) if m.contains("U+05E9")), "{err:?}");
        assert!(!doc.is_modified(), "nothing is written when the font is missing");
        return;
    }
    assert_eq!(first_undrawable(&text), None);
    add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap();
    // Points stay on their letters; a Hebrew line in an Arabic item and Arabic in a Hebrew one.
    for other in ["שָׁלוֹם", "שלום مرحبا"] {
        let other = AddedText { text: other.into(), rect: [72.0, 500.0, 400.0, 520.0], ..text.clone() };
        if other.text.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)) && pdfcraft_fonts::document_arabic_font().is_none() {
            continue;
        }
        add_content(&mut doc, 0, &Content::Text(other)).unwrap();
    }
    let doc = reopen(&doc);
    let added = list_added(&doc);
    let Content::Text(t) = &added[0].content else { panic!() };
    assert_eq!(t.text, "שלום (עולם) 2026 World", "the logical text is kept for editing");
    let s = streams(&doc, 0);
    let item = s.iter().find(|s| s.contains("(World) Tj")).unwrap();
    // A right-to-left paragraph: "World", typed last, is drawn left of the Hebrew.
    let x_before = |s: &str, at: usize| -> f64 {
        let tm = s[..at].rfind("1 0 0 1 ").unwrap() + 8;
        s[tm..].split(' ').next().unwrap().parse().unwrap()
    };
    let latin_x = x_before(item, item.find("(World) Tj").unwrap());
    let hebrew_at = item.find("/PCAr").unwrap();
    let hebrew_x = x_before(item, item[hebrew_at..].find(" Tj").unwrap() + hebrew_at);
    assert!(latin_x < hebrew_x, "{item}");
    // Copy and search get the typing order back.
    let logical: String = t.text.encode_utf16().map(|u| format!("{u:04X}")).collect();
    assert!(item.contains(&format!("/ActualText <FEFF{logical}>")), "{item}");
    // The Hebrew glyphs come from a Type 3 font named after the face, whose ToUnicode map gives
    // back each letter.
    let p = &pdfcraft_model::pages(&doc)[0];
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap()).as_dict().unwrap().clone();
    let shaped: Vec<_> = fonts.iter().filter(|(k, _)| k.starts_with(b"PCAr")).map(|(_, v)| doc.resolve(v).as_dict().unwrap().clone()).collect();
    let mut mapped = String::new();
    for f in &shaped {
        assert_eq!(f.name(b"Subtype"), Some(&b"Type3"[..]));
        mapped += &String::from_utf8_lossy(&stream_bytes(&doc, f.get(b"ToUnicode").unwrap()).unwrap());
    }
    for letter in ["05E9", "05DC", "05D5", "05DD", "05E2"] {
        assert!(mapped.contains(&format!("<{letter}>")), "{letter} in {mapped}");
    }
    let font_name = |f: &Dict| doc.resolve(f.get(b"FontDescriptor").unwrap()).as_dict().unwrap().name(b"FontName").map(<[u8]>::to_vec);
    assert!(shaped.iter().any(|f| font_name(f).as_deref() == Some(&b"NotoSansHebrew"[..])));
    // A pointed letter is one glyph with its points, mapped back whole.
    assert!(mapped.contains("<05E905"), "{mapped}");
}

#[test]
fn odd_hebrew_text_never_panics() {
    let mut doc = fixture();
    let long = "שלום עולם ".repeat(2_000);
    for s in ["\u{202E}ש\u{05B8}\u{05B8} (]", "\u{05B8}", "א\u{200F}\u{2067}b\u{2069}", "א\n\nב\tc", "׳״ 456", long.as_str()] {
        let r = add_content(&mut doc, 0, &Content::Text(AddedText { rect: [0.0, 800.0, 300.0, 780.0], text: s.into(), ..AddedText::default() }));
        assert_eq!(r.is_ok(), pdfcraft_fonts::document_hebrew_font(false).is_some(), "{:?}: {r:?}", s.chars().take(12).collect::<String>());
    }
}

#[test]
fn arabic_items_keep_their_own_fonts_through_saves_updates_and_deletes() {
    let mut doc = fixture();
    let text = AddedText { rect: [72.0, 600.0, 400.0, 700.0], text: "مرحبا".into(), size: 14.0, ..AddedText::default() };
    if pdfcraft_fonts::document_arabic_font().is_none() {
        // An item that already holds Arabic (stored by an older version, drawn as "?") can still
        // be moved and retyped without the face.
        add_content(&mut doc, 0, &Content::Text(AddedText { text: "x".into(), ..text.clone() })).unwrap();
        update_content(&mut doc, 0, 0, &Content::Text(text)).unwrap();
        assert!(streams(&doc, 0).join("").contains("(?????) Tj"));
        return;
    }
    // A full save renumbers objects but keeps resource names, so a font named after its object
    // number may meet a name already on the page: here, every name it could take.
    let mut taken = Dict::new();
    for num in 1..2_000 {
        taken.set(format!("PCAr{num}").into_bytes(), Object::name("Taken"));
    }
    let mut res = Dict::new();
    res.set(b"Font".to_vec(), Object::Dict(taken));
    let page = pdfcraft_model::pages(&doc)[0].obj;
    doc.update_dict(page, |d| d.set(b"Resources".to_vec(), Object::Dict(res))).unwrap();
    add_content(&mut doc, 0, &Content::Text(text.clone())).unwrap();
    add_content(&mut doc, 0, &Content::Text(AddedText { text: "شكرا".into(), rect: [72.0, 500.0, 400.0, 520.0], ..text.clone() })).unwrap();
    let names = arabic_font_names(&doc, 0);
    assert_eq!(names.len(), 1_999 + 2, "nothing replaced");
    let p = &pdfcraft_model::pages(&doc)[0];
    let fonts = doc.resolve(p.dict.get(b"Resources").unwrap()).as_dict().unwrap().get(b"Font").cloned().unwrap();
    let fonts = doc.resolve(&fonts).as_dict().unwrap().clone();
    assert_eq!(fonts.iter().filter(|(_, v)| v.as_name() == Some(&b"Taken"[..])).count(), 1_999);
    let first = own(&doc, 0);
    assert!(first.len() == 1 && first[0].contains('_'), "a free name, not a taken one: {first:?}");
    // Updating an item swaps its fonts; deleting it removes them.
    update_content(&mut doc, 0, 0, &Content::Text(AddedText { text: "أهلا".into(), ..text })).unwrap();
    let names = arabic_font_names(&doc, 0);
    assert!(names.len() == 2_001 && !names.contains(&first[0]), "the replaced item's font is gone");
    delete_content(&mut doc, 0, 0).unwrap();
    delete_content(&mut doc, 0, 0).unwrap();
    assert_eq!(arabic_font_names(&doc, 0).len(), 1_999, "only the page's own names are left");
    reopen(&doc);
}

#[test]
fn odd_arabic_text_never_panics() {
    let mut doc = fixture();
    let long = "بسم الله ".repeat(2_000);
    for s in ["\u{202E}ب\u{064B}\u{064B} (]", "\u{064B}", "ا\u{200F}\u{2067}b\u{2069}", "ا\n\nب\tc", "ﷺ ١٢٣ 456", long.as_str()] {
        let r = add_content(&mut doc, 0, &Content::Text(AddedText { rect: [0.0, 800.0, 300.0, 780.0], text: s.into(), ..AddedText::default() }));
        assert_eq!(r.is_ok(), pdfcraft_fonts::document_arabic_font().is_some(), "{:?}: {r:?}", s.chars().take(12).collect::<String>());
    }
    // U+2029 and U+0085 (paragraph and line separators) have no WinAnsi code, so the standard font
    // would draw them as `?`: refused, by name, rather than written (#125). Laying it out still
    // mustn't panic.
    let r = add_content(
        &mut doc,
        0,
        &Content::Text(AddedText { rect: [0.0, 800.0, 300.0, 780.0], text: "ا\u{2029}ب\rc\u{85}د\n".into(), ..AddedText::default() }),
    );
    assert!(r.is_err(), "{r:?}");
    if pdfcraft_fonts::document_arabic_font().is_some() {
        assert!(matches!(&r, Err(EditError::Invalid(m)) if m.contains("U+2029") || m.contains("U+0085")), "{r:?}");
    }
    // A character the face lacks is an error that names it, not a box or a crash.
    if pdfcraft_fonts::document_arabic_font().is_some() && !pdfcraft_fonts::arabic_has('\u{FDFD}') {
        let r = add_content(
            &mut doc,
            0,
            &Content::Text(AddedText { rect: [0.0, 800.0, 300.0, 780.0], text: "ب \u{FDFD}".into(), ..AddedText::default() }),
        );
        assert!(matches!(&r, Err(EditError::Invalid(m)) if m.contains('\u{FDFD}')), "{r:?}");
    }
}

#[test]
fn tracked_text_keeps_words_in_paragraphs_and_objects() {
    let expected = "TRACKED SAMPLE TEXT";
    for spacing in [1.5, 2.0001, 3.0] {
        for scale in [50.0, 100.0, 150.0] {
            for positioned in [false, true] {
                let mut show = String::new();
                if !positioned {
                    show.push('[');
                }
                for (i, c) in expected.chars().enumerate() {
                    if positioned {
                        show.push_str(&format!("({c}) Tj "));
                        if i + 1 < expected.len() {
                            show.push_str(&format!("[{}] TJ ", -spacing * 100.0));
                        }
                    } else {
                        show.push_str(&format!("({c}) "));
                        if i + 1 < expected.len() {
                            show.push_str(&format!("{} ", -spacing * 100.0));
                        }
                    }
                }
                if !positioned {
                    show.push_str("] TJ");
                }
                let doc = text_page(&format!("BT /F1 10 Tf {scale} Tz 72 700 Td {show} ET"));
                assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, expected, "{spacing}, {scale}, {positioned}");
                assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].text, expected);
                assert_eq!(crate::objects::editable_objects(&doc, 0).unwrap()[0].text.as_deref(), Some(expected));
            }
        }
        let doc = text_page(&format!("BT /F1 10 Tf {spacing} Tc 72 700 Td ({expected}) Tj ET"));
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].text, expected);
    }
}

#[test]
fn single_glyph_words_and_cells_are_not_tracking() {
    for (content, expected) in [
        ("BT /F1 10 Tf 72 700 Td [(a) -1000 (b) -1000 (c) -1000 (d)] TJ ET", "a b c d"),
        ("BT /F1 10 Tf 72 700 Td [(1) -2500 (2) -2500 (3) -2500 (4) -2500 (5)] TJ ET", "1 2 3 4 5"),
        ("BT /F1 10 Tf 72 700 Td [(A) -300 (B) -1200 (C) -1200 (D) -1200 (E) -300 (F) -1200 (G)] TJ ET", "A B C D E F G"),
    ] {
        assert_eq!(text::text_blocks(&text_page(content), 0).unwrap()[0].text, expected, "{content}");
    }
    let mut s = String::from("BT /F1 10 Tf ");
    for (i, c) in "12345".chars().enumerate() {
        s.push_str(&format!("1 0 0 1 {} 700 Tm ({c}) Tj ", 72 + i * 12));
    }
    s.push_str("ET");
    assert_eq!(text::text_blocks(&text_page(&s), 0).unwrap()[0].text, "1 2 3 4 5");
}

#[test]
fn tracked_text_preserves_larger_word_gaps() {
    for show in [
        "[(T) -300 (E) -300 (S) -300 (T) -600 (W) -300 (O) -300 (R) -300 (D)] TJ",
        "(T) Tj [-300] TJ (E) Tj [-300] TJ (S) Tj [-300] TJ (T) Tj [-600] TJ (W) Tj [-300] TJ (O) Tj [-300] TJ (R) Tj [-300] TJ (D) Tj",
        "[(TEST) -300 (WORD)] TJ",
        "(TEST) Tj [-300] TJ (W) Tj [-300] TJ (O) Tj [-300] TJ (R) Tj [-300] TJ (D) Tj",
    ] {
        let doc = text_page(&format!("BT /F1 10 Tf 72 700 Td {show} ET"));
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].text, "TEST WORD", "{show}");
    }
}
