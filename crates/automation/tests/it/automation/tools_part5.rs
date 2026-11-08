//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 5 of 8).

#[allow(unused_imports)]
use super::*;

#[test]
fn comments_and_form_data_travel_as_xfdf_fdf_and_text() {
    let dir = workdir("xfdf");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 2, "type": "note", "at": [30, 30], "contents": "Please review", "author": "Ada" }));
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, 20, 180, 42], "name": "City" }));
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "City": "Paris" } }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "reviewed.pdf" }));
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "all.xfdf" }));
    ok(&mut a, "doc_export_data", json!({ "doc": doc, "path": "data.txt", "what": "fields" }));
    assert_eq!(std::fs::read_to_string(dir.join("data.txt")).unwrap(), "City\nParis\n");
    assert!(matches!(a.call("doc_export_data", &json!({ "doc": doc, "path": "c.csv", "what": "comments" })), Err(ToolError::InvalidArgs(_))));
    // A copy without the comment and with the field empty takes both back.
    ok(&mut a, "comment_delete", json!({ "doc": doc, "page": 2, "index": 1 }));
    ok(&mut a, "form_reset", json!({ "doc": doc }));
    let r = ok(&mut a, "doc_import_data", json!({ "doc": doc, "path": "all.xfdf" }));
    assert_eq!(r["filled_fields"], 1);
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["comments"][0]["contents"], "Please review", "{list}");
    assert_eq!(r["undo"], "Import all.xfdf");
}

#[test]
fn natural_image_stamps_through_tools_on_rotated_pages() {
    for degrees in [0, 90, 180, 270] {
        let dir = workdir(&format!("natural-stamps-{degrees}"));
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 80, 40);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let pixels: Vec<u8> = (0..40)
                .flat_map(|y| {
                    (0..80).flat_map(move |x| {
                        [[240, 20, 20], [20, 180, 20], [20, 20, 240], [230, 180, 20]][usize::from(y >= 20) * 2 + usize::from(x >= 40)]
                    })
                })
                .collect();
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        std::fs::write(dir.join("quadrants.png"), bytes).unwrap();
        let mut a = auto(&dir);
        let doc = ok(&mut a, "doc_open", json!({"path":"a.pdf"}))["doc"].as_u64().unwrap();
        ok(&mut a, "page_rotate", json!({"doc":doc,"pages":[1],"degrees":degrees}));
        ok(&mut a, "stamp_custom", json!({"doc":doc,"page":1,"path":"quadrants.png","at":[100,100]}));
        let rendered = a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap();
        let Content::Png { data, .. } = &rendered[0] else { panic!("expected PNG") };
        let mut reader = png::Decoder::new(std::io::Cursor::new(data)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        // A rendered page is opaque, so it comes as RGB.
        assert_eq!(info.color_type, png::ColorType::Rgb);
        for ((x, y), colour) in
            [(80, 90), (120, 90), (80, 110), (120, 110)].into_iter().zip([[240, 20, 20], [20, 180, 20], [20, 20, 240], [230, 180, 20]])
        {
            let offset = ((y * info.width + x) * 3) as usize;
            assert_eq!(&pixels[offset..offset + 3], &colour, "page rotation {degrees}");
        }
    }
}

#[test]
fn stamps_through_tools() {
    let dir = workdir("stamps");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "stamp", "stamp": "approved", "at": [100, 100] }));
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "stamp", "stamp": "reviewed", "dynamic": true, "at": [100, 200], "author": "Ada" }),
    );
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "stamp", "stamp": "sign here", "at": [100, 250] }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 3, "{list}");
    let text = page_text(&mut a, doc)[0].clone();
    assert!(text.contains("APPROVED") && text.contains("REVIEWED") && text.contains("By Ada at") && text.contains("SIGN HERE"), "{text}");
    assert!(matches!(
        a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "stamp", "stamp": "nonsense", "at": [1, 1] })),
        Err(ToolError::InvalidArgs(_))
    ));
    // A custom stamp from another PDF's page.
    ok(&mut a, "stamp_custom", json!({ "doc": doc, "page": 2, "path": "b.pdf", "file_page": 2, "at": [100, 150], "name": "Logo" }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 4, "{list}");
    // Its "Page 2" is drawn over the page's own.
    assert_eq!(page_text(&mut a, doc)[1].matches('2').count(), 2);
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Add stamp");
    assert!(matches!(a.call("stamp_custom", &json!({ "doc": doc, "page": 1, "path": "nope.png", "at": [1, 1] })), Err(ToolError::Failed(_))));
}

/// #4077: a PDF page placed at a given width (a text column, 483 pt) through `rect`, fitted
/// inside it keeping its shape and centred, and still vector (its text is text).
#[test]
fn custom_stamps_fit_a_pdf_page_inside_a_rect() {
    let dir = workdir("stamp_rect");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 595, "height": 842 }))["doc"].as_u64().unwrap();
    let rect_of = |a: &mut Automation, i: usize| -> Vec<f64> {
        let list = ok(a, "comment_list", json!({ "doc": doc }));
        list["comments"][i]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    };
    // Without rect, the page keeps its natural size, at most 200 pt: 200 × 300 is 133⅓ × 200.
    ok(&mut a, "stamp_custom", json!({ "doc": doc, "page": 1, "path": "b.pdf", "file_page": 2, "at": [100, 150] }));
    let natural = rect_of(&mut a, 0);
    assert!((natural[2] - natural[0] - 400.0 / 3.0).abs() < 0.01 && (natural[3] - natural[1] - 200.0).abs() < 0.01, "{natural:?}");
    // 483 × 750: the width limits the 200 × 300 page to 483 × 724.5, centred top to bottom.
    ok(&mut a, "stamp_custom", json!({ "doc": doc, "page": 1, "path": "b.pdf", "file_page": 2, "rect": [56, 50, 539, 800] }));
    assert_eq!(rect_of(&mut a, 1), [56.0, 62.75, 539.0, 787.25]);
    assert!(page_text(&mut a, doc)[0].contains("Page 2"), "the stamped page's text is still text");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Add stamp");
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 1);

    for (args, why) in [
        (json!({ "rect": [10, 10, 10, 50] }), "zero width"),
        (json!({ "rect": [10, 10, 50, 10] }), "zero height"),
        (json!({ "rect": [0, 0, 1e-10, 100] }), "thinner than a point"),
        (json!({ "rect": [0, 0, 20000, 100] }), "wider than a PDF page can be"),
        (json!({ "rect": [0, 0, 100, f64::NAN] }), "not a number"),
        (json!({ "rect": [0, 0, 100] }), "three numbers"),
        (json!({ "rect": [0, 0, 100, 100], "at": [10, 10] }), "both a box and a point"),
        (json!({}), "neither a box nor a point"),
    ] {
        let mut call = json!({ "doc": doc, "page": 1, "path": "b.pdf" });
        call.as_object_mut().unwrap().extend(args.as_object().unwrap().clone());
        assert!(matches!(a.call("stamp_custom", &call), Err(ToolError::InvalidArgs(_))), "{why}");
    }
    let past_the_end = json!({ "doc": doc, "page": 1, "path": "b.pdf", "file_page": 9, "rect": [0, 0, 100, 100] });
    assert!(matches!(a.call("stamp_custom", &past_the_end), Err(ToolError::Failed(_))));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 1, "a refused stamp adds nothing");

    // On a quarter-turned page the picture is fitted as displayed: an 80 × 40 picture in a
    // 100 × 100 box is 100 × 50 on screen, centred.
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 80, 40);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[200; 80 * 40 * 3]).unwrap();
    }
    std::fs::write(dir.join("wide.png"), png_bytes).unwrap();
    let turned = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_rotate", json!({ "doc": turned, "pages": [1], "degrees": 90 }));
    ok(&mut a, "stamp_custom", json!({ "doc": turned, "page": 1, "path": "wide.png", "rect": [50, 60, 150, 160] }));
    let list = ok(&mut a, "comment_list", json!({ "doc": turned }));
    let rect: Vec<f64> = list["comments"][0]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(rect, [50.0, 85.0, 150.0, 135.0]);
}

#[test]
fn bookmark_split_with_equal_titles_keeps_every_part() {
    let dir = workdir("organize_dup_titles");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Same", "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Same", "page": 2 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "same", "page": 3 }));
    let s = ok(&mut a, "doc_split", json!({ "doc": doc, "bookmarks": true, "out_dir": "parts" }));
    let files: Vec<String> = s["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| std::path::Path::new(f["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, ["a-Same.pdf", "a-Same-2.pdf", "a-same-3.pdf"]);
    for f in &files {
        assert!(dir.join("parts").join(f).exists(), "{f} missing");
    }
}

#[test]
fn bookmark_split_with_long_titles_shortens_the_file_names() {
    let dir = workdir("organize_long_titles");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    // Two equal titles far over the 255-byte name limit, then a multibyte one (3 bytes a character).
    let long = format!("Chapter {}", "x".repeat(300));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": long, "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": long, "page": 2 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "章".repeat(100), "page": 3 }));
    let s = ok(&mut a, "doc_split", json!({ "doc": doc, "bookmarks": true, "out_dir": "parts" }));
    let files: Vec<String> = s["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| std::path::Path::new(f["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 3, "{files:?}");
    let distinct: std::collections::HashSet<_> = files.iter().map(|f| f.to_lowercase()).collect();
    assert_eq!(distinct.len(), 3, "{files:?}");
    for f in &files {
        assert!(f.len() <= 255, "{f} is {} bytes", f.len());
        assert!(f.ends_with(".pdf"), "{f}");
        assert!(dir.join("parts").join(f).exists(), "{f} missing");
    }
    assert!(files[0].starts_with("a-Chapter x"), "{files:?}");
    assert!(files[2].starts_with("a-章"), "{files:?}");
}

#[test]
fn organizing_with_filters_bookmark_splits_and_extract_options() {
    let dir = workdir("organize2");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    // Rotate the even page numbers only (a.pdf has 3 pages: page 2 is the only even one).
    let r = ok(&mut a, "page_rotate", json!({ "doc": doc, "degrees": 90, "subset": "even" }));
    assert_eq!(r["rotated"], 1);
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert_eq!(info["pages"][1]["rotation"], 90);
    assert_eq!(info["pages"][0]["rotation"], 0);
    // Split at top-level bookmarks: parts named after them.
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Start", "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "End/Part", "page": 3 }));
    let s = ok(&mut a, "doc_split", json!({ "doc": doc, "bookmarks": true, "out_dir": "parts" }));
    let files: Vec<String> = s["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| std::path::Path::new(f["path"].as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, ["a-Start.pdf", "a-End_Part.pdf"]);
    let s = ok(&mut a, "doc_split", json!({ "doc": doc, "max_mb": 0.0001, "out_dir": "sized" }));
    assert_eq!(s["files"].as_array().unwrap().len(), 3, "tiny limit: a page per file");
    // Extract as separate files and delete them from the original.
    let e = ok(&mut a, "page_extract", json!({ "doc": doc, "pages": [1, 2], "separate": true, "out_dir": "pages", "delete": true }));
    assert_eq!(e["files"].as_array().unwrap().len(), 2);
    assert_eq!(e["original"]["pages"], 1);
    assert!(matches!(a.call("page_extract", &json!({ "doc": doc, "pages": [1], "separate": true })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn links_through_tools() {
    let dir = workdir("links");
    std::fs::write(dir.join("notes.txt"), "Docs at https://example.org/docs and www.rust-lang.org").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": "notes.txt" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "links_from_urls", json!({ "doc": doc }));
    assert_eq!(r["created"], json!(["https://example.org/docs", "http://www.rust-lang.org"]));
    ok(&mut a, "link_add", json!({ "doc": doc, "page": 1, "rect": [72, 300, 200, 320], "to_page": 1, "visible": true, "color": "red" }));
    let list = ok(&mut a, "link_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 3);
    let added = list["links"].as_array().unwrap().iter().find(|l| l["to_page"] == 1).unwrap().clone();
    assert_eq!(added["rect"], json!([72.0, 300.0, 200.0, 320.0]));
    ok(&mut a, "link_edit", json!({ "doc": doc, "page": 1, "index": added["index"], "url": "https://pdfcraft.dev" }));
    let list = ok(&mut a, "link_list", json!({ "doc": doc }));
    assert!(list["links"].as_array().unwrap().iter().any(|l| l["url"] == "https://pdfcraft.dev"));
    ok(&mut a, "link_delete", json!({ "doc": doc, "page": 1, "index": added["index"] }));
    let r = ok(&mut a, "links_remove", json!({ "doc": doc, "pages": [1] }));
    assert_eq!(r["removed"], 2);
    assert_eq!(ok(&mut a, "link_list", json!({ "doc": doc }))["count"], 0);
    // Add one more link and test links_remove across all pages
    ok(&mut a, "link_add", json!({ "doc": doc, "page": 1, "rect": [72, 300, 200, 320], "url": "https://example.org" }));
    let r2 = ok(&mut a, "links_remove", json!({ "doc": doc }));
    assert_eq!(r2["removed"], 1);
    assert_eq!(ok(&mut a, "link_list", json!({ "doc": doc }))["count"], 0);
    assert!(matches!(a.call("link_add", &json!({ "doc": doc, "page": 1, "rect": [0, 0, 50, 20] })), Err(ToolError::InvalidArgs(_))));
}

/// #4168: a Launch action whose file is a PDF (here with Windows `\` separators, relative to the
/// source as a remote link is) opens that PDF in PdfCraft through link_open, within the root; one
/// naming a program is listed but never followed.
#[test]
fn a_launch_link_to_a_pdf_opens_it_like_a_remote_link() {
    let (_base, root) = sandbox("launch-pdf");
    std::fs::create_dir_all(root.join("ts")).unwrap();
    std::fs::create_dir_all(root.join("se")).unwrap();
    std::fs::write(root.join("se/se_B2.pdf"), fixture(2)).unwrap();
    // The helper writes a GoToR; the action is renamed (the parser rebuilds the shifted offsets).
    let launch = |file: &str| String::from_utf8(remote_link_pdf(file, None)).unwrap().replace("/S /GoToR", "/S /Launch").into_bytes();
    std::fs::write(root.join("ts/TS_304.pdf"), launch(r"..\se\se_B2.pdf")).unwrap();
    std::fs::write(root.join("ts/program.pdf"), launch("tool.exe")).unwrap();
    let mut a = auto(&root);
    let source = ok(&mut a, "doc_open", json!({ "path": "ts/TS_304.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut a, "doc_info", json!({ "doc": source }));
    let link = &info["links"][0]["target"];
    assert_eq!((link["action"].as_str(), link["remote_document"].as_str()), (Some("Launch"), Some("../se/se_B2.pdf")), "{info}");
    let opened = ok(&mut a, "link_open", json!({ "doc": source, "page": 1, "index": 1 }));
    assert_eq!(opened["path"].as_str().map(|p| p.replace('\\', "/")).as_deref(), Some("se/se_B2.pdf"), "{opened}");
    assert_eq!(opened["document"]["pages"], 2);
    let program = ok(&mut a, "doc_open", json!({ "path": "ts/program.pdf" }))["doc"].as_u64().unwrap();
    let before = a.session().docs().len();
    assert!(matches!(err(&mut a, "link_open", json!({ "doc": program, "page": 1, "index": 1 })), ToolError::InvalidArgs(_) | ToolError::Failed(_)));
    assert_eq!(a.session().docs().len(), before, "nothing was opened or launched");
}

#[test]
fn local_gotor_link_open_refuses_directories_devices_and_non_pdf_targets() {
    let (_base, root) = sandbox("gotor-not-a-file");
    std::fs::create_dir_all(root.join("folder.pdf")).unwrap();
    std::fs::write(root.join("notes.txt"), fixture(1)).unwrap();
    std::fs::write(root.join("dir-source.pdf"), remote_link_pdf("folder.pdf", None)).unwrap();
    std::fs::write(root.join("txt-source.pdf"), remote_link_pdf("notes.txt", None)).unwrap();
    // Only Unix adds a case (a FIFO), so elsewhere the list is never changed.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut cases = vec![("dir-source.pdf", "not a regular file"), ("txt-source.pdf", "not a .pdf file")];
    #[cfg(unix)]
    {
        // A FIFO inside the root would block a plain read forever.
        if std::process::Command::new("mkfifo").arg(root.join("pipe.pdf")).status().is_ok_and(|s| s.success()) {
            std::fs::write(root.join("fifo-source.pdf"), remote_link_pdf("pipe.pdf", None)).unwrap();
            cases.push(("fifo-source.pdf", "not a regular file"));
        }
    }
    let mut a = auto(&root);
    for (source_path, reason) in cases {
        let source = ok(&mut a, "doc_open", json!({ "path": source_path }))["doc"].as_u64().unwrap();
        let documents_before = a.session().docs().len();
        let e = err(&mut a, "link_open", json!({ "doc": source, "page": 1, "index": 1 }));
        assert!(matches!(&e, ToolError::Failed(m) if m.contains(reason)), "{source_path}: {e:?}");
        assert_eq!(a.session().docs().len(), documents_before, "a refused target is not opened: {source_path}");
    }
}

#[test]
fn local_gotor_link_open_applies_destinations_and_honors_root() {
    let root = local_gotor_fixtures();
    let mut a = auto(&root);
    assert_relative_gotor_destination(&mut a);
    assert_absolute_gotor_policy(&mut a);
    assert_unapplied_gotor_destinations(&mut a);
    assert_encrypted_gotor_passwords(&mut a);
    assert_unsupported_gotor_targets(&mut a);
    assert_gotor_root_boundaries(&mut a);
    assert_url_links_cannot_open_as_gotor(&mut a);
}

/// doc_info reports annotation and link rectangles in the tools' displayed-page convention,
/// the same values comment_list and link_list give, so they can be fed back to link_edit (#129).
#[test]
fn doc_info_rects_are_displayed_page_coordinates() {
    let dir = workdir("info-rects");
    let mut a = auto(&dir);
    // Page 2 is rotated so an unrotated-only y flip cannot pass.
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 612, "height": 792, "pages": 2 }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [2], "degrees": 90 }));
    for page in 1..=2 {
        ok(
            &mut a,
            "comment_add",
            json!({ "doc": doc, "page": page, "type": "note", "at": [72, 72], "author": "Example", "contents": "Fixture note" }),
        );
        ok(&mut a, "link_add", json!({ "doc": doc, "page": page, "rect": [72, 100, 200, 120], "url": "https://example.com" }));
    }
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "fixture.pdf" }));
    let re = ok(&mut a, "doc_open", json!({ "path": "fixture.pdf" }))["doc"].as_u64().unwrap();
    let info = ok(&mut a, "doc_info", json!({ "doc": re }));
    let comments = ok(&mut a, "comment_list", json!({ "doc": re }));
    let links = ok(&mut a, "link_list", json!({ "doc": re }));
    let close = |a: &Value, b: &Value| {
        let (a, b) = (a.as_array().unwrap(), b.as_array().unwrap());
        a.len() == 4 && a.iter().zip(b).all(|(x, y)| (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 0.01)
    };
    for page in 1..=2 {
        let page = json!(page);
        let find = |items: &Value| items.as_array().unwrap().iter().find(|x| x["page"] == page).unwrap()["rect"].clone();
        let (info_note, note) = (find(&info["annotations"]), find(&comments["comments"]));
        assert!(close(&info_note, &note), "page {page}: doc_info note {info_note} vs comment_list {note}");
        if page == 1 {
            assert!(close(&note, &json!([72.0, 72.0, 92.0, 92.0])), "note {note}");
        }
        let (info_link, link) = (find(&info["links"]), find(&links["links"]));
        assert!(close(&info_link, &link), "page {page}: doc_info link {info_link} vs link_list {link}");
        assert!(close(&link, &json!([72.0, 100.0, 200.0, 120.0])), "page {page}: link {link}");
    }
    // Reusing the doc_info rectangle in a geometry-taking edit leaves the link where it is.
    let rotated = links["links"].as_array().unwrap().iter().find(|l| l["page"] == 2).unwrap().clone();
    let from_info = info["links"].as_array().unwrap().iter().find(|l| l["page"] == 2).unwrap()["rect"].clone();
    ok(&mut a, "link_edit", json!({ "doc": re, "page": 2, "index": rotated["index"], "rect": from_info }));
    let after = ok(&mut a, "link_list", json!({ "doc": re }));
    let moved = after["links"].as_array().unwrap().iter().find(|l| l["page"] == 2).unwrap()["rect"].clone();
    assert!(close(&moved, &rotated["rect"]), "link moved: {moved} vs {}", rotated["rect"]);
}

/// `comment_add` places a note's or an attachment's icon with its displayed top-left corner at
/// `at`, on rotated pages too. The engine anchors the icon at the user-space top-left of its
/// `/Rect`, which after `/Rotate` is another corner of the square as displayed; converting the
/// point alone put the icon one icon-width off.
#[test]
fn note_icons_anchor_at_the_requested_corner_on_rotated_pages() {
    let dir = workdir("note-anchor");
    std::fs::write(dir.join("note.txt"), b"attached").unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 612, "height": 792, "pages": 4 }))["doc"].as_u64().unwrap();
    for (page, degrees) in [(2, 90), (3, 180), (4, 270)] {
        ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [page], "degrees": degrees }));
    }
    for page in 1..=4 {
        ok(&mut a, "comment_add", json!({ "doc": doc, "page": page, "type": "note", "at": [72, 72], "contents": "Fixture note" }));
        ok(&mut a, "comment_add", json!({ "doc": doc, "page": page, "type": "attachment", "at": [200, 300], "path": "note.txt" }));
    }
    let comments = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let comments = comments["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 8);
    for c in comments {
        let want = if c["type"] == "Text" { [72.0, 72.0, 92.0, 92.0] } else { [200.0, 300.0, 220.0, 320.0] };
        let rect: Vec<f64> = c["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!(rect.iter().zip(want).all(|(x, y)| (x - y).abs() < 0.01), "page {} {}: rect {rect:?}, want {want:?}", c["page"], c["type"]);
    }
}

/// An image signature is drawn upright as displayed, and anchored at the displayed point asked
/// for, on every `/Rotate`: its picture is placed in user space, which the page turns, so the
/// appearance is counter-rotated and its box is found in user space.
#[test]
fn image_signatures_stay_upright_on_rotated_pages() {
    let dir = workdir("image-upright");
    // 80 x 40 px, one colour per quadrant: red and green above, blue and yellow below.
    let quadrants = image::RgbaImage::from_fn(80, 40, |x, y| match (x < 40, y < 20) {
        (true, true) => image::Rgba([255, 0, 0, 255]),
        (false, true) => image::Rgba([0, 255, 0, 255]),
        (true, false) => image::Rgba([0, 0, 255, 255]),
        (false, false) => image::Rgba([255, 255, 0, 255]),
    });
    quadrants.save(dir.join("quadrants.png")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300, "pages": 4 }))["doc"].as_u64().unwrap();
    for (page, degrees) in [(2, 90), (3, 180), (4, 270)] {
        ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [page], "degrees": degrees }));
    }
    for page in 1..=4 {
        // 64 x 32 pt from x = 60 as displayed, centred on y = 140.
        ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": page, "type": "signature", "at": [60, 140], "path": "quadrants.png" }));
    }
    let (white, red, green, blue, yellow) = ([255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]);
    let check = |a: &mut Automation, doc: u64| {
        for page in 1..=4 {
            let output = a.call("page_render", &json!({ "doc": doc, "page": page, "dpi": 72 })).unwrap();
            let Content::Png { data, .. } = &output[0] else { panic!() };
            let pixels = image::load_from_memory(data).unwrap().to_rgba8();
            for (x, y, want, what) in [
                (76, 132, red, "signature top-left"),
                (108, 132, green, "signature top-right"),
                (76, 148, blue, "signature bottom-left"),
                (108, 148, yellow, "signature bottom-right"),
                (56, 140, white, "left of the signature"),
                (128, 140, white, "right of the signature"),
                (92, 120, white, "above the signature"),
                (92, 160, white, "below the signature"),
            ] {
                let got = pixels.get_pixel(x, y).0;
                assert!(got[..3].iter().zip(want).all(|(g, w)| g.abs_diff(w) < 12), "page {page}, {what} at ({x}, {y}): {got:?}, want {want:?}");
            }
        }
    };
    check(&mut a, doc);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "placed.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "placed.pdf" }))["doc"].as_u64().unwrap();
    check(&mut a, reopened);
}

/// Issue #299: a typed signature and initials read across, as displayed, on every `/Rotate`,
/// anchored at the displayed point, and look the same as on an unturned page.
#[test]
fn typed_signatures_stay_upright_on_rotated_pages() {
    let dir = workdir("typed-upright");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300, "pages": 4 }))["doc"].as_u64().unwrap();
    for (page, degrees) in [(2, 90), (3, 180), (4, 270)] {
        ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [page], "degrees": degrees }));
    }
    for page in 1..=4 {
        ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": page, "type": "signature", "at": [30, 60], "text": "Ada Lovelace" }));
        ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": page, "type": "initials", "at": [30, 140], "text": "AL" }));
    }
    let check = |a: &mut Automation, doc: u64| {
        let mut upright = Vec::new();
        for page in 1..=4 {
            let output = a.call("page_render", &json!({ "doc": doc, "page": page, "dpi": 72 })).unwrap();
            let Content::Png { data, .. } = &output[0] else { panic!() };
            let pixels = image::load_from_memory(data).unwrap().to_rgba8();
            assert_eq!(pixels.dimensions(), if page % 2 == 1 { (200, 300) } else { (300, 200) }, "page {page}");
            let dark: Vec<bool> = pixels.pixels().map(|p| p.0[..3].iter().all(|v| *v < 128)).collect();
            for y in [60, 140] {
                let mut b = [u32::MAX, u32::MAX, 0, 0];
                for (x, py, p) in pixels.enumerate_pixels() {
                    if py.abs_diff(y) < 40 && p.0[..3].iter().all(|v| *v < 128) {
                        b = [b[0].min(x), b[1].min(py), b[2].max(x), b[3].max(py)];
                    }
                }
                assert!(b[0] <= b[2], "page {page}: no ink near y = {y}");
                assert!(b[2] - b[0] > b[3] - b[1], "page {page}: the name reads across: {b:?}");
                assert!(b[0].abs_diff(30) <= 2 && b[1] < y && b[3] > y, "page {page}: anchored at (30, {y}): {b:?}");
            }
            if page == 1 {
                upright = dark;
                continue;
            }
            // Same pixels as the unturned page where both pages are (pages 2 and 4 are wider).
            let width = pixels.width();
            let (mut differ, mut ink) = (0, 0);
            for y in 0..200 {
                for x in 0..200 {
                    let (want, got) = (upright[(y * 200 + x) as usize], dark[(y * width + x) as usize]);
                    ink += usize::from(want);
                    differ += usize::from(want != got);
                }
            }
            assert!(differ * 10 < ink, "page {page}: {differ} of {ink} ink pixels differ from the unturned page");
        }
    };
    check(&mut a, doc);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "placed.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "placed.pdf" }))["doc"].as_u64().unwrap();
    check(&mut a, reopened);
}

/// #2392: a Fill & Sign checkmark and line look the same on a turned page as on an unturned one:
/// the checkmark isn't upside down or sideways, and the line runs across as displayed.
#[test]
fn fill_sign_marks_stay_upright_on_rotated_pages() {
    let dir = workdir("marks-upright");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 200, "height": 300, "pages": 4 }))["doc"].as_u64().unwrap();
    for (page, degrees) in [(2, 90), (3, 180), (4, 270)] {
        ok(&mut a, "page_rotate", json!({ "doc": doc, "pages": [page], "degrees": degrees }));
    }
    for page in 1..=4 {
        ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": page, "type": "check", "at": [40, 60] }));
        ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": page, "type": "line", "at": [120, 60] }));
    }
    let check = |a: &mut Automation, doc: u64| {
        let mut upright = Vec::new();
        for page in 1..=4 {
            // 144 dpi: two pixels a point.
            let output = a.call("page_render", &json!({ "doc": doc, "page": page, "dpi": 144 })).unwrap();
            let Content::Png { data, .. } = &output[0] else { panic!() };
            let pixels = image::load_from_memory(data).unwrap().to_rgba8();
            let dark: Vec<bool> = pixels.pixels().map(|p| p.0[..3].iter().all(|v| *v < 128)).collect();
            let width = pixels.width();
            // The line, 36 × 4 pt displayed, runs across.
            let mut b = [u32::MAX, u32::MAX, 0, 0];
            for (x, y, p) in pixels.enumerate_pixels() {
                if x.abs_diff(240) < 60 && y.abs_diff(120) < 60 && p.0[..3].iter().all(|v| *v < 128) {
                    b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                }
            }
            assert!(b[0] <= b[2], "page {page}: no line drawn");
            assert!(b[2] - b[0] > 60 && b[3] - b[1] < 10, "page {page}: the line should run across: {b:?}");
            if page == 1 {
                upright = dark;
                continue;
            }
            // Same pixels as the unturned page where both pages are (pages 2 and 4 are wider).
            let (mut differ, mut ink) = (0, 0);
            for y in 0..400 {
                for x in 0..400 {
                    let (want, got) = (upright[(y * 400 + x) as usize], dark[(y * width + x) as usize]);
                    ink += usize::from(want);
                    differ += usize::from(want != got);
                }
            }
            assert!(ink > 0 && differ * 10 < ink, "page {page}: {differ} of {ink} ink pixels differ from the unturned page");
        }
    };
    check(&mut a, doc);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "marked.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "marked.pdf" }))["doc"].as_u64().unwrap();
    check(&mut a, reopened);
}

#[test]
fn comment_checkmarks_locks_hiding_and_summaries_through_tools() {
    let dir = workdir("comment-polish");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let c = ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [20, 20], "contents": "Sticky", "author": "Ada" }));
    let id = c["comment"]["id"].as_str().unwrap().to_string();
    ok(&mut a, "comment_set_status", json!({ "doc": doc, "id": id, "status": "accepted" }));
    ok(&mut a, "comment_mark", json!({ "doc": doc, "id": id }));
    ok(&mut a, "comment_lock", json!({ "doc": doc, "id": id }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 1, "{list}");
    let c = &list["comments"][0];
    assert_eq!((c["status"].as_str(), c["marked"].as_bool(), c["locked"].as_bool()), (Some("Accepted"), Some(true), Some(true)));
    assert!(matches!(a.call("comment_delete", &json!({ "doc": doc, "id": id })), Err(ToolError::Failed(_))), "locked");
    ok(&mut a, "comment_mark", json!({ "doc": doc, "id": id, "marked": false }));
    ok(&mut a, "comment_lock", json!({ "doc": doc, "id": id, "locked": false }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!((list["comments"][0]["marked"].as_bool(), list["comments"][0]["locked"].as_bool()), (Some(false), Some(false)));

    assert_eq!(ok(&mut a, "comments_hide", json!({ "doc": doc }))["hidden"], true);
    ok(&mut a, "comments_hide", json!({ "doc": doc, "hidden": false }));

    let r = ok(&mut a, "comments_summarize", json!({ "doc": doc, "sort": "author", "out": "summary.pdf" }));
    assert!(r["bytes"].as_u64().unwrap() > 500);
    let text = ok(&mut a, "doc_open", json!({ "path": "summary.pdf" }))["doc"].as_u64().unwrap();
    let found = ok(&mut a, "text_find", json!({ "doc": text, "query": "Sticky" }));
    assert!(found["count"].as_u64().unwrap() >= 1, "{found}");
    assert!(matches!(a.call("comments_summarize", &json!({ "doc": doc, "sort": "colour" })), Err(ToolError::InvalidArgs(_))));
}

/// Text the standard fonts can't draw is never written as `?` silently, through the tools. A form
/// value, a button caption, a text box and Fill & Sign text are kept as typed (as Acrobat keeps
/// them) and the result's `warnings` names the character; a form also asks other viewers to
/// redraw it. Where the `?` would be permanent the tool refuses: a typed signature, a redaction's
/// overlay text, a new document from text, and flattening what PdfCraft drew with `?`. A comment
/// summary that quotes such text writes it as [U+XXXX].
#[test]
fn text_the_standard_fonts_cannot_draw_is_never_silent_through_tools() {
    let dir = workdir("undrawable");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let refused = |a: &mut Automation, tool: &str, args: Value, ch: &str| match a.call(tool, &args) {
        Err(ToolError::Failed(e) | ToolError::InvalidArgs(e)) => assert!(e.contains(ch), "{tool}: {e}"),
        other => panic!("{tool}: expected a refusal, got {other:?}"),
    };
    let warned = |out: &Value, parts: &[&str]| {
        let w = out["warnings"].as_array().unwrap_or_else(|| panic!("no warnings: {out}"));
        assert!(w.iter().filter_map(Value::as_str).any(|m| parts.iter().all(|p| m.contains(p))), "{parts:?} not in {w:?}");
    };
    // Forms: a value is kept and the form asks other viewers to redraw it; a button caption too.
    ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [20, 20, 180, 42], "name": "who" }));
    let out = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "who": "Ada" } }));
    assert!(out.get("warnings").is_none(), "{out}");
    let out = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "who": "张三" } }));
    warned(&out, &["form field \"who\"", "\"张\" (U+5F20)", "other PDF viewers redraw the field"]);
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    assert_eq!(fields["fields"][0]["value"], "张三", "{fields}");
    let out = ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "button", "rect": [20, 60, 120, 82], "caption": "提交" }));
    warned(&out, &["\"提\" (U+63D0)"]);
    // Comments and Fill & Sign text: kept, and said.
    let out = ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [10, 200, 190, 240], "contents": "欢迎" }));
    warned(&out, &["a comment on page 1", "\"欢\" (U+6B22)", "every viewer"]);
    let out = ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 300], "text": "签名" }));
    warned(&out, &["U+7B7E"]);
    // A sticky note draws an icon, not its text: nothing to say.
    let out = ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [40, 40], "contents": "签名 OK", "author": "Ada" }));
    assert!(out.get("warnings").is_none(), "{out}");
    // Permanent `?`: refused.
    refused(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "signature", "at": [20, 340], "text": "张三" }), "pass `path`");
    // Cyrillic comes from the script font's Inter fallback (#963), so it isn't refused.
    ok(&mut a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "signature", "at": [20, 380], "text": "Саша" }));
    refused(&mut a, "redact_mark", json!({ "doc": doc, "page": 1, "rect": [10, 400, 190, 420], "overlay": "机密" }), "\"机\" (U+673A)");
    refused(&mut a, "doc_flatten", json!({ "doc": doc }), "nothing was flattened");
    refused(&mut a, "doc_create", json!({ "from": "text", "text": "Notes\n第二行" }), "(line 2)");
    // A summary quotes the note: escaped, not refused, not `?`.
    ok(&mut a, "comments_summarize", json!({ "doc": doc, "out": "summary.pdf" }));
    let summary = ok(&mut a, "doc_open", json!({ "path": "summary.pdf" }))["doc"].as_u64().unwrap();
    let text = page_text(&mut a, summary).join("\n");
    assert!(text.contains("[U+7B7E][U+540D] OK") && text.contains("[U+XXXX]"), "{text}");
    assert!(!text.contains("?? OK"), "{text}");
}

/// #1908: `comment_edit font_size` resizes the text of a text box, in one undo step, and refuses
/// sizes outside 1-400 pt and comments without text.
#[test]
fn text_box_font_size_through_the_tool() {
    let dir = workdir("fontsize");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [20, 40, 200, 80], "contents": "Hello" }));
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [20, 120, 80, 160] }));
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": 1, "index": 1, "font_size": 30 }));
    for bad in [0, 401] {
        assert!(matches!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 1, "font_size": bad })), Err(ToolError::InvalidArgs(_))));
    }
    assert!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 2, "font_size": 12 })).is_err());
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "sized.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("sized.pdf")).unwrap())).unwrap();
    let sizes: Vec<String> = saved
        .object_numbers()
        .iter()
        .filter_map(|n| saved.try_get(*n).ok())
        .filter_map(|o| o.as_dict().and_then(|d| d.get(b"DA")?.as_string().map(|s| String::from_utf8_lossy(&s.bytes).into_owned())))
        .collect();
    assert_eq!(sizes.len(), 1, "{sizes:?}");
    assert!(sizes[0].ends_with("/Helv 30 Tf"), "{sizes:?}");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Change comment properties");
}

#[test]
fn line_endings_through_the_tool() {
    let dir = workdir("endings");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "line", "from": [20, 40], "to": [120, 80], "endings": ["None", "Diamond"] }));
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "polyline", "points": [[20, 120], [60, 100], [100, 140]], "endings": ["Circle", "Slash"] }),
    );
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "callout", "rect": [110, 200, 190, 240], "to": [40, 160], "contents": "Look", "endings": ["Square"] }),
    );
    assert!(matches!(
        a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "line", "from": [20, 40], "to": [120, 80], "endings": ["Sparkle", "None"] })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(
        a.call(
            "comment_add",
            &json!({ "doc": doc, "page": 1, "type": "callout", "rect": [10, 10, 80, 40], "to": [90, 80], "endings": ["None", "None"] })
        ),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(
        a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [10, 10, 40, 40], "endings": ["Circle"] })),
        Err(ToolError::InvalidArgs(_))
    ));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "ended.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("ended.pdf")).unwrap())).unwrap();
    for name in [b"Diamond".as_slice(), b"Circle", b"Slash", b"Square"] {
        assert!(
            saved.object_numbers().iter().any(|n| saved.try_get(*n).ok().is_some_and(|o| object_has_name(&o, name))),
            "missing /{}",
            String::from_utf8_lossy(name)
        );
    }
    let drawn: Vec<String> = saved
        .object_numbers()
        .into_iter()
        .filter_map(|n| {
            let obj = saved.try_get(n).ok()?;
            let pdfcraft_cos::Object::Stream(s) = &*obj else { return None };
            if s.dict.name(b"Subtype") != Some(b"Form") {
                return None;
            }
            s.decoded().ok().map(|b| String::from_utf8_lossy(&b).into_owned())
        })
        .collect();
    assert!(drawn.iter().any(|s| s.contains("h B")), "a filled ending was drawn: {drawn:?}");
    assert!(drawn.iter().any(|s| s.contains(" l S")), "an open ending was drawn: {drawn:?}");
}

#[test]
fn drawing_comments_through_tools() {
    let dir = workdir("drawing");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (ty, extra) in [
        ("polygon", json!({ "points": [[20, 20], [80, 20], [50, 70]] })),
        ("cloud", json!({ "points": [[100, 20], [180, 20], [180, 80], [100, 80]], "color": "red" })),
        ("polyline", json!({ "points": [[20, 120], [60, 100], [100, 120]] })),
        ("callout", json!({ "rect": [110, 200, 190, 240], "to": [40, 160], "contents": "Look" })),
        ("caret", json!({ "at": [60, 150], "contents": "insert this" })),
    ] {
        let mut args = json!({ "doc": doc, "page": 1, "type": ty });
        args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        ok(&mut a, "comment_add", args);
    }
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let types: Vec<&str> = list["comments"].as_array().unwrap().iter().map(|c| c["type"].as_str().unwrap()).collect();
    for t in ["Polygon", "PolyLine", "FreeText", "Caret"] {
        assert!(types.contains(&t), "{types:?}");
    }
    assert_eq!(types.iter().filter(|t| **t == "Polygon").count(), 2);
    let callout = list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "FreeText").unwrap();
    let r: Vec<f64> = callout["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(r[0] < 40.0 && r[3] > 159.0, "the rect holds the leader line: {r:?}");
    assert!(matches!(
        a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "polygon", "points": [[1, 1]] })),
        Err(ToolError::Failed(_) | ToolError::InvalidArgs(_))
    ));
    // Replace Text: a strikeout over the found text and a grouped caret.
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 2, "type": "replace", "find": "page 2", "contents": "second page" }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc, "page": 2 }));
    let strike = list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "StrikeOut").unwrap().clone();
    assert_eq!(strike["replies"][0]["contents"], "second page", "the caret threads under the strikeout");
    assert!(matches!(a.call("comment_add", &json!({ "doc": doc, "page": 2, "type": "replace", "find": "page 2" })), Err(ToolError::InvalidArgs(_))));
    // A file attached as a comment.
    std::fs::write(dir.join("notes.txt"), b"remember").unwrap();
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "attachment", "path": "notes.txt", "at": [250, 20], "icon": "Paperclip" }));
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert!(info.to_string().contains("notes.txt"), "{info}");
    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    assert!(matches!(png[0], Content::Png { .. }));
}

/// comment_add `ortho`, Shift's hold while drawing: a line's end is held level or plumb with its
/// start, and each corner of connected lines or a polygon with the corner before.
#[test]
fn lines_and_corners_are_held_level_or_plumb_through_the_tool() {
    let dir = workdir("ortho");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for args in [
        json!({ "type": "line", "from": [20, 40], "to": [120, 52], "ortho": true }),
        json!({ "type": "arrow", "from": [180, 40], "to": [170, 140], "ortho": true }),
        json!({ "type": "polyline", "points": [[20, 200], [90, 210], [95, 260], [130, 250]], "ortho": true }),
        json!({ "type": "line", "from": [20, 280], "to": [120, 290] }),
    ] {
        let mut args = args;
        args.as_object_mut().unwrap().extend([("doc".to_string(), json!(doc)), ("page".to_string(), json!(1))]);
        ok(&mut a, "comment_add", args);
    }
    let list = ok(&mut a, "comment_list", json!({ "doc": doc, "page": 1 }));
    // In the order added: the list isn't sorted by index.
    let mut listed = list["comments"].as_array().unwrap().clone();
    listed.sort_by_key(|c| c["index"].as_u64());
    let points: Vec<Vec<[f64; 2]>> =
        listed.iter().map(|c| c["points"].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect()).collect();
    let want: [&[[f64; 2]]; 4] = [
        &[[20.0, 40.0], [120.0, 40.0]],
        &[[180.0, 40.0], [180.0, 140.0]],
        &[[20.0, 200.0], [90.0, 200.0], [90.0, 260.0], [130.0, 260.0]],
        &[[20.0, 280.0], [120.0, 290.0]],
    ];
    assert_eq!(points.len(), want.len(), "{points:?}");
    for (got, want) in points.iter().zip(want) {
        assert_eq!(got.len(), want.len(), "{got:?}");
        assert!(got.iter().zip(want).all(|(g, w)| (g[0] - w[0]).abs() < 0.01 && (g[1] - w[1]).abs() < 0.01), "{got:?} for {want:?}");
    }
    let refused = a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [10, 10, 50, 50], "ortho": true }));
    assert!(matches!(refused, Err(ToolError::InvalidArgs(_))), "ortho is for lines and corners: {refused:?}");
}

#[test]
fn signing_with_an_image_typed_or_drawn_graphic_through_tools() {
    let dir = workdir("signing-graphic");
    // A solid red 120 x 40 picture.
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 120, 40);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let rgba: Vec<u8> = (0..120 * 40).flat_map(|_| [220, 0, 0, 255]).collect();
        encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
    std::fs::write(dir.join("logo.png"), png).unwrap();
    std::fs::write(dir.join("broken.png"), b"broken").unwrap();
    let mut a = auto(&dir);
    ok(&mut a, "sign_id_create", json!({ "name": "Ada Lovelace", "key": "p256", "password": "secret12", "path": "ada.p12" }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let sign = |a: &mut Automation, rect: [f64; 4], graphic: Value, out: &str| {
        a.call(
            "sign_document",
            &json!({ "doc": doc, "id": "ada.p12", "password": "secret12", "page": 1, "rect": rect, "graphic": graphic, "out": out }),
        )
    };
    for (graphic, why) in [
        (json!({ "image": "logo.png", "text": "Ada" }), "two kinds"),
        (json!({}), "no kind"),
        (json!({ "picture": "logo.png" }), "unknown key"),
        (json!("logo.png"), "not an object"),
        (json!({ "image": "broken.png" }), "not an image"),
        (json!({ "text": "   " }), "nothing to draw"),
        (json!({ "text": "张三" }), "a name the script font and its fallback can't draw"),
        (json!({ "strokes": [[[0, 0], ["x", 1]]] }), "malformed strokes"),
        (json!({ "strokes": [] }), "empty strokes"),
    ] {
        let r = sign(&mut a, [20.0, 200.0, 180.0, 250.0], graphic, "bad.pdf");
        assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "{why}: {r:?}");
    }
    // A missing file fails as it does for every tool that reads one.
    assert!(matches!(sign(&mut a, [20.0, 200.0, 180.0, 250.0], json!({ "image": "missing.png" }), "bad.pdf"), Err(ToolError::Failed(_))));
    // An invisible signature shows nothing, so a graphic for one is a mistake.
    let r = a.call("sign_document", &json!({ "doc": doc, "id": "ada.p12", "password": "secret12", "graphic": { "text": "Ada" }, "out": "bad.pdf" }));
    assert!(matches!(r, Err(ToolError::InvalidArgs(_))), "{r:?}");
    assert!(!dir.join("bad.pdf").exists());

    // Image, typed and drawn graphics, each a signature of its own, all intact.
    sign(&mut a, [20.0, 200.0, 180.0, 250.0], json!({ "image": "logo.png" }), "s1.pdf").unwrap();
    sign(&mut a, [20.0, 130.0, 180.0, 180.0], json!({ "text": "Ada Lovelace" }), "s2.pdf").unwrap();
    sign(&mut a, [20.0, 60.0, 180.0, 110.0], json!({ "strokes": [[[0, 0], [0.3, 0.2], [0.6, 0], [1, 0.2]]] }), "s3.pdf").unwrap();
    let list = ok(&mut a, "sign_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 3);
    for s in list["signatures"].as_array().unwrap() {
        assert_eq!((s["status"].as_str(), s["visible"].as_bool()), (Some("unknown"), Some(true)), "{s}");
    }
    // At 72 dpi a pixel is a point. Each graphic is in the left half of its box (x 23–97), the
    // details on the right.
    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    let Content::Png { data, width, .. } = &png[0] else { panic!("expected an image") };
    let mut reader = png::Decoder::new(std::io::Cursor::new(data.as_slice())).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buf).unwrap();
    let channels = frame.line_size / *width as usize;
    let px = |x: usize, y: usize| buf[y * frame.line_size + x * channels..][..3].to_vec();
    assert_eq!(px(60, 225), [220, 0, 0], "the image fills the left half's middle");
    assert_ne!(px(150, 225), [220, 0, 0], "and not the details");
    let inked = |y0: usize, y1: usize| (y0..y1).any(|y| (23..97).any(|x| px(x, y).iter().all(|c| *c < 100)));
    assert!(inked(130, 180), "the typed name is drawn");
    assert!(inked(60, 110), "the strokes are drawn");
}

/// Signing a laid-out XFA form's signature box through the tools signs the field named by its
/// SOM path, which is how another viewer, laying the form out again, finds the signature; the
/// signed file reopens with the signature intact and on that field.
#[test]
fn a_dynamic_xfa_forms_signature_box_is_signed_under_its_som_path() {
    let dir = workdir("xfa-sign");
    std::fs::write(dir.join("form.pdf"), pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::fields_template(""))).unwrap();
    let mut a = auto(&dir);
    ok(&mut a, "sign_id_create", json!({ "name": "Ada Lovelace", "key": "p256", "password": "secret12", "path": "ada.p12" }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    let som = "form[0].page1[0].sign[0]";
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    assert!(fields["fields"].as_array().unwrap().iter().any(|f| f["name"] == som), "{fields}");
    ok(&mut a, "sign_document", json!({ "doc": doc, "id": "ada.p12", "password": "secret12", "field": som, "out": "signed.pdf" }));
    let signed = ok(&mut a, "doc_open", json!({ "path": "signed.pdf" }))["doc"].as_u64().unwrap();
    let list = ok(&mut a, "sign_list", json!({ "doc": signed }));
    let s = list["signatures"].as_array().unwrap().iter().find(|s| s["signed"] == true).cloned().unwrap_or_else(|| panic!("{list}"));
    assert_eq!(s["field"], som, "{s}");
    assert_eq!(s["status"], "unknown", "intact, the signer not trusted: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn digital_ids_signing_and_validation_through_tools() {
    let dir = workdir("signing");
    let mut a = auto(&dir);
    create_signing_identity(&mut a);
    let doc = sign_document_and_verify_trust(&mut a, &dir);
    verify_signature_revisions_and_bundle(&mut a, doc, &dir);
}

/// `sign_document` takes the words of a visible signature's labels (say, in German) and which
/// items it shows, as the interface does. A label the signature's font can't draw is refused,
/// naming the label, the character and what is allowed, before anything is written.
#[test]
fn signing_with_translated_labels_through_tools() {
    let dir = workdir("signing-labels");
    let mut a = auto(&dir);
    ok(&mut a, "sign_id_create", json!({ "name": "Ada Lovelace", "key": "p256", "password": "secret12", "path": "ada.p12" }));
    let open = |a: &mut Automation| ok(a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let doc = open(&mut a);
    let args = |doc: u64, extra: Value, out: &str| {
        let mut args = json!({ "doc": doc, "id": "ada.p12", "password": "secret12", "page": 1, "rect": [10, 150, 190, 290], "reason": "Genehmigt", "location": "Berlin", "out": out });
        for (k, v) in extra.as_object().unwrap() {
            args[k] = v.clone();
        }
        args
    };
    for (extra, why, says) in [
        (
            json!({ "labels": { "signed_by": "数字签名者" } }),
            "a label Helvetica can't draw",
            "labels.signed_by: the standard fonts can't draw \"数\" (U+6570)",
        ),
        (json!({ "labels": { "reason": "Причина: " } }), "even one this signature doesn't show", "labels.reason"),
        (json!({ "labels": { "signer": "Von" } }), "an unknown label", "unknown label \"signer\""),
        (json!({ "labels": { "date": 1 } }), "not a string", "labels.date must be a string"),
        (json!({ "labels": "Datum" }), "not an object", "labels must be an object"),
        (json!({ "labels": { "date": "x".repeat(201) } }), "too long", "longer than 200"),
        (json!({ "appearance": { "reason": "yes" } }), "not true or false", "appearance.reason must be true or false"),
        (json!({ "appearance": { "logo": true } }), "an unknown item", "unknown appearance item"),
        (json!({ "rect": null, "labels": { "date": "Datum: " } }), "labels for an invisible signature", "needs a visible signature"),
        (json!({ "rect": null, "appearance": { "reason": true } }), "an appearance for an invisible one", "needs a visible signature"),
    ] {
        let r = a.call("sign_document", &args(doc, extra, "bad.pdf"));
        assert!(matches!(&r, Err(ToolError::InvalidArgs(m)) if m.contains(says)), "{why}: {r:?}");
    }
    let r = a.call("sign_document", &args(doc, json!({ "labels": { "signed_by": "数字签名者" } }), "bad.pdf"));
    assert!(matches!(&r, Err(ToolError::InvalidArgs(m)) if m.contains("signed_by label") && m.contains("Western European")), "{r:?}");
    assert!(!dir.join("bad.pdf").exists(), "nothing is written for a refused call");

    // German labels, with the reason and the location shown: stamped as given.
    let german = json!({
        "labels": { "signed_by": "Digital signiert von", "reason": "Grund: ", "location": "Ort: ", "date": "Datum: " },
        "appearance": { "reason": true, "location": true },
    });
    let r = ok(&mut a, "sign_document", args(doc, german, "de.pdf"));
    assert_eq!(r["signature"]["reason"], "Genehmigt");
    let stamped = signature_appearances(&dir.join("de.pdf"));
    // Each line starts with its label (a long one wraps after it).
    for word in ["(Digital", "(Grund:", "Genehmigt", "(Ort:", "Berlin", "(Datum:"] {
        assert!(stamped.contains(word), "{word}: {stamped}");
    }
    assert!(!stamped.contains("Reason") && !stamped.contains("Date:"), "{stamped}");

    // Labels omitted keep the English; the labels turned off show the values alone.
    let doc = open(&mut a);
    ok(&mut a, "sign_document", args(doc, json!({ "labels": { "date": "Datum: " }, "appearance": { "reason": true } }), "mixed.pdf"));
    let stamped = signature_appearances(&dir.join("mixed.pdf"));
    assert!(stamped.contains("(Reason:") && stamped.contains("(Datum:") && !stamped.contains("Berlin"), "{stamped}");
    let doc = open(&mut a);
    ok(&mut a, "sign_document", args(doc, json!({ "appearance": { "reason": true, "labels": false } }), "bare.pdf"));
    let stamped = signature_appearances(&dir.join("bare.pdf"));
    assert!(stamped.contains("(Genehmigt)") && !stamped.contains("Reason") && !stamped.contains("Date"), "{stamped}");
}

#[test]
fn optimizing_through_tools() {
    let dir = workdir("optimize");
    // A 2400 × 1600 photo-like JPEG at 600 dpi: a 4 × 2.67 inch page.
    let (w, h) = (2400u32, 1600u32);
    let px: Vec<u8> = (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            [(x * 255 / w) as u8 ^ ((x * y) & 15) as u8, (y * 255 / h) as u8, ((x + y) & 255) as u8]
        })
        .collect();
    let mut jpeg = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95);
    enc.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(600));
    enc.encode(&px, w, h, image::ExtendedColorType::Rgb8).unwrap();
    std::fs::write(dir.join("photo.jpg"), &jpeg).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["photo.jpg"] }))["doc"].as_u64().unwrap();
    let r = ok(
        &mut a,
        "doc_optimize",
        json!({ "doc": doc, "path": "small.pdf", "color": { "ppi": 100, "above_ppi": 150, "compression": "jpeg", "quality": 45 }, "discard": ["metadata"] }),
    );
    assert_eq!((r["images"].as_u64(), r["images_resampled"].as_u64()), (Some(1), Some(1)), "{r}");
    assert!(r["bytes_after"].as_u64().unwrap() * 5 < r["bytes_before"].as_u64().unwrap(), "{r}");
    assert_eq!(r["discarded"][0]["category"], "metadata");
    // The result opens and shows one page of the same size.
    let small = ok(&mut a, "doc_open", json!({ "path": "small.pdf" }));
    assert_eq!(small["pages"], 1);
    let reduced = ok(&mut a, "doc_reduce", json!({ "doc": doc, "path": "reduced.pdf" }));
    assert!(reduced["bytes_after"].as_u64().unwrap() < reduced["bytes_before"].as_u64().unwrap());
    assert_eq!(reduced["written"], true);
    assert_eq!(reduced["images_refused"], 0);
    assert_eq!(std::fs::metadata(dir.join("reduced.pdf")).unwrap().len(), reduced["bytes_after"].as_u64().unwrap());

    // Target PDF 1.4 (disables object streams) and PDF 2.0 (with object_streams toggled off).
    let v14 = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "v14.pdf", "compatibility": "1.4" }));
    assert_eq!(v14["images_refused"], 0);
    assert_eq!((v14["pdf_version"].as_str(), v14["object_streams"].as_bool()), (Some("1.4"), Some(false)), "{v14}");
    let v14_bytes = std::fs::read(dir.join("v14.pdf")).unwrap();
    assert!(v14_bytes.starts_with(b"%PDF-1.4\n") && !v14_bytes.windows(7).any(|w| w == b"/ObjStm"));

    let v20 = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "v20.pdf", "compatibility": "2.0", "object_streams": false }));
    assert_eq!((v20["pdf_version"].as_str(), v20["object_streams"].as_bool()), (Some("2.0"), Some(false)), "{v20}");
    assert!(std::fs::read(dir.join("v20.pdf")).unwrap().starts_with(b"%PDF-2.0\n"));

    // In retain mode, a catalog /Version override is reported as the effective PDF version.
    let mut cos14 = pdfcraft_cos::Document::open(std::sync::Arc::new(v14_bytes)).unwrap();
    let root = cos14.root().unwrap();
    cos14.update_dict(root, |c| c.set(b"Version".to_vec(), pdfcraft_cos::Object::name("1.6"))).unwrap();
    let cat16 = pdfcraft_cos::write_full(&cos14, &pdfcraft_cos::SaveOptions { object_streams: false, ..Default::default() }).unwrap();
    std::fs::write(dir.join("cat16.pdf"), &cat16).unwrap();
    let cat_doc = ok(&mut a, "doc_open", json!({ "path": "cat16.pdf" }))["doc"].as_u64().unwrap();
    let retained = ok(&mut a, "doc_optimize", json!({ "doc": cat_doc, "path": "retained.pdf", "compatibility": "retain", "object_streams": false }));
    assert_eq!(retained["pdf_version"].as_str(), Some("1.6"), "{retained}");
    // The same with the catalog's /Version given by reference: read through it.
    let v16 = cos14.add(pdfcraft_cos::Object::name("1.6"));
    cos14.update_dict(root, |c| c.set(b"Version".to_vec(), pdfcraft_cos::Object::Ref(v16))).unwrap();
    let ref16 = pdfcraft_cos::write_full(&cos14, &pdfcraft_cos::SaveOptions { object_streams: false, ..Default::default() }).unwrap();
    std::fs::write(dir.join("ref16.pdf"), &ref16).unwrap();
    let ref_doc = ok(&mut a, "doc_open", json!({ "path": "ref16.pdf" }))["doc"].as_u64().unwrap();
    let by_ref =
        ok(&mut a, "doc_optimize", json!({ "doc": ref_doc, "path": "retained-ref.pdf", "compatibility": "retain", "object_streams": false }));
    assert_eq!(by_ref["pdf_version"].as_str(), Some("1.6"), "{by_ref}");

    assert!(matches!(a.call("doc_optimize", &json!({ "doc": doc, "path": "x.pdf", "compatibility": "1.3" })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(
        a.call("doc_optimize", &json!({ "doc": doc, "path": "x.pdf", "color": { "compression": "gif" } })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(a.call("doc_optimize", &json!({ "doc": doc, "path": "x.pdf", "discard": ["everything"] })), Err(ToolError::InvalidArgs(_))));
}

/// #3227 review: retaining the version, a catalog `/Version` below the 1.5 an object stream
/// needs is not what viewers read: the header is raised, and the later of the two is reported.
#[test]
fn retained_pdf_version_reports_the_header_raised_for_object_streams() {
    let dir = workdir("optimize-retain-version");
    let mut a = auto(&dir);
    let blank = ok(&mut a, "doc_create", json!({ "from": "blank", "width": 300, "height": 400 }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_optimize", json!({ "doc": blank, "path": "v14.pdf", "compatibility": "1.4" }));
    let mut cos = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("v14.pdf")).unwrap())).unwrap();
    let root = cos.root().unwrap();
    cos.update_dict(root, |c| c.set(b"Version".to_vec(), pdfcraft_cos::Object::name("1.4"))).unwrap();
    let bytes = pdfcraft_cos::write_full(&cos, &pdfcraft_cos::SaveOptions { object_streams: false, ..Default::default() }).unwrap();
    std::fs::write(dir.join("cat14.pdf"), &bytes).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "cat14.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "out.pdf", "compatibility": "retain", "object_streams": true }));
    let out = std::fs::read(dir.join("out.pdf")).unwrap();
    assert!(out.starts_with(b"%PDF-1.5\n") && out.windows(7).any(|w| w == b"/ObjStm"), "{r}");
    assert_eq!(r["pdf_version"].as_str(), Some("1.5"), "{r}");
}

#[test]
fn lzw_streams_are_stored_with_flate_through_tools() {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_full};
    let dir = workdir("optimize-lzw");
    let ops = b"0.2 0.4 0.8 rg 72 72 200 100 re f 0 0 1 RG 72 300 m 400 500 l S".to_vec();
    let mut doc = Document::new_empty();
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let lzw = Stream::from_raw(Dict::from_iter([(b"Filter".to_vec(), Object::name("LZWDecode"))]), lzw_literals(&ops));
    assert_eq!(lzw.decoded_strict().unwrap(), ops);
    let content = doc.add(Object::Stream(lzw));
    let page = doc.add(Object::Dict(Dict::from_iter([
        (b"Type".to_vec(), Object::name("Page")),
        (b"Parent".to_vec(), Object::Ref(pages)),
        (b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()])),
        (b"Contents".to_vec(), Object::Ref(content)),
    ])));
    doc.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page)]));
        d.set(b"Count".to_vec(), Object::Int(1));
    })
    .unwrap();
    std::fs::write(dir.join("lzw.pdf"), write_full(&doc, &SaveOptions::default()).unwrap()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "lzw.pdf" }))["doc"].as_u64().unwrap();
    let filters = |name: &str| {
        let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join(name)).unwrap())).unwrap();
        let catalog = saved.get(saved.root().unwrap()).as_dict().cloned().unwrap();
        let pages = saved.get(catalog.reference(b"Pages").unwrap()).as_dict().cloned().unwrap();
        let page = saved.get(pages.get(b"Kids").and_then(|k| k.as_array()?.first()?.as_ref()).unwrap()).as_dict().cloned().unwrap();
        match &*saved.get(page.reference(b"Contents").unwrap()) {
            Object::Stream(s) => (s.dict.get(b"Filter").cloned(), s.decoded_strict().unwrap()),
            _ => panic!("the page content is not a stream"),
        }
    };
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "flate.pdf" }));
    assert_eq!(r["lzw_streams"].as_u64(), Some(1), "{r}");
    assert_eq!(filters("flate.pdf"), (Some(Object::name("FlateDecode")), ops.clone()));
    let off = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "kept.pdf", "lzw_to_flate": false }));
    assert_eq!(off["lzw_streams"].as_u64(), Some(0), "{off}");
    assert_eq!(filters("kept.pdf"), (Some(Object::name("LZWDecode")), ops));
}

/// A scan saved as a 1-bit PNG becomes a page whose picture stays one bit per pixel (not the eight
/// bits the PNG decodes to), and Export all images gives the picture back.
#[test]
fn a_one_bit_scan_stays_one_bit_and_exports() {
    let dir = workdir("bilevel");
    let (w, h) = (800u32, 200u32);
    let stride = (w as usize).div_ceil(8);
    let mut bits = vec![0xFFu8; stride * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            if y % 25 >= 6 && y % 25 < 18 && (x * 7 + y / 25 * 13) % 11 < 4 {
                bits[y * stride + x / 8] &= !(0x80 >> (x % 8));
            }
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, w, h);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::One);
        enc.write_header().unwrap().write_image_data(&bits).unwrap();
    }
    std::fs::write(dir.join("scan.png"), &png_bytes).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_create", json!({ "from": "images", "paths": ["scan.png"] }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "scan.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("scan.pdf")).unwrap())).unwrap();
    let images: Vec<Option<i64>> = saved
        .object_numbers()
        .into_iter()
        .filter_map(|num| match &*saved.get(pdfcraft_cos::ObjRef { num, generation: saved.generation(num) }) {
            pdfcraft_cos::Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Image") => Some(s.dict.int(b"BitsPerComponent")),
            _ => None,
        })
        .collect();
    assert_eq!(images, [Some(1)]);
    let r = ok(&mut a, "doc_export_all_images", json!({ "doc": doc, "folder": "out" }));
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{r}");
    let exported = std::fs::read(files[0]["path"].as_str().unwrap()).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(exported)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buf).unwrap();
    let white: Vec<bool> = buf[..frame.buffer_size()].as_chunks::<3>().0.iter().map(|p| *p == [255, 255, 255]).collect();
    let expected: Vec<bool> =
        (0..h as usize).flat_map(|y| (0..w as usize).map(move |x| (y, x))).map(|(y, x)| bits[y * stride + x / 8] & (0x80 >> (x % 8)) != 0).collect();
    assert_eq!(white, expected);
}

/// With `mono: { compression: jbig2 }` a scan's black-and-white image is stored as lossless JBIG2,
/// smaller than as Group 4, and exports as the same picture.
#[test]
fn doc_optimize_stores_black_and_white_images_as_jbig2() {
    let dir = workdir("optimize-jbig2");
    std::fs::write(dir.join("scan.pdf"), scanned_page()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "scan.pdf" }))["doc"].as_u64().unwrap();
    let g4 = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "g4.pdf" }));
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "jbig2.pdf", "mono": { "compression": "jbig2" } }));
    assert_eq!((r["images"].as_u64(), r["images_recompressed"].as_u64()), (Some(1), Some(1)), "{r}");
    assert!(r["image_bytes_after"].as_u64() < g4["image_bytes_after"].as_u64(), "JBIG2 {r}, Group 4 {g4}");
    let written = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("jbig2.pdf")).unwrap())).unwrap();
    let filters: Vec<Vec<u8>> = written
        .object_numbers()
        .into_iter()
        .filter_map(|num| match &*written.get(pdfcraft_cos::ObjRef { num, generation: written.generation(num) }) {
            pdfcraft_cos::Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Image") => s.dict.name(b"Filter").map(<[u8]>::to_vec),
            _ => None,
        })
        .collect();
    assert_eq!(filters, [b"JBIG2Decode".to_vec()]);
    // Export ▸ Images gives the copy's picture back as it gives the scan's.
    let copy = ok(&mut a, "doc_open", json!({ "path": "jbig2.pdf" }))["doc"].as_u64().unwrap();
    let mut exported = |doc: u64, folder: &str| {
        let r = ok(&mut a, "doc_export_all_images", json!({ "doc": doc, "folder": folder }));
        let files = r["files"].as_array().unwrap();
        assert_eq!(files.len(), 1, "{r}");
        std::fs::read(files[0]["path"].as_str().unwrap()).unwrap()
    };
    assert_eq!(exported(copy, "from-jbig2"), exported(doc, "from-scan"));
}

/// A scan's black-and-white image is stored as CCITT Group 4 by default, and kept as it is with
/// `mono: { compression: retain }`.
#[test]
fn doc_optimize_stores_black_and_white_images_as_ccitt_group_4() {
    let dir = workdir("optimize-mono");
    std::fs::write(dir.join("scan.pdf"), scanned_page()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "scan.pdf" }))["doc"].as_u64().unwrap();
    let kept = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "kept.pdf", "mono": { "compression": "retain" } }));
    assert_eq!((kept["images"].as_u64(), kept["images_recompressed"].as_u64()), (Some(1), Some(0)), "{kept}");
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "small.pdf" }));
    assert_eq!((r["images"].as_u64(), r["images_recompressed"].as_u64()), (Some(1), Some(1)), "{r}");
    assert!(r["image_bytes_after"].as_u64() < r["image_bytes_before"].as_u64(), "{r}");
    // The copy written stores the image as Group 4.
    let small = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("small.pdf")).unwrap())).unwrap();
    let filters: Vec<Vec<u8>> = small
        .object_numbers()
        .into_iter()
        .filter_map(|num| match &*small.get(pdfcraft_cos::ObjRef { num, generation: small.generation(num) }) {
            pdfcraft_cos::Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Image") => s.dict.name(b"Filter").map(<[u8]>::to_vec),
            _ => None,
        })
        .collect();
    assert_eq!(filters, [b"CCITTFaxDecode".to_vec()]);
    // An unknown compression, one that isn't a name, a misspelled key, or a mono that isn't an
    // object is refused, not read as the default.
    for mono in [json!({ "compression": "jpeg" }), json!({ "compression": false }), json!({ "compresson": "retain" }), json!("retain")] {
        assert!(matches!(a.call("doc_optimize", &json!({ "doc": doc, "path": "x.pdf", "mono": mono })), Err(ToolError::InvalidArgs(_))), "{mono}");
    }
}

/// A scan drawn at 600 ppi is averaged down to 300 ppi by default. `mono: { downsample: false }`
/// keeps its size, `ppi` and `above_ppi` choose others, and values of the wrong type are refused.
#[test]
fn doc_optimize_averages_black_and_white_scans_down() {
    let dir = workdir("optimize-mono-downsample");
    // 1000 × 400 samples on 120 × 48 pt: 600 ppi.
    std::fs::write(dir.join("scan.pdf"), simple_rule_scan_drawn(b"q 120 0 0 48 50 500 cm /Scan Do Q")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "scan.pdf" }))["doc"].as_u64().unwrap();
    let sizes = |name: &str| image_sizes(&dir.join(name));
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "small.pdf" }));
    assert_eq!(r["images_resampled"].as_u64(), Some(1), "{r}");
    assert_eq!(sizes("small.pdf"), [(Some(500), Some(200))]);
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "kept.pdf", "mono": { "downsample": false } }));
    assert_eq!(r["images_resampled"].as_u64(), Some(0), "{r}");
    assert_eq!(sizes("kept.pdf"), [(Some(1000), Some(400))]);
    ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "200.pdf", "mono": { "ppi": 200, "above_ppi": 250 } }));
    assert_eq!(sizes("200.pdf"), [(Some(333), Some(133))]);
    for mono in [json!({ "downsample": "yes" }), json!({ "ppi": "300" }), json!({ "above": 450 })] {
        assert!(matches!(a.call("doc_optimize", &json!({ "doc": doc, "path": "x.pdf", "mono": mono })), Err(ToolError::InvalidArgs(_))), "{mono}");
    }
}

/// The batch optimize step's `ppi` reaches black-and-white images too: they go to the larger of
/// it and Reduce File Size's 300 ppi. Before, a batch run took every scan to 300 ppi whatever
/// `ppi` said, so `ppi: 600` halved 600 ppi scans.
#[test]
fn batch_ppi_bounds_how_far_black_and_white_scans_are_resampled() {
    let dir = workdir("batch-mono-ppi");
    // 1000 × 400 samples on 120 × 48 pt: 600 ppi.
    std::fs::write(dir.join("scan.pdf"), simple_rule_scan_drawn(b"q 120 0 0 48 50 500 cm /Scan Do Q")).unwrap();
    let mut a = auto(&dir);
    for (ppi, size) in [(None, (500, 200)), (Some(600), (1000, 400)), (Some(96), (500, 200))] {
        let out = format!("out-{ppi:?}");
        let mut args = json!({ "paths": ["scan.pdf"], "steps": ["optimize"], "out_dir": out });
        if let Some(ppi) = ppi {
            args["ppi"] = json!(ppi);
        }
        let report = ok(&mut a, "doc_batch", args);
        assert_eq!(report["completed"], 1, "{report}");
        assert_eq!(image_sizes(&dir.join(&out).join("scan.pdf")), [(Some(size.0), Some(size.1))], "ppi {ppi:?}");
    }
}

#[test]
fn a_page_printed_in_strips_gets_small_through_tools() {
    let dir = workdir("optimize-strips");
    std::fs::write(dir.join("strips.pdf"), printed_in_strips(300)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "strips.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "mobile.pdf", "preset": "mobile" }));
    assert_eq!((r["merged_images"].as_u64(), r["fragments_merged"].as_u64(), r["unused_xobjects"].as_u64()), (Some(1), Some(300), Some(300)), "{r}");
    assert!(r["bytes_after"].as_u64().unwrap() * 10 < r["bytes_before"].as_u64().unwrap(), "{r}");
    // Reduce File Size merges too; with the option off, the strips stay.
    let reduced = ok(&mut a, "doc_reduce", json!({ "doc": doc, "path": "reduced.pdf" }));
    assert!(reduced["bytes_after"].as_u64().unwrap() * 10 < reduced["bytes_before"].as_u64().unwrap(), "{reduced}");
    let off = ok(&mut a, "doc_optimize", json!({ "doc": doc, "path": "off.pdf", "merge_fragments": false }));
    assert_eq!(off["merged_images"].as_u64(), Some(0), "{off}");
    // The merged page opens and renders.
    let mobile = ok(&mut a, "doc_open", json!({ "path": "mobile.pdf" }))["doc"].as_u64().unwrap();
    assert!(a.call("page_render", &json!({ "doc": mobile, "page": 1, "dpi": 72 })).is_ok());
    assert!(matches!(a.call("doc_optimize", &json!({ "doc": doc, "path": "x.pdf", "preset": "tiny" })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn initial_view_through_tools() {
    let dir = workdir("initial-view");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "doc_initial_view", json!({ "doc": doc }));
    assert_eq!((r["initial_view"]["layout"].as_str(), r["initial_view"]["page"].as_u64()), (Some("Default"), Some(1)));
    let r = ok(
        &mut a,
        "doc_initial_view",
        json!({ "doc": doc, "navigation": "pages", "layout": "two_up", "magnification": 150, "page": 2, "language": "en-GB", "binding": "right", "display_title": true }),
    );
    let v = &r["initial_view"];
    assert_eq!((v["navigation"].as_str(), v["magnification"].as_str(), v["page"].as_u64()), (Some("Pages"), Some("Percent(150.0)"), Some(2)));
    assert_eq!((v["language"].as_str(), v["binding"].as_str()), (Some("en-GB"), Some("right")));
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Change initial view");
    assert!(matches!(a.call("doc_initial_view", &json!({ "doc": doc, "layout": "spiral" })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("doc_initial_view", &json!({ "doc": doc, "page": 9 })), Err(ToolError::Failed(_))));
}
