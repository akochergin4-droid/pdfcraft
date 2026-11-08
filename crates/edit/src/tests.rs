use std::sync::Arc;

use pdfcraft_cos::{Document, SaveOptions, write_full, write_incremental};

use super::*;

// The tests are split by position, not by topic, so that no file runs to ten thousand lines.
// Shared fixtures and helpers stay in this file; each part reaches them through `use super::*`.
mod tests_part1;
mod tests_part2;
mod tests_part3;
mod tests_part4;
mod tests_part5;
mod tests_part6;
mod tests_part7;

#[path = "mark_transaction_tests.rs"]
mod mark_transactions;

/// Three pages: an upright page with content that leaves the graphics state changed (an
/// unbalanced `cm`), a page rotated 90° and one with inherited, shared resources.
fn fixture() -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 600 800] /Resources 6 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R /Rotate 90 /Contents [7 0 R] >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
        "<< /Font << /F1 8 0 R >> >>".into(),
        "<< /Length 37 >>\nstream\n2 0 0 2 0 0 cm BT /F1 9 Tf (Hi) Tj ET\nendstream".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn reopen(doc: &Document) -> Document {
    let bytes = write_incremental(doc, &SaveOptions::default()).unwrap();
    hayro_syntax::Pdf::new(bytes.clone()).expect("parses");
    Document::open(Arc::new(bytes)).unwrap()
}

/// The decoded content streams of a page, in order.
fn streams(doc: &Document, page: usize) -> Vec<String> {
    let p = &pdfcraft_model::pages(doc)[page];
    let c = p.dict.get(b"Contents").cloned();
    let list = match c.map(|c| doc.resolve(&c)).as_deref() {
        Some(Object::Array(a)) => a.clone(),
        Some(_) => vec![p.dict.get(b"Contents").cloned().unwrap()],
        None => vec![],
    };
    list.iter().map(|o| String::from_utf8_lossy(&stream_bytes(doc, o).unwrap()).into_owned()).collect()
}

fn cx() -> Context {
    Context { date: (2026, 10, 1) }
}

/// The six numbers of the first `… cm` in `content`.
fn first_cm(content: &str) -> [f64; 6] {
    let line = content.lines().find(|l| l.ends_with(" cm")).unwrap_or_else(|| panic!("no cm in {content:?}"));
    let v: Vec<f64> = line.split_whitespace().take(6).map(|x| x.parse().unwrap()).collect();
    <[f64; 6]>::try_from(v).unwrap()
}

fn map_point(m: [f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

fn shrinking(header: &str, footer: &str) -> HeaderFooter {
    let mut hf = HeaderFooter { shrink: true, ..HeaderFooter::default() };
    hf.text[1] = header.into();
    hf.text[5] = footer.into();
    hf
}

fn tagged_dictionary(tag: &str, added: Option<Object>) -> Dict {
    let mut dict = Dict::new();
    dict.set(b"PCMark".to_vec(), Object::name(tag));
    if let Some(added) = added {
        dict.set(b"PCAdded".to_vec(), added);
    }
    dict
}

fn tagged_stream(doc: &mut Document, tag: &str, added: Option<Object>) -> Object {
    Object::Ref(doc.add(Object::Stream(Stream::from_raw(tagged_dictionary(tag, added), Vec::new()))))
}

fn added_text_params() -> Dict {
    let mut params = Dict::new();
    params.set(b"Rect".to_vec(), Object::Array([10, 20, 110, 40].into_iter().map(Object::Int).collect()));
    params.set(b"Kind".to_vec(), Object::name("Text"));
    params.set(b"Text".to_vec(), Object::String(pdfcraft_cos::PdfString::text("Added text")));
    params.set(b"Size".to_vec(), Object::Int(12));
    params
}

fn added_image_params(image: pdfcraft_cos::ObjRef) -> Dict {
    let mut params = Dict::new();
    params.set(b"Rect".to_vec(), Object::Array([10, 20, 110, 70].into_iter().map(Object::Int).collect()));
    params.set(b"Kind".to_vec(), Object::name("Image"));
    params.set(b"Image".to_vec(), Object::Ref(image));
    params
}

fn set_page_contents(doc: &mut Document, contents: Object, user_unit: Option<Object>) {
    let page = pdfcraft_model::pages(doc)[0].obj;
    doc.update_dict(page, |dict| {
        dict.set(b"Contents".to_vec(), contents);
        if let Some(user_unit) = user_unit {
            dict.set(b"UserUnit".to_vec(), user_unit);
        }
    })
    .unwrap();
}

fn assert_marker_scan_matches_legacy(doc: &Document, label: &str, marks: &[MarkKind], added: usize) -> MarkerScan {
    let expected_marks = marks_present(doc);
    let expected_added = list_added(doc);
    let scan = scan_markers(doc);
    assert_eq!(scan.marks, expected_marks, "{label}: marks differ from the independent scan");
    assert_eq!(scan.added, expected_added, "{label}: added content differs from the independent scan");
    assert_eq!(scan.marks, marks, "{label}: fixture's mark order or deduplication is wrong");
    assert_eq!(scan.added.len(), added, "{label}: fixture's added-item count is wrong");
    scan
}

/// The `PCAr` font names in page `page`'s resources.
fn arabic_font_names(doc: &Document, page: usize) -> Vec<String> {
    let p = &pdfcraft_model::pages(doc)[page];
    let Some(res) = p.dict.get(b"Resources").map(|r| doc.resolve(r)) else { return vec![] };
    let Some(fonts) = res.as_dict().and_then(|r| r.get(b"Font")).map(|f| doc.resolve(f)) else { return vec![] };
    fonts.as_dict().unwrap().iter().map(|(k, _)| String::from_utf8_lossy(k).into_owned()).filter(|k| k.starts_with("PCAr")).collect()
}

/// The `PCAr` fonts the stream of added item `index` on page 0 shows text with.
fn own(doc: &Document, index: usize) -> Vec<String> {
    let s = String::from_utf8_lossy(&stream_bytes(doc, &Object::Ref(list_added(doc)[index].obj)).unwrap()).into_owned();
    let mut names: Vec<String> =
        s.split_whitespace().filter_map(|w| w.strip_prefix('/')).filter(|w| w.starts_with("PCAr")).map(String::from).collect();
    names.dedup();
    names
}

/// One page with Helvetica (WinAnsi) and subsets of Arial: `/F2` and `/F3` are two subsets of the
/// same font, as Word writes a font split across several resources, and `/F4` repeats Helvetica.
fn text_page(content: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R /F4 8 0 R /F5 9 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        // Subset: glyphs for a (97) and b (98) only.
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+Arial /FirstChar 97 /LastChar 99 /Widths [500 520 0] /Encoding /WinAnsiEncoding >>"
            .into(),
        // Another subset of the same Arial, with a subset tag of its own.
        "<< /Type /Font /Subtype /TrueType /BaseFont /GHIJKL+Arial /FirstChar 97 /LastChar 99 /Widths [500 520 0] /Encoding /WinAnsiEncoding >>"
            .into(),
        // Helvetica again, in a second resource.
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        // No `/BaseFont`: `Metrics::from_dict` leaves its base-font name empty.
        "<< /Type /Font /Subtype /Type1 /Encoding /WinAnsiEncoding >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn descriptor_text_page() -> Document {
    let content = "BT /F1 12 Tf 72 700 Td (ab) Tj ET";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+NeutralFace /FirstChar 97 /LastChar 98 /Widths [500 500] /FontDescriptor 6 0 R /Encoding /WinAnsiEncoding >>".into(),
        "<< /Type /FontDescriptor /FontName /NeutralFace /Flags 262208 /ItalicAngle -12 /FontWeight 700 /Ascent 900 /Descent -250 >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// The Japanese fallback face comes from craft-fonts, an optional build input.
fn without_craft_fonts(test: &str) -> bool {
    if pdfcraft_fonts::document_japanese_font().is_some() {
        return false;
    }
    eprintln!("skipping {test}: built without craft-fonts (set CRAFT_FONTS_DIR to run it)");
    true
}

fn styled_text_page(base: &str) -> Document {
    let mut doc = text_page("BT /F1 12 Tf 72 700 Td (Original) Tj ET");
    let r = pdfcraft_cos::ObjRef::new(5, 0);
    let mut font = doc.get(r).as_dict().unwrap().clone();
    font.set(b"BaseFont".to_vec(), pdfcraft_cos::Object::name(base));
    doc.set(r, pdfcraft_cos::Object::Dict(font));
    doc
}

fn fallback_paths(doc: &Document) -> Vec<Vec<u8>> {
    let p = pdfcraft_model::pages(doc).swap_remove(0);
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap());
    let font = doc.resolve(fonts.as_dict().unwrap().get(b"PCJp").unwrap());
    let procs = doc.resolve(font.as_dict().unwrap().get(b"CharProcs").unwrap());
    procs
        .as_dict()
        .unwrap()
        .iter()
        .map(|(_, object)| match &*doc.resolve(object) {
            pdfcraft_cos::Object::Stream(s) => s.decoded().unwrap(),
            other => panic!("expected a glyph stream, got {other:?}"),
        })
        .collect()
}

/// The first `n` Japanese characters, in code point order, that every craft-fonts face can draw, so
/// whichever face an edit picks has them all. Without craft-fonts there are no faces, every character
/// passes, and the text is still long.
fn covered_japanese_chars(n: usize) -> String {
    let faces = pdfcraft_fonts::document_japanese_fonts_for_style(false, false);
    (0x4E00..0xA000)
        .filter_map(char::from_u32)
        .filter(|ch| faces.iter().all(|face| pdfcraft_fonts::japanese_glyph_from(face, *ch).is_ok()))
        .take(n)
        .collect()
}

/// The page's Type 3 fallback fonts (`PCJp`, `PCJp1`, ...) by resource name.
fn fallback_fonts(doc: &Document) -> Vec<(Vec<u8>, Dict)> {
    let p = pdfcraft_model::pages(doc).swap_remove(0);
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap());
    fonts
        .as_dict()
        .unwrap()
        .iter()
        .filter(|(name, _)| name.starts_with(b"PCJp"))
        .map(|(name, font)| (name.clone(), doc.resolve(font).as_dict().unwrap().clone()))
        .collect()
}

/// The `/Widths` of the page's Type 3 fallback font, for its Cyrillic letters.
fn fallback_widths(doc: &Document) -> Vec<f64> {
    let p = pdfcraft_model::pages(doc).swap_remove(0);
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let fonts = doc.resolve(res.as_dict().unwrap().get(b"Font").unwrap());
    let font = doc.resolve(fonts.as_dict().unwrap().get(b"PCJp").unwrap());
    let font = font.as_dict().unwrap();
    let widths = doc.resolve(font.get(b"Widths").unwrap()).as_array().unwrap().iter().filter_map(|w| w.as_f64()).collect::<Vec<_>>();
    // "София 2027": the first five codes are the letters.
    widths[..5].to_vec()
}

/// A Chrome (Skia) style subset: Type0/Identity-H over a TrueType program, CID = glyph id.
/// `/W` lists most used glyphs, writes 0 for some it dropped, and leaves `a` and `y` to `/DW`;
/// `z` has a ToUnicode entry but no glyph in the program (its CID is past the last glyph).
fn skia_subset_page() -> Document {
    // Inter's glyph ids: P 375, o 790, l 736, a 507, n 773, d 597, G 141, e 614, r 852, m 766, y 998, space 1777.
    let mut doc = text_page(
        "BT /F3 12 Tf 72 700 Td <0177031602E001FB03050255> Tj ET \
         BT /F3 12 Tf 72 650 Td <008D0266035402FE01FB030503E606F101FB0305025506F102FE031603540266> Tj ET",
    );
    let program = doc.add(Object::Stream(Stream::flate(Dict::new(), include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))));
    let unicode = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"1 begincodespacerange <0000> <FFFF> endcodespacerange 13 beginbfchar <0177> <0050> <0316> <006F> <02E0> <006C> \
          <01FB> <0061> <0305> <006E> <0255> <0064> <008D> <0047> <0266> <0065> <0354> <0072> <02FE> <006D> <03E6> <0079> \
          <06F1> <0020> <1000> <007A> endbfchar",
    )));
    let widths: Vec<Object> = [(0x8D, 746), (0x177, 639), (0x255, 612), (0x266, 583), (0x2E0, 242), (0x2FE, 876), (0x305, 591), (0x316, 600)]
        .into_iter()
        .chain([(0x354, 376), (0x355, 0), (0x356, 0), (0x6F1, 281)])
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
    doc
}

/// A page whose content is one stream in three pieces, split between tokens: a marked-content
/// dictionary closes at the start of the middle piece, and the `TJ` array that ends it is shown
/// by the operator that starts the last one.
fn split_streams_page() -> Document {
    let piece = |s: &str| format!("<< /Length {} >>\nstream\n{s}\nendstream", s.len());
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents [4 0 R 5 0 R 6 0 R] /Resources << /Font << /F1 7 0 R >> >> >>".into(),
        piece("/P << /MCID 0"),
        piece(">> BDC BT /F1 12 Tf 72 700 Td (Target) Tj 0 -20 Td [(After) -20 (wards)]"),
        piece("TJ ET EMC"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// The page's pieces joined back into one stream: every operator must still have its operands.
fn assert_split_tokens_kept(doc: &Document, new_text: &str) {
    let joined = streams(doc, 0).join("\n");
    assert!(joined.contains(new_text) && !joined.contains("Target"), "{joined}");
    assert!(joined.contains("[(After) -20 (wards)]"), "the text after the line is gone: {joined}");
    for op in pdfcraft_content::parse(joined.as_bytes()).ops {
        let want = match op.op.as_slice() {
            b"BDC" => 2,
            b"TJ" => 1,
            _ => continue,
        };
        assert_eq!(op.operands.len(), want, "{} lost its operands: {joined}", String::from_utf8_lossy(&op.op));
    }
}

/// The operators of the page's pieces joined back into one stream, each checked for operands.
fn joined_ops(doc: &Document) -> (String, Vec<pdfcraft_content::Op>) {
    let joined = streams(doc, 0).join("\n");
    let ops = pdfcraft_content::parse(joined.as_bytes());
    assert_eq!(ops.skipped, 0, "stray tokens: {joined}");
    (joined, ops.ops)
}

/// Bytes from `open` through the `EMC` that balances the marked region it begins.
fn region_through_matching_close<'a>(content: &'a str, open: &str) -> &'a str {
    let start = content.find(open).unwrap_or_else(|| panic!("missing {open}: {content}"));
    let bytes = content.as_bytes();
    let mut index = start;
    let mut depth = 0i32;
    while index < bytes.len() {
        let rest = &bytes[index..];
        let boundary = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        if boundary && (rest.starts_with(b"BMC") || rest.starts_with(b"BDC")) && rest.get(3).is_none_or(|b| !b.is_ascii_alphanumeric()) {
            depth += 1;
            index += 3;
            continue;
        }
        if boundary && rest.starts_with(b"EMC") && rest.get(3).is_none_or(|b| !b.is_ascii_alphanumeric()) {
            depth -= 1;
            if depth == 0 {
                return &content[start..index];
            }
            index += 3;
            continue;
        }
        index += 1;
    }
    panic!("unbalanced region {open}: {content}");
}

fn page_content(doc: &Document) -> String {
    String::from_utf8_lossy(&page_content_bytes(doc, 0)).into_owned()
}

/// `open` through the next `EMC` contains both strings, and no `EMC` sits inside a text object.
fn assert_marker_wraps(content: &str, open: &str, first: &str, second: &str) {
    let marked = region_through_matching_close(content, open);
    assert!(marked.contains(first) && marked.contains(second), "{open} closed too early: {content}");
    assert!(!marker_crosses_a_text_object(content), "a marker opened outside a text object and closed inside it: {content}");
}

/// A marker may sit inside one text object. It may not open outside one and close inside it.
fn marker_crosses_a_text_object(content: &str) -> bool {
    let mut text = 0i32;
    let mut opened_outside = 0i32;
    let mut opened_inside = 0i32;
    for token in content.split_whitespace() {
        match token {
            "BT" => text += 1,
            "ET" => text -= 1,
            "BMC" | "BDC" => {
                if text == 0 {
                    opened_outside += 1;
                } else {
                    opened_inside += 1;
                }
            }
            "EMC" => {
                if text > 0 {
                    if opened_inside > 0 {
                        opened_inside -= 1;
                    } else {
                        return true;
                    }
                } else if opened_outside > 0 {
                    opened_outside -= 1;
                }
            }
            _ => {}
        }
    }
    text != 0 || opened_outside != 0 || opened_inside != 0
}

/// The letter the screenshots edit. Widening example 1, or adding a row there, must leave the
/// heading above it and both paragraphs under example 2 as they were.
fn letter_page() -> &'static str {
    "BT /F1 14 Tf 72 740 Td (1. Repeated step, no TL. Click Two.) Tj ET \
     BT /F1 12 Tf 72 700 Td (One) Tj ET \
     BT /F1 12 Tf 72 672.4 Td (Two) Tj ET \
     BT /F1 12 Tf 72 644.8 Td (Three) Tj ET \
     BT /F1 14 Tf 72 590 Td (2. Blank line. Edit the first paragraph.) Tj ET \
     BT /F1 12 Tf 14.4 TL 1 0 0 1 72 550 Tm (First paragraph, first row.) Tj T* (First paragraph, second row.) Tj ET \
     BT /F1 12 Tf 14.4 TL 1 0 0 1 72 506.8 Tm (Second paragraph, first row.) Tj T* (Second paragraph, second row.) Tj ET \
     BT /F1 14 Tf 72 450 Td (3. Add a row under Alpha. Neighbor stays.) Tj ET \
     BT /F1 12 Tf 14.4 TL 1 0 0 1 72 410 Tm (Alpha first row.) Tj T* (Alpha next row.) Tj ET \
     BT /F1 12 Tf 14.4 TL 1 0 0 1 72 366.8 Tm (Neighbor stays.) Tj ET"
}

fn block_texts(doc: &Document) -> Vec<String> {
    text::text_blocks(doc, 0).unwrap().into_iter().map(|b| b.text).collect()
}

fn page_content_bytes(doc: &Document, page: usize) -> Vec<u8> {
    let p = pdfcraft_model::pages(doc).swap_remove(page);
    let c = p.dict.get(b"Contents").unwrap();
    match &*doc.resolve(c) {
        pdfcraft_cos::Object::Stream(s) => s.decoded().unwrap(),
        pdfcraft_cos::Object::Array(a) => a
            .iter()
            .flat_map(|x| match &*doc.resolve(x) {
                pdfcraft_cos::Object::Stream(s) => s.decoded().unwrap(),
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A page drawing image XObject /Im0 at 100,100 size 200 × 100 (pixels 4 × 2), on a real optional
/// content layer: the catalog's `/OCProperties` declares object 7 as the only OCG and the page
/// resources' `/Properties` maps `/L0` to it, so `/OC /L0 BDC` marks a valid optional-content
/// member rather than a dangling name.
fn image_page() -> Document {
    image_page_with("q 200 0 0 100 100 100 cm /Im0 Do Q BT /F1 12 Tf 72 700 Td (Caption) Tj ET")
}

/// Arrange, Crop and Remove crop as the engine asks for them, through [`images::change_image`].
fn arrange(doc: &mut Document, page: usize, index: usize, order: images::ArrangeOrder) -> Result<(), EditError> {
    images::change_image(doc, page, index, &images::ImageChange::Arrange(order))
}

fn crop(doc: &mut Document, page: usize, index: usize, keep: [f64; 4]) -> Result<(), EditError> {
    images::change_image(doc, page, index, &images::ImageChange::Crop(keep))
}

fn remove_crop(doc: &mut Document, page: usize, index: usize) -> Result<(), EditError> {
    images::change_image(doc, page, index, &images::ImageChange::RemoveCrop)
}

fn image_page_with(content: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [7 0 R] /D << /Order [7 0 R] >> >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R >> /ExtGState << /GS0 << /Type /ExtGState /ca 0.5 /CA 0.5 >> /GSalpha << /Type /ExtGState /ca 0.6 >> /GSblend << /Type /ExtGState /BM /Multiply >> /GS_translucent << /Type /ExtGState /ca 0.5 >> >> /Properties << /L0 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 8 >>\nstream\n\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\nendstream".into(),
        "<< /Type /OCG /Name (Layer 0) >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn stencil_page_with(content: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R >> >> >>"
            .into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ImageMask true /Filter /ASCIIHexDecode /Length 5 >>\nstream\n0000>\nendstream".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn form_page_with(content: &str) -> Document {
    let form_content = "0 0 100 100 re f";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm0 6 0 R >> >> >>"
            .into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length {} >>\nstream\n{form_content}\nendstream", form_content.len()),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// The box of the first `re` on the page in user space, after the `cm`s before it.
fn first_rect_in_user_space(doc: &Document) -> [f64; 4] {
    let mut ctm = pdfcraft_content::Matrix::IDENTITY;
    for op in pdfcraft_content::parse(&page_content_bytes(doc, 0)).ops {
        if op.is("cm") {
            ctm = pdfcraft_content::Matrix(op.nums::<6>().unwrap()).then(&ctm);
        } else if op.is("re") {
            let [x, y, w, h] = op.nums::<4>().unwrap();
            return ctm.bbox([x, y, x + w, y + h]);
        }
    }
    panic!("no clip rectangle");
}

fn close(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

/// The deepest `q` nesting on the page.
fn q_depth(doc: &Document) -> usize {
    let mut depth = 0_usize;
    let mut deepest = 0;
    for op in pdfcraft_content::parse(&page_content_bytes(doc, 0)).ops {
        if op.is("q") {
            depth += 1;
            deepest = deepest.max(depth);
        } else if op.is("Q") {
            depth = depth.saturating_sub(1);
        }
    }
    deepest
}

/// The marked-content sequence that `mcid` opens (`/MCID n >> BDC` … its `EMC`), as content text.
fn marked_sequence(content: &str, mcid: u32) -> &str {
    let tag = format!("/MCID {mcid}");
    let start = content.find(&tag).unwrap_or_else(|| panic!("no {tag} in {content}"));
    let end = content[start..].find("EMC").map(|e| start + e).unwrap_or_else(|| panic!("{tag} has no EMC in {content}"));
    &content[start..end]
}

fn nested_soft_mask_page(content: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R /GroupA 9 0 R /GroupB 10 0 R >> /ExtGState << /GSA 7 0 R /GSB 8 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n\u{0}\u{0}\u{0}\u{0}\nendstream".into(),
        "<< /Type /ExtGState /SMask << /Type /Mask /S /Alpha /G 9 0 R >> >>".into(),
        "<< /Type /ExtGState /SMask << /Type /Mask /S /Alpha /G 10 0 R >> >>".into(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency /I true >> /Length 20 >>\nstream\n0 g 0 0 100 50 re f\nendstream".into(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency /I true >> /Length 20 >>\nstream\n0 g 0 0 50 100 re f\nendstream".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn soft_mask_with_alpha_page(content: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R /GroupA 9 0 R >> /ExtGState << /GSA 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n\u{0}\u{0}\u{0}\u{0}\nendstream".into(),
        "<< /Type /ExtGState /ca 0.25 /SMask << /Type /Mask /S /Alpha /G 9 0 R >> >>".into(),
        "<< /Type /ExtGState /SMask /None >>".into(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency /I true >> /Length 20 >>\nstream\n0 g 0 0 100 50 re f\nendstream".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// A page split into several content streams, as AutoCAD writes them: the first scales the
/// page (`0.12 0 0 0.12 0 0 cm`, no `q`), the text comes in a later one.
fn split_page(first: &str, second: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents [4 0 R 5 0 R] /Resources << /Font << /F1 6 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{first}\nendstream", first.len()),
        format!("<< /Length {} >>\nstream\n{second}\nendstream", second.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn two_stream_image_page(first: &str, second: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents [4 0 R 5 0 R] /Resources << /Font << /F1 6 0 R >> /XObject << /Im0 7 0 R /Im1 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{first}\nendstream", first.len()),
        format!("<< /Length {} >>\nstream\n{second}\nendstream", second.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 8 >>\nstream\n\0\0\0\0\0\0\0\0\nendstream".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// One page whose font draws a few Hebrew letters (ToUnicode): A ב, B ד, C י, D ו, E ק, F ש,
/// G ל, H ם. Content streams draw Hebrew in visual order, so "בדיוק" is `(EDCBA)`.
fn hebrew_page(content: &str) -> Document {
    let cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapType 2 def \
                1 begincodespacerange <00> <FF> endcodespacerange 9 beginbfchar \
                <20> <0020> <41> <05D1> <42> <05D3> <43> <05D9> <44> <05D5> <45> <05E7> <46> <05E9> <47> <05DC> <48> <05DD> \
                endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
    // Widths for codes 32..=72: a space, then the eight letters at 65..=72.
    let widths: Vec<&str> = (32..=72)
        .map(|c| {
            if c == 32 {
                "250"
            } else if c >= 65 {
                "500"
            } else {
                "0"
            }
        })
        .collect();
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+FakeHebrew /FirstChar 32 /LastChar 72 /Widths [{}] /ToUnicode 6 0 R >>",
            widths.join(" ")
        ),
        format!("<< /Length {} >>\nstream\n{cmap}\nendstream", cmap.len()),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn xobject(doc: &Document, page: usize, name: &[u8]) -> Option<pdfcraft_cos::ObjRef> {
    let p = &pdfcraft_model::pages(doc)[page];
    let res = doc.resolve(p.dict.get(b"Resources")?);
    let xo = doc.resolve(res.as_dict()?.get(b"XObject")?);
    xo.as_dict()?.get(name).and_then(Object::as_ref)
}

fn annot_at(doc: &Document, page: usize, index: usize) -> pdfcraft_cos::ObjRef {
    let p = &pdfcraft_model::pages(doc)[page];
    let list = doc.resolve(p.dict.get(b"Annots").unwrap());
    list.as_array().unwrap()[index].as_ref().unwrap()
}

/// A one-page document from its objects (object 1 is the catalog).
fn build(objs: &[&str]) -> Document {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn stream(dict: &str, data: &str) -> String {
    format!("<< {dict} /Length {} >>\nstream\n{data}\nendstream", data.len())
}

/// #3120: a one-page document whose page draws "Secret", then `content`. `forms` are (name,
/// content) form XObjects, which the page and every form can draw, as they can a 1×1 image `/Im`.
fn page_with_forms(content: &str, forms: &[(String, String)]) -> Document {
    let xobjects: String = forms.iter().enumerate().map(|(i, (name, _))| format!("/{name} {} 0 R ", 7 + i)).collect();
    let resources = format!("<< /Font << /F1 5 0 R >> /XObject << /Im 6 0 R {xobjects}>> >>");
    let mut objs = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources {resources} /Contents 4 0 R >>"),
        stream("", &format!("BT /F1 24 Tf 72 700 Td (Secret) Tj ET\n{content}")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        stream("/Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8", "A"),
    ];
    objs.extend(forms.iter().map(|(_, body)| stream(&format!("/Type /XObject /Subtype /Form /BBox [0 0 612 792] /Resources {resources}"), body)));
    build(&objs.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Synthetic codes A س, B م and X a lam-alef glyph, drawn from left to right.
fn arabic_ligature_page(content: &str, composite: bool) -> Document {
    let prefix = if composite { "00" } else { "" };
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapType 2 def \
         1 begincodespacerange <{prefix}00> <{prefix}FF> endcodespacerange 4 beginbfchar \
         <{prefix}20> <0020> <{prefix}41> <0633> <{prefix}42> <0645> <{prefix}58> <FEFB> \
         endbfchar endcmap CMapName currentdict /CMap defineresource pop end end"
    );
    let widths: Vec<_> = (32..=88).map(|c| if c == 32 { "250" } else { "500" }).collect();
    let font = if composite {
        "<< /Type /Font /Subtype /Type0 /BaseFont /FakeArabic /Encoding /Identity-H /DescendantFonts [7 0 R] /ToUnicode 6 0 R >>".into()
    } else {
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /FakeArabic /FirstChar 32 /LastChar 88 /Widths [{}] /ToUnicode 6 0 R >>", widths.join(" "))
    };
    build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        &stream("", content),
        &font,
        &stream("", &cmap),
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /FakeArabic /CIDSystemInfo << /Registry (Synthetic) /Ordering (Identity) /Supplement 0 >> /DW 500 /W [32 [250]] >>",
    ])
}

fn mixed_region_page() -> Document {
    let mut doc = text_page(
        "BT /F2 1 Tf 18 0 0 18 123 700 Tm (TITLE) Tj ET \
         BT /F2 1 Tf 12 0 0 12 117.6 676 Tm (two lines) Tj ET \
         BT /F2 1 Tf 12 0 0 12 135.6 652 Tm (last) Tj ET \
         BT 0 0.2 0.5 rg /F2 10 Tf 72 600 Td (About) Tj ET \
         BT 0 g /F1 10 Tf 72 582 Td (Plain ) Tj /F2 10 Tf 0 0.2 0.5 rg (bold) Tj /F1 10 Tf 0 g ( text.) Tj ET \
         BT 0.2 0.4 0.7 rg /F3 8 Tf 72 566 Td (Italic footer) Tj ET \
         BT 0 g /F1 10 Tf 400 600 Td (Other column) Tj ET",
    );
    for (obj, base) in [(5, "Courier"), (6, "Courier-Bold")] {
        let mut f = Dict::new();
        f.set(b"Type".to_vec(), Object::name("Font"));
        f.set(b"Subtype".to_vec(), Object::name("Type1"));
        f.set(b"BaseFont".to_vec(), Object::name(base));
        f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        doc.set(pdfcraft_cos::ObjRef::new(obj, 0), Object::Dict(f));
    }
    let mut f = Dict::new();
    f.set(b"Type".to_vec(), Object::name("Font"));
    f.set(b"Subtype".to_vec(), Object::name("Type1"));
    f.set(b"BaseFont".to_vec(), Object::name("Courier-Oblique"));
    f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    let font = doc.add(Object::Dict(f));
    let page = pdfcraft_model::pages(&doc)[0].clone();
    let mut resources = doc.resolve(page.dict.get(b"Resources").unwrap()).as_dict().unwrap().clone();
    let mut fonts = doc.resolve(resources.get(b"Font").unwrap()).as_dict().unwrap().clone();
    fonts.set(b"F3".to_vec(), Object::Ref(font));
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    doc.update_dict(page.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
    doc
}

/// [`image_page`] with `pieces` as its `/Contents` array and a Form /Fm0 drawing 72,500–172,600.
fn split_image_page(pieces: &[&[u8]]) -> Document {
    let mut doc = image_page();
    let page = pdfcraft_model::pages(&doc)[0].obj;
    let mut form = Dict::new();
    form.set(b"Type".to_vec(), Object::name("XObject"));
    form.set(b"Subtype".to_vec(), Object::name("Form"));
    form.set(b"BBox".to_vec(), Object::Array([72, 500, 172, 600].map(Object::Int).to_vec()));
    let form = doc.add(Object::Stream(Stream::flate(form, b"0.5 g 72 500 100 100 re f")));
    let image = doc.get(pdfcraft_model::pages(&doc)[0].obj);
    let mut res = image.as_dict().and_then(|d| d.get(b"Resources")).and_then(|r| r.as_dict().cloned()).unwrap();
    let mut xo = res.get(b"XObject").and_then(|x| x.as_dict().cloned()).unwrap();
    xo.set(b"Fm0".to_vec(), Object::Ref(form));
    res.set(b"XObject".to_vec(), Object::Dict(xo));
    let refs = pieces.iter().map(|b| Object::Ref(doc.add(Object::Stream(Stream::flate(Dict::new(), b))))).collect();
    doc.update_dict(page, |d| {
        d.set(b"Contents".to_vec(), Object::Array(refs));
        d.set(b"Resources".to_vec(), Object::Dict(res));
    })
    .unwrap();
    doc
}

/// One page in a font named `base_font` whose ToUnicode maps A ब, B ि, C ढ़, D य, E ा, F क, G त.
/// A legacy visual-order font draws "बढ़िया" as `(ABCDE)` (ब ि ढ़ या) and "किताब" as `(BFGEA)`.
fn devanagari_page(base_font: &str, content: &str) -> Document {
    let cmap = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapType 2 def \
                1 begincodespacerange <00> <FF> endcodespacerange 8 beginbfchar \
                <20> <0020> <41> <092C> <42> <093F> <43> <0922093C> <44> <092F> <45> <093E> <46> <0915> <47> <0924> \
                endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
    let widths: Vec<&str> = (32..=71)
        .map(|c| {
            if c == 32 {
                "250"
            } else if c >= 65 {
                "500"
            } else {
                "0"
            }
        })
        .collect();
    let font = format!(
        "<< /Type /Font /Subtype /Type1 /BaseFont /{base_font} /FirstChar 32 /LastChar 71 /Widths [{}] /ToUnicode 6 0 R >>",
        widths.join(" ")
    );
    build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        &stream("", content),
        &font,
        &stream("", cmap),
    ])
}

/// A page whose `/F1` is Inter embedded as a simple WinAnsi TrueType font, under `base_font`.
fn embedded_program_page(content: &str, base_font: &str) -> Document {
    let mut doc = text_page(content);
    let program = doc.add(Object::Stream(Stream::flate(Dict::new(), include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))));
    let mut descriptor = Dict::new();
    descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
    descriptor.set(b"FontName".to_vec(), Object::name(base_font));
    descriptor.set(b"FontFile2".to_vec(), Object::Ref(program));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("TrueType"));
    font.set(b"BaseFont".to_vec(), Object::name(base_font));
    font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    font.set(b"FirstChar".to_vec(), Object::Int(32));
    font.set(b"LastChar".to_vec(), Object::Int(126));
    font.set(b"Widths".to_vec(), Object::Array((32..=126).map(|_| Object::Int(560)).collect()));
    font.set(b"FontDescriptor".to_vec(), Object::Dict(descriptor));
    doc.set(pdfcraft_cos::ObjRef::new(5, 0), Object::Dict(font));
    reopen(&doc)
}

/// The font programs and subtypes of the page's font resources.
fn page_font_programs(doc: &Document) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    let page = pdfcraft_model::pages(doc).remove(0);
    let resources = page.dict.get(b"Resources").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let fonts = resources.get(b"Font").and_then(|f| doc.resolve(f).as_dict().cloned()).unwrap_or_default();
    fonts
        .iter()
        .filter_map(|(_, f)| doc.resolve(f).as_dict().cloned())
        .map(|f| (f.name(b"Subtype").unwrap_or_default().to_vec(), pdfcraft_fonts::pdf::embedded_sfnt(doc, &f)))
        .collect()
}

/// Small graphics-state oracle for synthetic streams: every tracked state change must
/// have been restored when a probe XObject would draw. No external assets are involved.
fn probe_states(doc: &Document) -> Vec<std::collections::BTreeMap<Vec<u8>, Vec<Object>>> {
    let bytes = streams(doc, 0);
    let joined = pdfcraft_content::Pieces::join(&bytes);
    let mut state = std::collections::BTreeMap::new();
    let mut stack = Vec::new();
    let mut probes = Vec::new();
    for op in joined.parse().ops {
        match op.op.as_slice() {
            b"q" => stack.push(state.clone()),
            b"Q" => state = stack.pop().expect("balanced graphics state"),
            b"cm" | b"W" | b"W*" | b"Tz" | b"Tc" | b"Tw" | b"Tr" | b"cs" => {
                state.insert(op.op, op.operands);
            }
            b"Do" if op.name(0) == Some(b"PCProbe") => probes.push(state.clone()),
            _ => {}
        }
    }
    probes
}

fn isolation_fixture(parts: &[(&str, &str)]) -> Document {
    let mut doc = fixture();
    let page = page_list(&doc).remove(0);
    let list = parts
        .iter()
        .map(|(tag, bytes)| {
            Object::Ref(doc.add(Object::Stream(Stream::from_raw(if tag.is_empty() { Dict::new() } else { tagged(tag) }, bytes.as_bytes().to_vec()))))
        })
        .collect();
    doc.update_dict(page.obj, |d| d.set(b"Contents".to_vec(), Object::Array(list))).unwrap();
    doc
}

/// [`probe_states`] for unbalanced content: a `Q` with nothing saved leaves the state as is,
/// as renderers do.
fn probe_states_lenient(doc: &Document) -> Vec<std::collections::BTreeMap<Vec<u8>, Vec<Object>>> {
    let joined = pdfcraft_content::Pieces::join(&streams(doc, 0));
    let mut state = std::collections::BTreeMap::new();
    let mut stack = Vec::new();
    let mut probes = Vec::new();
    for op in joined.parse().ops {
        match op.op.as_slice() {
            b"q" => stack.push(state.clone()),
            b"Q" => state = stack.pop().unwrap_or(state),
            b"cm" | b"W" | b"W*" | b"Tz" | b"Tc" | b"Tw" | b"Tr" | b"cs" => {
                state.insert(op.op, op.operands);
            }
            b"Do" if op.name(0) == Some(b"PCProbe") => probes.push(state.clone()),
            _ => {}
        }
    }
    probes
}

/// A page whose text is a clipping mask (`mode` Tr) with a fill painted through it.
fn clipping_text_page(mode: i64) -> Document {
    text_page(&format!("q\nBT {mode} Tr /F1 48 Tf 40 60 Td (MASK) Tj ET\n1 0 0 rg 0 0 300 150 re f\nQ\n"))
}

/// A one-page document whose resources have `/GS0` (30 % opacity) and `/L1`, a layer that is OFF.
fn row_state_page(content: &str) -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] /Order [5 0 R] >> >> >>".into(),
        "<< /Type /Pages /Kids [6 0 R] /Count 1 >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        "<< /Type /ExtGState /ca 0.3 /CA 0.3 >>".into(),
        "<< /Type /OCG /Name (Hidden layer) >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 7 0 R /Resources << /Font << /F1 3 0 R >> /ExtGState << /GS0 4 0 R /GS1 << /Type /ExtGState /ca 0.3 /CA 0.3 /BM /Normal >> /GS2 << /Type /ExtGState /ca 1 /CA 1 >> >> /Properties << /L1 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// Each shown string on page 0 with the state it is drawn in: inside a `/OC` marked-content
/// sequence, after `/GS0 gs`, and its horizontal scale (percent).
fn shown_states(doc: &Document) -> Vec<(String, bool, bool, f64)> {
    let joined = streams(doc, 0).join("\n");
    let (mut oc, mut marks) = (0usize, Vec::new());
    let (mut gs, mut tz, mut stack) = (false, 100.0, Vec::new());
    let mut out = Vec::new();
    for op in pdfcraft_content::parse(joined.as_bytes()).ops {
        match op.op.as_slice() {
            b"BDC" | b"BMC" => {
                let layer = op.name(0) == Some(b"OC".as_slice());
                oc += usize::from(layer);
                marks.push(layer);
            }
            b"EMC" => oc -= usize::from(marks.pop().unwrap()),
            b"q" => stack.push((gs, tz)),
            b"Q" => (gs, tz) = stack.pop().unwrap(),
            b"gs" => gs |= op.name(0) == Some(b"GS0".as_slice()),
            b"Tz" => tz = op.num(0).unwrap(),
            b"Tj" | b"'" => {
                let Some(Object::String(s)) = op.operands.last() else { continue };
                if !s.bytes.is_empty() {
                    out.push((String::from_utf8_lossy(&s.bytes).into_owned(), oc > 0, gs, tz));
                }
            }
            _ => {}
        }
    }
    out
}

/// A page of three vertical (`Identity-V`) columns at 20 pt: `ABCDEF` at x = 500, the same
/// text 30 pt to its left, and `ABCD` far away. Synthetic font: no font program is needed.
fn vertical_columns_page() -> Document {
    vertical_page(
        "BT /V1 20 Tf 500 720 Td <004100420043004400450046> Tj ET\nBT /V1 20 Tf 470 720 Td <004100420043004400450046> Tj ET\nBT /V1 20 Tf 300 720 Td <0041004200430044> Tj ET",
    )
}

/// A page drawing `content` with the vertical font `/V1` (as in [`vertical_columns_page`]) and
/// Helvetica `/F1`.
fn vertical_page(content: &str) -> Document {
    let mut doc = text_page(content);
    let mut font = Dict::from_iter([
        (b"Type".to_vec(), Object::name("Font")),
        (b"Subtype".to_vec(), Object::name("Type0")),
        (b"BaseFont".to_vec(), Object::name("SyntheticCID")),
        (b"Encoding".to_vec(), Object::name("Identity-V")),
    ]);
    let unicode = doc.add(Object::Stream(Stream::flate(
        Dict::new(),
        b"1 begincodespacerange <0000> <ffff> endcodespacerange 6 beginbfchar <0041> <0041> <0042> <0042> <0043> <0043> <0044> <0044> <0045> <0045> <0046> <0046> endbfchar",
    )));
    font.set(b"ToUnicode".to_vec(), Object::Ref(unicode));
    font.set(
        b"DescendantFonts".to_vec(),
        Object::Array(vec![Object::Dict(Dict::from_iter([
            (b"Subtype".to_vec(), Object::name("CIDFontType2")),
            (b"DW".to_vec(), Object::Int(1000)),
        ]))]),
    );
    let p = pdfcraft_model::pages(&doc)[0].obj;
    let helvetica = Dict::from_iter([
        (b"Type".to_vec(), Object::name("Font")),
        (b"Subtype".to_vec(), Object::name("Type1")),
        (b"BaseFont".to_vec(), Object::name("Helvetica")),
        (b"Encoding".to_vec(), Object::name("WinAnsiEncoding")),
    ]);
    let fonts = Dict::from_iter([(b"V1".to_vec(), Object::Dict(font)), (b"F1".to_vec(), Object::Dict(helvetica))]);
    let resources = Dict::from_iter([(b"Font".to_vec(), Object::Dict(fonts))]);
    doc.update_dict(p, |d| d.set(b"Resources".to_vec(), Object::Dict(resources))).unwrap();
    doc
}

/// Gives `/V1` an embedded CMap that uses itself: which way its glyphs advance can't be known.
fn make_cmap_cyclic(doc: &mut Document) {
    let cmap = doc.add(Object::Null);
    doc.set(
        cmap,
        Object::Stream(Stream::flate(
            Dict::from_iter([(b"UseCMap".to_vec(), Object::Ref(cmap))]),
            b"1 begincodespacerange <0000> <ffff> endcodespacerange",
        )),
    );
    let p = pdfcraft_model::pages(doc)[0].obj;
    doc.update_dict(p, |d| {
        if let Some(Object::Dict(res)) = d.get_mut(b"Resources")
            && let Some(Object::Dict(fonts)) = res.get_mut(b"Font")
            && let Some(Object::Dict(font)) = fonts.get_mut(b"V1")
        {
            font.set(b"Encoding".to_vec(), Object::Ref(cmap));
        }
    })
    .unwrap();
}

fn page_background_from_content(content: &str, resources: &str) -> Option<[f64; 3]> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /CropBox [0 0 500 700] /Contents 4 0 R /Resources << {resources} >> >>"),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref_offset = bytes.len();
    bytes.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());
    page_background(&Document::open(Arc::new(bytes)).unwrap(), 0).unwrap()
}

fn assert_page_background_fill_cases(gs: &str) {
    assert_eq!(page_background_from_content("1 g 0 0 600 800 re f", ""), Some([1.0; 3]), "a producer's white page fill");
    assert_eq!(page_background_from_content("0 0 1 0 k 0 0 500 700 re f", ""), Some([1.0, 1.0, 0.0]), "CMYK, covering the crop box");
    assert_eq!(page_background_from_content("/DeviceRGB cs 1 0 0 sc 0 0 500 700 re f", ""), Some([1.0, 0.0, 0.0]), "cs and sc");
    assert_eq!(page_background_from_content("/Full gs q 2 0 0 2 0 0 cm 0 1 0 rg 0 0 300 400 re f Q", gs), Some([0.0, 1.0, 0.0]), "under a cm");
    assert_eq!(page_background_from_content("1 0 0 rg 0 0 300 800 re f", ""), None, "half the page");
    assert_eq!(page_background_from_content("1 0 0 rg 0 0 300 800 re 300 0 300 800 re f", ""), None, "two halves are not one covering rectangle");
    assert_eq!(
        page_background_from_content("1 0 0 rg 0 0 300 800 re 0 0 600 800 re f", ""),
        Some([1.0, 0.0, 0.0]),
        "one of the rectangles covers it"
    );
    // #2994 review: a second rectangle that cuts a hole: any one under even-odd, one wound the
    // other way under nonzero.
    assert_eq!(page_background_from_content("1 0 0 rg 0 0 600 800 re 100 100 200 200 re f*", ""), None, "even-odd hole");
    assert_eq!(page_background_from_content("1 0 0 rg 0 0 600 800 re 300 100 -200 200 re f", ""), None, "nonzero, reversed inner rectangle");
    assert_eq!(
        page_background_from_content("1 0 0 rg 0 0 600 800 re 100 100 200 200 re f", ""),
        Some([1.0, 0.0, 0.0]),
        "nonzero, same winding: no hole"
    );
    assert_eq!(
        page_background_from_content("1 0 0 rg q -1 0 0 1 600 0 cm 0 0 600 800 re Q 100 100 200 200 re f", ""),
        None,
        "reversed by a mirroring cm"
    );
    assert_eq!(page_background_from_content("/Half gs 1 0 0 rg 0 0 600 800 re f", gs), None, "translucent");
    assert_eq!(page_background_from_content("/Mul gs 1 0 0 rg 0 0 600 800 re f", gs), None, "blended");
    assert_eq!(page_background_from_content("0 0 100 100 re W n 1 0 0 rg 0 0 600 800 re f", ""), None, "clipped to a corner");
    assert_eq!(page_background_from_content("0 0 600 800 re W n 1 0 0 rg 0 0 600 800 re f", ""), Some([1.0, 0.0, 0.0]), "a page-sized clip");
    assert_eq!(page_background_from_content("/Pattern cs /P1 scn 0 0 600 800 re f", ""), None, "a pattern");
    assert_eq!(page_background_from_content("BT ET 0 0 600 800 re S 1 0 0 rg 0 0 600 800 re f", ""), None, "a stroke first");
    assert_eq!(page_background_from_content("0 0 m 600 0 l 600 800 l 0 800 l h f", ""), None, "a path that isn't a rectangle");
    assert_eq!(
        page_background_from_content("1 0 0 rg 0 0 600 800 re 100 100 m 200 100 l 200 200 l h f*", ""),
        None,
        "a page fill with a hole cut in it"
    );
}

fn assert_page_background_transform_and_clip_cases() {
    // #2994 review: a rectangle turned 45° about the crop box's centre covers it only if the crop
    // box's corners are inside it; its bounding box covering the crop box isn't enough. This one
    // reaches 500 from the centre along each axis (its box covers the 500 × 700 crop box) but
    // 354 along the diagonals, short of the corners. One reaching 707 covers them.
    let turned = "0.70710678 0.70710678 -0.70710678 0.70710678 250 350 cm";
    assert_eq!(
        page_background_from_content(&format!("1 0 0 rg {turned} -353.553 -353.553 707.107 707.107 re f"), ""),
        None,
        "a turned fill short of the corners"
    );
    assert_eq!(
        page_background_from_content(&format!("1 0 0 rg {turned} -500 -500 1000 1000 re f"), ""),
        Some([1.0, 0.0, 0.0]),
        "a turned fill over them"
    );
    // The same for a clip.
    assert_eq!(
        page_background_from_content(&format!("{turned} -353.553 -353.553 707.107 707.107 re W n 1 0 0 rg -2000 -2000 4000 4000 re f"), ""),
        None,
        "a turned clip"
    );
    assert_eq!(
        page_background_from_content(&format!("{turned} -500 -500 1000 1000 re W n 1 0 0 rg -2000 -2000 4000 4000 re f"), ""),
        Some([1.0, 0.0, 0.0]),
        "a turned clip over the corners"
    );
    // Clips add up: two that each cover the page leave it covered, one that doesn't among them
    // doesn't.
    assert_eq!(
        page_background_from_content("0 0 600 800 re W n 0 0 550 750 re W n 1 0 0 rg 0 0 600 800 re f", ""),
        Some([1.0, 0.0, 0.0]),
        "two covering clips"
    );
    assert_eq!(
        page_background_from_content("0 0 600 800 re W n 0 0 300 750 re W n 1 0 0 rg 0 0 600 800 re f", ""),
        None,
        "the second clip doesn't cover"
    );
    // Past 64 clipping rectangles the clip counts as unknown.
    let clips = "0 0 600 800 re W n ".repeat(65);
    assert_eq!(page_background_from_content(&format!("{clips}1 0 0 rg 0 0 600 800 re f"), ""), None, "too many clips to follow");
    // A rectangle drawn the other way round (negative width) covers the page all the same.
    assert_eq!(page_background_from_content("1 0 0 rg 600 0 -600 800 re f", ""), Some([1.0, 0.0, 0.0]), "drawn clockwise");
    // The crop box counts as covered within half a percent of its size (2.5 × 3.5 here), the
    // edge of that included.
    assert_eq!(page_background_from_content("1 0 0 rg 2 3 496 694 re f", ""), Some([1.0, 0.0, 0.0]), "within the tolerance");
    assert_eq!(page_background_from_content("1 0 0 rg 2.5 3.5 495 693 re f", ""), Some([1.0, 0.0, 0.0]), "on the tolerance's edge");
    assert_eq!(page_background_from_content("1 0 0 rg 3 4 494 692 re f", ""), None, "past the tolerance");
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {

        #[test]
        fn prop_is_entire_page_wrapped_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            let pieces = pdfcraft_content::Pieces::join(&[&bytes]);
            let ops = pieces.parse().ops;
            let _ = images::is_entire_page_wrapped(&ops);
        }

        #[test]
        fn prop_content_units_arbitrary_bytes(
            bytes in proptest::collection::vec(any::<u8>(), 0..2048),
            start in 0usize..50,
            end in 0usize..50,
        ) {
            let pieces = pdfcraft_content::Pieces::join(&[&bytes]);
            let ops = pieces.parse().ops;
            let scan_start = start.min(ops.len());
            let scan_end = end.min(ops.len()).max(scan_start);
            let units = images::content_units(&ops, scan_start, scan_end);
            for u in units {
                prop_assert!(u.start <= u.end);
                prop_assert!(u.end <= ops.len());
            }
        }

        #[test]
        fn prop_arrange_image_arbitrary_malformed(
            index in 0usize..10,
            order_idx in 0u8..4,
            bytes in proptest::collection::vec(any::<u8>(), 0..1024),
        ) {
            let order = match order_idx {
                0 => images::ArrangeOrder::BringToFront,
                1 => images::ArrangeOrder::SendToBack,
                2 => images::ArrangeOrder::BringForward,
                _ => images::ArrangeOrder::SendBackward,
            };
            let mut doc = image_page_with(&String::from_utf8_lossy(&bytes));
            let _ = arrange(&mut doc, 0, index, order);
        }

        #[test]
        fn prop_crop_image_malformed_inputs(
            x1 in -500.0f64..500.0,
            y1 in -500.0f64..500.0,
            x2 in -500.0f64..500.0,
            y2 in -500.0f64..500.0,
            bytes in proptest::collection::vec(any::<u8>(), 0..1024),
        ) {
            let mut doc = image_page_with(&String::from_utf8_lossy(&bytes));
            let _ = crop(&mut doc, 0, 0, [x1, y1, x2, y2]);
            let _ = remove_crop(&mut doc, 0, 0);
        }
    }
}

/// One 600×800 page: Im0 (4×2 gray image), Fm0 (200×100 Form), ExtGState GSone (ca 1).
fn regression_page(content: &str) -> Document {
    let form = "0 g 0 0 200 100 re f";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im0 6 0 R /Fm0 7 0 R >> /ExtGState << /GSone << /Type /ExtGState /ca 1 >> >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 8 >>\nstream\n\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\nendstream".into(),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 200 100] /Length {} >>\nstream\n{form}\nendstream", form.len()),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn regression_rects(doc: &Document) -> Vec<[i64; 4]> {
    let mut v: Vec<[i64; 4]> = images::page_images(doc, 0).unwrap().iter().map(|i| i.rect.map(|x| x.round() as i64)).collect();
    v.sort();
    v
}

/// A one-page document from the page's /Contents and /Resources, built in code.
fn page_with(build: impl FnOnce(&mut Document) -> (Object, Dict)) -> Document {
    let mut doc = Document::new_empty();
    let pages = doc.add(Object::Null);
    let (contents, resources) = build(&mut doc);
    let page: Dict = [
        (b"Type".to_vec(), Object::Name(b"Page".to_vec())),
        (b"Parent".to_vec(), Object::Ref(pages)),
        (b"MediaBox".to_vec(), Object::Array([0, 0, 200, 300].map(Object::from).to_vec())),
        (b"Contents".to_vec(), contents),
        (b"Resources".to_vec(), Object::Dict(resources)),
    ]
    .into_iter()
    .collect();
    let page = doc.add(page);
    let tree: Dict = [
        (b"Type".to_vec(), Object::Name(b"Pages".to_vec())),
        (b"Kids".to_vec(), Object::Array(vec![Object::Ref(page)])),
        (b"Count".to_vec(), Object::Int(1)),
    ]
    .into_iter()
    .collect();
    doc.set(pages, Object::Dict(tree));
    let catalog: Dict = [(b"Type".to_vec(), Object::Name(b"Catalog".to_vec())), (b"Pages".to_vec(), Object::Ref(pages))].into_iter().collect();
    let catalog = doc.add(catalog);
    doc.trailer_mut().set(b"Root".to_vec(), Object::Ref(catalog));
    doc
}

/// How each glyph-showing operator on page 0 is painted, read the way a renderer reads the page:
/// its text, `Tr`, stroke colour operators, line width (from `w` or an ExtGState's `/LW`) and
/// the ExtGStates selected before it, following `q`/`Q`.
#[derive(Clone, Debug, PartialEq)]
struct Painted {
    text: String,
    render_mode: i64,
    stroke: Vec<String>,
    line_width: f64,
    gs: Vec<String>,
}

fn painted_shows(doc: &Document) -> Vec<Painted> {
    let (_, ops) = joined_ops(doc);
    let p = &pdfcraft_model::pages(doc)[0];
    let res = doc.resolve(p.dict.get(b"Resources").unwrap());
    let ext = res.as_dict().unwrap().get(b"ExtGState").map(|g| doc.resolve(g).as_dict().cloned().unwrap()).unwrap_or_default();
    let mut state = Painted { text: String::new(), render_mode: 0, stroke: Vec::new(), line_width: 1.0, gs: Vec::new() };
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let show = |op: &pdfcraft_content::Op| String::from_utf8_lossy(&pdfcraft_content::serialize_ops(std::slice::from_ref(op))).trim().to_string();
    for op in ops {
        match op.op.as_slice() {
            b"q" => stack.push(state.clone()),
            b"Q" => state = stack.pop().unwrap(),
            b"Tr" => state.render_mode = op.operands[0].as_int().unwrap(),
            b"G" | b"RG" | b"K" | b"CS" => state.stroke = vec![show(&op)],
            b"SC" | b"SCN" => state.stroke.push(show(&op)),
            b"w" => state.line_width = op.num(0).unwrap(),
            b"gs" => {
                let name = op.name(0).unwrap();
                state.gs.push(String::from_utf8_lossy(name).into_owned());
                let lw = ext.get(name).and_then(|g| doc.resolve(g).as_dict().and_then(|g| g.get(b"LW")).and_then(|w| w.as_f64()));
                state.line_width = lw.unwrap_or(state.line_width);
            }
            b"Tj" => {
                let Some(Object::String(s)) = op.operands.last() else { continue };
                if !s.bytes.is_empty() {
                    out.push(Painted { text: String::from_utf8_lossy(&s.bytes).into_owned(), ..state.clone() });
                }
            }
            _ => {}
        }
    }
    out
}

/// A page with Helvetica as /F1, /GS1, an ExtGState that paints at 20 % opacity, /GS2, one that
/// sets the line width to 4, /GS3, another at 20 % opacity, and /GS4, at 20 % opacity and dashed.
fn faint_page(content: &str) -> Document {
    build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /ExtGState << /GS1 6 0 R /GS2 7 0 R /GS3 8 0 R /GS4 9 0 R >> >> >>",
        &format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        "<< /Type /ExtGState /ca 0.2 >>",
        "<< /Type /ExtGState /LW 4 >>",
        "<< /Type /ExtGState /ca 0.2 >>",
        "<< /Type /ExtGState /ca 0.2 /D [[6 3] 0] >>",
    ])
}

/// A 1 × 1 grey inline image whose one sample is `data`.
fn inline_image(data: char) -> String {
    format!("BI /W 1 /H 1 /CS /G /BPC 8 ID {data} EI")
}

/// What page 0 paints, in order: `BI x` for an inline image with sample `x`, `Do x,y` for an
/// XObject placed by a `cm` translating to x,y, and `Tj` for text. A `cm` lasts until its `Q`, so an
/// XObject moved out of its `q`/`Q` wrapper shows as a bare `Do `.
fn painted(doc: &Document) -> Vec<String> {
    let mut at = vec![String::new()];
    pdfcraft_content::parse(&page_content_bytes(doc, 0))
        .ops
        .iter()
        .filter_map(|op| match op.op.as_slice() {
            b"q" => {
                at.push(at.last().unwrap().clone());
                None
            }
            b"Q" => {
                at.pop();
                None
            }
            b"cm" => {
                let m = op.nums::<6>().unwrap();
                *at.last_mut().unwrap() = format!("{},{}", m[4], m[5]);
                None
            }
            b"Do" => Some(format!("Do {}", at.last().unwrap())),
            b"BI" => Some(format!("BI {}", String::from_utf8_lossy(&op.inline.as_ref().unwrap().1))),
            b"Tj" => Some("Tj".into()),
            _ => None,
        })
        .collect()
}

/// The index in [`images::page_images`] of the image whose box starts at `x`,`y`.
fn image_at(doc: &Document, x: f64, y: f64) -> usize {
    images::page_images(doc, 0).unwrap().iter().position(|i| (i.rect[0] - x).abs() < 1e-6 && (i.rect[1] - y).abs() < 1e-6).unwrap()
}

fn review_state_raster(doc: &Document) -> Vec<u8> {
    let bytes = write_full(doc, &SaveOptions::default()).unwrap();
    let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(bytes), Default::default());
    renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() }).rgba.to_vec()
}

fn review_shared_mask_page(states: &str, luminosity_b: bool, mask_paint: &str) -> Document {
    let content = format!("q {states} q 100 0 0 100 20 20 cm /Im0 Do Q Q q 0 0 1 rg 200 200 5 5 re f Q");
    let stream = format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len());
    let subtype = if luminosity_b { "Luminosity" } else { "Alpha" };
    let mask_b = format!("<< /Type /ExtGState /SMask << /S /{subtype} /G 8 0 R >> >>");
    let group = format!(
        "<< /Type /XObject /Subtype /Form /BBox [0 0 300 300] /Group << /S /Transparency /I true /CS /DeviceGray >> /Length {} >>\nstream\n{mask_paint}\nendstream",
        mask_paint.len()
    );
    build(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> /ExtGState << /A 6 0 R /B 7 0 R >> >> >>",
        &stream,
        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n\0\0\0\0\nendstream",
        "<< /Type /ExtGState /SMask << /S /Alpha /G 8 0 R >> >>",
        &mask_b,
        &group,
    ])
}
