//! Editing tests through the app UI (part 2 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn ctrl_and_shift_clicks_select_several_files_that_move_and_go_together() {
    let mut h = combine_of(&[("a.pdf", 1), ("b.pdf", 1), ("c.pdf", 1), ("d.pdf", 1), ("e.pdf", 1)]);
    h.get_by_label("b.pdf").click();
    h.run_steps(1);
    h.get_by_label("d.pdf").click_modifiers(Modifiers::SHIFT);
    h.run_steps(1);
    assert_eq!(h.state().combine_selection(), [1, 2, 3]);
    h.get_by_label("c.pdf").click_modifiers(Modifiers::COMMAND);
    h.run_steps(1);
    assert_eq!(h.state().combine_selection(), [1, 3], "Ctrl/⌘ toggles one file");
    h.get_by_label("2 selected");
    h.get_by_label("Move up").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["b.pdf", "a.pdf", "d.pdf", "c.pdf", "e.pdf"]);
    h.get_by_label("Remove 2 files").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["a.pdf", "c.pdf", "e.pdf"]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    assert_eq!(h.state().combine_selection(), [0, 1, 2]);
}

#[test]
fn undo_and_redo_restore_the_combine_list() {
    let mut h = combine_of(&[("a.pdf", 1), ("b.pdf", 1), ("c.pdf", 1)]);
    h.state_mut().select_combine_rows(&[0, 2]);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["b.pdf"]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["a.pdf", "b.pdf", "c.pdf"], "⌘Z brings the removed files back");
    assert_eq!(h.state().combine_selection(), [0, 2], "…selected as they were");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["b.pdf"]);
    // The toolbar's buttons do the same; undo also reverses a sort and the files' addition.
    h.get_by_label("Undo").click();
    h.run_steps(2);
    h.get_by_label("File name").click();
    h.run_steps(2);
    h.get_by_label("File name, sorted ascending").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["c.pdf", "b.pdf", "a.pdf"]);
    h.get_by_label("Undo").click();
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["a.pdf", "b.pdf", "c.pdf"]);
    // A page range typed in is one step.
    h.state_mut().combine_draft[0].range.clear();
    let field = h.get_all_by_role(Role::TextInput).next().unwrap();
    field.focus();
    h.run_steps(1);
    h.get_all_by_role(Role::TextInput).next().unwrap().type_text("1");
    h.run_steps(2);
    assert_eq!(h.state().combine_draft[0].range, "1");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().combine_draft[0].range, "");
    // The document's own history is untouched: it has nothing to undo.
    h.get_by_label("doc.pdf").click();
    h.run_steps(2);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().can_undo().is_none());
}

#[test]
fn protected_files_that_can_be_combined_are_flagged() {
    let mut h = harness(1, |app| {
        app.set_option("combine-view", "list").unwrap();
        app.use_files(
            pdfcraft_ui_egui::FilePurpose::Combine,
            vec![("one.pdf".into(), fixture(1)), ("no-copy.pdf".into(), protected("", "owner", -1 ^ 16))],
        );
    });
    h.run_steps(3);
    let app = h.state();
    assert!(app.combine_draft[1].problem.is_none(), "copying text is withheld, assembling pages isn't");
    h.get_by_label("Protected: the combined file won't keep its security settings");
    assert!(!h.get_by_label("Combine").accesskit_node().is_disabled(), "a warning, not a blocker");
}

#[test]
fn the_whole_heading_sorts_and_dragging_it_moves_the_column() {
    let mut h = combine_of(&[("b.pdf", 1), ("a.pdf", 3)]);
    // A click in the empty right part of the heading cell, away from its text.
    let cell = h.get_by_label("File name").rect();
    let spot = egui::pos2(cell.right() - 30.0, cell.center().y);
    h.hover_at(spot);
    h.run_steps(1);
    h.drag_at(spot);
    h.run_steps(1);
    h.drop_at(spot);
    h.run_steps(2);
    assert_eq!(combine_names(&h), ["a.pdf", "b.pdf"]);
    // Drag Size before File name.
    let size = h.get_by_label("Size").rect().center();
    let name = h.get_by_label("File name, sorted ascending").rect();
    drag(&mut h, size, egui::pos2(name.left() + 10.0, name.center().y));
    use pdfcraft_ui_egui::SortKey::*;
    assert_eq!(h.state().combine_columns.order, [Size, Name, Pages, Modified, Warnings]);
    assert_eq!(combine_names(&h), ["a.pdf", "b.pdf"], "moving a column doesn't sort");
    let (size, name) = (h.get_by_label("Size").rect(), h.get_by_label("File name, sorted ascending").rect());
    assert!(size.left() < name.left(), "Size now shows first");
}

#[test]
fn columns_resize_and_the_layout_is_kept_in_the_settings() {
    let mut h = combine_of(&[("a.pdf", 1), ("b.pdf", 1)]);
    let size = h.get_by_label("Size").rect();
    // The separator after Size: drag it right by 60 points.
    let edge = egui::pos2(size.right() + 2.0, size.center().y);
    drag(&mut h, edge, edge + egui::vec2(60.0, 0.0));
    let widths = h.state().combine_columns.widths.expect("a resized column keeps its width");
    let size_index = pdfcraft_ui_egui::SortKey::Size as usize;
    assert!(widths[size_index] > 120.0, "{widths:?}");
    let saved = h.state().persist();
    let mut fresh = PdfCraftApp::new();
    fresh.set_option("language", "en").unwrap();
    fresh.restore(&saved);
    assert_eq!(fresh.combine_columns, h.state().combine_columns);
    // Malformed settings give the default layout.
    fresh.restore(r#"{"combine_columns":{"order":["size","size"],"widths":[1,2]}}"#);
    assert_eq!(fresh.combine_columns, Default::default());
}

#[test]
fn a_folder_adds_its_pdfs_images_and_text_in_order_and_optionally_its_subfolders() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-combine-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    for (name, pages) in [("page10.pdf", 1), ("page2.PDF", 2), ("sub/inner.pdf", 3)] {
        std::fs::write(dir.join(name), fixture(pages)).unwrap();
    }
    // #569: images and text files join too; other files are left out.
    std::fs::write(dir.join("notes.txt"), b"Some notes").unwrap();
    std::fs::write(dir.join("readme.md"), b"# not combinable").unwrap();
    let mut h = harness(1, |_| {});
    h.state_mut().execute("page.combine");
    h.run_steps(2);
    h.state_mut().pick_override = Some(vec![dir.to_string_lossy().into_owned()]);
    h.get_by_label("Add folder…").click();
    h.run_steps(4);
    assert_eq!(combine_names(&h), ["notes.txt", "page2.PDF", "page10.pdf"], "numbers by value");
    // The toolbar's menu: the folder and its subfolders, as one undo step.
    h.get_by_label("More ways to add files").click();
    h.run_steps(2);
    h.get_by_label("Add folder and subfolders…").click();
    h.run_steps(4);
    assert_eq!(combine_names(&h), ["notes.txt", "page2.PDF", "page10.pdf", "notes.txt", "page2.PDF", "page10.pdf", "inner.pdf"]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(combine_names(&h).len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

/// #569: an image or a text file added to Combine files becomes a PDF as it joins the list, an
/// ordinary row with its pages.
#[test]
fn images_and_text_files_join_the_combine_list_as_pdfs() {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(40, 20, image::Rgb([90, 120, 200])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut h = combine_of(&[("one.pdf", 2)]);
    h.state_mut()
        .use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("scan.png".into(), png.into_inner()), ("notes.txt".into(), b"Some notes".to_vec())]);
    h.run_steps(3);
    assert_eq!(combine_names(&h), ["one.pdf", "scan.png", "notes.txt"]);
    let draft = &h.state().combine_draft;
    assert!(draft.iter().all(|f| f.bytes.starts_with(b"%PDF-") && f.problem.is_none()), "made into PDFs");
    assert_eq!(draft.iter().map(|f| f.pages).collect::<Vec<_>>(), [2, 1, 1]);
    h.get_by_label("Combine").click();
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 1), ["Page 1", "Page 2", "", "Some notes"]);
}

/// A file that can't be read (a damaged image, a type that can't be made into a PDF) joins the
/// list all the same, with why; Combine asks before leaving it out, and Cancel changes nothing.
#[test]
fn files_that_cannot_be_read_are_listed_and_left_out_once_the_user_agrees() {
    let mut h = combine_of(&[("one.pdf", 2)]);
    let bad = vec![("broken.jpg".to_string(), b"\xff\xd8\xff\xe0 not a JPEG".to_vec()), ("report.docx".to_string(), b"PK\x03\x04".to_vec())];
    h.state_mut().use_files(pdfcraft_ui_egui::FilePurpose::Combine, bad);
    h.run_steps(3);
    assert_eq!(combine_names(&h), ["one.pdf", "broken.jpg", "report.docx"]);
    for f in &h.state().combine_draft[1..] {
        let why = f.problem.as_deref().expect("why it can't be combined");
        assert!(why.starts_with("Can't be read: ") && !why.contains(&f.name), "{why}");
        assert!(f.lock.is_none() && f.pages == 0);
    }
    h.get_by_label_contains("this file type can't be converted");
    // One readable file is not enough to combine.
    assert!(h.get_by_label("Combine").accesskit_node().is_disabled());
    h.state_mut().use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("two.pdf".into(), fixture(1))]);
    h.run_steps(3);
    assert!(!h.get_by_label("Combine").accesskit_node().is_disabled());
    // Combine asks first, naming the files left out; Cancel leaves everything as it was.
    h.get_by_label("Combine").click();
    h.run_steps(3);
    h.get_by_label("Leave out files that can't be read?");
    h.get_by_label("• broken.jpg");
    h.get_by_label("• report.docx");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.query_by_label("Leave out files that can't be read?").is_none());
    assert_eq!(h.state().views.len(), 1, "nothing combined");
    assert_eq!(combine_names(&h).len(), 4);
    // Asked again: Combine the rest.
    h.get_by_label("Combine").click();
    h.run_steps(3);
    h.get_by_label("Combine the rest").click();
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 1), ["Page 1", "Page 2", "Page 1"]);
}

#[test]
fn a_password_protected_file_is_unlocked_with_its_password_and_combined() {
    let mut h = combine_of(&[("one.pdf", 1)]);
    h.state_mut().use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("secret.pdf".into(), protected("pw", "owner", -1))]);
    h.run_steps(3);
    h.get_by_label("Password-protected");
    assert!(h.get_by_label("Combine").accesskit_node().is_disabled());
    // The row's link (the toolbar's Unlock… comes first).
    h.get_all_by_label("Unlock…").last().unwrap().click();
    h.run_steps(3);
    h.get_by_label("Unlock secret.pdf");
    type_password(&mut h, "nope");
    h.get_by_label("Wrong password");
    assert!(h.state().combine_draft[1].lock.is_some());
    // The box stays open for another try.
    let field = h.get_by_role(Role::PasswordInput);
    field.focus();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    type_password(&mut h, "pw");
    let f = &h.state().combine_draft[1];
    assert!(f.lock.is_none() && f.problem.is_none());
    assert_eq!(f.pages, 2);
    h.get_by_label("Unlocked: the combined file won't be password-protected");
    // Never printed: debug output ends up in logs.
    assert!(!format!("{:?} {:?}", h.state().combine_draft, h.state().combine_tab).contains("pw\""));
    h.get_by_label("Combine").click();
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 1), ["Page 1", "Page 1", "Page 2"]);
}

#[test]
fn one_password_unlocks_several_selected_files_and_undo_locks_them_again() {
    let mut h = combine_of(&[("one.pdf", 1)]);
    h.state_mut().use_files(
        pdfcraft_ui_egui::FilePurpose::Combine,
        vec![
            ("a.pdf".into(), protected("same", "o1", -1)),
            ("b.pdf".into(), protected("same", "o2", -1)),
            ("c.pdf".into(), protected("other", "o3", -1)),
        ],
    );
    h.run_steps(3);
    h.state_mut().select_combine_rows(&[1, 2, 3]);
    h.run_steps(1);
    h.get_all_by_label("Unlock…").next().unwrap().click(); // the toolbar's (the rows' links come after it)
    h.run_steps(3);
    h.get_by_label("Unlock 3 files");
    type_password(&mut h, "same");
    let locked: Vec<bool> = h.state().combine_draft.iter().map(|f| f.lock.is_some()).collect();
    assert_eq!(locked, [false, false, false, true]);
    h.get_by_label_contains("Unlocked 2 of 3 files");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert!(h.state().combine_draft[1..].iter().all(|f| f.lock.is_some()), "undo locks them again");
}

#[test]
fn a_file_that_opens_but_forbids_combining_takes_the_permissions_password() {
    let mut h = combine_of(&[("one.pdf", 1)]);
    // Opens with "pw"; only the owner may assemble pages.
    h.state_mut().use_files(pdfcraft_ui_egui::FilePurpose::Combine, vec![("both.pdf".into(), protected("pw", "owner", -1 ^ 1024 ^ 8))]);
    h.run_steps(3);
    assert!(h.state_mut().combine_unlock_rows(&[1], "pw"));
    h.run_steps(2);
    assert_eq!(h.state().combine_draft[1].lock, Some(pdfcraft_ui_egui::CombineLock::Permissions));
    h.get_by_label_contains("Enter the permissions password");
    h.get_all_by_label("Unlock…").last().unwrap().click();
    h.run_steps(3);
    h.get_by_label("Enter the permissions password to allow combining");
    type_password(&mut h, "owner");
    assert!(h.state().combine_draft[1].lock.is_none());
    assert!(!h.get_by_label("Combine").accesskit_node().is_disabled());
}

#[test]
fn an_rc4_file_unlocks_with_its_owner_password_too() {
    // The engine reads RC4 files with their owner password; the inspector may not, and the
    // engine's answer is the one that counts.
    let mut h = combine_of(&[("one.pdf", 1)]);
    h.state_mut().use_files(
        pdfcraft_ui_egui::FilePurpose::Combine,
        vec![("rc4.pdf".into(), protected_with(pdfcraft_cos::Algorithm::Rc4_128, "user", "boss", -1))],
    );
    h.run_steps(3);
    assert!(h.state_mut().combine_unlock_rows(&[1], "boss"));
    let f = &h.state().combine_draft[1];
    assert!(f.lock.is_none());
    assert_eq!(f.pages, 2);
}

#[test]
fn open_documents_can_be_added_to_combine() {
    let mut h = harness(2, |_| {});
    h.state_mut().execute("page.combine");
    h.run_steps(3);
    h.get_by_label("Add open documents").click();
    h.run_steps(2);
    // The menu's entry, after the tab of the same name.
    h.get_all_by_label("doc.pdf").last().unwrap().click();
    h.run_steps(3);
    let app = h.state();
    assert_eq!(app.combine_draft.len(), 1);
    assert_eq!(app.combine_draft[0].pages, 2);
    assert!(app.combine_showing());
}

#[test]
fn extract_button_copies_selected_pages_to_a_new_tab() {
    let mut h = organize(3);
    h.get_by_label("Page 2").click();
    h.run_steps(1);
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    h.get_by_label("Extract pages to a new document").click();
    h.run_steps(3);
    h.get_by_label("2 pages selected.");
    h.get_all_by_label("Extract").last().unwrap().click();
    h.run_steps(3);
    let app = h.state();
    assert_eq!(app.views.len(), 2);
    assert_eq!(texts_of(app, 1), ["Page 2", "Page 3"]);
    assert_eq!(texts_of(app, 0).len(), 3, "the original is unchanged");
    assert!(!app.session.get(app.views[0].id).unwrap().dirty);
}

#[test]
fn replace_button_replaces_selected_pages_in_organize() {
    let dir = temp_path("replace-btn");
    std::fs::create_dir_all(&dir).unwrap();
    let other = dir.join("other.pdf");
    std::fs::write(&other, fixture(5)).unwrap();

    let mut h = organize(3);
    h.get_by_label("Page 2").click();
    h.run_steps(1);
    h.get_by_label("Page 3").click_modifiers(Modifiers::COMMAND);
    h.run_steps(2);
    h.state_mut().pick_override = Some(vec![other.to_string_lossy().into_owned()]);
    h.get_by_label("Replace").click();
    h.run_steps(3);
    egui_kittest::kittest::Queryable::get_by_label(&h, "Replace Pages");
    assert_eq!(h.state().replace_draft.as_ref().unwrap().from, 2);
    assert_eq!(h.state().replace_draft.as_ref().unwrap().to, 3);
    egui_kittest::kittest::Queryable::get_by_label(&h, "OK").click();
    h.run_steps(3);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.info.pages.len(), 3);
    assert_eq!(doc.can_undo(), Some("Replace pages"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_pages_from_thumbnail_context_menu() {
    let dir = temp_path("replace-ctx");
    std::fs::create_dir_all(&dir).unwrap();
    let other = dir.join("other.pdf");
    std::fs::write(&other, fixture(5)).unwrap();

    let mut h = organize(3);
    h.state_mut().pick_override = Some(vec![other.to_string_lossy().into_owned()]);
    h.get_by_label("Page 2").click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Replace pages…").last().unwrap().click();
    h.run_steps(3);
    egui_kittest::kittest::Queryable::get_by_label(&h, "Replace Pages");
    assert_eq!(h.state().replace_draft.as_ref().unwrap().from, 2);
    assert_eq!(h.state().replace_draft.as_ref().unwrap().to, 2);
    egui_kittest::kittest::Queryable::get_by_label(&h, "OK").click();
    h.run_steps(3);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.info.pages.len(), 3);
    assert_eq!(doc.can_undo(), Some("Replace page"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_pages_from_pages_panel_context_menu() {
    let dir = temp_path("replace-panel-ctx");
    std::fs::create_dir_all(&dir).unwrap();
    let other = dir.join("other.pdf");
    std::fs::write(&other, fixture(5)).unwrap();

    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    h.state_mut().pick_override = Some(vec![other.to_string_lossy().into_owned()]);
    let thumb = h.get_all_by_label("Page 2").max_by(|a, b| a.rect().left().total_cmp(&b.rect().left())).unwrap();
    thumb.click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Replace pages…").last().unwrap().click();
    h.run_steps(3);
    egui_kittest::kittest::Queryable::get_by_label(&h, "Replace Pages");
    assert_eq!(h.state().replace_draft.as_ref().unwrap().from, 2);
    assert_eq!(h.state().replace_draft.as_ref().unwrap().to, 2);
    egui_kittest::kittest::Queryable::get_by_label(&h, "OK").click();
    h.run_steps(3);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.info.pages.len(), 4);
    assert_eq!(doc.can_undo(), Some("Replace page"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inserting_a_file_goes_after_the_selection_and_undoes() {
    let mut h = organize(2);
    h.get_by_label("Page 1").click();
    h.run_steps(2);
    h.state_mut().use_files(pdfcraft_ui_egui::FilePurpose::InsertPages, vec![("extra.pdf".into(), fixture(2))]);
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 1", "Page 2", "Page 2"]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2"]);
}

#[test]
fn plus_between_pages_inserts_picked_files_there_in_order() {
    let dir = temp_path("insert-gap");
    std::fs::create_dir_all(&dir).unwrap();
    let (extra, note) = (dir.join("extra.pdf"), dir.join("note.txt"));
    std::fs::write(&extra, fixture(2)).unwrap();
    std::fs::write(&note, "a note").unwrap();
    let mut h = organize(3);
    // Page 3 is selected, yet the files go where the "+" is: between pages 1 and 2.
    h.get_by_label("Page 3").click();
    h.run_steps(2);
    h.state_mut().pick_override = Some(vec![extra.to_string_lossy().into_owned(), note.to_string_lossy().into_owned()]);
    h.get_by_label("Insert a file before page 2").click();
    h.run_steps(4);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 1", "Page 2", "a note", "Page 2", "Page 3"]);
    assert_eq!(picked(&h), [1, 2, 3], "the inserted pages are selected");
    // Each file is one undo step.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2", "Page 3"]);
    // Both ends.
    h.state_mut().pick_override = Some(vec![note.to_string_lossy().into_owned()]);
    h.get_by_label("Insert a file before page 1").click();
    h.run_steps(4);
    h.get_by_label("Insert a file at the end").click();
    h.run_steps(4);
    assert_eq!(texts_of(h.state(), 0), ["a note", "Page 1", "Page 2", "Page 3", "a note"]);
    // The toolbar button still inserts after the selection, not at the last "+" used.
    h.get_by_label("Page 2").click();
    h.run_steps(2);
    insert_menu(&mut h, "Insert pages from a file…");
    h.run_steps(4);
    assert_eq!(texts_of(h.state(), 0), ["a note", "Page 1", "a note", "Page 2", "Page 3", "a note"]);
    // A file that can't be converted changes nothing.
    let before = texts_of(h.state(), 0);
    h.state_mut().use_files(pdfcraft_ui_egui::FilePurpose::InsertPages, vec![("report.docx".into(), b"PK\x03\x04".to_vec())]);
    h.run_steps(2);
    assert_eq!(texts_of(h.state(), 0), before);
}

#[test]
fn files_dropped_between_pages_are_inserted_there() {
    let mut h = organize(3);
    // Over the gap between pages 1 and 2.
    let gap = h.get_by_label("Insert a file before page 2").rect().center();
    h.hover_at(gap + egui::vec2(0.0, 40.0));
    h.run_steps(2);
    drop_files(&mut h, vec![Dropped("extra.pdf", fixture(2)), Dropped("note.txt", b"a note".to_vec())]);
    h.run_steps(4);
    assert_eq!(h.state().views.len(), 1, "dropped files are not opened as documents");
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 1", "Page 2", "a note", "Page 2", "Page 3"]);
    assert_eq!(picked(&h), [1, 2, 3], "the inserted pages are selected");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2", "Page 3"]);
    // Right of the last page: the end.
    let last = h.get_by_label("Page 3").rect();
    h.hover_at(last.center() + egui::vec2(last.width() * 0.4, 0.0));
    h.run_steps(2);
    drop_files(&mut h, vec![Dropped("note.txt", b"a note".to_vec())]);
    h.run_steps(4);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2", "Page 3", "a note"]);
}

#[test]
fn files_dropped_before_the_pointer_is_known_wait_for_it() {
    // Some platforms don't report the pointer while files are dragged over the window.
    let mut h = organize(2);
    drop_files(&mut h, vec![Dropped("note.txt", b"a note".to_vec())]);
    h.run_steps(1);
    assert_eq!(texts_of(h.state(), 0).len(), 2, "not placed yet");
    let gap = h.get_by_label("Insert a file before page 2").rect().center();
    h.hover_at(gap + egui::vec2(0.0, 40.0));
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "a note", "Page 2"]);
}

#[test]
fn files_dropped_outside_the_page_grid_still_open_as_documents() {
    let mut h = harness(2, |_| {});
    let original = h.state().views[0].id;
    drop_files(&mut h, vec![Dropped("other.pdf", fixture(1))]);
    support::wait_until(&mut h, "dropped document", |app| app.views.len() == 2 && app.password_prompt.is_none());
    assert_eq!(h.state().views.len(), 2);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2"]);
    assert!(h.state().views.iter().any(|view| view.id == original));
    assert_eq!(h.state().session.get(original).unwrap().bytes.as_slice(), fixture(2).as_slice());
    let (_, id) = h.state().active_ids().unwrap();
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.name, "other.pdf");
    assert_eq!(doc.info.pages.len(), 1);
    assert_eq!(doc.bytes.as_slice(), fixture(1).as_slice());
}

#[test]
fn files_dropped_on_the_pages_panel_are_inserted_there() {
    let mut h = harness(3, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    // The top half of page 2's thumbnail: between pages 1 and 2.
    let thumb = panel_thumb_rect(&h, "Page 2");
    h.hover_at(thumb.center() - egui::vec2(0.0, thumb.height() * 0.25));
    h.run_steps(2);
    assert_eq!(h.state().views[0].panel_gap, Some(1));
    drop_files(&mut h, vec![Dropped("extra.pdf", fixture(2)), Dropped("note.txt", b"a note".to_vec())]);
    h.run_steps(4);
    assert_eq!(h.state().views.len(), 1, "dropped files are not opened as documents");
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 1", "Page 2", "a note", "Page 2", "Page 3"]);
    assert_eq!(picked(&h), [1, 2, 3], "the inserted pages are selected");
    // The bottom half of the last thumbnail: the end.
    let mut h = harness(2, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    let last = panel_thumb_rect(&h, "Page 2");
    h.hover_at(last.center() + egui::vec2(0.0, last.height() * 0.25));
    h.run_steps(2);
    drop_files(&mut h, vec![Dropped("note.txt", b"a note".to_vec())]);
    h.run_steps(4);
    assert_eq!(h.state().views.len(), 1);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2", "a note"]);
}

#[test]
fn files_dropped_beside_the_pages_panel_still_open_as_documents() {
    let mut h = harness(2, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    // Over the page view, not the panel.
    let thumb = panel_thumb_rect(&h, "Page 1");
    h.hover_at(egui::pos2(thumb.left() - 300.0, thumb.center().y));
    h.run_steps(2);
    drop_files(&mut h, vec![Dropped("other.pdf", fixture(1))]);
    // Native opening prepares the document on a worker; three UI frames need not give it
    // any CPU time. Wait for the result, as the other dropped-document regression does.
    support::wait_until(&mut h, "document dropped beside the Pages panel", |app| app.views.len() == 2 && app.password_prompt.is_none());
    assert_eq!(h.state().views.len(), 2);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 2"]);
    assert_eq!(texts_of(h.state(), 1), ["Page 1"], "the dropped PDF opens as its own document");
}

#[test]
fn page_grid_zooms_with_its_buttons_keys_and_pinch() {
    let mut h = organize(12);
    let width = |h: &Harness<'static, PdfCraftApp>| h.get_by_label("Page 2").rect().left() - h.get_by_label("Page 1").rect().left();
    let usual = width(&h);
    h.get_by_label("Larger pages").click();
    h.run_steps(3);
    assert_eq!(grid_zoom(&h), 1.25);
    assert!(width(&h) > usual + 20.0, "the pages are drawn larger");
    h.get_by_label("Reset page size").click();
    h.run_steps(3);
    assert_eq!((grid_zoom(&h), width(&h)), (1.0, usual));
    h.get_by_label("Smaller pages").click();
    h.run_steps(3);
    assert_eq!(grid_zoom(&h), 0.8);
    assert!(width(&h) < usual - 20.0);
    // Keys.
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 1.0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 1.25);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 1.0);
    // Pinch (or Ctrl/⌘ + wheel) over the grid; elsewhere it leaves the grid alone.
    let page = h.get_by_label("Page 1").rect().center();
    h.hover_at(page);
    h.run_steps(1);
    h.event(egui::Event::Zoom(1.5));
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 1.5);
    h.hover_at(h.get_by_label("Larger pages").rect().center());
    h.run_steps(1);
    h.event(egui::Event::Zoom(1.5));
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 1.5, "the pointer was on the toolbar");
    // It stops at its limits, where the buttons switch off.
    for _ in 0..12 {
        h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
        h.run_steps(1);
    }
    assert_eq!(grid_zoom(&h), 3.0);
    h.get_by_label("Larger pages").click();
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 3.0);
    for _ in 0..20 {
        h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
        h.run_steps(1);
    }
    assert_eq!(grid_zoom(&h), 0.5);
    assert!(!dirty(&h), "zooming is not an edit");
}

/// As in Acrobat, Organize pages keeps the size last chosen: every document's grid opens at it,
/// and so does the next start of the app.
#[test]
fn page_grid_size_is_remembered_for_every_document_and_restart() {
    let mut h = organize(3);
    h.get_by_label("Larger pages").click();
    h.run_steps(3);
    assert_eq!(grid_zoom(&h), 1.25);
    h.state_mut().open_bytes("other.pdf", None, fixture(2)).expect("second fixture opens");
    h.state_mut().set_option("organize", "on").unwrap();
    h.run_steps(4);
    let other = h.state().active.expect("the new document is active");
    assert_ne!(other, 0);
    assert_eq!(h.state().views[other].grid_zoom(), 1.25, "the other document");
    let settings = h.state().persist();
    let h = harness(3, move |app| {
        app.restore(&settings);
        app.set_option("organize", "on").unwrap();
    });
    assert_eq!(grid_zoom(&h), 1.25, "after a restart");
}

#[test]
fn a_damaged_remembered_page_grid_size_is_ignored_or_clamped() {
    let mut app = PdfCraftApp::new();
    app.restore(r#"{"organize_zoom": "huge"}"#);
    assert_eq!(app.organize_zoom, 1.0);
    app.restore(r#"{"organize_zoom": 40.0}"#);
    assert_eq!(app.organize_zoom, 3.0);
    app.restore(r#"{"organize_zoom": -2}"#);
    assert_eq!(app.organize_zoom, 0.5);
}

#[test]
fn zoomed_pages_in_view_are_rendered_at_the_size_drawn() {
    // A long document: sharpness must not depend on how many pages there are.
    let mut h = harness(400, |app| {
        app.set_option("organize", "on").unwrap();
        app.set_option("grid-zoom", "300").unwrap();
    });
    // The 200 × 300 fixture fits by height; keep its aspect ratio instead of stretching
    // it to the full cell width. Request enough pixels for that actual preview width.
    let drawn = 194.0 * 3.0 * (200.0 / 300.0) * h.ctx.pixels_per_point();
    let sharp = |h: &Harness<'static, PdfCraftApp>, page| h.state().views[0].grid_page_pixels(page);
    for _ in 0..200 {
        if sharp(&h, 0).is_some_and(|w| w as f32 >= drawn * 0.9) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        h.run_steps(1);
    }
    let width = sharp(&h, 0).expect("the first page has a sharp render");
    assert!((drawn * 0.9..=drawn * 1.6).contains(&(width as f32)), "{width} for {drawn}");
    assert!(sharp(&h, 399).is_none(), "pages out of view have none");
    // Zoomed out again, thumbnails are enough and the sharp renders are dropped.
    h.state_mut().set_option("grid-zoom", "50").unwrap();
    h.run_steps(3);
    assert!(sharp(&h, 0).is_none(), "page 0 still has a sharp render: {:?}", sharp(&h, 0));
}

#[test]
fn a_zoomed_page_grid_still_inserts_and_moves_at_the_right_gap() {
    let dir = temp_path("zoomed-grid");
    std::fs::create_dir_all(&dir).unwrap();
    let note = dir.join("note.txt");
    std::fs::write(&note, "a note").unwrap();
    let mut h = harness(3, |app| {
        app.set_option("organize", "on").unwrap();
        app.set_option("grid-zoom", "200").unwrap();
        assert!(
            app.set_option("grid-zoom", "20").is_err() && app.set_option("grid-zoom", "NaN").is_err() && app.set_option("grid-zoom", "big").is_err()
        );
    });
    assert_eq!(grid_zoom(&h), 2.0);
    h.state_mut().pick_override = Some(vec![note.to_string_lossy().into_owned()]);
    h.get_by_label("Insert a file before page 2").click();
    h.run_steps(4);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "a note", "Page 2", "Page 3"]);
    // Files dropped over a gap go there too.
    let gap = h.get_by_label("Insert a file before page 2").rect().center();
    h.hover_at(gap + egui::vec2(0.0, 60.0));
    h.run_steps(2);
    drop_files(&mut h, vec![Dropped("more.txt", b"more".to_vec())]);
    h.run_steps(4);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "more", "a note", "Page 2", "Page 3"]);
}

#[test]
fn save_pages_writes_what_the_grid_shows() {
    let path = temp_path("grid-save.pdf");
    let mut h = organize(3);
    h.get_by_label("Page 2").click();
    h.run_steps(2);
    h.key_press(Key::Delete);
    h.run_steps(3);
    h.state_mut().save_override = Some(path.to_string_lossy().into_owned());
    h.get_by_label("Save pages").click();
    h.run_steps(4);
    assert!(!dirty(&h));
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.open_bytes("saved.pdf", None, std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(texts_of(&app, 0), ["Page 1", "Page 3"]);
}

#[test]
fn split_dialog_writes_one_file_per_part() {
    let dir = temp_path("split-count");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().into_owned();
    let mut h = organize(3);
    h.state_mut().export_dir_override = Some(d);
    h.get_all_by_label("Split").last().unwrap().click();
    h.run_steps(3);
    h.get_by_label_contains("Creates 3 files from 3 pages");
    h.get_all_by_label("Split").last().unwrap().click();
    h.run_steps(3);
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["doc (page 1).pdf", "doc (page 2).pdf", "doc (page 3).pdf"]);
    for name in names {
        let info = pdfcraft_render::inspect(std::sync::Arc::new(std::fs::read(dir.join(name)).unwrap()), None).unwrap();
        assert_eq!(info.pages.len(), 1);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn split_at_bookmarks_with_equal_titles_keeps_every_part() {
    let dir = temp_path("split-dup-titles");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = organize(3);
    h.state_mut().export_dir_override = Some(dir.to_string_lossy().into_owned());
    for (index, title) in [(0, "Same"), (1, "Same"), (2, "same")] {
        h.state_mut().apply_edit(pdfcraft_engine::Edit::AddBookmark {
            parent: vec![],
            index,
            title: title.into(),
            page: index,
            left: None,
            top: None,
        });
    }
    h.state_mut().split_draft.mode = pdfcraft_ui_egui::SplitMode::Bookmarks;
    h.state_mut().run_command("page.split");
    h.run_steps(3);
    h.get_by_label_contains("Creates 3 files from 3 pages");
    h.get_all_by_label("Split").last().unwrap().click();
    h.run_steps(3);
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["doc - Same (2).pdf", "doc - Same.pdf", "doc - same (3).pdf"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn split_before_selected_pages() {
    let dir = temp_path("split-sel");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = organize(4);
    h.state_mut().export_dir_override = Some(dir.to_string_lossy().into_owned());
    h.state_mut().views[0].select_pages(&[2]);
    h.state_mut().split_draft.mode = pdfcraft_ui_egui::SplitMode::Selection;
    h.state_mut().run_command("page.split");
    h.run_steps(3);
    h.get_by_label_contains("Creates 2 files from 4 pages");
    h.get_all_by_label("Split").last().unwrap().click();
    h.run_steps(3);
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["doc (pages 1-2).pdf", "doc (pages 3-4).pdf"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn split_from_organize_grid_context_menu() {
    let dir = temp_path("split-org-ctx");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = organize(4);
    h.state_mut().export_dir_override = Some(dir.to_string_lossy().into_owned());
    h.get_by_label("Page 3").click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Split document…").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Split));
    assert_eq!(h.state().split_draft.mode, pdfcraft_ui_egui::SplitMode::Selection);
    h.get_by_label_contains("Creates 2 files from 4 pages");
    h.get_all_by_label("Split").last().unwrap().click();
    h.run_steps(3);
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["doc (pages 1-2).pdf", "doc (pages 3-4).pdf"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn split_from_pages_panel_context_menu() {
    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    let thumb = h.get_all_by_label("Page 3").max_by(|a, b| a.rect().left().total_cmp(&b.rect().left())).unwrap();
    thumb.click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Split document…").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Split));
    assert_eq!(h.state().split_draft.mode, pdfcraft_ui_egui::SplitMode::Selection);
    h.get_by_label_contains("Creates 2 files from 4 pages");
}

#[test]
fn split_from_pages_panel_context_menu_on_current_page_without_selection() {
    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    // Navigate to Page 2 (0-indexed 1) so it is current, but keep selected empty
    h.state_mut().views[0].go_to_page(1);
    h.state_mut().views[0].selected.clear();
    h.run_steps(2);
    assert_eq!(h.state().views[0].current, 1);
    assert!(h.state().views[0].selected.is_empty());
    let thumb = h.get_all_by_label("Page 2").max_by(|a, b| a.rect().left().total_cmp(&b.rect().left())).unwrap();
    thumb.click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Split document…").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Split));
    assert_eq!(h.state().split_draft.mode, pdfcraft_ui_egui::SplitMode::Selection);
    h.get_by_label_contains("Creates 2 files from 4 pages");
}

#[test]
fn split_from_pages_panel_context_menu_on_page_one_falls_back_to_pages() {
    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    let thumb = h.get_all_by_label("Page 1").max_by(|a, b| a.rect().left().total_cmp(&b.rect().left())).unwrap();
    thumb.click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Split document…").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Split));
    assert_eq!(h.state().split_draft.mode, pdfcraft_ui_egui::SplitMode::Pages);
}

#[test]
fn split_from_pages_panel_context_menu_with_multi_selection() {
    let mut h = harness(4, |app| app.right = Some(pdfcraft_ui_egui::RightPanel::Pages));
    // Select pages 2 and 4 (indices 1 and 3)
    h.state_mut().views[0].select_pages(&[1, 3]);
    h.run_steps(2);
    let thumb = h.get_all_by_label("Page 2").max_by(|a, b| a.rect().left().total_cmp(&b.rect().left())).unwrap();
    thumb.click_button(egui::PointerButton::Secondary);
    h.run_steps(2);
    h.get_all_by_label("Split document…").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Split));
    assert_eq!(h.state().split_draft.mode, pdfcraft_ui_egui::SplitMode::Selection);
    h.get_by_label_contains("Creates 3 files from 4 pages");
}

#[test]
fn launch_display_override_survives_password_retry_without_changing_defaults() {
    use pdfcraft_ui_egui::canvas::PageLayout;
    use std::io::Write as _;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = temp_path(&format!("startup-layout-{nonce}.pdf"));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    file.write_all(&protected("reader", "owner", -4)).unwrap();
    drop(file);
    let mut app = PdfCraftApp::new();
    app.set_option("default-layout", "continuous").unwrap();
    app.open_path_with_display(path.to_str().unwrap(), PageLayout::TwoUp, false);
    assert!(app.views.is_empty() && app.password_prompt.is_some());
    app.submit_password(Some("wrong".into()));
    assert!(app.views.is_empty() && app.password_prompt.is_some());
    app.submit_password(Some("reader".into()));
    assert_eq!(app.views.len(), 1);
    assert_eq!(app.views[0].layout, PageLayout::TwoUp);
    assert!(!app.views[0].continuous());
    let persisted: serde_json::Value = serde_json::from_str(&app.persist()).unwrap();
    assert_eq!(persisted["default_layout"], "continuous");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn asynchronous_launch_display_override_survives_retries_and_cancel() {
    asynchronous_launch_display(false);
}

#[test]
fn asynchronous_launch_display_explicit_layout_survives_retries_and_cancel() {
    asynchronous_launch_display(true);
}

#[test]
fn canceling_launch_display_override_does_not_affect_later_documents() {
    use pdfcraft_ui_egui::canvas::PageLayout;
    use std::io::Write as _;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = temp_path(&format!("startup-cancel-layout-{nonce}.pdf"));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
    file.write_all(&protected("reader", "owner", -4)).unwrap();
    drop(file);
    let mut app = PdfCraftApp::new();
    app.set_option("default-layout", "continuous").unwrap();
    app.open_path_with_display(path.to_str().unwrap(), PageLayout::TwoUp, false);
    assert!(app.views.is_empty() && app.password_prompt.is_some());
    app.submit_password(None);
    assert!(app.views.is_empty() && app.password_prompt.is_none());
    app.open_bytes("next.pdf", None, fixture(2)).unwrap();
    assert_eq!(app.views[0].layout, PageLayout::Single);
    assert!(app.views[0].continuous());
    // Reopening the same protected file normally must not resurrect the canceled override.
    app.open_path(path.to_str().unwrap());
    app.submit_password(Some("reader".into()));
    assert_eq!(app.views.len(), 2);
    assert_eq!(app.views[1].layout, PageLayout::Single);
    assert!(app.views[1].continuous());
    let persisted: serde_json::Value = serde_json::from_str(&app.persist()).unwrap();
    assert_eq!(persisted["default_layout"], "continuous");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn password_prompt_opens_and_security_tab_reports_the_details() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("secret.pdf", None, protected("pw", "owner", -1)).unwrap();
        app
    });
    h.run_steps(4);
    h.get_by_label_contains("is protected");
    let field = h.get_by_role(Role::PasswordInput);
    field.focus();
    field.type_text("pw");
    h.run_steps(2);
    h.key_press(Key::Enter);
    support::wait_until(&mut h, "unlocked document", |app| app.views.len() == 1);
    assert_eq!(h.state().views.len(), 1);
    h.state_mut().set_option("dialog", "properties").unwrap();
    h.run_steps(2);
    h.get_by_label("Security").click();
    h.run_steps(3);
    h.get_by_label("AES, 256-bit");
    h.get_by_label("User password");
}

/// #785: the notice's message used to be laid out before its buttons, so in a narrow document
/// area (a side panel open) it ran under "Security settings" and on into the panel, and it sat
/// off the buttons' centre line.
#[test]
fn the_security_notice_wraps_beside_its_buttons_and_lines_up_with_them() {
    // Wide: one line. Narrower: wrapped. Narrowest: cut short (the full text is on hover).
    for (width, wraps) in [(1700.0, false), (1250.0, true), (900.0, false)] {
        let mut h = Harness::builder().with_size(egui::vec2(width, 800.0)).build_eframe(|_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("locked.pdf", None, protected("", "owner", 0b0100)).unwrap();
            app.set_option("panel", "bookmarks").unwrap();
            app
        });
        h.run_steps(4);
        let msg = h.get_by_label_contains("This document is secured").rect();
        let button = h.get_by_label("Security settings").rect();
        let dismiss = h.get_by_label("Dismiss").rect();
        assert!(msg.right() <= button.left() + 0.5, "{width}: the message stops before the buttons: {msg:?} vs {button:?}");
        assert!(button.right() <= dismiss.left() + 0.5, "{width}: {button:?} vs {dismiss:?}");
        assert!((button.center().y - dismiss.center().y).abs() <= 0.5, "{width}: the buttons share a centre line");
        assert!(msg.top() >= button.top(), "{width}: the message stays inside the bar: {msg:?} vs {button:?}");
        let lines = (msg.height() / 14.0).floor().max(1.0);
        let first_line = msg.top() + msg.height() / lines / 2.0;
        assert!((first_line - button.center().y).abs() <= 1.5, "{width}: first line centred on the buttons: {msg:?} vs {button:?}");
        assert_eq!(msg.height() > button.height(), wraps, "{width}: {msg:?}");
    }
}

/// #995, #1077: a file with only a permissions password opens without asking, restricted. Its
/// notice offers Unlock…, which takes the permissions password and lifts the restrictions.
#[test]
fn the_security_notice_unlocks_with_the_permissions_password() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("locked.pdf", None, protected("", "owner", 0b0100)).unwrap();
        app
    });
    h.run_steps(4);
    h.get_by_label_contains("This document is secured");
    h.get_by_label("Unlock…").click();
    h.run_steps(2);
    h.get_by_label_contains("Enter its permissions password");
    type_password(&mut h, "wrong");
    h.get_by_label("Incorrect password. Try again.");
    type_password(&mut h, "owner");
    assert!(h.state().unlock_prompt.is_none());
    assert!(h.query_by_label_contains("This document is secured").is_none(), "the restrictions are lifted");
    h.state_mut().set_option("dialog", "properties").unwrap();
    h.run_steps(2);
    h.get_by_label("Security").click();
    h.run_steps(3);
    h.get_by_label("Owner password (no restrictions apply)");
    assert!(!h.state().session.get(h.state().active_ids().unwrap().1).unwrap().dirty, "unlocking is not an edit");
}

/// #4035: the secured-document notice can be turned off for good. "Don't show again" hides it
/// in every document, the choice is kept in the settings and turned back on in Preferences (or
/// `--secured-notice`); the document stays restricted and Properties ▸ Security still opens.
#[test]
fn the_security_notice_can_be_turned_off_for_good() {
    let open = |settings: Option<String>| {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            if let Some(s) = &settings {
                app.restore(s);
            }
            app.set_option("language", "en").unwrap();
            app.open_bytes("locked.pdf", None, protected("", "owner", 0b0100)).unwrap();
            app
        });
        h.run_steps(4);
        h
    };
    let mut h = open(None);
    assert!(h.state().show_secured_notice, "on by default");
    h.get_by_label_contains("This document is secured");
    h.get_by_label("Don't show again").click();
    h.run_steps(2);
    assert!(h.query_by_label_contains("This document is secured").is_none());
    assert!(!h.state().show_secured_notice);
    let id = h.state().active_ids().unwrap().1;
    assert!(h.state().session.get(id).unwrap().security_summary().is_some_and(|s| !s.owner), "still restricted");
    h.state_mut().set_option("dialog", "properties").unwrap();
    h.run_steps(2);
    h.get_by_label("Security").click();
    h.run_steps(3);
    h.get_by_label("Password security");
    h.get_by_label("User password");
    // Kept in the settings: a later session opens the same file without the notice.
    let saved = h.state().persist();
    let mut h = open(Some(saved));
    assert!(h.query_by_label_contains("This document is secured").is_none(), "remembered across a restart");
    // Back on from Preferences.
    h.state_mut().set_option("dialog", "preferences").unwrap();
    h.run_steps(2);
    h.get_by_role_and_label(Role::CheckBox, "Show a notice when a document is secured").scroll_to_me();
    h.run_steps(4);
    h.get_by_role_and_label(Role::CheckBox, "Show a notice when a document is secured").click();
    h.run_steps(2);
    assert!(h.state().show_secured_notice);
    h.state_mut().dialog = None;
    h.run_steps(2);
    h.get_by_label_contains("This document is secured");
    // Scriptable, with a clear refusal for anything else.
    let app = h.state_mut();
    app.set_option("secured-notice", "false").unwrap();
    assert!(!app.show_secured_notice);
    assert_eq!(app.set_option("secured-notice", "maybe"), Err("secured-notice must be true or false".into()));
}

/// Changing a restricted document's security asks for its permissions password first (Acrobat
/// does too), then carries on.
#[test]
fn remove_security_asks_for_the_permissions_password_first() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("locked.pdf", None, protected("", "owner", 0b0100)).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("protect.remove"));
    h.run_steps(2);
    assert!(h.state().unlock_prompt.is_some(), "asks for the password");
    // Cancelling changes nothing.
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().unlock_prompt.is_none());
    let id = h.state().active_ids().unwrap().1;
    assert!(!h.state().session.get(id).unwrap().dirty);
    assert!(h.state_mut().execute("protect.remove"));
    h.run_steps(2);
    type_password(&mut h, "owner");
    let doc = h.state().session.get(id).unwrap();
    assert!(doc.dirty && doc.security_summary().is_none(), "security is removed (written by the next save)");
}

/// A secured form still says it is one and offers Highlight fields: the security notice joins
/// the form's instead of replacing it.
#[test]
fn a_secured_form_shows_its_form_notice_and_its_security() {
    // Changing and assembling withheld, filling allowed (as government forms are often set).
    let bytes = encrypted(include_bytes!("../../data/form.pdf").to_vec(), pdfcraft_cos::Algorithm::Aes256, "", "owner", -1036);
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    h.get_by_label_contains("interactive form fields. This document is secured. Some changes are restricted by its security settings.");
    h.get_by_label("Hide field highlights");
    h.get_by_label("Security settings");
    h.get_by_label("Unlock…");
}

#[test]
fn restricted_documents_show_a_notice_and_block_page_changes() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("locked.pdf", None, protected("", "owner", 0b0100)).unwrap(); // opens without a password
        app.set_option("organize", "on").unwrap();
        app
    });
    h.run_steps(4);
    h.get_by_label_contains("This document is secured");
    h.get_by_label("Page 1").click();
    h.run_steps(1);
    h.get_by_label("Delete pages (Delete)").click();
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(texts_of(h.state(), 0).len(), 2, "page changes are blocked");
    assert_eq!(h.query_all_by_label_contains("Insert a file").count(), 0, "nothing to insert into");
    // Looking closer is not a page change.
    h.get_by_label("Larger pages").click();
    h.run_steps(2);
    assert_eq!(grid_zoom(&h), 1.25);
    assert!(!dirty(&h));
    h.get_by_label("Security settings").click();
    h.run_steps(3);
    assert!(h.query_all_by_label("Not allowed").count() >= 4);
}

/// Table 22, bit 5 (#568): a document whose security withholds copying keeps its text and images
/// in. ⌘C, snapshots and exports are refused with a notice; the owner password lifts that.
#[test]
fn copying_out_of_a_document_honours_its_security() {
    fn open(password: &'static str) -> Harness<'static, PdfCraftApp> {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.system_clipboard = false;
            app.open_bytes("no-copy.pdf", None, protected("pw", "owner", -1 ^ 0b1_0000)).unwrap();
            app.submit_password(Some(password.into()));
            // One page at a time: Select All takes that page's text (scrolling continuously it would
            // take both pages', #2639; what is tested here is the security of copying it).
            if let Some(v) = app.views.first_mut() {
                v.set_continuous(false);
            }
            app
        });
        // Select the page's text once its text layer is loaded (up to ~4 s: a loaded test machine
        // can take far longer than the 0.2 s this used to allow).
        for _ in 0..200 {
            h.run_steps(2);
            if h.state_mut().views[0].select_all() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(h.state().views[0].selected_text().as_deref(), Some("Page 1"));
        h
    }
    fn copy(h: &mut Harness<'static, PdfCraftApp>) -> bool {
        h.state_mut().toast = None;
        h.event(egui::Event::Copy);
        let mut copied = false;
        for _ in 0..4 {
            h.step();
            copied |= h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "Page 1"));
        }
        copied
    }
    fn snapshot(h: &mut Harness<'static, PdfCraftApp>) {
        // Around "Page 1", at (20, 150) on the 200 × 300 pt page.
        let r = h.state().views[0].page_screen_rect(0).expect("on screen");
        let at = |x: f32, y: f32| egui::pos2(r.left() + x / 200.0 * r.width(), r.top() + (300.0 - y) / 300.0 * r.height());
        let (a, b) = (at(15.0, 175.0), at(120.0, 140.0));
        h.hover_at(a);
        h.run_steps(1);
        h.drag_at(a);
        h.run_steps(1);
        h.hover_at(b);
        h.run_steps(1);
        h.drop_at(b);
        h.run_steps(3);
    }
    let refused = Some("The document's security settings don't allow copying text and images");

    let mut h = open("pw");
    assert!(!copy(&mut h), "⌘C copies nothing");
    assert_eq!(h.state().toast.as_ref().map(|t| t.0.as_str()), refused);
    for command in ["edit.snapshot", "export.image", "export.text", "export.docx", "export.pptx", "export.html", "export.rtf", "export.all_images"] {
        h.state_mut().toast = None;
        assert!(!h.state_mut().execute(command), "{command} is refused");
        assert_eq!(h.state().toast.as_ref().map(|t| t.0.as_str()), refused, "{command} says why");
    }
    assert_eq!(h.state().dialog, None, "no export dialog");
    // The snapshot tool, chosen on another tab, takes nothing here either.
    h.state_mut().set_option("quick", "snapshot").unwrap();
    snapshot(&mut h);
    assert!(h.state().last_snapshot.is_none());
    assert_eq!(h.state().toast.as_ref().map(|t| t.0.as_str()), refused);

    let mut h = open("owner");
    assert!(copy(&mut h), "the owner may copy");
    assert!(h.state_mut().execute("edit.snapshot"));
    snapshot(&mut h);
    assert!(h.state().last_snapshot.is_some());
}

#[test]
fn replace_pages_dialog_swaps_page_content() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.open_bytes("doc.pdf", None, fixture(3)).unwrap();
    app.views[0].select_pages(&[1]);
    app.start_replace("other.pdf".into(), fixture(5));
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| app);
    h.run_steps(3);
    egui_kittest::kittest::Queryable::get_by_label(&h, "Replace Pages");
    h.state_mut().replace_draft.as_mut().unwrap().src_from = 5;
    h.run_steps(1);
    egui_kittest::kittest::Queryable::get_by_label(&h, "OK").click();
    h.run_steps(3);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.info.pages.len(), 3);
    assert_eq!(doc.can_undo(), Some("Replace page"));
}

/// Pages 1 and 3 are selected. OK replaces those two and leaves the unchecked page between them.
#[test]
fn replace_split_selection_keeps_the_unchecked_page() {
    fn sized(n: usize, width: u32, height: u32, label: &str) -> Vec<u8> {
        let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
        objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 {width} {height}] >>", kids.join(" ")));
        objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
        for i in 0..n {
            objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
            let body = format!("BT /F1 24 Tf 20 150 Td ({label} {}) Tj ET", i + 1);
            objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
        }
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for o in offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }
    let open = |selected: &[usize], edited: bool| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("original.pdf", None, sized(3, 200, 300, "Original")).unwrap();
        app.views[0].select_pages(selected);
        app.start_replace("other.pdf".into(), sized(5, 400, 500, "Replacement"));
        if edited {
            app.replace_draft.as_mut().unwrap().range_edited = true;
        }
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| app);
        h.run_steps(2);
        h
    };
    let sizes = |app: &PdfCraftApp| {
        let doc = app.session.get(app.views[0].id).unwrap();
        doc.info.pages.iter().map(|p| (p.width.round() as i32, p.height.round() as i32)).collect::<Vec<_>>()
    };
    let mut split = open(&[0, 2], false);
    let draft = split.state().replace_draft.as_ref().unwrap();
    assert_eq!((draft.from, draft.to, draft.pages.as_slice(), draft.range_edited), (1, 3, &[0usize, 2][..], false));
    egui_kittest::kittest::Queryable::get_by_label(&split, "OK").click();
    split.run_steps(3);
    assert_eq!(texts_of(split.state(), 0), ["Replacement 1", "Original 2", "Replacement 2"]);
    assert_eq!(sizes(split.state()), [(400, 500), (200, 300), (400, 500)]);
    assert_eq!(split.state().session.get(split.state().views[0].id).unwrap().can_undo(), Some("Replace pages"));
    split.state_mut().undo();
    split.run_steps(2);
    assert_eq!(texts_of(split.state(), 0), ["Original 1", "Original 2", "Original 3"]);
    assert_eq!(sizes(split.state()), [(200, 300), (200, 300), (200, 300)]);

    let mut span = open(&[0, 2], true);
    egui_kittest::kittest::Queryable::get_by_label(&span, "OK").click();
    span.run_steps(3);
    assert_eq!(texts_of(span.state(), 0), ["Replacement 1", "Replacement 2", "Replacement 3"]);

    let mut adjacent = open(&[1, 2], false);
    egui_kittest::kittest::Queryable::get_by_label(&adjacent, "OK").click();
    adjacent.run_steps(3);
    assert_eq!(texts_of(adjacent.state(), 0), ["Original 1", "Replacement 1", "Replacement 2"]);
    assert_eq!(sizes(adjacent.state()), [(200, 300), (400, 500), (400, 500)]);
}

#[test]
fn extract_options_and_rotate_pages_dialog() {
    let dir = temp_path("extract-sep");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = organize(4);
    h.state_mut().export_dir_override = Some(dir.to_string_lossy().into_owned());
    h.state_mut().views[0].select_pages(&[1, 2]);
    h.state_mut().run_command("page.extract");
    h.run_steps(2);
    h.state_mut().extract_draft = pdfcraft_ui_egui::ExtractDraft { separate: true, delete: true, ..Default::default() };
    h.get_all_by_label("Extract").last().unwrap().click();
    h.run_steps(3);
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["doc (page 2).pdf", "doc (page 3).pdf"]);
    assert_eq!(texts_of(h.state(), 0), ["Page 1", "Page 4"], "deleted after extracting");
    let _ = std::fs::remove_dir_all(dir);
    // Rotate Pages: odd page numbers only.
    h.state_mut().run_command("page.rotate_dialog");
    h.run_steps(2);
    h.state_mut().rotate_draft.which = 0;
    h.state_mut().rotate_draft.parity = pdfcraft_engine::PageParity::Odd;
    h.get_by_label("OK").click();
    h.run_steps(3);
    let s = h.state();
    let d = s.session.get(s.views[0].id).unwrap();
    assert_eq!(d.info.pages.iter().map(|p| p.rotation).collect::<Vec<_>>(), [90, 0]);
}

/// #5579: typing the last page of Rotate Pages' range keeps what is typed (croquete1): "12" after
/// a first page of 5 rotates pages 5–12, not page 5 alone. An end typed below the start and left
/// by clicking OK is raised to the start as the field lets go: page 5, not nothing.
#[test]
fn rotate_pages_keeps_a_typed_last_page() {
    use egui::accesskit::Role;
    for (typed, rotated) in [("12", 5..13), ("3", 5..6)] {
        let mut h = organize(20);
        h.state_mut().run_command("page.rotate_dialog");
        h.run_steps(2);
        let d = &mut h.state_mut().rotate_draft;
        (d.which, d.from, d.to) = (2, 5, 5);
        h.run_steps(1);
        h.get_by_role_and_label(Role::SpinButton, "to").click();
        h.run_steps(2);
        // As events, not `key_press`: its `run` checks the dialog's unnamed combo boxes.
        h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND });
        for c in typed.chars() {
            h.event(egui::Event::Text(c.to_string()));
            h.run_steps(2);
        }
        // OK at once, the end still being typed.
        h.get_by_label("OK").click();
        h.run_steps(3);
        let s = h.state();
        let d = s.session.get(s.views[0].id).unwrap();
        let turned: Vec<usize> = d.info.pages.iter().enumerate().filter(|(_, p)| p.rotation != 0).map(|(i, _)| i + 1).collect();
        assert_eq!(turned, rotated.collect::<Vec<usize>>(), "typed {typed:?}");
    }
}

#[test]
fn dragging_thumbnails_reorders_pages() {
    let mut h = organize(4);
    let before = page_texts(h.state());
    let grab = |h: &Harness<'static, PdfCraftApp>, label: &str| h.get_by_label(label).rect();
    let (from, to) = (grab(&h, "Page 1").center(), grab(&h, "Page 3").right_center() - egui::vec2(10.0, 0.0));
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    for k in 1..=5 {
        h.hover_at(from + (to - from) * (k as f32 / 5.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(4);
    // Page 1 now sits after page 3.
    let after = page_texts(h.state());
    assert_eq!(after, vec![before[1].clone(), before[2].clone(), before[0].clone(), before[3].clone()], "{after:?}");
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Move page"));
    let sel: Vec<usize> = h.state().views[0].selected.iter().copied().collect();
    assert_eq!(sel, vec![2], "the moved page stays selected");
}
