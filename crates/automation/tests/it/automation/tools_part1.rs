//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 1 of 8).

#[allow(unused_imports)]
use super::*;

#[cfg(target_os = "macos")]
#[test]
fn saving_a_pdf_keeps_the_destinations_extended_attributes() {
    use rustix::fs::{XattrFlags, getxattr, setxattr};
    let dir = workdir("save-xattrs");
    let target = dir.join("a.pdf");
    let tags = b"<plist version=\"1.0\"><array><string>PdfCraft\n6</string></array></plist>";
    let binary = b"\0\xffpdfcraft\0";
    let attributes: &[(&str, &[u8])] = &[("com.apple.metadata:_kMDItemUserTags", tags), ("com.pdfcraft.test", binary)];
    for (name, value) in attributes {
        setxattr(&target, *name, value, XattrFlags::empty()).unwrap();
    }
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "a.pdf"}))["doc"].as_u64().unwrap();
    for full in [false, true] {
        ok(&mut a, "page_rotate", json!({"doc": doc, "pages": [1], "degrees": 90}));
        let saved = ok(&mut a, "doc_save", json!({"doc": doc, "full": full}));
        assert_eq!(saved["incremental"], !full);
        for (name, value) in attributes {
            let mut bytes = [0u8; 256];
            let len = getxattr(&target, *name, &mut bytes[..]).unwrap();
            assert_eq!(&bytes[..len], *value, "full={full}, {name}");
        }
    }
    // Save As preserves the destination's metadata, not attributes from the open input file.
    let other = dir.join("other.pdf");
    std::fs::write(&other, fixture(1)).unwrap();
    setxattr(&other, "com.pdfcraft.test", b"destination", XattrFlags::empty()).unwrap();
    ok(&mut a, "doc_save", json!({"doc": doc, "path": "other.pdf"}));
    let mut bytes = [0u8; 256];
    let len = getxattr(&other, "com.pdfcraft.test", &mut bytes[..]).unwrap();
    assert_eq!(&bytes[..len], b"destination");
    assert_eq!(getxattr(&other, "com.apple.metadata:_kMDItemUserTags", &mut bytes[..]).unwrap_err(), rustix::io::Errno::NOATTR);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn a_metadata_preservation_error_keeps_the_pdf_and_unsaved_edit() {
    use rustix::fs::{XattrFlags, setxattr};
    let dir = workdir("save-xattrs-error");
    let target = dir.join("a.pdf");
    let original = std::fs::read(&target).unwrap();
    setxattr(&target, "com.apple.ResourceFork", &vec![42u8; 16 * 1024 * 1024 + 1], XattrFlags::empty()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path": "a.pdf"}))["doc"].as_u64().unwrap();
    ok(&mut a, "page_rotate", json!({"doc": doc, "pages": [1], "degrees": 90}));
    let failure = err(&mut a, "doc_save", json!({"doc": doc}));
    assert!(failure.to_string().contains("extended attributes"), "{failure}");
    assert!(failure.to_string().contains("Save As"), "{failure}");
    assert_eq!(std::fs::read(&target).unwrap(), original);
    assert_eq!(ok(&mut a, "doc_info", json!({"doc": doc}))["document"]["dirty"], true);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "only the two original PDFs remain");
    // A fresh name remains usable: metadata refusal doesn't discard the accepted edit.
    ok(&mut a, "doc_save", json!({"doc": doc, "path": "copy.pdf"}));
    assert_ne!(std::fs::read(dir.join("copy.pdf")).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn workdir_call_sites_do_not_delete_each_other() {
    let first = workdir("same-tag");
    let sentinel = first.join("sentinel");
    std::fs::write(&sentinel, b"first call site").unwrap();

    let second = workdir("same-tag");
    let paths_differ = first != second;
    let sentinel_bytes = std::fs::read(&sentinel).ok();
    let first_a = std::fs::read(first.join("a.pdf")).ok();
    let first_b = std::fs::read(first.join("b.pdf")).ok();
    let second_a = std::fs::read(second.join("a.pdf")).ok();
    let second_b = std::fs::read(second.join("b.pdf")).ok();

    let _ = std::fs::remove_dir_all(&first);
    if paths_differ {
        let _ = std::fs::remove_dir_all(&second);
    }

    assert!(paths_differ, "distinct call sites shared {first:?}");
    assert_eq!(sentinel_bytes.as_deref(), Some(&b"first call site"[..]));
    assert_eq!(first_a, Some(fixture(3)));
    assert_eq!(first_b, Some(fixture(2)));
    assert_eq!(second_a, Some(fixture(3)));
    assert_eq!(second_b, Some(fixture(2)));
}

/// Full-screen presentation settings (`/Trans`, `/Dur`) reach `doc_info`, 1-based pages, only
/// for the pages that carry them.
#[test]
fn doc_info_lists_page_transitions_and_display_times() {
    let dir = workdir("transitions");
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Trans << /S /Fade /D 0.5 >> /Dur 4 >>".to_vec(),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    std::fs::write(dir.join("slides.pdf"), bytes).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "slides.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    let t = info["transitions"].as_array().unwrap();
    assert_eq!(t.len(), 1, "{}", info["transitions"]);
    assert_eq!(
        (t[0]["page"].as_u64(), t[0]["style"].as_str(), t[0]["duration"].as_f64(), t[0]["display"].as_f64()),
        (Some(2), Some("Fade"), Some(0.5), Some(4.0)),
        "{}",
        t[0]
    );
}

/// `doc_info` says where the document opens: the `/OpenAction` destination, a named one
/// resolved (ISO 32000-2 §12.3.2.4), as a 1-based page and a destination in view coordinates.
#[test]
fn doc_info_reports_the_open_destination() {
    let dir = workdir("open-destination");
    let write = |name: &str, catalog: &str| {
        let objects: Vec<String> = vec![
            format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>"),
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
        ];
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = bytes.len();
        bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for o in offsets {
            bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        std::fs::write(dir.join(name), bytes).unwrap();
    };
    write("named.pdf", "/OpenAction << /S /GoTo /D /there >> /Dests << /there [4 0 R /XYZ 100 500 2] >>");
    write("script.pdf", "/OpenAction << /S /JavaScript /JS (1) >>");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "named.pdf" }))["doc"].as_u64().unwrap();
    let open = ok(&mut a, "doc_info", json!({ "doc": doc }))["open_destination"].clone();
    assert_eq!(open["page"], 2, "{open}");
    // y 500 is 292 points below the top of the 792-point page.
    assert_eq!(
        (open["destination"]["fit"].as_str(), open["destination"]["left"].as_f64(), open["destination"]["top"].as_f64()),
        (Some("XYZ"), Some(100.0), Some(292.0)),
        "{open}"
    );
    assert_eq!(open["destination"]["zoom"].as_f64(), Some(2.0));
    // The initial view reads the named destination's page, and choosing page 1 replaces it (ISO
    // 32000-2 §12.3.2.4): one undo step, kept by the save.
    let page = |a: &mut Automation, doc: u64| ok(a, "doc_info", json!({ "doc": doc }))["open_destination"]["page"].clone();
    assert_eq!(ok(&mut a, "doc_initial_view", json!({ "doc": doc }))["initial_view"]["page"], 2);
    assert_eq!(ok(&mut a, "doc_initial_view", json!({ "doc": doc, "page": 1 }))["initial_view"]["page"], 1);
    assert_eq!(page(&mut a, doc), 1);
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(page(&mut a, doc), 2);
    ok(&mut a, "edit_redo", json!({ "doc": doc }));
    assert_eq!(page(&mut a, doc), 1);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "named-page1.pdf" }));
    let saved = ok(&mut a, "doc_open", json!({ "path": "named-page1.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(page(&mut a, saved), 1);
    let doc = ok(&mut a, "doc_open", json!({ "path": "script.pdf" }))["doc"].as_u64().unwrap();
    assert!(ok(&mut a, "doc_info", json!({ "doc": doc }))["open_destination"].is_null());
}

/// Page transitions through the tools: set on chosen pages, written to the file as `/Trans` and
/// `/Dur`, removed again, bad values refused, and one undo step. (Read through `pdfcraft_cos`,
/// so this does not lean on `doc_info` listing them.)
#[test]
fn page_transitions_are_set_and_removed_through_the_tools() {
    use pdfcraft_cos::Object;
    /// `key` of 0-based `page` in the saved file `name`, resolved.
    fn entry(dir: &Path, name: &str, page: usize, key: &[u8]) -> Option<Object> {
        let d = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join(name)).ok()?)).ok()?;
        let root = d.root()?;
        let pages = d.get(root).as_dict()?.reference(b"Pages")?;
        let kids = d.get(pages).as_dict()?.get(b"Kids")?.clone();
        let kid = d.resolve(&kids).as_array()?.get(page)?.clone();
        let o = d.resolve(&kid).as_dict()?.get(key)?.clone();
        Some((*d.resolve(&o)).clone())
    }
    let dir = workdir("transitions-set");
    std::fs::write(dir.join("a.pdf"), fixture(3)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_transition_set", json!({ "doc": doc, "pages": [2, 3], "style": "Wipe", "duration": 2, "direction": 270, "display": 5 }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out/t1.pdf" }));
    let t = entry(&dir, "out/t1.pdf", 1, b"Trans").expect("/Trans on page 2");
    let t = t.as_dict().expect("a dictionary");
    assert_eq!(
        (t.name(b"S"), t.get(b"D").and_then(Object::as_f64), t.get(b"Di").and_then(Object::as_f64)),
        (Some(&b"Wipe"[..]), Some(2.0), Some(270.0)),
        "{t:?}"
    );
    assert_eq!(entry(&dir, "out/t1.pdf", 1, b"Dur").and_then(|o| o.as_f64()), Some(5.0));
    assert!(entry(&dir, "out/t1.pdf", 2, b"Trans").is_some(), "page 3 too");
    assert!(entry(&dir, "out/t1.pdf", 0, b"Trans").is_none() && entry(&dir, "out/t1.pdf", 0, b"Dur").is_none(), "page 1 was not asked for");
    ok(
        &mut a,
        "page_transition_set",
        json!({ "doc": doc, "pages": [3], "style": "Fly", "motion": "outward", "direction": "none", "scale": 0.5, "opaque": true }),
    );
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out/t2.pdf" }));
    let t = entry(&dir, "out/t2.pdf", 2, b"Trans").expect("/Trans on page 3");
    let t = t.as_dict().expect("a dictionary");
    assert_eq!(
        (t.name(b"S"), t.name(b"M"), t.name(b"Di"), t.get(b"SS").and_then(Object::as_f64), t.get(b"B")),
        (Some(&b"Fly"[..]), Some(&b"O"[..]), Some(&b"None"[..]), Some(0.5), Some(&Object::Bool(true))),
        "{t:?}"
    );
    assert!(entry(&dir, "out/t2.pdf", 2, b"Dur").is_none(), "display time left out removes /Dur");
    assert!(a.call("page_transition_set", &json!({ "doc": doc, "style": "Swirl" })).is_err(), "an unknown style is refused");
    assert!(a.call("page_transition_set", &json!({ "doc": doc, "style": "Wipe", "direction": 45 })).is_err());
    assert!(a.call("page_transition_set", &json!({ "doc": doc, "pages": [9], "style": "Fade" })).is_err(), "no such page");
    ok(&mut a, "page_transition_set", json!({ "doc": doc, "style": "none" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out/t3.pdf" }));
    assert!((0..3).all(|p| entry(&dir, "out/t3.pdf", p, b"Trans").is_none() && entry(&dir, "out/t3.pdf", p, b"Dur").is_none()), "all removed");
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out/t4.pdf" }));
    assert!(
        entry(&dir, "out/t4.pdf", 1, b"Trans").is_some() && entry(&dir, "out/t4.pdf", 2, b"Trans").is_some(),
        "one undo step brings both pages back"
    );
}

/// #1937 through the tools: a damaged file is repaired on open (`doc_info` lists the repair) and
/// `doc_save` over it writes the rebuilt file, so opening it again needs no repair.
#[test]
fn saving_a_repaired_file_puts_the_repair_on_disk() {
    let dir = workdir("repaired-save");
    let good = fixture(2);
    let mut broken = b"%PDF-1.7\n%shifted\n".to_vec();
    broken.extend_from_slice(&good[b"%PDF-1.7\n".len()..]);
    std::fs::write(dir.join("broken.pdf"), &broken).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "broken.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert!(info["repairs"].as_array().unwrap().iter().any(|r| r.as_str().unwrap_or("").contains("rebuilt")), "{}", info["repairs"]);
    let saved = ok(&mut a, "doc_save", json!({ "doc": doc }));
    assert_eq!(saved["incremental"], false, "{saved}");
    let bytes = std::fs::read(dir.join("broken.pdf")).unwrap();
    assert_ne!(bytes, broken, "the damaged bytes were written back");
    let reopened = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    assert!(reopened.repair_log().is_empty(), "{:?}", reopened.repair_log());
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert_eq!(info["repairs"].as_array().unwrap().len(), 0, "{}", info["repairs"]);
    assert_eq!(info["pages"].as_array().unwrap().len(), 2);
}

/// Custom document properties through the tools: set and cleared with doc_set_info under any
/// name the spec does not use, listed by doc_info apart from the standard entries, bad names
/// refused without a change.
#[test]
fn custom_document_properties_are_set_listed_and_cleared_through_the_tools() {
    let dir = workdir("custom-info");
    std::fs::write(dir.join("a.pdf"), fixture(1)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Report" }));
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Project", "value": "Apollo" }));
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Revision", "value": "7" }));
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert_eq!(info["title"], "Report");
    let mut custom: Vec<(String, String)> =
        info["custom"].as_array().unwrap().iter().map(|c| (c["name"].as_str().unwrap().into(), c["value"].as_str().unwrap().into())).collect();
    custom.sort();
    assert_eq!(custom, [("Project".to_string(), "Apollo".to_string()), ("Revision".to_string(), "7".to_string())], "{}", info["custom"]);
    assert!(a.call("doc_set_info", &json!({ "doc": doc, "key": "Bad Name", "value": "x" })).is_err(), "a name with a space is refused");
    assert!(a.call("doc_set_info", &json!({ "doc": doc, "key": "", "value": "x" })).is_err());
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Project", "value": "" }));
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert_eq!(info["custom"].as_array().unwrap().len(), 1, "{}", info["custom"]);
    assert_eq!(info["custom"][0]["name"], "Revision");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out/custom.pdf" }));
    let mut b = auto(&dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "out/custom.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut b, "doc_info", json!({ "doc": re }));
    assert_eq!(info["custom"][0]["value"], "7", "{}", info["custom"]);
}

/// The custom list `doc_info` shows when a file opens is the one it shows after an unrelated
/// edit: a name value, a blank or NUL-padded string and a number are judged the same way by the
/// open-time reader and by the engine's refresh.
#[test]
fn custom_document_properties_do_not_change_after_an_unrelated_edit() {
    let dir = workdir("custom-info-stable");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Producer (Test) /Project (Apollo) /Dept /RnD /Blank (   ) /Padded (Lab\\000\\000) /Revision 7 /Empty () /Mixed ( \\000Lab\\000 ) /Pad ( \\000 ) >>",
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
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    std::fs::write(dir.join("custom.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "custom.pdf" }))["doc"].as_u64().unwrap();
    let opened = ok(&mut a, "doc_info", json!({ "doc": doc }))["custom"].clone();
    let names: Vec<&str> = opened.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Project", "Padded", "Mixed"], "{opened}");
    assert_eq!(opened[1]["value"], "Lab", "{opened}");
    assert_eq!(opened[2]["value"], "Lab", "{opened}");
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Report" }));
    let edited = ok(&mut a, "doc_info", json!({ "doc": doc }))["custom"].clone();
    assert_eq!(edited, opened);
}

#[test]
fn autotag_preview_proposes_text_without_changing_the_pdf() {
    let dir = workdir("autotag-preview");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let before = std::fs::read(dir.join("a.pdf")).unwrap();

    let preview = ok(&mut a, "accessibility_autotag_preview", json!({ "doc": doc, "page": 1 }));
    assert_eq!(preview["page"], 1);
    assert_eq!(preview["candidates"][0]["type"], "P");
    assert_eq!(preview["candidates"][0]["text"], "Page 1");
    assert_eq!(preview["candidates"][0]["needs_review"], true);
    assert_eq!(preview["unclassified_images"], 0);
    assert_eq!(preview["unclassified_text_blocks"], 0);
    assert_eq!(preview["existing_structure"], false);
    assert_eq!(std::fs::read(dir.join("a.pdf")).unwrap(), before);
    assert!(matches!(
        a.call("accessibility_autotag_preview", &json!({ "doc": doc, "page": 4 })),
        Err(ToolError::InvalidArgs(_) | ToolError::Failed(_))
    ));

    let blank = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 300, "height": 400 }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_add_text", json!({ "doc": blank, "page": 1, "text": "Annual report", "at": [20, 25], "size": 24 }));
    ok(&mut a, "page_add_text", json!({ "doc": blank, "page": 1, "text": "First paragraph", "at": [20, 100], "size": 12 }));
    ok(&mut a, "page_add_text", json!({ "doc": blank, "page": 1, "text": "Second paragraph", "at": [20, 140], "size": 12 }));
    let preview = ok(&mut a, "accessibility_autotag_preview", json!({ "doc": blank, "page": 1 }));
    assert!(preview["candidates"].as_array().unwrap().iter().any(|c| c["text"] == "Annual report" && c["type"] == "H1"));
    assert!(preview["candidates"].as_array().unwrap().iter().any(|c| c["text"] == "First paragraph" && c["type"] == "P"));
}

#[test]
fn render_pixels_agrees_with_page_render_and_skips_the_png() {
    let dir = workdir("render-pixels");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();

    let (width, height, pixels) = a.render_pixels(doc, 1, 1.0).unwrap();
    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    let Content::Png { width: png_width, height: png_height, .. } = png[0] else { panic!("expected an image") };
    assert_eq!((width, height), (png_width, png_height), "scale 1.0 is 72 dpi, like the tool");
    assert_eq!(pixels.as_ref().len(), width as usize * height as usize * 4, "RGBA, four bytes a pixel");
    assert!(pixels.as_ref().as_chunks::<4>().0.iter().any(|p| p[..3] != [255, 255, 255]), "the page text is drawn, not a blank sheet");

    let (double_width, _, _) = a.render_pixels(doc, 1, 2.0).unwrap();
    assert_eq!(double_width, width * 2, "the scale is pixels per point");

    for scale in [0.0_f32, -1.0, f32::NAN, f32::INFINITY, 1.0e6] {
        assert!(matches!(a.render_pixels(doc, 1, scale), Err(ToolError::InvalidArgs(_))), "scale {scale} must be refused");
    }
    assert!(matches!(a.render_pixels(doc, 0, 1.0), Err(ToolError::InvalidArgs(_))), "pages are 1-based");
    assert!(matches!(a.render_pixels(doc, 99, 1.0), Err(ToolError::InvalidArgs(_))), "a page past the end");
    assert!(matches!(a.render_pixels(9_999, 1, 1.0), Err(ToolError::Failed(_))), "a document that is not open");
}

#[test]
fn autotag_apply_writes_reviewable_tags_and_survives_save_reopen() {
    let dir = workdir("autotag-apply");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "accessibility_check", json!({ "doc": doc, "rules": ["tagged-pdf", "tagged-content"] }));
    assert_eq!(before["failed"], 2);
    let rendered_before = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();

    let applied = ok(&mut a, "accessibility_autotag_apply", json!({ "doc": doc }));
    assert_eq!(applied["count"], 3, "Document plus one paragraph per page");
    assert_eq!(applied["tags"][1]["type"], "P");
    let after = ok(&mut a, "accessibility_check", json!({ "doc": doc, "rules": ["tagged-pdf", "tagged-content"] }));
    assert_eq!(after["passed"], 2);
    assert_eq!(a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap(), rendered_before);
    assert!(matches!(a.call("accessibility_autotag_apply", &json!({ "doc": doc })), Err(ToolError::Failed(_))));

    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "tagged.pdf" }));
    let saved = std::fs::read(dir.join("tagged.pdf")).unwrap();
    let reopened = pdfcraft_cos::Document::open(std::sync::Arc::new(saved)).unwrap();
    let pages = pdfcraft_model::pages(&reopened);
    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|page| page.dict.contains(b"StructParents")));
}

#[test]
fn autotag_form_fields_writes_reviewable_widget_tags() {
    let dir = workdir("autotag-form-fields");
    std::fs::write(dir.join("field.pdf"), form_pdf("name")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "field.pdf" }))["doc"].as_u64().unwrap();
    let tagged = ok(&mut a, "accessibility_autotag_form_fields", json!({ "doc": doc }));
    assert_eq!(tagged["status"], "passed");
    assert_eq!(tagged["remaining_findings"], 0);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "tagged.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "tagged.pdf" }))["doc"].as_u64().unwrap();
    let check = ok(&mut a, "accessibility_check", json!({ "doc": reopened, "rules": ["tagged-form-fields"] }));
    let result = check["results"].as_array().unwrap().iter().find(|result| result["rule"] == "tagged-form-fields").unwrap();
    assert_eq!(result["status"], "passed");
}

/// #1693: text_styles lists the styles, largest first; bookmark_from_text_styles makes the
/// bookmarks from the ones given per level, returns the tree, undoes in one step and says why
/// when it can't.
#[test]
fn bookmarks_from_text_styles_through_tools() {
    let dir = workdir("text-style-bookmarks");
    std::fs::write(dir.join("styled.pdf"), styled_headings_pdf()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "styled.pdf" }))["doc"].as_u64().unwrap();
    let styles = ok(&mut a, "text_styles", json!({ "doc": doc }))["styles"].clone();
    assert_eq!(
        styles,
        json!([
            { "font": "Helvetica-Bold", "size": 18.0, "bold": true, "italic": false, "lines": 2, "pages": 2, "sample": "Introduction" },
            { "font": "Helvetica-Bold", "size": 14.0, "bold": true, "italic": false, "lines": 1, "pages": 1, "sample": "Background" },
            { "font": "Helvetica", "size": 11.0, "bold": false, "italic": false, "lines": 2, "pages": 2, "sample": "Some text" },
        ])
    );
    // The styles as text_styles gave them, the chapters first.
    let levels = json!([styles[0].clone(), { "font": "Helvetica-Bold", "size": 14, "bold": true }]);
    let r = ok(&mut a, "bookmark_from_text_styles", json!({ "doc": doc, "levels": levels }));
    let top = &r["bookmarks"][0];
    assert_eq!(top["title"], "Untitled");
    let titles: Vec<(&str, u64)> =
        top["children"].as_array().unwrap().iter().map(|c| (c["title"].as_str().unwrap(), c["page"].as_u64().unwrap())).collect();
    assert_eq!(titles, [("Introduction", 1), ("Results", 2)]);
    assert_eq!(top["children"][0]["children"][0]["title"], "Background");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "New bookmarks from text styles");
    // What can't be done says why.
    let call = |a: &mut pdfcraft_automation::Automation, levels: Value| a.call("bookmark_from_text_styles", &json!({ "doc": doc, "levels": levels }));
    assert!(matches!(call(&mut a, json!([])), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(call(&mut a, json!([{ "font": "Helvetica-Bold" }])), Err(ToolError::InvalidArgs(m)) if m.contains("size")));
    assert!(
        matches!(call(&mut a, json!([{ "font": "Helvetica-Bold", "size": 18, "bold": "yes" }])), Err(ToolError::InvalidArgs(m)) if m.contains("bold"))
    );
    let unused = call(&mut a, json!([{ "font": "Times-Roman", "size": 30 }]));
    assert!(matches!(&unused, Err(ToolError::Failed(m)) if m.contains("no text is set in the chosen styles")), "{unused:?}");
    let too_many: Vec<Value> = (0..10).map(|_| styles[0].clone()).collect();
    assert!(matches!(call(&mut a, json!(too_many)), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn accessibility_check_locates_field_description_on_page() {
    let dir = workdir("accessibility-field-location");
    std::fs::write(dir.join("field.pdf"), form_pdf("name")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "field.pdf" }))["doc"].as_u64().unwrap();
    let report = ok(&mut a, "accessibility_check", json!({ "doc": doc, "rules": ["field-descriptions"] }));
    let rule = report["results"].as_array().unwrap().iter().find(|r| r["rule"] == "field-descriptions").unwrap();
    assert_eq!(rule["status"], "failed");
    assert_eq!(rule["findings"][0]["page"], 1);
    assert_eq!(rule["findings"][0]["bbox_user_space"], json!([60.0, 60.0, 200.0, 80.0]));
}

#[test]
fn tool_table_is_well_formed() {
    let all = tools();
    let mut names = std::collections::HashSet::new();
    for t in &all {
        assert!(names.insert(t.name), "duplicate tool {}", t.name);
        assert!(t.name.len() <= 64 && t.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "bad MCP tool name {}", t.name);
        assert_eq!(t.input_schema["type"], "object", "{}", t.name);
        let props = t.input_schema["properties"].as_object().unwrap();
        for r in t.input_schema["required"].as_array().unwrap() {
            assert!(props.contains_key(r.as_str().unwrap()), "{}: required {r} is not a property", t.name);
        }
        assert!(!(t.read_only && t.destructive), "{} is both read-only and destructive", t.name);
        for k in ["oneOf", "anyOf", "allOf"] {
            assert!(t.input_schema.get(k).is_none(), "{}: top-level {k} in input_schema is rejected by MCP clients such as Claude", t.name);
        }
        if let Some(c) = t.command {
            assert!(pdfcraft_engine::commands::command(c).is_some(), "{} names unregistered command {c}", t.name);
        }
    }
}

#[test]
fn tool_schema_enums_have_one_json_type() {
    for tool in tools() {
        assert_homogeneous_enums(&tool.input_schema, tool.name);
    }
}

#[test]
fn explicit_font_and_disjoint_ranges_save_reopen_through_tools() {
    let dir = workdir("font-ranges");
    std::fs::write(dir.join("inter.ttf"), include_bytes!("../../../../../assets/fonts/Inter-Regular.ttf")).unwrap();
    let mut a = auto(&dir);
    std::fs::write(dir.join("broken.otf"), b"not a font").unwrap();
    let found = ok(&mut a, "text_font_files", json!({}));
    assert_eq!(found["fonts"].as_array().unwrap().len(), 1);
    assert_eq!(found["fonts"][0]["font"], "Inter-Regular");
    assert!(a.call("text_font_files", &json!({"directories":["../outside"]})).is_err());
    let id = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
    let font = ok(&mut a, "text_font_import", json!({"doc":id,"page":1,"path":"inter.ttf"}));
    let resource = font["resource"].as_str().unwrap();
    assert_eq!(font["font"], "Inter-Regular");
    assert_eq!(page_text(&mut a, id), ["Page 1", "Page 2", "Page 3"]);
    let draft = ok(
        &mut a,
        "text_edit_layout",
        json!({"doc":id,"page":1,"paragraph":1,"text":"Aé ΩЖ","font_resource":resource,"ranges":[{"range":[0,3],"size":16,"color":"#cc0000"},{"range":[4,8],"size":20,"color":"#003399"}]}),
    );
    assert!(draft["draft_rect"].is_array());
    assert_eq!(page_text(&mut a, id), ["Page 1", "Page 2", "Page 3"]);
    ok(
        &mut a,
        "text_edit",
        json!({"doc":id,"page":1,"paragraph":1,"text":"Aé ΩЖ","font_resource":resource,"ranges":[{"range":[0,3],"size":16,"color":"#cc0000"},{"range":[4,8],"size":20,"color":"#003399"}]}),
    );
    let lines = ok(&mut a, "text_lines", json!({"doc":id,"page":1}));
    assert!(
        lines["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["text"].as_str().is_some_and(|s| s.trim_end() == "Aé") && l["size"].as_f64() == Some(16.0)),
        "{lines}"
    );
    let rendered = a.call("page_render", &json!({"doc":id,"page":1,"dpi":72})).unwrap();
    ok(&mut a, "doc_save", json!({"doc":id,"path":"edited.pdf"}));
    let reopened = ok(&mut a, "doc_open", json!({"path":"edited.pdf"}))["doc"].as_u64().unwrap();
    let saved = ok(&mut a, "text_lines", json!({"doc":reopened,"page":1}));
    assert_eq!(lines, saved);
    assert_eq!(rendered, a.call("page_render", &json!({"doc":reopened,"page":1,"dpi":72})).unwrap());
    let before = saved.clone();
    assert!(a.call("text_edit", &json!({"doc":reopened,"page":1,"paragraph":1,"text":"é test","ranges":[{"range":[1,2],"size":18}]})).is_err());
    assert_eq!(ok(&mut a, "text_lines", json!({"doc":reopened,"page":1})), before);
    assert!(a.call("text_font_import", &json!({"doc":id,"page":1,"path":"../outside.ttf"})).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn added_arabic_text_keeps_copy_search_and_editor_order_after_retyping_and_reopen() {
    if pdfcraft_fonts::document_arabic_font().is_none() {
        eprintln!("set CRAFT_FONTS_DIR to exercise added Arabic text");
        return;
    }
    let dir = workdir("arabic-copy-order");
    let mut a = auto(&dir);
    let id = ok(&mut a, "doc_create", json!({"from":"blank","width":600,"height":400}))["doc"].as_u64().unwrap();
    let original = "سَلَام 123 (PDF) - اختبار الكتابة العربية";
    ok(&mut a, "page_add_text", json!({"doc":id,"page":1,"text":original,"at":[40,50],"width":500,"size":16,"align":"right"}));
    assert_eq!(page_text(&mut a, id), [original]);
    assert_eq!(ok(&mut a, "text_lines", json!({"doc":id,"page":1}))["lines"][0]["text"], original);
    let found = ok(&mut a, "text_find", json!({"doc":id,"query":original}));
    assert_eq!(found["matches"].as_array().unwrap().len(), 1);
    let retyped = "مُرَاجَعَة PDF رقم ١٢٣";
    ok(&mut a, "content_update", json!({"doc":id,"page":1,"index":1,"text":retyped}));
    ok(&mut a, "doc_save", json!({"doc":id,"path":"retyped.pdf"}));
    let re = ok(&mut a, "doc_open", json!({"path":"retyped.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, re), [retyped]);
    assert_eq!(ok(&mut a, "text_lines", json!({"doc":re,"page":1}))["lines"][0]["text"], retyped);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn arabic_actual_text_stays_logical_through_editor_tools_and_save_reopen() {
    use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, Stream, write_full};
    use std::sync::Arc;

    let dir = workdir("arabic-actual-text");
    let logical = "سَلَام 123 (PDF)";
    let hex: String = logical.encode_utf16().map(|u| format!("{u:04X}")).collect();
    let content = format!("/Span <</ActualText <FEFF{hex}>>> BDC BT /F1 24 Tf 20 150 Td (abcd) Tj (efgh) Tj ET EMC");
    let mut doc = Document::open(Arc::new(fixture(1))).unwrap();
    doc.set(ObjRef::new(5, 0), Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    std::fs::write(dir.join("arabic.pdf"), write_full(&doc, &SaveOptions::default()).unwrap()).unwrap();

    let mut a = auto(&dir);
    let id = ok(&mut a, "doc_open", json!({"path":"arabic.pdf"}))["doc"].as_u64().unwrap();
    let lines = ok(&mut a, "text_lines", json!({"doc":id,"page":1}));
    assert_eq!(lines["lines"][0]["text"], logical);
    let render = a.call("page_render", &json!({"doc":id,"page":1,"dpi":72})).unwrap();
    ok(&mut a, "doc_save", json!({"doc":id,"path":"saved.pdf"}));
    let re = ok(&mut a, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_lines", json!({"doc":re,"page":1})), lines);
    assert_eq!(a.call("page_render", &json!({"doc":re,"page":1,"dpi":72})).unwrap(), render);
    ok(&mut a, "text_edit", json!({"doc":re,"page":1,"paragraph":1,"text":"Replacement"}));
    ok(&mut a, "doc_save", json!({"doc":re,"path":"edited.pdf"}));
    let edited = ok(&mut a, "doc_open", json!({"path":"edited.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_lines", json!({"doc":edited,"page":1}))["lines"][0]["text"], "Replacement");
    assert_eq!(page_text(&mut a, edited), ["Replacement"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn arabic_ligature_actual_text_is_one_editable_word_through_save_reopen() {
    use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, Stream, write_full};
    use std::sync::Arc;

    let dir = workdir("arabic-ligature-actual-text");
    let mut doc = Document::open(Arc::new(fixture(1))).unwrap();
    // Synthetic glyph codes: A س, B م, X lam-alef. No installed Arabic font is needed.
    let cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapType 2 def \
                1 begincodespacerange <00> <FF> endcodespacerange 3 beginbfchar \
                <41> <0633> <42> <0645> <58> <FEFB> \
                endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
    let unicode = doc.add(Object::Stream(Stream::flate(Dict::new(), cmap.as_bytes())));
    doc.update_dict(ObjRef::new(3, 0), |d| d.set(b"ToUnicode".to_vec(), Object::Ref(unicode))).unwrap();
    let content = "BT /F1 24 Tf 20 150 Td (B) Tj /Span <</ActualText <FEFF06440627>>> BDC (X) Tj EMC (A) Tj ET";
    doc.set(ObjRef::new(5, 0), Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    std::fs::write(dir.join("arabic.pdf"), write_full(&doc, &SaveOptions::default()).unwrap()).unwrap();

    let mut a = auto(&dir);
    let id = ok(&mut a, "doc_open", json!({"path":"arabic.pdf"}))["doc"].as_u64().unwrap();
    let lines = ok(&mut a, "text_lines", json!({"doc":id,"page":1}));
    assert_eq!(lines["lines"].as_array().unwrap().len(), 1);
    assert_eq!(lines["lines"][0]["text"], "سلام");
    assert_eq!(page_text(&mut a, id), ["سلام"]);
    assert_eq!(ok(&mut a, "text_find", json!({"doc":id,"query":"سلام"}))["matches"].as_array().unwrap().len(), 1);
    let render = a.call("page_render", &json!({"doc":id,"page":1,"dpi":72})).unwrap();
    ok(&mut a, "doc_save", json!({"doc":id,"path":"saved.pdf"}));
    let re = ok(&mut a, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_lines", json!({"doc":re,"page":1})), lines);
    assert_eq!(a.call("page_render", &json!({"doc":re,"page":1,"dpi":72})).unwrap(), render);
    ok(&mut a, "text_edit", json!({"doc":re,"page":1,"paragraph":1,"text":"Replacement"}));
    ok(&mut a, "doc_save", json!({"doc":re,"path":"edited.pdf"}));
    let edited = ok(&mut a, "doc_open", json!({"path":"edited.pdf"}))["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, edited), ["Replacement"]);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Each of these writes to its required `path` and replaces an existing file there, so the MCP
/// annotations must not tell clients the call is read-only or harmless (#130). The OCR
/// dictionary's updates rewrite the file in place (lower-cased and sorted), so it counts too.
#[test]
fn file_writing_tools_are_not_read_only() {
    for name in ["doc_export_data", "accessibility_report", "image_save", "ocr_dictionary"] {
        let t = tools().into_iter().find(|t| t.name == name).unwrap();
        assert!(!t.read_only, "{name} writes a file but advertises read-only");
        assert!(t.destructive, "{name} overwrites its path but advertises non-destructive");
    }
}

#[test]
fn open_inspect_render_and_find() {
    let dir = workdir("inspect");
    let mut a = auto(&dir);
    let opened = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }));
    assert_eq!(opened["pages"], 3);
    assert_eq!(opened["editable"], true);
    let doc = opened["doc"].as_u64().unwrap();

    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert_eq!(info["pages"].as_array().unwrap().len(), 3);
    assert_eq!(info["pages"][0]["width"], 200.0);
    assert_eq!(info["pages"][1]["label"], "2");

    // The fixture names /Helvetica without embedding it, so an agent can see both that a face is
    // standing in and why that face. `substitution` is null only when nothing stands in.
    let font = &info["fonts"][0];
    assert_eq!((&font["name"], &font["embedded"]), (&json!("Helvetica"), &json!(false)));
    assert_eq!(font["substitute"], json!("Helvetica"));
    assert!(font["substitution"].as_str().is_some_and(|w| w.contains("standard 14")), "{font}");

    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 3"]);

    let found = ok(&mut a, "text_find", json!({ "doc": doc, "query": "page  2" }));
    assert_eq!(found["count"], 1);
    assert_eq!(found["matches"][0]["page"], 2);
    let rect = found["matches"][0]["rects"][0].as_array().unwrap();
    assert!(rect[1].as_f64().unwrap() > 100.0 && rect[3].as_f64().unwrap() < 160.0, "rect is top-left based: {rect:?}");

    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    let Content::Png { data, width, height } = &png[0] else { panic!("expected an image") };
    assert_eq!((*width, *height), (200, 300));
    assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
    let decoder = png::Decoder::new(std::io::Cursor::new(data.as_slice()));
    let info = decoder.read_info().unwrap().info().clone();
    assert_eq!((info.width, info.height), (200, 300));
}

#[test]
fn word_count_reads_selected_pages_and_unsaved_edits_without_mutating() {
    let dir = workdir("word-count");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let before = ok(&mut a, "doc_list", json!({}));
    let report = ok(&mut a, "text_word_count", json!({ "doc": doc }));
    assert_eq!(report["total"]["words"], 6);
    assert_eq!(report["pages"].as_array().unwrap().len(), 3);
    let subset = ok(&mut a, "text_word_count", json!({ "doc": doc, "pages": [3, 1], "exclude_tables": true }));
    assert_eq!(subset["total"]["words"], 4);
    assert_eq!(subset["pages"][0]["page"], 3);
    assert_eq!(subset["unclassified_structure"], true);
    assert_eq!(ok(&mut a, "doc_list", json!({})), before);
    assert!(a.call("text_word_count", &json!({ "doc": doc, "pages": [4] })).is_err());
    ok(&mut a, "page_delete", json!({ "doc": doc, "pages": [1] }));
    assert_eq!(ok(&mut a, "text_word_count", json!({ "doc": doc }))["total"]["words"], 4);
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(ok(&mut a, "text_word_count", json!({ "doc": doc }))["total"]["words"], 6);
    let def = tools().into_iter().find(|t| t.name == "text_word_count").unwrap();
    assert!(def.read_only && !def.destructive);
}

#[test]
fn text_editor_background_is_read_only_and_removes_source_glyphs() {
    let dir = workdir("text-preview");
    let mut a = auto(&dir);
    std::fs::write(dir.join("text-preview.pdf"), fixture(2)).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "text-preview.pdf" }))["doc"].as_u64().unwrap();
    let args = json!({ "doc": doc, "page": 1, "dpi": 72 });
    let before = a.call("page_render", &args).unwrap();
    let state = ok(&mut a, "doc_info", json!({ "doc": doc }));
    let preview = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72, "hide_text_block": 1 })).unwrap();
    assert_ne!(preview, before);
    let Content::Png { data, .. } = &preview[0] else { panic!("expected PNG") };
    let mut reader = png::Decoder::new(std::io::Cursor::new(data)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert!(pixels[..info.buffer_size()].iter().all(|b| *b == 255), "no source glyphs remain on the white page");
    assert_eq!(a.call("page_render", &args).unwrap(), before);
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc })), state);
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2"]);
    for block in [-1, 0, 2, i64::MAX] {
        assert!(a.call("page_render", &json!({ "doc": doc, "page": 1, "hide_text_block": block })).is_err());
    }
}

#[test]
fn added_text_editor_background_hides_only_the_item_without_editing() {
    let dir = workdir("added-text-preview");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let render = json!({"doc":doc,"page":1,"dpi":72});
    let before = a.call("page_render", &render).unwrap();
    ok(&mut a, "page_add_text", json!({"doc":doc,"page":1,"text":"Retype this label","at":[20,20],"width":160}));
    let drawn = a.call("page_render", &render).unwrap();
    assert_ne!(drawn, before);
    let state = ok(&mut a, "doc_info", json!({"doc":doc}));
    let items = ok(&mut a, "content_list", json!({"doc":doc}));
    let preview = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72,"hide_added_text":1})).unwrap();
    assert_eq!(preview, before, "native text and the page background stay intact");
    assert_eq!(a.call("page_render", &render).unwrap(), drawn);
    assert_eq!(ok(&mut a, "doc_info", json!({"doc":doc})), state);
    assert_eq!(ok(&mut a, "content_list", json!({"doc":doc})), items);
    for index in [-1, 0, 2, i64::MAX] {
        assert!(a.call("page_render", &json!({"doc":doc,"page":1,"hide_added_text":index})).is_err());
    }
    assert!(a.call("page_render", &json!({"doc":doc,"page":1,"hide_added_text":1,"hide_text_block":1})).is_err());
}

#[test]
fn edit_undo_redo_and_save_round_trip() {
    let dir = workdir("edit");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();

    let s = ok(&mut a, "page_delete", json!({ "doc": doc, "pages": [2] }));
    assert_eq!((s["pages"].as_u64(), s["dirty"].as_bool()), (Some(2), Some(true)));
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 3"]);

    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 3"]);
    ok(&mut a, "edit_redo", json!({ "doc": doc }));
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 3"]);

    ok(&mut a, "page_insert_file", json!({ "doc": doc, "path": "b.pdf", "pages": [2], "at": 1 }));
    ok(&mut a, "page_insert_blank", json!({ "doc": doc, "at": 4 }));
    ok(&mut a, "page_move", json!({ "doc": doc, "pages": [3], "to": 1 }));
    ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [1], "degrees": 90 }));
    ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Automated" }));
    assert_eq!(page_text(&mut a, doc), ["Page 3", "Page 2", "Page 1", ""]);

    let saved = ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out/edited.pdf" }));
    assert_eq!(saved["incremental"], false);
    assert_eq!(saved["document"]["dirty"], false);

    let mut b = auto(&dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "out/edited.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut b, "doc_info", json!({ "doc": re }));
    assert_eq!(info["title"], "Automated");
    assert_eq!(info["pages"][0]["rotation"], 90);
    assert_eq!(page_text(&mut b, re), ["Page 3", "Page 2", "Page 1", ""]);

    // Saving in place appends an incremental update.
    ok(&mut b, "doc_set_info", json!({ "doc": re, "key": "Author", "value": "Agent" }));
    let before = std::fs::metadata(dir.join("out/edited.pdf")).unwrap().len();
    let again = ok(&mut b, "doc_save", json!({ "doc": re }));
    assert_eq!(again["incremental"], true);
    let after = std::fs::read(dir.join("out/edited.pdf")).unwrap();
    assert!(after.len() as u64 > before);
    assert_eq!(after.windows(5).filter(|w| w == b"%%EOF").count(), 2);
}

#[test]
fn page_delete_handles_deep_destination_trees_with_history_and_reopen() {
    delete_destination_tree_through_tools(16_384, 2, "deep-destinations");
}

#[test]
fn page_delete_handles_sixty_thousand_destinations_with_history_and_reopen() {
    delete_destination_tree_through_tools(0, 60_000, "many-destinations");
}

/// Issue #1329: an in-place save says it was incremental only when it kept the file it was
/// saving over: protecting or sanitizing rewrites the file, and the result says so.
#[test]
fn an_in_place_save_reports_a_required_rewrite_as_not_incremental() {
    let dir = workdir("save-incremental-flag");
    let original = std::fs::read(dir.join("a.pdf")).unwrap();
    let cases = [
        ("page_rotate", json!({ "pages": [1], "degrees": 90 }), true),
        ("doc_protect", json!({ "open_password": "openme" }), false),
        ("doc_remove_hidden", json!({}), false),
    ];
    for (tool, args, incremental) in cases {
        std::fs::write(dir.join("a.pdf"), &original).unwrap();
        let mut a = auto(&dir);
        let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
        let mut args = args;
        args["doc"] = json!(doc);
        ok(&mut a, tool, args);
        let saved = ok(&mut a, "doc_save", json!({ "doc": doc }));
        let kept = std::fs::read(dir.join("a.pdf")).unwrap().starts_with(&original);
        assert_eq!((saved["incremental"].as_bool(), kept), (Some(incremental), incremental), "{tool}");
    }
}

/// #1664 repairs the missing page tree on open. The former #1698 failure fixture now
/// saves successfully, keeping its page and pixels through a full rewrite.
#[test]
fn a_recovered_page_tree_survives_a_full_save() {
    let dir = workdir("save-verify-before-write");
    let objs: [&[u8]; 4] = [
        b"<< /Type /Catalog /Pages 3 0 R >>",
        b"<< /Type /Page /MediaBox [0 0 200 300] /Contents 4 0 R >>",
        b"[ 0 ]",
        b"<< /Length 18 >>\nstream\n0 0 m 100 100 l S\nendstream",
    ];
    let mut original = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(original.len());
        original.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        original.extend_from_slice(o);
        original.extend_from_slice(b"\nendobj\n");
    }
    let xref = original.len();
    original.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for o in offsets {
        original.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    original.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    std::fs::write(dir.join("damaged.pdf"), &original).unwrap();

    let mut a = auto(&dir);
    let opened = ok(&mut a, "doc_open", json!({ "path": "damaged.pdf" }));
    assert_eq!(opened["pages"], 1, "{opened}");
    let doc = opened["doc"].as_u64().unwrap();
    let (width, height, pixels) = a.render_pixels(doc, 1, 1.0).unwrap();
    assert!(pixels.as_ref().as_chunks::<4>().0.iter().any(|p| p[..3] != [255, 255, 255]), "the original page is not blank");
    let saved = ok(&mut a, "doc_save", json!({ "doc": doc, "full": true }));
    assert_eq!(saved["document"]["dirty"], false);
    let on_disk = std::fs::read(dir.join("damaged.pdf")).unwrap();
    assert_ne!(on_disk, original, "a full save writes the repaired page tree");
    let mut reopened = auto(&dir);
    let opened = ok(&mut reopened, "doc_open", json!({ "path": "damaged.pdf" }));
    assert_eq!(opened["pages"], 1);
    let again = opened["doc"].as_u64().unwrap();
    let (new_width, new_height, new_pixels) = reopened.render_pixels(again, 1, 1.0).unwrap();
    assert_eq!((new_width, new_height), (width, height));
    assert_eq!(new_pixels.as_ref(), pixels.as_ref(), "saving must preserve the recovered page's drawing");
}

/// organize.field-name-conflicts: combining files whose forms share a field name keeps the
/// fields separate by renaming, says so in the result, and can be told to merge instead; any
/// other policy is an argument error.
#[test]
fn combine_renames_colliding_field_names() {
    let dir = workdir("combine-field-conflicts");
    std::fs::write(dir.join("form-a.pdf"), form_pdf("Text1")).unwrap();
    std::fs::write(dir.join("form-b.pdf"), form_pdf("Text1")).unwrap();
    let mut a = auto(&dir);
    let combined = ok(&mut a, "doc_combine", json!({ "paths": ["form-a.pdf", "form-b.pdf"], "open": true }));
    assert_eq!(combined["renamed_fields"], json!([["Text1", "Text1-1"]]), "{combined}");
    let merged = ok(&mut a, "doc_combine", json!({ "paths": ["form-a.pdf", "form-b.pdf"], "field_conflicts": "merge" }));
    assert!(merged.get("renamed_fields").is_none(), "merge renames nothing: {merged}");
    let bad = a.call("doc_combine", &json!({ "paths": ["form-a.pdf", "form-b.pdf"], "field_conflicts": "bogus" }));
    assert!(matches!(bad, Err(ToolError::InvalidArgs(_))));
}

/// #569: images and text files combine with PDFs, each made into a PDF first.
#[test]
fn combine_takes_images_and_text_files() {
    let dir = workdir("combine-images");
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 4, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[90; 24]).unwrap();
    }
    std::fs::write(dir.join("scan.png"), png).unwrap();
    std::fs::write(dir.join("notes.txt"), "Some notes").unwrap();
    std::fs::write(dir.join("broken.png"), b"\x89PNG\r\n\x1a\nbroken").unwrap();
    std::fs::write(dir.join("report.docx"), b"PK\x03\x04").unwrap();
    let mut a = auto(&dir);
    let combined = ok(&mut a, "doc_combine", json!({ "paths": ["a.pdf", "scan.png", "notes.txt"], "pages": [null, null, "1"], "open": true }));
    let doc = combined["document"]["doc"].as_u64().unwrap();
    assert_eq!(combined["document"]["pages"], 5);
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 3", "", "Some notes"]);
    let marks = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    let titles: Vec<&str> = marks.as_array().unwrap().iter().map(|b| b["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["a", "scan", "notes"]);
    // A file that can't be made into a PDF stops the combine, and says which.
    for bad in ["broken.png", "report.docx"] {
        let refused = a.call("doc_combine", &json!({ "paths": ["a.pdf", bad] }));
        assert!(matches!(&refused, Err(ToolError::Failed(e)) if e.contains(bad)), "{bad}: {refused:?}");
    }
}

#[test]
fn combine_extract_and_split() {
    let dir = workdir("organize");
    let mut a = auto(&dir);
    let combined = ok(&mut a, "doc_combine", json!({ "paths": ["a.pdf", "b.pdf"], "out": "ab.pdf", "open": true }));
    let doc = combined["document"]["doc"].as_u64().unwrap();
    assert_eq!(combined["document"]["pages"], 5);
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 3", "Page 1", "Page 2"]);
    // Chosen pages per file, in the order given.
    let some = ok(&mut a, "doc_combine", json!({ "paths": ["a.pdf", "b.pdf"], "pages": ["3, 1", null], "open": true }));
    assert_eq!(page_text(&mut a, some["document"]["doc"].as_u64().unwrap()), ["Page 3", "Page 1", "Page 1", "Page 2"]);
    assert!(matches!(a.call("doc_combine", &json!({ "paths": ["a.pdf", "b.pdf"], "pages": ["9", null] })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("doc_combine", &json!({ "paths": ["a.pdf", "b.pdf"], "pages": ["1"] })), Err(ToolError::InvalidArgs(_))));

    // A file split around another, as the Combine files grid makes it: copied once, one bookmark.
    let split = ok(
        &mut a,
        "doc_combine",
        json!({ "paths": ["a.pdf", "b.pdf", "a.pdf"], "pages": ["1", null, "3, 2"], "groups": [4, null, 4], "open": true }),
    );
    let split_doc = split["document"]["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, split_doc), ["Page 1", "Page 1", "Page 2", "Page 3", "Page 2"]);
    let marks = ok(&mut a, "bookmark_list", json!({ "doc": split_doc }))["bookmarks"].clone();
    let marks: Vec<_> = marks.as_array().unwrap().iter().map(|b| (b["title"].as_str().unwrap().to_string(), b["page"].as_u64().unwrap())).collect();
    assert_eq!(marks, [("a".to_string(), 1), ("b".to_string(), 2)]);
    // Without groups the same paths are separate files, each with its own bookmark.
    let apart = ok(&mut a, "doc_combine", json!({ "paths": ["a.pdf", "b.pdf", "a.pdf"], "pages": ["1", null, "3, 2"], "open": true }));
    let apart = ok(&mut a, "bookmark_list", json!({ "doc": apart["document"]["doc"].as_u64().unwrap() }))["bookmarks"].clone();
    assert_eq!(apart.as_array().unwrap().len(), 3);
    // A group names one file; groups must be in step with paths and whole numbers.
    let not_same = a.call("doc_combine", &json!({ "paths": ["a.pdf", "b.pdf"], "groups": [1, 1] }));
    assert!(matches!(not_same, Err(ToolError::InvalidArgs(e)) if e.contains("group 1")));
    for groups in [json!([1]), json!([1, -1]), json!([1, 1.5]), json!(["1", null]), json!(7)] {
        let bad = a.call("doc_combine", &json!({ "paths": ["a.pdf", "b.pdf"], "groups": groups }));
        assert!(matches!(bad, Err(ToolError::InvalidArgs(_))), "{groups}");
    }
    // The largest group number still leaves a key for a file of its own, or says why not.
    let max = ok(&mut a, "doc_combine", json!({ "paths": ["a.pdf", "b.pdf"], "groups": [u64::MAX - 1, null], "open": true }));
    assert_eq!(max["document"]["pages"], 5);
    let full = a.call("doc_combine", &json!({ "paths": ["a.pdf", "b.pdf"], "groups": [u64::MAX, null] }));
    assert!(matches!(full, Err(ToolError::InvalidArgs(_))));

    let ex = ok(&mut a, "page_extract", json!({ "doc": doc, "pages": [2, 4] }));
    let ex_doc = ex["document"]["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, ex_doc), ["Page 2", "Page 1"]);
    assert!(ex.get("path").is_none());

    let split = ok(&mut a, "doc_split", json!({ "doc": doc, "every": 2, "out_dir": "parts" }));
    let files = split["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!((files[2]["first_page"].as_u64(), files[2]["last_page"].as_u64()), (Some(5), Some(5)));
    for f in files {
        assert!(Path::new(f["path"].as_str().unwrap()).starts_with(dir.canonicalize().unwrap()));
    }
}

#[test]
fn errors_are_specific_and_safe() {
    let dir = workdir("errors");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();

    let err = |a: &mut Automation, tool: &str, args: Value| a.call(tool, &args).unwrap_err();
    assert!(matches!(err(&mut a, "nope", json!({})), ToolError::UnknownTool(_)));
    assert!(matches!(err(&mut a, "page_delete", json!({ "doc": doc, "pages": [9] })), ToolError::InvalidArgs(m) if m.contains("3 pages")));
    assert!(matches!(err(&mut a, "page_delete", json!({ "doc": doc, "pages": [0] })), ToolError::InvalidArgs(_)));
    assert!(matches!(err(&mut a, "page_delete", json!({ "doc": doc, "page": [1] })), ToolError::InvalidArgs(m) if m.contains("unknown argument")));
    assert!(matches!(err(&mut a, "page_rotate", json!({ "doc": doc, "pages": [1], "degrees": 45 })), ToolError::InvalidArgs(_)));
    assert!(matches!(err(&mut a, "doc_info", json!({ "doc": 99 })), ToolError::Failed(_)));
    assert!(matches!(err(&mut a, "edit_undo", json!({ "doc": doc })), ToolError::Failed(_)));

    // Unsaved changes are never dropped silently.
    ok(&mut a, "page_delete", json!({ "doc": doc, "pages": [1] }));
    assert!(matches!(err(&mut a, "doc_close", json!({ "doc": doc })), ToolError::Failed(m) if m.contains("unsaved")));
    ok(&mut a, "doc_close", json!({ "doc": doc, "discard_changes": true }));
    assert_eq!(ok(&mut a, "doc_list", json!({}))["documents"], json!([]));

    // The root confines reads and writes.
    let outside = std::env::temp_dir().join("pdfcraft-automation-outside.pdf");
    std::fs::write(&outside, fixture(1)).unwrap();
    assert!(matches!(err(&mut a, "doc_open", json!({ "path": outside.to_str().unwrap() })), ToolError::Failed(m) if m.contains("outside")));
    assert!(
        matches!(err(&mut a, "doc_open", json!({ "path": "../pdfcraft-automation-outside.pdf" })), ToolError::Failed(m) if m.contains("outside"))
    );
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert!(a.call("doc_save", &json!({ "doc": doc, "path": "new/../../escape.pdf" })).is_err());
    assert!(a.call("doc_save", &json!({ "doc": doc, "path": outside.to_str().unwrap() })).is_err());
    let _ = std::fs::remove_file(outside);
}

#[test]
fn root_refusals_do_not_reveal_what_exists_outside() {
    // Regression test for #136: every path outside the root gets the same refusal, whether it
    // exists, is a file or a folder, or passes through a missing folder.
    let (base, root) = sandbox("root-oracle");
    let mut a = auto(&root);
    let refusal = |p: &str| ToolError::Failed(format!("{p} is outside the allowed directory {}", root.display()));
    let abs = |rel: &str| base.join(rel).to_str().unwrap().to_owned();

    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut reads = vec![
        "../outside/secret.pdf".to_owned(),
        "../outside/nope.pdf".into(),
        "../outside/secret.pdf/x".into(),
        "../outside/sub".into(),
        "../outside/sub/x".into(),
        "../nowhere/at/all.pdf".into(),
        "missing/../../outside/secret.pdf".into(),
        "missing/../../outside/nope.pdf".into(),
        "../../../../../../../../../../../../../../../../../../../../../../../../x.pdf".into(),
        abs("outside/secret.pdf"),
        abs("outside/nope.pdf"),
        abs("outside/sub/x"),
        abs("nowhere/x.pdf"),
        abs("nowhere/../outside/nope.pdf"),
        abs("outside/../outside/secret.pdf"),
        abs("root/../outside/secret.pdf"),
    ];
    #[cfg(windows)]
    {
        reads.extend([
            r"..\outside\secret.pdf".to_owned(),
            r"..\outside/nope.pdf".into(),
            "../outside/secret.pdf.".into(),
            "../outside/secret.pdf ".into(),
            // Another network share or device namespace is refused by name, without contacting
            // it (`.invalid` never resolves, so a regression fails instead of reaching a host).
            r"\\pdfcraft-test.invalid\share\secret.pdf".into(),
            "//pdfcraft-test.invalid/share/secret.pdf".into(),
            r"\\?\UNC\pdfcraft-test.invalid\share\secret.pdf".into(),
            r"\\.\pipe\pdfcraft-test".into(),
            r"\\?\GLOBALROOT\Device\Null".into(),
        ]);
        let other = base.join("outside/secret.pdf").canonicalize().unwrap();
        reads.push(other.to_str().unwrap().to_owned()); // the verbatim \\?\C:\… form
        if let Some(drive) = (b'D'..=b'Z').rev().map(|d| format!("{}:\\", d as char)).find(|d| !Path::new(d).exists()) {
            reads.push(format!("{drive}secret.pdf")); // a drive that doesn't exist
        }
    }
    for p in &reads {
        assert_eq!(a.call("doc_open", &json!({ "path": p })).unwrap_err(), refusal(p), "reading {p}");
    }

    let writes = [
        "../outside/sub/../y.pdf".to_owned(),
        "../outside/nosub/../y.pdf".into(),
        "../outside/y.pdf".into(),
        "../outside/nosub/y.pdf".into(),
        "../outside/secret.pdf".into(),
        "../outside/secret.pdf/y.pdf".into(),
        "new/../../escape.pdf".into(),
        abs("outside/nosub/deeper/y.pdf"),
    ];
    let doc = ok(&mut a, "doc_open", json!({ "path": "inside.pdf" }))["doc"].as_u64().unwrap();
    for p in &writes {
        assert_eq!(a.call("doc_save", &json!({ "doc": doc, "path": p })).unwrap_err(), refusal(p), "writing {p}");
    }
    #[cfg(windows)]
    {
        // In a verbatim path `/` is not a separator, so `x/../..` can't climb out of it either.
        for tail in [r"\x/../../outside/v.pdf", r"\x/../../outside/secret.pdf"] {
            let p = format!("{}{tail}", root.display());
            if let Ok(c) = a.call("doc_save", &json!({ "doc": doc, "path": p })) {
                let Content::Json(v) = &c[0] else { panic!("{p}: expected JSON") };
                assert!(Path::new(v["path"].as_str().unwrap()).starts_with(&root), "{p} wrote {v}");
            }
            // (What lands outside the root, if anything, is checked below.)
        }
    }

    // A link inside the root that leads out of it is refused the same way, below it too.
    link_dir(&base.join("root"), "link", &base.join("outside"));
    for p in ["link/secret.pdf", "link/nope.pdf", "link/sub/x", "link"] {
        assert_eq!(a.call("doc_open", &json!({ "path": p })).unwrap_err(), refusal(p), "reading {p}");
    }
    for p in ["link/new.pdf", "link/nosub/new.pdf", "link/secret.pdf"] {
        assert_eq!(a.call("doc_save", &json!({ "doc": doc, "path": p })).unwrap_err(), refusal(p), "writing {p}");
    }
    // So is a link whose target is gone: whether a link's target exists stays hidden too.
    std::fs::create_dir_all(base.join("outside/gone")).unwrap();
    link_dir(&base.join("root"), "broken", &base.join("outside/gone"));
    std::fs::remove_dir(base.join("outside/gone")).unwrap();
    for p in ["broken", "broken/x.pdf", "broken/x/y.pdf"] {
        assert_eq!(a.call("doc_open", &json!({ "path": p })).unwrap_err(), refusal(p), "reading {p}");
        assert_eq!(a.call("doc_save", &json!({ "doc": doc, "path": p })).unwrap_err(), refusal(p), "writing {p}");
    }

    // Nothing was written outside the root, and the outside files are untouched.
    let mut left: Vec<String> =
        std::fs::read_dir(base.join("outside")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["secret.pdf", "sub"]);
    assert_eq!(std::fs::read(base.join("outside/secret.pdf")).unwrap(), fixture(1));
    assert!(!base.join("escape.pdf").exists() && !base.join("y.pdf").exists());

    // Inside the root nothing changes: missing files say so, and existing ones open and save,
    // however the path is spelled.
    let missing = a.call("doc_open", &json!({ "path": "missing-inside.pdf" })).unwrap_err();
    assert!(
        matches!(&missing, ToolError::Failed(m) if m.starts_with("missing-inside.pdf not found (looked for ") && !m.contains("outside")),
        "{missing:?}"
    );
    let not_dir = a.call("doc_open", &json!({ "path": "inside.pdf/x" })).unwrap_err();
    assert!(matches!(&not_dir, ToolError::Failed(m) if !m.contains("outside")), "{not_dir:?}");
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut opens = vec![
        "inside.pdf".to_owned(),
        "./inside.pdf".into(),
        "sub/../inside.pdf".into(),
        "../root/inside.pdf".into(),
        "../outside/../root/inside.pdf".into(),
        "../nowhere/../root/inside.pdf".into(),
        abs("root/inside.pdf"),
        abs("outside/../root/inside.pdf"),
        abs("nowhere/../root/inside.pdf"),
        root.join("inside.pdf").to_str().unwrap().to_owned(),
    ];
    #[cfg(windows)]
    {
        let plain = abs("root/inside.pdf");
        opens.extend([plain.to_lowercase(), plain.to_uppercase(), r"..\root\inside.pdf".into()]);
    }
    for p in &opens {
        ok(&mut a, "doc_open", json!({ "path": p }));
    }
    for p in ["new.pdf", "fresh/dir/new.pdf", "fresh/../also-new.pdf"] {
        ok(&mut a, "doc_save", json!({ "doc": doc, "path": p }));
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": abs("root/abs-new.pdf") }));
    for f in ["new.pdf", "fresh/dir/new.pdf", "also-new.pdf", "abs-new.pdf"] {
        assert!(root.join(f).is_file(), "{f} was written inside the root");
    }

    // Without a root, paths are used as given.
    let mut free = Automation::new();
    ok(&mut free, "doc_open", json!({ "path": abs("outside/secret.pdf") }));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn writing_to_a_folder_touches_nothing_beside_it() {
    // "." names the root itself. Saving there used to stage its temporary file next to the
    // root, outside it, overwriting and then deleting any file of that name. Staging names are
    // random now and a failed rename removes the staging file, so the "is a folder" refusal is
    // what this checks; the listings and the file at the old staging name are canaries.
    let (base, root) = sandbox("root-itself");
    let mut a = auto(&root);
    let beside = base.join(".root.pdfcraft-tmp");
    std::fs::write(&beside, "SENTINEL").unwrap();
    std::fs::create_dir_all(root.join("folder")).unwrap();
    let listing = |dir: &Path| {
        let mut names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        names
    };
    let (base_before, folder_before) = (listing(&base), listing(&root.join("folder")));
    let doc = ok(&mut a, "doc_open", json!({ "path": "inside.pdf" }))["doc"].as_u64().unwrap();
    for p in [".", "", "folder", "folder/"] {
        let e = a.call("doc_save", &json!({ "doc": doc, "path": p })).unwrap_err();
        assert!(matches!(&e, ToolError::Failed(m) if m.contains("is a folder")), "{p:?}: {e:?}");
    }
    let png = vec![1, 2, 3];
    assert!(a.write_output(".", &png).is_err());
    assert_eq!(std::fs::read_to_string(&beside).unwrap(), "SENTINEL");
    assert_eq!(listing(&base), base_before, "nothing was left beside the root");
    assert_eq!(listing(&root.join("folder")), folder_before, "nothing was left in the folder");
    // `image_save` adds an extension when the path has none, which turned "." into `root.png`
    // beside the root.
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 18 }));
    let pic = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/inside_page_1.png"] }))["doc"].as_u64().unwrap();
    for p in [".", "", "folder", "src/.."] {
        let e = a.call("image_save", &json!({ "doc": pic, "page": 1, "image": 1, "path": p })).unwrap_err();
        assert!(matches!(&e, ToolError::Failed(m) if m.contains("is a folder")), "{p:?}: {e:?}");
    }
    assert!(!base.join("root.png").exists() && !root.join("folder.png").exists());
    ok(&mut a, "image_save", json!({ "doc": pic, "page": 1, "image": 1, "path": "folder/picture" }));
    assert!(root.join("folder/picture.png").is_file());
    // Folder outputs may still name the root.
    ok(&mut a, "doc_split", json!({ "doc": doc, "every": 1, "out_dir": "." }));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn file_names_from_documents_stay_in_the_output_folder() {
    // Folder outputs name their files after the document. A document name with separators,
    // `..` or (on Windows) a drive letter used to take those files out of the folder, and out
    // of the root.
    let (base, root) = sandbox("doc-names");
    let mut a = auto(&root);
    let names = ["../../escape", "../../escape.pdf", "x/../../../escape.pdf", "/tmp/escape.pdf", r"x\C:escape.pdf", "C:escape.pdf", "..", "."];
    for (i, name) in names.iter().enumerate() {
        let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "pages": 2, "name": name }))["doc"].as_u64().unwrap();
        let out = format!("out{i}");
        let mut files: Vec<String> = Vec::new();
        let r = ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": out, "dpi": 10 }));
        files.extend(r["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_owned()));
        let r = ok(&mut a, "page_extract", json!({ "doc": doc, "pages": [1], "separate": true, "out_dir": out }));
        files.extend(r["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_owned()));
        let r = ok(&mut a, "doc_split", json!({ "doc": doc, "every": 1, "out_dir": out }));
        files.extend(r["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap().to_owned()));
        assert_eq!(files.len(), 5, "{name:?}: {files:?}");
        for f in &files {
            let f = Path::new(f);
            assert_eq!(f.parent(), Some(root.join(&out).as_path()), "{name:?} wrote {}", f.display());
            assert!(f.is_file(), "{name:?}: {} exists", f.display());
        }
    }
    // The same for the images a page uses, with a document that has one.
    let doc = ok(&mut a, "doc_open", json!({ "path": "inside.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": doc, "folder": "src", "dpi": 18 }));
    let pic =
        ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["src/inside_page_1.png"], "name": "../../escape" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_export_all_images", json!({ "doc": pic, "folder": "all" }));
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{r}");
    for f in files {
        assert_eq!(Path::new(f["path"].as_str().unwrap()).parent(), Some(root.join("all").as_path()), "{r}");
    }
    let mut left: Vec<String> = std::fs::read_dir(&base).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left, ["outside", "root"]);

    // The root itself must be a folder.
    assert!(Automation::new().with_root(root.join("inside.pdf")).is_err());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn command_list_reports_enablement_and_tools() {
    let dir = workdir("commands");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let list = ok(&mut a, "command_list", json!({ "doc": doc }));
    let find = |id: &str| list["commands"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap().clone();
    assert_eq!(find("page.rotate")["tool"], "page_rotate");
    assert_eq!(find("page.rotate")["enabled"], true);
    assert_eq!(find("edit.undo")["enabled"], false);
    ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [1], "degrees": 90 }));
    let list = ok(&mut a, "command_list", json!({ "doc": doc }));
    let undo = list["commands"].as_array().unwrap().iter().find(|c| c["id"] == "edit.undo").unwrap().clone();
    assert_eq!(undo["enabled"], true);
    assert_eq!(undo["label"], "Undo Rotate page");
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_session_over_stdio() {
    let dir = workdir("mcp");
    let path = dir.join("a.pdf");
    let input = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "doc_open", "arguments": { "path": path } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "page_render", "arguments": { "doc": 1, "page": 2, "dpi": 36 } } }),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    let mut out = Vec::new();
    McpServer::new(Automation::new()).serve(input.as_bytes(), &mut out).unwrap();
    let replies: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 4, "the notification gets no reply");

    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "pdfcraft");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), tools().len());
    assert_eq!(replies[2]["result"]["structuredContent"]["pages"], 3);
    assert_eq!(replies[2]["result"]["isError"], false);
    let img = &replies[3]["result"]["content"][0];
    assert_eq!((img["type"].as_str(), img["mimeType"].as_str()), (Some("image"), Some("image/png")));
    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD.decode(img["data"].as_str().unwrap()).unwrap();
    assert_eq!(&png[1..4], b"PNG");
}
