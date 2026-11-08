//! Editing tests through the app UI (part 1 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn organize_page_range_tracks_thumbnails_and_applies_typed_selection() {
    use egui::accesskit::Role;
    let _gpu = gpu();
    let mut h = organize(100);
    h.get_by_label("Select page 2").click();
    h.run_steps(2);
    assert_eq!(h.get_by_role_and_label(Role::TextInput, "Selected pages").value().as_deref(), Some("2"));
    h.get_by_label("Select page 3").click();
    h.run_steps(2);
    assert_eq!(h.get_by_role_and_label(Role::TextInput, "Selected pages").value().as_deref(), Some("2-3"));
    let old = h.state().views[0].selected.clone();
    type_selection(&mut h, "1,12,16-18,89,92,93,97,99");
    assert_eq!(h.state().views[0].selected, old, "typing is not committed until Enter or focus loss");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [0, 11, 15, 16, 17, 88, 91, 92, 96, 98]);
    assert_eq!(h.get_by_role_and_label(Role::TextInput, "Selected pages").value().as_deref(), Some("1, 12, 16-18, 89, 92-93, 97, 99"));
    if let Some(path) = std::env::var_os("PDFCRAFT_ORGANIZE_SNAPSHOT") {
        h.render().unwrap().save(std::path::Path::new(&path)).unwrap();
    }
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    for (i, page) in doc.info.pages.iter().enumerate() {
        assert_eq!(page.rotation, if h.state().views[0].selected.contains(&i) { 90 } else { 0 });
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().info.pages.iter().all(|p| p.rotation == 0));
}

#[test]
fn organize_page_range_rejects_invalid_input_and_escape_cancels_without_clearing() {
    use egui::accesskit::Role;
    let mut h = organize(5);
    h.state_mut().set_option("select", "2-3").unwrap();
    h.run_steps(2);
    for text in ["0", "6", "1,", "no"] {
        type_selection(&mut h, text);
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().views[0].target_pages(), [1, 2]);
    }
    type_selection(&mut h, "1-5");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [1, 2]);
    assert_eq!(h.get_by_role_and_label(Role::TextInput, "Selected pages").value().as_deref(), Some("2-3"));
    type_selection(&mut h, "5, 4, 4");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [3, 4]);
    type_selection(&mut h, " ");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().views[0].selected.is_empty());
    assert!(h.state_mut().set_option("select", "0").is_err());
    assert!(h.state_mut().set_option("select", "6").is_err());
}

#[test]
fn organize_tool_opens_the_overview_directly_and_can_be_reentered() {
    let mut h = harness(12, |_| {});
    assert!(!h.state().views[0].organize);
    h.get_by_label("Organize pages").click();
    h.run_steps(3);
    assert!(h.state().views[0].organize);
    h.get_by_label("Page 1");
    h.get_by_label("Page 5");
    h.get_by_label("Select page 2").click();
    h.run_steps(2);
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(!h.state().views[0].organize);
    h.state_mut().set_option("tool", "organize").unwrap();
    h.state_mut().set_option("tool", "organize").unwrap();
    h.run_steps(2);
    assert!(h.state().views[0].organize, "entering the tool is idempotent");
    assert_eq!(h.state().views[0].target_pages(), [1]);
    h.state_mut().set_option("tool", "edit").unwrap();
    h.run_steps(2);
    assert!(!h.state().views[0].organize, "another tool returns to the document");
}

/// Issue #1910: the page grid opens on the page the user was reading: it is selected and
/// scrolled into view, whether the document was on a middle page, its last page or a page
/// other than an earlier grid selection.
#[test]
fn organize_opens_on_the_current_page() {
    let mut h = harness(60, |_| {});
    let in_view = |h: &Harness<'static, PdfCraftApp>, page: usize| {
        let viewport = h.state().views[0].viewport_rect();
        let label = format!("Page {}", page + 1);
        h.query_by_label(&label).is_some_and(|n| viewport.contains_rect(n.rect()))
    };
    // From a middle page.
    h.state_mut().views[0].go_to_page(30);
    h.run_steps(2);
    assert!(h.state_mut().execute("page.organize"));
    h.run_steps(4);
    assert!(h.state().views[0].organize);
    assert_eq!(h.state().views[0].target_pages(), [30]);
    assert!(h.state().views[0].selected.is_empty(), "an empty selection is the current page, so checkboxes still toggle");
    assert!(in_view(&h, 30), "the current page is scrolled into view");
    assert!(!in_view(&h, 0), "the grid does not open on page 1");
    // An earlier grid selection on another page does not win over the page the user went to.
    h.get_by_label("Select page 32").click();
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [31]);
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(!h.state().views[0].organize);
    h.state_mut().views[0].go_to_page(45);
    h.run_steps(2);
    h.state_mut().set_option("tool", "organize").unwrap();
    h.run_steps(4);
    assert_eq!(h.state().views[0].target_pages(), [45]);
    assert!(in_view(&h, 45));
    assert!(!in_view(&h, 1));
    // From the last page.
    h.get_by_label("Close").click();
    h.run_steps(2);
    h.state_mut().views[0].go_to_page(59);
    h.run_steps(2);
    h.state_mut().set_option("organize", "on").unwrap();
    h.run_steps(4);
    assert_eq!(h.state().views[0].target_pages(), [59]);
    assert!(in_view(&h, 59));
    assert!(!in_view(&h, 0));
}

#[test]
fn organize_tool_chosen_before_opening_a_document_shows_its_pages() {
    let mut app = PdfCraftApp::new();
    app.set_option("tool", "organize").unwrap();
    app.open_bytes("doc.pdf", None, fixture(3)).unwrap();
    assert!(app.views[0].organize);
}

#[test]
fn organize_checkboxes_toggle_multiple_pages_without_modifiers() {
    let mut h = organize(4);
    h.get_by_label("Select page 1").click();
    h.run_steps(2);
    h.get_by_label("Select page 3").click();
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [0, 2]);
    h.get_by_label("Select page 1").click();
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [2]);
    h.get_by_label("Page 4").click_modifiers(Modifiers::SHIFT);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [0, 1, 2, 3], "checkbox toggles update the range anchor");
}

#[test]
fn organize_page_actions_rotate_only_the_named_page_and_undo() {
    let mut h = organize(3);
    h.get_by_label("Select page 1").click();
    h.run_steps(2);
    h.get_by_label("Select page 3").click();
    h.run_steps(2);
    h.get_by_label("Rotate page 1 clockwise").click();
    h.run_steps(3);
    let rotations = |h: &Harness<'static, PdfCraftApp>| {
        h.state().session.get(h.state().views[0].id).unwrap().info.pages.iter().map(|p| p.rotation).collect::<Vec<_>>()
    };
    assert_eq!(rotations(&h), [90, 0, 0]);
    assert_eq!(h.state().views[0].target_pages(), [0, 2], "a local action preserves the multi-selection");
    h.get_by_label("Rotate counterclockwise").click();
    h.run_steps(3);
    assert_eq!(rotations(&h), [0, 0, 270], "toolbar actions apply to the selection");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(rotations(&h), [90, 0, 0]);
    h.get_by_label("Rotate page 1 counterclockwise").click();
    h.run_steps(3);
    assert_eq!(rotations(&h), [0, 0, 0]);
}

#[test]
fn organize_hover_actions_delete_only_the_named_page_and_undo() {
    let mut h = organize(3);
    h.get_by_label("Page 2").hover();
    h.run_steps(2);
    h.get_by_label("Delete page 2").click();
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 3"]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 2", "Page 3"]);
    let single = organize(1);
    assert!(single.get_by_label("Delete page 1").accesskit_node().is_disabled());
}

#[test]
fn organize_local_actions_respect_document_permissions() {
    let mut h = harness(3, |app| {
        app.apply_edit(pdfcraft_engine::Edit::Protect(pdfcraft_engine::Protection {
            permissions_password: Some("owner".into()),
            changes: pdfcraft_engine::Changes::None,
            ..Default::default()
        }));
        let bytes = app.session.save_bytes(app.views[0].id).unwrap();
        app.open_bytes("restricted.pdf", None, bytes.as_ref().clone()).unwrap();
        app.set_option("tool", "organize").unwrap();
    });
    h.get_by_label("Select page 2").click();
    h.run_steps(2);
    assert!(h.get_by_label("Rotate page 2 clockwise").accesskit_node().is_disabled());
    assert!(h.get_by_label("Rotate page 2 counterclockwise").accesskit_node().is_disabled());
    assert!(h.get_by_label("Delete page 2").accesskit_node().is_disabled());
    let app = h.state();
    let view = &app.views[app.active.unwrap()];
    assert_eq!(view.target_pages(), [1]);
    assert!(!app.session.get(view.id).unwrap().dirty);
}

#[test]
fn organize_delete_then_undo_and_redo_with_keys() {
    let mut h = organize(3);
    h.get_by_label("Page 2").click();
    h.run_steps(2);
    h.get_by_label_contains("1 page selected");
    h.get_by_label("Delete pages (Delete)").click();
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 3"]);
    assert!(dirty(&h));
    h.get_by_label("doc.pdf (edited)"); // the tab shows unsaved changes

    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 2", "Page 3"]);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 3"]);
}

#[test]
fn shift_click_selects_a_range_and_moves_it() {
    let mut h = organize(4);
    h.get_by_label("Page 1").click();
    h.run_steps(1);
    h.get_by_label("Page 2").click_modifiers(Modifiers::SHIFT);
    h.run_steps(2);
    h.get_by_label_contains("2 pages selected");
    h.get_by_label("Move later").click();
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 3", "Page 1", "Page 2", "Page 4"]);
    // The moved pages stay selected at their new position, so repeated clicks keep moving them.
    assert_eq!(h.state().views[0].selected.iter().copied().collect::<Vec<_>>(), [1, 2]);
    h.get_by_label("Move later").click();
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 3", "Page 4", "Page 1", "Page 2"]);
}

#[test]
fn reverse_pages_reverses_the_selection_or_else_the_whole_document() {
    let mut h = organize(4);
    assert!(h.state_mut().execute("page.reverse"));
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 4", "Page 3", "Page 2", "Page 1"]);
    h.get_by_label("Page 1").click();
    h.run_steps(1);
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert!(h.state_mut().execute("page.reverse"));
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 2", "Page 3", "Page 4", "Page 1"]);
}

#[test]
fn command_click_toggles_and_rotation_applies_to_selection() {
    let mut h = organize(3);
    h.get_by_label("Page 1").click();
    h.run_steps(1);
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    let rot: Vec<u16> = {
        let app = h.state();
        app.session.get(app.views[0].id).unwrap().info.pages.iter().map(|p| p.rotation).collect()
    };
    assert_eq!(rot, [90, 0, 90]);
}

#[test]
fn organize_select_all_shortcut_preserves_current_page_and_sets_range_anchor() {
    let mut h = organize(4);
    h.get_by_label("Page 3").click();
    h.run_steps(1);
    // Automation may move the current selection without a click's range anchor.
    h.state_mut().views[0].select_pages(&[1]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.get_by_label_contains("4 pages selected");
    assert_eq!(h.state().views[0].target_pages(), [0, 1, 2, 3]);
    assert_eq!(h.state().views[0].current, 1, "selecting all keeps the reader's place");
    assert!(!dirty(&h), "selection doesn't edit the document");

    // Repeating Select all is harmless; deleting all pages still keeps the document intact.
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(page_texts(h.state()).len(), 4);
    assert!(!dirty(&h));
    h.get_by_label("Page 4").click_modifiers(Modifiers::SHIFT);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [1, 2, 3], "Shift-click extends from the current page");
    h.get_by_label("Page 2").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [2, 3], "command-click still toggles a page");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().views[0].selected.is_empty());
}

#[test]
fn organize_select_all_applies_operations_to_every_page() {
    let mut h = organize(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [0, 1, 2]);
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    let app = h.state();
    let rotations: Vec<_> = app.session.get(app.views[0].id).unwrap().info.pages.iter().map(|p| p.rotation).collect();
    assert_eq!(rotations, [90, 90, 90]);
}

#[test]
fn organize_select_all_works_without_page_editing_permission() {
    let mut h = harness(3, |app| {
        app.apply_edit(pdfcraft_engine::Edit::Protect(pdfcraft_engine::Protection {
            permissions_password: Some("owner".into()),
            changes: pdfcraft_engine::Changes::None,
            ..Default::default()
        }));
        let bytes = app.session.save_bytes(app.views[0].id).unwrap();
        app.open_bytes("restricted.pdf", None, bytes.as_ref().clone()).unwrap();
        app.set_option("organize", "on").unwrap();
    });
    let index = h.state().active.unwrap();
    assert!(!h.state().session.get(h.state().views[index].id).unwrap().allows_assembly());
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    assert_eq!(h.state().views[index].target_pages(), [0, 1, 2]);
    assert!(!h.state().session.get(h.state().views[index].id).unwrap().dirty);

    let mut h = organize(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    assert_eq!(h.state().views[0].target_pages(), [0]);
    assert_eq!(h.state().views[0].selected.len(), 1);

    // The opener refuses zero-page PDFs, but the view method also handles empty geometry.
    let mut empty = pdfcraft_ui_egui::canvas::DocView::new(pdfcraft_engine::DocId(0), &Default::default(), Default::default());
    empty.organize = true;
    assert!(!empty.select_all());
    assert!(empty.selected.is_empty());
}

#[test]
fn organize_select_all_leaves_focused_text_input_and_dialogs_alone() {
    let mut h = organize(3);
    h.query_all_by_value("1").next().expect("current page input").focus();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.query_all_by_value("1").next().expect("current page input").type_text("2");
    h.run_steps(2);
    assert_eq!(h.state().views[0].page_input, "2", "Ctrl/Cmd+A selected the field's text");
    assert!(h.state().views[0].selected.is_empty());
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().views[0].current, 1);

    h.state_mut().execute("help.about");
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    assert!(h.state().views[0].selected.is_empty(), "a modal dialog isolates the page selection");
    h.state_mut().dialog = None;
    h.state_mut().execute("view.palette");
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    assert!(h.state().views[0].selected.is_empty(), "the palette keeps its own keyboard input");
}

#[test]
fn delete_is_refused_when_it_would_remove_every_page() {
    let mut h = organize(2);
    h.state_mut().views[0].select_pages(&[0, 1]);
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(page_texts(h.state()).len(), 2, "a document keeps at least one page");
    assert!(!dirty(&h));
}

#[test]
fn insert_blank_page_after_selection() {
    let mut h = organize(2);
    h.get_by_label("Page 1").click();
    h.run_steps(1);
    insert_menu(&mut h, "Insert a blank page after the selection");
    h.run_steps(3);
    assert_eq!(page_texts(h.state()), ["Page 1", "", "Page 2"]);
    let app = h.state();
    let info = &app.session.get(app.views[0].id).unwrap().info;
    assert_eq!((info.pages[1].width, info.pages[1].height), (200.0, 300.0), "matches the neighbouring page");
}

#[test]
fn save_writes_an_incremental_update_and_clears_dirty() {
    let out = temp_path("saved.pdf");
    let original = fixture(3);
    let target = out.to_string_lossy().into_owned();
    let mut h = organize(3);
    h.state_mut().save_override = Some(target.clone());
    h.get_by_label("Page 3").click();
    h.run_steps(1);
    h.get_by_label("Rotate counterclockwise").click();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(3);
    let saved = std::fs::read(&out).expect("file written");
    assert_eq!(&saved[..original.len()], &original[..], "original bytes untouched");
    assert!(!dirty(&h));
    h.get_by_label("saved.pdf"); // the tab takes the new name, no edited marker
    // What was written is what the app now shows.
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().bytes.as_slice(), saved.as_slice());
    let reopened = pdfcraft_render::inspect(std::sync::Arc::new(saved), None).unwrap();
    assert_eq!(reopened.pages[2].rotation, 270);
    let _ = std::fs::remove_file(out);
}

#[test]
fn save_as_to_a_new_file_drops_earlier_revisions() {
    // #2165: Save As appended a revision, so a deleted page stayed in the new file's first one.
    let out = temp_path("save-as-clean.pdf");
    let mut h = organize(3);
    h.get_by_label("Page 2").hover();
    h.run_steps(2);
    h.get_by_label("Delete page 2").click();
    h.run_steps(3);
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    assert!(h.state_mut().save_active(pdfcraft_ui_egui::SaveTarget::As));
    let saved = std::fs::read(&out).expect("file written");
    let count = |needle: &[u8]| saved.windows(needle.len()).filter(|w| *w == needle).count();
    assert_eq!(count(b"%%EOF"), 1, "one revision only");
    assert_eq!((count(b"(Page 1)"), count(b"(Page 2)")), (1, 0), "the deleted page's content is gone");
    assert!(!dirty(&h));
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().bytes.as_slice(), saved.as_slice());
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 3"]);
    let _ = std::fs::remove_file(out);
}

#[test]
fn closing_a_dirty_tab_asks_and_cancel_keeps_it() {
    let mut h = organize(2);
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    h.get_by_label_contains("Save changes to “doc.pdf”");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert_eq!(h.state().views.len(), 1, "cancel keeps the document open");
    assert!(dirty(&h));
    assert!(h.state().close_request.is_none());

    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    h.get_by_label("Don't save").click();
    h.run_steps(3);
    assert!(h.state().views.is_empty(), "discarding closes the tab");
}

#[test]
fn closing_a_dirty_tab_can_save_first() {
    let out = temp_path("closed.pdf");
    let mut h = organize(2);
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    h.state_mut().request_close_tab(0);
    h.run_steps(2);
    h.get_by_label("Save").click();
    h.run_steps(3);
    assert!(h.state().views.is_empty());
    let saved = pdfcraft_render::inspect(std::sync::Arc::new(std::fs::read(&out).unwrap()), None).unwrap();
    assert_eq!(saved.pages[0].rotation, 90);
    let _ = std::fs::remove_file(out);
}

#[test]
fn the_save_prompt_answers_to_the_keyboard() {
    // Issue #8: Enter saves; ⌘D / Ctrl+D, Alt+D and Alt+N don't save; Escape cancels. While the
    // prompt is open, ⌘D is not Document properties.
    for (m, key) in [(Modifiers::COMMAND, Key::D), (Modifiers::ALT, Key::D), (Modifiers::ALT, Key::N)] {
        let mut h = organize(2);
        h.get_by_label("Rotate clockwise").click();
        h.run_steps(3);
        h.key_press_modifiers(Modifiers::COMMAND, Key::W);
        h.run_steps(3);
        h.get_by_label_contains("Save changes to “doc.pdf”");
        h.key_press_modifiers(m, key);
        h.run_steps(3);
        assert!(h.state().views.is_empty(), "{m:?}+{key:?} doesn't save and closes");
        assert!(h.state().dialog.is_none(), "{m:?}+{key:?} ran no command underneath");
    }
    let mut h = organize(2);
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().views.len(), 1, "Escape cancels");
    assert!(h.state().close_request.is_none());

    let out = temp_path("enter-saves.pdf");
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().views.is_empty(), "Enter saves and closes");
    let saved = pdfcraft_render::inspect(std::sync::Arc::new(std::fs::read(&out).unwrap()), None).unwrap();
    assert_eq!(saved.pages[0].rotation, 90);
    let _ = std::fs::remove_file(out);
}

#[test]
fn the_save_prompt_moves_between_buttons_with_the_keyboard() {
    let none = Modifiers::NONE;
    // Left from Save is Cancel: Enter or Space presses it, so nothing closes and nothing saves.
    for activate in [Key::Enter, Key::Space] {
        let h = press_after(&[(none, Key::ArrowLeft)], activate);
        assert_eq!(h.state().views.len(), 1, "{activate:?} on Cancel keeps the document");
        assert!(h.state().close_request.is_none());
        assert!(dirty(&h), "{activate:?} on Cancel doesn't save");
    }
    // Two left is Don't save, which closes without saving; Shift+Tab walks the same way.
    for keys in [
        vec![(none, Key::ArrowLeft), (none, Key::ArrowLeft)],
        vec![(Modifiers::SHIFT, Key::Tab), (Modifiers::SHIFT, Key::Tab)],
        vec![(none, Key::ArrowLeft), (none, Key::ArrowLeft), (none, Key::ArrowLeft)],
        vec![(none, Key::Tab)],
    ] {
        let h = press_after(&keys, Key::Enter);
        assert!(h.state().views.is_empty(), "{keys:?} reaches Don't save, which closes the tab");
    }
    // Tab from Save wraps to Don't save; arrows stop at the right end, so Save stays chosen.
    let out = temp_path("arrows-stop.pdf");
    let mut h = dirty_prompt();
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.key_press(Key::ArrowRight);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().views.is_empty(), "Right from Save stays on Save, which saves");
    assert!(out.exists());
    let _ = std::fs::remove_file(out);
}

#[test]
fn the_save_prompt_escape_still_cancels_whatever_is_chosen() {
    let h = press_after(&[(Modifiers::NONE, Key::ArrowLeft), (Modifiers::NONE, Key::ArrowLeft)], Key::Escape);
    assert_eq!(h.state().views.len(), 1, "Escape cancels even with Don't save chosen");
    assert!(dirty(&h));
}

#[test]
fn the_save_prompt_starts_on_save_every_time_it_opens() {
    // Move to Don't save, cancel with Escape, and ask again: the choice is back on Save.
    let out = temp_path("reopened.pdf");
    let mut h = dirty_prompt();
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.key_press(Key::ArrowLeft);
    h.key_press(Key::ArrowLeft);
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().views.len(), 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().views.is_empty(), "Enter saved and closed");
    assert!(out.exists(), "the second prompt opened on Save");
    let _ = std::fs::remove_file(out);
}

#[test]
fn keys_leave_the_document_under_a_dialog_alone() {
    // Issue #870: ⌘W with Preferences open closed the file underneath it, and in the page grid
    // Delete deleted the selected page. While a dialog is open its keys are its own.
    let mut h = organize(3);
    h.get_by_label("Page 2").click();
    h.run_steps(2);
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    assert!(h.state_mut().execute("app.preferences"));
    h.run_steps(3);
    let shift = Modifiers::COMMAND | Modifiers::SHIFT;
    for (m, key) in [
        (Modifiers::COMMAND, Key::W),
        (shift, Key::W),
        (Modifiers::COMMAND, Key::Z),
        (Modifiers::COMMAND, Key::D),
        (Modifiers::COMMAND, Key::F),
        (Modifiers::NONE, Key::Delete),
        (Modifiers::NONE, Key::Backspace),
    ] {
        h.key_press_modifiers(m, key);
        h.run_steps(3);
        let app = h.state();
        assert_eq!(app.views.len(), 1, "{m:?}+{key:?} closed the file");
        assert!(app.close_request.is_none(), "{m:?}+{key:?} asked to close the file");
        assert!(app.session.get(app.views[0].id).unwrap().can_undo().is_some(), "{m:?}+{key:?} undid the rotation");
        assert!(app.views[0].find.is_none(), "{m:?}+{key:?} opened Find");
        assert_eq!(app.dialog, Some(Dialog::Preferences), "{m:?}+{key:?} replaced Preferences");
    }
    assert_eq!(page_texts(h.state()), ["Page 1", "Page 2", "Page 3"], "a page was deleted under the dialog");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().dialog.is_none(), "Escape closes Preferences");
    assert_eq!(h.state().views[0].target_pages(), [1], "Escape went to the dialog, not the page selection");
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    assert!(h.state().close_request.is_some(), "with the dialog closed, ⌘W closes the file again");
}

#[test]
fn clean_tabs_close_without_asking() {
    let mut h = harness(1, |_| {});
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    assert!(h.state().views.is_empty());
    assert!(h.query_by_label("Don't save").is_none());
}

#[test]
fn quitting_with_unsaved_changes_asks_for_each_document() {
    let mut h = harness(2, |app| {
        app.open_bytes("second.pdf", None, fixture(1)).unwrap();
    });
    // Edit both documents.
    for tab in 0..2 {
        h.state_mut().active = Some(tab);
        h.state_mut().views[tab].select_pages(&[0]);
        h.state_mut().apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
    }
    h.state_mut().close_request = Some(CloseRequest::Quit);
    h.run_steps(3);
    h.get_by_label_contains("Save changes to “doc.pdf”");
    h.get_by_label("Don't save").click();
    h.run_steps(3);
    h.get_by_label_contains("Save changes to “second.pdf”");
    h.get_by_label("Don't save").click();
    h.run_steps(3);
    assert!(h.state().views.is_empty());
    assert!(h.state().close_request.is_none());
}

/// Quit from the Dock while another app is in front: the prompt opens in a window the user can't
/// see, so the Dock icon bounces (the taskbar button flashes) until they come back. A prompt that
/// opens in front asks for nothing.
#[test]
fn a_save_prompt_opened_in_the_background_asks_for_attention_until_the_user_returns() {
    use egui::UserAttentionType::{Critical, Reset};
    let mut h = harness(2, |_| {});
    h.state_mut().views[0].select_pages(&[0]);
    h.state_mut().apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
    h.input_mut().focused = false;
    close_window(&mut h);
    assert_eq!(attention_over(&mut h, 4), [Critical], "asked once, when the prompt opened");
    assert_eq!(h.state().close_request, Some(CloseRequest::Quit));
    h.input_mut().focused = true;
    assert_eq!(attention_over(&mut h, 3), [Reset], "coming back withdraws the request");
    h.get_by_label_contains("Save changes to “doc.pdf”");
    h.get_by_label("Cancel").click();
    assert!(attention_over(&mut h, 3).is_empty());
    assert_eq!(h.state().views.len(), 1, "cancel keeps the document open");
    // With PdfCraft in front, the prompt is in plain sight.
    close_window(&mut h);
    assert!(attention_over(&mut h, 4).is_empty());
    h.get_by_label_contains("Save changes to “doc.pdf”");
    // Leaving with the prompt open doesn't ask either: the user has seen it.
    h.input_mut().focused = false;
    assert!(attention_over(&mut h, 3).is_empty());
}

#[test]
fn the_last_session_reopens_its_files_pages_and_active_tab() {
    let dir = TempPdfs::new("reopen");
    let files = [dir.file("a.pdf", 1), dir.file("b.pdf", 5), dir.file("c.pdf", 2)];
    let opened = files.clone();
    let mut h = session_harness(move |app| {
        opened.iter().for_each(|f| {
            app.open_path(f);
        })
    });
    h.state_mut().views[1].go_to_page(3);
    h.state_mut().active = Some(1);
    h.run_steps(4);
    assert_eq!(h.state().views[1].current, 3, "b.pdf shows page 4");
    let settings = h.state().persist();
    let mut h = session_harness(move |app| {
        app.restore(&settings);
        app.reopen_last_files(&[]);
    });
    h.run_steps(4);
    let app = h.state();
    assert_eq!(open_paths(app), files, "the same files, in the same order");
    assert_eq!(app.active, Some(1), "b.pdf is in front again");
    assert_eq!(app.views[1].current, 3, "at the page it showed");
}

#[test]
fn quitting_with_unsaved_changes_still_remembers_every_file() {
    let dir = TempPdfs::new("quit");
    let files = [dir.file("a.pdf", 1), dir.file("b.pdf", 1)];
    let opened = files.clone();
    let mut h = session_harness(move |app| {
        opened.iter().for_each(|f| {
            app.open_path(f);
        })
    });
    for tab in 0..2 {
        h.state_mut().active = Some(tab);
        h.state_mut().views[tab].select_pages(&[0]);
        h.state_mut().apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
    }
    h.state_mut().active = Some(1);
    h.state_mut().close_request = Some(CloseRequest::Quit);
    h.run_steps(3);
    // Each unsaved tab closes as it is answered, before PdfCraft quits.
    for _ in 0..2 {
        h.get_by_label("Don't save").click();
        h.run_steps(3);
    }
    assert!(h.state().views.is_empty());
    let mut app = PdfCraftApp::new();
    app.restore(&h.state().persist());
    app.reopen_last_files(&[]);
    assert_eq!(open_paths(&app), files, "both files come back");
    assert_eq!(app.active, Some(1), "with b.pdf in front, as when the quit began");
}

#[test]
fn the_last_session_is_off_by_default_and_skips_missing_and_duplicate_files() {
    let dir = TempPdfs::new("skip");
    let (a, b) = (dir.file("a.pdf", 1), dir.file("b.pdf", 1));
    let mut app = PdfCraftApp::new();
    app.open_path(&a);
    app.open_path(&b);
    let off: serde_json::Value = serde_json::from_str(&app.persist()).unwrap();
    assert!(off["last_session"].is_null(), "nothing is kept while the preference is off");
    app.reopen_last_session = true;
    let settings = app.persist();
    let mut later = PdfCraftApp::new();
    later.restore(&settings);
    later.reopen_last_session = false;
    later.reopen_last_files(&[]);
    assert!(later.views.is_empty(), "turned off again: nothing reopens");
    // b.pdf is gone, and a.pdf is about to open from the command line.
    std::fs::remove_file(&b).unwrap();
    let mut later = PdfCraftApp::new();
    later.restore(&settings);
    later.reopen_last_files(&[a]);
    assert!(later.views.is_empty(), "neither opens from the session");
}

#[test]
fn opening_the_same_path_again_focuses_its_tab() {
    let pdfs = TempPdfs::new("dedup-open");
    let a = pdfs.file("a.pdf", 1);
    let b = pdfs.file("b.pdf", 1);
    // Inline opens: this pins the synchronous open contract (open vs focus). A live app
    // schedules the background worker instead and answers `Pending` until it completes.
    let mut h = session_harness(|app| app.run_inline = true);
    assert_eq!(h.state_mut().open_path(&a), OpenOutcome::Opened);
    assert_eq!(h.state_mut().open_path(&b), OpenOutcome::Opened);
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 2, "two different files open two tabs");
    assert_eq!(h.state_mut().open_path(&a), OpenOutcome::Focused, "opening an open file focuses, it doesn't reopen");
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 2, "opening an open file again adds no tab");
    assert_eq!(h.state().active, Some(0), "the existing tab comes to the front");
    assert_eq!(open_paths(h.state()), vec![a, b], "the tab order is untouched");
}

#[test]
fn opening_a_missing_file_fails_and_tells_the_user() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    let missing = std::env::temp_dir().join(format!("pdfcraft-no-such-file-{}.pdf", std::process::id()));
    assert_eq!(app.open_path(&missing.to_string_lossy()), OpenOutcome::Failed);
    assert!(app.views.is_empty());
    assert!(app.toast.is_some(), "the failure is still reported to the user");
}

#[test]
fn opening_the_same_file_with_a_dot_component_focuses_its_tab() {
    let pdfs = TempPdfs::new("dedup-dot");
    let a = pdfs.file("a.pdf", 1);
    let dotted = format!("{}/./a.pdf", pdfs.0.display());
    let mut h = session_harness(|app| app.run_inline = true);
    h.state_mut().open_path(&a);
    h.state_mut().open_path(&dotted);
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 1, "another spelling of the same file matches");
    assert_eq!(h.state().active, Some(0));
}

#[test]
fn opening_a_file_whose_copy_vanished_from_disk_focuses_its_tab() {
    let pdfs = TempPdfs::new("dedup-vanished");
    let a = pdfs.file("a.pdf", 1);
    let mut h = session_harness(|_| {});
    h.state_mut().open_path(&a);
    // A live window opens through the background worker; the tab has to exist before the file
    // is deleted, or the worker itself fails to read what this test is about to remove.
    support::wait_until(&mut h, "the vanished file's tab", |app| app.views.len() == 1);
    std::fs::remove_file(&a).unwrap();
    h.state_mut().open_path(&a);
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 1, "no second tab");
    assert_eq!(h.state().active, Some(0), "the open tab is focused");
    assert!(h.state().toast.is_none(), "no read error: the file is never read again");
}

#[test]
fn opening_an_encrypted_document_twice_keeps_one_prompt_and_one_tab() {
    let pdfs = TempPdfs::new("dedup-encrypted");
    let secret = pdfs.0.join("secret.pdf");
    std::fs::write(&secret, protected("pw", "owner", -1)).unwrap();
    let s = secret.to_string_lossy().into_owned();
    let mut h = session_harness(|app| app.run_inline = true);
    h.state_mut().open_path(&s);
    h.state_mut().open_path(&s);
    h.run_steps(2);
    assert!(h.state().views.is_empty(), "still locked");
    let field = h.get_by_role(Role::PasswordInput);
    field.focus();
    field.type_text("pw");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert_eq!(h.state().views.len(), 1, "unlocking opens one tab, not two");
}

#[test]
fn opening_documents_without_a_path_still_open_twice() {
    let bytes = fixture(1);
    let mut h = session_harness(|_| {});
    h.state_mut().open_bytes("copy.pdf", None, bytes.clone()).unwrap();
    h.state_mut().open_bytes("copy.pdf", None, bytes).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().views.len(), 2, "bytes have no path to compare (attachments, conversions)");
}

#[test]
fn save_prompt_stays_inside_the_screen_for_a_long_filename() {
    // Issue #161: an unwrapped title carrying a long filename widened the centered modal past
    // the viewport, clipping the message and pushing the Save/Cancel buttons off-screen.
    let name = "Psychology_ The Science of Mind and Behaviour, -- Nigel Holt, Andy Bremner, Michael \
                Vliek, Ed Sutherland, -- 5, 2024 -- McGraw-Hill Education (UK) Ltd -- isbn13 97815268.pdf";
    let mut h = Harness::builder().with_size(egui::vec2(1365.0, 719.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes(name, None, fixture(1)).expect("fixture opens");
        app.close_request = Some(CloseRequest::Tab(app.views[0].id));
        app
    });
    h.run_steps(4);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1365.0, 719.0));
    let inside = |r: egui::Rect| screen.contains(r.min) && screen.contains(r.max);
    let title = h.get_by_label_contains("Save changes to");
    let title_rect = title.rect();
    assert!(inside(title_rect), "the title rect {title_rect:?} leaves the screen");
    for button in ["Save", "Cancel", "Don't save"] {
        let rect = h.get_by_label(button).rect();
        assert!(inside(rect), "the {button} button rect {rect:?} leaves the screen");
    }
}

#[test]
fn save_prompt_fits_the_smallest_window_whatever_the_name() {
    // Issue #236: wrapping (#161) kept the prompt narrow, but a long enough name still grew it
    // taller than the window and pushed the buttons off-screen. On the web a `?file=` URL names
    // the document, so the name has no length limit. Long names now give way in the middle.
    let names = [
        format!("{}.pdf", "a".repeat(400)),
        format!("{}.pdf", "Quarterly_Report_FY2026_Final_v3_".repeat(12)),
        format!("{}.pdf", "รายงานประจำปีงบประมาณ".repeat(15)),
        format!("{}.pdf", "年".repeat(251)),
        format!("{}.pdf", "W".repeat(2000)),
    ];
    // The desktop window's minimum inner size (apps/pdfcraft/src/main.rs).
    let size = egui::vec2(820.0, 520.0);
    for name in names {
        let start: String = name.chars().take(10).collect();
        if support::skip_without_glyphs(&format!("save_prompt_fits_the_smallest_window_whatever_the_name ({start}…)"), "en", &name) {
            continue;
        }
        let mut h = Harness::builder().with_size(size).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes(&name, None, fixture(1)).expect("fixture opens");
            app.close_request = Some(CloseRequest::Tab(app.views[0].id));
            app
        });
        h.run_steps(4);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let inside = |r: egui::Rect| screen.contains(r.min) && screen.contains(r.max);
        let title = h.get_by_label_contains("Save changes to");
        let title_rect = title.rect();
        assert!(inside(title_rect), "{start}…: the title rect {title_rect:?} leaves the screen");
        let opening = format!("Save changes to “{start}");
        assert!(
            h.query_by(|n| n.value().is_some_and(|l| l.starts_with(&opening) && l.contains('…') && l.contains(".pdf” before closing?"))).is_some(),
            "{start}…: the title keeps the start and the extension"
        );
        for button in ["Save", "Cancel", "Don't save"] {
            let rect = h.get_by_label(button).rect();
            assert!(inside(rect), "{start}…: the {button} button rect {rect:?} leaves the screen");
        }
    }
}

#[test]
fn document_properties_edit_is_one_undoable_step() {
    let mut h = harness(1, |app| app.set_option("dialog", "properties").unwrap());
    let title = h.get_by_role_and_label(Role::TextInput, "Title");
    title.focus();
    title.type_text("Annual report");
    h.run_steps(2);
    let author = h.get_by_role_and_label(Role::TextInput, "Author");
    author.focus();
    author.type_text("Finance team");
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.info.title.as_deref(), Some("Annual report"), "inspection sees the new title");
    assert_eq!(doc.info_value("Author").as_deref(), Some("Finance team"));
    assert_eq!(doc.can_undo(), Some("Change document properties"));
    assert!(app.dialog.is_none());

    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.info.title, None);
    assert_eq!(doc.info_value("Author"), None);
}

#[test]
fn cancelling_properties_discards_the_draft() {
    let mut h = harness(1, |app| app.set_option("dialog", "properties").unwrap());
    let title = h.get_by_role_and_label(Role::TextInput, "Title");
    title.focus();
    title.type_text("Draft");
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(!dirty(&h));
    assert!(h.state().props_draft.is_none());
}

#[test]
fn custom_properties_are_added_in_the_custom_tab_as_one_undoable_step() {
    let mut h = harness(1, |app| app.set_option("dialog", "custom").unwrap());
    h.get_by_label("None");
    type_into(&mut h, "Name:", "Project");
    type_into(&mut h, "Value:", "Apollo");
    h.get_by_role_and_label(Role::Button, "Add").click();
    h.run_steps(3);
    assert!(custom_properties(&h).is_empty(), "nothing is written before OK");
    h.get_by_label("Project");
    type_into(&mut h, "Name:", "Revision");
    type_into(&mut h, "Value:", "4");
    h.get_by_role_and_label(Role::Button, "Add").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(custom_properties(&h), [("Project".to_string(), "Apollo".to_string()), ("Revision".to_string(), "4".to_string())]);
    let app = h.state();
    assert_eq!(app.session.get(app.views[0].id).unwrap().can_undo(), Some("Change document properties"));
    assert!(app.dialog.is_none() && app.custom_draft.is_none());

    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert!(custom_properties(&h).is_empty());
}

#[test]
fn custom_properties_are_changed_and_deleted_in_the_custom_tab() {
    let mut h = harness(1, |app| {
        for (key, value) in [("Project", "Apollo"), ("Revision", "4")] {
            assert!(app.apply_edit(pdfcraft_engine::Edit::SetInfo { key: key.into(), value: value.into() }));
        }
        app.set_option("dialog", "custom").unwrap();
    });
    h.get_by_label("Revision").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Delete").click();
    h.run_steps(3);
    h.get_by_label("Project").click();
    h.run_steps(3);
    assert!(h.get_by_role_and_label(Role::Button, "Update").accesskit_node().is_disabled(), "the value is unchanged");
    clear(&mut h, "Value:");
    type_into(&mut h, "Value:", "Apollo 2");
    h.get_by_role_and_label(Role::Button, "Update").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(custom_properties(&h), [("Project".to_string(), "Apollo 2".to_string())]);
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.info_value("Revision"), None);
    assert_eq!(doc.can_undo(), Some("Change document properties"));
}

#[test]
fn the_custom_tab_refuses_names_a_custom_property_cannot_have() {
    let mut h = harness(1, |app| {
        assert!(app.apply_edit(pdfcraft_engine::Edit::SetInfo { key: "Project".into(), value: "Apollo".into() }));
        app.set_option("dialog", "custom").unwrap();
    });
    type_into(&mut h, "Value:", "x");
    let add_disabled = |h: &Harness<'static, PdfCraftApp>| h.get_by_role_and_label(Role::Button, "Add").accesskit_node().is_disabled();
    assert!(add_disabled(&h), "no name");
    // A standard name, a name with a space, a delimiter, and a name already listed.
    for name in ["Title", "Client name", "R&D/2", "Project"] {
        type_into(&mut h, "Name:", name);
        assert!(add_disabled(&h), "{name} is refused");
        clear(&mut h, "Name:");
    }
    type_into(&mut h, "Name:", "Client");
    assert!(!add_disabled(&h));
    assert!(h.get_by_role_and_label(Role::Button, "Delete").accesskit_node().is_disabled(), "Client is not listed");
}

#[test]
fn a_custom_property_whose_name_the_tools_refuse_is_listed_and_left_alone() {
    // `/Client#20name` and `/Project#20` are valid PDF names with a space in them, which a new custom
    // property can't have; `/Project#20` is not `/Project`. `/Dept#E9` is not UTF-8 and is listed as
    // "Dept\u{FFFD}". `/Revision` is a number, so it isn't listed.
    let pdf = String::from_utf8(fixture(1)).unwrap().replace(
        "/Root 1 0 R >>\nstartxref",
        "/Root 1 0 R /Info << /Client#20name (Acme) /Project#20 (Padded) /Dept#E9 (Sales) /Project (Apollo) /Revision 4 >> >>\nstartxref",
    );
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, pdf.into_bytes()).expect("fixture opens");
        app.set_option("dialog", "custom").unwrap();
        app
    });
    h.run_steps(4);
    for name in ["Client name", "Project ", "Dept\u{FFFD}"] {
        h.get_by_label(name).click();
        h.run_steps(3);
        for button in ["Update", "Delete"] {
            assert!(h.get_by_role_and_label(Role::Button, button).accesskit_node().is_disabled(), "{button} is off for {name:?}");
        }
    }
    clear(&mut h, "Name:");
    type_into(&mut h, "Name:", "Revision");
    assert!(h.get_by_role_and_label(Role::Button, "Add").accesskit_node().is_disabled(), "Add would overwrite the number");
    assert_eq!(h.state().custom_draft.as_ref().map(|(_, d)| d.value.as_str()), Some("Sales"));
    h.get_by_label("Project").click();
    h.run_steps(3);
    clear(&mut h, "Value:");
    type_into(&mut h, "Value:", "Gemini");
    h.get_by_role_and_label(Role::Button, "Update").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(
        custom_properties(&h),
        [
            ("Client name".to_string(), "Acme".to_string()),
            ("Project ".to_string(), "Padded".to_string()),
            ("Dept\u{FFFD}".to_string(), "Sales".to_string()),
            ("Project".to_string(), "Gemini".to_string())
        ]
    );
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    assert!(doc.has_info("Revision") && doc.info_value("Revision").is_none(), "the number is kept");
    assert!(app.dialog.is_none());
}

#[test]
fn a_custom_property_is_deleted_from_an_info_dictionary_in_the_trailer() {
    let pdf = String::from_utf8(fixture(1))
        .unwrap()
        .replace("/Root 1 0 R >>\nstartxref", "/Root 1 0 R /Info << /Project (Apollo) /Revision (4) >> >>\nstartxref");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, pdf.into_bytes()).expect("fixture opens");
        app.set_option("dialog", "custom").unwrap();
        app
    });
    h.run_steps(4);
    h.get_by_label("Project").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Delete").click();
    h.run_steps(3);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(custom_properties(&h), [("Revision".to_string(), "4".to_string())]);
    let app = h.state();
    assert_eq!(app.session.get(app.views[0].id).unwrap().can_undo(), Some("Change document properties"));
}

#[test]
fn add_stops_at_the_256_custom_properties_the_tab_lists() {
    let with_props = |count: usize| {
        let entries: String = (0..count).map(|n| format!("/P{n:03} (v) ")).collect();
        let pdf =
            String::from_utf8(fixture(1)).unwrap().replace("/Root 1 0 R >>\nstartxref", &format!("/Root 1 0 R /Info << {entries}>> >>\nstartxref"));
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("doc.pdf", None, pdf.into_bytes()).expect("fixture opens");
            app.set_option("dialog", "custom").unwrap();
            app
        });
        h.run_steps(4);
        h
    };
    let add_disabled = |h: &Harness<'static, PdfCraftApp>| h.get_by_role_and_label(Role::Button, "Add").accesskit_node().is_disabled();

    // 255 in the file: one more fits, a second does not.
    let mut h = with_props(255);
    type_into(&mut h, "Name:", "Client");
    type_into(&mut h, "Value:", "Acme");
    assert!(!add_disabled(&h));
    h.get_by_role_and_label(Role::Button, "Add").click();
    h.run_steps(3);
    type_into(&mut h, "Name:", "Other");
    type_into(&mut h, "Value:", "x");
    assert!(add_disabled(&h), "the draft holds 256");
    assert_eq!(h.state().custom_draft.as_ref().map(|(_, d)| d.props.len()), Some(256));

    // 257 in the file: after a Delete a new name would still be past the 256 listed.
    let mut h = with_props(257);
    h.get_by_label("P000").click();
    h.run_steps(3);
    h.get_by_role_and_label(Role::Button, "Delete").click();
    h.run_steps(3);
    type_into(&mut h, "Name:", "Client");
    type_into(&mut h, "Value:", "Acme");
    assert!(add_disabled(&h), "the file lists 256");
}

#[test]
fn edit_menu_names_the_step_to_undo() {
    let mut h = harness(2, |app| {
        app.apply_edit(pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 });
    });
    support::open_menu(&mut h, "Edit");
    support::menu_row(&h, "Undo Rotate page").click();
    h.run_steps(3);
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.can_redo(), Some("Rotate page"));
    assert_eq!(doc.info.pages[0].rotation, 0);
}

/// organize.field-name-conflicts: the Combine tab warns, before anything is combined, that a
/// file's form fields collide with earlier files' and will be renamed, naming the new names.
#[test]
fn combine_list_warns_that_colliding_field_names_will_be_renamed() {
    let mut h = harness(1, |app| {
        app.set_option("combine-view", "list").unwrap();
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("one.pdf".into(), form_pdf("Text1")), ("two.pdf".into(), form_pdf("Text1"))]);
    });
    h.run_steps(3);
    h.get_by_label_contains("will be renamed");
    h.get_by_label_contains("Text1 → Text1-1");
}

/// A file split into parts (its pages moved among other files') is combined as one file, once —
/// so the preview must not warn that its field names collide with their own.
#[test]
fn combine_list_does_not_warn_that_a_split_file_collides_with_itself() {
    let mut h = harness(1, |app| {
        app.set_option("combine-view", "list").unwrap();
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("one.pdf".into(), form_pdf_pages("Text1", 2))]);
    });
    h.run_steps(3);
    // one.pdf in two parts — pages 1 and 2, sharing a group, as moving its pages apart leaves it.
    let mut part = h.state().combine_draft[0].clone();
    part.id += 1000;
    h.state_mut().combine_draft[0].range = "1".into();
    part.range = "2".into();
    h.state_mut().combine_draft.push(part);
    h.run_steps(3);
    assert!(h.query_by_label_contains("will be renamed").is_none(), "one file, however many parts, cannot collide with itself");
}

/// Copying a page with a field and pasting it back into the same document is a duplicate: the
/// copy keeps the field's name rather than becoming Text1-1.
#[test]
fn pasting_pages_into_their_own_document_keeps_field_names() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.open_bytes("form.pdf", None, form_pdf("Text1")).unwrap();
    app.views[0].select_pages(&[0]);
    assert!(app.execute("page.copy"));
    assert!(app.execute("page.paste"));
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.info.pages.len(), 2, "pasted");
    assert!(doc.form.iter().all(|f| f.name == "Text1"), "{:?}", doc.form.iter().map(|f| &f.name).collect::<Vec<_>>());
}

#[test]
fn combining_files_opens_a_new_unsaved_tab() {
    let mut h = harness(1, |app| {
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("one.pdf".into(), fixture(2)), ("two.pdf".into(), fixture(1))]);
    });
    h.run_steps(3);
    h.get_by_label("Combine").click();
    h.run_steps(3);
    let app = h.state();
    assert_eq!(app.views.len(), 2);
    assert_eq!(app.active, Some(1));
    assert_eq!(texts_of(app, 1), ["Page 1", "Page 2", "Page 1"]);
    let doc = app.session.get(app.views[1].id).unwrap();
    assert_eq!(doc.name, "Combined.pdf");
    assert!(doc.dirty && doc.path.is_none(), "unsaved until the user saves it");
    assert_eq!(doc.info.outline.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), ["one", "two"]);
    h.get_by_label("Combined.pdf (edited)");
}

#[test]
fn combine_files_takes_chosen_pages_in_the_order_listed() {
    let mut h = harness(1, |app| {
        app.set_option("combine-view", "list").unwrap();
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("one.pdf".into(), fixture(3)), ("two.pdf".into(), fixture(2))]);
    });
    h.run_steps(3);
    h.get_by_label_contains("Files are combined from top to bottom");
    // Nothing is selected yet: the toolbar's Move up is there but disabled.
    assert!(h.get_by_label("Move up").accesskit_node().is_disabled());
    // Select two.pdf with the keyboard and move it up: two.pdf first; one.pdf's pages 3 and 1.
    h.key_press(Key::ArrowUp);
    h.run_steps(2);
    assert_eq!(h.state().combine_selection(), [1]);
    h.get_by_label("Move up").click();
    h.run_steps(2);
    assert_eq!(h.state().combine_draft[0].name, "two.pdf");
    assert_eq!(h.state().combine_selection(), [0], "the selection follows the moved file");
    h.state_mut().combine_draft[1].range = "3, 1".into();
    h.run_steps(2);
    h.get_by_label("2 of 3");
    h.get_by_label("Combine").click();
    h.run_steps(3);
    let app = h.state();
    assert_eq!(texts_of(app, 1), ["Page 1", "Page 2", "Page 3", "Page 1"]);
    let doc = app.session.get(app.views[1].id).unwrap();
    assert_eq!(doc.info.outline.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), ["two", "one"]);
    assert!(app.combine_draft.is_empty());
    assert!(!app.combine_tab.open, "the tab makes way for the result");
}

#[test]
fn combine_files_opens_in_its_own_tab() {
    let mut h = harness(1, |_| {});
    h.state_mut().execute("page.combine");
    h.run_steps(3);
    assert!(h.state().combine_showing() && h.state().active.is_none());
    h.get_by_label("Add the files to combine");
    // Back to the document, then to Combine through its tab.
    h.get_by_label("doc.pdf").click();
    h.run_steps(3);
    assert_eq!(h.state().active, Some(0));
    assert!(!h.state().combine_showing() && h.state().combine_tab.open);
    // The tab, in the tab strip (All tools lists Combine files too).
    h.get_all_by_label("Combine files").find(|n| n.rect().top() < 40.0).unwrap().click();
    h.run_steps(3);
    assert!(h.state().combine_showing());
    // Home hides it too.
    h.get_by_label("Home").click();
    h.run_steps(3);
    assert!(!h.state().combine_showing() && h.state().combine_tab.open);
}

#[test]
fn closing_the_combine_tab_forgets_its_list() {
    let mut h = harness(1, |app| {
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("one.pdf".into(), fixture(1))]);
    });
    h.run_steps(3);
    assert!(h.state().combine_showing());
    h.state_mut().close_combine_tab();
    h.run_steps(3);
    let app = h.state();
    assert!(app.combine_draft.is_empty() && !app.combine_tab.open);
    assert_eq!(app.active, Some(0), "the document next to it shows");
}

#[test]
fn combine_lists_size_and_warns_before_combining() {
    let mut h = harness(1, |app| {
        app.set_option("combine-view", "list").unwrap();
        app.use_files(
            pdfcraft_ui_egui::FilePurpose::Combine,
            vec![
                ("one.pdf".into(), fixture(3)),
                ("locked.pdf".into(), protected("", "owner", -1 ^ 1024 ^ 8)),
                ("secret.pdf".into(), protected("pw", "owner", -1)),
            ],
        );
    });
    h.run_steps(3);
    h.get_by_label("File name");
    h.get_by_label("Warnings");
    h.get_by_label("Its security settings don't allow copying pages");
    h.get_by_label("Password-protected");
    h.get_by_label("2 files need attention");
    assert!(h.get_by_label("Combine").accesskit_node().is_disabled());
    // Removing them (Delete on the selection) lets Combine run.
    for _ in 0..2 {
        h.state_mut().select_combine_rows(&[1]);
        h.key_press(Key::Delete);
        h.run_steps(2);
    }
    assert_eq!(h.state().combine_draft.len(), 1);
    h.get_by_label_contains("1 file · 3 pages");
}

#[test]
fn a_bad_range_stops_combine_until_fixed() {
    let mut h = harness(1, |app| {
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("one.pdf".into(), fixture(3)), ("two.pdf".into(), fixture(2))]);
    });
    h.run_steps(3);
    h.get_by_label_contains("2 files · 5 pages");
    h.state_mut().combine_draft[0].range = "1-99".into();
    h.run_steps(2);
    h.get_by_label("1 file needs attention");
    assert!(h.get_by_label("Combine").accesskit_node().is_disabled());
    h.state_mut().combine_draft[0].range = "2-3".into();
    h.run_steps(2);
    h.get_by_label_contains("2 files · 4 pages");
    assert!(!h.get_by_label("Combine").accesskit_node().is_disabled());
}

#[test]
fn alt_arrows_move_the_selected_file() {
    let mut h = harness(1, |app| {
        app.use_files(
            pdfcraft_ui_egui::FilePurpose::Combine,
            vec![("a.pdf".into(), fixture(1)), ("b.pdf".into(), fixture(1)), ("c.pdf".into(), fixture(1))],
        );
    });
    h.run_steps(3);
    h.key_press(Key::ArrowDown);
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowDown);
    h.run_steps(2);
    let names: Vec<_> = h.state().combine_draft.iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, ["b.pdf", "a.pdf", "c.pdf"]);
    assert_eq!(h.state().combine_selection(), [1]);
}

#[test]
fn column_headings_sort_the_list_and_flip_on_the_second_click() {
    let mut h = combine_of(&[("scan10.pdf", 1), ("Scan2.pdf", 3), ("a.pdf", 2)]);
    h.get_by_label("File name").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["a.pdf", "Scan2.pdf", "scan10.pdf"], "case-insensitive, numbers by value");
    h.get_by_label("File name, sorted ascending").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["scan10.pdf", "Scan2.pdf", "a.pdf"]);
    h.get_by_label("Pages").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["scan10.pdf", "a.pdf", "Scan2.pdf"]);
    // Moving a file by hand ends the sorted order.
    h.state_mut().select_combine_rows(&[2]);
    h.get_by_label("Move up").click();
    h.run_steps(2);
    assert_eq!(h.state().combine_tab.sort, None);
    h.get_by_label("Pages");
}
