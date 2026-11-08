//! Editing tests through the app UI (part 4 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn the_format_panel_restyles_the_paragraph_being_edited() {
    let mut h = harness(1, |_| {});
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
    assert!(h.state().views[0].line_editor.is_some());
    // The Format text panel shows the paragraph's look; making it bold applies at once.
    h.get_by_label("FORMAT TEXT");
    h.get_by_label("Bold").click();
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Edit text"));
    assert_eq!(doc.text_blocks(0)[0].base_font, "Helvetica-Bold");
    assert!(s.views[0].line_editor.is_some(), "still editing");
    // Underline: another step, the text keeps its bold.
    h.get_by_label("Underline").click();
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.text_blocks(0)[0].base_font, "Helvetica-Bold");
    assert!(s.views[0].line_editor.as_ref().is_some_and(|e| e.extras.underline));
}

#[test]
fn the_format_panel_toggles_superscript_and_subscript_on_selection() {
    let mut h = harness(1, |_| {});
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
    assert!(h.state().views[0].line_editor.is_some());

    // In line editor, select "1" in "Page 1":
    let ed = h.state_mut().views[0].line_editor.as_mut().unwrap();
    ed.selection = Some(5..6);
    ed.focus = true;
    h.run_steps(1);

    // Click Superscript: "1" becomes "¹"
    h.get_by_label("FORMAT TEXT");
    let btn = h.get_by_label("Superscript");
    btn.click();
    h.run_steps(4);

    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Edit text"));
    assert_eq!(doc.text_blocks(0)[0].text, "Page ¹");

    // Select "¹" in "Page ¹" to toggle it back:
    let ed = h.state_mut().views[0].line_editor.as_mut().unwrap();
    ed.selection = Some(5..7);
    ed.focus = true;
    h.run_steps(1);

    // Click Superscript again: "¹" toggles back to "1"
    h.get_by_label("Superscript").click();
    h.run_steps(4);
    let s2 = h.state();
    let doc2 = s2.session.get(s2.views[0].id).unwrap();
    assert_eq!(doc2.text_blocks(0)[0].text, "Page 1");

    // Subscript toggle in added text:
    h.state_mut().views[0].line_editor = None;
    h.state_mut().quick_tool = pdfcraft_ui_egui::QuickTool::AddText;
    let draft = pdfcraft_ui_egui::TextDraft {
        page: 0,
        index: None,
        rect: [20.0, 20.0, 120.0, 40.0],
        text: "H2O".into(),
        style: Default::default(),
        focus: false,
        selection: Some(1..2),
        typing_from: None,
    };
    h.state_mut().views[0].content.draft = Some(draft);
    h.run_steps(2);

    h.get_by_label("Subscript").click();
    h.run_steps(2);
    assert_eq!(h.state().views[0].content.draft.as_ref().map(|d| d.text.as_str()), Some("H₂O"));

    h.get_by_label("Subscript").click();
    h.run_steps(2);
    assert_eq!(h.state().views[0].content.draft.as_ref().map(|d| d.text.as_str()), Some("H2O"));
}

/// Horizontal scale and Character spacing take any value in their ranges, not only the listed
/// ones: values between the listed ones and beyond them, typed in Format text, reach the saved
/// file as the engine writes them.
#[test]
fn the_format_panel_takes_any_scale_and_character_spacing() {
    let mut h = harness(1, |_| {});
    let id = h.state().views[0].id;
    let width = |h: &Harness<'static, PdfCraftApp>, view: usize| {
        let s = h.state();
        let r = s.session.get(s.views[view].id).unwrap().text_blocks(0)[0].rect;
        r[2] - r[0]
    };
    let plain = width(&h, 0);
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
    assert!(h.state().views[0].line_editor.is_some());
    // Typed the way a person does: click the field, type, Enter.
    let type_in = |h: &mut Harness<'static, PdfCraftApp>, label: &str, value: &str| {
        h.get_by_role_and_label(Role::SpinButton, label).click();
        h.run_steps(2);
        h.event(egui::Event::Text(value.into()));
        h.run_steps(1);
        h.key_press(Key::Enter);
        h.run_steps(4);
    };
    let extras = |h: &Harness<'static, PdfCraftApp>| h.state().views[0].line_editor.as_ref().map(|e| (e.extras.scale, e.extras.char_spacing));
    // Between the listed values (50…200 %, −1…5 pt) and beyond them, within what was always allowed.
    type_in(&mut h, "Horizontal scale", "80");
    assert_eq!(extras(&h), Some((80.0, 0.0)));
    type_in(&mut h, "Horizontal scale", "250");
    assert_eq!(extras(&h), Some((250.0, 0.0)));
    type_in(&mut h, "Character spacing", "0.25");
    assert_eq!(extras(&h), Some((250.0, 0.25)));
    type_in(&mut h, "Character spacing", "-2");
    assert_eq!(extras(&h), Some((250.0, -2.0)));
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Edit text"));
    // Saved and reopened, the paragraph is as wide as the engine draws it with those values.
    let bytes = h.state().session.save_bytes(id).unwrap();
    h.state_mut().open_bytes("reference.pdf", None, fixture(1)).expect("opens");
    let style = pdfcraft_engine::BlockStyle { scale: Some(250.0), char_spacing: Some(-2.0), ..Default::default() };
    assert!(h.state_mut().apply_edit(pdfcraft_engine::Edit::EditTextBlock { page: 0, block: 0, text: "Page 1".into(), style }));
    h.state_mut().open_bytes("saved.pdf", None, bytes.as_ref().clone()).expect("the saved file opens");
    h.run_steps(3);
    let (reference, saved) = (width(&h, 1), width(&h, 2));
    assert!(saved > plain * 1.5, "250 % widens it: {plain} → {saved}");
    assert!((saved - reference).abs() < 0.5, "saved {saved}, the engine's own {reference}");
}

#[test]
fn typed_small_coloured_text_size_survives_save_and_reopen() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).unwrap();
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
    assert!(h.state().views[0].line_editor.is_some());
    h.get_by_label("Font size:").click();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("6.5".into()));
    h.key_press(Key::Enter);
    h.run_steps(4);
    h.get_by_label("Colour").click();
    h.run_steps(2);
    h.get_by_label("Blue").click();
    h.run_steps(4);
    let id = h.state().views[0].id;
    let lines = h.state().session.get(id).unwrap().text_lines(0);
    assert!(!lines.is_empty());
    for line in &lines {
        assert!((line.size - 6.5).abs() < 0.01, "{line:?}");
        assert!(line.color[2] > line.color[0], "the small text stays blue: {line:?}");
    }
    let saved = h.state().session.save_bytes(id).unwrap();
    let mut reopened = pdfcraft_engine::Session::new();
    let rid = reopened.open("small-blue.pdf", None, saved, None).unwrap();
    assert_eq!(reopened.get(rid).unwrap().text_lines(0), lines);
}

#[test]
fn selected_text_formatting_keeps_the_rest_of_the_paragraph_and_reopens() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).unwrap();
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(4);
    h.key_press_modifiers(Modifiers::COMMAND, Key::ArrowRight);
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
    h.run_steps(2);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().selection, Some(5..6));
    h.get_by_label("Bold").click();
    h.run_steps(5);
    let id = h.state().views[0].id;
    let doc = h.state().session.get(id).unwrap();
    let lines = doc.text_lines(0);
    assert!(lines.iter().any(|l| l.text == "1" && l.base_font == "Helvetica-Bold"), "{lines:?}");
    assert!(lines.iter().any(|l| l.text.trim_end() == "Page" && l.base_font == "Helvetica"));
    let saved = h.state_mut().session.save_bytes(id).unwrap();
    let mut reopen = pdfcraft_engine::Session::new();
    let rid = reopen.open("saved.pdf", None, saved, None).unwrap();
    assert_eq!(reopen.get(rid).unwrap().text_lines(0), lines);
}

#[test]
fn dragging_a_paragraph_moves_it_and_its_edge_rewraps_it() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let block = |h: &Harness<'static, PdfCraftApp>| {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0)[0].clone()
    };
    // User space (200 × 300 page) → screen.
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    // Move "Page 1" 30 pt right and 50 pt up.
    let before = block(&h);
    let mid = screen((before.rect[0] + before.rect[2]) / 2.0, (before.rect[1] + before.rect[3]) / 2.0);
    drag(&mut h, mid, mid + egui::vec2(30.0 * k, -50.0 * k));
    assert!(h.state().views[0].line_editor.is_none(), "dragging moves the box; it doesn't open it for typing");
    let moved = block(&h);
    assert_eq!(moved.text, "Page 1");
    let near = |a: f64, b: f64| (a - b).abs() < 1.5;
    assert!(near(moved.rect[0], before.rect[0] + 30.0) && near(moved.rect[1], before.rect[1] + 50.0), "{:?} → {:?}", before.rect, moved.rect);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Move object"));
    // Drag the handle on its right edge in to about 45 pt wide: "Page" and "1" rewrap onto two lines.
    let edge = screen(moved.rect[2], (moved.rect[1] + moved.rect[3]) / 2.0) + egui::vec2(2.0, 0.0);
    let narrower = (moved.rect[2] - moved.rect[0] - 45.0) as f32 * k;
    drag(&mut h, edge, edge - egui::vec2(narrower, 0.0));
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    let lines: Vec<String> = doc.text_lines(0).iter().map(|l| l.text.clone()).collect();
    assert_eq!(lines, ["Page", "1"], "rewrapped to the narrower box");
    assert!(near(doc.text_lines(0)[0].rect[0], moved.rect[0]), "it keeps its place");
    assert_eq!(doc.can_undo(), Some("Edit text"), "edge drags still rewrap text");
    assert!(h.state().views[0].line_editor.is_none());
}

#[test]
fn arrow_keys_nudge_a_selected_image_instead_of_turning_the_page() {
    let mut h = selected_figure();
    let id = h.state().views[0].id;
    let rect = |h: &Harness<'static, PdfCraftApp>| h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let before = rect(&h);
    // As in Acrobat: an arrow moves it a point, Shift and an arrow ten.
    press(&mut h, Modifiers::NONE, Key::ArrowRight);
    press(&mut h, Modifiers::SHIFT, Key::ArrowDown);
    let moved = rect(&h);
    let near = |a: f64, b: f64| (a - b).abs() < 0.01;
    assert!(near(moved[0], before[0] + 1.0) && near(moved[1], before[1] - 10.0), "{before:?} → {moved:?}");
    assert!(near(moved[2] - moved[0], before[2] - before[0]), "moved, not resized");
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Move image"));
    assert_eq!(h.state().views[0].current, 0, "the arrows moved the image, not the page");
    assert!(h.state().views[0].image_selection.is_some(), "it stays selected for the next nudge");
}

/// #3632: with a picture selected, Alt+← / Alt+→ still go back and forward through the view
/// history. They don't nudge it (the nudge patterns also match a press with Alt), so nothing moves
/// and the document isn't edited.
#[test]
fn alt_arrows_navigate_instead_of_nudging_a_selected_image() {
    let mut h = selected_figure();
    let id = h.state().views[0].id;
    let rect = |h: &Harness<'static, PdfCraftApp>| h.state().session.get(id).unwrap().page_images(0)[0].rect;
    let before = rect(&h);
    // A followed link's jump to page 2 and back, as `go_to_dest` makes it.
    h.state_mut().views[0].go_to_page(1);
    h.run_steps(2);
    h.state_mut().views[0].go_to_page(0);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "still selected, so a nudge could take the keys");
    press(&mut h, Modifiers::ALT, Key::ArrowLeft);
    assert_eq!(h.state().views[0].current, 1, "Alt+← went back");
    press(&mut h, Modifiers::ALT, Key::ArrowRight);
    assert_eq!(h.state().views[0].current, 0, "Alt+→ went forward");
    // Each press is judged by its own modifiers, not the frame's: Alt let go in the same frame
    // as Alt+← leaves the frame with none, and the press still goes back.
    let arrow = |key, modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    h.event(arrow(Key::ArrowLeft, Modifiers::ALT));
    h.run_steps(4);
    assert_eq!(h.state().views[0].current, 1, "an Alt+← press went back");
    assert_eq!(rect(&h), before, "the picture didn't move");
    let doc = h.state().session.get(id).unwrap();
    assert!(!doc.dirty && doc.can_undo().is_none(), "nothing was edited");
    // And a plain → pressed before Alt goes down in the same frame still nudges.
    h.state_mut().views[0].go_to_page(0);
    h.run_steps(2);
    h.event(arrow(Key::ArrowRight, Modifiers::NONE));
    h.event(egui::Event::ModifiersChanged(Modifiers::ALT));
    h.run_steps(4);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
    let moved = rect(&h);
    assert!((moved[0] - before[0] - 1.0).abs() < 0.01, "a plain → nudged a point: {before:?} → {moved:?}");
}

#[test]
fn an_open_paragraph_moves_by_its_frame_and_by_the_arrow_keys() {
    let mut h = harness(2, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let block = |h: &Harness<'static, PdfCraftApp>| h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    // Just above the paragraph's frame: on the frame, off its text.
    let frame = |b: &pdfcraft_engine::TextBlock| screen((b.rect[0] + b.rect[2]) / 2.0, b.rect[3]) - egui::vec2(0.0, 4.0);
    let near = |a: f64, b: f64| (a - b).abs() < 0.01;

    let before = block(&h);
    click_at(&mut h, screen((before.rect[0] + before.rect[2]) / 2.0, (before.rect[1] + before.rect[3]) / 2.0));
    assert!(h.state().views[0].line_editor.is_some(), "a click on the text opens it for typing");
    // A click on its frame selects the box instead, and the arrows move it.
    click_at(&mut h, frame(&before));
    assert!(h.state().views[0].line_editor.is_none(), "the frame selects the box, it doesn't type");
    press(&mut h, Modifiers::NONE, Key::ArrowRight);
    press(&mut h, Modifiers::SHIFT, Key::ArrowUp);
    let nudged = block(&h);
    assert_eq!(nudged.text, "Page 1");
    assert!(near(nudged.rect[0], before.rect[0] + 1.0) && near(nudged.rect[1], before.rect[1] + 10.0), "{:?} → {:?}", before.rect, nudged.rect);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Move object"));
    assert_eq!(h.state().views[0].current, 0, "the arrows moved the paragraph, not the page");

    // Open it again and drag it by its frame: it follows the pointer.
    press(&mut h, Modifiers::NONE, Key::Escape);
    click_at(&mut h, screen((nudged.rect[0] + nudged.rect[2]) / 2.0, (nudged.rect[1] + nudged.rect[3]) / 2.0));
    assert!(h.state().views[0].line_editor.is_some());
    let from = frame(&nudged);
    drag(&mut h, from, from + egui::vec2(30.0 * k, -50.0 * k));
    assert!(h.state().views[0].line_editor.is_none());
    let dragged = block(&h);
    assert!(
        (dragged.rect[0] - nudged.rect[0] - 30.0).abs() < 1.5 && (dragged.rect[1] - nudged.rect[1] - 50.0).abs() < 1.5,
        "{:?} → {:?}",
        nudged.rect,
        dragged.rect
    );
    assert_eq!(h.state().session.get(id).unwrap().text_lines(0).len(), 1, "moved, not retyped");
}

/// The open paragraph's frame shows the Move cursor as far out as a press there takes hold of the
/// box: the cursor reached 4 px past the frame, the press 6.
#[test]
fn an_open_paragraphs_frame_shows_move_wherever_a_press_takes_the_box() {
    let mut h = harness(2, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    click_at(&mut h, screen((before.rect[0] + before.rect[2]) / 2.0, (before.rect[1] + before.rect[3]) / 2.0));
    assert!(h.state().views[0].line_editor.is_some(), "a click on the text opens it for typing");
    // 7 px above the text: 5 px past the frame drawn 2 px out.
    let far = screen((before.rect[0] + before.rect[2]) / 2.0, before.rect[3]) - egui::vec2(0.0, 7.0);
    h.hover_at(far);
    h.run_steps(3);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Move, "the frame offers to move the box there");
    click_at(&mut h, far);
    assert!(h.state().views[0].line_editor.is_none(), "and a click there selects the box");
}

/// The open paragraph's frame takes hold of the box on its sides too: there the edges of a closed
/// paragraph would rewrap it, but around the open one lies its frame.
#[test]
fn an_open_paragraphs_frame_takes_the_box_beside_its_text() {
    let mut h = harness(2, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    let middle = (before.rect[1] + before.rect[3]) / 2.0;
    click_at(&mut h, screen((before.rect[0] + before.rect[2]) / 2.0, middle));
    assert!(h.state().views[0].line_editor.is_some(), "a click on the text opens it for typing");
    // 6 px left of the text: 4 px past the frame drawn 2 px out.
    let beside = screen(before.rect[0], middle) - egui::vec2(6.0, 0.0);
    h.hover_at(beside);
    h.run_steps(3);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Move, "the frame offers to move the box there");
    click_at(&mut h, beside);
    assert!(h.state().views[0].line_editor.is_none(), "and a click there selects the box");
}

#[test]
fn ordinary_paragraph_drags_preserve_overlapping_text_and_history() {
    check_paragraph_overlap_drag(false, false);
}

#[test]
fn ordinary_paragraph_drags_preserve_identical_overlapping_labels() {
    check_paragraph_overlap_drag(true, false);
}

#[test]
fn ordinary_paragraph_drags_preserve_overlap_after_save_and_reopen() {
    check_paragraph_overlap_drag(false, true);
    check_paragraph_overlap_drag(true, true);
}

#[test]
fn ordinary_paragraph_drags_preserve_partial_overlap_and_touching_labels() {
    let content = "BT /F1 12 Tf 60 220 Td (Repeated) Tj ET BT /F1 12 Tf 60 320 Td (Repeated) Tj ET";
    for fraction in [0.25, 0.75, 1.0] {
        let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, 0, 1), 0);
        let mut expected = dragged_paragraphs(&h);
        assert_eq!(expected.len(), 2);
        let width = expected[1].rect[2] - expected[1].rect[0];
        // 75% overlap, 25% overlap, then exact touching of the original source rectangles.
        for offset in [[width * fraction, -100.0], [150.0, -40.0]] {
            drag_paragraph(&mut h, "Repeated", offset);
            for (i, coordinate) in expected[1].rect.iter_mut().enumerate() {
                *coordinate += offset[i % 2];
            }
            assert_dragged_paragraphs(&dragged_paragraphs(&h), &expected);
            assert_eq!(paragraph_drag_content(&h).matches("(Repeated)").count(), 2);
        }
    }
}

#[test]
fn ordinary_paragraph_drags_preserve_multiline_kerning_strings() {
    let content =
        concat!("BT /F1 12 Tf 60 220 Td (Keep here) Tj ET ", "BT /F1 12 Tf 14 TL 60 320 Td [(First) -120 ( line)] TJ T* (Second line) Tj ET");
    let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, 0, 1), 0);
    let mut expected = dragged_paragraphs(&h);
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[1].lines.len(), 2);
    let text = expected[1].text.clone();
    for offset in [[0.0, -100.0], [150.0, 0.0]] {
        drag_paragraph(&mut h, &text, offset);
        for (i, coordinate) in expected[1].rect.iter_mut().enumerate() {
            *coordinate += offset[i % 2];
        }
        assert_dragged_paragraphs(&dragged_paragraphs(&h), &expected);
        let current = paragraph_drag_content(&h);
        assert!(current.contains("[(First) -120 ( line)] TJ"), "original encoded strings and kerning stay byte-identical");
        for literal in ["(Keep here)", "(Second line)"] {
            assert_eq!(current.matches(literal).count(), 1);
        }
    }
}

#[test]
fn ordinary_paragraph_drag_respects_rotations_crop_and_user_unit() {
    let content = "BT /F1 12 Tf 60 220 Td (Keep here) Tj ET BT /F1 12 Tf 60 320 Td (Moving alpha) Tj ET";
    for rotation in [0, 90, 180, 270] {
        for view_rotation in [0, 90, 180, 270] {
            let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, rotation, 2), view_rotation);
            let mut expected = dragged_paragraphs(&h);
            let moving = expected.iter_mut().find(|b| b.text == "Moving alpha").unwrap();
            for (i, coordinate) in moving.rect.iter_mut().enumerate() {
                *coordinate += [35.0, -27.0][i % 2];
            }
            drag_paragraph(&mut h, "Moving alpha", [35.0, -27.0]);
            assert_dragged_paragraphs(&dragged_paragraphs(&h), &expected);
        }
    }
}

#[test]
fn an_ordinary_added_text_drag_keeps_its_content_edit() {
    for panel_open in [true, false] {
        check_added_text_drag(panel_open);
    }
}

#[test]
fn an_ordinary_paragraph_drag_refusal_never_rewrites_the_text() {
    let content = "q BT 4 Tr /F1 12 Tf 60 320 Td (Clipping text) Tj ET Q";
    let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, 0, 1), 0);
    let before = dragged_paragraphs(&h);
    let id = h.state().views[0].id;
    let bytes = h.state().session.get(id).unwrap().bytes.clone();
    let generation = h.state().session.get(id).unwrap().edit_generation();
    drag_paragraph(&mut h, "Clipping text", [30.0, -20.0]);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.bytes, bytes);
    assert_eq!(doc.edit_generation(), generation);
    assert!(doc.can_undo().is_none());
    assert!(!doc.dirty);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &before);
    assert!(h.state().toast.as_ref().unwrap().0.contains("Text used as a clipping path cannot be moved with this selection."));
}

/// A delete the engine refuses (text used as a clipping path) leaves the document as it was and
/// keeps the selection, so the user can still move or nudge what they had selected.
#[test]
fn a_refused_delete_keeps_the_object_selection() {
    let content = "q BT 4 Tr /F1 12 Tf 60 320 Td (Clipping text) Tj ET Q";
    let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, 0, 1), 0);
    let id = h.state().views[0].id;
    let (generation, bytes) = {
        let doc = h.state().session.get(id).unwrap();
        (doc.edit_generation(), doc.bytes.clone())
    };
    // An area selection from empty page space around the paragraph.
    let (center, _) = paragraph_drag_points(&h, "Clipping text", [0.0, 0.0]);
    drag(&mut h, center - egui::vec2(60.0, 25.0), center + egui::vec2(60.0, 25.0));
    h.get_by_label("1 object selected");
    h.key_press(Key::Delete);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!((doc.edit_generation(), &doc.bytes), (generation, &bytes), "the refused delete changed nothing");
    assert!(doc.can_undo().is_none());
    assert!(h.state().toast.is_some(), "the refusal is reported");
    h.get_by_label("1 object selected");
}

#[test]
fn an_ordinary_paragraph_drag_cannot_reuse_an_index_after_an_edit() {
    let content = "BT /F1 12 Tf 60 120 Td (Discard) Tj ET BT /F1 12 Tf 60 320 Td (Moving alpha) Tj ET BT /F1 12 Tf 60 220 Td (Keep here) Tj ET";
    let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, 0, 1), 0);
    let (from, to) = paragraph_drag_points(&h, "Moving alpha", [30.0, -20.0]);
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    h.hover_at(to);
    h.run_steps(2);
    assert!(h.state().views[0].block_drag.is_some());
    let id = h.state().views[0].id;
    h.state_mut()
        .session
        .apply(id, pdfcraft_engine::Edit::EditTextBlock { page: 0, block: 0, text: String::new(), style: Default::default() })
        .unwrap();
    let after_edit = dragged_paragraphs(&h);
    assert_eq!(after_edit.len(), 2, "the surviving paragraphs get different indexes");
    assert_eq!(after_edit[1].text, "Keep here", "the old target index now names an unrelated paragraph");
    let generation = h.state().session.get(id).unwrap().edit_generation();
    h.run_steps(2);
    h.drop_at(to);
    h.run_steps(4);
    assert!(h.state().views[0].block_drag.is_none());
    assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation, "the stale release adds no edit");
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &after_edit);
}

#[test]
fn a_quick_flick_on_either_edge_rewraps_the_paragraph() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let lines = |h: &Harness<'static, PdfCraftApp>| {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_lines(0)
    };
    let block = |h: &Harness<'static, PdfCraftApp>| {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0)[0].clone()
    };
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    let near = |a: f64, b: f64| (a - b).abs() < 1.5;
    // Grab edges near the box's top: a rewrapped box reaches the window's bottom, where the
    // tool's toast sits over the page and takes the press.
    let grip_y = |b: &pdfcraft_engine::TextBlock| b.rect[3] - 6.0;
    // Narrow "Page 1" from its right edge to about 45 pt: it rewraps onto two lines.
    let b = block(&h);
    let edge = screen(b.rect[2], grip_y(&b)) + egui::vec2(2.0, 0.0);
    flick(&mut h, edge, edge - egui::vec2((b.rect[2] - b.rect[0] - 45.0) as f32 * k, 0.0));
    let text: Vec<String> = lines(&h).iter().map(|l| l.text.clone()).collect();
    assert_eq!(text, ["Page", "1"], "narrowed from the right edge");
    // Flick the right edge outward, past the box and its handle: back onto one line.
    let b = block(&h);
    let edge = screen(b.rect[2], grip_y(&b)) + egui::vec2(2.0, 0.0);
    flick(&mut h, edge, edge + egui::vec2(80.0 * k, 0.0));
    let text: Vec<String> = lines(&h).iter().map(|l| l.text.clone()).collect();
    assert_eq!(text, ["Page 1"], "widened by a fast drag that left the handle at once");
    // Flick the left edge inward: it rewraps and its left side follows the pointer.
    let b = block(&h);
    let edge = screen(b.rect[0], grip_y(&b)) - egui::vec2(2.0, 0.0);
    let inward = (b.rect[2] - b.rect[0] - 45.0) as f32;
    flick(&mut h, edge, edge + egui::vec2(inward * k, 0.0));
    let after = lines(&h);
    let text: Vec<String> = after.iter().map(|l| l.text.clone()).collect();
    assert_eq!(text, ["Page", "1"], "narrowed from the left edge");
    assert!(near(after[0].rect[0], b.rect[0] + inward as f64), "{:?} → {:?}", b.rect, after[0].rect);
    assert!(h.state().views[0].line_editor.is_none());
}

/// #2276: a paragraph's rewrap handle is drawn half outside its box, and the resize pointer
/// shows a few pixels further out still. A press there started an area selection instead: its
/// rectangle followed the pointer, and on release the box went back to its old width.
#[test]
fn a_paragraph_rewraps_from_the_outer_half_of_its_edge_handles() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let lines = |h: &Harness<'static, PdfCraftApp>| {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_lines(0).iter().map(|l| l.text.clone()).collect::<Vec<_>>()
    };
    let block = |h: &Harness<'static, PdfCraftApp>| {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().text_blocks(0)[0].clone()
    };
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
    let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1.5);
    // The box is 2 px outside the text and its handle 7 px wide, centred on the box's edge: press
    // 3 px outside the box, on the handle. Near the box's top, clear of the tool's toast.
    let outside = 2.0 + 3.0;
    let before = block(&h);
    let grip = screen(before.rect[2], before.rect[3] - 6.0) + egui::vec2(outside, 0.0);
    drag(&mut h, grip, grip - egui::vec2((before.rect[2] - before.rect[0] - 45.0) as f32 * k, 0.0));
    assert_eq!(lines(&h), ["Page", "1"], "narrowed from the right handle");
    let narrowed = block(&h);
    assert!(narrowed.rect[2] < before.rect[2] - 20.0 && (narrowed.rect[0] - before.rect[0]).abs() < 1.5, "{:?} → {:?}", before.rect, narrowed.rect);
    let id = h.state().views[0].id;
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Edit text"));
    assert!(h.state().views[0].line_editor.is_none());
    // Saved and reopened, the paragraph keeps its new width.
    let saved = h.state_mut().session.save_bytes(id).unwrap();
    let mut reopen = pdfcraft_engine::Session::new();
    let rid = reopen.open("saved.pdf", None, saved, None).unwrap();
    assert!(near(reopen.get(rid).unwrap().text_blocks(0)[0].rect, narrowed.rect));
    // Undo gives the old width back, redo the new one.
    h.state_mut().undo();
    h.run_steps(2);
    assert_eq!(lines(&h), ["Page 1"]);
    assert!(near(block(&h).rect, before.rect), "{:?} → {:?}", before.rect, block(&h).rect);
    h.state_mut().redo();
    h.run_steps(2);
    assert!(near(block(&h).rect, narrowed.rect), "{:?} → {:?}", narrowed.rect, block(&h).rect);
    // The left handle's outer half too.
    h.state_mut().undo();
    h.run_steps(2);
    let grip = screen(before.rect[0], before.rect[3] - 6.0) - egui::vec2(outside, 0.0);
    let inward = (before.rect[2] - before.rect[0] - 45.0) as f32;
    drag(&mut h, grip, grip + egui::vec2(inward * k, 0.0));
    assert_eq!(lines(&h), ["Page", "1"], "narrowed from the left handle");
    assert!((block(&h).rect[0] - (before.rect[0] + f64::from(inward))).abs() < 1.5, "{:?} → {:?}", before.rect, block(&h).rect);
}

#[test]
fn pages_panel_command_click_picks_pages_without_moving_the_document() {
    let mut h = pages_panel(5);
    // The first ⌘-click on another page keeps the current page (1) selected too.
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(picked(&h), [0, 2]);
    assert_eq!(h.state().views[0].current, 0, "picking pages does not turn the page");
    h.get_by_label_contains("2 pages selected");
    h.get_by_label("Page 5").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(picked(&h), [0, 2, 4]);
    // ⌘-click again takes a page back out.
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(picked(&h), [0, 4]);
    // Page commands act on what is picked, as in the organize grid.
    assert_eq!(h.state().views[0].target_pages(), [0, 4]);
}

#[test]
fn pages_panel_shift_click_picks_a_range_and_a_plain_click_starts_over() {
    let mut h = pages_panel(6);
    // A plain click goes to the page and is the anchor of the next range.
    h.get_by_label("Page 2").click_modifiers(Modifiers::NONE);
    h.run_steps(2);
    assert_eq!(h.state().views[0].current, 1);
    assert!(picked(&h).is_empty());
    h.get_by_label("Page 4").click_modifiers(Modifiers::SHIFT);
    h.run_steps(2);
    assert_eq!(picked(&h), [1, 2, 3]);
    assert_eq!(h.state().views[0].current, 1);
    // A second ⇧-click ranges from the same anchor, in either direction.
    h.get_by_label("Page 1").click_modifiers(Modifiers::SHIFT);
    h.run_steps(2);
    assert_eq!(picked(&h), [0, 1]);
    // ⇧ with no earlier click ranges from the current page.
    h.get_by_label("Page 6").click_modifiers(Modifiers::NONE);
    h.run_steps(2);
    assert!(picked(&h).is_empty(), "a plain click drops the selection");
    assert_eq!(h.state().views[0].current, 5);
    h.get_by_label("Page 5").click_modifiers(Modifiers::SHIFT);
    h.run_steps(2);
    assert_eq!(picked(&h), [4, 5]);
}

#[test]
fn pages_panel_escape_clears_and_deleted_pages_leave_the_selection() {
    let mut h = pages_panel(5);
    h.get_by_label("Page 2").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(picked(&h), [0, 1]);
    // The pointer is still over the panel.
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(picked(&h).is_empty());
    // Pages that no longer exist drop out of the selection.
    h.get_by_label("Page 5").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(picked(&h), [0, 4]);
    h.state_mut().views[0].select_pages(&[3, 4]);
    assert!(h.state_mut().execute("page.delete"));
    h.run_steps(3);
    assert_eq!(page_texts(h.state()).len(), 3);
    assert!(picked(&h).iter().all(|p| *p < 3), "{:?}", picked(&h));
}

#[test]
fn print_shortcut_offers_the_pages_picked_in_the_pages_panel() {
    let mut h = pages_panel(5);
    h.get_by_label("Page 2").click_modifiers(Modifiers::NONE);
    h.run_steps(2);
    h.get_by_label("Page 4").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(picked(&h), [1, 3]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Print));
    assert_eq!(h.state().print_draft.which, pdfcraft_ui_egui::PrintWhich::Selected);
    assert_eq!(h.state().print_draft.selected, [1, 3]);
    h.get_by_label("Selected pages (2)");
    h.get_by_label("Sheet 1 of 2");
}

#[test]
fn dragging_pages_panel_thumbnails_reorders_pages() {
    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    let before = page_texts(h.state());
    // The Pages panel is on the right; the document view may label its pages too.
    let thumb = |h: &Harness<'static, PdfCraftApp>, label: &str| {
        h.get_all_by_label(label).map(|n| n.rect()).max_by(|a, b| a.left().total_cmp(&b.left())).expect("the thumbnail")
    };
    // Grab page 3 (page 4 is below the window) and drop it above page 1.
    let (from, to) = (thumb(&h, "Page 3").center(), thumb(&h, "Page 1").center_top() + egui::vec2(0.0, 4.0));
    drag(&mut h, from, to);
    h.run_steps(4);
    let after = page_texts(h.state());
    assert_eq!(after, vec![before[2].clone(), before[0].clone(), before[1].clone(), before[3].clone()], "{after:?}");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Move page"));
    let view = &h.state().views[0];
    assert_eq!(view.selected.iter().copied().collect::<Vec<_>>(), vec![0], "the moved page stays selected");
    assert!(view.panel_drag.is_none());
}

#[test]
fn resized_pages_panel_thumbnails_reorder_multiple_pages_and_undo() {
    let mut h = harness(4, |app| {
        app.right = Some(pdfcraft_ui_egui::RightPanel::Pages);
        app.set_option("thumbnail-zoom", "50").unwrap();
    });
    let before = page_texts(h.state());
    h.state_mut().views[0].select_pages(&[1, 2]);
    h.run_steps(2);
    let from = panel_thumb_rect(&h, "Page 2").center();
    let to = panel_thumb_rect(&h, "Page 1").center_top() + egui::vec2(0.0, 4.0);
    drag(&mut h, from, to);
    h.run_steps(4);
    assert_eq!(page_texts(h.state()), vec![before[1].clone(), before[2].clone(), before[0].clone(), before[3].clone()]);
    assert_eq!(h.state().views[0].target_pages(), vec![0, 1]);
    assert_eq!(h.state().views[0].thumbnail_zoom(), 0.5);
    assert!(h.state_mut().execute("edit.undo"));
    h.run_steps(4);
    assert_eq!(page_texts(h.state()), before);
    assert_eq!(h.state().views[0].thumbnail_zoom(), 0.5);
}

#[test]
fn a_thumbnail_drag_that_ends_off_the_panel_or_outlives_the_pages_is_dropped() {
    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    let before = page_texts(h.state());
    // A drag left over (the panel closed mid-drag, the button released elsewhere) moves nothing.
    h.state_mut().views[0].panel_drag = Some(vec![2]);
    h.run_steps(2);
    assert!(h.state().views[0].panel_drag.is_none());
    assert_eq!(page_texts(h.state()), before);
    // A change to the document drops page indexes taken before it.
    h.state_mut().views[0].panel_drag = Some(vec![3]);
    let id = h.state().views[0].id;
    let info = h.state().session.get(id).unwrap().info.clone();
    h.state_mut().views[0].document_changed(&info);
    assert!(h.state().views[0].panel_drag.is_none());
}

#[test]
fn deleting_pages_from_the_pages_panel_menu() {
    let mut h = pages_panel(4);
    let at = panel_thumb(&h, "Page 2");
    right_click_at(&mut h, at);
    h.get_by_label("Delete pages").click();
    h.run_steps(4);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 3", "Page 4"]);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Delete page"));
    assert!(h.state().views[0].selected.is_empty(), "the deleted pages leave the selection");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 2", "Page 3", "Page 4"]);
}

#[test]
fn the_delete_key_deletes_pages_picked_in_the_pages_panel_but_never_every_page() {
    let mut h = pages_panel(3);
    // ⌘-click picks pages 2 and 3 (and the current page, 1: the first ⌘-click keeps it).
    h.get_by_label("Page 2").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    let (p1, p3) = (panel_thumb(&h, "Page 1"), panel_thumb(&h, "Page 3"));
    h.hover_at(p3);
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected.iter().copied().collect::<Vec<_>>(), vec![0, 1, 2]);
    // Every page picked: Delete does nothing, and the menu item is off.
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()).len(), 3, "a document keeps at least one page");
    right_click_at(&mut h, p3);
    assert!(h.query_by(|n| n.label().as_deref() == Some("Delete pages") && n.is_disabled()).is_some());
    h.key_press(Key::Escape);
    h.run_steps(2);
    // Pages 2 and 3 picked (page 2 current): Delete deletes them, and page 1 becomes current.
    h.state_mut().views[0].select_pages(&[1, 2]);
    h.hover_at(p1);
    h.run_steps(1);
    h.key_press(Key::Delete);
    h.run_steps(4);
    assert_eq!(page_texts(h.state()), ["Page 1"]);
    assert_eq!(h.state().views[0].current, 0);
}

#[test]
fn host_dirty_hears_when_unsaved_work_appears_and_goes() {
    let reports = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = reports.clone();
    let mut h = harness(2, move |app| {
        app.set_option("organize", "on").unwrap();
        app.host_dirty = Some(Box::new(move |dirty| sink.lock().unwrap().push(dirty)));
    });
    assert_eq!(reports.lock().unwrap().last(), Some(&false), "a freshly opened document has nothing unsaved");
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    assert_eq!(reports.lock().unwrap().last(), Some(&true), "an edit is unsaved work");

    let out = temp_path("host-dirty.pdf");
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(3);
    assert_eq!(reports.lock().unwrap().last(), Some(&false), "saving clears it");
    let _ = std::fs::remove_file(out);
}

#[test]
fn one_editor_contains_mixed_styles_and_visual_rows() {
    let _gpu = gpu();
    let bytes = source_font_fixture_with_text(
        "BT 0 0.2 0.5 rg /F1 1 Tf 18 0 0 18 20 240 Tm (Heading) Tj ET \
         BT 0 g /F2 12 Tf 20 215 Td (Plain ) Tj /F1 12 Tf 0 0.2 0.5 rg (bold) Tj /F2 12 Tf 0 g ( words) Tj ET \
         BT /F2 10 Tf 20 195 Td (Last row) Tj ET",
    );
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("mixed.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let point = egui::pos2(page.left() + 30.0 / 300.0 * page.width(), page.top() + 55.0 / 300.0 * page.height());
    h.hover_at(point);
    h.run_steps(1);
    h.drag_at(point);
    h.run_steps(1);
    h.drop_at(point);
    h.run_steps(6);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, "Heading\nPlain bold words\nLast row");
    let galley = h
        .output()
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text().starts_with("Heading\n") => Some(&t.galley),
            _ => None,
        })
        .expect("one galley contains the whole region");
    assert_eq!(galley.rows.len(), 3);
    assert!(galley.job.sections.iter().any(|s| s.format.color == egui::Color32::BLACK));
    assert!(galley.job.sections.iter().any(|s| s.format.color == egui::Color32::from_rgb(0, 51, 128)));
    assert!(galley.job.sections.iter().any(|s| s.format.font_id.size > galley.job.sections.last().unwrap().format.font_id.size));
    let original_text = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.state_mut().views[0].line_editor.as_mut().unwrap().text.push_str("\nAdditional row\nAnother row");
    h.run_steps(3);
    let visible = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text().ends_with("Another row") => Some(t.galley.rect.translate(t.pos.to_vec2())),
            _ => None,
        })
        .unwrap();
    let frame = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.stroke.color == egui::Color32::from_rgb(0x14, 0x73, 0xE6) && r.rect.width() > 50.0 => Some(r.rect),
            _ => None,
        })
        .unwrap();
    assert!(frame.contains_rect(visible), "fallback font rows remain inside the growing frame");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = original_text;
    h.run_steps(3);
    if let Ok(path) = std::env::var("PDFCRAFT_RICH_FRAME_SHOT") {
        h.render().unwrap().save(path).unwrap();
    }
    h.state_mut().views[0].line_editor.as_mut().unwrap().text.clear();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(4);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().text_blocks(0).is_empty());
    assert!(dirty(&h));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().text_blocks(0)[0].rows.len(), 3);
}

#[test]
fn the_caret_on_a_small_row_is_as_tall_as_that_row() {
    // It was one row of the region's first (18 pt) size tall, reaching into the row below.
    let bytes = source_font_fixture_with_text(
        "BT 0 0.2 0.5 rg /F1 1 Tf 18 0 0 18 20 240 Tm (Heading) Tj ET \
         BT 0 g /F2 12 Tf 20 215 Td (Plain ) Tj /F1 12 Tf 0 0.2 0.5 rg (bold) Tj /F2 12 Tf 0 g ( words) Tj ET \
         BT /F2 10 Tf 20 195 Td (Last row) Tj ET",
    );
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("mixed.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let k = page.width() / 300.0;
    let screen = |x: f32, y: f32| egui::pos2(page.left() + x * k, page.top() + (300.0 - y) * k);
    let click = |h: &mut Harness<'static, PdfCraftApp>, at: egui::Pos2| {
        h.hover_at(at);
        h.run_steps(1);
        h.drag_at(at);
        h.run_steps(1);
        h.drop_at(at);
        h.run_steps(4);
    };
    click(&mut h, screen(30.0, 245.0));
    assert!(h.state().views[0].line_editor.is_some(), "the region opens");
    let row = h.state().session.get(h.state().views[0].id).unwrap().text_lines(0).into_iter().find(|l| l.text == "Last row").unwrap().rect;
    click(&mut h, screen(40.0, ((row[1] + row[3]) / 2.0) as f32));
    let row_height = (row[3] - row[1]) as f32 * k;
    let (left, right) = (screen(row[0] as f32, 0.0).x - 2.0, screen(row[2] as f32, 0.0).x + 2.0);
    let middle = screen(0.0, ((row[1] + row[3]) / 2.0) as f32).y;
    // The caret: a vertical segment within the row's span, through its middle.
    let caret = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::LineSegment { points: [a, b], .. }
                if (a.x - b.x).abs() < 0.01 && (left..right).contains(&a.x) && a.y.min(b.y) < middle && a.y.max(b.y) > middle =>
            {
                Some((b.y - a.y).abs())
            }
            _ => None,
        })
        .expect("a caret on the last row");
    assert!(caret > row_height * 0.8 && caret < row_height + 4.0, "caret {caret} px on a {row_height} px row");
}

/// Opt-in private-document regression. This never captures images or saves the source file.
/// Run with PDFCRAFT_TEXT_REGRESSION_PDF=/path/to/document.pdf and --ignored.
#[test]
#[ignore = "requires an explicitly supplied local PDF"]
fn private_document_regions_open_as_one_editor() {
    let path = std::env::var("PDFCRAFT_TEXT_REGRESSION_PDF").expect("supply a local PDF path");
    let bytes = std::fs::read(&path).unwrap();
    let original = bytes.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("private.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("view.layout.single"));
    h.run_steps(3);
    let page_count = h.state().session.get(h.state().views[0].id).unwrap().info.pages.len();
    let mut checked = 0;
    for page_index in 0..page_count {
        h.state_mut().views[0].go_to_page(page_index);
        h.run_steps(4);
        let blocks = h.state().session.get(h.state().views[0].id).unwrap().text_blocks(page_index);
        let page = h.state().views[0].page_screen_rect(page_index).unwrap();
        let info = h.state().session.get(h.state().views[0].id).unwrap().info.pages[page_index].clone();
        let source_lines = h.state().session.get(h.state().views[0].id).unwrap().text_lines(page_index);
        let programs = h.state().session.get(h.state().views[0].id).unwrap().text_editor_fonts(page_index);
        assert!(!programs.is_empty(), "source sfnt fonts must be usable in the editor");
        let scale = page.width() / info.width;
        for (index, block) in blocks.iter().enumerate().filter(|(_, b)| b.rows.len() >= 3) {
            let line = &source_lines[block.lines[0]];
            let v = info.user_to_view((line.rect[0] + 3.0) as f32, (line.rect[1] + line.rect[3]) as f32 / 2.0);
            let point = egui::pos2(page.left() + v[0] * scale, page.top() + v[1] * scale);
            h.hover_at(point);
            h.run_steps(1);
            h.drag_at(point);
            h.run_steps(1);
            h.drop_at(point);
            h.run_steps(8);
            let editor = h.state().views[0].line_editor.as_ref().unwrap();
            assert_eq!(editor.block, index);
            assert_eq!(editor.text, block.editor_text);
            assert!(!editor.overflow, "unchanged source region must fit (page {}, region {})", page_index + 1, index + 1);
            let text_shape = h
                .output()
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == block.editor_text => Some(t),
                    _ => None,
                })
                .expect("the whole source region is editable");
            for (section, (_, source)) in text_shape.galley.job.sections.iter().zip(block.edited_spans(&block.editor_text)) {
                if programs.iter().any(|font| font.resource == source_lines[source].font) {
                    assert!(
                        matches!(&section.format.font_id.family,
                        egui::FontFamily::Name(name) if name.starts_with("pdf-source-")),
                        "each available embedded source face is used in the editor"
                    );
                }
            }
            assert_eq!(text_shape.galley.rows.len(), block.rows.len(), "source rows stay in place");
            for (row, source_row) in text_shape.galley.rows.iter().zip(&block.rows) {
                let source = &source_lines[source_row[0]];
                let baseline = text_shape.pos.y + row.pos.y + row.glyphs[0].pos.y;
                let expected = page.top() + info.user_to_view(0.0, source.baseline()[1] as f32)[1] * scale;
                assert!((baseline - expected).abs() < 1.5, "editor baseline stays on the source baseline");
            }
            let frames = h
                .output()
                .shapes
                .iter()
                .filter(|s| {
                    matches!(&s.shape,
                egui::Shape::Rect(r) if r.stroke.color==egui::Color32::from_rgb(0x14,0x73,0xE6)
                    && r.rect.width()>50.0 && page.contains(r.rect.center()))
                })
                .count();
            assert_eq!(frames, 1, "one stable editor frame");
            h.state_mut().views[0].line_editor.as_mut().unwrap().text.push_str("\nAdditional text\nAdditional text");
            h.run_steps(3);
            let editor = h.state().views[0].line_editor.as_ref().unwrap();
            assert!(editor.overflow, "extra rows extend the block");
            let draft =
                h.state().session.get(h.state().views[0].id).unwrap().text_edit_layout(page_index, index, &editor.text, &editor.style()).unwrap();
            let bounds = draft.draft_rect.unwrap();
            assert!((bounds[3] - block.rect[3]).abs() < 0.5, "draft bounds exclude untouched neighboring regions");
            h.key_press(Key::Escape);
            h.run_steps(2);
            checked += 1;
        }
    }
    assert!(checked > 0, "the supplied PDF must contain a multiline region");
    assert!(!dirty(&h));
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn inline_editor_preserves_source_font_and_previews_live_bounds() {
    use pdfcraft_cos::{Dict, Document, Object, Stream, write_incremental};
    use skrifa::MetadataProvider as _;
    let _gpu = gpu();
    let mut cos = Document::open(std::sync::Arc::new(source_font_fixture_with_text(
        "BT /F1 18 Tf 20 220 Td (Stable source text) Tj ET BT /F1 12 Tf 20 194 Td (The second row stays here) Tj ET",
    )))
    .unwrap();
    let bytes = include_bytes!("../../../../../assets/fonts/Inter-Regular.ttf");
    let face = skrifa::FontRef::new(bytes).unwrap();
    let metrics = face.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
    let glyphs = face.glyph_metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
    let units = 1000.0 / f64::from(metrics.units_per_em);
    let program = cos.add(Object::Stream(Stream::flate(Dict::new(), bytes)));
    let mut descriptor = Dict::new();
    descriptor.set(b"FontFile2".to_vec(), Object::Ref(program));
    descriptor.set(b"Ascent".to_vec(), Object::Real(f64::from(metrics.ascent) * units));
    descriptor.set(b"Descent".to_vec(), Object::Real(f64::from(metrics.descent) * units));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("TrueType"));
    font.set(b"BaseFont".to_vec(), Object::name("Inter-Regular"));
    font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    font.set(b"FirstChar".to_vec(), Object::Int(32));
    font.set(b"LastChar".to_vec(), Object::Int(126));
    font.set(
        b"Widths".to_vec(),
        Object::Array(
            (32..=126u8)
                .map(|c| {
                    let id = face.charmap().map(char::from(c)).unwrap();
                    Object::Real(f64::from(glyphs.advance_width(id).unwrap()) * units)
                })
                .collect(),
        ),
    );
    font.set(b"FontDescriptor".to_vec(), Object::Dict(descriptor));
    cos.update_dict(pdfcraft_cos::ObjRef::new(3, 0), |d| *d = font).unwrap();
    let bytes = write_incremental(&cos, &Default::default()).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("stable.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let source = h.state().session.get(h.state().views[0].id).unwrap().text_blocks(0)[0].clone();
    let lines = h.state().session.get(h.state().views[0].id).unwrap().text_lines(0);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let scale = page.width() / 300.0;
    let point = egui::pos2(page.left() + 30.0 * scale, page.top() + 75.0 * scale);
    h.hover_at(point);
    h.run_steps(1);
    h.drag_at(point);
    h.run_steps(1);
    h.drop_at(point);
    h.run_steps(8);
    assert!(!h.state().views[0].line_editor.as_ref().unwrap().overflow);
    let shape = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == source.editor_text => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(shape.galley.rows.len(), 2);
    for (row, source_row) in shape.galley.rows.iter().zip(&source.rows) {
        let line = &lines[source_row[0]];
        let y = shape.pos.y + row.pos.y + row.glyphs[0].pos.y;
        assert!((y - (page.top() + (300.0 - line.baseline()[1] as f32) * scale)).abs() < 1.0);
    }
    for section in &shape.galley.job.sections {
        assert!(matches!(&section.format.font_id.family,egui::FontFamily::Name(name) if name.starts_with("pdf-source-")));
    }
    let frame = |h: &Harness<'_, PdfCraftApp>| {
        h.output()
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.stroke.color == egui::Color32::from_rgb(0x14, 0x73, 0xE6) && r.rect.width() > 100.0 => Some(r.rect),
                _ => None,
            })
            .unwrap()
    };
    let original_frame = frame(&h);
    h.state_mut().views[0].line_editor.as_mut().unwrap().text.push_str("\nExtra row that does not fit\nAnother row");
    h.run_steps(3);
    assert!(h.state().views[0].line_editor.as_ref().unwrap().overflow);
    let expanded = frame(&h);
    assert!(expanded.bottom() > original_frame.bottom() + 20.0, "the frame visibly expands around the extra rows");
    assert_eq!(expanded.left(), original_frame.left());
    assert_eq!(expanded.top(), original_frame.top());
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Stable source text".into();
    h.run_steps(3);
    let reduced = frame(&h);
    assert!(reduced.bottom() < original_frame.bottom() - 20.0, "deleting a row visibly shrinks the block");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text.push_str("\nExtra row that does not fit\nAnother row");
    h.run_steps(3);
    if let Ok(path) = std::env::var("PDFCRAFT_OVERFLOW_FRAME_SHOT") {
        h.render().unwrap().save(path).unwrap();
    }
    let id = h.state().views[0].id;
    let editor = h.state().views[0].line_editor.as_ref().unwrap();
    let draft_text = editor.text.clone();
    let measured = h.state().session.get(id).unwrap().text_edit_layout(0, 0, &draft_text, &editor.style()).unwrap().draft_rect.unwrap();
    let expected_frame = egui::Rect::from_min_max(
        egui::pos2(page.left() + measured[0] as f32 * scale, page.top() + (300.0 - measured[3] as f32) * scale),
        egui::pos2(page.left() + measured[2] as f32 * scale, page.top() + (300.0 - measured[1] as f32) * scale),
    )
    .expand(2.0);
    let live = frame(&h);
    assert!(
        (live.min - expected_frame.min).length() < 0.1 && (live.max - expected_frame.max).length() < 0.1,
        "the live frame shows the same PDF geometry as Apply/Save"
    );
    assert!(!dirty(&h), "draft measurement does not modify the document");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    h.run_steps(3);
    assert!(dirty(&h));
    assert_eq!(h.state().session.get(id).unwrap().text_blocks(0)[0].rect, measured);
    let bytes = h.state().session.save_bytes(id).unwrap();
    let mut reopened = pdfcraft_engine::Session::new();
    let reopened_id = reopened.open("again.pdf", None, bytes, None).unwrap();
    assert_eq!(reopened.get(reopened_id).unwrap().text_blocks(0)[0].rect, measured);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().text_blocks(0)[0], source);
}

#[test]
fn cropped_images_do_not_intercept_text_and_stay_inside_each_page() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("clipped.pdf", None, clipped_artwork_with_labels()).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("view.layout.single"));
    h.run_steps(3);
    for page in 0..2 {
        h.state_mut().views[0].go_to_page(page);
        h.run_steps(3);
        let r = h.state().views[0].page_screen_rect(page).unwrap();
        let at = |x: f32, y: f32| egui::pos2(r.left() + x / 200.0 * r.width(), r.top() + (300.0 - y) / 300.0 * r.height());
        for (x, y, label) in [(65.0, 134.0, "Editable caption"), (65.0, 224.0, "Overlay")] {
            let p = at(x, y);
            h.hover_at(p);
            h.run_steps(1);
            h.drag_at(p);
            h.run_steps(1);
            h.drop_at(p);
            h.run_steps(3);
            assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, label);
            assert!(h.state().views[0].image_selection.is_none(), "text wins even inside the visible artwork");
            h.key_press(Key::Escape);
            h.run_steps(2);
        }
        let p = at(95.0, 195.0);
        h.hover_at(p);
        h.run_steps(1);
        h.drag_at(p);
        h.run_steps(1);
        h.drop_at(p);
        h.run_steps(3);
        assert!(h.state().views[0].image_selection.as_ref().is_some_and(|s| s.page == page));
        for shape in &h.output().shapes {
            if let egui::Shape::Rect(rect) = &shape.shape
                && rect.stroke.color == egui::Color32::from_rgb(0x14, 0x73, 0xE6)
                && rect.rect.width() > 50.0
            {
                assert!(r.expand(3.0).contains_rect(rect.rect), "no image frame escapes its own page");
            }
        }
    }
    assert!(!dirty(&h));
}

#[test]
fn cropped_image_corner_resize_keeps_opposite_corner_and_scales_crop() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("clipped.pdf", None, clipped_artwork_with_labels_drawn(true)).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("view.layout.single"));
    h.run_steps(3);
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let at = |x: f32, y: f32| egui::pos2(page.left() + x / 200.0 * page.width(), page.top() + (300.0 - y) / 300.0 * page.height());
    let p = at(95.0, 195.0);
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
    let id = h.state().views[0].id;
    assert!(h.state().views[0].image_selection.is_some());
    let text = |doc: &pdfcraft_engine::Document| {
        doc.text_lines(0).into_iter().map(|l| (l.text.clone(), l.rect, l.font.clone(), l.size, l.color, l.baseline())).collect::<Vec<_>>()
    };
    let original_text = text(h.state().session.get(id).unwrap());
    // Grow then shrink using the visible top-right grip, never the off-page raw placement.
    for (factor, right, top, horizontal_only, off_centre) in [
        (1.15, 179.5, 262.0, false, false),
        (0.7, 134.65, 234.4, false, false),
        (0.75, 108.4875, 218.3, true, false),
        (1.1, 116.33625, 223.13, false, true),
    ] {
        let before = h.state().session.get(id).unwrap().page_images(0)[0].visible_rect.unwrap();
        let grip = at(before[2] as f32, before[3] as f32);
        let offset = if off_centre { egui::vec2(3.0, -3.0) } else { egui::Vec2::ZERO };
        let from = grip + offset;
        let fixed = at(before[0] as f32, before[1] as f32);
        let desired = fixed + (grip - fixed) * factor + offset;
        let to = if horizontal_only { egui::pos2(desired.x, from.y) } else { desired };
        let generation = h.state().session.get(id).unwrap().edit_generation();
        h.hover_at(from);
        h.run_steps(1);
        h.drag_at(from);
        h.run_steps(1);
        for k in 1..=5 {
            h.hover_at(from + (to - from) * (k as f32 / 5.0));
            h.run_steps(1);
        }
        assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation, "dragging previews without committing every frame");
        h.drop_at(to);
        h.run_steps(4);
        assert!(h.state().views[0].line_editor.is_none(), "a label under a selected image grip cannot take its drag");
        let after = h.state().session.get(id).unwrap().page_images(0)[0].visible_rect.unwrap();
        for (actual, expected) in after.into_iter().zip([30.0, 170.0, right, top]) {
            assert!((actual - expected).abs() < 0.05, "cropped image follows the visible grip: {actual} vs {expected}");
        }
        assert_eq!(text(h.state().session.get(id).unwrap()), original_text, "image resizing leaves captions in place");
    }
    let saved = h.state().session.save_bytes(id).unwrap();
    let mut reopened = pdfcraft_engine::Session::new();
    let re = reopened.open("again.pdf", None, saved, None).unwrap();
    assert_eq!(reopened.get(re).unwrap().page_images(0), h.state().session.get(id).unwrap().page_images(0));
}

/// While a corner is dragged, the image's pixels are drawn in the blue frame, not left at their
/// old size until release (#1671).
#[test]
fn image_pixels_follow_the_frame_while_a_corner_is_dragged() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("clipped.pdf", None, clipped_artwork_with_labels_drawn(true)).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].set_zoom(1.2);
    h.run_steps(4);
    // The dragged pixels are the page raster's: wait for it, as a slow runner may not have it yet
    // (the drag then drew no picture at all).
    assert!(
        crate::support::step_until(&mut h, |h| {
            h.run_steps(1);
            !h.state().render_pending()
        }),
        "the page raster arrives"
    );
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let at = |x: f32, y: f32| egui::pos2(page.left() + x / 200.0 * page.width(), page.top() + (300.0 - y) / 300.0 * page.height());
    let p = at(95.0, 195.0);
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(3);
    assert!(h.state().views[0].image_selection.is_some());
    let frame = |h: &Harness<'static, PdfCraftApp>| {
        h.output().shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.stroke.color == egui::Color32::from_rgb(0x14, 0x73, 0xE6) && r.rect.width() > 20.0 => Some(r.rect),
            _ => None,
        })
    };
    // Textured meshes smaller than the page: the page raster itself always covers the page.
    let pictures = |h: &Harness<'static, PdfCraftApp>| -> Vec<egui::Rect> {
        h.output()
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() => Some(m.calc_bounds()),
                _ => None,
            })
            .filter(|b| b.width() < page.width() - 1.0)
            .collect()
    };
    // The dragged picture is cut from the page raster, which a render thread delivers after
    // however many frames it takes (more on a slow runner): wait for it on screen, by state.
    let raster_shown = |h: &Harness<'static, PdfCraftApp>| {
        h.output().shapes.iter().any(|s| {
            matches!(&s.shape, egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default() && m.calc_bounds().width() >= page.width() - 1.0)
        })
    };
    let wait_for_raster = |h: &mut Harness<'static, PdfCraftApp>| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while h.state().render_pending() || !raster_shown(h) {
            assert!(std::time::Instant::now() < deadline, "timed out waiting for the page raster");
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    let start = frame(&h).expect("the selected image has a frame");
    let (grip, fixed) = (start.right_top(), start.left_bottom());
    for factor in [0.6, 1.3] {
        wait_for_raster(&mut h);
        let to = fixed + (grip - fixed) * factor;
        h.hover_at(grip);
        h.run_steps(1);
        h.drag_at(grip);
        h.run_steps(1);
        for k in 1..=4 {
            h.hover_at(grip + (to - grip) * (k as f32 / 4.0));
            h.run_steps(1);
        }
        let live = frame(&h).expect("the drag shows its frame");
        assert!((live.width() - start.width() * factor).abs() < 1.0, "the frame follows the grip: {live:?} from {start:?}");
        let drawn = pictures(&h);
        assert!(
            drawn.iter().any(|b| (b.min - live.min).length() < 0.5 && (b.max - live.max).length() < 0.5),
            "the picture is drawn in the frame {live:?}, not only at {drawn:?}"
        );
        h.drop_at(to);
        h.run_steps(4);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(4);
    }
}
