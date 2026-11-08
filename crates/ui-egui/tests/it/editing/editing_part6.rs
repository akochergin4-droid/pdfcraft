//! Editing tests through the app UI (part 6 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn image_copy_cut_paste_keyboard_and_context_menu() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(60, 40, image::Rgb([180, 50, 50])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let id = h.state().views[0].id;
    let initial_count = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(initial_count, 1, "starts with 1 image");

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // 1. Select the image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "image selected");

    // 2. Copy via keyboard shortcut (COMMAND+C / Event::Copy)
    h.event(egui::Event::Copy);
    h.run_steps(2);
    assert!(h.state().last_copied_image.is_some(), "image copied to clipboard buffer");

    // 3. The integration delivers the copied pixels for the keyboard paste.
    let (width, height, rgba) = h.state().last_copied_image.clone().unwrap();
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
    h.event(egui::Event::PasteImage(std::sync::Arc::new(image)));
    h.run_steps(3);
    let after_paste = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(after_paste, 2, "now has 2 images");
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(1), "new image is selected");

    // 4. Cut via keyboard shortcut (COMMAND+X / Event::Cut)
    h.event(egui::Event::Cut);
    h.run_steps(3);
    let after_cut = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(after_cut, 1, "cut deleted the pasted image");
    assert!(h.state().views[0].image_selection.is_none(), "selection cleared after cut");

    // 5. Paste again via context menu on the page
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    h.get_by_label_contains("Paste").click();
    h.run_steps(3);
    let after_paste2 = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(after_paste2, 2, "pasted via context menu");
}

#[test]
fn added_and_pasted_image_uses_refactored_image_edits() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 80, image::Rgb([120, 180, 240])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();

    let mut h = harness(1, |_| {});
    h.run_steps(4);

    // 1. Add image via add_image
    h.state_mut().add_image("test_add.png".into(), png);
    h.run_steps(3);

    // Verify refactored mode is active
    assert_eq!(h.state().quick_tool, pdfcraft_ui_egui::QuickTool::EditText);
    let id = h.state().views[0].id;
    let images = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images.len(), 1, "image is added");
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| (s.page, s.index)), Some((0, 0)), "added image has refactored image_selection");

    // 2. Perform refactored crop on the added image
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    h.get_by_label("Crop").click();
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_some()), "crop mode entered");

    let smaller = egui::Rect::from_center_size(c, egui::vec2(40.0, 40.0));
    set_crop_box(&mut h, smaller);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert!(!h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_some()), "crop mode exited");

    let images_after_crop = h.state().session.get(id).unwrap().page_images(0);
    assert!(images_after_crop[0].visible_rect.is_some());
    assert_ne!(images_after_crop[0].visible_rect, Some(images_after_crop[0].rect), "crop applied to added image");

    // 3. Move the cropped added image
    let orig_rect = images_after_crop[0].rect;
    let orig_vis = images_after_crop[0].visible_rect.unwrap();
    let dx = 20.0;
    let dy = 20.0;
    let new_rect = [orig_rect[0] + dx, orig_rect[1] + dy, orig_rect[2] + dx, orig_rect[3] + dy];
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 0, change: pdfcraft_engine::ImageEdit::Move(new_rect) });
    h.run_steps(3);

    let images_after_move = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images_after_move[0].rect, new_rect, "moved rect matches");
    assert_eq!(
        images_after_move[0].visible_rect,
        Some([orig_vis[0] + dx, orig_vis[1] + dy, orig_vis[2] + dx, orig_vis[3] + dy]),
        "crop preserved after move"
    );
}

#[test]
fn stale_clipboard_plain_text_does_not_paste_image() {
    let mut h = harness(1, |_| {});
    h.run_steps(4);
    // Put an image into internal cache
    h.state_mut().last_copied_image = Some((10, 10, vec![0; 400]));
    h.state_mut().last_copied_file = Some(("cached.png".to_string(), vec![1, 2, 3]));
    // The text arrives in the paste event, as it does from the OS. The real system clipboard is
    // shared by every test thread, so this test doesn't write it.
    h.state_mut().system_clipboard = false;

    // A paste event containing plain text clears internal image cache and does not paste image
    h.event(egui::Event::Paste("just plain text".into()));
    h.run_steps(2);

    // can_paste_image should now be false because the clipboard has text, not an image!
    assert!(!h.state().can_paste_image(), "cannot paste image when the clipboard has text");

    // paste_image should fail and return false without pasting
    assert!(!h.state_mut().paste_image(0, None), "must not paste stale cached image");

    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert_eq!(doc.page_images(0).len(), 0, "no image pasted on the page");
}

/// Delivered image pixels paste once; accompanying key releases add no second image.
#[test]
fn delivered_image_with_key_releases_pastes_once() {
    let pdf = include_bytes!("../../data/form.pdf").to_vec();
    let mut h = Harness::builder().with_step_dt(1.0 / 60.0).with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("form.pdf", None, pdf.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let id = h.state().views[0].id;
    let initial_count = h.state().session.get(id).unwrap().page_images(0).len();

    h.state_mut().clipboard_override = Some(pdfcraft_ui_egui::Clip::Image { width: 40, height: 20, rgba: [100u8, 200, 100, 255].repeat(40 * 20) });

    // The integration supplies the pixels and consumes native shortcut keys. Extra releases carry no paste authority.
    h.event(egui::Event::PasteImage(std::sync::Arc::new(egui::ColorImage::filled([40, 20], egui::Color32::GREEN))));
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(3);

    let count = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(count, initial_count + 1, "exactly one image must be pasted for delivered pixels, got {count}");
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().session.get(id).unwrap().page_images(0).len(), initial_count + 1);
}

/// Win+V release without COMMAND must NOT paste an image.
#[test]
fn win_v_does_not_paste_on_open() {
    let pdf = include_bytes!("../../data/form.pdf").to_vec();
    let mut h = Harness::builder().with_step_dt(1.0 / 60.0).with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("form.pdf", None, pdf.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let id = h.state().views[0].id;
    let initial_count = h.state().session.get(id).unwrap().page_images(0).len();

    h.state_mut().clipboard_override = Some(pdfcraft_ui_egui::Clip::Image { width: 40, height: 20, rgba: [100u8, 200, 100, 255].repeat(40 * 20) });

    // Complete an intentional paste immediately before opening clipboard history.
    h.event(egui::Event::PasteImage(std::sync::Arc::new(egui::ColorImage::filled([40, 20], egui::Color32::GREEN))));
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(1);
    assert_eq!(h.state().session.get(id).unwrap().page_images(0).len(), initial_count + 1);

    // Simulate pressing Win+V (opening clipboard history flyout):
    // The OS intercepts V press, so PdfCraft only sees V release without COMMAND (Modifiers::NONE).
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);

    let count = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(count, initial_count + 1, "opening Win+V after a completed paste must not paste again");

    // Choosing an item delivers another image, possibly alongside synthetic shortcut events.
    h.event(egui::Event::PasteImage(std::sync::Arc::new(egui::ColorImage::filled([40, 20], egui::Color32::GREEN))));
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(3);

    let final_count = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(final_count, initial_count + 2, "the two intentional deliveries must paste exactly two images");
}

/// Normal Ctrl+V press followed by release delivers exactly one image, not two.
#[test]
fn paste_press_and_release_delivers_single_image() {
    let pdf = include_bytes!("../../data/form.pdf").to_vec();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("form.pdf", None, pdf.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let id = h.state().views[0].id;
    let initial_count = h.state().session.get(id).unwrap().page_images(0).len();

    h.state_mut().clipboard_override = Some(pdfcraft_ui_egui::Clip::Image { width: 40, height: 20, rgba: [100u8, 200, 100, 255].repeat(40 * 20) });

    // Press V with COMMAND
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(1);

    // Release V with COMMAND
    h.event(egui::Event::Key { key: egui::Key::V, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::COMMAND });
    h.run_steps(2);

    let count = h.state().session.get(id).unwrap().page_images(0).len();
    assert_eq!(count, initial_count + 1, "Ctrl+V press and release must deliver exactly one image, got {count}");
}

/// Issue P1: Paste via context menu at a point away from the centre centers the image at that point.
#[test]
fn paste_at_context_menu_point_centers_on_cursor() {
    let pdf = include_bytes!("../../data/form.pdf").to_vec();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("form.pdf", None, pdf.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let id = h.state().views[0].id;
    let initial_count = h.state().session.get(id).unwrap().page_images(0).len();

    h.state_mut().clipboard_override = Some(pdfcraft_ui_egui::Clip::Image { width: 60, height: 40, rgba: [50u8, 150, 250, 255].repeat(60 * 40) });

    let target_at = [120.0, 180.0];
    assert!(h.state_mut().paste_image_at(0, target_at));

    let images = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images.len(), initial_count + 1);
    let r = images[initial_count].rect;
    let center = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    assert!((center.0 - target_at[0]).abs() < 1.0 && (center.1 - target_at[1]).abs() < 1.0, "expected center {target_at:?}, got {center:?}");
}

/// Issue P2: A newer clipboard override image beats stale in-app copy.
#[test]
fn newer_clipboard_override_beats_stale_in_app_copy() {
    let pdf = include_bytes!("../../data/form.pdf").to_vec();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.system_clipboard = false;
        app.open_bytes("form.pdf", None, pdf.clone()).expect("opens");
        app
    });
    h.run_steps(4);

    let id = h.state().views[0].id;
    let initial_count = h.state().session.get(id).unwrap().page_images(0).len();

    // Stale in-app copy: 10x10 red image
    h.state_mut().last_copied_image = Some((10, 10, [255u8, 0, 0, 255].repeat(100)));

    // Newer clipboard override: 50x50 green image
    h.state_mut().clipboard_override = Some(pdfcraft_ui_egui::Clip::Image { width: 50, height: 50, rgba: [0u8, 255, 0, 255].repeat(2500) });

    assert!(h.state_mut().paste_image(0, None));
    let images = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images.len(), initial_count + 1);
    let r = images[initial_count].rect;
    let (w, h_dim) = (r[2] - r[0], r[3] - r[1]);
    assert!((w - 50.0).abs() < 1.0 && (h_dim - 50.0).abs() < 1.0, "expected 50x50 override image, got {w}x{h_dim}");
}

#[test]
fn rotated_image_selection_and_crop() {
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
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // Select the image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    // Right-click and rotate clockwise
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    h.get_by_label("Rotate Clockwise").click();
    h.run_steps(3);

    let doc = h.state().session.get(id).unwrap();
    let im_rotated = &doc.page_images(0)[0];
    assert!(im_rotated.matrix[1].abs() > 1.0 || im_rotated.matrix[2].abs() > 1.0, "matrix is rotated: {:?}", im_rotated.matrix);

    // Enter crop mode with user-space bounds [0.2..0.8] of rotated image rect
    let full = im_rotated.rect;
    let crop_keep = [
        full[0] + (full[2] - full[0]) * 0.2,
        full[1] + (full[3] - full[1]) * 0.2,
        full[0] + (full[2] - full[0]) * 0.8,
        full[1] + (full[3] - full[1]) * 0.8,
    ];
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new(crop_keep));
    h.run_steps(2);

    // Apply crop via Enter key
    h.key_press(egui::Key::Enter);
    h.run_steps(4);

    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Crop image"));
    let im_cropped = &doc.page_images(0)[0];
    assert!(im_cropped.matrix[1].abs() > 1.0 || im_cropped.matrix[2].abs() > 1.0, "matrix remains rotated: {:?}", im_cropped.matrix);
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_none()), "crop mode ended");
}

#[test]
fn test_image_8_handles_render_and_drag() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(100, 100, image::Rgb([100, 150, 200]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("square.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // Select the image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    let view = &h.state().views[0];
    let sel = view.image_selection.as_ref().unwrap();
    let doc = h.state().session.get(view.id).unwrap();
    let img = &doc.page_images(sel.page)[sel.index];
    let xf =
        pdfcraft_ui_egui::canvas::PageXform { rect: r, rot: view.rotation, pw: doc.info.pages[sel.page].width, ph: doc.info.pages[sel.page].height };
    let obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(img, &xf, &doc.info, sel.page);

    // Assert that exactly 8 unique resize handles (4 corners, 4 midpoints) are generated
    let handles = obb.handles();
    assert_eq!(handles.len(), 8, "exactly 8 resize handles generated");

    let corners = [
        pdfcraft_ui_egui::ImageHandle::TopLeft,
        pdfcraft_ui_egui::ImageHandle::TopRight,
        pdfcraft_ui_egui::ImageHandle::BottomRight,
        pdfcraft_ui_egui::ImageHandle::BottomLeft,
    ];
    let midpoints = [
        pdfcraft_ui_egui::ImageHandle::TopCenter,
        pdfcraft_ui_egui::ImageHandle::RightCenter,
        pdfcraft_ui_egui::ImageHandle::BottomCenter,
        pdfcraft_ui_egui::ImageHandle::LeftCenter,
    ];
    for corner in &corners {
        assert!(handles.iter().any(|(h, _)| h == corner), "missing corner handle: {:?}", corner);
    }
    for midpoint in &midpoints {
        assert!(handles.iter().any(|(h, _)| h == midpoint), "missing midpoint handle: {:?}", midpoint);
    }

    for i in 0..8 {
        for j in (i + 1)..8 {
            assert!((handles[i].1 - handles[j].1).length() > 1.0, "handles {} and {} must be unique positions", i, j);
        }
    }

    // Simulate a pointer drag on a corner that shrinks the image to 5x5 pixels and assert it does not panic
    let tr_pos = obb.handle_pos(pdfcraft_ui_egui::ImageHandle::TopRight);
    let anchor = obb.s00;
    let shrink_target = anchor + egui::vec2(5.0, 5.0);
    h.hover_at(tr_pos);
    h.run_steps(1);
    h.drag_at(tr_pos);
    h.run_steps(1);
    h.hover_at(shrink_target);
    h.run_steps(2);
    h.drop_at(shrink_target);
    h.run_steps(4);
}

#[test]
fn test_image_context_menu_precedence() {
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
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // Select the image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    // Simulate a secondary click strictly inside an image's bounds
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    // Assert that the image widget consumes the click and the canvas background menu does not trigger
    h.get_by_label("Rotate Clockwise");
    h.get_by_label("Flip horizontal");
    assert!(h.query_all_by_label("Rotate view clockwise").next().is_none(), "canvas background menu must not trigger");
    assert!(h.query_all_by_label("Add bookmark").next().is_none(), "canvas background menu must not trigger");
    assert!(h.query_all_by_label("Take a snapshot").next().is_none(), "canvas background menu must not trigger");
}

#[test]
fn test_image_context_menu_in_crop_mode_and_when_moved() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(100, 100, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    // Shrink and center the picture on the page so page margins exist around it
    let full = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let (qw, qh) = ((full[2] - full[0]) / 4.0, (full[3] - full[1]) / 4.0);
    let (mx, my) = ((full[0] + full[2]) / 2.0, (full[1] + full[3]) / 2.0);
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Move([mx - qw, my - qh, mx + qw, my + qh]),
    });
    h.run_steps(4);

    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // Select the image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    // Enter crop mode:
    let doc_id = h.state().views[0].id;
    let original_rect = h.state().session.get(doc_id).unwrap().page_images(0)[0].rect;
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new(original_rect));
    h.run_steps(2);

    // Set a smaller crop box that is moved towards top-left
    let moved_crop = egui::Rect::from_center_size(c - egui::vec2(5.0, 5.0), egui::vec2(20.0, 20.0));
    set_crop_box(&mut h, moved_crop);
    h.run_steps(2);

    // Simulate secondary click on the moved crop box
    let click_pos = moved_crop.center();
    h.hover_at(click_pos);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: click_pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: click_pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    // In crop mode and when moved, do not open right click window when clicked on the image (#5835)
    assert!(h.query_all_by_label("Rotate Clockwise").next().is_none(), "image context menu must not trigger in crop mode");
    assert!(h.query_all_by_label("Flip horizontal").next().is_none(), "image context menu must not trigger in crop mode");
    assert!(h.query_all_by_label("Rotate view clockwise").next().is_none(), "canvas background menu must not trigger in crop mode");
    assert!(h.query_all_by_label("Add bookmark").next().is_none(), "canvas background menu must not trigger in crop mode");
    assert!(h.query_all_by_label("Take a snapshot").next().is_none(), "canvas background menu must not trigger in crop mode");
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_some()), "crop mode must remain active after right click on image");
    // Action pills remain visible and interactive
    assert!(h.query_by_label("Done").is_some());
    assert!(h.query_by_label("Cancel").is_some());

    // Right clicking outside the image on the page exits crop mode and opens the canvas background menu
    let outside_pos = r.left_top() + egui::vec2(10.0, 10.0);
    h.hover_at(outside_pos);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: outside_pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: outside_pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    assert!(h.state().views[0].image_selection.is_none(), "clicking outside clears image selection");
    assert!(h.query_all_by_label("Rotate view clockwise").next().is_some(), "canvas background menu triggers outside image");
}

#[test]
fn test_image_context_menu_crop_overlapping_image_and_handle() {
    for rotation in [0, 90, 180, 270] {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("overlap.pdf", None, two_image_fixture()).unwrap();
            app.views[0].rotation = rotation;
            app
        });
        h.run_steps(4);
        let id = h.state().views[0].id;
        let rect = h.state().session.get(id).unwrap().page_images(0)[0].rect;
        h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 1, change: pdfcraft_engine::ImageEdit::Move(rect) });
        let images = h.state().session.get(id).unwrap().page_images(0);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].rect, images[1].rect, "fixture images must overlap");
        assert!(h.state_mut().execute("edit.edit_text"));
        h.run_steps(4);
        let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
        selection.crop = Some(pdfcraft_ui_egui::CropMode::new(rect));
        h.state_mut().views[0].image_selection = Some(selection);
        h.run_steps(2);
        let xf = h.state().views[0].page_xform(0).unwrap();
        let doc = h.state().session.get(id).unwrap();
        let image_rect = pdfcraft_ui_egui::canvas::image_screen_rect(&doc.page_images(0)[0], &xf, &doc.info, 0);
        crop_secondary_click(&mut h, image_rect.center());
        assert_crop_context_preserved(&h);
        // This point is outside both images, but inside the left handle's grip.
        crop_secondary_click(&mut h, image_rect.left_center() - egui::vec2(5.0, 0.0));
        assert_crop_context_preserved(&h);
        crop_secondary_click(&mut h, image_rect.left_center() - egui::vec2(20.0, 0.0));
        assert!(h.state().views[0].image_selection.is_none());
        assert!(h.query_by_label("Rotate view clockwise").is_some());
    }
}

/// A right-click on the part of a picture its crop cuts away is the crop's, as one on the box kept:
/// crop mode stays and no menu opens (#5835).
#[test]
fn a_right_click_on_the_part_a_crop_cuts_away_keeps_crop_mode() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("cut.pdf", None, two_image_fixture()).unwrap();
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    let rect = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    // Keep the picture's left half.
    let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
    selection.crop = Some(pdfcraft_ui_egui::CropMode::new([rect[0], rect[1], (rect[0] + rect[2]) / 2.0, rect[3]]));
    h.state_mut().views[0].image_selection = Some(selection);
    h.run_steps(2);
    let xf = h.state().views[0].page_xform(0).unwrap();
    let doc = h.state().session.get(id).unwrap();
    let b = pdfcraft_ui_egui::canvas::image_screen_rect(&doc.page_images(0)[0], &xf, &doc.info, 0);
    assert!(b.width() > 60.0, "the picture is wide enough for a point clear of the grips: {b:?}");
    crop_secondary_click(&mut h, b.lerp_inside(egui::vec2(0.8, 0.5)));
    assert_crop_context_preserved(&h);
}

#[test]
fn test_image_context_menu_crop_page_edge_pills() {
    for rotation in [0, 90, 180, 270] {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("edge.pdf", None, two_image_fixture()).unwrap();
            app.views[0].rotation = rotation;
            app.views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
            app
        });
        h.run_steps(4);
        // A small image at a page corner exercises horizontal clamping and above placement.
        let rect = [1.0, 1.0, 21.0, 21.0];
        h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 0, change: pdfcraft_engine::ImageEdit::Move(rect) });
        h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
            page: 0,
            index: 1,
            change: pdfcraft_engine::ImageEdit::Move([100.0, 150.0, 120.0, 170.0]),
        });
        assert!(h.state_mut().execute("edit.edit_text"));
        h.run_steps(4);
        let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
        selection.crop = Some(pdfcraft_ui_egui::CropMode::new([2.0, 2.0, 20.0, 20.0]));
        h.state_mut().views[0].image_selection = Some(selection);
        h.run_steps(2);
        for label in ["Done", "Cancel", "Remove Crop"] {
            let p = h.get_by_label(label).rect().center();
            crop_secondary_click(&mut h, p);
            assert_crop_context_preserved(&h);
        }
        // Beyond egui's 5-point interaction radius, empty space beside the rightmost pill is canvas.
        let beside = h.get_by_label("Done").rect().right_center() + egui::vec2(8.0, 0.0);
        let view = &h.state().views[0];
        let doc = h.state().session.get(view.id).unwrap();
        let xf = view.page_xform(0).unwrap();
        assert!(
            doc.page_images(0).iter().all(|img| !pdfcraft_ui_egui::canvas::image_screen_rect(img, &xf, &doc.info, 0).contains(beside)),
            "background test point must miss both images"
        );
        crop_secondary_click(&mut h, beside);
        assert!(
            h.state().views[0].image_selection.is_none(),
            "empty pill-adjacent space, rotation={rotation}, point={beside:?}, viewport={:?}, selection={:?}",
            h.state().views[0].viewport_rect(),
            h.state().views[0].image_selection
        );
        assert!(h.query_by_label("Rotate view clockwise").is_some());
    }
}

#[test]
fn test_rotated_image_crop_secondary_click_in_outer_bounding_box_clears_selection() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 80, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    let full = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let (qw, qh) = ((full[2] - full[0]) / 4.0, (full[3] - full[1]) / 4.0);
    let (mx, my) = ((full[0] + full[2]) / 2.0, (full[1] + full[3]) / 2.0);
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Move([mx - qw, my - qh, mx + qw, my + qh]),
    });
    h.run_steps(4);
    // Rotate the image by 45°
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 0, change: pdfcraft_engine::ImageEdit::RotateBy(45.0) });
    h.run_steps(4);

    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // Select the image and put it into crop mode
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    let doc_id = h.state().views[0].id;
    let img = &h.state().session.get(doc_id).unwrap().page_images(0)[0];
    let (cx, cy) = ((full[0] + full[2]) / 2.0, (full[1] + full[3]) / 2.0);
    let shrunk_crop = [cx - 5.0, cy - 5.0, cx + 5.0, cy + 5.0];
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new(shrunk_crop));
    h.run_steps(2);

    let xf = h.state().views[0].page_xform(0).unwrap();
    let doc_info = &h.state().session.get(doc_id).unwrap().info;
    let b = pdfcraft_ui_egui::canvas::image_screen_rect(img, &xf, doc_info, 0);

    // The top-left corner of the axis-aligned bounding box `b` is outside the 45°-rotated diamond quad
    let corner = b.left_top() + egui::vec2(2.0, 2.0);
    assert!(b.contains(corner), "corner point is inside screen bounding box");

    // Right-clicking at the corner must clear the crop selection and show the canvas context menu (#5900 review)
    crop_secondary_click(&mut h, corner);
    assert!(h.state().views[0].image_selection.is_none(), "clicking in outer bounding box of rotated image clears crop selection");
    assert!(h.query_by_label("Rotate view clockwise").is_some());
}

#[test]
fn crop_action_pills_interactive_and_apply_cancel_reset() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(100, 100, image::Rgb([100, 150, 200]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("square.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();
    // Select image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some());

    // Enter crop mode
    let doc_id = h.state().views[0].id;
    let original_rect = h.state().session.get(doc_id).unwrap().page_images(0)[0].rect;
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new(original_rect));
    h.run_steps(2);

    // Set smaller crop rect (50x50 in center)
    let smaller = egui::Rect::from_center_size(c, egui::vec2(50.0, 50.0));
    set_crop_box(&mut h, smaller);
    h.run_steps(2);

    let pill_y = smaller.max.y + 17.0;
    let done_pos = egui::pos2(smaller.max.x - 20.0, pill_y);
    h.hover_at(done_pos);
    h.run_steps(1);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand, "Done pill has PointingHand cursor");

    let cancel_pos = egui::pos2(smaller.max.x - 85.0, pill_y);
    h.hover_at(cancel_pos);
    h.run_steps(1);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand, "Cancel pill has PointingHand cursor");

    // Click Cancel pill: exits crop mode without applying changes
    click_at(&mut h, cancel_pos);
    h.run_steps(3);
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_none()), "Cancel exits crop mode");
    assert_eq!(h.state().session.get(doc_id).unwrap().page_images(0)[0].visible_rect, Some(original_rect), "image is uncropped");

    // Re-enter crop mode and set crop rect
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop = Some(pdfcraft_ui_egui::CropMode::new(original_rect));
    set_crop_box(&mut h, smaller);
    h.run_steps(2);

    // Click "Remove Crop" pill
    let reset_pos = egui::pos2(smaller.max.x - 170.0, pill_y);
    h.hover_at(reset_pos);
    h.run_steps(1);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand, "Remove Crop pill has PointingHand cursor");
    click_at(&mut h, reset_pos);
    h.run_steps(2);
    let current_keep = h.state().views[0].image_selection.as_ref().unwrap().crop.as_ref().unwrap().keep;
    assert_eq!(current_keep, original_rect, "Remove Crop restored crop to full bounds");

    // Re-set smaller crop and click "Done" pill to apply
    set_crop_box(&mut h, smaller);
    h.run_steps(2);
    let done_pos_after = egui::pos2(smaller.max.x - 20.0, pill_y);
    click_at(&mut h, done_pos_after);
    h.run_steps(4);

    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_none()), "Done exits crop mode");
    let after_images = h.state().session.get(doc_id).unwrap().page_images(0);
    assert_ne!(after_images[0].visible_rect, Some(original_rect), "crop was applied");
}

#[test]
fn test_cropped_image_selection_and_bounding_box() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(100, 100, image::Rgb([100, 150, 200]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("square.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let doc_id = h.state().views[0].id;
    let full_rect = h.state().session.get(doc_id).unwrap().page_images(0)[0].rect;

    // Apply crop to the center quarter: [0.25, 0.25, 0.75, 0.75]
    let crop_keep = [
        full_rect[0] + (full_rect[2] - full_rect[0]) * 0.25,
        full_rect[1] + (full_rect[3] - full_rect[1]) * 0.25,
        full_rect[0] + (full_rect[2] - full_rect[0]) * 0.75,
        full_rect[1] + (full_rect[3] - full_rect[1]) * 0.75,
    ];
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 0, change: pdfcraft_engine::ImageEdit::Crop(crop_keep) });
    h.run_steps(4);

    let img = &h.state().session.get(doc_id).unwrap().page_images(0)[0];
    assert_eq!(img.rect, full_rect, "underlying CTM rect is non-destructively preserved");
    let vis = img.visible_rect.expect("has visible_rect");
    assert!((vis[0] - crop_keep[0]).abs() < 1e-3);
    assert!((vis[1] - crop_keep[1]).abs() < 1e-3);
    assert!((vis[2] - crop_keep[2]).abs() < 1e-3);
    assert!((vis[3] - crop_keep[3]).abs() < 1e-3);

    // Verify selectable bounds helper gives the cropped intersection
    assert_eq!(pdfcraft_ui_egui::canvas::image_selectable_bounds(img), vis);

    // Select tool (mouse tool)
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::Select;
    h.run_steps(2);

    let page_rect = h.state().views[0].page_screen_rect(0).expect("on screen");
    // Center point is inside the cropped area:
    let center = page_rect.center();
    click_at(&mut h, center);
    h.run_steps(2);

    let picked = h.state().views[0].picked_image.expect("image is picked when clicking inside crop");
    assert_eq!(picked.page, 0);
    assert_eq!(picked.index, 0);
    // The selection bounding box wraps the visible cropped area, NOT the full CTM rect!
    assert_eq!(picked.rect, vis, "picked rect wraps the cropped bounds");

    // Deselect
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().views[0].picked_image.is_none());

    // Corner point (e.g. top-left corner of the page) is outside the crop box but inside full CTM rect
    let corner = page_rect.min + egui::vec2(10.0, 10.0);
    click_at(&mut h, corner);
    h.run_steps(2);
    assert!(h.state().views[0].picked_image.is_none(), "clicking cropped-away region must not hit-test or select the image");
}

/// The Read mode command (not the mode bar) applies a paragraph still open in the editor, as
/// putting the Edit text tool down does; left open, it would stay drawn in Read mode with no tool.
#[test]
fn read_mode_shortcut_applies_the_open_paragraph() {
    use pdfcraft_ui_egui::{Mode, QuickTool};
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    // The line "Page 1" sits at (20, 150) on a 200 × 300 page, 24 pt.
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
    assert!(h.state().views[0].line_editor.is_some(), "the line opens for editing");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Chapter One".into();
    h.run_steps(1);
    h.state_mut().run_command("view.read_mode");
    h.run_steps(3);
    let s = h.state();
    assert_eq!(s.mode, Mode::Read);
    assert_eq!(s.quick_tool, QuickTool::Select);
    assert!(s.views[0].line_editor.is_none(), "no editor stays open in Read mode");
    assert_eq!(s.session.get(s.views[0].id).unwrap().text_lines(0)[0].text, "Chapter One", "the edit is applied");
}

#[test]
fn arrange_duplicate_image_clears_selection_when_ambiguous() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_incremental};
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 200, 300].map(Object::Int).to_vec()));
    let artwork = doc.add(Object::Stream(Stream::flate(form, b"0.2 0.5 0.9 rg 20 20 160 260 re f")));
    // Paint two occurrences of the same figure at the exact same position
    let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), b"q 1 0 0 1 0 0 cm /Figure Do Q q 1 0 0 1 0 0 cm /Figure Do Q")));
    doc.update_dict(page.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Ref(contents));
        let mut xo = Dict::new();
        xo.set(b"Figure".to_vec(), Object::Ref(artwork));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        d.set(b"Resources".to_vec(), Object::Dict(resources));
    })
    .unwrap();
    let bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("dup.pdf", None, bytes.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let doc_id = h.state().views[0].id;
    let images = h.state().session.get(doc_id).unwrap().page_images(0);
    assert_eq!(images.len(), 2, "two duplicate image occurrences exist");
    assert_eq!(images[0].rect, images[1].rect);
    assert_eq!(images[0].name, images[1].name);

    // Select the second occurrence (index 1)
    h.state_mut().views[0].image_selection =
        Some(pdfcraft_ui_egui::ImageSelection { page: 0, index: 1, drag: None, crop: None, turning: false, settled: None });
    h.run_steps(2);

    // Arrange send to back
    let applied = h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 1,
        change: pdfcraft_engine::ImageEdit::Arrange(pdfcraft_engine::ArrangeOrder::SendToBack),
    });
    assert!(applied);
    h.run_steps(2);

    // Because there are duplicate occurrences matching name, object, and rect,
    // selection must be cleared rather than picking an arbitrary occurrence.
    assert!(h.state().views[0].image_selection.is_none(), "selection is safely cleared when reordering ambiguous duplicate images");
}

#[test]
fn arrange_unique_image_preserves_selection() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_incremental};
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 200, 300].map(Object::Int).to_vec()));
    let artwork1 = doc.add(Object::Stream(Stream::flate(form.clone(), b"0.2 0.5 0.9 rg 20 20 160 260 re f")));
    let artwork2 = doc.add(Object::Stream(Stream::flate(form, b"0.9 0.2 0.5 rg 40 40 100 100 re f")));
    let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), b"q 1 0 0 1 0 0 cm /FigA Do Q q 1 0 0 1 0 0 cm /FigB Do Q")));
    doc.update_dict(page.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Ref(contents));
        let mut xo = Dict::new();
        xo.set(b"FigA".to_vec(), Object::Ref(artwork1));
        xo.set(b"FigB".to_vec(), Object::Ref(artwork2));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        d.set(b"Resources".to_vec(), Object::Dict(resources));
    })
    .unwrap();
    let bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("unique.pdf", None, bytes.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let doc_id = h.state().views[0].id;
    let images = h.state().session.get(doc_id).unwrap().page_images(0);
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].name, "FigA");
    assert_eq!(images[1].name, "FigB");

    // Select FigA at index 0 and bring it to front (should move after FigB, becoming index 1)
    h.state_mut().views[0].image_selection =
        Some(pdfcraft_ui_egui::ImageSelection { page: 0, index: 0, drag: None, crop: None, turning: false, settled: None });
    h.run_steps(2);

    let applied = h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Arrange(pdfcraft_engine::ArrangeOrder::BringToFront),
    });
    assert!(applied);
    h.run_steps(2);

    let sel = h.state().views[0].image_selection.as_ref().expect("selection is preserved for unique image");
    assert_eq!(sel.index, 1, "selection index tracks the unique image's new position");
}

#[test]
fn undo_arrange_re_resolves_image_selection_or_clears() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_incremental};
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 200, 300].map(Object::Int).to_vec()));
    let artwork1 = doc.add(Object::Stream(Stream::flate(form.clone(), b"0.2 0.5 0.9 rg 20 20 160 260 re f")));
    let artwork2 = doc.add(Object::Stream(Stream::flate(form, b"0.9 0.2 0.5 rg 40 40 100 100 re f")));
    let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), b"q 1 0 0 1 0 0 cm /FigA Do Q q 1 0 0 1 0 0 cm /FigB Do Q")));
    doc.update_dict(page.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Ref(contents));
        let mut xo = Dict::new();
        xo.set(b"FigA".to_vec(), Object::Ref(artwork1));
        xo.set(b"FigB".to_vec(), Object::Ref(artwork2));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        d.set(b"Resources".to_vec(), Object::Dict(resources));
    })
    .unwrap();
    let bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("undo_arrange.pdf", None, bytes.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let doc_id = h.state().views[0].id;
    let images = h.state().session.get(doc_id).unwrap().page_images(0);
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].name, "FigA");
    assert_eq!(images[1].name, "FigB");

    // Select FigA (back image, index 0)
    h.state_mut().views[0].image_selection =
        Some(pdfcraft_ui_egui::ImageSelection { page: 0, index: 0, drag: None, crop: None, turning: false, settled: None });
    h.run_steps(2);

    // Bring to front
    let applied = h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Arrange(pdfcraft_engine::ArrangeOrder::BringToFront),
    });
    assert!(applied);
    h.run_steps(2);

    // Selection now tracks FigA at index 1
    let sel = h.state().views[0].image_selection.as_ref().expect("selected after arrange");
    assert_eq!(sel.index, 1);

    // Undo
    assert!(h.state_mut().execute("edit.undo"));
    h.run_steps(2);

    let doc_after = h.state().session.get(doc_id).unwrap();
    let images_after = doc_after.page_images(0);
    assert_eq!(images_after[0].name, "FigA");
    assert_eq!(images_after[1].name, "FigB");

    // After undoing Bring to front, selection must either be FigA (index 0) or None, never FigB (index 1)
    let sel_after = h.state().views[0].image_selection.as_ref().expect("selection re-resolved to the same image");
    assert_eq!(sel_after.index, 0, "selection should point to FigA at index 0, not FigB at index 1");
    assert_eq!(images_after[sel_after.index].name, "FigA");

    // Redo
    assert!(h.state_mut().execute("edit.redo"));
    h.run_steps(2);

    let doc_redo = h.state().session.get(doc_id).unwrap();
    let images_redo = doc_redo.page_images(0);
    assert_eq!(images_redo[0].name, "FigB");
    assert_eq!(images_redo[1].name, "FigA");

    let sel_redo = h.state().views[0].image_selection.as_ref().expect("selection re-resolved after redo");
    assert_eq!(sel_redo.index, 1, "selection should point to FigA at index 1");
    assert_eq!(images_redo[sel_redo.index].name, "FigA");

    // Bringing to front when already at front is refused, but selection must be preserved
    let applied_again = h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 1,
        change: pdfcraft_engine::ImageEdit::Arrange(pdfcraft_engine::ArrangeOrder::BringToFront),
    });
    assert!(!applied_again);
    h.run_steps(2);
    let sel_refused = h.state().views[0].image_selection.as_ref().expect("selection preserved after refused arrange");
    assert_eq!(sel_refused.index, 1);
}

/// #3061: a paragraph running past its page's bottom edge (an HTML converter relying on the
/// page clip) opens in an editor that draws on its page only. It covered the next page.
#[test]
fn a_paragraph_running_off_its_page_edits_without_covering_the_next_page() {
    let _gpu = gpu();
    let text = |first: &str, count: usize, leading: usize| {
        let mut s = String::from("BT /F1 14 Tf\n");
        for i in 0..count {
            s.push_str(&format!("1 0 0 1 72 {} Tm ({first} line {}: the client may cancel this) Tj\n", 720 - (leading * i) as i64, i + 1));
        }
        s.push_str("ET");
        s.into_bytes()
    };
    // Page 1's single paragraph goes down to y = -270; page 2 is ordinary.
    let (one, two) = (text("Page one", 56, 18), text("PAGE TWO", 3, 40));
    let stream = |d: &[u8]| [format!("<< /Length {} >>\nstream\n", d.len()).into_bytes(), d.to_vec(), b"\nendstream".to_vec()].concat();
    let page = |contents: usize| {
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {contents} 0 R /Resources << /Font << /F1 7 0 R >> >> >>")
            .into_bytes()
    };
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_vec(),
        page(4),
        stream(&one),
        page(6),
        stream(&two),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".to_vec(),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(o);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 1400.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("off-page.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    h.run_steps(4);
    let rects =
        |h: &Harness<'static, PdfCraftApp>| (h.state().views[0].page_screen_rect(0).unwrap(), h.state().views[0].page_screen_rect(1).unwrap());
    // The same pages without Edit text, so without the paragraph's outline: what the gap looks
    // like there, pixel by pixel (review on #3070: it isn't one colour on every platform).
    let (first, second) = rects(&h);
    let plain = h.render().unwrap();
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    assert_eq!(rects(&h), (first, second), "Edit text keeps the pages where they were");
    let id = h.state().views[0].id;
    let info = h.state().session.get(id).unwrap().info.pages[0].clone();
    shown_on_its_page_only(&mut h, &plain, first, second, "the paragraph's outline");
    let v = info.user_to_view(200.0, 378.0);
    let p = first.min + egui::vec2(v[0], v[1]) * (first.width() / info.width);
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    crate::support::settle(&mut h);
    assert!(
        h.state().views[0].line_editor.as_ref().is_some_and(|e| e.page == 0 && e.text.contains("line 50")),
        "the paragraph's off-page lines are in it"
    );
    shown_on_its_page_only(&mut h, &plain, first, second, "the open editor");
    // Scrolled up past the tool bar, the editor stays inside the document area.
    let viewport = h.state().views[0].viewport_rect();
    let above = viewport.top() - 3.0;
    let before = h.render().unwrap();
    h.hover_at(egui::pos2(first.right() - 20.0, first.center().y));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -300.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
        source: egui::MouseWheelSource::Unknown,
    });
    h.run_steps(4);
    let scrolled = h.state().views[0].page_screen_rect(0).unwrap();
    assert!(scrolled.top() < viewport.top() - 100.0, "scrolled: {scrolled:?} in {viewport:?}");
    assert!(h.state().views[0].line_editor.is_some(), "still editing");
    let after = h.render().unwrap();
    for x in (first.left() as u32 + 70)..(first.left() as u32 + 420) {
        assert_eq!(after.get_pixel(x, above as u32), before.get_pixel(x, above as u32), "above the document area at x = {x}");
    }
}

/// Text scrolled up under the mode bar leaves its tabs the hand (#2755). The Select tool's
/// text layer showed its I-beam there, over chrome that hides the page.
#[test]
fn mode_tabs_keep_the_hand_over_page_text_scrolled_under_them() {
    let mut h = harness(2, |app| app.set_option("left", "closed").unwrap());
    let text = page_text(&h);
    // Where the page shows, its text takes the I-beam, once the page's text has arrived.
    h.state_mut().views[0].goto = Some((0, 0.3));
    h.run_steps(4);
    let top = h.state().views[0].page_screen_rect(0).expect("on screen").top();
    let mut cursor = egui::CursorIcon::Default;
    let text_cursor = crate::support::step_until(&mut h, |h| {
        h.hover_at(text.center() + egui::vec2(0.0, top));
        h.run_steps(2);
        cursor = h.output().platform_output.cursor_icon;
        cursor == egui::CursorIcon::Text
    });
    assert!(text_cursor, "the I-beam over page text, not {cursor:?}");
    assert_eq!(mode_tab_cursors(&mut h, text), HAND_ON_EVERY_TAB);
}

/// Edit text, then the All tools panel closed, as reported (#2755): a paragraph scrolled up
/// under the mode bar gave its tabs the four-way arrow, or the I-beam once a paragraph was open.
/// The open paragraph's editor also covered the tabs and took their clicks.
#[test]
fn mode_tabs_keep_the_hand_over_a_paragraph_scrolled_under_them() {
    let mut h = harness(2, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().set_option("left", "closed").unwrap();
    h.run_steps(4);
    let text = page_text(&h);
    assert_eq!(mode_tab_cursors(&mut h, text), HAND_ON_EVERY_TAB, "a closed paragraph under the tabs");
    // Open it, where it shows.
    h.state_mut().views[0].goto = Some((0, 0.3));
    h.run_steps(4);
    let top = h.state().views[0].page_screen_rect(0).expect("on screen").top();
    click_at(&mut h, text.center() + egui::vec2(0.0, top));
    assert!(h.state().views[0].line_editor.is_some());
    assert_eq!(mode_tab_cursors(&mut h, text), HAND_ON_EVERY_TAB, "the open paragraph under the tabs");
    // Nor does the editor's frame, with the paragraph's top edge just under a tab (the frame
    // reaches 2 to 6 points outside it).
    let convert = mode_tab(&h, "Convert").center();
    h.state_mut().views[0].goto = Some((0, 0.0));
    h.run_steps(4);
    let page = h.state().views[0].page_screen_rect(0).expect("on screen");
    h.state_mut().views[0].goto = Some((0, (page.top() - (convert.y + 4.0 - text.top())) / page.height()));
    h.run_steps(4);
    let page = h.state().views[0].page_screen_rect(0).expect("on screen");
    assert!((page.top() + text.top() - convert.y - 4.0).abs() < 1.0, "Convert lies on the paragraph's frame");
    h.hover_at(convert);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand, "the open paragraph's frame under a tab");
    h.state_mut().views[0].goto = Some((0, 0.5));
    h.run_steps(4);
    click_at(&mut h, convert);
    assert_eq!(h.state().mode, pdfcraft_ui_egui::Mode::Convert, "the tab takes the click, not the paragraph's editor");
}

#[test]
fn copy_pasted_image_discards_coordinates_and_moves_without_offset() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(60, 60, image::Rgb([100, 150, 200])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();

    let mut h = harness(1, |_| {});
    h.state_mut().system_clipboard = false;
    h.run_steps(4);

    // 1. Add image to page 0
    h.state_mut().add_image("test_img.png".into(), png);
    h.run_steps(3);

    let id = h.state().views[0].id;
    let images_before = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images_before.len(), 1);

    // 2. Copy the image from page 0
    assert!(h.state_mut().copy_page_image(0, 0));
    assert!(h.state().last_copied_image.is_some(), "image pixel data is kept");

    // 3. Paste the image via Ctrl+V (paste_image with rect: None)
    assert!(h.state_mut().paste_image(0, None));
    h.run_steps(3);

    let images_after_paste = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images_after_paste.len(), 2, "second image pasted");

    // The pasted image should be centered on the page, not offset by (+20, -20) from the first image
    let img1 = &images_after_paste[0];
    let img2 = &images_after_paste[1];
    assert_eq!(img1.rect, img2.rect, "pasted image placed using natural page centering");

    // 4. Move the pasted image by an explicit Move change
    let target = [50.0, 50.0, 110.0, 110.0];
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 1, change: pdfcraft_engine::ImageEdit::Move(target) });
    h.run_steps(3);

    let images_after_move = h.state().session.get(id).unwrap().page_images(0);
    assert_eq!(images_after_move[1].rect, target, "moved image rect matches target exactly without offset");
}

#[test]
fn clicking_after_image_context_menu_action_resets_menu_state_and_updates_selection() {
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
    // Move the first image to left side: [10.0, 10.0, 30.0, 30.0]
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Move([10.0, 10.0, 30.0, 30.0]),
    });
    // Add second image to right side: [50.0, 10.0, 70.0, 30.0]
    let mut png2 = Vec::new();
    image::RgbImage::from_pixel(20, 20, image::Rgb([40, 200, 40])).write_to(&mut std::io::Cursor::new(&mut png2), image::ImageFormat::Png).unwrap();
    h.state_mut().apply_edit(pdfcraft_engine::Edit::AddImage {
        page: 0,
        rect: Some([50.0, 10.0, 70.0, 30.0]),
        name: "img2.png".into(),
        bytes: std::sync::Arc::new(png2),
    });
    h.run_steps(4);

    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let info = h.state().session.get(id).unwrap().info.pages[0].clone();
    let user_to_screen = move |u: [f64; 2]| {
        let v = info.user_to_view(u[0] as f32, u[1] as f32);
        let scale = r.width() / info.width;
        egui::pos2(r.left() + v[0] * scale, r.top() + v[1] * scale)
    };

    let p0 = user_to_screen([20.0, 20.0]);
    let p1 = user_to_screen([60.0, 20.0]);
    let p_empty = user_to_screen([40.0, 35.0]);

    // 1. Select image 0
    h.hover_at(p0);
    h.run_steps(1);
    h.drag_at(p0);
    h.run_steps(1);
    h.drop_at(p0);
    h.run_steps(2);
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(0), "image 0 selected");

    // 2. Select image 1 (switches selection)
    h.hover_at(p1);
    h.run_steps(1);
    h.drag_at(p1);
    h.run_steps(1);
    h.drop_at(p1);
    h.run_steps(2);
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(1), "image 1 selected");

    // 3. Click empty space (clears selection)
    h.hover_at(p_empty);
    h.run_steps(1);
    h.drag_at(p_empty);
    h.run_steps(1);
    h.drop_at(p_empty);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_none(), "empty page clears selection");

    // 4. Select image 0 again
    h.hover_at(p0);
    h.run_steps(1);
    h.drag_at(p0);
    h.run_steps(1);
    h.drop_at(p0);
    h.run_steps(2);
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(0), "image 0 selected again");

    // 5. Right-click image 0 to open context menu
    h.hover_at(p0);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p0, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: p0, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    // 6. Click "Flip horizontal"
    h.get_by_label("Flip horizontal").click();
    h.run_steps(3);

    assert!(h.query_all_by_label("Flip horizontal").next().is_none(), "the image menu closes after its action");

    // 7. Click image 1: should switch selection to image 1
    h.hover_at(p1);
    h.run_steps(1);
    h.drag_at(p1);
    h.run_steps(1);
    h.drop_at(p1);
    h.run_steps(2);
    assert_eq!(
        h.state().views[0].image_selection.as_ref().map(|s| s.index),
        Some(1),
        "clicking other image switches selection after context menu action"
    );

    // 8. Click empty space: should clear selection
    h.hover_at(p_empty);
    h.run_steps(1);
    h.drag_at(p_empty);
    h.run_steps(1);
    h.drop_at(p_empty);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_none(), "clicking empty page clears selection after context menu action");
}
