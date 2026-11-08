//! Editing tests through the app UI (part 3 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn dragging_with_the_copy_modifier_copies_the_pages_into_the_gap() {
    let mut h = organize(3);
    let before = page_texts(h.state());
    let grab = |h: &Harness<'static, PdfCraftApp>, label: &str| h.get_by_label(label).rect();
    let (from, to) = (grab(&h, "Page 1").center(), grab(&h, "Page 3").right_center() - egui::vec2(10.0, 0.0));
    copy_drag(&mut h, from, to);
    // The original stays first; a copy of page 1 lands after page 3 (#2997).
    let after = page_texts(h.state());
    assert_eq!(after, vec![before[0].clone(), before[1].clone(), before[2].clone(), before[0].clone()], "{after:?}");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Duplicate page"));
    let sel: Vec<usize> = h.state().views[0].selected.iter().copied().collect();
    assert_eq!(sel, vec![3], "the copy is selected");
}

/// macOS is not only `cfg!(target_os = "macos")`: the wasm build runs in a Mac browser, where the
/// cfg is false but the context says Mac. The gesture must follow the context, or Option-drag
/// there moves the pages instead of copying them (#3092 review).
#[test]
fn option_copies_and_ctrl_moves_when_the_context_reports_macos() {
    let grab = |h: &Harness<'static, PdfCraftApp>, label: &str| h.get_by_label(label).rect();
    let points = |h: &Harness<'static, PdfCraftApp>| (grab(h, "Page 1").center(), grab(h, "Page 3").right_center() - egui::vec2(10.0, 0.0));

    // Ctrl is not the copy modifier on macOS, so that drag moves the page.
    let mut h = organize(3);
    h.ctx.set_os(egui::os::OperatingSystem::Mac);
    let before = page_texts(h.state());
    let (from, to) = points(&h);
    page_drag(&mut h, from, to, Modifiers::CTRL);
    assert_eq!(page_texts(h.state()), vec![before[1].clone(), before[2].clone(), before[0].clone()], "Ctrl moves on macOS");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Move page"));

    // Option is, so the same drag drops a copy at the gap.
    let mut h = organize(3);
    h.ctx.set_os(egui::os::OperatingSystem::Mac);
    let before = page_texts(h.state());
    let (from, to) = points(&h);
    page_drag(&mut h, from, to, Modifiers::ALT);
    assert_eq!(page_texts(h.state()), vec![before[0].clone(), before[1].clone(), before[2].clone(), before[0].clone()], "Option copies on macOS");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Duplicate page"));
    let sel: Vec<usize> = h.state().views[0].selected.iter().copied().collect();
    assert_eq!(sel, vec![3], "the copy is selected");
}

#[test]
fn copying_cutting_and_pasting_pages() {
    let mut h = organize(4);
    let before = page_texts(h.state());
    h.state_mut().views[0].select_pages(&[0]);
    assert!(h.state_mut().execute("page.copy"));
    h.state_mut().views[0].select_pages(&[2]);
    assert!(h.state_mut().execute("page.paste"));
    h.run_steps(2);
    let after = page_texts(h.state());
    assert_eq!(after, vec![before[0].clone(), before[1].clone(), before[2].clone(), before[0].clone(), before[3].clone()], "pasted after page 3");
    let sel: Vec<usize> = h.state().views[0].selected.iter().copied().collect();
    assert_eq!(sel, vec![3], "the pasted page is selected");
    // Cut page 2 and paste it at the end.
    h.state_mut().views[0].select_pages(&[1]);
    assert!(h.state_mut().execute("page.cut"));
    h.state_mut().views[0].select_pages(&[3]);
    assert!(h.state_mut().execute("page.paste"));
    let after = page_texts(h.state());
    assert_eq!(after, vec![before[0].clone(), before[2].clone(), before[0].clone(), before[3].clone(), before[1].clone()]);
    // Every page can be copied but not cut.
    h.state_mut().views[0].select_pages(&[0, 1, 2, 3, 4]);
    h.state_mut().execute("page.cut");
    assert_eq!(page_texts(h.state()).len(), 5);
}

#[test]
fn clicking_existing_text_selects_its_source_font_style() {
    let mut h = open_source_font_fixture();
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).expect("on screen");
    let click = |x: f32, y: f32| egui::pos2(page.left() + x / 300.0 * page.width(), page.top() + (300.0 - y) / 300.0 * page.height());

    let serif = click(25.0, 228.0);
    h.hover_at(serif);
    h.run_steps(1);
    h.drag_at(serif);
    h.run_steps(1);
    h.drop_at(serif);
    h.run_steps(3);
    let ed = h.state().views[0].line_editor.clone().expect("serif editor opens");
    assert_eq!(ed.look.family, pdfcraft_engine::FontFamily::Times);
    assert!(ed.look.bold && ed.look.italic, "source style: {:?}", ed.look);

    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    let mono = click(25.0, 128.0);
    h.hover_at(mono);
    h.run_steps(1);
    h.drag_at(mono);
    h.run_steps(1);
    h.drop_at(mono);
    h.run_steps(3);
    let ed = h.state().views[0].line_editor.clone().expect("mono editor opens");
    assert_eq!(ed.look.family, pdfcraft_engine::FontFamily::Courier);
    assert!(!ed.look.bold && ed.look.italic, "source style: {:?}", ed.look);
}

#[test]
fn deleting_a_draft_does_not_show_the_original_text_underneath() {
    let _gpu = gpu();
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).unwrap();
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + 142.0 / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
    h.state_mut().views[0].line_editor.as_mut().unwrap().text.clear();
    crate::support::settle(&mut h);
    let shot = h.render().unwrap();
    if let Ok(path) = std::env::var("PDFCRAFT_TEXT_PREVIEW_SHOT") {
        shot.save(path).unwrap();
    }
    // The right half of the original glyphs, away from the caret and editor border.
    let k = r.width() / 200.0;
    for y in 152..166 {
        for x in 65..82 {
            let px = (r.left() + x as f32 * k) as u32;
            let py = (r.top() + (300 - y) as f32 * k) as u32;
            let [red, green, blue, alpha] = shot.get_pixel(px, py).0;
            assert!(
                !(red < 100 && green < 100 && blue < 100) && alpha == 255,
                "the deleted source is still visible at {x},{y}: {:?}",
                [red, green, blue, alpha]
            );
        }
    }
    assert!(!dirty(&h), "the document is unchanged until the draft is applied");
    assert_eq!(page_texts(h.state()), ["Page 1"]);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1"]);
}

#[test]
fn switching_from_a_narrow_editor_keeps_one_frame_and_a_wide_heading_on_one_line() {
    let _gpu = gpu();
    let bytes = source_font_fixture_with_text(
        "BT /F1 18 Tf 20 220 Td (Editable PDF heading) Tj ET BT /F2 18 Tf 20 120 Td (Mono) Tj ET BT /F2 18 Tf 20 98 Td (Text) Tj ET",
    );
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("editor.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    for y in [128.0, 228.0] {
        let point = egui::pos2(page.left() + 25.0 / 300.0 * page.width(), page.top() + (300.0 - y) / 300.0 * page.height());
        h.hover_at(point);
        h.run_steps(1);
        h.drag_at(point);
        h.run_steps(1);
        h.drop_at(point);
        h.run_steps(4);
        if y == 128.0 {
            assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, "Mono\nText", "opening a paragraph keeps its source line breaks");
            h.key_press(Key::Escape);
            h.run_steps(2);
        }
    }
    let heading = h
        .output()
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Editable PDF heading" => Some(&text.galley),
            _ => None,
        })
        .expect("the heading editor is painted");
    assert_eq!(heading.rows.len(), 1, "opening a wider block must not reuse the previous editor's narrow width");
    let frames = h
        .output()
        .shapes
        .iter()
        .filter(|shape| {
            matches!(&shape.shape,
                egui::Shape::Rect(rect) if rect.stroke.color == egui::Color32::from_rgb(0x14, 0x73, 0xE6)
                    && rect.rect.width() > 100.0 && page.contains(rect.rect.center())
            )
        })
        .count();
    assert_eq!(frames, 1, "the active block has only the editor frame");
    if let Ok(path) = std::env::var("PDFCRAFT_TEXT_FRAME_SHOT") {
        h.render().unwrap().save(path).unwrap();
    }
    assert!(!dirty(&h));
}

/// Clicking a paragraph changes nothing on the page but the editor's frame and caret: the page's
/// own text stays until something is typed. The editor used to cover the paragraph (white, or
/// dark for pale text) until its textless background was ready, and drew it again in its own fonts.
#[test]
fn opening_a_paragraph_leaves_the_page_as_it_is() {
    let _gpu = gpu();
    let bytes = source_font_fixture_with_text("0 0 0.45 rg 10 180 280 70 re f 1 1 1 rg BT /F2 20 Tf 20 205 Td (White on a banner) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("banner.pdf", None, bytes).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    crate::support::settle(&mut h);
    let page = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = page.width() / 300.0;
    let block = h.state().session.get(h.state().views[0].id).unwrap().text_blocks(0)[0].rect;
    let r = egui::Rect::from_min_max(
        egui::pos2(page.left() + block[0] as f32 * k, page.top() + (300.0 - block[3] as f32) * k),
        egui::pos2(page.left() + block[2] as f32 * k, page.top() + (300.0 - block[1] as f32) * k),
    );
    let at = egui::pos2(r.left() + 2.0, r.center().y);
    h.hover_at(at);
    h.run_steps(2);
    let before = h.render().unwrap();
    // Inside the paragraph, away from the caret (at the click) and the frame around it.
    let inside = r.shrink(3.0);
    let changed = |img: &image::RgbaImage| {
        let mut n = 0;
        for y in inside.top() as u32..inside.bottom() as u32 {
            for x in inside.left() as u32..inside.right() as u32 {
                if (x as f32 - at.x).abs() < 14.0 {
                    continue;
                }
                let (a, b) = (img.get_pixel(x, y).0, before.get_pixel(x, y).0);
                if a.iter().zip(b.iter()).take(3).any(|(p, q)| (i32::from(*p) - i32::from(*q)).abs() > 40) {
                    n += 1;
                }
            }
        }
        n
    };
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    for frame in 0..8 {
        h.step();
        let n = changed(&h.render().unwrap());
        assert!(n < 20, "frame {frame} after the click changed {n} pixels of the paragraph");
    }
    assert_eq!(h.state().views[0].line_editor.as_ref().map(|e| e.text.as_str()), Some("White on a banner"));
}

/// A quick click (press and release in one frame) from an open, unchanged paragraph straight into
/// another one: in no frame does the first paragraph lose its text, and the new editor's frame
/// never spans the first one (it took the first paragraph's measured extent for a frame).
#[test]
fn a_quick_click_into_another_paragraph_keeps_both_in_place() {
    let _gpu = gpu();
    let bytes = source_font_fixture_with_text("BT /F1 24 Tf 20 220 Td (Top) Tj ET BT /F2 24 Tf 20 120 Td (Bottom) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("boxes.pdf", None, bytes).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = page.width() / 300.0;
    let mut blocks = {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0)
    };
    blocks.sort_by(|a, b| b.rect[1].total_cmp(&a.rect[1]));
    let screen = |b: &pdfcraft_engine::TextBlock| {
        egui::Rect::from_min_max(
            egui::pos2(page.left() + b.rect[0] as f32 * k, page.top() + (300.0 - b.rect[3] as f32) * k),
            egui::pos2(page.left() + b.rect[2] as f32 * k, page.top() + (300.0 - b.rect[1] as f32) * k),
        )
    };
    let (top, bottom) = (screen(&blocks[0]), screen(&blocks[1]));
    click_at(&mut h, top.center());
    crate::support::settle(&mut h);
    assert_eq!(h.state().views[0].line_editor.as_ref().map(|e| e.text.as_str()), Some("Top"));
    let ink = |pixels: &image::RgbaImage, r: egui::Rect| {
        (r.top() as u32..r.bottom() as u32)
            .flat_map(|y| (r.left() as u32..r.right() as u32).map(move |x| (x, y)))
            .filter(|(x, y)| pixels.get_pixel(*x, *y).0[..3].iter().all(|v| *v < 90))
            .count()
    };
    h.hover_at(bottom.center());
    h.run_steps(1);
    let before = ink(&h.render().unwrap(), top.shrink(2.0));
    assert!(before > 50, "the top paragraph shows");
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: bottom.center(), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
    }
    for frame in 0..4 {
        h.step();
        let now = ink(&h.render().unwrap(), top.shrink(2.0));
        assert!(now * 2 > before, "frame {frame}: the top paragraph lost its text ({now} of {before} dark pixels)");
        for shape in &h.output().shapes {
            if let egui::Shape::Rect(r) = &shape.shape
                && r.stroke.width == 1.5
                && r.rect.contains(bottom.center())
            {
                assert!(!r.rect.intersects(top), "frame {frame}: the bottom editor's frame {:?} spans the top paragraph {top:?}", r.rect);
            }
        }
    }
    assert_eq!(h.state().views[0].line_editor.as_ref().map(|e| e.text.as_str()), Some("Bottom"));
}

/// #2022: clicking straight into another text box while one is open for typing must apply the
/// open paragraph first; the typed text used to be dropped with the editor that switched away.
#[test]
fn switching_straight_to_another_box_applies_the_open_paragraph() {
    let bytes = source_font_fixture_with_text("BT /F1 24 Tf 20 220 Td (Top) Tj ET BT /F2 24 Tf 20 120 Td (Bottom) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("boxes.pdf", None, bytes).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 300.0;
    let mut blocks = {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0)
    };
    blocks.sort_by(|a, b| b.rect[1].partial_cmp(&a.rect[1]).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["Top", "Bottom"], "two editable boxes");
    let center = |b: &pdfcraft_engine::TextBlock| {
        egui::pos2(r.left() + ((b.rect[0] + b.rect[2]) / 2.0) as f32 * k, r.top() + (300.0 - (b.rect[1] + b.rect[3]) / 2.0) as f32 * k)
    };
    // Open the top box and type into it.
    click_at(&mut h, center(&blocks[0]));
    let ed = h.state().views[0].line_editor.clone().expect("the top box opens for typing");
    assert_eq!(ed.text, "Top");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Top edited".into();
    h.run_steps(1);
    // Click straight into the bottom box: the open paragraph is applied, and the bottom box opens.
    click_at(&mut h, center(&blocks[1]));
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(
        doc.text_blocks(0).iter().map(|b| b.text.clone()).collect::<Vec<_>>(),
        ["Top edited", "Bottom"],
        "the typed text is applied, not dropped"
    );
    assert_eq!(doc.can_undo(), Some("Edit text"));
    let ed = s.views[0].line_editor.clone().expect("the bottom box is now open");
    assert_eq!(ed.text, "Bottom");
}

// Regression test for #811: text typed into an open paragraph was not counted as unsaved work, so
// closing the tab or quitting discarded it without the save prompt.
#[test]
fn text_typed_into_an_open_paragraph_counts_as_unsaved_work() {
    let bytes = source_font_fixture_with_text("BT /F1 24 Tf 20 220 Td (Top) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("box.pdf", None, bytes).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 300.0;
    let block = {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0).into_iter().next().expect("one editable box")
    };
    let center = egui::pos2(
        r.left() + ((block.rect[0] + block.rect[2]) / 2.0) as f32 * k,
        r.top() + (300.0 - (block.rect[1] + block.rect[3]) / 2.0) as f32 * k,
    );
    click_at(&mut h, center);
    assert_eq!(h.state().views[0].line_editor.as_ref().map(|e| e.text.as_str()), Some("Top"), "the paragraph is open for typing");
    assert_eq!(h.state().first_dirty(), None, "opening a paragraph changes nothing");

    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Top, and a draft not applied yet".into();
    h.run_steps(1);
    assert!(!h.state().session.get(h.state().views[0].id).unwrap().dirty, "the document itself is untouched until the draft is applied");
    assert_eq!(h.state().first_dirty(), Some(0), "the draft in the open paragraph is unsaved work");

    // Typing the original text back leaves nothing to lose.
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Top".into();
    h.run_steps(1);
    assert_eq!(h.state().first_dirty(), None, "a draft equal to the page text is not unsaved work");
}

/// Applying a paragraph changes the page, whose raster still has the old text until it is
/// rendered again. Until then the paragraph keeps the look it had in the editor; the old text
/// used to come back under the new outline for those frames and then jump to the new one.
#[test]
fn an_applied_paragraph_keeps_its_edited_look_until_the_page_is_redrawn() {
    let _gpu = gpu();
    let bytes = source_font_fixture_with_text("BT /F1 24 Tf 20 220 Td (Topmost) Tj ET BT /F2 24 Tf 20 120 Td (Bottom) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("boxes.pdf", None, bytes).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 300.0;
    let top = {
        let s = h.state();
        let blocks = s.session.get(s.views[0].id).unwrap().text_blocks(0);
        let b = blocks.iter().find(|b| b.text == "Topmost").expect("the top box").rect;
        egui::Rect::from_min_max(
            egui::pos2(r.left() + b[0] as f32 * k, r.top() + (300.0 - b[3] as f32) * k),
            egui::pos2(r.left() + b[2] as f32 * k, r.top() + (300.0 - b[1] as f32) * k),
        )
    };
    // "T" stays; the rest of the word goes.
    let head = egui::Rect::from_min_max(top.min, egui::pos2(top.left() + top.width() * 0.12, top.bottom()));
    let tail = egui::Rect::from_min_max(egui::pos2(top.left() + top.width() * 0.3, top.top()), top.max).shrink(2.0);
    click_at(&mut h, top.center());
    support::wait_until(&mut h, "the editor's textless page", |app| !app.render_pending());
    h.state_mut().views[0].line_editor.as_mut().expect("the top box is open").text = "T".into();
    h.run_steps(2);
    h.state_mut().views[0].hold_rasters = true;
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    for frame in 0..4 {
        h.step();
        assert!(h.state().views[0].line_editor.is_none(), "applied");
        let pixels = h.render().unwrap();
        assert!(ink(&pixels, head) > 20, "frame {frame} after applying shows no text");
        assert_eq!(ink(&pixels, tail), 0, "frame {frame} after applying shows the old text again");
    }
    h.state_mut().views[0].hold_rasters = false;
    support::wait_until(&mut h, "the page drawn with the edit", |app| !app.render_pending());
    h.run_steps(2);
    let pixels = h.render().unwrap();
    assert!(ink(&pixels, head) > 20, "the page draws the applied text");
    assert_eq!(ink(&pixels, tail), 0, "and not the old one");
    let s = h.state();
    assert_eq!(s.session.get(s.views[0].id).unwrap().text_blocks(0).iter().map(|b| b.text.clone()).collect::<Vec<_>>(), ["T", "Bottom"]);
}

/// A paragraph whose edit fails (here a letter no font can draw) stays open with what was typed,
/// as a form field does; the editor used to close with the error and the typing was lost.
#[test]
fn a_paragraph_whose_edit_fails_stays_open_with_its_typing() {
    if crate::support::skip_without_glyphs("a_paragraph_whose_edit_fails_stays_open_with_its_typing", "en", "Top ก") {
        return;
    }
    let bytes = source_font_fixture_with_text("BT /F1 24 Tf 20 220 Td (Top) Tj ET BT /F2 24 Tf 20 120 Td (Bottom) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("boxes.pdf", None, bytes).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 300.0;
    let top = egui::pos2(r.left() + 30.0 * k, r.top() + (300.0 - 228.0) * k);
    click_at(&mut h, top);
    assert_eq!(h.state().views[0].line_editor.as_ref().map(|e| e.text.as_str()), Some("Top"));
    // U+0E01 (Thai): neither the page's font nor a fallback face draws it.
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Top ก".into();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(3);
    let s = h.state();
    assert!(s.toast.as_ref().is_some_and(|(m, _)| m.starts_with("Edit text failed")), "the failure is said: {:?}", s.toast);
    assert_eq!(s.session.get(s.views[0].id).unwrap().can_undo(), None, "nothing was applied");
    let ed = s.views[0].line_editor.as_ref().expect("the paragraph is still open");
    assert_eq!((ed.block, ed.text.as_str()), (0, "Top ก"), "with what was typed");
    // Taking the letter out applies it.
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Top edited".into();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(3);
    let s = h.state();
    assert!(s.views[0].line_editor.is_none());
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.text_blocks(0).iter().map(|b| b.text.clone()).collect::<Vec<_>>(), ["Top edited", "Bottom"]);
}

/// A paragraph opens at once: it used to fade in over a fifth of a second, its frame dimmed and
/// the page's own copy of the text showing through the editor's cover.
#[test]
fn a_paragraph_editor_opens_at_full_opacity() {
    let (mut h, [top, _]) = two_boxes_animated();
    h.hover_at(top.center());
    h.run_steps(1);
    h.drag_at(top.center());
    h.run_steps(1);
    h.drop_at(top.center());
    let accent = egui::Color32::from_rgb(0x14, 0x73, 0xE6);
    for frame in 0..8 {
        h.step();
        // The click's own frame opens it: the editor's frame is there from then on, at full colour.
        assert_eq!(editor_frame_color(&h, top), Some(accent), "frame {frame} after the click");
    }
}

/// Live editor: a repeated step with no TL opens as one paragraph, and adding a row
/// beside a kept first row leaves the neighbour as its own paragraph on screen.
#[test]
fn the_live_editor_groups_a_repeated_step_and_keeps_the_neighbour() {
    let step = 27.6_f32;
    let repeated = format!(
        "BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {} Td (Two) Tj ET BT /F1 12 Tf 72 {} Td (Three) Tj ET",
        700.0 - step,
        700.0 - 2.0 * step
    );
    let mut h = open_letter("repeated.pdf", letter_text(repeated.as_str()));
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    click_page(&mut h, 90.0, 700.0 - step + 4.0);
    let editor = h.state().views[0].line_editor.clone().expect("the middle row opens");
    assert!(editor.text.contains("One") && editor.text.contains("Two") && editor.text.contains("Three"), "{}", editor.text);
    save_live_shot(&mut h, "live-repeated-step.png");

    let partial = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
                   BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET";
    let mut h = open_letter("partial.pdf", letter_text(partial));
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    click_page(&mut h, 120.0, 704.0);
    let editor = h.state().views[0].line_editor.clone().expect("the first paragraph opens");
    assert!(editor.text.contains("Alpha first row.") && editor.text.contains("Alpha next row."), "{}", editor.text);
    assert!(!editor.text.contains("Neighbor"), "{}", editor.text);
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Alpha first row.\nTwo\nThree".into();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(6);
    let blocks = {
        let s = h.state();
        let doc = s.session.get(s.views[0].id).unwrap();
        doc.text_blocks(0).iter().map(|b| b.text.clone()).collect::<Vec<_>>()
    };
    assert_eq!(blocks, ["Alpha first row. Two Three", "Neighbor stays."]);
    save_live_shot(&mut h, "live-after-adding-a-row.png");
    click_page(&mut h, 120.0, 660.0);
    let neighbour = h.state().views[0].line_editor.clone().expect("the neighbour opens on its own");
    assert_eq!(neighbour.text, "Neighbor stays.");
    save_live_shot(&mut h, "live-neighbour-open.png");
}

/// Widening example 1 must not pull the heading into that box or split the paragraph under example 2.
#[test]
fn widening_example_one_keeps_the_heading_and_the_next_paragraph() {
    let body = "BT /F1 14 Tf 72 740 Td (1. Repeated step, no TL. Click Two.) Tj ET \
                BT /F1 12 Tf 72 700 Td (One) Tj ET \
                BT /F1 12 Tf 72 672.4 Td (Two) Tj ET \
                BT /F1 12 Tf 72 644.8 Td (Three) Tj ET \
                BT /F1 14 Tf 72 590 Td (2. Blank line. Edit the first paragraph.) Tj ET \
                BT /F1 12 Tf 14.4 TL 1 0 0 1 72 550 Tm (First paragraph, first row.) Tj T* (First paragraph, second row.) Tj ET \
                BT /F1 12 Tf 14.4 TL 1 0 0 1 72 506.8 Tm (Second paragraph, first row.) Tj T* (Second paragraph, second row.) Tj ET \
                BT /F1 14 Tf 72 450 Td (3. Add a row under Alpha. Neighbor stays.) Tj ET \
                BT /F1 12 Tf 14.4 TL 1 0 0 1 72 410 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
                BT /F1 12 Tf 14.4 TL 1 0 0 1 72 366.8 Tm (Neighbor stays.) Tj ET";
    let mut h = open_letter("letter.pdf", letter_text(body));
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    click_page(&mut h, 90.0, 690.0);
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "One\nTwo\nThree\nfour".into();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(8);
    let (edge, page_id) = {
        let s = h.state();
        let id = s.views[0].id;
        let blocks = s.session.get(id).unwrap().text_blocks(0);
        let texts: Vec<_> = blocks.iter().map(|b| b.text.clone()).collect();
        assert!(texts.iter().any(|t| t == "1. Repeated step, no TL. Click Two."), "commit already joined the heading: {texts:?}");
        assert!(
            texts.iter().any(|t| t.contains("First paragraph, first row") && t.contains("second row")),
            "commit already split the next paragraph: {texts:?}"
        );
        let block = blocks.iter().find(|b| b.text.contains("four")).expect("the edited paragraph");
        let page = s.views[0].page_screen_rect(0).unwrap();
        let edge = egui::pos2(
            page.left() + block.rect[2] as f32 / 612.0 * page.width(),
            page.top() + (792.0 - (block.rect[1] + block.rect[3]) as f32 / 2.0) / 792.0 * page.height(),
        );
        (edge, id)
    };
    let to = edge + egui::vec2(280.0, 0.0);
    h.hover_at(edge);
    h.run_steps(1);
    h.drag_at(edge);
    h.run_steps(1);
    for k in 1..=6 {
        h.hover_at(edge + (to - edge) * (k as f32 / 6.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(8);
    let texts: Vec<_> = {
        let s = h.state();
        s.session.get(page_id).unwrap().text_blocks(0).iter().map(|b| b.text.clone()).collect()
    };
    let edited = {
        let s = h.state();
        s.session
            .get(page_id)
            .unwrap()
            .text_blocks(0)
            .iter()
            .find(|b| b.editor_text.contains("four"))
            .expect("the widened paragraph")
            .editor_text
            .clone()
    };
    assert_eq!(edited, "One\nTwo\nThree\nfour", "the width handle flattened the line breaks: {edited:?}");
    assert!(texts.iter().any(|t| t == "1. Repeated step, no TL. Click Two."), "the width handle joined the heading: {texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("First paragraph, first row") && t.contains("First paragraph, second row")),
        "the width handle split the next paragraph: {texts:?}"
    );
    save_live_shot(&mut h, "live-example-one-widened.png");
}

/// The live width handle, after a row was added on top of the neighbour.
#[test]
fn resizing_the_live_box_keeps_the_neighbour() {
    let partial = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 410 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
                   BT /F1 12 Tf 14.4 TL 1 0 0 1 72 366.8 Tm (Neighbor stays.) Tj ET";
    let mut h = open_letter("partial.pdf", letter_text(partial));
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    click_page(&mut h, 120.0, 414.0);
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Alpha first row.\nthis is another\nalpha test\nAlpha next row.".into();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(6);
    let before = {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0).iter().map(|b| b.text.clone()).collect::<Vec<_>>()
    };
    assert!(before.iter().any(|t| t.contains("Neighbor stays.")), "the edit already dropped it: {before:?}");
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let rect = h.state().session.get(h.state().views[0].id).unwrap().text_blocks(0)[0].rect;
    let edge = egui::pos2(
        page.left() + rect[2] as f32 / 612.0 * page.width(),
        page.top() + (792.0 - (rect[1] + rect[3]) as f32 / 2.0) / 792.0 * page.height(),
    );
    let to = edge + egui::vec2(80.0, 0.0);
    h.hover_at(edge);
    h.run_steps(1);
    h.drag_at(edge);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(edge + (to - edge) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(6);
    let after = {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0).iter().map(|b| b.text.clone()).collect::<Vec<_>>()
    };
    assert!(after.iter().any(|t| t.contains("Neighbor stays.")), "resize removed the neighbour: {after:?}");
    save_live_shot(&mut h, "live-after-resize.png");
}

#[test]
fn editing_existing_text_in_place() {
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
    let ed = h.state().views[0].line_editor.clone().expect("the line opens for editing");
    assert_eq!((ed.page, ed.block, ed.text.as_str()), (0, 0, "Page 1"));
    // The source container stays visible while added text can overflow it.
    assert!(ed.growth().is_none(), "opening the editor preserves the source container");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Chapter One".into();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Edit text"));
    assert_eq!(doc.text_lines(0)[0].text, "Chapter One");
    assert_eq!(texts_of(s, 0), ["Chapter One"], "the page shows it");
}

/// The Read mode command (View ▸ Read mode, its shortcut, the palette) leaves Edit as the Edit tab
/// does when another tab is chosen (#2241): Edit text & images is put down, and a paragraph still
/// open in the editor is applied instead of staying open, editable, over the page in Read mode.
#[test]
fn the_read_mode_command_applies_an_open_paragraph_and_puts_edit_text_down() {
    let mut h = harness(1, |_| {});
    edit_tab_with_an_open_paragraph(&mut h);
    assert!(h.state_mut().execute("view.read_mode"));
    h.run_steps(3);
    assert_eq!(h.state().mode, pdfcraft_ui_egui::Mode::Read);
    assert_edit_text_put_down_and_the_paragraph_applied(h.state());
}

/// The Edit panel's back arrow (it has one when opened from All tools; the Edit tab is its own way
/// back) leaves for All tools as the mode bar does: Edit text & images is put down and a paragraph
/// still open in the editor is applied, instead of every paragraph keeping its box, and the editor
/// staying open, under All tools.
#[test]
fn the_edit_panel_back_arrow_applies_an_open_paragraph_and_puts_edit_text_down() {
    use pdfcraft_ui_egui::{LeftPanel, Mode, QuickTool};
    let mut h = harness(1, |_| {});
    // All tools ▸ Edit a PDF ▸ Edit text & images: the Edit panel, under All tools.
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(3);
    assert_eq!((h.state().mode, h.state().quick_tool), (Mode::AllTools, QuickTool::EditText));
    open_the_first_line(&mut h);
    h.get_by_label("Back to all tools").click();
    h.run_steps(3);
    assert_eq!((h.state().mode, h.state().left), (Mode::AllTools, LeftPanel::AllTools));
    assert_edit_text_put_down_and_the_paragraph_applied(h.state());
}

/// Organize pages, opened under the Edit tab, leaves Edit the same way through its own back arrow.
#[test]
fn the_organize_back_arrow_applies_an_open_paragraph_and_puts_edit_text_down() {
    use pdfcraft_ui_egui::{LeftPanel, Mode};
    let mut h = harness(1, |_| {});
    edit_tab_with_an_open_paragraph(&mut h);
    // What the Edit panel's Organize pages row runs.
    assert!(h.state_mut().execute("page.organize"));
    h.run_steps(3);
    assert!(h.state().views[0].organize, "the page grid opens");
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph is still open behind the grid");
    h.get_by_label("Back to all tools").click();
    h.run_steps(3);
    let s = h.state();
    assert_eq!((s.mode, s.left), (Mode::AllTools, LeftPanel::AllTools));
    assert!(!s.views[0].organize, "the grid closes");
    assert_edit_text_put_down_and_the_paragraph_applied(s);
}

#[test]
fn text_tool_clicking_source_text_edits_instead_of_adding_a_copy() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let blank = egui::pos2(page.left() + 0.2 * page.width(), page.top() + 0.1 * page.height());
    h.hover_at(blank);
    h.run_steps(1);
    h.drag_at(blank);
    h.run_steps(1);
    h.drop_at(blank);
    h.run_steps(3);
    h.event(egui::Event::Text("Keep this note".into()));
    h.run_steps(3);
    let point = egui::pos2(page.left() + 40.0 / 200.0 * page.width(), page.top() + 142.0 / 300.0 * page.height());
    h.hover_at(point);
    h.run_steps(1);
    h.drag_at(point);
    h.run_steps(1);
    h.drop_at(point);
    h.run_steps(4);
    assert!(h.state().views[0].content.draft.is_none(), "clicking source text must not start an added layer");
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, "Page 1");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().added().len(), 1, "the previous draft is kept");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("Replacement".into()));
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Keep this note\nReplacement"]);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().added().len(), 1, "editing the source adds no second layer");
}

#[test]
fn text_tool_mixed_style_row_grows_in_the_editor_and_saved_pdf() {
    let _gpu = gpu();
    let bytes = source_font_fixture_with_text("BT /F1 12 Tf 20 220 Td (Small ) Tj /F2 12 Tf (row) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("mixed-row.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let point = egui::pos2(page.left() + 25.0 / 300.0 * page.width(), page.top() + 75.0 / 300.0 * page.height());
    h.hover_at(point);
    h.run_steps(1);
    h.drag_at(point);
    h.run_steps(1);
    h.drop_at(point);
    h.run_steps(4);
    let replacement = "Small row with more words";
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" with more words".into()));
    h.run_steps(4);
    let galley = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == replacement => Some(&t.galley),
            _ => None,
        })
        .expect("the draft is painted");
    assert_eq!(galley.rows.len(), 1, "mixed source runs grow as one visual row");
    if let Ok(path) = std::env::var("PDFCRAFT_MIXED_ROW_SHOT") {
        h.render().unwrap().save(path).unwrap();
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(3);
    let state = h.state();
    let doc = state.session.get(state.views[0].id).unwrap();
    assert_eq!(doc.text_blocks(0)[0].editor_text, replacement);
    assert_eq!(doc.text_blocks(0)[0].rows.len(), 1);
    let bytes = state.session.save_bytes(state.views[0].id).unwrap();
    let mut reopened = pdfcraft_engine::Session::new();
    let id = reopened.open("again.pdf", None, bytes, None).unwrap();
    assert_eq!(reopened.get(id).unwrap().text_blocks(0)[0].rows.len(), 1);
}

#[test]
fn editing_a_double_drawn_line_replaces_every_copy() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("bold.pdf", None, double_drawn()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
    let ed = h.state().views[0].line_editor.clone().expect("the line opens for editing");
    assert_eq!(ed.text, "Page 1");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Replaced".into();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    h.run_steps(4);
    let s = h.state();
    assert_eq!(s.session.get(s.views[0].id).unwrap().text_lines(0).len(), 1, "one line");
    assert_eq!(texts_of(s, 0), ["Replaced"], "no copy of the old text shows under it");
}

#[test]
fn editing_existing_images_on_the_page() {
    // A page made from a 40 × 20 image (at 72 dpi: a 40 × 20 pt page filled by it).
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 40, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    check_existing_artwork("picture.png", png);
}

/// Add text places a box with a click anywhere off its own boxes: on a scanned page (one picture
/// filling it) too, and just beside a paragraph, where Edit text would rewrap it.
#[test]
fn add_text_places_a_box_on_a_picture_and_beside_a_paragraph() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(80, 40, image::Rgb([200, 40, 40])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("picture.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::AddText;
    h.run_steps(2);
    let c = h.state().views[0].page_screen_rect(0).expect("on screen").center();
    h.hover_at(c);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Text, "the I-beam over the picture");
    click_at(&mut h, c);
    assert!(h.state().views[0].content.draft.is_some(), "a click on the picture starts a text box");

    let mut h = harness(2, |_| {});
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::AddText;
    h.run_steps(2);
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    // 6 px left of the text: 4 px past its box, within a closed paragraph's edge grip in Edit text.
    let beside = egui::pos2(r.left() + block.rect[0] as f32 * k, r.top() + (300.0 - ((block.rect[1] + block.rect[3]) / 2.0) as f32) * k)
        - egui::vec2(6.0, 0.0);
    click_at(&mut h, beside);
    assert!(h.state().views[0].content.draft.is_some(), "a click beside the paragraph starts a text box");
}

/// Add text opens a paragraph with a click anywhere in its box, just inside its edge too, where the
/// edge's grip shows (a drag there rewraps it, as with Edit text). Add text's hits left the grip
/// out there, so the click neither opened the paragraph nor started a box.
#[test]
fn add_text_opens_a_paragraph_from_just_inside_its_edge() {
    let mut h = harness(2, |_| {});
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::AddText;
    h.run_steps(2);
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    // 2 px right of where the text starts: 4 px into its box, within its left edge's grip.
    let inside = egui::pos2(r.left() + block.rect[0] as f32 * k, r.top() + (300.0 - ((block.rect[1] + block.rect[3]) / 2.0) as f32) * k)
        + egui::vec2(2.0, 0.0);
    h.hover_at(inside);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::ResizeHorizontal, "the edge's grip");
    click_at(&mut h, inside);
    assert!(h.state().views[0].line_editor.is_some(), "the click opens the paragraph for typing");
    assert!(h.state().views[0].content.draft.is_none(), "and starts no text box");
}

/// With Add text a paragraph dragged off its own box still moves to where the button comes up, as
/// with Edit text. Add text acted on the page's paragraphs only while the pointer was over one, so
/// the drag was left hanging, to land wherever the pointer next met a paragraph.
#[test]
fn add_text_finishes_a_paragraph_drag_that_leaves_its_box() {
    let mut h = harness(2, |_| {});
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::AddText;
    h.run_steps(2);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().text_blocks(0)[0].rect;
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let from = egui::pos2(r.left() + ((before[0] + before[2]) / 2.0) as f32 * k, r.top() + (300.0 - ((before[1] + before[3]) / 2.0) as f32) * k);
    // The drag starts in the box (8 px down), then goes 60 pt down, onto the blank page under it.
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    h.hover_at(from + egui::vec2(0.0, 8.0));
    h.run_steps(1);
    for i in 1..=4 {
        h.hover_at(from + egui::vec2(0.0, 15.0 * k * i as f32));
        h.run_steps(1);
    }
    h.drop_at(from + egui::vec2(0.0, 60.0 * k));
    for _ in 0..10 {
        h.run_steps(2);
    }
    assert!(h.state().views[0].block_drag.is_none(), "the drag is over");
    let after = h.state().session.get(id).unwrap().text_blocks(0)[0].rect;
    assert!(before[1] - after[1] > 50.0, "the paragraph moved down with it: {before:?} to {after:?}");
}

#[test]
fn dragging_the_rotate_knob_turns_a_page_image() {
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
    // A smaller picture in the middle of the page: the knob is over the page, not beside it.
    let full = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let (qw, qh) = ((full[2] - full[0]) / 4.0, (full[3] - full[1]) / 4.0);
    let (mx, my) = ((full[0] + full[2]) / 2.0, (full[1] + full[3]) / 2.0);
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Move([mx - qw, my - qh, mx + qw, my + qh]),
    });
    h.run_steps(4);
    let before = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();
    // Select the picture.
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");
    // Grab the knob above the top edge and swing it a quarter turn clockwise (to the right).
    let knob = egui::pos2(c.x, c.y - r.height() / 4.0 - 22.0);
    let to = egui::pos2(c.x + 100.0, c.y);
    h.hover_at(knob);
    h.run_steps(1);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Grab, "the knob offers to turn the picture");
    h.drag_at(knob);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(knob + (to - knob) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Grabbing, "and holds it while it turns");
    h.drop_at(to);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Rotate image"));
    let turned = doc.page_images(0)[0].rect;
    let (w, hh) = (before[2] - before[0], before[3] - before[1]);
    assert!((turned[2] - turned[0] - hh).abs() < 1.5 && (turned[3] - turned[1] - w).abs() < 1.5, "{before:?} -> {turned:?}");
}

#[test]
fn dragging_rotate_knob_on_already_rotated_image_applies_delta_only() {
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

    // Center picture
    let full = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let (qw, qh) = ((full[2] - full[0]) / 4.0, (full[3] - full[1]) / 4.0);
    let (mx, my) = ((full[0] + full[2]) / 2.0, (full[1] + full[3]) / 2.0);
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
        page: 0,
        index: 0,
        change: pdfcraft_engine::ImageEdit::Move([mx - qw, my - qh, mx + qw, my + qh]),
    });
    h.run_steps(4);

    // Rotate the image by 45° clockwise initially
    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage { page: 0, index: 0, change: pdfcraft_engine::ImageEdit::RotateBy(45.0) });
    h.run_steps(4);

    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();

    // Select the picture
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    let initial_img = &h.state().session.get(id).unwrap().page_images(0)[0];
    let initial_matrix = initial_img.matrix;
    let xf = h.state().views[0].page_xform(0).expect("page 0 xform");
    let doc_info = &h.state().session.get(id).unwrap().info;
    let obb = pdfcraft_ui_egui::canvas::ImageObb::from_matrix(initial_matrix, &xf, doc_info, 0);
    let knob = obb.s_rot;
    let center = obb.screen_center;
    let radius = (knob - center).length();

    // Hover at the rotated knob position:
    h.hover_at(knob);
    h.run_steps(1);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Grab, "knob offers Grab cursor on 45° rotated image");

    // Rotate clockwise by a small delta of 15° (from 45° to 60°):
    let to_angle = (45.0 + 15.0_f32).to_radians();
    let to = center + egui::vec2(radius * to_angle.sin(), -radius * to_angle.cos());

    h.drag_at(knob);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(knob + (to - knob) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Grabbing);
    h.drop_at(to);
    h.run_steps(4);

    let doc = h.state().session.get(id).unwrap();
    let final_img = &doc.page_images(0)[0];
    let angle_deg = (-final_img.matrix[1]).atan2(final_img.matrix[0]).to_degrees();
    let angle_deg = (angle_deg % 360.0 + 360.0) % 360.0;
    assert!((angle_deg - 60.0).abs() < 2.0, "expected ~60° total rotation, got {angle_deg}°");
}

#[test]
fn editing_existing_form_artwork_on_the_page() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_incremental};
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 200, 300].map(Object::Int).to_vec()));
    let artwork = doc.add(Object::Stream(Stream::flate(form, b"0.2 0.5 0.9 rg 20 20 160 260 re f")));
    let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), b"/Figure Do")));
    doc.update_dict(page.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Ref(contents));
        let mut xo = Dict::new();
        xo.set(b"Figure".to_vec(), Object::Ref(artwork));
        let mut resources = Dict::new();
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        d.set(b"Resources".to_vec(), Object::Dict(resources));
    })
    .unwrap();
    check_existing_artwork("figure.pdf", write_incremental(&doc, &SaveOptions::default()).unwrap());
}

#[test]
fn image_right_click_context_menu_items_and_actions() {
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
    // Select image
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");

    // Right-click image
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some());

    for item in [
        "Cut",
        "Copy",
        "Paste",
        "Flip horizontal",
        "Flip vertical",
        "Arrange image",
        "Replace image",
        "Draw freehand",
        "Redact Image",
        "Create Link",
        "Recognize Text",
    ] {
        h.get_by_label_contains(item);
    }

    // Top horizontal icon buttons
    h.get_by_label("Rotate Counterclockwise");
    h.get_by_label("Rotate Clockwise");
    h.get_by_label("Delete");

    // Flip horizontal via context menu item
    h.get_by_label("Flip horizontal").click();
    h.run_steps(3);
    let id = h.state().views[0].id;
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Flip image"));

    // Undo flip
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(3);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.can_undo(), None);
}

#[test]
fn default_page_context_menu_when_not_in_edit_mode() {
    let mut h = harness(1, |_| {});
    h.run_steps(4);
    assert_eq!(h.state().quick_tool, pdfcraft_ui_egui::QuickTool::Select);

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();
    // Right click on page in Select mode
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    // Verify default page context menu items
    h.get_by_label("Rotate view counterclockwise");
    h.get_by_label("Print");
    h.get_by_label("Select Tool");
    h.get_by_label("Hand Tool");
    h.get_by_label("Marquee Zoom");

    for item in ["Add bookmark", "Take a snapshot", "Edit this PDF", "Delete page", "Export this PDF", "Find text or tools", "Document properties…"]
    {
        h.get_by_label_contains(item);
    }
    h.get_by_label("Rotate view clockwise");
    assert!(h.query_all_by_label("Add comment").next().is_some());
    assert!(h.query_all_by_label("Add text").next().is_some());
    assert!(h.query_all_by_label("Add image").next().is_some());
    assert!(h.query_all_by_label_contains("Copy").next().is_some());
    assert!(h.query_all_by_label_contains("Paste").next().is_some());

    // Clicking "Edit this PDF" enters edit mode
    h.get_by_label("Edit this PDF").click();
    h.run_steps(2);
    assert_eq!(h.state().quick_tool, pdfcraft_ui_egui::QuickTool::EditText);
}

#[test]
fn page_context_menu_items_disabled_on_protected_pdf() {
    let bytes = protected("", "owner", 0b0100);
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("protected.pdf", None, bytes.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert_eq!(h.state().quick_tool, pdfcraft_ui_egui::QuickTool::Select);

    let doc_id = h.state().views[0].id;
    let doc = h.state().session.get(doc_id).unwrap();
    assert!(!doc.allows_modification());
    assert!(!doc.allows_assembly());
    assert!(!doc.allows_annotation());

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let c = r.center();
    // Right click on page in Select mode
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    let is_disabled =
        |h: &mut Harness<'static, PdfCraftApp>, label: &str| h.query_by(|n| n.label().as_deref() == Some(label) && n.is_disabled()).is_some();

    // Context menu is open; check restricted items are greyed out (disabled)
    assert!(is_disabled(&mut h, "Edit this PDF"), "Edit this PDF should be disabled on protected document");
    assert!(is_disabled(&mut h, "Add text"), "Add text should be disabled");
    assert!(is_disabled(&mut h, "Add image"), "Add image should be disabled");
    assert!(is_disabled(&mut h, "Delete page"), "Delete page should be disabled");
    assert!(is_disabled(&mut h, "Add comment"), "Add comment should be disabled");
    assert!(is_disabled(&mut h, "Add bookmark"), "Add bookmark should be disabled");

    // View-only items should still be enabled
    assert!(!is_disabled(&mut h, "Select Tool"));
    assert!(!is_disabled(&mut h, "Hand Tool"));
    assert!(!is_disabled(&mut h, "Find text or tools"));
}

#[test]
fn image_edge_handle_stretch_and_crop_mode() {
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

    // Enter crop mode via context menu
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    h.get_by_label("Crop").click();
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_some()));

    // Set smaller crop rect
    let smaller = egui::Rect::from_center_size(c, egui::vec2(50.0, 50.0));
    set_crop_box(&mut h, smaller);

    // Confirm crop with Enter key
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert!(!h.state().views[0].image_selection.as_ref().is_some_and(|s| s.crop.is_some()));
    let doc_id = h.state().views[0].id;
    let images = h.state().session.get(doc_id).unwrap().page_images(0);
    assert_eq!(images.len(), 1, "image still exists: {images:?}");
    assert!(images[0].visible_rect.is_some(), "cropped image is visible");
    assert_ne!(images[0].visible_rect, Some(images[0].rect), "image is cropped");

    // Remove crop via context menu
    h.hover_at(c);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: c, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    h.get_by_label("Remove crop").click();
    h.run_steps(4);
    let images = h.state().session.get(doc_id).unwrap().page_images(0);
    assert_eq!(images[0].visible_rect, Some(images[0].rect), "crop is removed");
}

#[test]
fn tiny_image_crop_handle_drag_does_not_panic() {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(5, 5, image::Rgb([100, 150, 200])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("tiny.png", None, png.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    // Enter crop mode on image 0 with small dimensions (5.0 x 5.0)
    h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection {
        page: 0,
        index: 0,
        drag: None,
        crop: Some(pdfcraft_ui_egui::CropMode::new([100.0, 100.0, 105.0, 105.0])),
        turning: false,
        settled: None,
    });
    h.run_steps(1);

    // Hover and drag a handle across a 5.0 x 5.0 image to ensure clamp bounds do not panic
    h.hover_at(egui::pos2(100.0, 100.0));
    h.run_steps(1);
    h.drag_at(egui::pos2(100.0, 100.0));
    h.run_steps(1);
    h.hover_at(egui::pos2(105.0, 105.0));
    h.run_steps(2);
    h.drop_at(egui::pos2(105.0, 105.0));
    h.run_steps(2);
}

#[test]
fn text_selection_action_icons() {
    let mut h = harness(1, |_| {});
    // Dragging selects only once the page's text layer is in, and it comes from the render pool
    // in the background: under load a fixed number of frames was sometimes not enough.
    support::wait_until(&mut h, "page 1's text layer", |app| app.views[0].has_page_text(0));
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let p0 = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    let p1 = egui::pos2(r.left() + 100.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());

    // Drag to select text
    h.hover_at(p0);
    h.run_steps(1);
    h.drag_at(p0);
    h.run_steps(1);
    h.hover_at(p1);
    h.run_steps(1);
    h.drop_at(p1);
    h.run_steps(2);

    assert!(h.state().views[0].selected_text().is_some());

    // Verify all 6 minimalist action icons are present in the floating menu
    assert!(h.query_all_by_label("Add comment").next().is_some());
    assert!(h.query_all_by_label("Highlight").next().is_some());
    assert!(h.query_all_by_label("Underline").next().is_some());
    assert!(h.query_all_by_label("Strikethrough").next().is_some());
    assert!(h.query_all_by_label("Replace text").next().is_some());
    assert!(h.query_all_by_label("Copy").next().is_some());
}
