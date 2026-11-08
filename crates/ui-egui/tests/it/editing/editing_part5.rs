//! Editing tests through the app UI (part 5 of 7).

#[allow(unused_imports)]
use super::*;

/// A resized picture is applied when the corner is let go, but the page raster still has it at
/// its old size until the page is rendered again. Until then it stays drawn at its new size and
/// blanked at its old one; the old one used to show again for those frames.
#[test]
fn a_resized_image_keeps_its_new_size_until_the_page_is_redrawn() {
    use pdfcraft_cos::{Dict, Document, Object, Stream, write_incremental};
    let _gpu = gpu();
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let mut image = Dict::new();
    image.set(b"Type".to_vec(), Object::name("XObject"));
    image.set(b"Subtype".to_vec(), Object::name("Image"));
    image.set(b"Width".to_vec(), Object::Int(1));
    image.set(b"Height".to_vec(), Object::Int(1));
    image.set(b"BitsPerComponent".to_vec(), Object::Int(8));
    image.set(b"ColorSpace".to_vec(), Object::name("DeviceRGB"));
    let image = doc.add(Object::Stream(Stream::flate(image, &[200, 30, 30])));
    let page = pdfcraft_model::pages(&doc).remove(0);
    let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), b"q 80 0 0 80 60 40 cm /Im0 Do Q")));
    let mut resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let mut xo = Dict::new();
    xo.set(b"Im0".to_vec(), Object::Ref(image));
    resources.set(b"XObject".to_vec(), Object::Dict(xo));
    doc.update_dict(page.obj, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(contents));
    })
    .unwrap();
    let bytes = write_incremental(&doc, &Default::default()).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].set_zoom(1.2);
    h.run_steps(4);
    support::wait_until(&mut h, "the page raster", |app| !app.render_pending());
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let at = |x: f32, y: f32| egui::pos2(page.left() + x / 200.0 * page.width(), page.top() + (300.0 - y) / 300.0 * page.height());
    click_at(&mut h, at(100.0, 80.0));
    assert!(h.state().views[0].image_selection.is_some(), "the picture is selected");
    support::wait_until(&mut h, "the page raster", |app| !app.render_pending());
    // From 60..140 × 40..120 to half its size, keeping its lower left corner.
    let (grip, to) = (at(140.0, 120.0), at(100.0, 80.0));
    let kept = egui::Rect::from_two_pos(at(62.0, 42.0), at(98.0, 78.0));
    let gone = egui::Rect::from_two_pos(at(104.0, 84.0), at(138.0, 118.0));
    h.hover_at(grip);
    h.run_steps(1);
    h.drag_at(grip);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(grip + (to - grip) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    // While dragged, no outline stays where the picture was.
    let old = egui::Rect::from_two_pos(at(60.0, 40.0), at(140.0, 120.0));
    let trailing = h.output().shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(r) if r.stroke.width == 0.75 && (r.rect.min - old.min).length() < 2.0 && (r.rect.max - old.max).length() < 2.0)
    });
    assert!(!trailing, "the dragged picture's outline trails behind it");
    h.state_mut().views[0].hold_rasters = true;
    h.drop_at(to);
    for frame in 0..4 {
        h.step();
        let pixels = h.render().unwrap();
        assert!(red(&pixels, kept) > 100, "frame {frame} after the release: the picture is not at its new size");
        assert_eq!(red(&pixels, gone), 0, "frame {frame} after the release: the picture shows at its old size");
    }
    // Resized again before the page is drawn: the raster still has the first size, so that is
    // what stays blanked while the corner moves.
    let (grip, to) = (to, at(80.0, 60.0));
    let again = egui::Rect::from_two_pos(at(62.0, 42.0), at(78.0, 58.0));
    h.hover_at(grip);
    h.run_steps(1);
    h.drag_at(grip);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(grip + (to - grip) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    let pixels = h.render().unwrap();
    assert!(red(&pixels, again) > 20, "the second drag draws the picture at its new size");
    assert_eq!(red(&pixels, gone), 0, "the second drag shows the picture at its first size");
    h.drop_at(to);
    h.run_steps(2);
    let pixels = h.render().unwrap();
    assert_eq!(red(&pixels, gone), 0, "after the second release");
    h.state_mut().views[0].hold_rasters = false;
    support::wait_until(&mut h, "the page drawn with the resized picture", |app| !app.render_pending());
    h.run_steps(2);
    let pixels = h.render().unwrap();
    assert!(red(&pixels, again) > 20 && red(&pixels, gone) == 0, "the page raster has the picture at its new size");
}

/// Exercise actual pointer gestures on each cropped placement, without capturing private pixels.
#[test]
#[ignore = "requires an explicitly supplied local PDF with cropped artwork"]
fn private_cropped_image_grips_resize_without_moving_the_opposite_corner() {
    let path = std::env::var("PDFCRAFT_CLIPPED_ARTWORK_PDF").expect("supply a local PDF path");
    let original = std::fs::read(&path).unwrap();
    let bytes = original.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("private.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("view.layout.single"));
    let id = h.state().views[0].id;
    let pages = h.state().session.get(id).unwrap().info.pages.len();
    let mut checked = 0;
    for page_index in 0..pages {
        h.state_mut().views[0].go_to_page(page_index);
        h.run_steps(4);
        let doc = h.state().session.get(id).unwrap();
        let info = doc.info.pages[page_index].clone();
        let images = doc.page_images(page_index);
        let blocks = doc.text_blocks(page_index);
        let page = h.state().views[0].page_screen_rect(page_index).unwrap();
        let at = |x: f64, y: f64| {
            let v = info.user_to_view(x as f32, y as f32);
            page.min + egui::vec2(v[0], v[1]) * (page.width() / info.width)
        };
        for (index, image) in images.iter().enumerate() {
            let Some(b) = image.visible_rect else { continue };
            let blank = (1..10)
                .flat_map(|x| (1..10).map(move |y| (x, y)))
                .map(|(x, y)| (b[0] + (b[2] - b[0]) * x as f64 / 10.0, b[1] + (b[3] - b[1]) * y as f64 / 10.0))
                .find(|(x, y)| !blocks.iter().any(|l| *x >= l.rect[0] && *x <= l.rect[2] && *y >= l.rect[1] && *y <= l.rect[3]))
                .expect("image has an unlabelled selection point");
            let corners = [at(b[0], b[3]), at(b[2], b[3]), at(b[0], b[1]), at(b[2], b[1])];
            for corner in 0..4 {
                for (factor, axis) in [(0.8, 0), (1.04, 0), (0.8, 1), (0.8, 2)] {
                    let p = at(blank.0, blank.1);
                    h.hover_at(p);
                    h.run_steps(1);
                    h.drag_at(p);
                    h.run_steps(1);
                    h.drop_at(p);
                    h.run_steps(3);
                    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(index));
                    let generation = h.state().session.get(id).unwrap().edit_generation();
                    let from = corners[corner];
                    let fixed = corners[3 - corner];
                    let desired = fixed + (from - fixed) * factor;
                    let to = match axis {
                        1 => egui::pos2(desired.x, from.y),
                        2 => egui::pos2(from.x, desired.y),
                        _ => desired,
                    };
                    h.hover_at(from);
                    h.run_steps(1);
                    h.drag_at(from);
                    h.run_steps(1);
                    for k in 1..=5 {
                        h.hover_at(from + (to - from) * (k as f32 / 5.0));
                        h.run_steps(1);
                        assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation, "no PDF rewrite on pointer movement");
                    }
                    h.drop_at(to);
                    h.run_steps(4);
                    let after = h.state().session.get(id).unwrap().page_images(page_index)[index].visible_rect.unwrap();
                    let actual = egui::Rect::from_two_pos(at(after[0], after[1]), at(after[2], after[3]));
                    let expected = egui::Rect::from_two_pos(fixed, desired).intersect(page);
                    assert!(
                        (actual.min - expected.min).length() < 0.2 && (actual.max - expected.max).length() < 0.2,
                        "page {}, image {}, corner {corner}: {actual:?} vs {expected:?}",
                        page_index + 1,
                        index + 1
                    );
                    assert!(h.state().views[0].line_editor.is_none());
                    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
                    h.run_steps(4);
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(h.state().session.get(id).unwrap().bytes.as_slice(), original.as_slice(), "undo restored the source document in memory");
    assert_eq!(std::fs::read(path).unwrap(), original);
    println!("verified {checked} real-file corner drag gestures on {pages} pages");
}

/// Local acceptance check for PDFs containing cropped artwork. No screenshots or source saves.
#[test]
#[ignore = "requires an explicitly supplied local PDF with cropped artwork"]
fn private_document_cropped_artwork_does_not_block_text() {
    let path = std::env::var("PDFCRAFT_CLIPPED_ARTWORK_PDF").expect("supply a local PDF path");
    let original = std::fs::read(&path).unwrap();
    let bytes = original.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("private.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("view.layout.single"));
    h.run_steps(3);
    let pages = h.state().session.get(h.state().views[0].id).unwrap().info.pages.len();
    let mut checked = 0;
    let mut cropped = 0;
    for page in 0..pages {
        h.state_mut().views[0].go_to_page(page);
        h.run_steps(4);
        let doc = h.state().session.get(h.state().views[0].id).unwrap();
        let info = doc.info.pages[page].clone();
        let blocks = doc.text_blocks(page);
        let images = doc.page_images(page);
        let r = h.state().views[0].page_screen_rect(page).unwrap();
        for image in &images {
            let visible = image.visible_rect.unwrap();
            assert!(visible[0] >= f64::from(info.crop[0]) - 0.01 && visible[1] >= f64::from(info.crop[1]) - 0.01);
            assert!(visible[2] <= f64::from(info.crop[2]) + 0.01 && visible[3] <= f64::from(info.crop[3]) + 0.01);
            if image.rect != visible {
                cropped += 1;
            }
        }
        for (index, block) in blocks.iter().enumerate() {
            let [x0, y0, x1, y1] = block.rect;
            let v = info.user_to_view(((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32);
            let scale = r.width() / info.width;
            let p = egui::pos2(r.left() + v[0] * scale, r.top() + v[1] * scale);
            h.hover_at(p);
            h.run_steps(1);
            h.drag_at(p);
            h.run_steps(1);
            h.drop_at(p);
            h.run_steps(3);
            assert_eq!(h.state().views[0].line_editor.as_ref().map(|e| e.block), Some(index), "every source block can open directly");
            assert!(h.state().views[0].image_selection.is_none());
            h.key_press(Key::Escape);
            h.run_steps(2);
            checked += 1;
        }
    }
    assert!(checked > 0 && cropped > 0);
    assert!(!dirty(&h));
    assert_eq!(std::fs::read(path).unwrap(), original);
    println!("checked {pages} pages, {checked} text regions, {cropped} clipped image placements");
}

/// Actual keyboard input into every page of a private Chinese PDF; no private captures.
#[test]
#[ignore = "requires a private Chinese PDF and an approved full source font file"]
fn private_new_chinese_keyboard_input_preserves_font_layout_and_saved_pixels() {
    use pdfcraft_engine::{Edit, Session};
    use std::sync::Arc;
    let path = std::env::var("PDFCRAFT_CHINESE_TEXT_PDF").expect("supply a local PDF path");
    let font_path = std::env::var("PDFCRAFT_CHINESE_FULL_FONT").expect("supply the full source face");
    let original = std::fs::read(&path).unwrap();
    let bytes = original.clone();
    let font = Arc::new(std::fs::read(font_path).unwrap());
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("private.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("view.layout.single"));
    let id = h.state().views[0].id;
    let pages = h.state().session.get(id).unwrap().info.pages.len();
    let all: String = (0..pages).flat_map(|p| h.state().session.get(id).unwrap().text_lines(p)).map(|l| l.text).collect();
    let new: String = "测试编辑删除龘".chars().filter(|c| !all.contains(*c)).take(4).collect();
    assert!(new.chars().count() >= 2);
    let pixels = |bytes: Arc<Vec<u8>>, page| {
        let mut r = PageRenderer::new(bytes, Default::default());
        let p = r.render(RenderRequest { page, scale: 1.5, ..Default::default() });
        assert!(p.error.is_none() && p.warnings.is_empty());
        p.rgba.to_vec()
    };
    for page_index in 0..pages {
        h.state_mut().views[0].go_to_page(page_index);
        h.state_mut().session.apply(id, Edit::RegisterTextFont { page: page_index, bytes: font.clone() }).unwrap();
        h.run_steps(4);
        let doc = h.state().session.get(id).unwrap();
        let (index, block) = doc
            .text_blocks(page_index)
            .into_iter()
            .enumerate()
            .find(|(_, b)| b.rows.len() == 1 && b.editor_text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)))
            .unwrap();
        let source = doc.text_lines(page_index)[block.lines[0]].clone();
        let info = doc.info.pages[page_index].clone();
        let page = h.state().views[0].page_screen_rect(page_index).unwrap();
        let scale = page.width() / info.width;
        let v = info.user_to_view((source.rect[0] + 3.0) as f32, ((source.rect[1] + source.rect[3]) / 2.0) as f32);
        let point = page.min + egui::vec2(v[0], v[1]) * scale;
        h.hover_at(point);
        h.run_steps(1);
        h.drag_at(point);
        h.run_steps(1);
        h.drop_at(point);
        h.run_steps(8);
        assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().block, index);
        let draft = format!("{}{new}", block.editor_text);
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text(draft.clone()));
        h.run_steps(4);
        assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, draft);
        let galley = h
            .output()
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == draft => Some(t.galley.clone()),
                _ => None,
            })
            .expect("typed Chinese is in the actual inline editor");
        assert!(
            galley
                .job
                .sections
                .iter()
                .filter(|s| draft
                    .get(usize::from(s.byte_range.start)..usize::from(s.byte_range.end))
                    .is_some_and(|text| text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))))
                .all(|s| matches!(&s.format.font_id.family, egui::FontFamily::Name(n) if n.starts_with("pdf-source-")))
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
        h.run_steps(5);
        assert!(h.state().views[0].line_editor.is_none());
        let edited = h.state().session.get(id).unwrap();
        assert!(edited.text_lines(page_index).iter().any(|l| l.text.contains(&new) && l.base_font == "DroidSansFallback"));
        let first = edited.text_lines(page_index).into_iter().find(|l| l.text.starts_with(source.text.trim())).unwrap();
        assert!(first.baseline().iter().zip(source.baseline()).all(|(a, b)| (a - b).abs() < 0.01));
        let current = pixels(edited.display.clone(), page_index);
        let saved = h.state_mut().session.save_bytes(id).unwrap();
        let mut reopen = Session::new();
        let rid = reopen.open("saved.pdf", None, saved, None).unwrap();
        assert_eq!(pixels(reopen.get(rid).unwrap().display.clone(), page_index), current);
        // Repeat keyboard input and deletion without recreating the editor via state mutation.
        h.hover_at(point);
        h.run_steps(1);
        h.drag_at(point);
        h.run_steps(1);
        h.drop_at(point);
        h.run_steps(6);
        assert!(h.state().views[0].line_editor.as_ref().unwrap().text.contains(&new));
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        let shortened = draft.replace(&new, &new.chars().skip(1).collect::<String>());
        h.event(egui::Event::Text(shortened.clone()));
        h.run_steps(3);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
        h.run_steps(4);
        let current = pixels(h.state().session.get(id).unwrap().display.clone(), page_index);
        let saved = h.state_mut().session.save_bytes(id).unwrap();
        let rid = reopen.open("deleted.pdf", None, saved, None).unwrap();
        assert_eq!(pixels(reopen.get(rid).unwrap().display.clone(), page_index), current);
        let content = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(content(&reopen.get(rid).unwrap().text_blocks(page_index)[index].editor_text), content(&shortened));
        h.state_mut().session.undo(id).unwrap();
        h.state_mut().session.undo(id).unwrap();
        h.state_mut().session.undo(id).unwrap();
        h.run_steps(4);
    }
    assert_eq!(std::fs::read(path).unwrap(), original);
    println!("verified Chinese keyboard insertion/deletion and save/reopen rendering on {pages} pages");
}

/// Moving text shows the four-way arrow wherever a drag would move it (#1520 follow-up): over a
/// closed paragraph, on the frame of an open one, and while dragging. Inside the open one the
/// pointer reads as text; on a closed one's edge, as the rewrap handle.
#[test]
fn moving_text_offers_the_four_way_arrow() {
    let mut h = harness(2, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    let centre = screen((block.rect[0] + block.rect[2]) / 2.0, (block.rect[1] + block.rect[3]) / 2.0);
    let cursor_at = |h: &mut Harness<'static, PdfCraftApp>, at: egui::Pos2| {
        h.hover_at(at);
        h.run_steps(2);
        h.output().platform_output.cursor_icon
    };
    // A closed paragraph: a drag moves it; its left edge rewraps it.
    assert_eq!(cursor_at(&mut h, centre), egui::CursorIcon::Move);
    assert_eq!(cursor_at(&mut h, screen(block.rect[0], (block.rect[1] + block.rect[3]) / 2.0)), egui::CursorIcon::ResizeHorizontal);
    // While it is being dragged.
    h.hover_at(centre);
    h.run_steps(1);
    h.drag_at(centre);
    h.run_steps(1);
    let mid = centre + egui::vec2(20.0 * k, 10.0 * k);
    h.hover_at(mid);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Move);
    h.drop_at(mid);
    h.run_steps(4);
    let moved = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    assert!((moved.rect[0] - block.rect[0] - 20.0).abs() < 1.5, "{:?} → {:?}", block.rect, moved.rect);
    // Open: text inside, the frame moves it.
    let centre = screen((moved.rect[0] + moved.rect[2]) / 2.0, (moved.rect[1] + moved.rect[3]) / 2.0);
    click_at(&mut h, centre);
    assert!(h.state().views[0].line_editor.is_some());
    assert_eq!(cursor_at(&mut h, centre), egui::CursorIcon::Text);
    let frame = screen((moved.rect[0] + moved.rect[2]) / 2.0, moved.rect[3]) - egui::vec2(0.0, 4.0);
    assert_eq!(cursor_at(&mut h, frame), egui::CursorIcon::Move);
}

/// Cropping a picture on the page (Acrobat's crop handles): in crop mode the
/// handles trim, Enter applies what is kept, Esc leaves the picture as it was.
#[test]
fn cropping_a_picture_with_its_handles() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 40, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().page_images(0)[0].visible_rect.unwrap();
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");
    // Crop mode (as the context menu's Crop Image starts it): drag the right-bottom handle a
    // quarter of the picture inward, then Enter.
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new(before));
    h.run_steps(2);
    let corner = r.right_bottom();
    let to = corner - egui::vec2(r.width() / 4.0, r.height() / 4.0);
    h.hover_at(corner);
    h.run_steps(1);
    h.drag_at(corner);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(corner + (to - corner) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(2);
    let keep = h.state().views[0].image_selection.as_ref().unwrap().crop.as_ref().unwrap().keep;
    assert!(keep[2] < before[2] - 1.0 && keep[1] > before[1] + 1.0, "trimmed on the right and the bottom: {keep:?} of {before:?}");
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Crop image"));
    let im = &doc.page_images(0)[0];
    assert_eq!(im.rect, [0.0, 0.0, 80.0, 40.0], "the placement is not scaled");
    let v = im.visible_rect.unwrap();
    assert!((v[2] - 60.0).abs() < 1.5 && (v[1] - 10.0).abs() < 1.5, "kept {v:?}");
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_none()), "crop mode ends");
    // Esc leaves it.
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new([10.0, 10.0, 20.0, 20.0]));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.page_images(0)[0].visible_rect.unwrap(), v);
    assert!(h.state().views[0].image_selection.as_ref().is_none_or(|s| s.crop.is_none()));
}

/// Crop mode holds every press on its page, and its cursor stays there: the grips show theirs and
/// the box kept the hand that drags it, and on the next page the pointer shows what a press there
/// takes (a paragraph's move arrow). Crop mode's cursor used to win on every page.
#[test]
fn crop_modes_cursor_stays_on_its_page() {
    let mut h = picture_pages(true);
    // The visible picture is x 30..160, y 170..250.
    for (page, x, y, cursor, what) in [
        (0, 160.0, 170.0, egui::CursorIcon::ResizeNwSe, "a corner grip"),
        (0, 95.0, 210.0, egui::CursorIcon::Grab, "the box kept"),
        (0, 65.0, 134.0, egui::CursorIcon::Default, "crop mode holds its page's paragraphs"),
        (1, 65.0, 134.0, egui::CursorIcon::Move, "the next page's paragraph"),
    ] {
        let p = page_at(&h, page, x, y);
        assert_eq!(cursor_over(&mut h, p), cursor, "{what}");
    }
}

/// A press on a crop grip a little outside the picture drags that grip, and Escape in the middle
/// of the drag cancels it: the box goes back and crop mode stays. Escape once the button is up
/// leaves crop mode.
#[test]
fn escape_during_a_crop_drag_keeps_crop_mode() {
    let mut h = picture_pages(true);
    let before = h.state().views[0].image_selection.as_ref().unwrap().crop.as_ref().unwrap().keep;
    let grip = page_at(&h, 0, 160.0, 170.0) + egui::vec2(5.0, 5.0);
    h.hover_at(grip);
    h.run_steps(1);
    h.drag_at(grip);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(grip - egui::vec2(10.0, 10.0) * k as f32);
        h.run_steps(1);
    }
    let keep = |h: &Harness<'static, PdfCraftApp>| h.state().views[0].image_selection.as_ref().and_then(|s| s.crop.as_ref()).map(|c| c.keep);
    assert!(keep(&h).is_some_and(|k| k[2] < before[2] - 5.0 && k[1] > before[1] + 5.0), "the grip outside the picture is dragged");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(keep(&h), Some(before), "Escape cancels the drag, and crop mode stays");
    h.drop_at(grip - egui::vec2(40.0, 40.0));
    h.run_steps(2);
    assert_eq!(keep(&h), Some(before), "the cancelled drag's release crops nothing");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(keep(&h), None, "Escape leaves crop mode");
    assert!(!dirty(&h));
}

/// With Edit text, a right-click on text lying over a picture is the picture's, which is selected
/// for its menu: paragraphs have no menu of their own (the user's ruling).
#[test]
fn a_right_click_on_text_over_a_picture_selects_the_picture() {
    let mut h = picture_pages(false);
    h.state_mut().views[0].image_selection = None;
    h.run_steps(2);
    // "Overlay" lies on the picture, at x 40, y 220.
    let overlay = page_at(&h, 0, 50.0, 224.0);
    right_click_at(&mut h, overlay);
    let selected = h.state().views[0].image_selection.as_ref().map(|s| (s.page, s.index));
    assert_eq!(selected, Some((0, 0)), "the picture under the text is selected for its menu");
}

/// With Edit text, a right-click on added text over a picture is the picture's, which is selected
/// for its menu: added text has no menu of its own.
#[test]
fn a_right_click_on_added_text_over_a_picture_selects_the_picture() {
    let mut h = picture_pages(false);
    h.state_mut().views[0].image_selection = None;
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::AddText {
        page: 0,
        text: pdfcraft_engine::AddedText { rect: [60.0, 190.0, 150.0, 205.0], text: "Added".into(), ..Default::default() },
    }));
    h.run_steps(3);
    let on_it = page_at(&h, 0, 100.0, 198.0);
    right_click_at(&mut h, on_it);
    let selected = h.state().views[0].image_selection.as_ref().map(|s| (s.page, s.index));
    assert_eq!(selected, Some((0, 0)), "the picture under the added text is selected for its menu");
    assert!(h.query_by_label("Flip horizontal").is_some(), "its menu opens");
}

/// With Edit text, a right-click on the frame of a paragraph open for typing over a picture is the
/// picture's too.
#[test]
fn a_right_click_on_an_open_paragraphs_frame_over_a_picture_selects_the_picture() {
    let mut h = picture_pages(false);
    h.state_mut().views[0].image_selection = None;
    h.run_steps(2);
    // "Overlay" lies on the picture, at x 40, y 220.
    let overlay = page_at(&h, 0, 50.0, 224.0);
    slipping_click(&mut h, overlay, overlay);
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph is open for typing");
    let id = h.state().views[0].id;
    let blocks = h.state().session.get(id).unwrap().text_blocks(0);
    let block =
        blocks.iter().find(|b| b.rect[0] < 50.0 && 50.0 < b.rect[2] && b.rect[1] < 224.0 && 224.0 < b.rect[3]).expect("the paragraph opened").clone();
    // Its frame runs just outside its box.
    let frame = page_at(&h, 0, block.rect[0] as f32, ((block.rect[1] + block.rect[3]) / 2.0) as f32) - egui::vec2(3.0, 0.0);
    right_click_at(&mut h, frame);
    let selected = h.state().views[0].image_selection.as_ref().map(|s| (s.page, s.index));
    assert_eq!(selected, Some((0, 0)), "the picture under the frame is selected for its menu");
    assert!(h.query_by_label("Flip horizontal").is_some(), "its menu opens");
}

/// A drag from the selected picture moves it, though another picture lies over it there; a click
/// there selects the one on top.
#[test]
fn a_drag_from_the_selected_picture_under_another_moves_the_selected_one() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("two.pdf", None, two_image_fixture()).unwrap();
        app
    });
    h.run_steps(4);
    // The second picture lies over the right half of the first.
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 1,
        change: pdfcraft_engine::ImageEdit::Move([110.0, 100.0, 130.0, 120.0]),
    });
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection::at(0, 0));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let rects = |h: &Harness<'static, PdfCraftApp>| {
        let images = h.state().session.get(id).unwrap().page_images(0);
        (images[0].rect, images[1].rect)
    };
    let before = rects(&h);
    let xf = h.state().views[0].page_xform(0).unwrap();
    let doc = h.state().session.get(id).unwrap();
    let images = doc.page_images(0);
    let frame = |i: usize| pdfcraft_ui_egui::canvas::image_screen_rect(&images[i], &xf, &doc.info, 0);
    let overlap = frame(0).intersect(frame(1)).center();
    h.hover_at(overlap);
    h.run_steps(1);
    h.drag_at(overlap);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(overlap + egui::vec2(8.0 * k as f32, 0.0));
        h.run_steps(1);
    }
    h.drop_at(overlap + egui::vec2(32.0, 0.0));
    h.run_steps(3);
    let after = rects(&h);
    assert_ne!(after.0, before.0, "the selected picture moves");
    assert_eq!(after.1, before.1, "the one over it stays");
    let selected = h.state().views[0].image_selection.as_ref().map(|s| s.index);
    assert_eq!(selected, Some(0), "and stays selected");
    // After the move the two still overlap, a little further along.
    let xf = h.state().views[0].page_xform(0).unwrap();
    let doc = h.state().session.get(id).unwrap();
    let images = doc.page_images(0);
    let frame = |i: usize| pdfcraft_ui_egui::canvas::image_screen_rect(&images[i], &xf, &doc.info, 0);
    let overlap = frame(0).intersect(frame(1)).center();
    slipping_click(&mut h, overlap, overlap);
    let selected = h.state().views[0].image_selection.as_ref().map(|s| s.index);
    assert_eq!(selected, Some(1), "a click selects the one on top");
}

/// A move too short to be a drag of the page still resizes the selected picture from its grip, as
/// the grip takes its drag from the press.
#[test]
fn a_short_drag_from_a_picture_grip_resizes_it() {
    let mut h = picture_pages(false);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().page_images(0)[0].matrix;
    // The picture shows at x 30..160, y 170..250: its lower right grip, moved 3 px.
    let grip = page_at(&h, 0, 160.0, 170.0);
    h.hover_at(grip);
    h.run_steps(1);
    h.drag_at(grip);
    h.run_steps(1);
    h.hover_at(grip + egui::vec2(3.0, 0.0));
    h.run_steps(1);
    h.drop_at(grip + egui::vec2(3.0, 0.0));
    h.run_steps(3);
    assert_ne!(h.state().session.get(id).unwrap().page_images(0)[0].matrix, before, "the picture is resized");
}

/// A click on a selected picture's grip that doesn't move resizes nothing: no edit, and the picture
/// stays selected.
#[test]
fn a_click_on_a_picture_grip_changes_nothing() {
    let mut h = picture_pages(false);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().edit_generation();
    let grip = page_at(&h, 0, 160.0, 170.0);
    slipping_click(&mut h, grip, grip);
    assert_eq!(h.state().session.get(id).unwrap().edit_generation(), before, "no edit");
    assert!(h.state().views[0].image_selection.is_some(), "the picture stays selected");
}

/// With Add text holding text that can't be drawn, a drag from a paragraph moves nothing: the draft
/// stays where it is being typed (#125), as a click there leaves it.
#[test]
fn add_text_moves_no_paragraph_while_it_holds_a_draft_it_cant_draw() {
    if crate::support::skip_without_glyphs("add_text_moves_no_paragraph_while_it_holds_a_draft_it_cant_draw", "en", "Hello 世界") {
        return;
    }
    let mut h = edit_page(pdfcraft_ui_egui::QuickTool::AddText);
    assert!(h.state_mut().execute("edit.text"));
    h.run_steps(3);
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let blank = point_on(&h, block.rect[0], block.rect[3] + 12.0);
    slipping_click(&mut h, blank, blank);
    h.event(egui::Event::Text("Hello 世界".into()));
    h.run_steps(2);
    assert_eq!(h.state().views[0].content.draft.as_ref().map(|d| d.text.as_str()), Some("Hello 世界"), "the draft holds the text");
    let rect = h.state().views[0].content.draft.as_ref().map(|d| d.rect).unwrap();
    let on_text = point_on(&h, (block.rect[0] + block.rect[2]) / 2.0, (block.rect[1] + block.rect[3]) / 2.0);
    h.hover_at(on_text);
    h.run_steps(1);
    h.drag_at(on_text);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(on_text + egui::vec2(10.0 * k as f32, 0.0));
        h.run_steps(1);
    }
    h.drop_at(on_text + egui::vec2(40.0, 0.0));
    h.run_steps(3);
    assert_eq!(h.state().session.get(id).unwrap().text_blocks(0)[0].rect, block.rect, "the paragraph stays");
    let draft = h.state().views[0].content.draft.as_ref().map(|d| (d.text.clone(), d.rect));
    assert_eq!(draft, Some(("Hello 世界".to_string(), rect)), "and the draft stays where it is typed");
}

/// With Edit text, a right-click on a comment over a picture opens the comment's menu, and the
/// picture is not selected for its own.
#[test]
fn a_right_click_on_a_comment_over_a_picture_opens_its_menu() {
    let mut h = picture_pages(false);
    h.state_mut().views[0].image_selection = None;
    let shape = pdfcraft_engine::Shape::Rectangle { rect: [60.0, 190.0, 150.0, 230.0] };
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::AddAnnotation(pdfcraft_engine::NewAnnotation {
        page: 0,
        style: pdfcraft_engine::Style::default_for(&shape),
        shape,
        contents: "Reviewed".into(),
        author: "Tester".into(),
    })));
    h.state_mut().views[0].comments.selected = None;
    h.run_steps(3);
    let on_it = page_at(&h, 0, 100.0, 210.0);
    right_click_at(&mut h, on_it);
    assert!(h.state().views[0].comments.selected.is_some(), "the comment is selected");
    assert!(h.state().views[0].image_selection.is_none(), "the picture is not");
    assert!(h.query_by_label("Properties…").is_some(), "the comment's menu opens");
}

/// In crop mode, a right-click on the next page goes to what lies there, though a clipped artwork's
/// whole frame reaches over it: crop mode ends and the page's menu opens.
#[test]
fn a_right_click_on_the_next_page_leaves_crop_mode() {
    let mut h = picture_pages(true);
    let on_two = page_at(&h, 1, 100.0, 40.0);
    right_click_at(&mut h, on_two);
    assert!(h.state().views[0].image_selection.is_none(), "crop mode ends");
    assert!(h.query_by_label("Marquee Zoom").is_some(), "the page's menu opens");
}

/// A crop grip takes a press as far off as its cursor shows: 9 px from the grip, outside the box.
#[test]
fn a_crop_grip_takes_a_press_within_its_reach() {
    let mut h = picture_pages(true);
    let keep = |h: &Harness<'static, PdfCraftApp>| h.state().views[0].image_selection.as_ref().and_then(|s| s.crop.as_ref()).map(|c| c.keep);
    let before = keep(&h);
    // The box kept is the picture's frame shown, x 30..160, y 170..250: its upper left corner.
    let press = page_at(&h, 0, 30.0, 250.0) - egui::vec2(9.0, 0.0);
    h.hover_at(press);
    h.run_steps(1);
    h.drag_at(press);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(press + egui::vec2(8.0 * k as f32, 8.0 * k as f32));
        h.run_steps(1);
    }
    h.drop_at(press + egui::vec2(32.0, 32.0));
    h.run_steps(2);
    assert_ne!(keep(&h), before, "the grip takes the drag");
}

/// With Add text, a click on another paragraph keeps both what was typed in the open paragraph and
/// the text being added: neither edit takes the other's place.
#[test]
fn add_text_keeps_a_changed_paragraph_and_a_draft_when_it_moves_on() {
    let mut h = picture_pages(false);
    h.state_mut().views[0].image_selection = None;
    assert!(h.state_mut().execute("edit.text"));
    h.run_steps(3);
    let id = h.state().views[0].id;
    let blocks = h.state().session.get(id).unwrap().text_blocks(0);
    assert!(blocks.len() >= 2, "two paragraphs: {blocks:?}");
    let centre = |r: [f64; 4]| ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    let (ax, ay) = centre(blocks[0].rect);
    let (bx, by) = centre(blocks[1].rect);
    let a = page_at(&h, 0, ax as f32, ay as f32);
    let b = page_at(&h, 0, bx as f32, by as f32);
    slipping_click(&mut h, a, a);
    assert!(h.state().views[0].line_editor.is_some(), "the first paragraph is open");
    h.event(egui::Event::Text("X".into()));
    h.run_steps(3);
    // A click on the page's empty part, below the picture, starts a box.
    let blank = page_at(&h, 0, 180.0, 20.0);
    slipping_click(&mut h, blank, blank);
    assert!(h.state().views[0].line_editor.is_none(), "that click applied the paragraph");
    h.event(egui::Event::Text("Added words".into()));
    h.run_steps(3);
    slipping_click(&mut h, b, b);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    let texts: Vec<String> = doc.text_blocks(0).iter().map(|b| b.text.clone()).collect();
    let added: Vec<String> = doc
        .added()
        .iter()
        .filter_map(|a| match &a.content {
            pdfcraft_engine::AddedContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert!(added.iter().any(|t| t == "Added words"), "the added text is kept: {added:?}");
    assert!(texts.iter().any(|t| t.contains('X')), "the paragraph's typing is kept: {texts:?}");
}

/// In the page grid, Delete deletes the page chosen there. The picture selected before the grid
/// opened is not on screen, and stays.
#[test]
fn delete_in_the_page_grid_deletes_the_page_and_not_the_hidden_picture() {
    let mut h = picture_pages(false);
    let id = h.state().views[0].id;
    let pictures = h.state().session.get(id).unwrap().page_images(0).len();
    let pages = h.state().session.get(id).unwrap().info.pages.len();
    assert!(h.state_mut().execute("page.organize"));
    h.run_steps(4);
    h.get_by_label("Select page 2").click();
    h.run_steps(2);
    h.key_press(egui::Key::Delete);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.info.pages.len(), pages - 1, "the page chosen in the grid is deleted");
    assert_eq!(doc.page_images(0).len(), pictures, "the picture stays");
}

/// With the picture's page not on screen, its keys do nothing: Delete removes no picture the reader
/// can't see, and Enter applies no crop.
#[test]
fn a_selected_pictures_keys_wait_for_its_page_to_be_on_screen() {
    use pdfcraft_ui_egui::canvas::PageLayout;
    for (crop, key) in [(false, egui::Key::Delete), (true, egui::Key::Enter)] {
        let mut h = picture_pages(crop);
        let id = h.state().views[0].id;
        let before = h.state().session.get(id).unwrap().page_images(0);
        if crop {
            let keep = before[0].visible_rect.unwrap();
            let half = [keep[0], keep[1], (keep[0] + keep[2]) / 2.0, keep[3]];
            h.state_mut().views[0].image_selection.as_mut().unwrap().crop.as_mut().unwrap().keep = half;
        }
        h.state_mut().views[0].layout = PageLayout::Single;
        h.state_mut().views[0].continuous = false;
        h.state_mut().views[0].go_to_page(1);
        h.run_steps(4);
        assert!(h.state().views[0].page_screen_rect(0).is_none(), "page 1 is off the screen");
        h.key_press(key);
        h.run_steps(4);
        let after = h.state().session.get(id).unwrap().page_images(0);
        assert_eq!(after.len(), before.len(), "the picture stays");
        assert_eq!(after[0].visible_rect, before[0].visible_rect, "and is not cropped");
    }
    // A page just off the screen, in a continuous view: it is still run for what lies near it.
    let mut h = picture_pages(false);
    let id = h.state().views[0].id;
    let pictures = h.state().session.get(id).unwrap().page_images(0).len();
    h.set_size(egui::vec2(1400.0, 400.0));
    h.state_mut().views[0].go_to_page(1);
    h.run_steps(6);
    // Going to a page leaves a margin above it: a little further, past page 1's foot.
    let centre = h.state().views[0].viewport_rect().center();
    h.hover_at(centre);
    h.run_steps(1);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -60.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
        source: egui::MouseWheelSource::Trackpad,
    });
    h.run_steps(6);
    assert!(h.state().views[0].continuous, "a continuous view");
    assert!(h.state().views[0].page_screen_rect(0).is_none(), "page 1 is off the screen");
    h.key_press(egui::Key::Delete);
    h.run_steps(4);
    assert_eq!(h.state().session.get(id).unwrap().page_images(0).len(), pictures, "the picture stays");
}

/// Delete waits for the button to come up: the press held would then name another item, whose drag or
/// click would follow.
#[test]
fn delete_waits_for_the_button_to_come_up() {
    let hold_and_delete = |h: &mut Harness<'static, PdfCraftApp>, on: egui::Pos2| {
        h.hover_at(on);
        h.run_steps(1);
        h.drag_at(on);
        h.run_steps(1);
        h.key_press(egui::Key::Delete);
        h.run_steps(2);
        h.drop_at(on);
        h.run_steps(3);
    };
    let mut h = picture_pages(false);
    let id = h.state().views[0].id;
    let pictures = h.state().session.get(id).unwrap().page_images(0).len();
    let on_picture = page_at(&h, 0, 100.0, 180.0);
    hold_and_delete(&mut h, on_picture);
    assert_eq!(h.state().session.get(id).unwrap().page_images(0).len(), pictures, "the picture stays");

    let mut h = edit_page(pdfcraft_ui_egui::QuickTool::Select);
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::AddText {
        page: 0,
        text: pdfcraft_engine::AddedText { rect: [40.0, 250.0, 140.0, 265.0], text: "Added".into(), ..Default::default() },
    }));
    assert!(h.state_mut().execute("edit.text"));
    h.run_steps(3);
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::Select;
    h.run_steps(3);
    let on_added = point_on(&h, 90.0, 258.0);
    slipping_click(&mut h, on_added, on_added);
    assert_eq!(h.state().views[0].content.selected, Some((0, 0)), "the added text is selected");
    hold_and_delete(&mut h, on_added);
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.get(id).unwrap().added().len(), 1, "the added text stays");
}

/// A drag the page it is over first sees lets the picked picture go, whichever page it began on.
#[test]
fn a_drag_first_seen_on_the_next_page_lets_the_picked_picture_go() {
    let mut h = picture_pages(false);
    h.state_mut().views[0].image_selection = None;
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::Select;
    h.run_steps(3);
    let on_picture = page_at(&h, 0, 100.0, 180.0);
    slipping_click(&mut h, on_picture, on_picture);
    assert!(h.state().views[0].picked_image.is_some(), "the picture is picked");
    let press = page_at(&h, 0, 180.0, 20.0);
    let release = page_at(&h, 1, 180.0, 280.0);
    h.hover_at(press);
    h.run_steps(1);
    h.drag_at(press);
    h.run_steps(1);
    h.hover_at(release);
    h.run_steps(1);
    h.drop_at(release);
    h.run_steps(4);
    assert!(h.state().views[0].picked_image.is_none(), "the drag lets it go");
}

/// With the pointer on the Pages panel, Delete is the panel's: a page goes, though a picture is
/// selected on the pages.
#[test]
fn delete_with_the_pointer_on_the_pages_panel_deletes_a_page() {
    let mut h = picture_pages(false);
    h.state_mut().set_option("panel", "pages").unwrap();
    h.run_steps(4);
    let id = h.state().views[0].id;
    let pages = h.state().session.get(id).unwrap().info.pages.len();
    assert!(h.state().views[0].image_selection.is_some(), "the picture is selected");
    assert!(h.state().views[0].page_screen_rect(0).is_some(), "on a page on screen");
    let viewport = h.state().views[0].viewport_rect();
    assert!(viewport.right() < 1300.0, "the panel lies right of the pages: {viewport:?}");
    h.hover_at(egui::pos2(viewport.right() + 60.0, viewport.center().y));
    h.run_steps(2);
    h.key_press(egui::Key::Delete);
    h.run_steps(4);
    assert_eq!(h.state().session.get(id).unwrap().info.pages.len(), pages - 1, "a page goes");
}

/// In crop mode, a right-click on another picture selects it for its menu, as without crop mode. It
/// went to crop mode's whole-page hit, which unselected every picture.
#[test]
fn in_crop_mode_a_right_click_on_another_picture_selects_it() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("two.pdf", None, two_image_fixture()).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    let id = h.state().views[0].id;
    let rect = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
    selection.crop = Some(pdfcraft_ui_egui::CropMode::new(rect));
    h.state_mut().views[0].image_selection = Some(selection);
    h.run_steps(2);
    let xf = h.state().views[0].page_xform(0).unwrap();
    let doc = h.state().session.get(id).unwrap();
    let other = pdfcraft_ui_egui::canvas::image_screen_rect(&doc.page_images(0)[1], &xf, &doc.info, 0);
    crop_secondary_click(&mut h, other.center());
    let selection = h.state().views[0].image_selection.clone();
    assert!(selection.as_ref().is_some_and(|s| s.index == 1 && s.crop.is_none()), "the other picture is selected: {selection:?}");
    assert!(h.query_by_label("Flip horizontal").is_some(), "its menu opens");
}

/// A drag from a hole in the selected picture (drawn through a clip with a hole in it) moves the
/// picture: its frame is its own.
#[test]
fn a_drag_from_a_hole_in_the_selected_picture_moves_it() {
    // The picture shows at x 30..160, y 170..250, around a hole at x 70..120, y 190..230.
    let mut h = picture_pages_of(artwork_drawn(b"q 30 170 130 80 re 70 190 50 40 re W* n 800 0 0 900 -300 -400 cm /Im0 Do Q"), false);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let hole = page_at(&h, 0, 95.0, 210.0);
    h.hover_at(hole);
    h.run_steps(1);
    h.drag_at(hole);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(hole + egui::vec2(5.0 * k as f32, 0.0));
        h.run_steps(1);
    }
    h.drop_at(hole + egui::vec2(20.0, 0.0));
    h.run_steps(4);
    let after = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    assert!(after[0] - before[0] > 5.0, "the picture moved right: {before:?} → {after:?}");
}

/// A right-click on a crop grip past the page's edge, where those of a picture filling its page lie
/// half, is the crop's, as on the page: crop mode stays and no menu opens (#5835).
#[test]
fn a_right_click_on_a_crop_grip_past_the_page_edge_keeps_crop_mode() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 40, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let keep = h.state().session.get(id).unwrap().page_images(0)[0].visible_rect.unwrap();
    let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
    selection.crop = Some(pdfcraft_ui_egui::CropMode::new(keep));
    h.state_mut().views[0].image_selection = Some(selection);
    h.run_steps(3);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    crop_secondary_click(&mut h, r.right_bottom() + egui::vec2(4.0, 4.0));
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_some()), "crop mode stays");
    for label in ["Flip horizontal", "Rotate view clockwise"] {
        assert!(h.query_all_by_label(label).next().is_none(), "no menu: {label}");
    }
}

/// Crop mode's grips reach past the page's edge, where those of a picture filling its page lie
/// half: a drag from just outside the page's corner trims the picture.
#[test]
fn a_crop_grip_past_the_page_edge_drags() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 40, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().page_images(0)[0].visible_rect.unwrap();
    let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
    selection.crop = Some(pdfcraft_ui_egui::CropMode::new(before));
    h.state_mut().views[0].image_selection = Some(selection);
    h.run_steps(3);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let outside = r.right_bottom() + egui::vec2(4.0, 4.0);
    let to = r.right_bottom() - egui::vec2(r.width() / 4.0, r.height() / 4.0);
    h.hover_at(outside);
    h.run_steps(1);
    h.drag_at(outside);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(outside + (to - outside) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(2);
    let keep = h.state().views[0].image_selection.as_ref().and_then(|s| s.crop.as_ref()).map(|c| c.keep);
    assert!(keep.is_some_and(|k| k[2] < before[2] - 1.0 && k[1] > before[1] + 1.0), "trimmed from the corner: {keep:?} of {before:?}");
}

/// Delete removes the selected picture wherever the pointer is, on a paragraph too: the key is the
/// app's, not the pointer's.
#[test]
fn delete_removes_the_selected_picture_with_the_pointer_on_a_paragraph() {
    let mut h = picture_pages(false);
    let caption = page_at(&h, 0, 65.0, 134.0);
    assert_eq!(cursor_over(&mut h, caption), egui::CursorIcon::Move, "on the caption's paragraph");
    h.key_press(Key::Delete);
    h.run_steps(3);
    let id = h.state().views[0].id;
    assert!(h.state().session.get(id).unwrap().page_images(0).is_empty(), "the picture is deleted");
    assert!(h.state().views[0].image_selection.is_none());
}

/// Delete on a selected added picture deletes it alone: the added text placed after it stays. The
/// key's second reader, added content's own Delete, took the text that the picture's index then
/// named.
#[test]
fn delete_on_a_selected_added_picture_deletes_it_alone() {
    let mut h = edit_page(pdfcraft_ui_egui::QuickTool::EditText);
    let mut png = Vec::new();
    image::RgbImage::from_pixel(30, 20, image::Rgb([20, 90, 200])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    h.state_mut().add_image("logo.png".into(), png);
    h.run_steps(3);
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::AddText {
        page: 0,
        text: pdfcraft_engine::AddedText { rect: [20.0, 20.0, 120.0, 35.0], text: "Caption".into(), ..Default::default() },
    }));
    h.run_steps(3);
    let id = h.state().views[0].id;
    let images = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(images, 1, "the added picture is the page's picture");
    h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection::at(0, 0));
    h.run_steps(3);
    h.key_press(Key::Delete);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    assert!(doc.page_images(0).is_empty(), "the picture is deleted");
    let texts: Vec<&str> = doc
        .added()
        .iter()
        .filter_map(|a| if let pdfcraft_engine::AddedContent::Text(t) = &a.content { Some(t.text.as_str()) } else { None })
        .collect();
    assert_eq!(texts, ["Caption"], "and the text placed after it stays");
}

/// With a dialog open, Delete and Backspace are the dialog's: the selected picture stays.
#[test]
fn a_dialog_keeps_its_delete_while_a_picture_is_selected() {
    let mut h = picture_pages(false);
    let on_picture = page_at(&h, 0, 95.0, 210.0);
    h.hover_at(on_picture);
    h.run_steps(1);
    assert!(h.state_mut().execute("app.preferences"));
    h.run_steps(3);
    for key in [Key::Delete, Key::Backspace] {
        h.key_press(key);
        h.run_steps(3);
    }
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.get(id).unwrap().page_images(0).len(), 1, "the picture stays under the dialog");
    assert!(h.state().views[0].image_selection.is_some());
    assert_eq!(h.state().dialog, Some(Dialog::Preferences));
}

/// A click on a paragraph opens it where it was pressed, though the button came up just below its
/// box: an owner acts where its press was. It opened nothing.
#[test]
fn a_click_on_a_paragraph_released_just_below_it_opens_it() {
    let mut h = edit_page(pdfcraft_ui_egui::QuickTool::EditText);
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    // The box reaches 2 px below the text: pressed 1 px inside it, released 3 px below it.
    let bottom = point_on(&h, (block.rect[0] + block.rect[2]) / 2.0, block.rect[1]) + egui::vec2(0.0, 2.0);
    slipping_click(&mut h, bottom - egui::vec2(0.0, 1.0), bottom + egui::vec2(0.0, 3.0));
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph pressed is open for typing");
}

/// With the Select tool and the Edit panel open, a click on added text selects it where it was
/// pressed, though the button came up just beside it. It left nothing selected.
#[test]
fn a_click_on_added_text_released_just_beside_it_selects_it() {
    let mut h = edit_page(pdfcraft_ui_egui::QuickTool::Select);
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::AddText {
        page: 0,
        text: pdfcraft_engine::AddedText { rect: [40.0, 250.0, 140.0, 265.0], text: "Added".into(), ..Default::default() },
    }));
    assert!(h.state_mut().execute("edit.text"));
    h.run_steps(3);
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::Select;
    h.run_steps(3);
    // The item takes presses 2 px past its box: pressed 1 px inside that, released 3 px past it.
    let edge = point_on(&h, 140.0, 258.0) + egui::vec2(2.0, 0.0);
    slipping_click(&mut h, edge - egui::vec2(1.0, 0.0), edge + egui::vec2(3.0, 0.0));
    assert_eq!(h.state().views[0].content.selected, Some((0, 0)), "the added text pressed is selected");
}

/// With Add text, a click pressed beside a paragraph and released inside its box starts a new box
/// where it was pressed. It finished the text being typed and placed nothing.
#[test]
fn an_add_text_click_pressed_beside_a_paragraph_starts_a_box_there() {
    let mut h = edit_page(pdfcraft_ui_egui::QuickTool::AddText);
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    // The box reaches 2 px past the text: pressed 2 px left of it, released 2 px inside it.
    let left = point_on(&h, block.rect[0], (block.rect[1] + block.rect[3]) / 2.0) - egui::vec2(2.0, 0.0);
    slipping_click(&mut h, left - egui::vec2(2.0, 0.0), left + egui::vec2(2.0, 0.0));
    let draft = h.state().views[0].content.draft.as_ref().map(|d| d.rect);
    let x = draft.map(|r| r[0]);
    assert!(x.is_some_and(|x| x < block.rect[0]), "a new box at the press, left of the paragraph: {x:?} of {:?}", block.rect);
    assert!(h.state().views[0].line_editor.is_none(), "the paragraph stays closed");
}

/// A paragraph drag in Edit text ends when Edit text's input is blocked (the window loses focus), as
/// a drag of the objects selected together does: nothing moves once the input comes back. It
/// waited, and moved the paragraph to wherever the pointer then was.
#[test]
fn a_paragraph_drag_ends_when_the_window_loses_focus() {
    for tool in [pdfcraft_ui_egui::QuickTool::EditText, pdfcraft_ui_egui::QuickTool::AddText] {
        let mut h = edit_page(tool);
        let id = h.state().views[0].id;
        let before = h.state().session.get(id).unwrap().text_blocks(0)[0].rect;
        let from = point_on(&h, (before[0] + before[2]) / 2.0, (before[1] + before[3]) / 2.0);
        h.hover_at(from);
        h.run_steps(1);
        h.drag_at(from);
        h.run_steps(1);
        for k in 1..=4 {
            h.hover_at(from + egui::vec2(0.0, 10.0 * k as f32));
            h.run_steps(1);
        }
        assert!(h.state().views[0].block_drag.is_some(), "the paragraph is being moved");
        h.input_mut().focused = false;
        h.run_steps(2);
        h.drop_at(from + egui::vec2(0.0, 40.0));
        h.run_steps(2);
        h.input_mut().focused = true;
        h.hover_at(from + egui::vec2(0.0, 80.0));
        h.run_steps(4);
        assert!(h.state().views[0].block_drag.is_none(), "the drag is over");
        assert_eq!(h.state().session.get(id).unwrap().text_blocks(0)[0].rect, before, "nothing moved");
        assert!(!dirty(&h));
    }
}

/// The rotate knob of a picture that fills its page lies past the page's edge, and a drag there
/// still turns the picture: the press is the page's whose knob it is.
#[test]
fn dragging_the_rotate_knob_past_the_page_edge_turns_the_picture() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 40, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection::at(0, 0));
    h.run_steps(3);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    // Above the top edge, or below the bottom one when the view has no room above.
    let knob = [egui::pos2(r.center().x, r.top() - 22.0), egui::pos2(r.center().x, r.bottom() + 22.0)]
        .into_iter()
        .find(|p| cursor_over(&mut h, *p) == egui::CursorIcon::Grab)
        .expect("the knob past the page's edge");
    assert!(!r.contains(knob));
    let to = egui::pos2(r.right() + 60.0, r.center().y);
    h.hover_at(knob);
    h.run_steps(1);
    h.drag_at(knob);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(knob + (to - knob) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(4);
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Rotate image"));
}

/// Organize pages has Acrobat's panel whenever the grid shows, however it was opened, and the
/// panel's Close ends Organize: the grid closes and All tools comes back.
#[test]
fn organize_panel_comes_with_the_grid_and_closes_it() {
    let mut h = organize(3);
    h.get_by_label("Insert");
    h.get_by_label("Bates numbering");
    h.get_by_label("Page properties");
    h.get_by_label("Close panel").click();
    h.run_steps(3);
    assert!(!h.state().views[0].organize, "Close ends Organize");
    assert_eq!(h.state().left, pdfcraft_ui_egui::LeftPanel::AllTools);
    assert!(h.query_by_label("Page properties").is_none(), "the panel is gone");
}

/// Organize pages ▸ Page properties ▸ Tab Order sets the tab order of the selected pages only,
/// as one undo step, and opens on the order the pages have.
#[test]
fn page_properties_sets_the_tab_order_of_the_selected_pages() {
    use pdfcraft_engine::TabOrder::{Annotations, Column, Row};
    let tabs = |h: &Harness<'static, PdfCraftApp>| h.state().session.get(h.state().views[0].id).unwrap().tab_orders();
    // The undo and redo labels: a new step replaces the redo step an undo left.
    let undo = |h: &Harness<'static, PdfCraftApp>| {
        let doc = h.state().session.get(h.state().views[0].id).unwrap();
        (doc.can_undo().map(str::to_owned), doc.can_redo().map(str::to_owned))
    };
    let mut h = organize(3);
    h.get_by_label("Select page 1").click();
    h.run_steps(2);
    h.get_by_label("Select page 3").click();
    h.run_steps(2);
    h.get_by_label("Page properties").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::PageProps));
    assert_eq!(h.state().page_props_draft.as_ref().unwrap().tab_order, Some(Annotations), "opens on Unspecified, as the pages have it");
    h.get_by_label("Use Column Order").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    assert_eq!(tabs(&h), [Column, Annotations, Column]);
    assert_eq!(undo(&h).0.as_deref(), Some("Set tab order"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(tabs(&h), [Annotations; 3], "one undo step");

    // Pages with different orders open with no choice made, and OK is off until one is.
    h.state_mut().apply_edit(pdfcraft_engine::Edit::SetTabOrder { pages: vec![0], order: Row });
    h.run_steps(2);
    h.state_mut().views[0].select_pages(&[0, 1]);
    assert!(h.state_mut().execute("page.properties"));
    h.run_steps(3);
    assert_eq!(h.state().page_props_draft.as_ref().unwrap().tab_order, None);
    assert!(h.get_by_label("OK").accesskit_node().is_disabled());
    h.get_by_label("Use Row Order").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(tabs(&h), [Row, Row, Annotations]);

    // OK on the order the pages already have adds no undo step, and Cancel changes nothing.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    let before = (tabs(&h), undo(&h));
    assert_eq!(before.1.1.as_deref(), Some("Set tab order"), "the undone step can be redone");
    h.state_mut().views[0].select_pages(&[2]);
    assert!(h.state_mut().execute("page.properties"));
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!((tabs(&h), undo(&h)), before);
    assert!(h.state_mut().execute("page.properties"));
    h.run_steps(3);
    h.get_by_label("Use Document Structure").click();
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    assert_eq!((tabs(&h), undo(&h)), before);
}

/// Page properties sets the tab order of the document it was opened on: when another document
/// is active by OK, neither document changes.
#[test]
fn page_properties_does_not_apply_to_another_active_document() {
    use pdfcraft_engine::TabOrder::Annotations;
    let mut h = organize(3);
    let first = h.state().views[0].id;
    h.state_mut().views[0].select_pages(&[0]);
    assert!(h.state_mut().execute("page.properties"));
    h.run_steps(3);
    h.get_by_label("Use Column Order").click();
    h.run_steps(2);
    h.state_mut().open_bytes("other.pdf", None, fixture(3)).unwrap();
    h.run_steps(4);
    let other = h.state().views[1].id;
    assert_eq!(h.state().active_ids().map(|(_, id)| id), Some(other));
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    for id in [first, other] {
        let doc = h.state().session.get(id).unwrap();
        assert_eq!(doc.tab_orders(), [Annotations; 3]);
        assert!(doc.can_undo().is_none());
    }
}

/// Page properties sets the tab order of the pages it was opened on: when an edit changes the
/// document while the dialog is open, OK closes it without changing a page.
#[test]
fn page_properties_does_not_apply_after_the_document_changes() {
    use pdfcraft_engine::TabOrder::Annotations;
    let mut h = organize(3);
    let id = h.state().views[0].id;
    h.state_mut().views[0].select_pages(&[0]);
    assert!(h.state_mut().execute("page.properties"));
    h.run_steps(3);
    h.get_by_label("Use Column Order").click();
    h.run_steps(2);
    // Page 1 becomes page 3, so index 0 is another page now.
    assert!(h.state_mut().execute("page.reverse"));
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.tab_orders(), [Annotations; 3]);
    assert_ne!(doc.can_undo(), Some("Set tab order"));
    assert_eq!(
        h.state().toast.as_ref().map(|t| t.0.as_str()),
        Some("The document changed while the dialog was open, so nothing was changed. Open Page properties again to set the tab order.")
    );
}

/// Leaving Organize (a page opened from the grid, or the grid's Close) goes back to the
/// workspace's panel: under the Edit tab, the Edit panel, not All tools.
#[test]
fn leaving_organize_goes_back_to_the_workspace_panel() {
    use pdfcraft_ui_egui::{LeftPanel, Mode};
    let mut h = harness(3, |_| {});
    h.state_mut().mode = Mode::Edit;
    h.state_mut().left = LeftPanel::Tool("edit");
    h.run_steps(2);
    h.state_mut().left = LeftPanel::Tool("organize");
    h.state_mut().views[0].organize = true;
    h.run_steps(3);
    h.get_by_label("Pages selected".to_uppercase().as_str());
    // What opening a page from the grid does.
    h.state_mut().views[0].organize = false;
    h.run_steps(3);
    assert_eq!(h.state().left, LeftPanel::Tool("edit"), "back to the Edit panel");
    // The same from All tools goes back to All tools.
    h.state_mut().mode = Mode::AllTools;
    h.state_mut().left = LeftPanel::Tool("organize");
    h.state_mut().views[0].organize = true;
    h.run_steps(3);
    h.state_mut().views[0].organize = false;
    h.run_steps(3);
    assert_eq!(h.state().left, LeftPanel::AllTools);
}
