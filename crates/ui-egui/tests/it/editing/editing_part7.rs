//! Editing tests through the app UI (part 7 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn format_panel_shows_selection_size_and_updates_after_font_applied() {
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >> >>".into(),
    ];
    let body = "BT /F1 24 Tf 20 200 Td (Heading) Tj /F1 12 Tf 0 -25 Td (Paragraph) Tj ET";
    objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
    let mut pdf = Vec::from(b"%PDF-1.7\n");
    let mut xref = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        xref.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref_pos = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in xref {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n", objs.len() + 1).as_bytes());

    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("test.pdf", None, pdf).expect("fixture opens");
        app
    });
    h.run_steps(4);

    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 180.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);

    assert!(h.state().views[0].line_editor.is_some(), "editor opened");
    let text = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    assert!(text.contains("Paragraph"), "text is {text}");

    // Select the 12 pt paragraph text "Paragraph" via keyboard selection in the TextEdit
    h.key_press_modifiers(Modifiers::COMMAND, Key::ArrowRight);
    h.run_steps(1);
    for _ in 0.."Paragraph".len() {
        h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
        h.run_steps(1);
    }
    use egui::accesskit::Role;
    let size_node = h.get_by_role_and_label(Role::TextInput, "Font size:");
    assert_eq!(size_node.value().as_deref(), Some("12"), "size text input value is 12");

    let font_node = h.get_by_role_and_label(Role::ComboBox, "Font");
    assert_eq!(font_node.value().as_deref(), Some("Helvetica"), "initial font is Helvetica");

    // Apply Times to the selection
    font_node.click();
    h.run_steps(2);
    h.get_by_label("Times").click();
    h.run_steps(4);

    // Font combo box for the whole block is now Mixed because Heading is Helvetica and Paragraph is Times
    let font_node = h.get_by_role_and_label(Role::ComboBox, "Font");
    assert_eq!(font_node.value().as_deref(), Some("Mixed"), "whole block has mixed fonts");

    // Apply Times to the whole block
    font_node.click();
    h.run_steps(2);
    h.get_by_label("Times").click();
    h.run_steps(4);

    // Now all text is Times, so font combo box updates to Times
    let font_node = h.get_by_role_and_label(Role::ComboBox, "Font");
    assert_eq!(font_node.value().as_deref(), Some("Times"), "font combo box updates to Times");
}

/// Ctrl+Z after a formatting change, with the paragraph still open: the change is undone and
/// the editor closes. The box's own Undo used to take the key, putting back the text it had
/// before the change into a box laid out for the page after it, and the document kept the change.
#[test]
fn undo_with_the_paragraph_open_undoes_the_formatting() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    h.get_by_label("Bold").click();
    h.run_steps(4);
    {
        let s = h.state();
        let doc = s.session.get(s.views[0].id).unwrap();
        assert_eq!(doc.can_undo(), Some("Edit text"));
        assert_eq!(doc.text_blocks(0)[0].base_font, "Helvetica-Bold");
        assert!(s.views[0].line_editor.is_some(), "still editing");
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.can_undo(), None, "the formatting is undone");
    assert_eq!(doc.text_blocks(0)[0].base_font, "Helvetica");
    assert!(s.views[0].line_editor.is_none(), "the editor closed with the step it showed");
}

/// Undo while typing takes back the typing in the box first; the document is left alone.
#[test]
fn undo_while_typing_takes_back_the_typing_first() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    assert_ne!(h.state().views[0].line_editor.as_ref().unwrap().text, original);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    let s = h.state();
    let ed = s.views[0].line_editor.as_ref().expect("still typing");
    assert_eq!(ed.text, original, "the typing is taken back");
    assert_eq!(s.session.get(s.views[0].id).unwrap().can_undo(), None, "the document is untouched");
}

/// Existing-row alignment stays disabled until intent survives reopening; cancellation keeps all rows.
#[test]
fn selected_existing_rows_keep_alignment_disabled_and_the_complete_paragraph() {
    let mut h = three_row_paragraph();
    let before = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    let first = row_rect(&h, "One");
    let last = row_rect(&h, "nine");
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Home);
    h.run_steps(2);
    assert!(h.get_by_label("Alignment").accesskit_node().is_disabled());
    let button = h.get_by_label("Alignment").rect().center();
    click_at(&mut h, button);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, before);
    assert_eq!(row_rect(&h, "One"), first);
    assert_eq!(row_rect(&h, "nine"), last);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), None);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(row_rect(&h, "One"), first);
    assert_eq!(row_rect(&h, "nine"), last);
}

/// Clearing the selection restores ordinary whole-paragraph alignment and its undo step.
#[test]
fn whole_paragraph_alignment_stays_available_without_a_row_selection() {
    let mut h = three_row_paragraph();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Home);
    h.run_steps(2);
    assert!(h.get_by_label("Alignment").accesskit_node().is_disabled());
    h.key_press(Key::End);
    h.run_steps(2);
    assert!(!h.get_by_label("Alignment").accesskit_node().is_disabled());
    align_by_pointer(&mut h, "Align right");
    assert!(h.state().views[0].line_editor.is_some());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), None);
}

/// Redo after Undo in the box puts the typing back, in the box: it doesn't reach the document,
/// whose own Redo (a formatting change waiting here) must stay waiting.
#[test]
fn redo_after_undoing_typing_puts_the_typing_back() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    h.get_by_label("Bold").click();
    h.run_steps(4);
    // Undo the bold (closing the editor), so the document has it to redo.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_redo(), Some("Edit text"));
    open_paragraph(&mut h);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    let typed = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "the typing is taken back");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(3);
    let s = h.state();
    let ed = s.views[0].line_editor.as_ref().expect("still typing");
    assert_eq!(ed.text, typed, "Redo puts the typing back");
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!((doc.can_redo(), doc.text_blocks(0)[0].base_font.as_str()), (Some("Edit text"), "Helvetica"), "the document's Redo still waits");
}

/// Redo from the menu or toolbar with typing in the box the page doesn't have yet. Closing the
/// box first applied the typing, a new edit that dropped the document's waiting Redo, and the
/// Redo itself then failed: the step was gone. The typing now stays in the box and the Redo
/// waits; once Undo in the box takes the typing back, Redo from the menu puts it back there.
#[test]
fn redo_from_the_menu_with_typing_in_the_box_keeps_the_documents_redo() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    h.get_by_label("Bold").click();
    h.run_steps(4);
    // Undo the bold (closing the editor), so the document has it to redo.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    open_paragraph(&mut h);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    let typed = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    assert!(h.state_mut().execute("edit.redo"));
    h.run_steps(3);
    {
        let s = h.state();
        let ed = s.views[0].line_editor.as_ref().expect("the box stays open");
        assert_eq!(ed.text, typed, "the typing stays in the box, unapplied");
        let doc = s.session.get(s.views[0].id).unwrap();
        assert_eq!((doc.can_redo(), doc.text_blocks(0)[0].base_font.as_str()), (Some("Edit text"), "Helvetica"), "the document's Redo still waits");
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "Undo takes the typing back in the box");
    assert!(h.state_mut().execute("edit.redo"));
    h.run_steps(3);
    let s = h.state();
    assert_eq!(s.views[0].line_editor.as_ref().expect("still open").text, typed, "Redo from the menu puts the typing back");
    assert_eq!(s.session.get(s.views[0].id).unwrap().can_redo(), Some("Edit text"), "the document's Redo still waits");
}

/// Undo from the menu or toolbar with typing in the box the page doesn't have yet: the box takes
/// the typing back, as the key does, and stays open, and the document's waiting Redo stays. The
/// menu applied the typing first (a new edit, which dropped that Redo), closed the box and undid
/// the typing all at once.
#[test]
fn undo_from_the_menu_takes_back_typing_in_the_box() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    h.get_by_label("Bold").click();
    h.run_steps(4);
    // Undo the bold (closing the editor), so the document has it to redo and nothing to undo.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    open_paragraph(&mut h);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Home);
    h.run_steps(2);
    assert!(h.state().views[0].line_editor.as_ref().unwrap().selection.is_some());
    h.event(egui::Event::PointerMoved(egui::pos2(10.0, 10.0)));
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::unique("test-menu")));
    assert!(h.state_mut().execute("edit.undo"), "Undo is enabled for the box's typing");
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().selection, None, "menu history clears the old byte range before the next frame");
    h.run_steps(3);
    let s = h.state();
    let ed = s.views[0].line_editor.as_ref().expect("the box stays open");
    assert_eq!(ed.text, original, "the typing is taken back in the box");
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!((doc.can_redo(), doc.can_undo()), (Some("Edit text"), None), "the document is untouched");
}

/// Redo from the menu after Undo in the box took the typing back, with nothing for the document
/// to redo: the menu item follows the box's own Redo too, and puts the typing back. It was
/// disabled, as it followed only the document's Redo, and only the key could do it.
#[test]
fn redo_from_the_menu_is_enabled_for_the_boxs_own_typing() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    let typed = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "the typing is taken back");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_redo(), None, "the document has nothing to redo");
    // A range retained while the unfocused editor still showed its previous text.
    h.state_mut().views[0].line_editor.as_mut().unwrap().selection = Some(0..1);
    h.event(egui::Event::PointerMoved(egui::pos2(10.0, 10.0)));
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::unique("test-menu")));
    assert!(h.state_mut().execute("edit.redo"), "Redo is enabled for the box's own step");
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().selection, None, "menu history clears the old byte range before the next frame");
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().expect("still open").text, typed, "Redo puts the typing back");
}

/// Typing in tab A's paragraph, a paragraph opened in tab B, then back to A: Edit ▸ Undo takes
/// the typing back in A's box. The tabs share one text box, and its history was cleared each time
/// another tab's box took it, so Undo found no step and closed A's paragraph instead
/// (skylartaylor's and FredApps's review of 08ec1db7).
#[test]
fn undo_after_a_round_trip_through_another_tab_takes_back_the_typing() {
    let mut h = harness(1, |app| {
        app.open_bytes("copy.pdf", None, fixture(1)).expect("the copy opens");
    });
    let open_in = |h: &mut Harness<'static, PdfCraftApp>, tab: usize| {
        h.state_mut().active = Some(tab);
        h.run_steps(2);
        if h.state().views[tab].line_editor.is_none() {
            assert!(h.state_mut().execute("edit.edit_text"));
            h.run_steps(2);
        }
        let r = h.state().views[tab].page_screen_rect(0).expect("on screen");
        click_at(h, egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height()));
        assert!(h.state().views[tab].line_editor.is_some(), "the paragraph opens for typing");
    };
    open_in(&mut h, 0);
    h.get_by_label("Bold").click();
    h.run_steps(4);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    open_in(&mut h, 0);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    assert!(h.state().views[0].line_editor.as_ref().unwrap().text.ends_with(" more"));
    open_in(&mut h, 1);
    h.state_mut().active = Some(0);
    h.run_steps(3);
    assert!(h.state().views[0].line_editor.is_some(), "A's paragraph is still open after the round trip");
    assert!(h.state_mut().execute("edit.undo"), "Undo is enabled");
    h.run_steps(3);
    let s = h.state();
    let ed = s.views[0].line_editor.as_ref().expect("the box stays open: Undo takes back the typing");
    assert_eq!(ed.text, original, "the typing is taken back in the box");
    assert_eq!(s.session.get(s.views[0].id).unwrap().can_redo(), Some("Edit text"), "the document's Redo still waits");
}

/// Typing taken back with Ctrl+Z, then Bold applied with the paragraph still open: Ctrl+Shift+Z
/// must not bring the typing back on top of the bold, as the box's history is cleared once the
/// page has its text. The format left the text as it was, so the box kept its history and its
/// Redo put "Page 1 more" back (FredApps's review of abc378c0).
#[test]
fn redo_after_formatting_never_brings_back_typing_undone_before_it() {
    let mut h = harness(1, |_| {});
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    open_paragraph(&mut h);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "the typing is taken back");
    h.get_by_label("Bold").click();
    h.run_steps(4);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Edit text"), "the bold is applied");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(3);
    let text = h.state().views[0].line_editor.as_ref().map(|ed| ed.text.clone());
    assert_eq!(text.as_deref(), Some(original.as_str()), "the typing undone before the bold stays undone");
    // Nor through what the menu's Redo runs.
    h.state_mut().redo();
    h.run_steps(3);
    let s = h.state();
    let text = s.views[0].line_editor.as_ref().map(|ed| ed.text.clone());
    assert!(!text.as_deref().unwrap_or_default().contains("more"), "the menu's Redo brings it back: {text:?}");
    assert_eq!(s.session.get(s.views[0].id).unwrap().text_blocks(0)[0].base_font, "Helvetica-Bold", "the bold stays");
}

/// Edit ▸ Undo chosen in the same frame as the switch back to tab A, before A's box is drawn
/// again: the shared text box still holds tab B's history then. Stepping through it put B's
/// typing into A's box; the box now claims its own history back first (CodeRabbit on abc378c0).
#[test]
fn undo_in_the_frame_of_the_switch_back_takes_back_the_typing() {
    let mut h = harness(1, |app| {
        app.open_bytes("copy.pdf", None, fixture(1)).expect("the copy opens");
    });
    let open_in = |h: &mut Harness<'static, PdfCraftApp>, tab: usize| {
        h.state_mut().active = Some(tab);
        h.run_steps(2);
        if h.state().views[tab].line_editor.is_none() {
            assert!(h.state_mut().execute("edit.edit_text"));
            h.run_steps(2);
        }
        let r = h.state().views[tab].page_screen_rect(0).expect("on screen");
        click_at(h, egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height()));
        assert!(h.state().views[tab].line_editor.is_some(), "the paragraph opens for typing");
    };
    open_in(&mut h, 0);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" more".into()));
    h.run_steps(2);
    // In B, a paragraph whose text differs from A's ("Page 1 B", applied with Bold), so B's
    // history can't pass for A's.
    open_in(&mut h, 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" B".into()));
    h.run_steps(2);
    h.get_by_label("Bold").click();
    h.run_steps(4);
    assert_eq!(h.state().views[1].line_editor.as_ref().map(|ed| ed.text.as_str()), Some("Page 1 B"), "B's paragraph is open on its own text");
    // Back to A and Undo at once: no frame draws A's box in between.
    h.state_mut().active = Some(0);
    assert!(h.state_mut().execute("edit.undo"), "Undo is enabled");
    h.run_steps(3);
    let ed = h.state().views[0].line_editor.clone().expect("the box stays open: Undo takes back the typing");
    assert_eq!(ed.text, original, "the typing is taken back in A's box");
}

/// Redo from the menu chosen in the same frame as a switch to tab B, whose paragraph is open on
/// the same text as A's: the shared text box still holds A's history then, with A's undone
/// typing to redo, so B's box claims its own history back before stepping.
#[test]
fn redo_in_the_frame_of_a_tab_switch_never_brings_another_tabs_typing() {
    let mut h = harness(1, |app| {
        app.open_bytes("copy.pdf", None, fixture(1)).expect("the copy opens");
    });
    let open_in = |h: &mut Harness<'static, PdfCraftApp>, tab: usize| {
        h.state_mut().active = Some(tab);
        h.run_steps(2);
        assert!(h.state_mut().execute("edit.edit_text"));
        h.run_steps(2);
        let r = h.state().views[tab].page_screen_rect(0).expect("on screen");
        click_at(h, egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height()));
        assert!(h.state().views[tab].line_editor.is_some(), "the paragraph opens for typing");
    };
    open_in(&mut h, 1);
    open_in(&mut h, 0);
    let original = h.state().views[0].line_editor.as_ref().unwrap().text.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" secret".into()));
    h.run_steps(2);
    assert!(h.state_mut().execute("edit.undo"));
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "taken back in A");
    // To B and Redo at once: no frame draws B's box in between.
    h.state_mut().active = Some(1);
    h.state_mut().redo();
    h.run_steps(3);
    let s = h.state();
    let b = s.views[1].line_editor.as_ref().map(|ed| ed.text.clone());
    assert!(!b.as_deref().unwrap_or_default().contains("secret"), "A's typing came back in B: {b:?}");
    assert!(!s.session.get(s.views[1].id).unwrap().text_blocks(0)[0].text.contains("secret"), "A's typing reached B's page");
}

/// Two copies of a document, the same paragraph open in each. The paragraph boxes share one text
/// box, and so one Undo history: a box opened on the same text as another tab's didn't clear it.
/// Typing in one tab, taken back with Undo, then came back with Redo (the key or the menu) in the
/// other tab.
#[test]
fn typing_in_one_tab_never_reaches_another_tabs_paragraph() {
    let mut h = harness(1, |app| {
        app.open_bytes("copy.pdf", None, fixture(1)).expect("the copy opens");
    });
    let open_in = |h: &mut Harness<'static, PdfCraftApp>, tab: usize| {
        h.state_mut().active = Some(tab);
        h.run_steps(2);
        assert!(h.state_mut().execute("edit.edit_text"));
        h.run_steps(2);
        let r = h.state().views[tab].page_screen_rect(0).expect("on screen");
        click_at(h, egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height()));
        assert!(h.state().views[tab].line_editor.is_some(), "the paragraph opens for typing");
    };
    open_in(&mut h, 1);
    let original = h.state().views[1].line_editor.as_ref().unwrap().text.clone();
    open_in(&mut h, 0);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "the same paragraph");
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text(" secret".into()));
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().text, original, "taken back");
    // The copy, its paragraph still open: Redo there has nothing of its own to put back.
    h.state_mut().active = Some(1);
    h.run_steps(3);
    assert!(h.state().views[1].line_editor.is_some(), "the copy's paragraph is still open");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(3);
    let text = h.state().views[1].line_editor.as_ref().expect("the copy's paragraph stays open through Redo").text.clone();
    assert!(!text.contains("secret"), "the other tab's typing came in: {text:?}");
    // Nor through what the menu's Redo runs, which reads the box's history itself (the menu item
    // is off here, as the copy's document has nothing to redo). With no Redo of its own, the box
    // passes the step to the document and closes, so the page's text is checked too.
    h.state_mut().redo();
    h.run_steps(3);
    let s = h.state();
    let text = s.views[1].line_editor.as_ref().map(|ed| ed.text.clone());
    assert!(!text.as_deref().unwrap_or_default().contains("secret"), "the other tab's typing came in from the menu: {text:?}");
    assert_eq!(s.session.get(s.views[1].id).unwrap().text_blocks(0)[0].text, s.session.get(s.views[0].id).unwrap().text_blocks(0)[0].text);
}

/// A paragraph selected by its frame goes with one Delete (or Backspace), as in Acrobat, instead
/// of being emptied a typed character at a time, and undo brings it back.
#[test]
fn a_paragraph_selected_by_its_frame_deletes_with_one_key() {
    for key in [Key::Delete, Key::Backspace] {
        let mut h = harness(2, |_| {});
        assert!(h.state_mut().execute("edit.edit_text"));
        h.run_steps(2);
        let id = h.state().views[0].id;
        let blocks = |h: &Harness<'static, PdfCraftApp>| h.state().session.get(id).unwrap().text_blocks(0);
        let r = h.state().views[0].page_screen_rect(0).expect("on screen");
        let k = r.width() / 200.0;
        let screen = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k);
        let b = blocks(&h)[0].clone();
        click_at(&mut h, screen((b.rect[0] + b.rect[2]) / 2.0, (b.rect[1] + b.rect[3]) / 2.0));
        assert!(h.state().views[0].line_editor.is_some(), "a click on the text opens it for typing");
        click_at(&mut h, screen((b.rect[0] + b.rect[2]) / 2.0, b.rect[3]) - egui::vec2(0.0, 4.0));
        assert!(h.state().views[0].line_editor.is_none(), "the frame selects the box");
        press(&mut h, Modifiers::NONE, key);
        assert!(blocks(&h).is_empty(), "{key:?} deletes the whole paragraph: {:?}", blocks(&h));
        assert!(h.state().views[0].line_editor.is_none(), "no empty editor is left behind");
        assert!(h.query_all_by_label_contains("object selected").next().is_none(), "the deleted paragraph is no longer selected");
        assert_eq!(h.state().session.get(id).unwrap().text_blocks(1)[0].text, "Page 2", "the next page keeps its text");
        assert_eq!(h.state().session.get(id).unwrap().info.pages.len(), 2, "Delete removed the paragraph, not a page");
        assert!(h.state_mut().execute("edit.undo"));
        h.run_steps(2);
        assert_eq!(blocks(&h).iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["Page 1"], "undo restores it");
    }
}

#[test]
fn duplicate_xobject_at_different_rects_arrange_and_delete_keeps_other_instance_through_menu() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1400.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("dup.pdf", None, two_image_fixture()).expect("opens fixture");
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);

    let xf = h.state().views[0].page_xform(0).expect("page 0 xform");
    let imgs = h.state().session.get(id).unwrap().page_images(0);
    // B is at index 0 [100, 100, 120, 120], in the back
    let b0 = pdfcraft_ui_egui::canvas::image_screen_rect(&imgs[0], &xf, &h.state().session.get(id).unwrap().info, 0);
    let p_b = b0.center();

    // 1. Select B (image 0)
    h.hover_at(p_b);
    h.run_steps(1);
    h.drag_at(p_b);
    h.run_steps(1);
    h.drop_at(p_b);
    h.run_steps(2);
    assert_eq!(h.state().views[0].image_selection.as_ref().map(|s| s.index), Some(0), "image 0 (B) selected");

    // 2. Right-click B (image 0) to open context menu
    h.hover_at(p_b);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p_b, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: p_b, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    // 3. Open "Arrange image" submenu and click "Bring to front"
    h.get_by_label("Arrange image").click();
    h.run_steps(2);
    h.get_by_label("Bring to front").click();
    h.run_steps(3);

    // 4. Selection must be preserved after arrange: B is now at index 1 (in front)
    let sel = h.state().views[0].image_selection.as_ref().map(|s| s.index);
    assert_eq!(sel, Some(1), "selection must track B to index 1 after arrange");

    // 5. Right-click on the selected image (B at index 1) again to open context menu
    let sel_idx = sel.unwrap();
    let xf_again = h.state().views[0].page_xform(0).expect("page 0 xform");
    let cur_imgs = h.state().session.get(id).unwrap().page_images(0);
    let b_rect = pdfcraft_ui_egui::canvas::image_screen_rect(&cur_imgs[sel_idx], &xf_again, &h.state().session.get(id).unwrap().info, 0);
    let p_b2 = b_rect.center();
    h.hover_at(p_b2);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p_b2, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: p_b2, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    // 6. Click "Delete" in context menu
    h.get_by_label("Delete").click();
    h.run_steps(3);

    // 7. Save, reopen and verify: A remains at [20, 20, 40, 40], B is deleted
    let saved = h.state().session.save_bytes(id).unwrap();
    let mut reopened = pdfcraft_engine::Session::new();
    let rid = reopened.open("again.pdf", None, saved, None).unwrap();
    let imgs = reopened.get(rid).unwrap().page_images(0);
    assert_eq!(imgs.len(), 1, "exactly one image must remain after deleting B");
    assert!((imgs[0].rect[0] - 20.0).abs() < 1e-2, "remaining image must be A at [20, 20, 40, 40], was {:?}", imgs[0].rect);
}

/// Esc in an open paragraph leaves its text and selects its box, as in Acrobat, so Esc and
/// Delete remove it without aiming for the thin frame; the unapplied draft is discarded.
#[test]
fn escape_in_an_open_paragraph_selects_its_box_for_delete() {
    for draft in [None, Some("Page one")] {
        let mut h = harness(1, |_| {});
        assert!(h.state_mut().execute("edit.edit_text"));
        h.run_steps(2);
        let id = h.state().views[0].id;
        let blocks = |h: &Harness<'static, PdfCraftApp>| h.state().session.get(id).unwrap().text_blocks(0);
        let r = h.state().views[0].page_screen_rect(0).expect("on screen");
        let k = r.width() / 200.0;
        let b = blocks(&h)[0].clone();
        click_at(
            &mut h,
            egui::pos2(r.left() + ((b.rect[0] + b.rect[2]) / 2.0) as f32 * k, r.top() + (300.0 - (b.rect[1] + b.rect[3]) / 2.0) as f32 * k),
        );
        assert!(h.state().views[0].line_editor.is_some(), "a click inside the paragraph opens it for typing");
        if let Some(text) = draft {
            h.state_mut().views[0].line_editor.as_mut().unwrap().text = text.into();
            h.run_steps(1);
        }
        press(&mut h, Modifiers::NONE, Key::Escape);
        assert!(h.state().views[0].line_editor.is_none(), "Esc leaves the text");
        h.get_by_label("1 object selected");
        assert_eq!(blocks(&h)[0].text, "Page 1", "the draft {draft:?} is discarded");
        press(&mut h, Modifiers::NONE, Key::Delete);
        assert!(blocks(&h).is_empty(), "Esc then Delete removes the paragraph: {:?}", blocks(&h));
        // One undo step brings it back.
        assert!(h.state_mut().execute("edit.undo"));
        h.run_steps(2);
        assert_eq!(blocks(&h)[0].text, "Page 1");
    }
}

#[test]
fn outward_image_grip_drags_grow_on_rotated_pages() {
    for rotation in [0, 90, 180, 270] {
        for image_turn in [0.0, 25.0] {
            for grip in 0..8 {
                let mut png = Vec::new();
                image::RgbImage::from_pixel(100, 80, image::Rgb([100, 150, 200]))
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .unwrap();
                let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
                    let mut app = PdfCraftApp::new();
                    app.set_option("language", "en").unwrap();
                    app.open_bytes("grips.png", None, png.clone()).unwrap();
                    app
                });
                h.run_steps(4);
                h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
                    page: 0,
                    index: 0,
                    change: pdfcraft_engine::ImageEdit::Move([20.0, 20.0, 80.0, 60.0]),
                });
                h.state_mut().apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: rotation });
                if image_turn != 0.0 {
                    h.state_mut().apply_edit(pdfcraft_engine::Edit::EditPageImage {
                        page: 0,
                        index: 0,
                        change: pdfcraft_engine::ImageEdit::RotateBy(image_turn),
                    });
                }
                h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
                assert!(h.state_mut().execute("edit.edit_text"));
                h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection::at(0, 0));
                h.run_steps(3);
                let view = &h.state().views[0];
                let doc = h.state().session.get(view.id).unwrap();
                let before = doc.page_images(0)[0].clone();
                let obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(&before, &view.page_xform(0).unwrap(), &doc.info, 0);
                let (handle, start) = obb.handles()[grip];
                let opposite = obb.handles()[(grip + 4) % 8].0;
                let finish = start + (start - obb.screen_center) * 0.15;
                assert!(view.viewport_rect().contains(start) && view.viewport_rect().contains(finish), "test grips must be visible");
                h.hover_at(start);
                h.run_steps(1);
                h.drag_at(start);
                h.run_steps(1);
                let started = h.state().views[0].image_selection.clone();
                h.hover_at(finish);
                h.run_steps(2);
                let moved = h.state().views[0].image_selection.clone();
                h.drop_at(finish);
                h.run_steps(3);
                let view = &h.state().views[0];
                let doc = h.state().session.get(view.id).unwrap();
                let images = doc.page_images(0);
                let after = &images[0];
                let after_obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(after, &view.page_xform(0).unwrap(), &doc.info, 0);
                assert!(
                    after_obb.handle_pos(opposite).distance(obb.handle_pos(opposite)) < 1.0,
                    "fixed grip moved: rotation={rotation} image_turn={image_turn} grip={grip}"
                );
                assert!(
                    after_obb.handle_pos(handle).distance(finish) < 1.0,
                    "dragged grip missed pointer: rotation={rotation} image_turn={image_turn} grip={grip}"
                );
                let area = |m: [f64; 6]| (m[0] * m[3] - m[1] * m[2]).abs();
                assert!(
                    area(after.matrix) > area(before.matrix) * 1.05,
                    "outward grip shrank or ignored: rotation={rotation} image_turn={image_turn} grip={grip}: {:?} => {:?}; started={started:?} moved={moved:?}",
                    before.matrix,
                    after.matrix
                );
            }
        }
    }
}

#[test]
fn author_image_clip_is_not_offered_as_a_removable_editor_crop() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("author-clip.pdf", None, clipped_artwork_with_labels()).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection::at(0, 0));
    h.run_steps(3);
    let view = &h.state().views[0];
    let doc = h.state().session.get(view.id).unwrap();
    let image = doc.page_images(0)[0].clone();
    let visible = image.visible_rect.unwrap();
    let xf = view.page_xform(0).unwrap();
    let bounds = pdfcraft_ui_egui::canvas::image_screen_rect(&image, &xf, &doc.info, 0);
    let p = bounds.center();
    h.hover_at(p);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    assert!(h.query_all_by_label("Remove crop").next().is_none(), "author clips are not editor crops");
    h.get_by_label("Crop").click();
    h.run_steps(2);
    assert!(h.query_all_by_label("Remove Crop").next().is_none(), "the initial author clip has no removable draft crop");
    let mut draft = visible;
    draft[0] += (draft[2] - draft[0]) * 0.2;
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop.as_mut().unwrap().keep = draft;
    h.run_steps(3);
    h.get_by_label("Remove Crop").click();
    h.run_steps(1);
    assert_eq!(h.state().views[0].image_selection.as_ref().unwrap().crop.as_ref().unwrap().keep, visible, "draft reset preserves the author's clip");
    h.get_by_label("Done").click();
    h.run_steps(2);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().page_images(0)[0].visible_rect, Some(visible));
}

#[test]
fn affine_review_resize_out_and_back_preserves_saved_placement() {
    for (matrix, clip, rotation) in [
        ([100.0, 0.0, 0.0, 100.0, 100.0, 100.0], None, 0),
        ([100.0, 0.0, 50.0, 100.0, 100.0, 100.0], None, 0),
        ([86.60254, 50.0, -50.0, 86.60254, 150.0, 100.0], None, 0),
        ([100.0, 0.0, 0.0, 100.0, 100.0, 100.0], Some([100.0, 100.0, 150.0, 200.0]), 90),
    ] {
        for offset in [egui::Vec2::ZERO, egui::vec2(3.0, 4.0)] {
            let mut h = affine_review_harness(affine_review_image_fixture(matrix, clip, rotation));
            let view = &h.state().views[0];
            let id = view.id;
            let doc = h.state().session.get(id).unwrap();
            let original = doc.page_images(0)[0].matrix;
            let before = h.state().session.save_bytes(id).unwrap();
            let obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(&doc.page_images(0)[0], &view.page_xform(0).unwrap(), &doc.info, 0);
            let start = obb.handle_pos(pdfcraft_ui_egui::ImageHandle::TopRight) + offset;
            h.hover_at(start);
            h.run_steps(1);
            h.drag_at(start);
            h.run_steps(1);
            h.hover_at(start + egui::vec2(30.0, -25.0));
            h.run_steps(2);
            h.hover_at(start);
            h.run_steps(2);
            h.drop_at(start);
            h.run_steps(3);
            let saved = h.state().session.save_bytes(id).unwrap();
            assert!(saved == before, "out-and-back must not create an edit: matrix={matrix:?} clip={clip:?} rotation={rotation} offset={offset:?}");
            assert_eq!(h.state().session.get(id).unwrap().can_undo(), None);
            let mut reopened = pdfcraft_engine::Session::new();
            let reopened_id = reopened.open("saved.pdf", None, saved, None).unwrap();
            assert_eq!(reopened.get(reopened_id).unwrap().page_images(0)[0].matrix, original);
        }
    }
}

#[test]
fn affine_review_resize_uses_press_delta_and_affine_frame() {
    for (matrix, clip, rotation) in [
        ([100.0, 0.0, 0.0, 100.0, 100.0, 100.0], None, 0),
        ([100.0, 0.0, 50.0, 100.0, 100.0, 100.0], None, 0),
        ([86.60254, 50.0, -50.0, 86.60254, 150.0, 100.0], None, 0),
        ([100.0, 0.0, 0.0, 100.0, 100.0, 100.0], Some([100.0, 100.0, 150.0, 200.0]), 90),
    ] {
        let mut placements = Vec::new();
        for offset in [egui::Vec2::ZERO, egui::vec2(3.0, 4.0)] {
            let mut h = affine_review_harness(affine_review_image_fixture(matrix, clip, rotation));
            let view = &h.state().views[0];
            let id = view.id;
            let doc = h.state().session.get(id).unwrap();
            let xf = view.page_xform(0).unwrap();
            let obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(&doc.page_images(0)[0], &xf, &doc.info, 0);
            let grip = obb.handle_pos(pdfcraft_ui_egui::ImageHandle::TopRight);
            let start = grip + offset;
            let finish = start + (grip - obb.screen_center) * 0.2;
            h.hover_at(start);
            h.run_steps(1);
            h.drag_at(start);
            h.run_steps(1);
            h.hover_at(finish);
            h.run_steps(2);
            h.drop_at(finish);
            h.run_steps(3);
            let saved = h.state().session.save_bytes(id).unwrap();
            let mut reopened = pdfcraft_engine::Session::new();
            let reopened_id = reopened.open("saved.pdf", None, saved, None).unwrap();
            let after = reopened.get(reopened_id).unwrap().page_images(0)[0].clone();
            let after_obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(&after, &xf, &reopened.get(reopened_id).unwrap().info, 0);
            assert!(
                after_obb.handle_pos(pdfcraft_ui_egui::ImageHandle::BottomLeft).distance(obb.handle_pos(pdfcraft_ui_egui::ImageHandle::BottomLeft))
                    < 0.1,
                "opposite affine grip moved: {matrix:?} clip={clip:?} rotation={rotation}"
            );
            assert!(
                after_obb.handle_pos(pdfcraft_ui_egui::ImageHandle::TopRight).distance(grip + (grip - obb.screen_center) * 0.2) < 0.2,
                "resized affine frame missed the press-relative target: {matrix:?} clip={clip:?} rotation={rotation} offset={offset:?}"
            );
            placements.push(after.matrix);
        }
        assert!(placements[0].into_iter().zip(placements[1]).all(|(a, b)| (a - b).abs() < 1e-3), "press offset changed placement: {placements:?}");
    }
}

#[test]
fn affine_review_cropped_rotation_preview_matches_saved_corners() {
    let matrix = [70.710678, 70.710678, -70.710678, 70.710678, 170.710678, 100.0];
    for (clip, rotation) in [(None, 0), (Some([100.0, 170.0, 241.421356, 241.421356]), 0), (Some([100.0, 170.0, 241.421356, 241.421356]), 90)] {
        let mut h = affine_review_harness(affine_review_image_fixture(matrix, clip, rotation));
        let view = &h.state().views[0];
        let id = view.id;
        let doc = h.state().session.get(id).unwrap();
        let image = doc.page_images(0)[0].clone();
        let xf = view.page_xform(0).unwrap();
        let obb = pdfcraft_ui_egui::canvas::ImageObb::from_image(&image, &xf, &doc.info, 0);
        let visible = image.visible_rect.unwrap_or(image.rect);
        let p = doc.info.pages[0].user_to_view(((visible[0] + visible[2]) * 0.5) as f32, ((visible[1] + visible[3]) * 0.5) as f32);
        let centre = xf.norm_to_screen(p[0] / xf.pw, p[1] / xf.ph);
        let start = obb.s_rot;
        let v = start - centre;
        let finish = centre + egui::vec2(-v.y, v.x);
        h.hover_at(start);
        h.run_steps(1);
        h.drag_at(start);
        h.run_steps(1);
        h.hover_at(finish);
        h.run_steps(2);
        assert!(h.state().views[0].image_selection.as_ref().unwrap().turning);
        let preview = h
            .output()
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Path(path) if path.closed && path.points.len() == 4 && path.stroke.width == 1.5 => Some(path.points.clone()),
                _ => None,
            })
            .expect("rotation preview outline");
        if clip.is_some()
            && rotation == 0
            && let Ok(directory) = std::env::var("PDFCRAFT_AFFINE_SHOTS")
        {
            h.render().unwrap().save(std::path::Path::new(&directory).join("affine-rotation-preview.png")).unwrap();
        }
        h.drop_at(finish);
        h.run_steps(3);
        if clip.is_some()
            && rotation == 0
            && let Ok(directory) = std::env::var("PDFCRAFT_AFFINE_SHOTS")
        {
            h.render().unwrap().save(std::path::Path::new(&directory).join("affine-rotation-committed.png")).unwrap();
        }
        let saved = h.state().session.save_bytes(id).unwrap();
        let mut reopened = pdfcraft_engine::Session::new();
        let reopened_id = reopened.open("saved.pdf", None, saved, None).unwrap();
        let doc = reopened.get(reopened_id).unwrap();
        let after = doc.page_images(0)[0].clone();
        let committed = pdfcraft_ui_egui::canvas::ImageObb::from_matrix(after.matrix, &xf, &doc.info, 0).corners();
        let (cx, cy) = ((visible[0] + visible[2]) * 0.5, (visible[1] + visible[3]) * 0.5);
        let expected = pdfcraft_content::Matrix(image.matrix).then(&pdfcraft_content::Matrix([0.0, -1.0, 1.0, 0.0, cx - cy, cy + cx])).0;
        assert!(
            after.matrix.into_iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-3),
            "knob must commit the requested 90-degree turn around visible bounds"
        );
        assert!(
            preview.iter().zip(committed).all(|(a, b)| a.distance(b) < 1.0),
            "preview jumped after commit/save: clip={clip:?} rotation={rotation}: {preview:?} != {committed:?}"
        );
    }
}
