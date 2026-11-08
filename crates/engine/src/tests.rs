use std::sync::Arc;

use super::document::is_placeholder_title;
use super::edit::{EditCtx, check_permission, run_edit};
use super::*;
use pdfcraft_render::{Layer, LayerOp, OpenError, ReadingColors};

// The tests are split by position, not by topic, so that no file runs to ten thousand lines.
// Shared fixtures and helpers stay in this file; each part reaches them through `use super::*`.
mod tests_part1;
mod tests_part2;
mod tests_part3;
mod tests_part4;
mod tests_part5;
mod tests_part6;
mod tests_part7;

/// A minimal `n`-page document; page `i` shows "Page i+1".
pub(crate) fn fixture(n: usize) -> Vec<u8> {
    let mut objs: Vec<Vec<u8>> = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")).into_bytes());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    for i in 0..n {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i).into_bytes());
        let body = format!("BT /F1 24 Tf 20 150 Td (Page {}) Tj ET", i + 1);
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()).into_bytes());
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn fixture_with_rotation_and_user_unit(rotate: i64, user_unit: f64) -> Vec<u8> {
    let mut objs: Vec<Vec<u8>> = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()];
    objs.push(b"<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_vec());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    let mut page_dict = String::from("<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >>");
    if rotate != 0 {
        page_dict.push_str(&format!(" /Rotate {rotate}"));
    }
    if (user_unit - 1.0).abs() > 1e-6 {
        page_dict.push_str(&format!(" /UserUnit {user_unit}"));
    }
    page_dict.push_str(" >>");
    objs.push(page_dict.into_bytes());
    let body = "BT /F1 24 Tf 20 150 Td (Page 1) Tj ET";
    objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()).into_bytes());

    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn session_with(n: usize) -> (Session, DocId) {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("fixture.pdf", None, Arc::new(fixture(n)), None).expect("opens");
    (s, id)
}

/// A blank page turned `degrees`, with one annotation placed from displayed coordinates.
fn rotated_blank(degrees: i64) -> (Session, DocId) {
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let blank = session.create_blank(200.0, 300.0, 1).unwrap();
    let id = session.open_new("drawing.pdf", blank).unwrap();
    session.apply(id, Edit::RotatePages { pages: vec![0], degrees }).unwrap();
    (session, id)
}

/// The user-space box of a `w` by `h` point rectangle centred at a displayed point.
fn displayed_rect(info: &pdfcraft_render::PageInfo, vx: f64, vy: f64, w: f64, h: f64) -> [f64; 4] {
    let corners = [(vx - w / 2.0, vy - h / 2.0), (vx + w / 2.0, vy + h / 2.0)];
    let mapped: Vec<[f32; 2]> = corners.iter().map(|(x, y)| info.view_to_user(*x as f32, *y as f32)).collect();
    [
        f64::from(mapped[0][0].min(mapped[1][0])),
        f64::from(mapped[0][1].min(mapped[1][1])),
        f64::from(mapped[0][0].max(mapped[1][0])),
        f64::from(mapped[0][1].max(mapped[1][1])),
    ]
}

fn render_page(session: &Session, id: DocId) -> pdfcraft_render::RenderedPage {
    let doc = session.get(id).unwrap();
    let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
    let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 2.0, ..Default::default() });
    assert!(rendered.error.is_none(), "{:?}", rendered.error);
    rendered
}

fn stamp_pixel(rendered: &pdfcraft_render::RenderedPage, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * rendered.width + x) * 4) as usize;
    rendered.rgba[offset..offset + 4].try_into().unwrap()
}

/// A one-page PDF whose left half is red and whose right half is white, as displayed.
fn half_red_page() -> Vec<u8> {
    let content = "1 0 0 rg 0 0 40 40 re f";
    let objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 80 40] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /MediaBox [0 0 80 40] >>".to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn quadrant_png() -> Arc<Vec<u8>> {
    let colours = [[240, 20, 20, 255], [20, 180, 20, 255], [20, 20, 240, 255], [230, 180, 20, 255]];
    let image = image::RgbaImage::from_fn(80, 40, |x, y| image::Rgba(colours[usize::from(y >= 20) * 2 + usize::from(x >= 40)]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    Arc::new(png.into_inner())
}

fn assert_upright_quadrants(session: &Session, id: DocId, degrees: i64) {
    let doc = session.get(id).unwrap();
    let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
    let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(rendered.error.is_none(), "{:?}", rendered.error);
    let colours = [[240, 20, 20, 255], [20, 180, 20, 255], [20, 20, 240, 255], [230, 180, 20, 255]];
    for ((x, y), colour) in [(80, 90), (120, 90), (80, 110), (120, 110)].into_iter().zip(colours) {
        assert_eq!(stamp_pixel(&rendered, x, y), colour, "rotation {degrees}; displayed point ({x}, {y})");
    }
}

pub(crate) fn page_texts(s: &Session, id: DocId) -> Vec<String> {
    let doc = s.get(id).unwrap();
    let config = pdfcraft_render::RenderConfig { password: doc.password.as_deref().map(Arc::from), ..Default::default() };
    let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), config);
    (0..doc.info.pages.len())
        .map(|p| {
            let r = r.render(RenderRequestFor::text(p));
            r.text.map(|t| t.plain_text().trim().to_string()).unwrap_or_default()
        })
        .collect()
}

/// Shorthand for building text-extraction requests.
struct RenderRequestFor;
impl RenderRequestFor {
    fn text(page: usize) -> pdfcraft_render::RenderRequest {
        pdfcraft_render::RenderRequest { page, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() }
    }
}

/// A one-page PDF whose form holds one text field named `name` (a widget that is its own field).
fn form_with_field(name: &str) -> Vec<u8> {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>".into(), // 1
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>".into(), // 2
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".into(),                   // 3
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V (x) /Rect [60 60 200 80] /P 3 0 R >>"), // 4
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    bytes
}

/// A minimal baseline JPEG (headers only; the data is embedded as is).
fn jpeg_bytes() -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8];
    v.extend_from_slice(&[0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 1, 0x01, 0x2C, 0x01, 0x2C, 0, 0]);
    v.extend_from_slice(&[0xFF, 0xC0, 0, 17, 8, 0, 2, 0, 3, 3, 1, 0x11, 0, 2, 0x11, 1, 3, 0x11, 1]);
    v.extend_from_slice(&[0xFF, 0xD9]);
    v
}

fn protected(user: &str, owner: &str, permissions: i32) -> Arc<Vec<u8>> {
    let mut doc = pdfcraft_cos::Document::open(Arc::new(fixture(2))).unwrap();
    doc.set_encryption(&pdfcraft_cos::NewEncryption {
        algorithm: pdfcraft_cos::Algorithm::Aes256,
        user_password: user,
        owner_password: owner,
        permissions,
        encrypt_metadata: true,
        seed: [7; 32],
    })
    .unwrap();
    Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap())
}

/// A 100 × 400 pt page with one band per kind of thing that prints, top to bottom: page content
/// (blue), a filled text field (green), a stamp (red) and a square comment (yellow).
fn print_content_pdf() -> Vec<u8> {
    let content = "0 0 1 rg 0 300 100 100 re f";
    let ap = |colour: &str| {
        let ops = format!("{colour} rg 0 0 100 100 re f");
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length {} >> stream\n{ops}\nendstream", ops.len())
    };
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] >> >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 400] /Contents 4 0 R /Annots [5 0 R 6 0 R 7 0 R] >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n\
         5 0 obj << /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /V (Filled) /F 4 /P 3 0 R /Rect [0 200 100 300] /AP << /N 8 0 R >> >> endobj\n\
         6 0 obj << /Type /Annot /Subtype /Stamp /F 4 /P 3 0 R /Rect [0 100 100 200] /AP << /N 9 0 R >> >> endobj\n\
         7 0 obj << /Type /Annot /Subtype /Square /F 4 /P 3 0 R /Rect [0 0 100 100] /AP << /N 10 0 R >> >> endobj\n\
         8 0 obj {} endobj\n\
         9 0 obj {} endobj\n\
         10 0 obj {} endobj\n\
         trailer << /Root 1 0 R >>\n%%EOF",
        content.len(),
        ap("0 1 0"),
        ap("1 0 0"),
        ap("1 1 0"),
    )
    .into_bytes()
}

fn outline_titles(items: &[pdfcraft_render::OutlineItem]) -> Vec<String> {
    items
        .iter()
        .map(|o| {
            if o.children.is_empty() {
                format!("{}→{}", o.title, o.page.map_or(0, |p| p + 1))
            } else {
                format!("{}→{}[{}]", o.title, o.page.map_or(0, |p| p + 1), outline_titles(&o.children).join(","))
            }
        })
        .collect()
}

/// Two pages tagged with an H1 on the first and an H2 on the second.
fn tagged_headings_fixture() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 7 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 200] /Resources << /Font << /F1 6 0 R >> >> >>",
        "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>",
        "<< /Type /Page /Parent 2 0 R >>",
        "<< /Length 52 >>\nstream\n/H1 << /MCID 0 >> BDC BT /F1 12 Tf (Intro) Tj ET EMC\nendstream",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        "<< /Type /StructTreeRoot /K [8 0 R 9 0 R] >>",
        "<< /S /H1 /Pg 3 0 R /K 0 >>",
        "<< /S /H2 /Pg 4 0 R /ActualText (Details) >>",
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

/// Three pages whose headings are set apart only by how they look (#1693): chapters in
/// Helvetica-Bold 18, sections in Helvetica-Bold 14, text in Helvetica 11. Page 2's chapter title
/// runs over two lines, its bold font is another subset of the same font, and one section is set
/// with `1 Tf` and a text matrix 14 times larger. Page 3 has two sections on one row.
fn styled_headings_fixture() -> Vec<u8> {
    let page1 = "BT /FB 18 Tf 72 700 Td (Chapter One) Tj ET BT /FR 11 Tf 72 670 Td (Body text) Tj ET \
                 BT /FB 14 Tf 72 640 Td (Section A) Tj ET BT /FR 11 Tf 72 620 Td (More body) Tj ET";
    let page2 = "BT /FS 18 Tf 72 700 Td (Chapter Two) Tj ET BT /FS 18 Tf 72 680 Td (and its subtitle) Tj ET \
                 BT /FB 1 Tf 14 0 0 14 72 640 Tm (Section B) Tj ET BT /FR 11 Tf 72 620 Td (Body) Tj ET \
                 BT /FB 14 Tf 72 600 Td (Section C) Tj ET";
    // Two sections on one row, the right one drawn first: read from the left.
    let page3 = "BT /FB 14 Tf 300 500 Td (Right part) Tj ET BT /FB 14 Tf 72 500 Td (Left part) Tj ET";
    let fonts = "/Font << /FB 6 0 R /FR 7 0 R /FS 8 0 R >>";
    pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R >>")
        .obj(format!("<< /Type /Pages /Kids [3 0 R 4 0 R 10 0 R] /Count 3 /MediaBox [0 0 612 792] /Resources << {fonts} >> >>"))
        .obj("<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>")
        .obj("<< /Type /Page /Parent 2 0 R /Contents 9 0 R >>")
        .stream("", page1)
        .obj("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
        .obj("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
        .obj("<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+Helvetica-Bold /Encoding /WinAnsiEncoding >>")
        .stream("", page2)
        .obj("<< /Type /Page /Parent 2 0 R /Contents 11 0 R >>")
        .stream("", page3)
        .build()
}

/// `style` at another size.
fn pdfcraft_engine_style(style: &text_style_bookmarks::TextStyle, size: f64) -> text_style_bookmarks::TextStyle {
    text_style_bookmarks::TextStyle { size, ..style.clone() }
}

/// RGBA of the pixel at PDF point (x, y) on `page`, rendered at 1 px/pt (page height 300).
fn pixel(s: &Session, id: DocId, page: usize, x: u32, y: u32) -> [u8; 4] {
    let doc = s.get(id).unwrap();
    let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), doc.config.clone());
    let out = r.render(pdfcraft_render::RenderRequest { page, scale: 1.0, ..Default::default() });
    assert!(out.error.is_none(), "{:?}", out.error);
    let i = (((300 - y) * out.width + x) * 4) as usize;
    out.rgba[i..i + 4].try_into().unwrap()
}

fn rect_comment(page: usize, rect: [f64; 4]) -> Edit {
    Edit::AddAnnotation(NewAnnotation {
        page,
        shape: Shape::Rectangle { rect },
        style: Style { color: [1.0, 0.0, 0.0], opacity: 1.0, width: 2.0, fill: Some([1.0, 0.0, 0.0]), ..Style::default() },
        contents: "Look here".into(),
        author: "Reviewer".into(),
    })
}

fn protection(open: Option<&str>, perms: Option<&str>) -> Protection {
    Protection { open_password: open.map(Into::into), permissions_password: perms.map(Into::into), ..Protection::default() }
}

/// Protect a fresh document with `p` as the app does: the edit's result and, when it is
/// accepted, how the saved file authenticates with no password, the open password and the
/// permissions password (`None`: it doesn't open).
fn protect_and_reopen(p: &Protection) -> (Result<(), EditError>, Option<[Option<pdfcraft_cos::Auth>; 3]>) {
    let (mut s, id) = session_with(1);
    let result = s.apply(id, Edit::Protect(p.clone()));
    let reopened = result.is_ok().then(|| {
        let saved = s.save_bytes(id).unwrap();
        let auth = |pw: Option<&str>| pdfcraft_cos::Document::open_with_password(saved.clone(), pw).ok()?.security().map(|h| h.auth());
        [auth(None), auth(p.open_password.as_deref()), auth(p.permissions_password.as_deref())]
    });
    (result, reopened)
}

/// A one-page form: a text field and a check box (with appearance states).
fn form_fixture() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [20 250 180 270] /P 3 0 R /MK << /BC [0 0 0] >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (ok) /V /Off /AS /Off /Rect [20 200 35 215] /P 3 0 R /AP << /N << /Yes 7 0 R /Off 7 0 R >> >> >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        "<< /Length 0 >>\nstream\n\nendstream",
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

/// Whether `needle` appears in `bytes`, or in any stream of the PDF they hold once decoded (so
/// compressed object streams are searched too), as literal or hex string bytes.
fn appears_in_pdf(bytes: &Arc<Vec<u8>>, needle: &str) -> bool {
    let hex: String = needle.bytes().map(|b| format!("{b:02x}")).collect();
    let forms = [needle.as_bytes().to_vec(), hex.clone().into_bytes(), hex.to_uppercase().into_bytes()];
    let found = |hay: &[u8]| forms.iter().any(|f| hay.windows(f.len()).any(|w| w == f.as_slice()));
    if found(bytes) {
        return true;
    }
    let doc = pdfcraft_cos::Document::open(bytes.clone()).unwrap();
    doc.object_numbers().into_iter().any(|num| match &*doc.get(pdfcraft_cos::ObjRef { num, generation: 0 }) {
        pdfcraft_cos::Object::Stream(st) => st.decoded().is_ok_and(|d| found(&d)),
        _ => false,
    })
}

/// One 612×300 page with the content stream `body`. Object 4 is Helvetica; `resources` is what
/// the page's /Resources holds.
fn one_line_page(resources: &str, body: &str) -> Vec<u8> {
    one_line_page_with(resources, "", body)
}

/// [`one_line_page`] with `stream_entries` added to the content stream's dictionary.
fn one_line_page_with(resources: &str, stream_entries: &str, body: &str) -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 300] >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /Resources << {resources} >> /Contents 5 0 R >>"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        format!("<< /Length {} {stream_entries} >>\nstream\n{body}\nendstream", body.len()),
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

/// Put a redaction mark over `needle` where page 1 shows it, as Find text and redact does.
fn mark_shown_text(s: &mut Session, id: DocId, needle: &str) {
    let doc = s.get(id).unwrap();
    let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), pdfcraft_render::RenderConfig::default());
    let text = r.render(RenderRequestFor::text(0)).text.expect("the page has text");
    let range = text.find(needle).pop().expect("the page shows the text");
    let info = &doc.info.pages[0];
    let quads = text.line_rects(range).into_iter().map(|v| pdfcraft_annot::rect_quad(info.view_rect_to_user(v).map(f64::from))).collect();
    let shape = Shape::Redact { quads, overlay: String::new(), look: Default::default() };
    let mark =
        Edit::AddAnnotation(NewAnnotation { page: 0, style: Style::default_for(&shape), shape, contents: String::new(), author: "Ada".into() });
    s.apply(id, mark).unwrap();
}

/// What [`mark_with_reply`] writes that must not reach a file once the mark is removed.
const MARK_TRACES: [&str; 5] = ["/Redact", "MARKED OVERLAY", "Mark Author", "REPLY TO THE MARK", "Replier"];

/// A redaction mark on page 1 (the first annotation there) and a reply to it.
fn mark_with_reply(s: &mut Session, id: DocId) {
    let shape = Shape::Redact {
        quads: vec![pdfcraft_annot::rect_quad([80.0, 140.0, 100.0, 180.0])],
        overlay: "MARKED OVERLAY".into(),
        look: Default::default(),
    };
    let mark = Edit::AddAnnotation(NewAnnotation {
        page: 0,
        style: Style::default_for(&shape),
        shape,
        contents: String::new(),
        author: "Mark Author".into(),
    });
    s.apply(id, mark).unwrap();
    let reply = Edit::ReplyToAnnotation { page: 0, index: 0, text: "REPLY TO THE MARK".into(), author: "Replier".into() };
    s.apply(id, reply).unwrap();
}

/// A one-page document whose red square is in a layer; `off` switches the layer off by default.
fn red_layer_pdf(off: bool) -> Vec<u8> {
    let content = "/OC /L1 BDC 1 0 0 rg 0 0 100 100 re f EMC";
    let off = if off { "/OFF [5 0 R] " } else { "" };
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R] /D << {off}/Order [5 0 R] >> >> >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources << /Properties << /L1 5 0 R >> >> >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n\
         5 0 obj << /Type /OCG /Name (Red) >> endobj\n\
         trailer << /Root 1 0 R >>\n%%EOF",
        content.len()
    )
    .into_bytes()
}

/// Does the first page of `pdf` show any red when drawn?
fn first_page_has_red(pdf: Vec<u8>) -> bool {
    let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(pdf), pdfcraft_render::RenderConfig::default());
    let page = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 0.5, ..Default::default() });
    page.rgba.as_chunks::<4>().0.iter().any(|p| p[0] > 200 && p[1] < 60 && p[2] < 60)
}

/// One 100 × 100 page under a background (`/PCMark /Background`), its own content painting a
/// white backdrop first. With `copy`, that content is a backdrop copy whose tag is `copy`.
fn backdrop_pdf(copy: Option<&str>) -> Vec<u8> {
    let bg = "q 1 0 0 rg 0 0 100 100 re f Q";
    let (tag, fill) = match copy {
        Some(tag) => (format!("/PCBackdrop {tag} "), "n"),
        None => (String::new(), "f"),
    };
    let content = format!("1 g 0 0 100 100 re {fill} 0 g BT /F1 12 Tf 10 50 Td (Hi) Tj ET");
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents [4 0 R 5 0 R] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >> endobj\n\
         4 0 obj << /PCMark /Background /Length {} >> stream\n{bg}\nendstream endobj\n\
         5 0 obj << {tag}/Length {} >> stream\n{content}\nendstream endobj\n\
         trailer << /Root 1 0 R >>\n%%EOF",
        bg.len(),
        content.len()
    )
    .into_bytes()
}

fn metadata_and_comment_edits_keep_the_indexes(session: &mut Session, id: DocId) {
    session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Changed".into() }).unwrap();
    assert!(session.get(id).unwrap().links().is_empty());
    assert!(session.get(id).unwrap().marks().is_empty());
    assert_eq!(secondary_scan_counts(), (1, 1), "metadata does not invalidate either index");

    session.apply(id, rect_comment(0, [20.0, 20.0, 60.0, 60.0])).unwrap();
    assert!(session.get(id).unwrap().links().is_empty());
    assert_eq!(secondary_scan_counts(), (1, 2), "comment edits invalidate the annotation-position link index");
}

fn link_edits_rebuild_only_the_link_list(session: &mut Session, id: DocId) {
    session
        .apply(
            id,
            Edit::AddLink {
                page: 0,
                rect: [10.0, 10.0, 80.0, 30.0],
                action: LinkAction::Uri("https://example.org".into()),
                style: LinkStyle::default(),
            },
        )
        .unwrap();
    assert_eq!(secondary_scan_counts(), (1, 2), "a link edit invalidates only the link list");
    assert_eq!(session.get(id).unwrap().links()[0].index, 1, "link indexes include earlier annotations");
    assert_eq!(secondary_scan_counts(), (1, 3));
    session.undo(id).unwrap();
    assert_eq!(session.get(id).unwrap().links().len(), 0);
    assert_eq!(secondary_scan_counts(), (1, 4), "undo rebuilds the link list once");
    session.redo(id).unwrap();
    assert_eq!(session.get(id).unwrap().links()[0].index, 1);
    assert_eq!(secondary_scan_counts(), (1, 5), "redo rebuilds the link list once");
    session.apply(id, Edit::DeleteAnnotation { page: 0, index: 0 }).unwrap();
    assert_eq!(session.get(id).unwrap().links()[0].index, 0, "deleting an earlier annotation shifts link indexes");
    assert_eq!(secondary_scan_counts(), (1, 6), "comment deletion rebuilds annotation-position indexes");
}

fn content_and_marker_edits_rebuild_the_markers(session: &mut Session, id: DocId) {
    session
        .apply(id, Edit::AddText { page: 0, text: AddedText { rect: [20.0, 250.0, 100.0, 280.0], text: "Added".into(), ..Default::default() } })
        .unwrap();
    assert_eq!(secondary_scan_counts(), (1, 6), "content edits defer rebuilding until a marker getter is used");
    assert_eq!(session.get(id).unwrap().added().len(), 1);
    assert_eq!(session.get(id).unwrap().links().len(), 1);
    assert_eq!(secondary_scan_counts(), (2, 6), "content edits preserve the link cache");

    let mut header = HeaderFooter::default();
    header.text[0] = "Header".into();
    session.apply(id, Edit::AddHeaderFooter { pages: vec![0], settings: header, replace: false }).unwrap();
    assert_eq!(session.get(id).unwrap().marks(), [MarkKind::HeaderFooter]);
    assert_eq!(secondary_scan_counts(), (3, 6));
    session.undo(id).unwrap();
    assert!(session.get(id).unwrap().marks().is_empty());
    assert_eq!(secondary_scan_counts(), (4, 6), "undo invalidates marker state");
    session.undo(id).unwrap();
    assert!(session.get(id).unwrap().added().is_empty());
    assert_eq!(secondary_scan_counts(), (5, 6), "undo of added content invalidates marker state");
    session.redo(id).unwrap();
    assert_eq!(session.get(id).unwrap().added().len(), 1);
    assert_eq!(secondary_scan_counts(), (6, 6), "redo of added content rebuilds once");
    session.redo(id).unwrap();
    assert_eq!(session.get(id).unwrap().marks(), [MarkKind::HeaderFooter]);
    assert_eq!(secondary_scan_counts(), (7, 6), "redo of marks rebuilds once");
}

fn fill_item(shape: Shape, contents: &str) -> Edit {
    let style = Style::default_for(&shape);
    Edit::AddAnnotation(NewAnnotation { page: 0, shape, style, contents: contents.into(), author: "Ada".into() })
}

fn comment_text(s: &Session, id: DocId) -> Vec<String> {
    let mut t: Vec<String> = s.get(id).unwrap().info.annotations.iter().map(|a| a.contents.clone().unwrap_or_default()).collect();
    t.sort();
    t
}

/// A TIFF's resolution unit (2 = inch) and its horizontal and vertical resolution.
fn tiff_resolution(bytes: &[u8]) -> (u32, f64, f64) {
    use tiff::{decoder::ifd::Value, tags::Tag};
    let mut d = tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).unwrap();
    let mut res = |tag| match d.get_tag(tag).unwrap() {
        Value::Rational(n, d) => f64::from(n) / f64::from(d),
        v => panic!("{tag:?} is {v:?}"),
    };
    let (x, y) = (res(Tag::XResolution), res(Tag::YResolution));
    (d.get_tag_u32(Tag::ResolutionUnit).unwrap(), x, y)
}

/// ByteRange placeholder values: fixed width, patched after writing.
const LAYERED_BR_MARK: [i64; 3] = [1_111_111_111, 2_222_222_222, 3_333_333_333];

/// A one-page file whose signature field carries a layered appearance on the
/// page (background, words, vector glyph, border, unverified caption): object
/// numbers 1 catalog, 2 pages, 3 page, 4 field, 5 AcroForm, 6 /N, 7 frame,
/// 8 background, 9 words, 10 glyph, 11 caption, 12 border, 13 Helvetica,
/// 14 page content. The field is unsigned: `sign_prepared` adds the value.
fn layered_signable_fixture() -> Vec<u8> {
    fn stream(bbox: &str, resources: &str, ops: &str) -> String {
        format!("<< /Type /XObject /Subtype /Form /BBox {bbox} /Length {} /Resources {resources} >>\nstream\n{ops}\nendstream", ops.len())
    }
    let helv = "<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >>";
    let objs = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 5 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 14 0 R /Resources << /Font << /F1 13 0 R >> >> /Annots [4 0 R] >>"
            .to_string(),
        "<< /FT /Sig /T (Signature1) /Subtype /Widget /Rect [10 10 110 60] /P 3 0 R /AP << /N 6 0 R >> >>".to_string(),
        "<< /Fields [4 0 R] /SigFlags 3 >>".to_string(),
        stream("[0 0 50 30]", "<< /XObject << /frame 7 0 R >> >>", "q /frame Do Q"),
        stream(
            "[0 0 50 30]",
            "<< /XObject << /bg 8 0 R /words 9 0 R /glyph 10 0 R /bd 12 0 R /cap 11 0 R >> >>",
            "q /bg Do /words Do /glyph Do /bd Do /cap Do Q",
        ),
        stream("[0 0 100 100]", "<< >>", "0 g 0 0 100 100 re f"),
        stream("[0 0 100 100]", helv, "BT /F1 10 Tf 5 50 Td (Signed by Test) Tj ET"),
        stream("[0 0 50 30]", "<< >>", "0.8 0.1 0.1 RG 3 w 5 5 m 20 5 l 12 22 l h S"),
        stream("[0 21 50 30]", helv, "BT /F1 9 Tf 2 22 Td (Signature Not Verified) Tj ET"),
        stream("[0 0 100 100]", "<< >>", "0 g 0 0 100 100 re f"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Length 44 >>\nstream\nBT /F1 14 Tf 20 150 Td (Contract text) Tj ET\nendstream".to_string(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (k, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", k + 1).as_bytes());
    }
    let start = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for o in &offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF", objs.len() + 1).as_bytes());
    out
}

/// Whether `haystack` holds `needle`.
fn bytes_has(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Sign `doc`, whose field already carries its layered appearance, without
/// touching that appearance: reserve `/ByteRange` and `/Contents`, hash the
/// ranges and embed a detached CMS over them. The signed bytes keep the
/// prepared appearance, so they validate exactly like a product signer's file.
fn sign_prepared(doc: &mut pdfcraft_cos::Document, id: &sign::pkcs12::DigitalId, date: &str) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Object, PdfString};
    let reserve = 8192 + id.certificate.raw.len() + id.chain.iter().map(|c| c.raw.len()).sum::<usize>();
    let mut v = Dict::new();
    v.set(b"Type".to_vec(), Object::name("Sig"));
    v.set(b"Filter".to_vec(), Object::name("Adobe.PPKLite"));
    v.set(b"SubFilter".to_vec(), Object::name("ETSI.CAdES.detached"));
    v.set(b"ByteRange".to_vec(), Object::Array([0].iter().chain(LAYERED_BR_MARK.iter()).map(|n| Object::Int(*n)).collect()));
    v.set(b"Contents".to_vec(), Object::String(PdfString { bytes: vec![0; reserve], hex: true }));
    v.set(b"M".to_vec(), PdfString::literal(date.as_bytes().to_vec()));
    let sig = doc.add(Object::Dict(v));
    // The field by name, never by number: a full save (encryption, repairs) is
    // free to renumber objects, and a hardcoded reference would stamp the value
    // onto whatever moves into that slot.
    let root = doc.root().expect("catalog");
    let af = doc.get(root).as_dict().expect("catalog dict").get(b"AcroForm").cloned().expect("AcroForm");
    let fields = doc.resolve(&af).as_dict().expect("AcroForm dict").get(b"Fields").cloned().expect("Fields");
    let field = doc.resolve(&fields).as_array().expect("Fields array").iter().filter_map(|o| o.as_ref()).next().expect("signature field");
    doc.update_dict(field, |d| d.set(b"V".to_vec(), Object::Ref(sig))).unwrap();
    let save = pdfcraft_cos::SaveOptions { object_streams: false, ..pdfcraft_cos::SaveOptions::default() };
    let mut out = pdfcraft_cos::write_incremental(doc, &save).unwrap();
    let mark = format!("0 {} {} {}", LAYERED_BR_MARK[0], LAYERED_BR_MARK[1], LAYERED_BR_MARK[2]);
    let br = out.windows(mark.len()).position(|w| w == mark.as_bytes()).expect("ByteRange placeholder");
    let start = out[..br].windows(3).rposition(|w| w == b"obj").expect("signature object");
    let end = br + out[br..].windows(6).position(|w| w == b"endobj").expect("signature end");
    let mut zeros = vec![b'<'];
    zeros.extend(std::iter::repeat_n(b'0', reserve * 2));
    zeros.push(b'>');
    let region = &out[start..end];
    let c = region.windows(zeros.len()).position(|w| w == zeros.as_slice()).expect("Contents placeholder");
    let (cstart, cend) = (start + c, start + c + zeros.len());
    let mut br_text = format!("0 {cstart} {cend} {}", out.len() - cend).into_bytes();
    assert!(br_text.len() <= mark.len(), "ByteRange patch fits its placeholder");
    br_text.resize(mark.len(), b' ');
    out[br..br + mark.len()].copy_from_slice(&br_text);
    let alg = id.key.preferred_digest();
    let digest = alg.digest(&[&out[..cstart], &out[cend..]]);
    let cms = sign::cms::sign_detached(&id.key, &id.certificate, &id.chain, alg, &digest).unwrap();
    assert!(cms.len() <= reserve, "signature fits its placeholder");
    let hex: Vec<u8> = cms.iter().flat_map(|b| format!("{b:02X}").into_bytes()).collect();
    out[cstart + 1..cstart + 1 + hex.len()].copy_from_slice(&hex);
    out
}

/// Replace one synthetic appearance stream while retaining its resource dictionary.
fn signature_layer_ops(doc: &mut pdfcraft_cos::Document, number: u32, ops: &[u8]) {
    let r = pdfcraft_cos::ObjRef::new(number, 0);
    let dict = doc.resolve(&pdfcraft_cos::Object::Ref(r)).as_dict().unwrap().clone();
    doc.set(r, pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::from_raw(dict, ops.to_vec())));
}

fn printed_pixels(bytes: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(bytes), Default::default());
    let r = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(r.error.is_none(), "{:?}", r.error);
    (r.width, r.height, r.rgba.to_vec())
}

/// Validation belongs to viewer chrome, not to document artwork or export sources.
fn assert_signature_artwork_survives_trust(mut cos: pdfcraft_cos::Document, expected_text: &[&str]) {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let identity = sign::pkcs12::open(&p12, "test").unwrap();
    let anchor = identity.chain.first().cloned().unwrap_or_else(|| identity.certificate.clone());
    let signed = Arc::new(sign_prepared(&mut cos, &identity, "D:20261002120000+01'00'"));
    let mut session = Session::new().with_clock(|| 1_800_000_000);
    let id = session.open("artwork.pdf", None, signed.clone(), None).unwrap();
    let original_display = session.get(id).unwrap().display.clone();
    let mut exporter = export::Exporter::new(session.get(id).unwrap());
    let original_text = exporter.text_of(&[0]).unwrap();
    let original_png = exporter.png(0, 72.0).unwrap();
    for text in expected_text {
        assert!(original_text.contains(text), "fixture text {text:?} must be exported: {original_text:?}");
    }
    let mut settings = print::Settings { pages: vec![0], paper: (200.0, 200.0), ..Default::default() };
    let original_vector_print = printed_pixels(session.print_pdf(id, &settings).unwrap());
    settings.raster = Some(print::Raster { dpi: 72.0, grayscale: false });
    let original_raster_print = printed_pixels(session.print_pdf(id, &settings).unwrap());
    session.set_trusted_certificates(vec![anchor.clone()]);
    let doc = session.get(id).unwrap();
    assert_eq!(doc.signatures[0].status, SignatureStatus::Valid);
    assert!(doc.display == original_display, "trust must preserve all original appearance bytes");
    let mut exporter = export::Exporter::new(doc);
    assert_eq!(exporter.text_of(&[0]).unwrap(), original_text, "text exports retain signer details and baked caption");
    assert_eq!(exporter.png(0, 72.0).unwrap(), original_png, "image exports exclude the viewer overlay");
    assert_eq!(printed_pixels(session.print_pdf(id, &settings).unwrap()), original_raster_print, "Print as Image retains document artwork");
    settings.raster = None;
    assert_eq!(printed_pixels(session.print_pdf(id, &settings).unwrap()), original_vector_print);
    assert_eq!(session.save_bytes(id).unwrap(), signed, "unchanged saves retain the signed file exactly");
    session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Preserved artwork".into() }).unwrap();
    let saved = session.save_bytes(id).unwrap();
    assert!(saved.starts_with(&signed), "a real save appends without replacing signed appearance streams");
    let mut reopened = Session::new().with_clock(|| 1_800_000_000);
    reopened.set_trusted_certificates(vec![anchor]);
    let id = reopened.open("saved.pdf", None, saved, None).unwrap();
    let doc = reopened.get(id).unwrap();
    assert_eq!(doc.signatures[0].status, SignatureStatus::Valid);
    let mut exporter = export::Exporter::new(doc);
    assert_eq!(exporter.text_of(&[0]).unwrap(), original_text);
    assert_eq!(exporter.png(0, 72.0).unwrap(), original_png);
}

/// A real layered signature through `Session`: validity, original artwork,
/// rendering, saving and reopening. `password` opens the signed bytes.
fn check_layered_round_trip(signed: Arc<Vec<u8>>, password: Option<&str>, anchor: &sign::x509::Certificate) {
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let vid = s.open("layered.pdf", None, signed.clone(), password).unwrap();
    let doc = s.get(vid).unwrap();
    let sig = &doc.signatures[0];
    assert!(sig.signed && sig.field == "Signature1" && sig.page == Some(0) && sig.visible);
    assert_eq!(sig.status, SignatureStatus::Unknown, "untrusted at first");
    // Trust changes the verdict while preserving every appearance byte. The page
    // overlay is UI chrome and is not part of the document's display/export source.
    s.set_trusted_certificates(vec![anchor.clone()]);
    let doc = s.get(vid).unwrap();
    assert_eq!(doc.signatures[0].status, SignatureStatus::Valid);
    assert!(Arc::ptr_eq(&doc.display, &doc.bytes), "trust does not build a signature display copy");
    assert!(bytes_has(&doc.display, b"Not Verified"));
    assert!(!bytes_has(&doc.display, b"Signature valid"));
    let mut renderer = pdfcraft_render::PageRenderer::new(doc.display.clone(), doc.config.clone());
    let rendered = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(rendered.error.is_none(), "{:?}", rendered.error);
    // Saving keeps the original appearance and the signature: the validated
    // state never reaches the file.
    let saved = s.save_bytes(vid).unwrap();
    assert!(bytes_has(&saved, b"Not Verified"), "saved bytes keep the baked appearance");
    assert!(!bytes_has(&saved, b"Signature valid"));
    let mut t = Session::new().with_clock(|| 1_800_000_000);
    t.set_trusted_certificates(vec![anchor.clone()]);
    let tid = t.open("layered-saved.pdf", None, saved.clone(), password).unwrap();
    let rsig = &t.get(tid).unwrap().signatures[0];
    assert_eq!(rsig.status, SignatureStatus::Valid);
}

/// A trusted layered signature whose original appearance remains document artwork.
fn trusted_layered_session() -> (Session, DocId) {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let id = sign::pkcs12::open(&p12, "test").unwrap();
    let anchor = id.chain.first().cloned().unwrap_or_else(|| id.certificate.clone());
    let mut doc = pdfcraft_cos::Document::open(Arc::new(layered_signable_fixture())).unwrap();
    let signed = Arc::new(sign_prepared(&mut doc, &id, "D:20261002120000+01'00'"));
    let mut s = Session::new().with_clock(|| 1_800_000_000);
    let vid = s.open("layered.pdf", None, signed, None).unwrap();
    s.set_trusted_certificates(vec![anchor]);
    assert_eq!(s.get(vid).unwrap().signatures[0].status, SignatureStatus::Valid);
    (s, vid)
}

/// A tiny uncompressed BMP of one colour.
fn bmp(w: u32, h: u32, bgr: [u8; 3]) -> Vec<u8> {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut b = b"BM".to_vec();
    b.extend(size.to_le_bytes());
    b.extend([0u8; 4]);
    b.extend(54u32.to_le_bytes());
    b.extend(40u32.to_le_bytes());
    b.extend((w as i32).to_le_bytes());
    b.extend((h as i32).to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(24u16.to_le_bytes());
    b.extend([0u8; 24]);
    for _ in 0..h {
        for _ in 0..w {
            b.extend(bgr);
        }
        b.extend(vec![0u8; (row - w * 3) as usize]);
    }
    b
}

const SYNTHETIC_SCAN_DPI: f64 = 300.0;

/// A scan of the OCR sentence at the source page's physical size, without a text layer.
fn synthetic_ocr_scan(s: &mut Session) -> (DocId, [f32; 2]) {
    let text = s.create_from_text("t", "The quick brown fox jumps over the lazy dog.").unwrap();
    let id = s.open("text.pdf", None, text, None).unwrap();
    let original = &s.get(id).unwrap().info.pages[0];
    let size = [original.width, original.height];
    // A typical 300 DPI scan keeps the small leading glyph in "quick" legible to OCR.
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, SYNTHETIC_SCAN_DPI).unwrap();
    // Exported PNGs have no resolution metadata. The 72 DPI import default would enlarge
    // the page and change the PDF-point coordinates the placement assertions exercise.
    let scan = s.create_from_images_with_resolution(&[("scan.png".into(), png)], ImageResolution::Dpi(SYNTHETIC_SCAN_DPI)).unwrap();
    (s.open("scan.pdf", None, scan, None).unwrap(), size)
}

/// A form whose scripts are custom JavaScript: total = price × qty (calculate, through a
/// document-level function), shown with a custom format; qty is validated; a button script.
fn scripted_form() -> Vec<u8> {
    scripted_form_with("function times\\(a, b\\) { console.println('times'); return a * b; }")
}

/// [`scripted_form`] with `helpers` (PDF string syntax) as its document-level script.
fn scripted_form_with(helpers: &str) -> Vec<u8> {
    let text = |name: &str, y: u32, aa: &str| {
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /Rect [20 {y} 180 {}] /P 3 0 R /F 4 {aa} >>", y + 20)
    };
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /Names << /JavaScript << /Names [(helpers) 10 0 R] >> >> /AcroForm << /Fields [4 0 R 5 0 R 6 0 R 7 0 R] /CO [6 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R 6 0 R 7 0 R] >>".to_string(),
        text("price", 250, "/V (2.5)"),
        text("qty", 220, "/AA << /V << /S /JavaScript /JS (if (event.value > 100) { app.alert('At most 100'); event.rc = false; }) >> >>"),
        text("total", 190, "/AA << /C << /S /JavaScript /JS (event.value = times\\(getField('price').value, getField('qty').value\\);) >> /F << /S /JavaScript /JS (event.value = 'USD ' + util.printf('%.2f', event.value);) >> >>"),
        "<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 65536 /T (go) /Rect [20 150 80 170] /P 3 0 R /F 4 >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        "<< >>".to_string(),
        format!("<< /S /JavaScript /JS ({helpers}) >>"),
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

/// A printed form, 420 × 300 pt: a title, a label with a rule to write on, a box with a small
/// label in its corner, an empty box with its label above, and two check boxes.
fn paper_form() -> Vec<u8> {
    let content = "BT /F1 14 Tf 30 270 Td (Gewerbe-Ummeldung) Tj ET \
        BT /F1 10 Tf 30 230 Td (Name:) Tj ET 0.75 w 80 228 m 380 228 l S \
        1 w 30 150 350 50 re S BT /F1 7 Tf 34 191 Td (Anschrift) Tj ET \
        BT /F1 8 Tf 30 124 Td (Telefon) Tj ET 30 90 350 30 re S \
        30 50 10 10 re S BT /F1 9 Tf 46 51 Td (Haupt) Tj ET 150 50 10 10 re S BT /F1 9 Tf 166 51 Td (Zweig) Tj ET";
    let objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 420 300] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// How the paper form was scanned.
#[derive(Clone, Copy, Debug)]
enum Scan {
    Straight,
    /// Stored sideways and turned upright by `/Rotate 90`.
    Sideways,
    /// Speckled paper, uneven light and lines that lose some pixels.
    Noisy,
    /// Laid on the scanner a degree askew.
    Skewed,
}

/// The paper form as an image-only PDF at 150 dpi (the page keeps its 420 × 300 pt).
fn scanned_form(scan: Scan) -> Arc<Vec<u8>> {
    let mut s = Session::new().with_clock(|| 1_700_000_000);
    let id = s.open("paper.pdf", None, Arc::new(paper_form()), None).unwrap();
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, 150.0).unwrap();
    let mut img = image::load_from_memory(&png).unwrap().to_rgba8();
    match scan {
        Scan::Straight => {}
        Scan::Sideways => img = image::imageops::rotate270(&img),
        Scan::Skewed => {
            // Turned a degree about the centre (nearest pixel; the corners stay paper).
            let (w, h) = (img.width() as f64, img.height() as f64);
            let (sin, cos) = 1f64.to_radians().sin_cos();
            let src = img.clone();
            for (x, y, p) in img.enumerate_pixels_mut() {
                let (dx, dy) = (x as f64 - w / 2.0, y as f64 - h / 2.0);
                let (sx, sy) = (cos * dx + sin * dy + w / 2.0, -sin * dx + cos * dy + h / 2.0);
                *p = if sx >= 0.0 && sy >= 0.0 && sx < w && sy < h { *src.get_pixel(sx as u32, sy as u32) } else { image::Rgba([255; 4]) };
            }
        }
        Scan::Noisy => {
            let (w, mut seed) = (img.width(), 7u32);
            for (x, _, p) in img.enumerate_pixels_mut() {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let noise = ((seed >> 16) % 48) as u8;
                let shade = (x * 50 / w) as u8;
                for c in &mut p.0[..3] {
                    *c = if *c < 128 && noise < 4 { 230 } else { c.saturating_sub(shade).saturating_sub(noise) };
                }
            }
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let pdf = s.create_from_images_with_resolution(&[("scan.png".into(), bytes.into_inner())], ImageResolution::Dpi(150.0)).unwrap();
    if !matches!(scan, Scan::Sideways) {
        return pdf;
    }
    let id = s.open("scan.pdf", None, pdf, None).unwrap();
    s.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    s.save_bytes(id).unwrap()
}

/// Streams in `bytes` (every revision) that hold an XFA datasets packet.
fn datasets_streams(bytes: &Arc<Vec<u8>>) -> usize {
    let cos = pdfcraft_cos::Document::open(bytes.clone()).unwrap();
    cos.object_numbers()
        .into_iter()
        .filter(|&n| {
            cos.try_get(n).ok().is_some_and(|o| match &*o {
                pdfcraft_cos::Object::Stream(s) => s.decoded_within(1 << 24).is_ok_and(|b| b.windows(9).any(|w| w == b"datasets>")),
                _ => false,
            })
        })
        .count()
}

/// The page view's raster of `page` at 1 px/pt (the document's own render pool).
fn shown(s: &Session, id: DocId, page: usize) -> pdfcraft_render::RenderedPage {
    let doc = s.get(id).unwrap();
    doc.renderer.set_queue(vec![pdfcraft_render::RenderRequest { page, scale: 1.0, ..Default::default() }]);
    let start = std::time::Instant::now();
    loop {
        if let Some(p) = doc.renderer.try_recv() {
            assert!(p.error.is_none(), "{:?}", p.error);
            return p;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(20), "no render");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A stamp's `/Rect` and the colour family expected inside it.
type StampRect = ([f64; 4], char);

/// Rubber stamps without an appearance (ISO 32000-2 Table 184): the fourteen standard names, one
/// without `/Name`, one custom name, and last a Draft stamp with artwork of its own (magenta).
/// Each sits in a 120 × 60 `/Rect` on a 612 × 792 page; the name and the expected colour family
/// (`'r'`, `'g'`, `'b'`, or `'n'` for the neutral placeholder) of each, in page order.
fn stamps_without_appearances() -> (Arc<Vec<u8>>, Vec<StampRect>) {
    let stamps: [(Option<&str>, char); 16] = [
        (Some("Approved"), 'g'),
        (Some("Experimental"), 'b'),
        (Some("NotApproved"), 'r'),
        (Some("AsIs"), 'b'),
        (Some("Expired"), 'r'),
        (Some("NotForPublicRelease"), 'r'),
        (Some("Confidential"), 'r'),
        (Some("Final"), 'g'),
        (Some("Sold"), 'g'),
        (Some("Departmental"), 'b'),
        (Some("ForComment"), 'b'),
        (Some("TopSecret"), 'r'),
        (Some("Draft"), 'b'),
        (Some("ForPublicRelease"), 'b'),
        (None, 'b'),
        (Some("MyCustomStamp"), 'n'),
    ];
    let mut objs: Vec<String> =
        vec!["<< /Type /Catalog /Pages 2 0 R >>".into(), "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(), String::new()];
    let mut expected = Vec::new();
    for (i, (name, family)) in stamps.iter().enumerate() {
        let (x, y) = (30 + (i % 4) * 145, 660 - (i / 4) * 150);
        let rect = [x as f64, y as f64, x as f64 + 120.0, y as f64 + 60.0];
        let name = name.map(|n| format!("/Name /{n}")).unwrap_or_default();
        objs.push(format!("<< /Type /Annot /Subtype /Stamp /Rect [{} {} {} {}] {name} /F 4 >>", rect[0], rect[1], rect[2], rect[3]));
        expected.push((rect, *family));
    }
    let own = objs.len() + 2;
    objs.push(format!("<< /Type /Annot /Subtype /Stamp /Rect [30 60 150 120] /Name /Draft /F 4 /AP << /N {own} 0 R >> >>"));
    expected.push(([30.0, 60.0, 150.0, 120.0], 'm'));
    objs.push("<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 23 >>\nstream\n1 0 1 rg 0 0 10 10 re f\nendstream".into());
    let annots: Vec<String> = (4..own).map(|n| format!("{n} 0 R")).collect();
    objs[2] = format!("<< /Type /Page /Parent 2 0 R /Annots [{}] >>", annots.join(" "));
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    (Arc::new(bytes), expected)
}

/// The colour family of an RGBA pixel that isn't white: `'r'`, `'g'` or `'b'` for the dominant
/// channel, `'n'` for a grey, `'m'` for magenta.
fn colour_family(p: &[u8]) -> Option<char> {
    let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
    if r.min(g).min(b) > 200 {
        return None;
    }
    Some(if (r - g).abs() < 24 && (g - b).abs() < 24 {
        'n'
    } else if r > 200 && b > 200 && g < 80 {
        'm'
    } else if r > g && r > b {
        'r'
    } else if g > r && g > b {
        'g'
    } else {
        'b'
    })
}

/// A page with a FreeText comment that has no `/AP` stream: the smallest case behind #2371.
fn free_text_without_appearance() -> Arc<Vec<u8>> {
    let objs: Vec<&[u8]> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",
        b"<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>",
        b"<< /Type /Annot /Subtype /FreeText /Rect [20 200 180 260] /Contents (Box) /DA (/Helv 10 Tf 0 0 1 rg) /Border [0 0 1] /C [.85 .47 .02] >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(o);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    Arc::new(bytes)
}

/// AES-256 passwords as typed, each with its SASLprep form, which ISO 32000-2 keys the file on:
/// a decomposed é (NFKC composes it), a no-break space (mapped to a space) and fullwidth letters
/// (what a Japanese IME types in full-width mode; NFKC narrows them). Precomposed "café" is the
/// control: SASLprep leaves it as it is.
const SASLPREP_CHANGES: [(&str, &str); 4] =
    [("cafe\u{301}", "caf\u{E9}"), ("a\u{A0}b", "a b"), ("\u{FF41}\u{FF42}", "ab"), ("caf\u{E9}", "caf\u{E9}")];

#[test]
fn aes256_open_passwords_that_saslprep_changes_protect_and_reopen_as_typed() {
    for (typed, prepared) in SASLPREP_CHANGES {
        let (mut s, id) = session_with(1);
        // The renderer used to take the password as typed, so the protected file didn't reopen
        // and the edit was rolled back: "the password is incorrect".
        s.apply(id, Edit::Protect(protection(Some(typed), None))).unwrap_or_else(|e| panic!("{typed:?}: {e:?}"));
        assert!(s.get(id).unwrap().info.encrypted, "{typed:?}");
        assert_eq!(page_texts(&s, id), ["Page 1"], "{typed:?}");
        let saved = s.save_bytes(id).unwrap();
        for pw in [typed, prepared] {
            let mut again = Session::new();
            let reopened = again.open("x.pdf", None, saved.clone(), Some(pw)).unwrap_or_else(|e| panic!("{typed:?} as {pw:?}: {e:?}"));
            assert_eq!(page_texts(&again, reopened), ["Page 1"], "{typed:?} as {pw:?}");
        }
        assert!(matches!(Session::new().open("x.pdf", None, saved, Some("cafe")), Err(OpenError::WrongPassword)), "{typed:?}");
    }
}

/// A synthetic phone photo, 400 × 300 px: a white sheet, tilted, on a grey table, with a black
/// mark in the sheet's top-left corner. Returns the PNG and the sheet's corners in pixels.
fn camera_photo() -> (Vec<u8>, [[f64; 2]; 4]) {
    let sheet = [[60.0, 40.0], [330.0, 20.0], [350.0, 270.0], [40.0, 250.0]];
    let inside = |x: f64, y: f64| {
        (0..4).all(|i| {
            let (a, b) = (sheet[i], sheet[(i + 1) % 4]);
            (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]) >= 0.0
        })
    };
    let img = image::RgbImage::from_fn(400, 300, |x, y| {
        let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
        if (75.0..95.0).contains(&fx) && (55.0..75.0).contains(&fy) {
            image::Rgb([0, 0, 0])
        } else if inside(fx, fy) {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([110, 110, 110])
        }
    });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    (png, sheet)
}

/// `fixture(2)` with the catalog pointing at a page tree object the file does not have (#1656).
fn fixture_without_page_tree() -> Vec<u8> {
    let bytes = fixture(2);
    let catalog = b"<< /Type /Catalog /Pages 2 0 R >>";
    let at = bytes.windows(catalog.len()).position(|w| w == catalog).unwrap();
    let mut out = bytes.clone();
    out[at..at + catalog.len()].copy_from_slice(b"<< /Type /Catalog /Pages 9 0 R >>");
    out
}

/// A one-page PDF: a JPEG logo (DCTDecode) at the top and a big text title below it.
fn jpeg_cover_pdf(logo_y: i32) -> Vec<u8> {
    let mut jpeg = Vec::new();
    let img = image::RgbImage::from_fn(40, 20, |x, y| image::Rgb([(x * 6) as u8, (y * 12) as u8, 90]));
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode_image(&img).unwrap();
    let content = format!(
        "q 200 0 0 100 72 {logo_y} cm /Im1 Do Q BT /F1 30 Tf 72 580 Td (Annual Report) Tj ET \
BT /F1 11 Tf 72 500 Td (Body text that is long enough to count as the body size of this page.) Tj ET"
    );
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    let mut add = |pdf: &mut Vec<u8>, n: usize, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    add(&mut pdf, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    add(&mut pdf, 2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    add(
        &mut pdf,
        3,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> >>",
    );
    add(&mut pdf, 4, format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes());
    add(&mut pdf, 5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    let mut obj = format!("<< /Type /XObject /Subtype /Image /Width 40 /Height 20 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", jpeg.len()).into_bytes();
    obj.extend_from_slice(&jpeg);
    obj.extend_from_slice(b"\nendstream");
    add(&mut pdf, 6, &obj);
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    pdf
}

/// A 60 x 20 JPEG at 144 x 72 dpi, tagged with EXIF Orientation 6 (shown turned 90 degrees clockwise).
fn phone_jpeg() -> Vec<u8> {
    use image::codecs::jpeg::{JpegEncoder, PixelDensity, PixelDensityUnit};
    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new(&mut jpeg);
    encoder.set_pixel_density(PixelDensity { density: (144, 72), unit: PixelDensityUnit::Inches });
    encoder.encode_image(&image::RgbImage::from_pixel(60, 20, image::Rgb([20, 20, 20]))).unwrap();
    let mut exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    exif.extend_from_slice(&[6, 0, 0, 0, 0, 0, 0, 0]);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xff, 0xe1]);
    out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&exif);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// Undo/redo of a metadata edit rolled back after a refresh panic.
fn rolls_back_metadata_scope(s: &mut Session, id: DocId) {
    // 5. Metadata scope: a panic after `doc.info.title` was written rolls back `info.title` and keeps `edit_notices`.
    s.apply(id, Edit::SetInfo { key: "Title".into(), value: "Draft".into() }).unwrap();
    let notice = TextNotice { field: None, page: 0, ch: '☺', redrawn_elsewhere: false };
    s.doc_mut(id).unwrap().edit_notices.push(EditNotice::Undrawn(notice.clone()));
    panic_on_refresh(1);
    assert_eq!(s.undo(id), Err(EditError::Reopen("injected refresh panic".into())));
    assert_eq!(s.get(id).unwrap().info.title.as_deref(), Some("Draft"));
    assert_eq!(s.get(id).unwrap().edit_notices, [EditNotice::Undrawn(notice)]);
    assert_eq!(s.undo(id).unwrap().as_str(), "Change Title");
    assert_eq!(s.get(id).unwrap().info.title, None);
    assert!(s.get(id).unwrap().edit_notices.is_empty());

    panic_on_refresh(1);
    assert_eq!(s.redo(id), Err(EditError::Reopen("injected refresh panic".into())));
    assert_eq!(s.get(id).unwrap().info.title, None);
    assert_eq!(s.redo(id).unwrap().as_str(), "Change Title");
    assert_eq!(s.get(id).unwrap().info.title.as_deref(), Some("Draft"));
}

/// Undo/redo of a protection edit rolled back after a refresh panic.
fn rolls_back_protection_keys(s: &mut Session, id: DocId) {
    // 6. Protection keys: undo/redo rollback restores `editor.keys` and `doc.password` before re-inspecting.
    let protect = Protection { open_password: Some("u".into()), permissions_password: Some("o".into()), ..Protection::default() };
    s.apply(id, Edit::Protect(protect)).unwrap();
    assert!(s.get(id).unwrap().info.encrypted);
    panic_on_refresh(1);
    assert_eq!(s.undo(id), Err(EditError::Reopen("injected refresh panic".into())));
    assert!(s.get(id).unwrap().info.encrypted);
    assert_eq!(s.get(id).unwrap().password.as_deref(), Some("u"));
    assert_eq!(s.undo(id).unwrap().as_str(), "Protect with password");
    assert!(!s.get(id).unwrap().info.encrypted);
    assert_eq!(s.get(id).unwrap().password, None);

    panic_on_refresh(1);
    assert_eq!(s.redo(id), Err(EditError::Reopen("injected refresh panic".into())));
    assert!(!s.get(id).unwrap().info.encrypted);
    assert_eq!(s.get(id).unwrap().password, None);
    assert_eq!(s.redo(id).unwrap().as_str(), "Protect with password");
    assert!(s.get(id).unwrap().info.encrypted);
    assert_eq!(s.get(id).unwrap().password.as_deref(), Some("u"));
}
/// Reproducible workload with 3,129 fields and 190 ordered custom calculations.
/// No external PDF assets are needed; only in-memory contributor-original objects.
fn synthetic_acroform(field_count: usize, calculation_count: usize) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Object, PdfString};
    let mut doc = pdfcraft_cos::Document::open(Arc::new(fixture(6))).unwrap();
    let mut refs = Vec::new();
    let mut order = Vec::new();
    for i in 0..field_count {
        let name = match i {
            0 => "ranks".to_string(),
            1 => "trained".to_string(),
            2 => "bonus".to_string(),
            _ => format!("field{i}"),
        };
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::name("Annot"));
        d.set(b"Subtype".to_vec(), Object::name("Widget"));
        d.set(b"FT".to_vec(), Object::name(if i == 1 { "Btn" } else { "Tx" }));
        d.set(b"T".to_vec(), PdfString::text(&name));
        d.set(b"V".to_vec(), if i == 1 { Object::name("Off") } else { Object::String(PdfString::text("0")) });
        d.set(b"Rect".to_vec(), Object::Array([20, 20, 100, 40].map(Object::Int).to_vec()));
        d.set(b"P".to_vec(), Object::Ref(pdfcraft_cos::ObjRef::new(4, 0)));
        if (2..calculation_count + 2).contains(&i) {
            let js = if i == 2 {
                "var r = Number(getField('ranks').value); event.value = r + (r > 0 && getField('trained').isBoxChecked(0) ? 3 : 0);".to_string()
            } else {
                format!("event.value = Number(getField('{}').value) + 1;", if i == 3 { "bonus".to_string() } else { format!("field{}", i - 1) })
            };
            let mut a = Dict::new();
            a.set(b"S".to_vec(), Object::name("JavaScript"));
            a.set(b"JS".to_vec(), PdfString::text(&js));
            let mut aa = Dict::new();
            aa.set(b"C".to_vec(), Object::Dict(a));
            d.set(b"AA".to_vec(), Object::Dict(aa));
        }
        let r = doc.add(Object::Dict(d));
        refs.push(Object::Ref(r));
        if (2..calculation_count + 2).contains(&i) {
            order.push(Object::Ref(r));
        }
    }
    doc.update_dict(pdfcraft_cos::ObjRef::new(4, 0), |d| d.set(b"Annots".to_vec(), Object::Array(refs.clone()))).unwrap();
    let mut af = Dict::new();
    af.set(b"Fields".to_vec(), Object::Array(refs));
    af.set(b"CO".to_vec(), Object::Array(order));
    doc.update_dict(doc.root().unwrap(), |d| d.set(b"AcroForm".to_vec(), Object::Dict(af))).unwrap();
    pdfcraft_cos::write_incremental(&doc, &Default::default()).unwrap()
}

/// A page of `count` pictures of one flat grey, `side` pixels square, each an object of its own.
fn page_of_pictures(count: usize, side: usize) -> Vec<u8> {
    let data = pdfcraft_filters::encode_flate(&vec![0x80; side * side]);
    let names: String = (0..count).map(|i| format!("/Im{i} {} 0 R ", 5 + i)).collect();
    let content: String = (0..count).map(|i| format!("q 10 0 0 10 0 0 cm /Im{i} Do Q\n")).collect();
    let mut pdf = format!(
        "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /XObject << {names}>> >> >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n",
        content.len()
    )
    .into_bytes();
    for i in 0..count {
        let dict = format!(
            "{} 0 obj << /Type /XObject /Subtype /Image /Width {side} /Height {side} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >> stream\n",
            5 + i,
            data.len()
        );
        pdf.extend_from_slice(dict.as_bytes());
        pdf.extend_from_slice(&data);
        pdf.extend_from_slice(b"\nendstream endobj\n");
    }
    pdf.extend_from_slice(b"trailer << /Root 1 0 R >>\n%%EOF");
    pdf
}

/// A one-page 200 × 300 document whose page lists `annots` (objects 4, 5, …).
fn pdf_with_annots(annots: &[&[u8]]) -> Vec<u8> {
    let list: String = (0..annots.len()).map(|i| format!("{} 0 R ", i + 4)).collect();
    let page = format!("<< /Type /Page /Parent 2 0 R /Annots [{list}] >>");
    let mut objs: Vec<&[u8]> =
        vec![b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>", page.as_bytes()];
    objs.extend_from_slice(annots);
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend_from_slice(o);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
