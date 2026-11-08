//! Unit tests of the editing operations (part 2 of 7).

#[allow(unused_imports)]
use super::*;

#[test]
fn tracked_text_positions_do_not_count_character_spacing_twice() {
    let expected = "TRACKED SAMPLE TEXT";
    for char_spacing in [0.0, 2.0] {
        for transform in ["1 0 0 1 0 0", "0 1 -1 0 800 0"] {
            let mut content = format!("q {transform} cm BT /F1 10 Tf {char_spacing} Tc ");
            let mut x = 72.0;
            for c in expected.chars() {
                content.push_str(&format!("1 0 0 1 {x:.4} 700 Tm ({c}) Tj "));
                x += pdfcraft_fonts::helvetica_width(&c.to_string(), 10.0) + 2.0001;
            }
            content.push_str("ET Q");
            let doc = text_page(&content);
            assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].text, expected);
            assert_eq!(crate::objects::editable_objects(&doc, 0).unwrap()[0].text.as_deref(), Some(expected));
        }
    }
}

#[test]
fn japanese_line_uses_unicode_type3_fallback() {
    if without_craft_fonts("japanese_line_uses_unicode_type3_fallback") {
        return;
    }
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let replacement = "25362738こんにちは、お元気ですか？ 2.3444";
    let result = text::replace_line(&mut doc, 0, 0, replacement).unwrap();
    assert_eq!(result.substituted.as_deref(), Some("BIZ UDPGothic Type3"));
    let bytes = page_content_bytes(&doc, 0);
    let content = String::from_utf8_lossy(&bytes);
    assert!(content.contains("/PCJp"), "{content}");
    let reopened = reopen(&doc);
    assert_eq!(text::text_lines(&reopened, 0).unwrap()[0].text, replacement);
}

#[test]
fn japanese_line_fallback_uses_real_source_family_and_weight() {
    if without_craft_fonts("japanese_line_fallback_uses_real_source_family_and_weight") {
        return;
    }
    let mut sans = styled_text_page("Helvetica");
    let mut bold = descriptor_text_page();
    let mut serif = styled_text_page("ABCDEF+ShipporiMincho-Regular");
    let replacement = "見本商会　御中";
    text::replace_line(&mut sans, 0, 0, replacement).unwrap();
    text::replace_line(&mut bold, 0, 0, replacement).unwrap();
    text::replace_line(&mut serif, 0, 0, replacement).unwrap();
    assert_ne!(fallback_paths(&sans), fallback_paths(&serif), "sans source must not use Mincho outlines");
    if pdfcraft_fonts::CRAFT_FONTS.iter().any(|f| f.family == "BIZ UDPGothic" && f.style == "Bold") {
        assert_ne!(fallback_paths(&sans), fallback_paths(&bold), "bold must change the actual glyph paths");
    }
    for doc in [&sans, &bold, &serif] {
        assert_eq!(text::text_lines(&reopen(doc), 0).unwrap()[0].text, replacement);
    }
}

#[test]
fn japanese_paragraph_fallback_honors_requested_family_and_weight() {
    if without_craft_fonts("japanese_paragraph_fallback_honors_requested_family_and_weight") {
        return;
    }
    let replacement = "見本商会　御中";
    let mut regular = styled_text_page("Helvetica");
    let mut bold = styled_text_page("Helvetica");
    let mut serif = styled_text_page("Helvetica");
    text::replace_block(&mut regular, 0, 0, replacement).unwrap();
    text::rewrite_block(
        &mut bold,
        0,
        0,
        Some(replacement),
        &text::BlockStyle { family: Some((added::Family::Helvetica, true, false)), ..Default::default() },
    )
    .unwrap();
    text::rewrite_block(
        &mut serif,
        0,
        0,
        Some(replacement),
        &text::BlockStyle { family: Some((added::Family::Times, false, false)), ..Default::default() },
    )
    .unwrap();
    assert_ne!(fallback_paths(&regular), fallback_paths(&serif), "requested serif must change actual outlines");
    if pdfcraft_fonts::CRAFT_FONTS.iter().any(|f| f.family == "BIZ UDPGothic" && f.style == "Bold") {
        assert_ne!(fallback_paths(&regular), fallback_paths(&bold), "requested bold must change actual outlines");
        let mut source_bold = styled_text_page("Helvetica-Bold");
        text::replace_block(&mut source_bold, 0, 0, replacement).unwrap();
        assert_eq!(fallback_paths(&bold), fallback_paths(&source_bold));
    }
    for doc in [&regular, &bold, &serif] {
        assert_eq!(text::text_blocks(&reopen(doc), 0).unwrap()[0].text, replacement);
    }
}

#[test]
fn japanese_fallback_reports_the_face_after_save_and_reopen() {
    if without_craft_fonts("japanese_fallback_reports_the_face_after_save_and_reopen") {
        return;
    }
    // F2 is (a subset of) Arial: a regular sans face, so the fallback is the regular Gothic.
    let face = pdfcraft_fonts::document_japanese_font_for_style(false, false).unwrap();
    let expected = format!("{}-{}", face.family, face.style).replace(' ', "");
    for paragraph in [false, true] {
        let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
        let replacement = "見本商会　御中";
        if paragraph {
            text::replace_block(&mut doc, 0, 0, replacement).unwrap();
        } else {
            text::replace_line(&mut doc, 0, 0, replacement).unwrap();
        }
        for d in [&doc, &reopen(&doc)] {
            let lines = text::text_lines(d, 0).unwrap();
            assert_eq!(lines[0].text, replacement);
            assert_eq!(lines[0].base_font, expected, "report the real fallback face, not an empty BaseFont");
            assert_eq!(text::text_blocks(d, 0).unwrap()[0].base_font, expected);
        }
    }
}

#[test]
fn cyrillic_in_a_serif_line_falls_back_to_a_face_that_has_it() {
    if without_craft_fonts("cyrillic_in_a_serif_line_falls_back_to_a_face_that_has_it") {
        return;
    }
    let replacement = "Привет, world";
    let has_all = |f: &pdfcraft_fonts::CraftFont| replacement.chars().all(|ch| pdfcraft_fonts::japanese_glyph_from(f, ch).is_ok());
    if !pdfcraft_fonts::CRAFT_FONTS.iter().any(|f| f.covers("Jpan") && has_all(f)) {
        eprintln!("skipping: no Japanese fallback face has Cyrillic");
        return;
    }
    for paragraph in [false, true] {
        let mut doc = styled_text_page("Times-Roman");
        if paragraph {
            text::replace_block(&mut doc, 0, 0, replacement).unwrap();
        } else {
            text::replace_line(&mut doc, 0, 0, replacement).unwrap();
        }
        assert_eq!(text::text_lines(&reopen(&doc), 0).unwrap()[0].text, replacement);
    }
}

/// #2593: the craft-fonts faces are Japanese, and part of the Cyrillic (Uzbek қ ғ ҳ, Kazakh ә ө ү
/// ұ һ) is in none of them. The bundled Inter face has it, so the replacement is written instead of
/// being refused, and a build without craft-fonts can do it too.
#[test]
fn uzbek_and_kazakh_cyrillic_use_the_bundled_latin_face() {
    for replacement in ["The quick brown fox jumps over the lazy dog қизил", "әже", "ўзбекча ғўза"] {
        for paragraph in [false, true] {
            let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
            let edit =
                if paragraph { text::replace_block(&mut doc, 0, 0, replacement) } else { text::replace_line(&mut doc, 0, 0, replacement) }.unwrap();
            assert_eq!(edit.substituted.as_deref(), Some("Inter Type3"), "{replacement}: {edit:?}");
            let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
            assert!(content.contains("/PCLat"), "{replacement}: {content}");
            let lines = text::text_lines(&reopen(&doc), 0).unwrap();
            assert_eq!(lines[0].text, replacement);
            assert_eq!(lines[0].base_font, "Inter-Regular", "the generated font reports the face it drew");
        }
    }
}

/// The Japanese faces keep first refusal for the Cyrillic they do carry, so Russian text keeps the
/// face it was drawn with before the bundled Latin face was added.
#[test]
fn cyrillic_the_japanese_faces_carry_keeps_them() {
    if without_craft_fonts("cyrillic_the_japanese_faces_carry_keeps_them") {
        return;
    }
    let face = pdfcraft_fonts::document_japanese_font_for_style(false, false).unwrap();
    if pdfcraft_fonts::japanese_glyph_from(face, 'ф').is_err() {
        eprintln!("skipping: the Japanese fallback face has no Cyrillic");
        return;
    }
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let edit = text::replace_line(&mut doc, 0, 0, "Привет, world").unwrap();
    let expected = format!("{} Type3", face.family);
    assert_eq!(edit.substituted.as_deref(), Some(expected.as_str()));
}

#[test]
fn japanese_paragraph_uses_unicode_type3_fallback() {
    if without_craft_fonts("japanese_paragraph_uses_unicode_type3_fallback") {
        return;
    }
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let replacement = "こんにちは、お元気ですか？ 2.3444 日本語の文章";
    text::replace_block(&mut doc, 0, 0, replacement).unwrap();
    let reopened = reopen(&doc);
    assert_eq!(text::text_blocks(&reopened, 0).unwrap()[0].text, replacement);
}

#[test]
fn fallback_fonts_nothing_shows_any_more_leave_with_a_rewrite() {
    // `PCJp1` is selected by the page, `PCJp2` only by a form that uses the page's resources, `PCJp`
    // by nothing; `PCJpOther` is a Type 3 font of the file's own that only shares the prefix.
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Some text) Tj ET BT /PCJp1 12 Tf ET q /Fm0 Do Q");
    let generated = |doc: &mut Document, name: &str| {
        doc.add(Object::Dict(Dict::from_iter([
            (b"Type".to_vec(), Object::name("Font")),
            (b"Subtype".to_vec(), Object::name("Type3")),
            (b"Name".to_vec(), Object::name(name)),
        ])))
    };
    let (unused, page_shown, form_shown, own) =
        (generated(&mut doc, "PCJapanese"), generated(&mut doc, "PCJapanese"), generated(&mut doc, "PCJapanese"), generated(&mut doc, "Other"));
    let form = doc.add(Object::Stream(Stream::from_raw(
        Dict::from_iter([(b"Subtype".to_vec(), Object::name("Form")), (b"BBox".to_vec(), Object::Array(vec![Object::Int(0); 4]))]),
        b"BT /PCJp2 12 Tf ET".to_vec(),
    )));
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let mut fonts = Dict::from_iter([(b"F1".to_vec(), Object::Ref(pdfcraft_cos::ObjRef::new(5, 0)))]);
    for (name, font) in [("PCJp", unused), ("PCJp1", page_shown), ("PCJp2", form_shown), ("PCJpOther", own)] {
        fonts.set(name.as_bytes().to_vec(), Object::Ref(font));
    }
    let resources = Dict::from_iter([
        (b"Font".to_vec(), Object::Dict(fonts)),
        (b"XObject".to_vec(), Object::Dict(Dict::from_iter([(b"Fm0".to_vec(), Object::Ref(form))]))),
    ]);
    doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(resources.clone()))).unwrap();
    let names = |doc: &Document| {
        let page = pdfcraft_model::pages(doc).remove(0);
        let res = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
        let mut names: Vec<String> =
            doc.resolve(res.get(b"Font").unwrap()).as_dict().unwrap().iter().map(|(n, _)| String::from_utf8_lossy(n).into_owned()).collect();
        names.sort();
        names
    };
    // A rewrite that doesn't reach the fallback fonts still drops the one nothing shows.
    text::rewrite_block(&mut doc, 0, 0, Some("Other text"), &text::BlockStyle::default()).unwrap();
    assert_eq!(names(&reopen(&doc)), ["F1", "PCJp1", "PCJp2", "PCJpOther"]);

    // A form using the page's resources whose content can't be read might show any of them: they
    // all stay.
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Some text) Tj ET q /Fm0 Do Q");
    let unused = generated(&mut doc, "PCJapanese");
    let broken = doc.add(Object::Stream(Stream::from_raw(
        Dict::from_iter([
            (b"Subtype".to_vec(), Object::name("Form")),
            (b"BBox".to_vec(), Object::Array(vec![Object::Int(0); 4])),
            (b"Filter".to_vec(), Object::name("FlateDecode")),
        ]),
        b"not deflate data".to_vec(),
    )));
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let fonts = Dict::from_iter([(b"F1".to_vec(), Object::Ref(pdfcraft_cos::ObjRef::new(5, 0))), (b"PCJp".to_vec(), Object::Ref(unused))]);
    let resources = Dict::from_iter([
        (b"Font".to_vec(), Object::Dict(fonts)),
        (b"XObject".to_vec(), Object::Dict(Dict::from_iter([(b"Fm0".to_vec(), Object::Ref(broken))]))),
    ]);
    doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
    text::rewrite_block(&mut doc, 0, 0, Some("Other text"), &text::BlockStyle::default()).unwrap();
    assert_eq!(names(&doc), ["F1", "PCJp"]);
}

#[test]
fn rewriting_japanese_twice_keeps_only_the_fallback_fonts_it_shows() {
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello) Tj ET");
    if text::rewrite_block(&mut doc, 0, 0, Some("日本語の文章"), &text::BlockStyle::default()).is_err() {
        assert!(pdfcraft_fonts::document_japanese_font().is_none());
        eprintln!("built without a craft-fonts Japanese face: the Type 3 path is checked with CRAFT_FONTS_DIR");
        return;
    }
    let first: Vec<_> = fallback_fonts(&doc).into_iter().map(|(_, f)| f).collect();
    text::rewrite_block(&mut doc, 0, 0, Some("別の文章です"), &text::BlockStyle::default()).unwrap();
    let doc = reopen(&doc);
    let after = fallback_fonts(&doc);
    assert!(!after.is_empty());
    assert!(after.iter().all(|(_, f)| !first.contains(f)), "the first rewrite's fonts are gone");
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert!(after.iter().all(|(name, _)| content.contains(&format!("/{} ", String::from_utf8_lossy(name)))), "{content}");
}

/// More unique fallback glyphs than one Type 3 font holds (240) are split across several generated
/// fonts, each within that limit, and the text still reads back as one line.
#[test]
fn japanese_replacement_over_240_unique_glyphs_is_split_across_fallback_fonts() {
    let replacement = covered_japanese_chars(300);
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    if without_craft_fonts("japanese_replacement_over_240_unique_glyphs_is_split_across_fallback_fonts") {
        // Without craft-fonts the long text gets the clear error, not the old limit message.
        let err = text::replace_line(&mut doc, 0, 0, &replacement).unwrap_err().to_string();
        assert!(err.contains("CRAFT_FONTS_DIR"), "{err}");
        return;
    }
    text::replace_line(&mut doc, 0, 0, &replacement).unwrap();
    let reopened = reopen(&doc);
    let lines = text::text_lines(&reopened, 0).unwrap();
    assert_eq!(lines.len(), 1, "one line: {:?}", lines.iter().map(|l| &l.text).collect::<Vec<_>>());
    assert_eq!(lines[0].text, replacement);
    let fonts = fallback_fonts(&reopened);
    assert_eq!(fonts.len(), 2, "300 glyphs take a 240-glyph font and a 60-glyph font");
    let mut glyphs = 0;
    for (name, font) in &fonts {
        let procs = reopened.resolve(font.get(b"CharProcs").unwrap());
        let count = procs.as_dict().unwrap().len();
        let last = font.get(b"LastChar").and_then(|o| o.as_int()).unwrap_or(i64::MAX);
        assert!(count <= 240 && last <= 240, "{name:?}: {count} glyphs, LastChar {last}");
        glyphs += count;
    }
    assert_eq!(glyphs, 300);
}

#[test]
fn styled_paragraph_batches_cyrillic_without_changing_source_fonts() {
    if without_craft_fonts("styled_paragraph_batches_cyrillic_without_changing_source_fonts") {
        return;
    }
    let mut doc = text_page(
        "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (The quick brown fox jumps over the lazy dog) Tj T* (and keeps running through the long field.) Tj ET",
    );
    let replacement = "The quick brown fox jumps over the lazy dog шалом\nand keeps running through the long field.";
    text::rewrite_block(&mut doc, 0, 0, Some(replacement), &text::BlockStyle { width: Some(520.0), ..Default::default() }).unwrap();
    let doc = reopen(&doc);
    assert_eq!(fallback_fonts(&doc).len(), 1, "the five letters must share one font");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert!(lines.iter().any(|l| l.base_font == "Helvetica" && l.text.starts_with("The quick brown fox")), "{lines:?}");
    let fallback: Vec<_> = lines.iter().filter(|l| l.font.starts_with("PCJp")).collect();
    assert_eq!(fallback.len(), 1, "one word must be one text run: {fallback:?}");
    assert_eq!(fallback[0].text, "шалом");
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>().replace(' ', ""), replacement.replace([' ', '\n'], ""));
}

#[test]
fn styled_paragraph_over_240_fallback_glyphs_splits_and_refuses_atomically() {
    let replacement = format!("{}\nLatin", covered_japanese_chars(300));
    let mut doc = text_page("BT /F1 12 Tf 14.4 TL 72 700 Td (Original) Tj T* (Latin) Tj ET");
    let original = write_incremental(&doc, &SaveOptions::default()).unwrap();
    if without_craft_fonts("styled_paragraph_over_240_fallback_glyphs_splits_and_refuses_atomically") {
        assert!(text::replace_block(&mut doc, 0, 0, &replacement).is_err());
        assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), original);
        return;
    }
    text::rewrite_block(&mut doc, 0, 0, Some(&replacement), &text::BlockStyle { width: Some(14000.0), ..Default::default() }).unwrap();
    let reopened = reopen(&doc);
    let fonts = fallback_fonts(&reopened);
    assert_eq!(fonts.len(), 2, "300 glyphs need two fonts, not an error at glyph 241");
    let counts: Vec<_> = fonts.iter().map(|(_, f)| reopened.dict(f.get(b"CharProcs").unwrap()).unwrap().len()).collect();
    assert_eq!(counts.iter().sum::<usize>(), 300);
    assert!(counts.iter().all(|n| *n <= 240));
    assert_eq!(text::text_lines(&reopened, 0).unwrap().iter().map(|l| l.text.as_str()).collect::<String>(), replacement.replace('\n', ""));
    let before = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let snapshot = doc.clone();
    let err = text::replace_block(&mut doc, 0, 0, &format!("{replacement}\u{10ffff}")).unwrap_err();
    assert!(err.to_string().contains("U+10FFFF"), "{err}");
    assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), before);
    assert_eq!(write_incremental(&snapshot, &SaveOptions::default()).unwrap(), before);
}

#[test]
fn styled_fallback_batches_keep_face_choices_and_range_styles() {
    if without_craft_fonts("styled_fallback_batches_keep_face_choices_and_range_styles") {
        return;
    }
    if !pdfcraft_fonts::CRAFT_FONTS.iter().any(|f| f.family == "BIZ UDPGothic" && f.style == "Bold") {
        eprintln!("skipping: no bold Gothic face");
        return;
    }
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Original) Tj ET");
    let range = |range, family, bold| text::RangeStyle {
        range,
        family: Some((family, bold, false)),
        resource: None,
        size: None,
        color: None,
        underline: None,
        char_spacing: None,
        scale: None,
    };
    let style = text::BlockStyle {
        width: Some(520.0),
        ranges: vec![
            range(3..6, added::Family::Times, false),
            range(6..9, added::Family::Helvetica, true),
            range(9..11, added::Family::Times, false),
        ],
        ..Default::default()
    };
    let original = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let err = text::rewrite_block(&mut doc, 0, 0, Some("見見見ш見ш\nLatin\u{10ffff}"), &style).unwrap_err();
    assert!(err.to_string().contains("U+10FFFF"), "{err}");
    assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), original, "all faces must validate before writing fallback objects");
    text::rewrite_block(&mut doc, 0, 0, Some("見見見ш見ш\nLatin"), &style).unwrap();
    let doc = reopen(&doc);
    let fonts = fallback_fonts(&doc);
    let mut summary: Vec<_> = fonts
        .iter()
        .map(|(_, f)| {
            let face = pdfcraft_fonts::pdf::Metrics::from_dict(&doc, f).base_font;
            let count = doc.dict(f.get(b"CharProcs").unwrap()).unwrap().len();
            (face, count)
        })
        .collect();
    summary.sort();
    assert_eq!(
        summary,
        [
            ("BIZUDMincho-Regular".into(), 1),
            ("BIZUDPGothic-Bold".into(), 1),
            ("BIZUDPGothic-Regular".into(), 2),
            ("ShipporiMincho-Regular".into(), 1)
        ]
    );
    assert_eq!(text::text_lines(&doc, 0).unwrap().iter().map(|l| l.text.as_str()).collect::<String>(), "見見見ш見шLatin");
}

#[test]
fn styled_fallback_budget_is_enforced_before_objects_are_written() {
    let glyphs = covered_japanese_chars(65);
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Original) Tj ET");
    let original = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let snapshot = doc.clone();
    let style = text::BlockStyle { width: Some(14000.0), ..Default::default() };
    let over_budget = format!("{glyphs}\nLatin");
    let err = text::rewrite_block_with_limit(&mut doc, 0, 0, Some(&over_budget), &style, 1).unwrap_err().to_string();
    assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), original, "refusal must add no objects");
    assert_eq!(write_incremental(&snapshot, &SaveOptions::default()).unwrap(), original);
    if without_craft_fonts("styled_fallback_budget_is_enforced_before_objects_are_written") {
        assert!(err.contains("CRAFT_FONTS_DIR"), "{err}");
        return;
    }
    assert!(err.contains("more than 64 fallback glyphs"), "{err}");
    // One glyph per font gives a small, real budget of 64 distinct glyphs. At the
    // boundary a repeated glyph is free: this is not a limit on text length.
    let at_budget: String = glyphs.chars().take(64).chain(glyphs.chars().take(1)).collect();
    text::rewrite_block_with_limit(&mut doc, 0, 0, Some(&format!("{at_budget}\nLatin")), &style, 1).unwrap();
    let doc = reopen(&doc);
    assert_eq!(fallback_fonts(&doc).len(), 64);
    assert_eq!(text::text_lines(&doc, 0).unwrap().iter().map(|l| l.text.as_str()).collect::<String>(), format!("{at_budget}Latin"));
}

#[test]
fn styled_fallback_same_base_font_and_character_keep_both_weights() {
    if without_craft_fonts("styled_fallback_same_base_font_and_character_keep_both_weights") {
        return;
    }
    if !pdfcraft_fonts::CRAFT_FONTS.iter().any(|f| f.family == "BIZ UDPGothic" && f.style == "Bold") {
        eprintln!("skipping: no bold Gothic face");
        return;
    }
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (alpha ) Tj /F2 12 Tf (beta ) Tj ET");
    let source = doc.get(pdfcraft_cos::ObjRef::new(5, 0)).as_dict().unwrap().clone();
    for (object, weight) in [(5, 400), (6, 700)] {
        let mut font = source.clone();
        // The same BaseFont can have different descriptor weights. Changing a
        // RangeStyle family would also rename BaseFont and miss this cache key.
        font.set(b"BaseFont".to_vec(), Object::name("NeutralFace"));
        let mut descriptor = Dict::new();
        descriptor.set(b"FontName".to_vec(), Object::name("NeutralFace"));
        descriptor.set(b"FontWeight".to_vec(), Object::Int(weight));
        font.set(b"FontDescriptor".to_vec(), Object::Dict(descriptor));
        doc.set(pdfcraft_cos::ObjRef::new(object, 0), Object::Dict(font));
    }
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].base_font, lines[1].base_font);
    assert_eq!((lines[0].bold, lines[1].bold), (false, true));
    // Unchanged words anchor each inserted copy of the same letter in its own source style.
    let block = text::text_blocks(&doc, 0).unwrap().remove(0);
    let mut inherited = Vec::new();
    for (range, source) in block.edited_spans("alpha ш beta ш") {
        for _ in "alpha ш beta ш".get(range).unwrap().chars().filter(|ch| *ch == 'ш') {
            inherited.push(lines[source].bold);
        }
    }
    assert_eq!(inherited, [false, true], "the fixture must exercise both weights of the same BaseFont");
    text::replace_block(&mut doc, 0, 0, "alpha ш beta ш").unwrap();
    let doc = reopen(&doc);
    let fonts = fallback_fonts(&doc);
    let mut names: Vec<_> = fonts.iter().map(|(_, f)| pdfcraft_fonts::pdf::Metrics::from_dict(&doc, f).base_font).collect();
    names.sort();
    assert_eq!(names, ["BIZUDPGothic-Bold", "BIZUDPGothic-Regular"]);
    let outlines: Vec<_> = fonts
        .iter()
        .map(|(_, f)| {
            let procs = doc.dict(f.get(b"CharProcs").unwrap()).unwrap();
            assert_eq!(procs.len(), 1, "each weight needs just the shared letter");
            stream_bytes(&doc, procs.iter().next().unwrap().1).unwrap()
        })
        .collect();
    assert_ne!(outlines[0], outlines[1], "weight must change the outline, not only the descriptor");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), "alpha ш beta ш");
}

#[test]
fn styled_fallback_shards_keep_text_advances_and_alignment() {
    if without_craft_fonts("styled_fallback_shards_keep_text_advances_and_alignment") {
        return;
    }
    let source = "BT /F1 12 Tf 0.5 Tc 5 Tw 80 Tz 14.4 TL 72 700 Td (Original row) Tj T* (Second row) Tj ET";
    let style =
        text::BlockStyle { width: Some(520.0), align: Some(added::Align::Center), char_spacing: Some(0.7), scale: Some(80.0), ..Default::default() };
    let mut whole = text_page(source);
    let mut split = text_page(source);
    let replacement = "Latin шалом ш\nSecond row";
    text::rewrite_block_with_limit(&mut whole, 0, 0, Some(replacement), &style, 240).unwrap();
    text::rewrite_block_with_limit(&mut split, 0, 0, Some(replacement), &style, 2).unwrap();
    let (whole, split) = (reopen(&whole), reopen(&split));
    assert_eq!(fallback_fonts(&whole).len(), 1);
    assert_eq!(fallback_fonts(&split).len(), 3);
    let (one, many) = (text::text_lines(&whole, 0).unwrap(), text::text_lines(&split, 0).unwrap());
    assert_eq!(one.len(), many.len(), "{one:?} vs {many:?}");
    for (a, b) in one.iter().zip(&many) {
        assert_eq!(a.text, b.text);
        for (a, b) in a.rect.iter().zip(b.rect) {
            assert!((a - b).abs() < 0.002, "sharding must preserve alignment and advances: {a} vs {b}");
        }
    }
}

/// A replacement whose last glyph no face draws (in a later font) changes nothing: neither the
/// document nor a snapshot taken before the edit, which is what undo restores.
#[test]
fn japanese_replacement_missing_a_glyph_after_240_changes_nothing() {
    if without_craft_fonts("japanese_replacement_missing_a_glyph_after_240_changes_nothing") {
        return;
    }
    // The first CJK character that no face draws: the 242nd unique character, in a second font.
    let faces = pdfcraft_fonts::document_japanese_fonts_for_style(false, false);
    let missing = (0x4E00u32..0x9FFF)
        .filter_map(char::from_u32)
        .find(|ch| faces.iter().all(|face| pdfcraft_fonts::japanese_glyph_from(face, *ch).is_err()))
        .expect("some CJK character is outside every face");
    let replacement = format!("{}{missing}", covered_japanese_chars(241));
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let snapshot = doc.clone();
    let err = text::replace_line(&mut doc, 0, 0, &replacement).unwrap_err().to_string();
    assert!(err.contains(&format!("U+{:04X}", missing as u32)), "{err}");
    assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), saved);
    assert_eq!(write_incremental(&snapshot, &SaveOptions::default()).unwrap(), saved);
}

/// Runs that change font move nothing: with two glyphs per font the text reads back the same, in the
/// same box as with one font, and the font is switched only where the text moves to another font.
#[test]
fn japanese_fallback_split_across_fonts_keeps_order_and_positions() {
    if without_craft_fonts("japanese_fallback_split_across_fonts_keeps_order_and_positions") {
        return;
    }
    // 見 and 商 are in the first font, 御 in the second: runs [見商], [御], [見], three switches.
    let replacement = "見商御見";
    let mut whole = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let mut split = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    text::replace_line_with_limit(&mut whole, 0, 0, replacement, 240).unwrap();
    text::replace_line_with_limit(&mut split, 0, 0, replacement, 2).unwrap();
    let (whole, split) = (reopen(&whole), reopen(&split));
    assert_eq!(fallback_fonts(&whole).len(), 1);
    assert_eq!(fallback_fonts(&split).len(), 2);
    let (one, many) = (text::text_lines(&whole, 0).unwrap(), text::text_lines(&split, 0).unwrap());
    assert_eq!(many.len(), 1, "one line: {:?}", many.iter().map(|l| &l.text).collect::<Vec<_>>());
    assert_eq!(many[0].text, replacement);
    for (a, b) in one[0].rect.iter().zip(many[0].rect.iter()) {
        assert!((a - b).abs() < 1e-6, "box {:?} vs {:?}", one[0].rect, many[0].rect);
    }
    // One `Tf` per run plus the page's own switch to F2 and the switch back after the text.
    let content = String::from_utf8_lossy(&page_content_bytes(&split, 0)).into_owned();
    assert_eq!(content.matches(" Tf").count(), 5, "{content}");
    let content = String::from_utf8_lossy(&page_content_bytes(&whole, 0)).into_owned();
    assert_eq!(content.matches(" Tf").count(), 3, "{content}");
}

/// Only a change of font gets a `Tf`: alternating characters with one glyph per font switch at every
/// character, and a run of one font's characters switches nowhere.
#[test]
fn japanese_fallback_switches_font_only_between_runs() {
    if without_craft_fonts("japanese_fallback_switches_font_only_between_runs") {
        return;
    }
    let alternating = "見商".repeat(500);
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    text::replace_line_with_limit(&mut doc, 0, 0, &alternating, 1).unwrap();
    let reopened = reopen(&doc);
    assert_eq!(text::text_lines(&reopened, 0).unwrap()[0].text, alternating);
    let content = String::from_utf8_lossy(&page_content_bytes(&reopened, 0)).into_owned();
    assert_eq!(content.matches(" Tf").count(), 1000 + 2, "1000 runs, the F2 switch before and after");
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    text::replace_line_with_limit(&mut doc, 0, 0, "見見見", 1).unwrap();
    let content = String::from_utf8_lossy(&page_content_bytes(&reopen(&doc), 0)).into_owned();
    assert_eq!(content.matches(" Tf").count(), 1 + 1 + 1, "{content}");
}

/// A paragraph set in several fallback fonts reads back as one block with the same text, and it
/// wraps across lines.
#[test]
fn japanese_paragraph_split_across_fonts_reads_back_as_one_block() {
    if without_craft_fonts("japanese_paragraph_split_across_fonts_reads_back_as_one_block") {
        return;
    }
    // Spaces are glyphs too, so two glyphs per font needs several fonts; the narrow box wraps the text.
    let replacement = "見 商 御 見 商";
    let style = text::BlockStyle { width: Some(40.0), ..Default::default() };
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    text::rewrite_block_with_limit(&mut doc, 0, 0, Some(replacement), &style, 2).unwrap();
    let reopened = reopen(&doc);
    assert!(fallback_fonts(&reopened).len() >= 2);
    let blocks = text::text_blocks(&reopened, 0).unwrap();
    assert_eq!(blocks.len(), 1, "one paragraph: {:?}", blocks.iter().map(|b| &b.text).collect::<Vec<_>>());
    assert_eq!(blocks[0].text, replacement);
    assert!(blocks[0].lines.len() > 1, "wrapped to {} line(s)", blocks[0].lines.len());
}

/// A paragraph whose move or width is refused (not finite, or drawn in a space it cannot be moved in)
/// leaves the document as it was: the fallback fonts are written only once the edit is accepted.
#[test]
fn japanese_paragraph_refused_before_its_fallback_fonts_are_written() {
    if without_craft_fonts("japanese_paragraph_refused_before_its_fallback_fonts_are_written") {
        return;
    }
    let plain = "BT /F2 12 Tf 72 700 Td (ab) Tj ET";
    let cases = [
        (plain, text::BlockStyle { offset: Some([f64::NAN, 0.0]), ..Default::default() }, "can't be moved that far"),
        (plain, text::BlockStyle { width: Some(f64::INFINITY), ..Default::default() }, "width must be a number"),
        (
            "0 0 0 0 0 0 cm BT /F2 12 Tf 72 700 Td (ab) Tj ET",
            text::BlockStyle { offset: Some([5.0, 0.0]), ..Default::default() },
            "can't be moved in",
        ),
    ];
    for (content, style, message) in cases {
        let mut doc = text_page(content);
        let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
        let err = text::rewrite_block(&mut doc, 0, 0, Some("見本商会　御中"), &style).unwrap_err().to_string();
        assert!(err.contains(message), "{err}");
        assert_eq!(write_incremental(&doc, &SaveOptions::default()).unwrap(), saved, "{message}: the document changed");
    }
}

/// Every Type 3 fallback glyph starts with a well-formed `d1`: six operands, the second 0, and a
/// box that encloses every point of the glyph. Acrobat shows a bullet for a glyph whose `d1`
/// has the wrong operand count.
#[test]
fn type3_fallback_glyphs_declare_a_well_formed_d1() {
    if without_craft_fonts("type3_fallback_glyphs_declare_a_well_formed_d1") {
        return;
    }
    // Cyrillic is respaced (shifted by its side bearing), and its box must move with it.
    let mut glyphs = Vec::new();
    for replacement in ["見本商会 御中", "София 2027"] {
        let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
        text::replace_line(&mut doc, 0, 0, replacement).unwrap();
        glyphs.extend(fallback_paths(&reopen(&doc)));
    }
    assert!(!glyphs.is_empty());
    for glyph in glyphs {
        let text = String::from_utf8(glyph).unwrap();
        let mut lines = text.lines();
        let d1: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(d1.len(), 7, "six operands and d1: {d1:?}");
        assert_eq!(d1[6], "d1");
        let n: Vec<f64> = d1[..6].iter().map(|v| v.parse().unwrap()).collect();
        assert_eq!(n[1], 0.0, "wy");
        assert!(n[2] <= n[4] && n[3] <= n[5], "box: {n:?}");
        for line in lines.filter(|l| l.ends_with(" m") || l.ends_with(" l")) {
            let p: Vec<f64> = line.split_whitespace().take(2).map(|v| v.parse().unwrap()).collect();
            assert!(p[0] >= n[2] && p[0] <= n[4] && p[1] >= n[3] && p[1] <= n[5], "{line} outside {n:?}");
        }
    }
}

/// Shippori Mincho, the serif fallback, has no Cyrillic: Bulgarian replacement text in a serif
/// line takes a fallback face that has every letter instead of failing on the first one.
#[test]
fn cyrillic_replacement_in_a_serif_line_uses_a_face_with_the_letters() {
    if without_craft_fonts("cyrillic_replacement_in_a_serif_line_uses_a_face_with_the_letters") {
        return;
    }
    let replacement = "София 2027";
    for paragraph in [false, true] {
        let mut doc = styled_text_page("ABCDEF+Garamond");
        if paragraph {
            text::replace_block(&mut doc, 0, 0, replacement).unwrap();
        } else {
            text::replace_line(&mut doc, 0, 0, replacement).unwrap();
        }
        let lines = text::text_lines(&reopen(&doc), 0).unwrap();
        assert_eq!(lines[0].text, replacement);
        // Letters are spaced by their shape, not one em each (the faces' full-width Cyrillic).
        let widths = fallback_widths(&doc);
        assert!(widths.iter().all(|w| *w > 300.0 && *w < 900.0), "{widths:?}");
        assert!(widths.windows(2).any(|w| w[0] != w[1]), "proportional: {widths:?}");
    }
}

#[test]
fn japanese_paragraph_keeps_ideographic_spaces() {
    if without_craft_fonts("japanese_paragraph_keeps_ideographic_spaces") {
        return;
    }
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Original paragraph) Tj ET");
    let replacement = "　見本商会　　御中　";
    let style = text::BlockStyle { width: Some(400.0), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some(replacement), &style).unwrap();
    let reopened = reopen(&doc);
    let lines = text::text_lines(&reopened, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, replacement);
    assert_eq!(text::text_blocks(&reopened, 0).unwrap()[0].text, replacement);
    // Restyling the saved paragraph without new text must also preserve its spacing.
    let mut doc = reopened;
    text::rewrite_block(&mut doc, 0, 0, None, &text::BlockStyle { size: Some(14.0), ..style }).unwrap();
    assert_eq!(text::text_lines(&reopen(&doc), 0).unwrap()[0].text, replacement);
}

/// Without craft-fonts there is no Japanese face: editing in Japanese is a clear error that
/// leaves the page untouched, never a panic; Latin edits work as before.
#[test]
fn japanese_edit_without_craft_fonts_is_a_clear_error() {
    let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj ET");
    let before = page_content_bytes(&doc, 0);
    let line = text::replace_line(&mut doc, 0, 0, "日本語の文字");
    let block = text::replace_block(&mut doc, 0, 0, "日本語の文字");
    if pdfcraft_fonts::document_japanese_font().is_some() {
        eprintln!("built with craft-fonts: the Japanese edits succeed (checked by the tests above)");
        assert!(line.is_ok() && block.is_ok());
        return;
    }
    for result in [line.map(|_| ()), block.map(|_| ())] {
        let Err(EditError::Invalid(msg)) = result else { panic!("expected a clear error, got {result:?}") };
        assert!(msg.contains("Japanese fallback font") && msg.contains("CRAFT_FONTS_DIR"), "{msg}");
    }
    assert_eq!(page_content_bytes(&doc, 0), before);
    let latin = text::replace_line(&mut doc, 0, 0, "Hello").unwrap();
    assert_eq!(text::text_lines(&reopen(&doc), 0).unwrap()[0].text, "Hello", "{latin:?}");
}
#[test]
fn font_descriptor_style_is_exposed_even_with_a_neutral_name() {
    let doc = descriptor_text_page();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].bold && lines[0].italic, "{:?}", lines[0]);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert!(blocks[0].bold && blocks[0].italic, "{:?}", blocks[0]);
}

#[test]
fn mixed_font_runs_are_not_merged_into_one_source_style() {
    let doc = text_page("BT /F1 12 Tf 72 700 Td (Regular) Tj /F2 12 Tf 150 700 Td (Subset) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 2);
    assert_ne!(lines[0].font, lines[1].font);
}

#[test]
fn missing_glyphs_preserve_source_style_in_fallback() {
    let mut doc = descriptor_text_page();
    let result = text::replace_line(&mut doc, 0, 0, "Styled €").unwrap();
    assert_eq!(result.substituted.as_deref(), Some("Helvetica-BoldOblique"));
    let bytes = page_content_bytes(&doc, 0);
    let content = String::from_utf8_lossy(&bytes);
    assert!(content.contains("/PCEdHelveticaBoldOblique 12 Tf"), "{content}");
}

#[test]
fn a_subset_without_a_space_glyph_keeps_its_font_when_editing_words() {
    for paragraph in [false, true] {
        let mut doc = text_page("BT /F2 12 Tf 72 700 Td (ab) Tj /F1 12 Tf ( ) Tj /F2 12 Tf (ba) Tj ET");
        assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].text, "ab ba");
        let edit = if paragraph { text::replace_block(&mut doc, 0, 0, "ba ab ba") } else { text::replace_line(&mut doc, 0, 0, "ba ab ba") };
        assert_eq!(edit.unwrap().substituted, None);
        let lines = text::text_lines(&reopen(&doc), 0).unwrap();
        assert_eq!(lines[0].text, "ba ab ba");
        assert_eq!(lines[0].font, "F2");
        assert!((lines[0].rect[2] - lines[0].rect[0]) > 3.0 * 12.0);
    }
}

/// #2616: a subset whose ToUnicode has no space (as LaTeX and Sphinx output often has, spacing
/// words with TJ kerning) makes finding the space width read every font on the page. That ran
/// for every character of a multi-row draft, so each keystroke in a long paragraph took half a
/// second. It runs once per font now, however long the paragraph.
#[test]
fn a_long_draft_in_a_font_without_a_space_works_out_the_space_width_once() {
    // Two rows of one paragraph (14 pt apart) in the Chrome-style subset of `skia_subset_page`,
    // whose ToUnicode here has no entry for the space glyph.
    let mut doc =
        text_page("BT /F3 12 Tf 72 700 Td <0177031602E001FB03050255> Tj ET          BT /F3 12 Tf 72 686 Td <008D0266035402FE01FB030503E6> Tj ET");
    let program = doc.add(Object::Stream(Stream::flate(Dict::new(), include_bytes!("../../../../assets/fonts/Inter-Regular.ttf"))));
    let unicode = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"1 begincodespacerange <0000> <FFFF> endcodespacerange 12 beginbfchar <0177> <0050> <0316> <006F> <02E0> <006C>           <01FB> <0061> <0305> <006E> <0255> <0064> <008D> <0047> <0266> <0065> <0354> <0072> <02FE> <006D> <03E6> <0079>           <1000> <007A> endbfchar",
    )));
    let widths: Vec<Object> =
        [(0x8D, 746), (0x177, 639), (0x1FB, 551), (0x255, 612), (0x266, 583), (0x2E0, 242), (0x2FE, 876), (0x305, 591), (0x316, 600)]
            .into_iter()
            .chain([(0x354, 376), (0x3E6, 515)])
            .flat_map(|(cid, width)| [Object::Int(cid), Object::Array(vec![Object::Int(width)])])
            .collect();
    let descriptor = Dict::from_iter([
        (b"Type".to_vec(), Object::name("FontDescriptor")),
        (b"FontName".to_vec(), Object::name("BAAAAA+SampleSans")),
        (b"Flags".to_vec(), Object::Int(32)),
        (b"FontFile2".to_vec(), Object::Ref(program)),
    ]);
    let cid_font = Dict::from_iter([
        (b"Type".to_vec(), Object::name("Font")),
        (b"Subtype".to_vec(), Object::name("CIDFontType2")),
        (b"BaseFont".to_vec(), Object::name("BAAAAA+SampleSans")),
        (b"CIDToGIDMap".to_vec(), Object::name("Identity")),
        (b"DW".to_vec(), Object::Int(562)),
        (b"W".to_vec(), Object::Array(widths)),
        (b"FontDescriptor".to_vec(), Object::Dict(descriptor)),
    ]);
    let font = Dict::from_iter([
        (b"Type".to_vec(), Object::name("Font")),
        (b"Subtype".to_vec(), Object::name("Type0")),
        (b"BaseFont".to_vec(), Object::name("BAAAAA+SampleSans")),
        (b"Encoding".to_vec(), Object::name("Identity-H")),
        (b"DescendantFonts".to_vec(), Object::Array(vec![Object::Dict(cid_font)])),
        (b"ToUnicode".to_vec(), Object::Ref(unicode)),
    ]);
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let fonts = Dict::from_iter([(b"F3".to_vec(), Object::Dict(font))]);
    doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(Dict::from_iter([(b"Font".to_vec(), Object::Dict(fonts))])))).unwrap();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 1, "one paragraph: {:?}", blocks.iter().map(|b| &b.text).collect::<Vec<_>>());
    let draft = "Poland and Germany
"
    .repeat(20);
    text::SOURCE_SPACE_CALLS.with(|c| c.set(0));
    let layout = text::text_edit_layout(&doc, 0, 0, draft.trim_end(), &text::BlockStyle::default()).unwrap();
    let calls = text::SOURCE_SPACE_CALLS.with(std::cell::Cell::get);
    assert!(layout.draft_rect.is_some_and(|r| r[2] > r[0]), "laid out: {:?}", layout.draft_rect);
    assert!(calls <= 8, "the space width was worked out {calls} times for {} characters", draft.len());
}

#[test]
fn a_subset_keeps_its_font_for_glyphs_left_to_the_default_width() {
    let doc = skia_subset_page();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["Poland", "Germany and more"]);
    // Deleting words only, and replacing a word with one drawn elsewhere on the page (`y` and `a`
    // have no /W entry): every glyph is in the subset, so the font stays.
    for (block, text) in [(1, "Germany"), (0, "Germany")] {
        let mut doc = doc.clone();
        assert_eq!(text::replace_block(&mut doc, 0, block, text).unwrap().substituted, None, "{text}");
        let lines = text::text_lines(&reopen(&doc), 0).unwrap();
        assert_eq!(lines.iter().find(|l| l.text == text).unwrap().font, "F3");
    }
    // `z` maps to a CID the program has no glyph for: that still needs another font.
    let mut doc = doc.clone();
    assert!(text::replace_block(&mut doc, 0, 0, "Graz").unwrap().substituted.is_some());
}

#[test]
fn spaces_in_another_subset_do_not_split_a_line() {
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello) Tj /F2 12 Tf ( ) Tj /F1 12 Tf (world) Tj ET");
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "Hello world");
    text::replace_block(&mut doc, 0, 0, "Goodbye").unwrap();
    assert_eq!(text::text_blocks(&reopen(&doc), 0).unwrap()[0].text, "Goodbye");
}

#[test]
fn editor_background_removes_glyphs_but_preserves_following_text_advance() {
    for show in ["(Hello) Tj", "[(He) -20 (llo)] TJ", "(Hello) '", "2 1 (Hello) \""] {
        let mut doc = text_page(&format!("BT /F1 12 Tf 14 TL 72 700 Td {show} [-3000] TJ /F2 12 Tf (ab) Tj ET"));
        let before = text::text_lines(&doc, 0).unwrap();
        assert_eq!(before.len(), 2);
        text::clear_block(&mut doc, 0, 0).unwrap();
        let after = text::text_lines(&reopen(&doc), 0).unwrap();
        assert_eq!(after.len(), 1, "{show}");
        assert_eq!(after[0].text, "ab");
        for (a, b) in after[0].rect.iter().zip(before[1].rect) {
            assert!((a - b).abs() < 1e-6, "unselected text moved: {show}");
        }
    }
}

#[test]
fn editor_background_refuses_clipping_text_without_mutation() {
    let mut doc = text_page("BT /F1 12 Tf 7 Tr 72 700 Td (Clip) Tj ET");
    let before = page_content_bytes(&doc, 0);
    assert!(text::clear_block(&mut doc, 0, 0).is_err());
    assert_eq!(page_content_bytes(&doc, 0), before);
}

#[test]
fn explicit_colour_spaces_expose_the_text_colour() {
    let doc = text_page("/DeviceRGB cs 0 0.23 0.45 sc BT /F1 12 Tf 72 700 Td (Blue) Tj ET");
    assert_eq!(text::text_blocks(&doc, 0).unwrap()[0].color, [0.0, 0.23, 0.45]);
}

#[test]
fn words_in_separate_text_objects_form_one_editable_paragraph() {
    let mut doc = text_page(
        "BT /F1 12 Tf 1 0 0 1 72 700 Tm (Hello) Tj ET \
         BT /F1 12 Tf 1 0 0 1 103 700 Tm (world) Tj ET \
         BT /F1 12 Tf 1 0 0 1 72 685 Tm (Second) Tj ET \
         BT /F1 12 Tf 1 0 0 1 117 685 Tm (line) Tj ET \
         BT /F1 12 Tf 1 0 0 1 300 700 Tm (Other column) Tj ET",
    );
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "Hello world Second line");
    assert_eq!(blocks[0].lines.len(), 2);
    let other = blocks[1].rect;
    text::replace_block(&mut doc, 0, 0, "Replacement").unwrap();
    let blocks = text::text_blocks(&reopen(&doc), 0).unwrap();
    assert_eq!(blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["Replacement", "Other column"]);
    assert_eq!(blocks[1].rect, other);
}

#[test]
fn independent_text_objects_with_different_transforms_stay_separate() {
    let doc = text_page(
        "BT /F1 12 Tf 1 0 0 1 72 700 Tm (Hello) Tj ET \
         BT /F1 12 Tf 0 1 -1 0 103 700 Tm (Vertical) Tj ET",
    );
    assert_eq!(text::text_lines(&doc, 0).unwrap().len(), 2);
}

/// A negative font size or `Tz` runs the glyphs backwards along the text matrix: each word's
/// `ink` box still spans all its glyphs, not only the first.
#[test]
fn word_boxes_span_whole_words_when_glyphs_run_backwards() {
    // "someone@example.com" in Helvetica 11 is 121.814 pt wide.
    for content in [
        "BT /F1 -11 Tf -1 0 0 1 300 742 Tm (Mail the team: someone@example.com) Tj ET",
        "BT /F1 11 Tf -100 Tz 300 742 Td (Mail the team: someone@example.com) Tj ET",
    ] {
        let lines = text::text_lines(&text_page(content), 0).unwrap();
        assert_eq!(lines.len(), 1, "{content}");
        let ink = &lines[0].ink;
        assert_eq!(ink.len(), 4, "{content}: {ink:?}");
        let width = |r: &[f64; 4]| r[2] - r[0];
        assert!(ink.iter().all(|r| width(r) > 5.0), "{content}: {ink:?}");
        let address = ink.iter().map(width).fold(0.0, f64::max);
        assert!((address - 121.814).abs() < 0.01, "{content}: {ink:?}");
        let (x0, x1) = ink.iter().fold((f64::MAX, f64::MIN), |(a, b), r| (a.min(r[0]), b.max(r[2])));
        assert!(x0 >= lines[0].rect[0] - 0.01 && x1 <= lines[0].rect[2] + 0.01, "{content}: {ink:?} {:?}", lines[0].rect);
    }
}

/// PDF writers split one font into several subsets, each a font resource of its own: a word drawn
/// through two of them is still one visual line (#3661).
#[test]
fn one_line_continues_across_two_resources_of_the_same_font() {
    for (content, text) in [
        // The issue's case: two resources for the same Helvetica.
        ("BT /F1 12 Tf 72 700 Td (Hel) Tj /F4 12 Tf (lo) Tj ET", "Hello"),
        // Word's case: two tagged subsets of one font, "abcab" through /F2, /F3, /F2.
        ("BT /F2 12 Tf 72 700 Td (ab) Tj /F3 12 Tf (ca) Tj /F2 12 Tf (b) Tj ET", "abcab"),
    ] {
        let doc = text_page(content);
        let lines = text::text_lines(&doc, 0).unwrap();
        assert_eq!(lines.len(), 1, "{content}: {:?}", lines.iter().map(|l| &l.text).collect::<Vec<_>>());
        assert_eq!(lines[0].text, text, "{content}");
        let blocks = text::text_blocks(&doc, 0).unwrap();
        assert_eq!(blocks.len(), 1, "{content}");
        assert_eq!(blocks[0].text, text, "{content}");
    }
}

/// Two genuinely different fonts still start a new line: the subset tag is all that may differ.
#[test]
fn different_fonts_start_a_new_line() {
    let doc = text_page("BT /F1 12 Tf 72 700 Td (Hel) Tj /F2 12 Tf (lo) Tj ET");
    assert_eq!(text::text_lines(&doc, 0).unwrap().len(), 2);
}

/// A font without `/BaseFont` has no name to compare. Its lines still group by resource, as they
/// did before subset joining: an empty name must not split one paragraph into two blocks.
#[test]
fn lines_in_a_font_without_a_base_font_stay_one_block() {
    let doc = text_page("BT /F5 12 Tf 72 700 Td (First) Tj 0 -15 Td (second) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| l.base_font.is_empty()), "{:?}", lines.iter().map(|l| &l.base_font).collect::<Vec<_>>());
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].text, "First second");
    let reading = text::reading_blocks(&doc, 0).unwrap();
    assert_eq!(reading.len(), 1, "{:?}", reading.iter().map(|b| &b.text).collect::<Vec<_>>());
    assert_eq!(reading[0].text, "First second");
}

/// Two subsets of one font can carry different encodings: here the second names `-` and `l`
/// through `/Differences` (codes 33 and 34), so the same letters have different codes. The visual
/// line still joins and reads in coding order (#3661).
#[test]
fn a_reencoded_subset_of_the_same_font_stays_one_line() {
    let content = "BT /F1 16 Tf 72 700 Td (Two) Tj /F2 16 Tf (!\") Tj /F1 16 Tf (ine heading) Tj ET";
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>",
        &stream("", content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+Helvetica-Bold >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /GHIJKL+Helvetica-Bold /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding /Differences [33 /hyphen /l] >> >>",
    ]);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 1, "{:?}", lines.iter().map(|l| &l.text).collect::<Vec<_>>());
    assert_eq!(lines[0].text, "Two-line heading");
    let reading = text::reading_blocks(&doc, 0).unwrap();
    assert_eq!(reading.len(), 1);
    assert_eq!(reading[0].text, "Two-line heading");
    // Editing the one line replaces every run, including the re-encoded subset's.
    text::replace_line(&mut doc, 0, 0, "Two-line heading done").unwrap();
    let reopened = reopen(&doc);
    let lines = text::text_lines(&reopened, 0).unwrap();
    assert_eq!(lines.len(), 1, "{:?}", lines.iter().map(|l| &l.text).collect::<Vec<_>>());
    assert_eq!(lines[0].text, "Two-line heading done");
}

/// `ABCDE+` is five letters, not the six-letter subset tag of §9.6.4, so it stays part of the
/// name: `ABCDE+Helvetica-Bold` and `Helvetica-Bold` are different fonts and their runs stay apart.
#[test]
fn a_five_letter_prefix_is_not_a_subset_tag() {
    let content = "BT /F1 16 Tf 72 700 Td (Two) Tj /F2 16 Tf (-l) Tj /F1 16 Tf (ine heading) Tj ET";
    let doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>",
        &stream("", content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /ABCDE+Helvetica-Bold >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>",
    ]);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.len(), 3, "{:?}", lines.iter().map(|l| &l.text).collect::<Vec<_>>());
}

#[test]
fn text_lines_are_found_and_replaced_in_place() {
    let mut doc =
        text_page("BT /F1 12 Tf 72 700 Td (Hello) Tj 40 0 Td [(wor) -20 (ld)] TJ 0 -20 Td (Second line) Tj ET BT /F2 10 Tf 72 600 Td (ab) Tj ET");
    let lines = text::text_lines(&doc, 0).unwrap();
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Hello world", "Second line", "ab"]);
    assert!((lines[0].rect[0] - 72.0).abs() < 0.01 && lines[0].rect[1] < 700.0 && lines[0].rect[3] > 700.0, "{:?}", lines[0].rect);
    assert!((lines[0].size - 12.0).abs() < 1e-9 && lines[0].base_font == "Helvetica");
    let second = lines[1].rect;
    let r = text::replace_line(&mut doc, 0, 0, "Goodbye, café").unwrap();
    assert_eq!(r.substituted, None);
    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines[0].text, "Goodbye, café");
    assert_eq!(lines[1].text, "Second line");
    assert_eq!(lines[1].rect, second);
}

#[test]
fn editing_text_keeps_the_tokens_a_stream_shares_with_its_neighbours() {
    let mut doc = split_streams_page();
    assert_eq!(text::text_lines(&doc, 0).unwrap()[0].text, "Target");
    text::replace_line(&mut doc, 0, 0, "Edited").unwrap();
    assert_split_tokens_kept(&reopen(&doc), "Edited");
    // A paragraph rewrite: "Target" and the line after it (split across pieces) are one
    // paragraph, rewritten whole without stray tokens.
    let mut doc = split_streams_page();
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 1);
    text::replace_block(&mut doc, 0, 0, "Rewrapped").unwrap();
    let (joined, ops) = joined_ops(&reopen(&doc));
    assert!(joined.contains("Rewrapped") && !joined.contains("Target") && !joined.contains("wards"), "{joined}");
    assert!(ops.iter().find(|o| o.is("BDC")).is_some_and(|o| o.operands.len() == 2), "{joined}");
}

#[test]
fn a_line_whose_operator_starts_the_next_stream_can_be_edited() {
    // #155: `[(After) -20 (wards)]` ends one piece and its `TJ` starts the next.
    let doc = split_streams_page();
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Target", "Afterwards"]);
    assert!(lines[1].rect[1] < lines[0].rect[1] && lines[1].rect[2] > lines[1].rect[0] + 40.0, "{:?}", lines[1].rect);

    // Retyping it replaces the operator whole, in both pieces it spans.
    let mut doc = split_streams_page();
    text::replace_line(&mut doc, 0, 1, "Later").unwrap();
    let doc = reopen(&doc);
    let (joined, ops) = joined_ops(&doc);
    assert!(!joined.contains("After") && !joined.contains("wards") && joined.contains("Target"), "{joined}");
    assert!(ops.iter().filter(|o| o.is("TJ")).count() == 0, "{joined}");
    assert!(ops.iter().find(|o| o.is("BDC")).is_some_and(|o| o.operands.len() == 2), "{joined}");
    let lines = text::text_lines(&doc, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Target", "Later"]);
    // The pieces stay pieces: the first is untouched, the marked content still closes.
    let pieces = streams(&doc, 0);
    assert_eq!(pieces.len(), 3);
    assert_eq!(pieces[0].trim(), "/P << /MCID 0");
    assert!(pieces[2].contains("EMC") && !pieces[2].contains("TJ"), "{pieces:?}");

    // As a paragraph with the line before it: both lines are rewritten.
    let mut doc = split_streams_page();
    let blocks = text::text_blocks(&doc, 0).unwrap();
    let last = blocks.iter().position(|b| b.text.contains("Afterwards")).unwrap();
    text::replace_block(&mut doc, 0, last, "Rewrapped").unwrap();
    let doc = reopen(&doc);
    let (joined, ops) = joined_ops(&doc);
    assert!(!joined.contains("wards") && joined.contains("Rewrapped"), "{joined}");
    assert!(ops.iter().find(|o| o.is("BDC")).is_some_and(|o| o.operands.len() == 2), "{joined}");
    assert!(text::text_lines(&doc, 0).unwrap().iter().any(|l| l.text == "Rewrapped"));
}

#[test]
fn win_ansi_high_glyphs_use_helvetica_not_the_fallback_font() {
    // š ž Š Ž Œ Ÿ ƒ † ‰ (WinAnsi 0x80–0x9F) are standard-font glyphs (#347): no Type3 fallback.
    let text = "Šumava žaba œuvre Ÿ ƒ † ‰ €";
    let mut doc = text_page("BT /F2 10 Tf 72 600 Td (ab) Tj ET");
    let r = text::replace_line(&mut doc, 0, 0, text).unwrap();
    assert_eq!(r.substituted.as_deref(), Some("Helvetica"));
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert_eq!((lines[0].text.as_str(), lines[0].base_font.as_str()), (text, "Helvetica"));
    let mut doc = text_page("BT /F2 10 Tf 72 600 Td (ab) Tj ET");
    let r = text::replace_block(&mut doc, 0, 0, text).unwrap();
    assert!(r.substituted.as_deref().is_none_or(|s| !s.contains("Type3")), "{:?}", r.substituted);
    let lines = text::text_lines(&reopen(&doc), 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" "), text);
}

#[test]
fn missing_glyphs_substitute_helvetica_and_impossible_text_is_refused() {
    let mut doc = text_page("BT /F2 10 Tf 72 600 Td (ab) Tj ET");
    // "c" has no glyph in the subset: Helvetica takes over for this line.
    let r = text::replace_line(&mut doc, 0, 0, "abc").unwrap();
    assert_eq!(r.substituted.as_deref(), Some("Helvetica"));
    let doc2 = reopen(&doc);
    let lines = text::text_lines(&doc2, 0).unwrap();
    assert_eq!((lines[0].text.as_str(), lines[0].base_font.as_str()), ("abc", "Helvetica"));
    // Neither the standard font nor the craft-fonts Japanese faces have this emoji.
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Hello) Tj ET");
    assert!(text::replace_line(&mut doc, 0, 0, "\u{1f4a9}").is_err());
    assert!(text::replace_line(&mut doc, 0, 5, "x").is_err(), "no such line");
}

/// #4280: two paragraphs in subsets without Ç or Ã, one regular and one bold. Each edit that adds
/// an accent sets the paragraph in a standard font of its own weight. Editing the bold one must
/// not turn the regular one, set in its stand-in earlier, bold too, and the editor's frame for the
/// bold one must not reach the regular one.
#[test]
fn a_bold_stand_in_font_leaves_an_earlier_regular_one_alone() {
    let widths = vec!["600"; 26].join(" ");
    let font = |base: &str| {
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /FirstChar 65 /LastChar 90 /Widths [{widths}] /Encoding /WinAnsiEncoding >>")
    };
    let (regular, bold) = (font("ABCDEF+Garamond"), font("ABCDEF+Garamond-Bold"));
    let content = stream("", "BT /F1 12 Tf 72 700 Td (ACAO) Tj ET BT /F2 12 Tf 300 400 Td (TENSAO) Tj ET");
    let mut doc = build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>",
        &content,
        &regular,
        &bold,
    ]);
    let block = |doc: &Document, text: &str| text::text_blocks(doc, 0).unwrap().iter().position(|b| b.text == text).unwrap();
    let acao = block(&doc, "ACAO");
    assert_eq!(text::replace_block(&mut doc, 0, acao, "AÇÃO").unwrap().substituted.as_deref(), Some("Times-Roman"));
    let first = text::text_lines(&doc, 0).unwrap().into_iter().find(|l| l.text == "AÇÃO").unwrap();

    let tensao = block(&doc, "TENSAO");
    let layout = text::text_edit_layout(&doc, 0, tensao, "TENSÃO", &text::BlockStyle::default()).unwrap();
    let draft = layout.draft_rect.unwrap();
    assert!(draft[3] < first.rect[1] || draft[1] > first.rect[3], "the frame {draft:?} reaches the regular paragraph at {:?}", first.rect);
    assert_eq!(text::replace_block(&mut doc, 0, tensao, "TENSÃO").unwrap().substituted.as_deref(), Some("Times-Bold"));

    let doc = reopen(&doc);
    let lines = text::text_lines(&doc, 0).unwrap();
    let font_of = |text: &str| lines.iter().find(|l| l.text == text).map(|l| l.base_font.clone());
    assert_eq!(font_of("AÇÃO").as_deref(), Some("Times-Roman"), "{lines:?}");
    assert_eq!(font_of("TENSÃO").as_deref(), Some("Times-Bold"), "{lines:?}");
}

/// #2517: one blank line at ordinary leading (2 × 1.2 × the font size) is a paragraph break.
/// Editing the first paragraph must leave the second one in place.
#[test]
fn a_blank_line_keeps_the_next_paragraph() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (First paragraph, first row.) Tj T* (First paragraph, second row.) Tj ET \
               BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Second paragraph, first row.) Tj T* (Second paragraph, second row.) Tj ET";
    let mut doc = text_page(src);
    let before = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(before.len(), 2, "{:?}", before.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
    text::replace_block(&mut doc, 0, 0, "First paragraph, edited row.\nFirst paragraph, second row.").unwrap();
    let after = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(after.len(), 2, "{:?}", after.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
    assert!(after[0].text.contains("edited row") && after[0].text.contains("second row"), "{:?}", after[0].text);
    assert!(after[1].text.contains("Second paragraph"), "{:?}", after[1].text);
    // The second row is left in the source stream. A marker around only the rewritten row
    // would split the paragraph; either there is no marker, or it covers both rows and not the next paragraph.
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    if content.contains("/P BMC") {
        let marked = region_through_matching_close(&content, "/P BMC");
        assert!(marked.contains("edited row") && marked.contains("second row"), "marker split the paragraph: {content}");
        assert!(!marked.contains("Second paragraph"), "marker swallowed the next paragraph: {content}");
    }
}

/// #2517: adding a row must not pull in a paragraph that was 3 × the font size below.
#[test]
fn a_grown_paragraph_does_not_swallow_the_next_one() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (One) Tj T* (Two) Tj ET \
               BT /F1 12 Tf 1 0 0 1 72 649.6 Tm (Neighbour) Tj ET";
    let mut doc = text_page(src);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    text::replace_block(&mut doc, 0, 0, "One\nTwo\nThree").unwrap();
    let after = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(after.len(), 2, "{:?}", after.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
    assert!(after[1].text.contains("Neighbour"), "{:?}", after[1].text);
}

/// An explicit `TL` is the leading `T*` moves by. Rows at that gap are one paragraph, including
/// when the gap is larger than twice the font size. A following paragraph that starts two
/// leadings lower stays separate. The numbers are the gaps under test, not a claim about how a
/// word processor derives them: this repo's Helvetica box is 0.925 em (ascent 0.718, descent
/// 0.207), so twice that box at 12 pt is 22.2 pt, while 27.6 pt is 12 × 2.3.
#[test]
fn explicit_leading_is_the_measured_baseline_gap() {
    for tl in [14.4_f64, 18.0, 22.2, 24.0, 27.6, 28.8] {
        let src = format!("BT /F1 12 Tf {tl} TL 1 0 0 1 72 700 Tm (One) Tj T* (Two) Tj T* (Three) Tj ET");
        let doc = text_page(&src);
        let lines = text::text_lines(&doc, 0).unwrap();
        let gaps: Vec<f64> = lines.windows(2).map(|w| w[0].origin_baseline() - w[1].origin_baseline()).collect();
        assert!(gaps.iter().all(|gap| (gap - tl).abs() < 0.05), "TL {tl} did not move the baselines: {gaps:?}");
        let blocks = text::text_blocks(&doc, 0).unwrap();
        assert_eq!(blocks.len(), 1, "TL {tl}, measured gaps {gaps:?}: {:?}", blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
    }
    // Two rows at the TL, then a paragraph whose first baseline is two TLs below the second row.
    let tl = 14.4_f64;
    let next = 700.0 - tl - 2.0 * tl;
    let blank = format!(
        "BT /F1 12 Tf {tl} TL 1 0 0 1 72 700 Tm (One) Tj T* (Two) Tj ET \
         BT /F1 12 Tf {tl} TL 1 0 0 1 72 {next} Tm (Neighbour) Tj ET"
    );
    let doc = text_page(&blank);
    let lines = text::text_lines(&doc, 0).unwrap();
    let between = lines[1].origin_baseline() - lines[2].origin_baseline();
    assert!((between - 2.0 * tl).abs() < 0.05, "the blank line measured {between}, not {}", 2.0 * tl);
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 2, "{:?}", blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
}

/// The neighbour starts one blank line below (two leadings under the second row). Adding a row
/// puts the new baseline one leading above that neighbour, inside the join window. The neighbour
/// still has to survive the edit, both saves, and a second edit.
#[test]
fn growing_into_a_blank_line_keeps_the_neighbour_after_save() {
    let tl = 14.4_f64;
    let src = format!(
        "BT /F1 12 Tf {tl} TL 1 0 0 1 72 700 Tm (One) Tj T* (Two) Tj ET \
         BT /F1 12 Tf {tl} TL 1 0 0 1 72 {} Tm (Neighbour) Tj ET",
        700.0 - 3.0 * tl
    );
    let mut doc = text_page(&src);
    let before = text::text_lines(&doc, 0).unwrap();
    let opening = before[1].origin_baseline() - before[2].origin_baseline();
    assert!((opening - 2.0 * tl).abs() < 0.05, "fixture gap {opening} is not a blank line");
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    text::replace_block(&mut doc, 0, 0, "One\nTwo\nThree").unwrap();
    let lines = text::text_lines(&doc, 0).unwrap();
    let bases: Vec<f64> = lines.iter().map(|l| l.origin_baseline()).collect();
    assert_eq!(bases.len(), 4, "{bases:?}");
    let grown = bases[2] - bases[3];
    assert!((grown - tl).abs() < 0.05, "the new row is {grown} pt above the neighbour, not one leading");
    assert!(grown < tl * 1.5, "this gap is outside the join window, so the test would pass without the marker");
    let texts = |doc: &Document| text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
    assert_eq!(texts(&doc), ["One Two Three", "Neighbour"]);
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    let marked = region_through_matching_close(&content, "/P BMC");
    for row in ["(One)", "(Two)", "(Three)"] {
        assert!(marked.contains(row), "{row} is outside the marker: {content}");
    }
    assert!(!marked.contains("Neighbour"), "the neighbour was pulled into the marker: {content}");
    for saved in [reopen(&doc), Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap()] {
        assert_eq!(texts(&saved), ["One Two Three", "Neighbour"]);
        let mut again = saved;
        text::replace_block(&mut again, 0, 0, "One\nTwo\nFour").unwrap();
        assert_eq!(texts(&reopen(&again)), ["One Two Four", "Neighbour"]);
    }
}

/// No `TL`: `Td` does not set the leading (`TD` does). Three rows at one repeated step are one
/// paragraph. A single large gap, with no third row at that step, stays a break.
#[test]
fn a_repeated_step_without_tl_is_the_paragraph_leading() {
    let step = 27.6_f64;
    let bodies = [
        format!("BT /F1 12 Tf 1 0 0 1 72 700 Tm (One) Tj 0 -{step} Td (Two) Tj 0 -{step} Td (Three) Tj ET"),
        format!(
            "BT /F1 12 Tf 1 0 0 1 72 700 Tm (One) Tj ET BT /F1 12 Tf 1 0 0 1 72 {} Tm (Two) Tj ET BT /F1 12 Tf 1 0 0 1 72 {} Tm (Three) Tj ET",
            700.0 - step,
            700.0 - 2.0 * step
        ),
        format!(
            "BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {} Td (Two) Tj ET BT /F1 12 Tf 72 {} Td (Three) Tj ET",
            700.0 - step,
            700.0 - 2.0 * step
        ),
    ];
    for body in bodies {
        let doc = text_page(&body);
        let lines = text::text_lines(&doc, 0).unwrap();
        let gaps: Vec<f64> = lines.windows(2).map(|w| w[0].origin_baseline() - w[1].origin_baseline()).collect();
        assert!(gaps.iter().all(|gap| (gap - step).abs() < 0.05), "step {step} measured {gaps:?} in {body}");
        let blocks = text::text_blocks(&doc, 0).unwrap();
        assert_eq!(blocks.len(), 1, "{body} -> {:?}", blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
    }
    // Two rows and nothing after them: one gap cannot be told from a paragraph break.
    let pair = format!("BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {} Td (Two) Tj ET", 700.0 - step);
    assert_eq!(text::text_blocks(&text_page(&pair), 0).unwrap().len(), 2);
    // The original blank line, written with Td instead of TL: the inner step repeats, the break does not.
    let inner = 14.4_f64;
    let blank = format!(
        "BT /F1 12 Tf 72 700 Td (One) Tj ET BT /F1 12 Tf 72 {} Td (Two) Tj ET \
         BT /F1 12 Tf 72 {} Td (Neighbour) Tj ET BT /F1 12 Tf 72 {} Td (Line) Tj ET",
        700.0 - inner,
        700.0 - 3.0 * inner,
        700.0 - 4.0 * inner
    );
    let doc = text_page(&blank);
    let lines = text::text_lines(&doc, 0).unwrap();
    let break_gap = lines[1].origin_baseline() - lines[2].origin_baseline();
    assert!((break_gap - 2.0 * inner).abs() < 0.05, "blank line measured {break_gap}");
    let blocks = text::text_blocks(&doc, 0).unwrap();
    assert_eq!(blocks.len(), 2, "{:?}", blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>());
}

/// The first row is unchanged, so the rewrite leaves it in the source stream and skips the
/// paragraph marker. The added row then sits one leading above the neighbour.
#[test]
fn a_partial_rewrite_that_adds_a_row_keeps_the_neighbour() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
               BT /F1 12 Tf 14.4 TL 1 0 0 1 72 656.8 Tm (Neighbor stays.) Tj ET";
    let mut doc = text_page(src);
    assert_eq!(text::text_blocks(&doc, 0).unwrap().len(), 2);
    text::replace_block(&mut doc, 0, 0, "Alpha first row.\nTwo\nThree").unwrap();
    let texts = |doc: &Document| text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert_eq!(texts(&doc), ["Alpha first row. Two Three", "Neighbor stays."], "{content}");
    for saved in [reopen(&doc), Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap()] {
        assert_eq!(texts(&saved), ["Alpha first row. Two Three", "Neighbor stays."]);
        let mut again = saved;
        text::replace_block(&mut again, 0, 0, "Alpha first row.\nTwo\nFour").unwrap();
        assert_eq!(texts(&reopen(&again)), ["Alpha first row. Two Four", "Neighbor stays."]);
    }
}

/// Both paragraphs share one text object, with a blank line between them. The paragraph
/// marker has to close before the neighbour, not after the shared `ET`.
#[test]
fn a_partial_rewrite_inside_one_text_object_keeps_the_neighbour() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj T* T* (Neighbor stays.) Tj ET";
    let mut doc = text_page(src);
    let texts = |doc: &Document| text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
    assert_eq!(texts(&doc), ["Alpha first row. Alpha next row.", "Neighbor stays."], "the blank line is a paragraph break");
    text::replace_block(&mut doc, 0, 0, "Alpha first row.\nTwo\nThree").unwrap();
    let content = String::from_utf8_lossy(&page_content_bytes(&doc, 0)).into_owned();
    assert_eq!(texts(&doc), ["Alpha first row. Two Three", "Neighbor stays."], "{content}");
    for saved in [reopen(&doc), Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap()] {
        assert_eq!(texts(&saved), ["Alpha first row. Two Three", "Neighbor stays."], "save joined the neighbour");
        let mut again = saved;
        text::replace_block(&mut again, 0, 0, "Alpha first row.\nTwo\nFour").unwrap();
        assert_eq!(texts(&reopen(&again)), ["Alpha first row. Two Four", "Neighbor stays."]);
    }
}

/// The last paragraph of a shared text object has no neighbour after it. The marker still has
/// to open after the preceding paragraph, not at the shared `BT`, or that paragraph joins it.
#[test]
fn a_partial_rewrite_of_the_last_paragraph_in_one_text_object_keeps_the_one_before_it() {
    let src =
        "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj T* T* (Neighbor stays.) Tj T* (Neighbor next.) Tj ET";
    let mut doc = text_page(src);
    let texts = |doc: &Document| text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
    assert_eq!(texts(&doc), ["Alpha first row. Alpha next row.", "Neighbor stays. Neighbor next."], "the blank line is a paragraph break");
    text::replace_block(&mut doc, 0, 1, "Neighbor stays.\nChanged.").unwrap();
    let content = page_content(&doc);
    assert_eq!(texts(&doc), ["Alpha first row. Alpha next row.", "Neighbor stays. Changed."], "{content}");
    let alpha = content.find("(Alpha first row.)").expect("the preceding paragraph");
    let open = content.find("/P BMC").expect("marker");
    assert!(alpha < open, "the marker swallowed the preceding paragraph: {content}");
    let neighbour = content.find("(Neighbor stays.)").expect("the kept row");
    assert!(open < neighbour, "the kept row is outside the marker: {content}");
    for saved in [reopen(&doc), Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap()] {
        assert_eq!(texts(&saved), ["Alpha first row. Alpha next row.", "Neighbor stays. Changed."], "save joined the preceding paragraph");
        let mut again = saved;
        text::replace_block(&mut again, 0, 1, "Neighbor stays.\nEdited.").unwrap();
        assert_eq!(texts(&reopen(&again)), ["Alpha first row. Alpha next row.", "Neighbor stays. Edited."]);
    }
}

/// Three paragraphs share one text object. Rewriting the middle one must leave the paragraph
/// before it and the paragraph after it outside the marker.
#[test]
fn a_partial_rewrite_of_the_middle_paragraph_in_one_text_object_keeps_both_neighbours() {
    let src = "BT /F1 12 Tf 14.4 TL 1 0 0 1 72 700 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj \
               T* T* (Mid stays.) Tj T* (Mid next.) Tj T* T* (End stays.) Tj T* (End next.) Tj ET";
    let mut doc = text_page(src);
    let texts = |doc: &Document| text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect::<Vec<_>>();
    assert_eq!(
        texts(&doc),
        ["Alpha first row. Alpha next row.", "Mid stays. Mid next.", "End stays. End next."],
        "each blank line is a paragraph break"
    );
    text::replace_block(&mut doc, 0, 1, "Mid stays.\nChanged.").unwrap();
    let content = page_content(&doc);
    assert_eq!(texts(&doc), ["Alpha first row. Alpha next row.", "Mid stays. Changed.", "End stays. End next."], "{content}");
    let alpha = content.find("(Alpha first row.)").expect("the preceding paragraph");
    let open = content.find("/P BMC").expect("marker");
    let end = content.find("(End stays.)").expect("the following paragraph");
    assert!(alpha < open, "the marker swallowed the preceding paragraph: {content}");
    assert!(open < end, "the following paragraph is inside the marker: {content}");
    let close = content[open..].find("EMC").map(|at| open + at).expect("close");
    assert!(close < end, "the marker closed after the following paragraph: {content}");
    for saved in [reopen(&doc), Document::open(Arc::new(write_full(&doc, &SaveOptions::default()).unwrap())).unwrap()] {
        assert_eq!(texts(&saved), ["Alpha first row. Alpha next row.", "Mid stays. Changed.", "End stays. End next."]);
        let mut again = saved;
        text::replace_block(&mut again, 0, 1, "Mid stays.\nEdited.").unwrap();
        assert_eq!(texts(&reopen(&again)), ["Alpha first row. Alpha next row.", "Mid stays. Edited.", "End stays. End next."]);
    }
}
