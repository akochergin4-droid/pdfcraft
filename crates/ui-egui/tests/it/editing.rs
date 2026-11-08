//! Headless UI tests for editing and saving: the organize toolbar, selection, undo/redo keys,
//! save/save-as, the unsaved-changes prompt and editable document properties.

use crate::support;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_render::{PageRenderer, RenderRequest, RequestKind};
use pdfcraft_ui_egui::{CloseRequest, Dialog, OpenOutcome, PdfCraftApp};

use crate::gpu;

// The tests are split by position, not by topic, so that no file runs to ten thousand lines.
// Shared fixtures and helpers stay in this file; each part reaches them through `use super::*`.
mod editing_part1;
mod editing_part2;
mod editing_part3;
mod editing_part4;
mod editing_part5;
mod editing_part6;
mod editing_part7;

/// An `n`-page document with a proper xref table; page `i` shows "Page i+1".
fn fixture(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for i in 0..n {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
        let body = format!("BT /F1 24 Tf 20 150 Td (Page {}) Tj ET", i + 1);
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

fn harness(pages: usize, setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if std::env::var_os("PDFCRAFT_ORGANIZE_SNAPSHOT").is_some() {
        builder = builder.wgpu();
    }
    let mut h = builder.build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, fixture(pages)).expect("fixture opens");
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

fn organize(pages: usize) -> Harness<'static, PdfCraftApp> {
    harness(pages, |app| app.set_option("organize", "on").unwrap())
}

/// Organize pages ▸ Insert ▸ `item`.
fn insert_menu(h: &mut Harness<'static, PdfCraftApp>, item: &str) {
    h.get_by_label("Insert").click();
    h.run_steps(2);
    h.get_by_label(item).click();
}

fn type_selection(h: &mut Harness<'static, PdfCraftApp>, text: &str) {
    use egui::accesskit::Role;
    h.get_by_role_and_label(Role::TextInput, "Selected pages").click();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::TextInput, "Selected pages").type_text(text);
    h.run_steps(2);
}

/// Page labels of the active document, read back from its current bytes.
fn page_texts(app: &PdfCraftApp) -> Vec<String> {
    let doc = app.session.get(app.views[0].id).unwrap();
    let mut r = PageRenderer::new(doc.bytes.clone(), Default::default());
    (0..r.page_count())
        .map(|p| {
            let out = r.render(RenderRequest { page: p, kind: RequestKind::Text, scale: 1.0, ..Default::default() });
            out.text.map(|t| t.plain_text().trim().to_string()).unwrap_or_default()
        })
        .collect()
}

fn dirty(h: &Harness<'static, PdfCraftApp>) -> bool {
    let app = h.state();
    app.session.get(app.views[0].id).is_some_and(|d| d.dirty)
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfcraft-ui-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// An organize harness with a dirty document and the "Save changes?" prompt open.
fn dirty_prompt() -> Harness<'static, PdfCraftApp> {
    let mut h = organize(2);
    h.get_by_label("Rotate clockwise").click();
    h.run_steps(3);
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run_steps(3);
    h.get_by_label_contains("Save changes to “doc.pdf”");
    h
}

/// The prompt has Save chosen on arrival; `keys` then move the choice and Enter or Space presses it.
fn press_after(keys: &[(Modifiers, Key)], activate: Key) -> Harness<'static, PdfCraftApp> {
    let mut h = dirty_prompt();
    for (m, k) in keys {
        h.key_press_modifiers(*m, *k);
        h.run_steps(2);
    }
    h.key_press(activate);
    h.run_steps(3);
    h
}

/// Run `frames` frames and collect every attention request they sent to the window.
fn attention_over(h: &mut Harness<'static, PdfCraftApp>, frames: usize) -> Vec<egui::UserAttentionType> {
    let mut sent = Vec::new();
    for _ in 0..frames {
        h.step();
        let commands = h.output().viewport_output.values().flat_map(|v| v.commands.iter());
        sent.extend(commands.filter_map(|c| if let egui::ViewportCommand::RequestUserAttention(a) = c { Some(*a) } else { None }));
    }
    sent
}

/// Close the window as the operating system does: Quit in the Dock menu, or logging out.
fn close_window(h: &mut Harness<'static, PdfCraftApp>) {
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().events.push(egui::ViewportEvent::Close);
}

/// PDFs written to a fresh temp folder, removed when the test ends.
struct TempPdfs(std::path::PathBuf);

impl TempPdfs {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("pdfcraft-session-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, pages: usize) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, fixture(pages)).unwrap();
        path.to_string_lossy().into_owned()
    }
}

impl Drop for TempPdfs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The app with Preferences ▸ Reopen the files that were open when PdfCraft last closed on.
fn session_harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.reopen_last_session = true;
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

fn open_paths(app: &PdfCraftApp) -> Vec<String> {
    app.views.iter().map(|v| app.session.get(v.id).and_then(|d| d.path.clone()).unwrap_or_default()).collect()
}

fn custom_properties(h: &Harness<'static, PdfCraftApp>) -> Vec<(String, String)> {
    let app = h.state();
    app.session.get(app.views[0].id).unwrap().custom_info()
}

fn type_into(h: &mut Harness<'static, PdfCraftApp>, label: &str, text: &str) {
    let field = h.get_by_role_and_label(Role::TextInput, label);
    field.focus();
    field.type_text(text);
    h.run_steps(2);
}

fn clear(h: &mut Harness<'static, PdfCraftApp>, label: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.key_press(Key::Backspace);
    h.run_steps(2);
}

// ── Combine / insert from file / extract / split ──────────────────────────────────────────────

/// A one-page PDF whose form holds one text field named `name`.
fn form_pdf(name: &str) -> Vec<u8> {
    form_pdf_pages(name, 1)
}

/// A PDF of `pages` pages whose form holds one text field named `name` (on the first page).
fn form_pdf_pages(name: &str, pages: usize) -> Vec<u8> {
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 3 + i)).collect();
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>".into(),
        format!("<< /Type /Pages /Kids [{}] /Count {pages} /MediaBox [0 0 300 400] >>", kids.join(" ")),
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".into(),
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V (x) /Rect [60 60 200 80] /P 3 0 R >>"),
    ];
    for _ in 1..pages {
        objs.push("<< /Type /Page /Parent 2 0 R >>".to_string()); // objects after the field's number
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

fn texts_of(app: &PdfCraftApp, tab: usize) -> Vec<String> {
    let doc = app.session.get(app.views[tab].id).unwrap();
    let mut r = PageRenderer::new(doc.bytes.clone(), Default::default());
    (0..r.page_count())
        .map(|p| {
            let out = r.render(RenderRequest { page: p, kind: RequestKind::Text, scale: 1.0, ..Default::default() });
            out.text.map(|t| t.plain_text().trim().to_string()).unwrap_or_default()
        })
        .collect()
}

fn combine_names(h: &Harness<'static, PdfCraftApp>) -> Vec<String> {
    h.state().combine_draft.iter().map(|f| f.name.clone()).collect()
}

/// Files in the Combine tab's list view (the table: headings, page ranges, row links).
fn combine_of(names: &[(&str, usize)]) -> Harness<'static, PdfCraftApp> {
    let files: Vec<(String, Vec<u8>)> = names.iter().map(|(n, p)| (n.to_string(), fixture(*p))).collect();
    let mut h = harness(1, move |app| {
        app.set_option("combine-view", "list").unwrap();
        app.use_files(pdfcraft_ui_egui::FilePurpose::Combine, files);
    });
    h.run_steps(3);
    h
}

fn type_password(h: &mut Harness<'static, PdfCraftApp>, password: &str) {
    let field = h.get_by_role(Role::PasswordInput);
    field.focus();
    field.type_text(password);
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

/// A file dropped on the window, as the windowing layer hands it over.
#[derive(Debug)]
struct Dropped(&'static str, Vec<u8>);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        std::path::Path::new(self.0)
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.1.clone())
    }
}

fn drop_files(h: &mut Harness<'static, PdfCraftApp>, files: Vec<Dropped>) {
    for f in files {
        h.input_mut().dropped_files.push(std::sync::Arc::new(f));
    }
}

/// The Pages panel's thumbnail of page `label` (the panel is right of the page view).
fn panel_thumb_rect(h: &Harness<'static, PdfCraftApp>, label: &str) -> egui::Rect {
    h.get_all_by_label(label).map(|n| n.rect()).max_by(|a, b| a.left().total_cmp(&b.left())).unwrap()
}

fn grid_zoom(h: &Harness<'static, PdfCraftApp>) -> f32 {
    h.state().views[0].grid_zoom()
}

// ── Encrypted documents ───────────────────────────────────────────────────────────────────────

fn protected(user: &str, owner: &str, permissions: i32) -> Vec<u8> {
    protected_with(pdfcraft_cos::Algorithm::Aes256, user, owner, permissions)
}

fn protected_with(algorithm: pdfcraft_cos::Algorithm, user: &str, owner: &str, permissions: i32) -> Vec<u8> {
    encrypted(fixture(2), algorithm, user, owner, permissions)
}

/// `bytes` rewritten encrypted.
pub(crate) fn encrypted(bytes: Vec<u8>, algorithm: pdfcraft_cos::Algorithm, user: &str, owner: &str, permissions: i32) -> Vec<u8> {
    let mut doc = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    doc.set_encryption(&pdfcraft_cos::NewEncryption {
        algorithm,
        user_password: user,
        owner_password: owner,
        permissions,
        encrypt_metadata: true,
        seed: [4; 32],
    })
    .unwrap();
    pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()
}

fn asynchronous_launch_display(explicit: bool) {
    use pdfcraft_ui_egui::canvas::PageLayout;
    fn wait(h: &mut Harness<'static, PdfCraftApp>, stage: &str, ready: impl Fn(&PdfCraftApp) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready(h.state()) {
            assert!(
                std::time::Instant::now() < deadline,
                "{stage}: views={}, prompt={}, error={}, notice={}",
                h.state().views.len(),
                h.state().password_prompt.is_some(),
                h.state().password_prompt.as_ref().is_some_and(|p| p.error.is_some()),
                h.state().toast.is_some()
            );
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let path = temp_path(&format!("async-startup-layout-{explicit}.pdf"));
    std::fs::write(&path, protected("reader", "owner", -4)).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_option("default-layout", "continuous").unwrap();
        app.open_bytes("other.pdf", None, fixture(2)).unwrap();
        app
    });
    h.run_steps(2);
    assert!(!h.state().run_inline, "exercise upstream's actual background-opening path");
    let other = h.state().views[0].id;
    h.state_mut().open_path_with_display(path.to_str().unwrap(), PageLayout::TwoUp, false);
    if explicit {
        h.state_mut().set_launch_layout("single").unwrap();
    }
    wait(&mut h, "password prompt", |app| app.password_prompt.is_some());
    // The existing document keeps its default view while this request is pending.
    h.state_mut().submit_password(Some("wrong".into()));
    wait(&mut h, "wrong-password result", |app| app.password_prompt.as_ref().is_some_and(|p| p.error.is_some()));
    assert_eq!(h.state().views.len(), 1);
    h.state_mut().submit_password(Some("reader".into()));
    wait(&mut h, "first unlock", |app| app.views.len() == 2);
    assert_eq!(h.state().views[0].id, other);
    assert_eq!(h.state().views[0].layout, PageLayout::Single);
    assert!(h.state().views[0].continuous());
    assert_eq!(h.state().views[1].layout, if explicit { PageLayout::Single } else { PageLayout::TwoUp });
    assert!(!h.state().views[1].continuous());
    let second_path = path.with_extension("second.pdf");
    std::fs::copy(&path, &second_path).unwrap();
    h.state_mut().open_path_with_display(second_path.to_str().unwrap(), PageLayout::TwoUp, false);
    wait(&mut h, "password prompt", |app| app.password_prompt.is_some());
    h.state_mut().submit_password(None);
    h.state_mut().open_path(second_path.to_str().unwrap());
    wait(&mut h, "password prompt", |app| app.password_prompt.is_some());
    h.state_mut().submit_password(Some("reader".into()));
    wait(&mut h, "normal unlock after cancel", |app| app.views.len() == 3);
    assert_eq!(h.state().views[2].layout, PageLayout::Single);
    assert!(h.state().views[2].continuous());
    let persisted: serde_json::Value = serde_json::from_str(&h.state().persist()).unwrap();
    assert_eq!(persisted["default_layout"], "continuous");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(second_path).unwrap();
}

/// A page drag with the copy modifier held — Ctrl (Option on macOS) — instead of none.
/// Modifiers are input state: `ModifiersChanged` once, as the column-select drag does.
fn copy_drag(h: &mut Harness<'static, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2) {
    page_drag(h, from, to, copy_modifiers(h));
}

/// The copy modifier of the OS the app reports: Option on macOS, Ctrl elsewhere. That is the OS
/// the gesture itself reads, so a Mac browser — a wasm build, where `cfg!(target_os = "macos")` is
/// false — is covered by asking the context rather than the target.
fn copy_modifiers(h: &Harness<'static, PdfCraftApp>) -> Modifiers {
    if h.ctx.os() == egui::os::OperatingSystem::Mac { Modifiers::ALT } else { Modifiers::CTRL }
}

/// Drag the page under `from` to `to` with `modifiers` held for the whole gesture.
fn page_drag(h: &mut Harness<'static, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2, modifiers: Modifiers) {
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.run_steps(1);
    h.hover_at(from);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers });
    h.run_steps(1);
    for k in 1..=5 {
        h.hover_at(from + (to - from) * (k as f32 / 5.0));
        h.run_steps(1);
    }
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers });
    h.run_steps(4);
    h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

fn source_font_fixture() -> Vec<u8> {
    source_font_fixture_with_text("BT /F1 18 Tf 20 220 Td (Serif) Tj ET BT /F2 18 Tf 20 120 Td (Mono) Tj ET")
}

fn source_font_fixture_with_text(body: &str) -> Vec<u8> {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 300 300] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-BoldItalic >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 7 0 R /Resources << /Font << /F1 3 0 R /F2 5 0 R >> >> >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Oblique >>".into(),
        "<< /Producer (PdfCraft) >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
    ];
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

fn open_source_font_fixture() -> Harness<'static, PdfCraftApp> {
    Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("fonts.pdf", None, source_font_fixture()).expect("font fixture opens");
        app
    })
}

/// Dark pixels in `r`: glyph ink on a white page.
fn ink(pixels: &image::RgbaImage, r: egui::Rect) -> usize {
    let (x0, y0, x1, y1) = (r.left().max(0.0) as u32, r.top().max(0.0) as u32, r.right() as u32, r.bottom() as u32);
    (y0..y1.min(pixels.height()))
        .flat_map(|y| (x0..x1.min(pixels.width())).map(move |x| (x, y)))
        .filter(|(x, y)| pixels.get_pixel(*x, *y).0[..3].iter().all(|v| *v < 90))
        .count()
}

/// Two boxes, "Top" and "Bottom", in Edit text, with egui's animations on: the window animates
/// unless the system asks for less motion, and the test harness turns them off.
fn two_boxes_animated() -> (Harness<'static, PdfCraftApp>, [egui::Rect; 2]) {
    let bytes = source_font_fixture_with_text("BT /F1 24 Tf 20 220 Td (Top) Tj ET BT /F2 24 Tf 20 120 Td (Bottom) Tj ET");
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_step_dt(1.0 / 60.0).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("boxes.pdf", None, bytes).expect("opens");
        app
    });
    h.ctx.all_styles_mut(|s| s.animation_time = 0.2);
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
    let on_screen = |b: &pdfcraft_engine::TextBlock| {
        egui::Rect::from_min_max(
            egui::pos2(r.left() + b.rect[0] as f32 * k, r.top() + (300.0 - b.rect[3] as f32) * k),
            egui::pos2(r.left() + b.rect[2] as f32 * k, r.top() + (300.0 - b.rect[1] as f32) * k),
        )
    };
    (h, [on_screen(&blocks[0]), on_screen(&blocks[1])])
}

/// The colour of the editor's frame (the accent stroke around `r`) this frame, if it is painted.
fn editor_frame_color(h: &Harness<'static, PdfCraftApp>, r: egui::Rect) -> Option<egui::Color32> {
    h.output().shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.stroke.width == 1.5 && rect.rect.contains(r.center()) => Some(rect.stroke.color),
        _ => None,
    })
}

/// One US Letter page of Helvetica, for driving the live editor on a known baseline.
fn letter_text(body: &str) -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn open_letter(name: &'static str, bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 1400.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes(name, None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    h
}

fn click_page(h: &mut Harness<'static, PdfCraftApp>, x: f32, y: f32) {
    let page = h.state().views[0].page_screen_rect(0).expect("the page is on screen");
    let at = egui::pos2(page.left() + x / 612.0 * page.width(), page.top() + (792.0 - y) / 792.0 * page.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(4);
}

fn save_live_shot(h: &mut Harness<'static, PdfCraftApp>, name: &str) {
    for _ in 0..40 {
        h.run_steps(2);
        if !h.state().render_pending() {
            break;
        }
    }
    let dir = std::path::Path::new("/tmp/pdfcraft-visual");
    let _ = std::fs::create_dir_all(dir);
    if let Ok(image) = h.render() {
        let _ = image.save(dir.join(name));
    }
}

/// Under the Edit tab, opens the line "Page 1" (at (20, 150) on a 200 × 300 page, 24 pt) in the
/// paragraph editor and retypes it as "Chapter One", leaving the editor open.
fn edit_tab_with_an_open_paragraph(h: &mut Harness<'static, PdfCraftApp>) {
    use pdfcraft_ui_egui::{Mode, QuickTool};
    support::outside_menus(h, "Edit").click();
    h.run_steps(3);
    assert_eq!((h.state().mode, h.state().quick_tool), (Mode::Edit, QuickTool::EditText));
    open_the_first_line(h);
}

/// With Edit text & images on, opens the line "Page 1" in the paragraph editor and retypes it as
/// "Chapter One", leaving the editor open.
fn open_the_first_line(h: &mut Harness<'static, PdfCraftApp>) {
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(3);
    assert_eq!(h.state().views[0].line_editor.as_ref().expect("the line opens for editing").text, "Page 1");
    h.state_mut().views[0].line_editor.as_mut().unwrap().text = "Chapter One".into();
    h.run_steps(1);
}

/// Edit was left with the paragraph [`edit_tab_with_an_open_paragraph`] opened: Edit text & images
/// is put down, and the paragraph's edit is applied instead of its editor staying open over the page.
fn assert_edit_text_put_down_and_the_paragraph_applied(s: &PdfCraftApp) {
    assert_eq!(s.quick_tool, pdfcraft_ui_egui::QuickTool::Select, "Edit text & images is put down");
    assert!(s.views[0].line_editor.is_none(), "no paragraph editor is left open");
    let doc = s.session.get(s.views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Edit text"), "the open paragraph's edit is applied");
    assert_eq!(doc.text_lines(0)[0].text, "Chapter One");
    assert_eq!(texts_of(s, 0), ["Chapter One"], "the page shows it");
}

/// One page whose only line is drawn twice at the same spot (fake bold, as many generated
/// documents do).
fn double_drawn() -> Vec<u8> {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >> >>".into(),
        format!(
            "<< /Length {} >>\nstream\nBT /F1 24 Tf 20 150 Td (Page 1) Tj ET BT /F1 24 Tf 20 150 Td (Page 1) Tj ET\nendstream",
            "BT /F1 24 Tf 20 150 Td (Page 1) Tj ET BT /F1 24 Tf 20 150 Td (Page 1) Tj ET".len()
        ),
    ];
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

fn check_existing_artwork(name: &'static str, bytes: Vec<u8>) {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes(name, None, bytes.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    let before = h.state().session.get(id).unwrap().page_images(0)[0].rect;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    // Select it, then drag it a quarter of the page to the right.
    let c = r.center();
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    h.drop_at(c);
    h.run_steps(2);
    assert!(h.state().views[0].image_selection.is_some(), "selected");
    let to = c + egui::vec2(r.width() / 4.0, 0.0);
    h.hover_at(c);
    h.run_steps(1);
    h.drag_at(c);
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(c + (to - c) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.drop_at(to);
    h.run_steps(4);
    let doc = h.state().session.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Move image"));
    let moved = doc.page_images(0)[0].rect;
    let dx = moved[0] - before[0];
    assert!((dx - (before[2] - before[0]) / 4.0).abs() < 1.5, "moved {dx} pt: {before:?} → {moved:?}");
    // Delete with the Delete key.
    h.key_press(egui::Key::Delete);
    h.run_steps(4);
    assert!(h.state().session.get(id).unwrap().page_images(0).is_empty());
}

/// Drag with the pointer from `from` to `to` in a few steps.
fn drag(h: &mut Harness<'static, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2) {
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
}

/// Press and release at `at` without moving.
fn click_at(h: &mut Harness<'static, PdfCraftApp>, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(4);
}

/// Sets the open crop's box on page 0 to the screen rectangle `r`, as dragging its handles would.
fn set_crop_box(h: &mut Harness<'static, PdfCraftApp>, r: egui::Rect) {
    let view = &h.state().views[0];
    let page = &h.state().session.get(view.id).unwrap().info.pages[0];
    let xf = pdfcraft_ui_egui::canvas::PageXform { rect: view.page_screen_rect(0).unwrap(), rot: view.rotation, pw: page.width, ph: page.height };
    let (a, b) = (xf.screen_to_view(r.min), xf.screen_to_view(r.max));
    let (u, v) = (page.view_to_user(a.0, a.1), page.view_to_user(b.0, b.1));
    let keep = [u[0].min(v[0]), u[1].min(v[1]), u[0].max(v[0]), u[1].max(v[1])].map(f64::from);
    h.state_mut().views[0].image_selection.as_mut().unwrap().crop.as_mut().unwrap().keep = keep;
}

fn press(h: &mut Harness<'static, PdfCraftApp>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run_steps(4);
}

/// A two-page document whose first page draws one picture (a Form XObject), open in Edit text
/// & images with the picture selected.
fn selected_figure() -> Harness<'static, PdfCraftApp> {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_incremental};
    let mut doc = Document::open(std::sync::Arc::new(fixture(2))).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut form = Dict::new();
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([0, 0, 200, 300].map(Object::Int).to_vec()));
    let artwork = doc.add(Object::Stream(Stream::flate(form, b"0.2 0.5 0.9 rg 40 200 120 80 re f")));
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
    let bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("figure.pdf", None, bytes.clone()).expect("opens");
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    click_at(&mut h, egui::pos2(r.left() + 100.0 / 200.0 * r.width(), r.top() + (300.0 - 240.0) / 300.0 * r.height()));
    assert!(h.state().views[0].image_selection.is_some(), "selected");
    h
}

/// Contributor-original text, with an off-origin CropBox. Keep the moving paragraph last in
/// source order so an ordinary hit picks it again when two independent boxes fully overlap.
fn paragraph_drag_fixture(content: &str, rotation: i64, unit: i64) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, Object, Stream};
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    doc.update_dict(page, |d| {
        d.set(b"Contents".to_vec(), Object::Ref(contents));
        d.set(b"MediaBox".to_vec(), Object::Array([0, 0, 400, 500].map(Object::Int).to_vec()));
        d.set(b"CropBox".to_vec(), Object::Array([20, 30, 380, 470].map(Object::Int).to_vec()));
        d.set(b"Rotate".to_vec(), Object::Int(rotation));
        d.set(b"UserUnit".to_vec(), Object::Int(unit));
    })
    .unwrap();
    pdfcraft_cos::write_incremental(&doc, &Default::default()).unwrap()
}

fn paragraph_drag_harness(bytes: Vec<u8>, view_rotation: u16) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.session = pdfcraft_engine::Session::new().with_clock(|| 1_700_000_000);
        app.set_option("language", "en").unwrap();
        app.open_bytes("paragraphs.pdf", None, bytes).unwrap();
        app.set_option("zoom", "75").unwrap();
        app.views[0].rotation = view_rotation;
        assert!(app.execute("edit.edit_text"));
        app
    });
    h.run_steps(4);
    h
}

fn dragged_paragraphs(h: &Harness<'static, PdfCraftApp>) -> Vec<pdfcraft_engine::TextBlock> {
    let app = h.state();
    app.session.get(app.views[0].id).unwrap().text_blocks(0)
}

/// Resolve the current inventory and transform for every gesture, never an old block index
/// or cached screen position. With identical labels, choose the last (topmost) source box.
fn paragraph_drag_points(h: &Harness<'static, PdfCraftApp>, text: &str, offset: [f64; 2]) -> (egui::Pos2, egui::Pos2) {
    let app = h.state();
    let view = &app.views[0];
    let doc = app.session.get(view.id).unwrap();
    let b = doc.text_blocks(0).into_iter().rfind(|b| b.text == text).unwrap();
    let xf = pdfcraft_ui_egui::canvas::PageXform {
        rect: view.page_screen_rect(0).unwrap(),
        rot: view.rotation,
        pw: doc.info.pages[0].width,
        ph: doc.info.pages[0].height,
    };
    let moved = std::array::from_fn(|i| (b.rect[i] + offset[i % 2]) as f32);
    (xf.user_rect(&doc.info, 0, b.rect.map(|v| v as f32)).center(), xf.user_rect(&doc.info, 0, moved).center())
}

fn drag_paragraph(h: &mut Harness<'static, PdfCraftApp>, text: &str, offset: [f64; 2]) {
    let (from, to) = paragraph_drag_points(h, text, offset);
    assert!(
        h.query_all_by_label_contains("object selected").next().is_none() && h.query_all_by_label_contains("objects selected").next().is_none(),
        "no objects selected"
    );
    assert!(h.state().views[0].line_editor.is_none());
    drag(h, from, to);
    assert!(h.state().views[0].line_editor.is_none(), "a body drag must not open the typing editor");
    assert!(
        h.query_all_by_label_contains("object selected").next().is_none() && h.query_all_by_label_contains("objects selected").next().is_none(),
        "no objects selected"
    );
}

fn assert_dragged_paragraphs(actual: &[pdfcraft_engine::TextBlock], expected: &[pdfcraft_engine::TextBlock]) {
    assert_eq!(actual.len(), expected.len(), "independent paragraphs must survive");
    // Source order is itself an invariant, including when text and geometry are identical.
    for (a, e) in actual.iter().zip(expected) {
        assert_eq!((&a.text, &a.base_font, a.size, a.bold, a.italic, a.color), (&e.text, &e.base_font, e.size, e.bold, e.italic, e.color));
        assert_eq!(a.lines.len(), e.lines.len(), "moving must not rewrap");
        assert!(a.rect.iter().zip(e.rect).all(|(a, e)| (a - e).abs() < 0.01), "{a:?} != {e:?}");
    }
}

/// Read only the current page contents, not stale revisions or the renderer's deduplicated
/// text layer. These synthetic fixtures contain each literal exactly once per source draw.
fn paragraph_drag_content(h: &Harness<'static, PdfCraftApp>) -> String {
    let app = h.state();
    let doc = pdfcraft_cos::Document::open(app.session.get(app.views[0].id).unwrap().bytes.clone()).unwrap();
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let contents = doc.resolve(page.dict.get(b"Contents").unwrap());
    let pieces = match contents.as_array() {
        Some(a) => a.clone(),
        None => vec![contents.as_ref().clone()],
    };
    let mut bytes = Vec::new();
    for piece in pieces {
        let object = doc.resolve(&piece);
        let pdfcraft_cos::Object::Stream(stream) = object.as_ref() else { panic!("fixture content must be a stream") };
        bytes.extend(stream.decoded().unwrap());
    }
    String::from_utf8(bytes).unwrap()
}

fn save_paragraph_drag(h: &mut Harness<'static, PdfCraftApp>, name: &str, original: &[u8]) -> Vec<u8> {
    let out = temp_path(name);
    let id = h.state().views[0].id;
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(4);
    let saved = std::fs::read(&out).unwrap();
    assert!(saved.starts_with(original), "Save keeps the opened/saved revisions");
    assert!(!h.state().session.get(id).unwrap().dirty);
    assert_eq!(h.state().session.get(id).unwrap().bytes.as_slice(), saved.as_slice());
    std::fs::remove_file(out).unwrap();
    saved
}

fn paragraph_drag_shot(h: &mut Harness<'static, PdfCraftApp>, name: &str) {
    if let Ok(directory) = std::env::var("PDFCRAFT_PARAGRAPH_DRAG_SHOTS") {
        assert!(
            crate::support::step_until(h, |h| {
                h.run_steps(1);
                !h.state().render_pending()
            }),
            "the screenshot must show the current document generation"
        );
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        h.render().unwrap().save(directory.join(name)).unwrap();
    }
}

fn check_paragraph_overlap_drag(identical: bool, reopen_between: bool) {
    let _gpu = gpu();
    let (stationary, moving) = if identical { ("Repeated", "Repeated") } else { ("Stationary beta", "Moving alpha") };
    let content =
        format!("BT /F1 12 Tf 60 220 Td ({stationary}) Tj ET BT /F1 12 Tf 250 100 Td (Keep here) Tj ET BT /F1 12 Tf 60 320 Td ({moving}) Tj ET");
    let mut saved_base = paragraph_drag_fixture(&content, 0, 1);
    let mut h = paragraph_drag_harness(saved_base.clone(), 0);
    let before = dragged_paragraphs(&h);
    assert_eq!(before.len(), 3);
    paragraph_drag_shot(&mut h, &format!("before-{identical}-{reopen_between}.png"));
    let moving_index = before.iter().rposition(|b| b.text == moving).unwrap();
    let mut overlap = before.clone();
    for i in [1, 3] {
        overlap[moving_index].rect[i] -= 100.0;
    }
    drag_paragraph(&mut h, moving, [0.0, -100.0]);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &overlap);
    paragraph_drag_shot(&mut h, &format!("overlap-{identical}-{reopen_between}.png"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &before);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().can_undo().is_none(), "the first release was exactly one step");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(4);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &overlap);

    if reopen_between {
        let bytes = save_paragraph_drag(&mut h, &format!("paragraph-overlap-{identical}.pdf"), &saved_base);
        saved_base = bytes.clone();
        h = paragraph_drag_harness(bytes, 0);
        assert_dragged_paragraphs(&dragged_paragraphs(&h), &overlap);
    }
    // A rewrite here used to remove the stationary box, even if its text was different.
    drag_paragraph(&mut h, moving, [150.0, 0.0]);
    let mut after = overlap.clone();
    for i in [0, 2] {
        after[moving_index].rect[i] += 150.0;
    }
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &after);
    paragraph_drag_shot(&mut h, &format!("after-{identical}-{reopen_between}.png"));
    let current = paragraph_drag_content(&h);
    assert_eq!(current.matches(&format!("({stationary})")).count(), if identical { 2 } else { 1 });
    assert_eq!(current.matches(&format!("({moving})")).count(), if identical { 2 } else { 1 });
    assert_eq!(current.matches("(Keep here)").count(), 1);
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Move object"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &overlap);
    if !reopen_between {
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(4);
        assert_dragged_paragraphs(&dragged_paragraphs(&h), &before);
    }
    assert!(h.state().session.get(h.state().views[0].id).unwrap().can_undo().is_none(), "each release was exactly one step");
    if !reopen_between {
        h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        h.run_steps(4);
        assert_dragged_paragraphs(&dragged_paragraphs(&h), &overlap);
    }
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(4);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &after);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().can_redo().is_none());
    let bytes = save_paragraph_drag(&mut h, &format!("paragraph-separated-{identical}-{reopen_between}.pdf"), &saved_base);
    let reopened = paragraph_drag_harness(bytes, 0);
    assert_dragged_paragraphs(&dragged_paragraphs(&reopened), &after);
    assert_eq!(paragraph_drag_content(&reopened), current);
}

fn check_added_text_drag(panel_open: bool) {
    let content = "BT /F1 12 Tf 60 220 Td (Keep here) Tj ET";
    let mut h = paragraph_drag_harness(paragraph_drag_fixture(content, 0, 1), 0);
    let id = h.state().views[0].id;
    h.state_mut()
        .session
        .apply(
            id,
            pdfcraft_engine::Edit::AddText {
                page: 0,
                text: pdfcraft_engine::AddedText { text: "Added note".into(), rect: [100.0, 330.0, 230.0, 354.0], ..Default::default() },
            },
        )
        .unwrap();
    let original = h.state().session.save_bytes(id).unwrap();
    h = paragraph_drag_harness(original.as_ref().clone(), 0);
    h.state_mut().left_open = panel_open;
    h.run_steps(4);
    let added = |h: &Harness<'static, PdfCraftApp>| {
        let app = h.state();
        app.session.get(app.views[0].id).unwrap().added().to_vec()
    };
    let before_added = added(&h);
    assert_eq!(before_added.len(), 1);
    let before = dragged_paragraphs(&h);
    assert_eq!(before.len(), 2, "added text is also visible in the raw paragraph list");
    let mut expected = before.clone();
    let moved = expected.iter_mut().find(|b| b.text == "Added note").unwrap();
    for (i, coordinate) in moved.rect.iter_mut().enumerate() {
        *coordinate += [30.0, -20.0][i % 2];
    }
    // The same pointer hits added content and its source glyphs. Only content_ui may own it:
    // a Text target would be excluded from MoveObjects and overwrite the valid UpdateContent.
    let (from, to) = paragraph_drag_points(&h, "Added note", [30.0, -20.0]);
    drag(&mut h, from, to);
    assert!(h.state().views[0].line_editor.is_none());
    assert!(h.state().views[0].block_drag.is_none());
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &expected);
    let after_added = added(&h);
    assert_eq!(after_added.len(), 1);
    let (old_rect, new_rect) = (before_added[0].content.rect(), after_added[0].content.rect());
    for i in 0..4 {
        assert!((new_rect[i] - old_rect[i] - [30.0, -20.0][i % 2]).abs() < 0.01);
    }
    assert_eq!(after_added[0].content, before_added[0].content.with_rect(new_rect));
    assert_eq!(h.state().session.get(h.state().views[0].id).unwrap().can_undo(), Some("Edit content"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(4);
    assert_eq!(added(&h), before_added);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &before);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().can_undo().is_none(), "one content edit, no paragraph rewrite");
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run_steps(4);
    assert_eq!(added(&h), after_added);
    assert_dragged_paragraphs(&dragged_paragraphs(&h), &expected);
    let bytes = save_paragraph_drag(&mut h, &format!("paragraph-added-content-{panel_open}.pdf"), original.as_slice());
    let reopened = paragraph_drag_harness(bytes, 0);
    assert_eq!(added(&reopened), after_added);
    assert_dragged_paragraphs(&dragged_paragraphs(&reopened), &expected);
}

/// A quick flick: press at `from`, and the next frame already finds the pointer at `to`.
fn flick(h: &mut Harness<'static, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    h.hover_at(to);
    h.run_steps(1);
    h.drop_at(to);
    h.run_steps(4);
}

/// The Pages panel, in a window tall enough to show every thumbnail of a short fixture.
fn pages_panel(pages: usize) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, fixture(pages)).expect("fixture opens");
        app.set_option("panel", "pages").unwrap();
        app
    });
    h.run_steps(4);
    h
}

fn picked(h: &Harness<'static, PdfCraftApp>) -> Vec<usize> {
    h.state().views[0].selected.iter().copied().collect()
}

/// A Pages panel thumbnail (the rightmost node with that label).
fn panel_thumb(h: &Harness<'static, PdfCraftApp>, label: &str) -> egui::Pos2 {
    h.get_all_by_label(label).map(|n| n.rect()).max_by(|a, b| a.left().total_cmp(&b.left())).expect("the thumbnail").center()
}

fn right_click_at(h: &mut Harness<'static, PdfCraftApp>, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
}

fn clipped_artwork_with_labels() -> Vec<u8> {
    clipped_artwork_with_labels_drawn(false)
}

/// `own_clip` draws the image alone in its clip (`q re W n cm Do Q`), which moves with it (#1049).
fn clipped_artwork_with_labels_drawn(own_clip: bool) -> Vec<u8> {
    artwork_drawn(if own_clip {
        &b"q 30 170 130 80 re W* n 800 0 0 900 -300 -400 cm /Im0 Do Q BT /F1 12 Tf 30 130 Td (Editable caption) Tj ET BT /F1 12 Tf 40 220 Td (Overlay) Tj ET BT /F1 9 Tf 154 249 Td (Corner label) Tj ET"[..]
    } else {
        &b"q 30 170 130 80 re W* n q 800 0 0 900 -300 -400 cm /Im0 Do Q Q BT /F1 12 Tf 30 130 Td (Editable caption) Tj ET BT /F1 12 Tf 40 220 Td (Overlay) Tj ET BT /F1 9 Tf 154 249 Td (Corner label) Tj ET"[..]
    })
}

/// [`fixture`]'s two pages, each drawn by `content`, with a one-pixel picture as `/Im0`.
fn artwork_drawn(content: &[u8]) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, Object, Stream, write_incremental};
    let mut doc = Document::open(std::sync::Arc::new(fixture(2))).unwrap();
    let mut image = Dict::new();
    image.set(b"Type".to_vec(), Object::name("XObject"));
    image.set(b"Subtype".to_vec(), Object::name("Image"));
    image.set(b"Width".to_vec(), Object::Int(1));
    image.set(b"Height".to_vec(), Object::Int(1));
    image.set(b"BitsPerComponent".to_vec(), Object::Int(8));
    image.set(b"ColorSpace".to_vec(), Object::name("DeviceRGB"));
    let image = doc.add(Object::Stream(Stream::flate(image, &[220, 235, 245])));
    for page in pdfcraft_model::pages(&doc) {
        let contents = doc.add(Object::Stream(Stream::flate(Dict::new(), content)));
        let mut resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
        let mut xo = Dict::new();
        xo.set(b"Im0".to_vec(), Object::Ref(image));
        resources.set(b"XObject".to_vec(), Object::Dict(xo));
        doc.update_dict(page.obj, |d| {
            d.set(b"Resources".to_vec(), Object::Dict(resources));
            d.set(b"Contents".to_vec(), Object::Ref(contents));
        })
        .unwrap();
    }
    write_incremental(&doc, &Default::default()).unwrap()
}

/// Red pixels in `r`: the test picture.
fn red(pixels: &image::RgbaImage, r: egui::Rect) -> usize {
    let (x0, y0, x1, y1) = (r.left().max(0.0) as u32, r.top().max(0.0) as u32, r.right() as u32, r.bottom() as u32);
    (y0..y1.min(pixels.height()))
        .flat_map(|y| (x0..x1.min(pixels.width())).map(move |x| (x, y)))
        .filter(|(x, y)| matches!(pixels.get_pixel(*x, *y).0, [r, g, b, _] if r > 150 && g < 90 && b < 90))
        .count()
}

/// Both pages of [`clipped_artwork_with_labels`] at 100% in Edit text, the picture on page 0
/// selected (and being cropped, with `crop`).
fn picture_pages(crop: bool) -> Harness<'static, PdfCraftApp> {
    picture_pages_of(clipped_artwork_with_labels(), crop)
}

/// [`picture_pages`], of `pdf`.
fn picture_pages_of(pdf: Vec<u8>, crop: bool) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1100.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("clipped.pdf", None, pdf).unwrap();
        app.set_option("left", "closed").unwrap();
        app.set_option("zoom", "100").unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(3);
    let id = h.state().views[0].id;
    let keep = h.state().session.get(id).unwrap().page_images(0)[0].visible_rect.unwrap();
    let mut selection = pdfcraft_ui_egui::ImageSelection::at(0, 0);
    selection.crop = crop.then(|| pdfcraft_ui_egui::CropMode::new(keep));
    h.state_mut().views[0].image_selection = Some(selection);
    h.run_steps(3);
    h
}

/// The screen point at PDF (x, y) of `page` of [`picture_pages`].
fn page_at(h: &Harness<'static, PdfCraftApp>, page: usize, x: f32, y: f32) -> egui::Pos2 {
    let r = h.state().views[0].page_screen_rect(page).expect("both pages on screen");
    egui::pos2(r.left() + x / 200.0 * r.width(), r.top() + (300.0 - y) / 300.0 * r.height())
}

fn cursor_over(h: &mut Harness<'static, PdfCraftApp>, p: egui::Pos2) -> egui::CursorIcon {
    h.hover_at(p);
    h.run_steps(2);
    h.output().platform_output.cursor_icon
}

/// A click that slips a few pixels between its press and its release, within egui's click distance.
fn slipping_click(h: &mut Harness<'static, PdfCraftApp>, press: egui::Pos2, release: egui::Pos2) {
    h.hover_at(press);
    h.run_steps(1);
    h.drag_at(press);
    h.run_steps(1);
    h.drop_at(release);
    h.run_steps(4);
}

/// [`harness`]'s page 1 in Edit text (`tool`), and the screen point at PDF (x, y) on it.
fn edit_page(tool: pdfcraft_ui_egui::QuickTool) -> Harness<'static, PdfCraftApp> {
    let mut h = harness(2, |_| {});
    h.state_mut().quick_tool = tool;
    h.run_steps(3);
    h
}

fn point_on(h: &Harness<'static, PdfCraftApp>, x: f64, y: f64) -> egui::Pos2 {
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    egui::pos2(r.left() + x as f32 * k, r.top() + (300.0 - y as f32) * k)
}

fn crop_secondary_click(h: &mut Harness<'static, PdfCraftApp>, p: egui::Pos2) {
    assert!(h.state().views[0].viewport_rect().contains(p), "test point must be visible: {p:?}");
    h.hover_at(p);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
}

fn assert_crop_context_preserved(h: &Harness<'static, PdfCraftApp>) {
    let selection = h.state().views[0].image_selection.as_ref().expect("crop remains selected");
    assert_eq!(selection.index, 0, "an overlapping image must not steal the crop");
    assert!(selection.crop.is_some(), "crop mode remains active");
    for label in ["Flip horizontal", "Rotate view clockwise", "Take a snapshot"] {
        assert!(h.query_all_by_label(label).next().is_none(), "unexpected menu: {label}");
    }
    assert!(h.query_by_label("Done").is_some());
    assert!(h.query_by_label("Cancel").is_some());
}

/// Between pages `first` and `second`, and on `second` above its own text, across where page
/// 1's paragraph is: what is there without it (the gap as `plain` shows it without Edit text,
/// white paper).
fn shown_on_its_page_only(h: &mut Harness<'static, PdfCraftApp>, plain: &image::RgbaImage, first: egui::Rect, second: egui::Rect, what: &str) {
    let shot = h.render().unwrap();
    let pixel = |x: f32, y: f32| shot.get_pixel(x as u32, y as u32).0;
    let gap_y = (first.bottom() + second.top()) / 2.0;
    for x in (first.left() as u32 + 70)..(first.left() as u32 + 420) {
        let x = x as f32;
        assert_eq!(pixel(x, gap_y), plain.get_pixel(x as u32, gap_y as u32).0, "{what}: between the pages at x = {x}");
        for dy in [4.0, 20.0, 40.0] {
            let [r, g, b, _] = pixel(x, second.top() + dy);
            assert!(r > 240 && g > 240 && b > 240, "{what}: page 2 at {x}, +{dy}: {r},{g},{b}");
        }
    }
}

/// Where a mode bar tab is, by its label.
fn mode_tab(h: &Harness<'static, PdfCraftApp>, label: &str) -> egui::Rect {
    h.get_all_by_role_and_label(Role::Button, label).map(|n| n.rect()).find(|r| r.bottom() < 120.0).expect("a mode tab")
}

/// The cursor over each mode bar tab that page 1's text lies under, once that text is scrolled
/// up under the bar. With the All tools panel closed it lies under Edit, Convert and E-Sign; it
/// starts right of All tools.
fn mode_tab_cursors(h: &mut Harness<'static, PdfCraftApp>, text: egui::Rect) -> [(&'static str, egui::CursorIcon); 3] {
    h.state_mut().views[0].goto = Some((0, 0.5));
    h.run_steps(4);
    let offset = h.state().views[0].page_screen_rect(0).expect("on screen").top();
    ["Edit", "Convert", "E-Sign"].map(|label| {
        let at = mode_tab(h, label).center();
        assert!(text.translate(egui::vec2(0.0, offset)).contains(at), "the text lies under {label}");
        h.hover_at(at);
        h.run_steps(2);
        (label, h.output().platform_output.cursor_icon)
    })
}

const HAND_ON_EVERY_TAB: [(&str, egui::CursorIcon); 3] =
    [("Edit", egui::CursorIcon::PointingHand), ("Convert", egui::CursorIcon::PointingHand), ("E-Sign", egui::CursorIcon::PointingHand)];

/// Page 1's text block on screen, relative to the page's top edge, in the window as it is now.
fn page_text(h: &Harness<'static, PdfCraftApp>) -> egui::Rect {
    let id = h.state().views[0].id;
    let block = h.state().session.get(id).unwrap().text_blocks(0)[0].clone();
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let k = r.width() / 200.0;
    let at = |x: f64, y: f64| egui::pos2(r.left() + x as f32 * k, (300.0 - y as f32) * k);
    egui::Rect::from_two_pos(at(block.rect[0], block.rect[1]), at(block.rect[2], block.rect[3]))
}

/// The paragraph opened for typing, as a person opens it: a click on its text.
fn open_paragraph(h: &mut Harness<'static, PdfCraftApp>) {
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    click_at(h, at);
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph opens for typing");
}

/// A paragraph of a short, a wide and a short row, opened in the paragraph editor.
fn three_row_paragraph() -> Harness<'static, PdfCraftApp> {
    let bytes = paragraph_drag_fixture("BT /F1 10 Tf 12 TL 60 400 Td (One two) Tj T* (three four five six seven eight) Tj T* (nine ten) Tj ET", 0, 1);
    let mut h = paragraph_drag_harness(bytes, 0);
    let (at, _) = paragraph_drag_points(&h, "One two three four five six seven eight nine ten", [0.0, 0.0]);
    click_at(&mut h, at);
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph opens for typing");
    h
}

/// Format text's alignment flyout, by pointer.
fn align_by_pointer(h: &mut Harness<'static, PdfCraftApp>, choice: &str) {
    let button = h.get_by_label("Alignment").rect().center();
    click_at(h, button);
    let item = h.get_by_label(choice).rect().center();
    click_at(h, item);
}

fn row_rect(h: &Harness<'static, PdfCraftApp>, starts: &str) -> [f64; 4] {
    let s = h.state();
    s.session.get(s.views[0].id).unwrap().text_lines(0).into_iter().find(|l| l.text.starts_with(starts)).expect("the row").rect
}

fn two_image_fixture() -> Vec<u8> {
    let content = "q 20 0 0 20 100 100 cm /Im0 Do Q q 20 0 0 20 20 20 cm /Im0 Do Q";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n\0\0\0\0\nendstream"
            .into(),
    ];
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

fn affine_review_image_fixture(matrix: [f64; 6], clip: Option<[f64; 4]>, rotation: i64) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, Object, Stream};
    let mut doc = Document::open(std::sync::Arc::new(fixture(1))).unwrap();
    let page = pdfcraft_model::pages(&doc).remove(0);
    let mut image = Dict::new();
    image.set(b"Type".to_vec(), Object::name("XObject"));
    image.set(b"Subtype".to_vec(), Object::name("Image"));
    image.set(b"Width".to_vec(), Object::Int(1));
    image.set(b"Height".to_vec(), Object::Int(1));
    image.set(b"BitsPerComponent".to_vec(), Object::Int(8));
    image.set(b"ColorSpace".to_vec(), Object::name("DeviceRGB"));
    let image = doc.add(Object::Stream(Stream::flate(image, &[220, 80, 50])));
    let mut xobjects = Dict::new();
    xobjects.set(b"ImReview".to_vec(), Object::Ref(image));
    let mut resources = Dict::new();
    resources.set(b"XObject".to_vec(), Object::Dict(xobjects));
    let mut content = String::from("q ");
    if let Some([x0, y0, x1, y1]) = clip {
        content.push_str(&format!("{x0} {y0} {} {} re W n ", x1 - x0, y1 - y0));
    }
    let [a, b, c, d, e, f] = matrix;
    content.push_str(&format!("{a} {b} {c} {d} {e} {f} cm /ImReview Do Q"));
    let stream = doc.add(Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    doc.update_dict(page.obj, |dict| {
        dict.set(b"Contents".to_vec(), Object::Ref(stream));
        dict.set(b"Resources".to_vec(), Object::Dict(resources));
        dict.set(b"MediaBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Int(360), Object::Int(360)]));
        dict.set(b"Rotate".to_vec(), Object::Int(rotation));
    })
    .unwrap();
    pdfcraft_cos::write_incremental(&doc, &Default::default()).unwrap()
}

fn affine_review_harness(bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 1000.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("affine-review.pdf", None, bytes.clone()).unwrap();
        app
    });
    h.run_steps(4);
    h.state_mut().views[0].fit = pdfcraft_ui_egui::canvas::Fit::Page;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().views[0].image_selection = Some(pdfcraft_ui_egui::ImageSelection::at(0, 0));
    h.run_steps(3);
    h
}
