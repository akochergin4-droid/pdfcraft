//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs.

use std::path::{Path, PathBuf};

#[cfg(feature = "mcp")]
use pdfcraft_automation::mcp::McpServer;
use pdfcraft_automation::{Automation, Content, ToolError, tools};
use serde_json::{Value, json};

// The tests are split by position, not by topic, so that no file runs to ten thousand lines.
// Shared fixtures and helpers stay in this file; each part reaches them through `use super::*`.
mod tools_part1;
mod tools_part2;
mod tools_part3;
mod tools_part4;
mod tools_part5;
mod tools_part6;
mod tools_part7;
mod tools_part8;

/// A PDF with `n` 200×300 pt pages reading "Page 1", "Page 2", …
fn fixture(n: usize) -> Vec<u8> {
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

fn remote_link_pdf(file: &str, destination: Option<&str>) -> Vec<u8> {
    // canonicalize() uses a Windows device prefix; a PDF file specification uses the ordinary drive path.
    #[cfg(windows)]
    let file = file.strip_prefix(r"\\?\").filter(|path| path.as_bytes().get(1) == Some(&b':')).unwrap_or(file);
    let file = file.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let destination = destination.map_or_else(String::new, |d| format!("/D {d}"));
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".to_vec(),
        format!("<< /Type /Annot /Subtype /Link /Rect [10 10 80 30] /A << /S /GoToR /F ({file}) {destination} >> >>").into_bytes(),
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
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    bytes
}

fn protected_fixture(password: &str) -> Vec<u8> {
    use pdfcraft_engine::{Edit, Protection, Session};
    let mut session = Session::new();
    let id = session.open("target.pdf", None, std::sync::Arc::new(fixture(1)), None).unwrap();
    session.apply(id, Edit::Protect(Protection { open_password: Some(password.into()), ..Default::default() })).unwrap();
    session.save_bytes(id).unwrap().as_ref().clone()
}

/// A fresh directory with `a.pdf` (3 pages) and `b.pdf` (2 pages).
#[track_caller]
fn workdir(test: &str) -> PathBuf {
    let caller = std::panic::Location::caller();
    let dir = std::env::temp_dir().join(format!("pdfcraft-automation-{test}-{}-{}-{}", std::process::id(), caller.line(), caller.column(),));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.pdf"), fixture(3)).unwrap();
    std::fs::write(dir.join("b.pdf"), fixture(2)).unwrap();
    dir
}

fn auto(dir: &Path) -> Automation {
    Automation::new().with_root(dir).unwrap().with_clock(|| 1_700_000_000)
}

/// A two-page PDF with headings set apart only by their style: chapters in Helvetica-Bold 18,
/// a section in Helvetica-Bold 14, text in Helvetica 11 (#1693).
fn styled_headings_pdf() -> Vec<u8> {
    let page1 = "BT /FB 18 Tf 72 700 Td (Introduction) Tj ET BT /FR 11 Tf 72 670 Td (Some text) Tj ET \
                 BT /FB 14 Tf 72 640 Td (Background) Tj ET";
    let page2 = "BT /FB 18 Tf 72 700 Td (Results) Tj ET BT /FR 11 Tf 72 670 Td (More text) Tj ET";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] /Resources << /Font << /FB 7 0 R /FR 8 0 R >> >> >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".into(),
        format!("<< /Length {} >>\nstream\n{page1}\nendstream", page1.len()),
        format!("<< /Length {} >>\nstream\n{page2}\nendstream", page2.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
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

/// A one-page PDF whose form holds one text field named `name`.
fn form_pdf(name: &str) -> Vec<u8> {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".into(),
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V (x) /Rect [60 60 200 80] /P 3 0 R >>"),
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

fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(mut c) => match c.remove(0) {
            Content::Json(v) => v,
            other => panic!("{tool}: expected JSON, got {other:?}"),
        },
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

fn err(a: &mut Automation, tool: &str, args: Value) -> ToolError {
    a.call(tool, &args).unwrap_err()
}

fn page_text(a: &mut Automation, doc: u64) -> Vec<String> {
    ok(a, "text_extract", json!({ "doc": doc }))["pages"].as_array().unwrap().iter().map(|p| p["text"].as_str().unwrap().trim().to_string()).collect()
}

fn assert_homogeneous_enums(schema: &Value, path: &str) {
    fn json_kind(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let first = values.first().map(json_kind);
        assert!(values.iter().map(json_kind).all(|kind| Some(kind) == first), "mixed enum types at {path}: {values:?}");
    }
    match schema {
        Value::Object(object) => {
            for (key, child) in object {
                assert_homogeneous_enums(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(array) => {
            for (index, child) in array.iter().enumerate() {
                assert_homogeneous_enums(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

/// A shallow-object destination tree: `depth` single-child nodes above `count` names.
fn destination_graph_fixture(depth: usize, count: usize) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, Object, PdfString, SaveOptions};
    let mut doc = Document::open(std::sync::Arc::new(fixture(3))).unwrap();
    let pages = pdfcraft_organize::pages(&doc).unwrap();
    let pairs = (0..count)
        .flat_map(|i| {
            let page = if i % 2 == 0 { pages[0].obj } else { pages[2].obj };
            [Object::String(PdfString::text(&format!("dest-{i:05}"))), Object::Array(vec![Object::Ref(page), Object::name("Fit")])]
        })
        .collect();
    let mut leaf = Dict::new();
    leaf.set(b"Names".to_vec(), Object::Array(pairs));
    let mut tree = doc.add(leaf);
    for _ in 0..depth {
        let mut node = Dict::new();
        node.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(tree)]));
        tree = doc.add(node);
    }
    let mut names = Dict::new();
    names.set(b"Dests".to_vec(), Object::Ref(tree));
    doc.update_dict(doc.root().unwrap(), |d| d.set(b"Names".to_vec(), Object::Dict(names))).unwrap();
    pdfcraft_cos::write_incremental(&doc, &SaveOptions::default()).unwrap()
}

/// Inspect the actual working bytes, not just page counts or history labels. The fixture has
/// exactly `depth` single-child nodes, so this checks its leaf without a second general walker.
fn assert_destination_state(a: &Automation, doc: u64, depth: usize, count: usize, deleted: bool) {
    use pdfcraft_cos::{Document, Object};
    let bytes = a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes.clone();
    let parsed = Document::open(bytes).unwrap();
    let pages = pdfcraft_organize::pages(&parsed).unwrap();
    assert_eq!(pages.len(), if deleted { 2 } else { 3 });
    let catalog = parsed.get(parsed.root().unwrap());
    let names = parsed.resolve(catalog.as_dict().unwrap().get(b"Names").unwrap());
    let mut node = parsed.resolve(names.as_dict().unwrap().get(b"Dests").unwrap());
    for _ in 0..depth {
        let kids = node.as_dict().unwrap().get(b"Kids").unwrap().as_array().unwrap();
        assert_eq!(kids.len(), 1);
        let child = kids[0].clone();
        node = parsed.resolve(&child);
    }
    let pairs = node.as_dict().unwrap().get(b"Names").and_then(Object::as_array).unwrap();
    assert_eq!(pairs.len(), if deleted { count / 2 * 2 } else { count * 2 });
    let expected = (0..count).filter(|i| !deleted || i % 2 == 1);
    for (pair, i) in pairs.as_chunks::<2>().0.iter().zip(expected) {
        assert_eq!(pair[0].as_string().unwrap().to_text(), format!("dest-{i:05}"));
        let target = if i % 2 == 0 { pages[0].obj } else { pages.last().unwrap().obj };
        assert_eq!(pair[1].as_array().unwrap()[0].as_ref(), Some(target), "destination {i}");
    }
}

/// Exercise the real tools, including named destinations after undo and redo, and check both
/// kinds of save explicitly: a full copy, followed by an incremental update of that copy.
fn delete_destination_tree_through_tools(depth: usize, count: usize, label: &str) {
    let dir = workdir(label);
    let original = destination_graph_fixture(depth, count);
    std::fs::write(dir.join("source.pdf"), &original).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({"path":"source.pdf"}))["doc"].as_u64().unwrap();
    assert_destination_state(&a, doc, depth, count, false);
    let render = a.call("page_render", &json!({"doc":doc,"page":2,"dpi":72})).unwrap();
    ok(&mut a, "doc_set_info", json!({"doc":doc,"key":"Title","value":"Unsaved work"}));
    ok(&mut a, "page_delete", json!({"doc":doc,"pages":[1]}));
    assert_destination_state(&a, doc, depth, count, true);
    assert_eq!(page_text(&mut a, doc), ["Page 2", "Page 3"]);
    assert_eq!(a.call("page_render", &json!({"doc":doc,"page":1,"dpi":72})).unwrap(), render);
    ok(&mut a, "edit_undo", json!({"doc":doc}));
    assert_destination_state(&a, doc, depth, count, false);
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 3"]);
    assert_eq!(ok(&mut a, "doc_info", json!({"doc":doc}))["title"], "Unsaved work");
    ok(&mut a, "edit_redo", json!({"doc":doc}));
    assert_destination_state(&a, doc, depth, count, true);
    assert_eq!(page_text(&mut a, doc), ["Page 2", "Page 3"]);
    assert_eq!(ok(&mut a, "doc_info", json!({"doc":doc}))["title"], "Unsaved work");

    let saved = ok(&mut a, "doc_save", json!({"doc":doc,"path":"saved.pdf","full":true}));
    assert_eq!(saved["incremental"], false);
    let full = std::fs::read(dir.join("saved.pdf")).unwrap();
    let mut b = auto(&dir);
    let reopened = ok(&mut b, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    assert_destination_state(&b, reopened, depth, count, true);
    assert_eq!(b.call("page_render", &json!({"doc":reopened,"page":1,"dpi":72})).unwrap(), render);

    assert_eq!(ok(&mut b, "doc_info", json!({"doc":reopened}))["title"], "Unsaved work");
    ok(&mut b, "doc_set_info", json!({"doc":reopened,"key":"Author","value":"Incremental probe"}));
    let saved = ok(&mut b, "doc_save", json!({"doc":reopened}));
    assert_eq!(saved["incremental"], true);
    let incremental = std::fs::read(dir.join("saved.pdf")).unwrap();
    assert!(incremental.len() > full.len() && incremental.starts_with(&full));
    let mut b = auto(&dir);
    let reopened = ok(&mut b, "doc_open", json!({"path":"saved.pdf"}))["doc"].as_u64().unwrap();
    assert_destination_state(&b, reopened, depth, count, true);
    assert_eq!(page_text(&mut b, reopened), ["Page 2", "Page 3"]);
    assert_eq!(b.call("page_render", &json!({"doc":reopened,"page":1,"dpi":72})).unwrap(), render);
    let info = ok(&mut b, "doc_info", json!({"doc":reopened}));
    assert_eq!(info["title"], "Unsaved work");
    assert_eq!(info["author"], "Incremental probe");
    assert_eq!(std::fs::read(dir.join("source.pdf")).unwrap(), original);
}

/// `root/` (with `inside.pdf`) next to `outside/` (with the file `secret.pdf` and the folder
/// `sub`), all in a fresh temporary directory. Returns (base, canonical root).
fn sandbox(test: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("pdfcraft-automation-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("root")).unwrap();
    std::fs::create_dir_all(base.join("outside/sub")).unwrap();
    std::fs::write(base.join("root/inside.pdf"), fixture(1)).unwrap();
    std::fs::write(base.join("outside/secret.pdf"), fixture(1)).unwrap();
    let root = base.join("root").canonicalize().unwrap();
    (base, root)
}

/// A link `root/<name>` to the directory `target`: a symlink on Unix, a junction on Windows
/// (which needs no privilege).
fn link_dir(root: &Path, name: &str, target: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, root.join(name)).unwrap();
    #[cfg(windows)]
    {
        // Rebuilt from components so every separator is `\` (cmd reads `/x` as a switch).
        let (link, target): (PathBuf, PathBuf) = (root.join(name).components().collect(), target.components().collect());
        let status = std::process::Command::new("cmd").arg("/C").arg("mklink").arg("/J").arg(link).arg(target).output().unwrap();
        assert!(status.status.success(), "mklink /J failed: {}", String::from_utf8_lossy(&status.stderr));
    }
}

// ---- MCP ---------------------------------------------------------------------------------------

#[cfg(feature = "mcp")]
fn rpc(server: &mut McpServer, id: u64, method: &str, params: Value) -> Value {
    let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    serde_json::from_str(&server.handle_line(&line).expect("a reply")).unwrap()
}

/// A server in compact mode with `dir` as its root.
#[cfg(feature = "mcp")]
fn compact_server(dir: &Path) -> McpServer {
    McpServer::new(auto(dir)).with_compact(true)
}

/// Run a tool through the compact server's `tool_call`.
#[cfg(feature = "mcp")]
fn via_tool_call(s: &mut McpServer, id: u64, tool: &str, arguments: Value) -> Value {
    rpc(s, id, "tools/call", json!({ "name": "tool_call", "arguments": { "name": tool, "arguments": arguments } }))
}

#[cfg(feature = "mcp")]
fn tool_names(reply: &Value) -> Vec<String> {
    reply["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()
}

/// A flat outline of `n` top-level bookmarks, `Item 0` to `Item n-1`, on one page.
fn flat_outline_pdf(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 300 400] >>".into(),
        format!("<< /Type /Outlines /First 5 0 R /Last {} 0 R /Count {n} >>", 4 + n),
        "<< /Type /Page /Parent 2 0 R >>".into(),
    ];
    for k in 0..n {
        let prev = if k == 0 { String::new() } else { format!(" /Prev {} 0 R", 4 + k) };
        let next = if k + 1 == n { String::new() } else { format!(" /Next {} 0 R", 6 + k) };
        objs.push(format!("<< /Title (Item {k}) /Parent 3 0 R{prev}{next} /Dest [4 0 R /Fit] >>"));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn add_comment_examples(a: &mut Automation, doc: u64) -> String {
    // "Page 2" sits at y = 150 (user space) = 150 from the top of the 300 pt page.
    let hl = ok(a, "comment_add", json!({ "doc": doc, "page": 2, "type": "highlight", "find": "page 2", "contents": "check", "author": "Ada" }));
    assert_eq!(hl["comment"]["lines"], 1);
    let id = hl["comment"]["id"].as_str().unwrap().to_string();
    ok(a, "comment_add", json!({ "doc": doc, "page": 2, "type": "note", "at": [150, 20], "contents": "Sticky" }));
    ok(a, "comment_add", json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [10, 10, 60, 40], "color": "blue", "width": 3 }));
    ok(a, "comment_add", json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [10, 200, 190, 240], "contents": "Hello", "font_size": 10 }));
    ok(a, "comment_add", json!({ "doc": doc, "page": 1, "type": "ink", "strokes": [[[10, 280], [50, 260], [90, 285]]] }));
    ok(a, "comment_add", json!({ "doc": doc, "page": 3, "type": "arrow", "from": [10, 10], "to": [100, 100] }));
    ok(a, "comment_reply", json!({ "doc": doc, "id": id, "text": "Looks right", "author": "Bob" }));
    ok(a, "comment_set_status", json!({ "doc": doc, "id": id, "status": "accepted", "author": "Bob" }));
    id
}

fn verify_comment_listing(list: &Value, id: &str) -> Value {
    assert_eq!(list["count"], 6, "{list}");
    let c = list["comments"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap().clone();
    assert_eq!((c["type"].as_str(), c["author"].as_str(), c["contents"].as_str()), (Some("Highlight"), Some("Ada"), Some("check")));
    assert_eq!(c["status"], "Accepted");
    assert_eq!(c["replies"].as_array().unwrap().len(), 1);
    assert_eq!(c["replies"][0]["contents"], "Looks right");
    // The highlight covers the found text, reported in top-left-origin points.
    let r: Vec<f64> = c["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(r[0] >= 15.0 && r[0] <= 25.0 && r[1] > 120.0 && r[3] < 160.0, "{r:?}");
    let rect = list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "Square").unwrap().clone();
    assert_eq!(rect["color"], "#0078D6");
    assert_eq!(rect["rect"], json!([10.0, 10.0, 60.0, 40.0]));
    let note = list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "Text").unwrap().clone();
    assert_eq!(note["rect"], json!([150.0, 20.0, 170.0, 40.0]), "note icon hangs from its top-left point");
    rect
}

fn edit_rectangle_comment(a: &mut Automation, doc: u64, original: &Value) {
    // Edit by page + index; several changes are one undo step.
    let (page, index) = (original["page"].as_u64().unwrap(), original["index"].as_u64().unwrap());
    ok(a, "comment_edit", json!({ "doc": doc, "page": page, "index": index, "color": "#FF0000", "move": [5, 5], "contents": "moved" }));
    let list = ok(a, "comment_list", json!({ "doc": doc, "page": 1 }));
    let rect = list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "Square").unwrap().clone();
    assert_eq!((rect["color"].as_str(), rect["contents"].as_str()), (Some("#FF0000"), Some("moved")));
    assert_eq!(rect["rect"], json!([15.0, 15.0, 65.0, 45.0]));
    let undo = ok(a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(undo["undone"], "Edit comment");
    ok(a, "edit_redo", json!({ "doc": doc }));
    // Rectangles, ovals and polygons take a fill (#686); "none" removes it. Lines have none.
    ok(a, "comment_edit", json!({ "doc": doc, "page": page, "index": index, "fill": "yellow" }));
    ok(a, "comment_edit", json!({ "doc": doc, "page": page, "index": index, "fill": "none" }));
    assert!(matches!(a.call("comment_edit", &json!({ "doc": doc, "page": page, "index": index, "fill": "mauve" })), Err(ToolError::InvalidArgs(_))));
    let arrow = ok(a, "comment_list", json!({ "doc": doc, "page": 3 }))["comments"][0].clone();
    assert!(matches!(
        a.call("comment_edit", &json!({ "doc": doc, "page": arrow["page"], "index": arrow["index"], "fill": "red" })),
        Err(ToolError::Failed(_))
    ));
}

fn edit_text_comment(a: &mut Automation, doc: u64, original_list: &Value) {
    // Editing a text box re-fits its rectangle to the new text: the wrap width and top edge
    // stay, the height follows the wrapped lines.
    let tb = original_list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "FreeText").unwrap().clone();
    let long = "the quick brown fox jumps over the lazy dog ".repeat(3);
    ok(a, "comment_edit", json!({ "doc": doc, "page": tb["page"], "index": tb["index"], "contents": long.trim_end() }));
    let list = ok(a, "comment_list", json!({ "doc": doc, "page": 1 }));
    let tb = list["comments"].as_array().unwrap().iter().find(|c| c["type"] == "FreeText").unwrap().clone();
    let r: Vec<f64> = tb["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!((r[0], r[1], r[2] - r[0]), (10.0, 200.0, 180.0), "width and top edge stay: {r:?}");
    assert!(r[3] - r[1] > 40.0, "the box grew to fit the wrapped lines: {r:?}");

    // The rendered page shows the rectangle's border.
    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    assert!(matches!(png[0], Content::Png { .. }));
}

fn verify_comment_errors(a: &mut Automation, doc: u64, id: &str) {
    ok(a, "comment_delete", json!({ "doc": doc, "id": id }));
    assert_eq!(ok(a, "comment_list", json!({ "doc": doc }))["count"], 5);
    assert!(matches!(a.call("comment_delete", &json!({ "doc": doc, "id": id })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "highlight", "find": "nowhere" })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "rectangle" })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(
        a.call("comment_add", &json!({ "doc": doc, "page": 1, "type": "note", "at": [1, 1], "color": "mauve" })),
        Err(ToolError::InvalidArgs(_))
    ));
    assert!(matches!(a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": 1 })), Err(ToolError::InvalidArgs(_))));
}

fn save_and_reopen_comments(dir: &Path, a: &mut Automation, doc: u64) {
    ok(a, "doc_save", json!({ "doc": doc, "path": "commented.pdf" }));
    let mut b = auto(dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "commented.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut b, "comment_list", json!({ "doc": re }))["count"], 5);
}

/// A PDF of the given objects (numbered from 1).
fn pdf_of(objs: &[&str]) -> Vec<u8> {
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

/// One Letter page whose Helvetica is not embedded (WinAnsi), with `body` as its content stream.
fn win_ansi_paragraph_pdf(body: &str) -> Vec<u8> {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
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

/// One page whose content is one stream in three pieces (#155): a `TJ` array ends the middle
/// piece and its operator starts the last one.
fn split_streams_pdf() -> Vec<u8> {
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

/// One page with the text "Secret 1234" and a Redact annotation that has neither a usable
/// QuadPoints array nor a Rect, so there is no area to redact.
fn redact_pdf_with_unreadable_mark() -> Vec<u8> {
    let body = "BT /F1 24 Tf 20 150 Td (Secret 1234) Tj ET";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R] >>".into(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Annot /Subtype /Redact /QuadPoints /oops >>".into(),
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

/// A one-page PDF whose text "TOP SECRET" sits under an opaque black rectangle.
fn faux_redacted_fixture() -> Vec<u8> {
    helvetica_page("BT /F1 24 Tf 100 700 Td (TOP SECRET) Tj ET\n0 g\n90 688 220 32 re f")
}

/// A one-page US Letter PDF drawing `body` with Helvetica as /F1.
fn helvetica_page(body: &str) -> Vec<u8> {
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >> >>".to_string().into_bytes(),
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()).into_bytes(),
    ];
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

fn added_rotation_image(dir: &Path) {
    let mut png = Vec::new();
    let mut enc = png::Encoder::new(&mut png, 2, 1);
    enc.set_color(png::ColorType::Rgb);
    enc.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 0, 255]).unwrap();
    std::fs::write(dir.join("rotation.png"), png).unwrap();
}

/// A malformed form with two calculated fields named `dup`, one summing `src`, the other `x`.
fn two_calculations_with_one_name() -> Vec<u8> {
    let widget = |name: &str, y: u32, reads: &str| {
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /Rect [50 {y} 250 {}] /P 3 0 R /AA << /C << /S /JavaScript /JS (AFSimple_Calculate\\(\"SUM\", \"{reads}\"\\);) >> >> >>",
            y + 20
        )
    };
    pdf_of(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R 7 0 R 8 0 R] >>",
        "<< /Fields [5 0 R 6 0 R 7 0 R 8 0 R] /DA (/Helv 0 Tf 0 g) /CO [6 0 R 7 0 R 5 0 R] >>",
        &widget("dup", 700, "x"),
        &widget("dup", 660, "src"),
        &widget("src", 620, "x"),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (x) /Rect [50 580 250 600] /P 3 0 R /V (3) >>",
    ])
}

fn local_gotor_fixtures() -> PathBuf {
    let (base, root) = sandbox("gotor-open");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("target.pdf"), fixture(3)).unwrap();
    std::fs::write(root.join("a.pdf"), fixture(1)).unwrap();
    std::fs::write(root.join("sub/source.pdf"), remote_link_pdf("../target.pdf", Some("[2 /FitH 180]"))).unwrap();
    std::fs::write(root.join("sub/escape.pdf"), remote_link_pdf("../../outside/secret.pdf", Some("[0 /Fit]"))).unwrap();
    std::fs::write(root.join("named.pdf"), remote_link_pdf("target.pdf", Some("(chapter)"))).unwrap();
    std::fs::write(root.join("range.pdf"), remote_link_pdf("target.pdf", Some("[99 /Fit]"))).unwrap();
    std::fs::write(root.join("url.pdf"), remote_link_pdf("https://example.org/target.pdf", Some("[0 /Fit]"))).unwrap();
    std::fs::write(root.join("missing-source.pdf"), remote_link_pdf("does-not-exist.pdf", Some("[0 /Fit]"))).unwrap();
    std::fs::write(root.join("not-pdf.pdf"), b"not a PDF").unwrap();
    std::fs::write(root.join("non-pdf-source.pdf"), remote_link_pdf("not-pdf.pdf", Some("[0 /Fit]"))).unwrap();
    std::fs::write(root.join("encrypted.pdf"), protected_fixture("secret")).unwrap();
    std::fs::write(root.join("password-source.pdf"), remote_link_pdf("encrypted.pdf", Some("[0 /Fit]"))).unwrap();
    std::fs::write(root.join("absolute.pdf"), remote_link_pdf(root.join("target.pdf").to_str().unwrap(), Some("[1 /Fit]"))).unwrap();
    link_dir(&root, "outside-link", &base.join("outside"));
    std::fs::write(root.join("symlink.pdf"), remote_link_pdf("outside-link/secret.pdf", Some("[0 /Fit]"))).unwrap();
    root
}

fn assert_relative_gotor_destination(a: &mut Automation) {
    let source = ok(a, "doc_open", json!({ "path": "sub/source.pdf" }))["doc"].as_u64().unwrap();
    let listed = ok(a, "link_list", json!({ "doc": source }));
    assert_eq!(listed["links"][0]["other"], "GoToR");
    let opened = ok(a, "link_open", json!({ "doc": source, "page": 1, "index": 1 }));
    assert_eq!(opened["path"], "target.pdf");
    assert_eq!(opened["destination_applied"], true);
    assert_eq!(opened["destination"]["page"], 3);
    assert_eq!(opened["destination"]["view"]["kind"], "fit_h");
    assert_eq!(opened["document"]["pages"], 3);
    let target_id = opened["document"]["doc"].as_u64().unwrap();
    assert_eq!(a.session().get(pdfcraft_engine::DocId(target_id)).unwrap().info.pages.len(), 3);
}

fn assert_absolute_gotor_policy(a: &mut Automation) {
    let absolute = ok(a, "doc_open", json!({ "path": "absolute.pdf" }))["doc"].as_u64().unwrap();
    let before = a.session().docs().len();
    for args in [json!({ "doc": absolute, "page": 1, "index": 1 }), json!({ "doc": absolute, "page": 1, "index": 1, "allow_absolute": false })] {
        assert!(matches!(err(a, "link_open", args), ToolError::Failed(m) if m.contains("allow_absolute")));
        assert_eq!(a.session().docs().len(), before);
    }
    let opened = ok(a, "link_open", json!({ "doc": absolute, "page": 1, "index": 1, "allow_absolute": true }));
    assert_eq!(opened["path"], "target.pdf");
    assert_eq!(opened["destination"]["page"], 2);
}

fn assert_unapplied_gotor_destinations(a: &mut Automation) {
    for (source_path, warning) in [("named.pdf", "named destination"), ("range.pdf", "out of range")] {
        let source = ok(a, "doc_open", json!({ "path": source_path }))["doc"].as_u64().unwrap();
        let opened = ok(a, "link_open", json!({ "doc": source, "page": 1, "index": 1 }));
        assert_eq!(opened["destination_applied"], false);
        assert!(opened["warning"].as_str().unwrap().contains(warning), "{opened}");
    }
}

fn assert_encrypted_gotor_passwords(a: &mut Automation) {
    let password_source = ok(a, "doc_open", json!({ "path": "password-source.pdf" }))["doc"].as_u64().unwrap();
    let documents_before = a.session().docs().len();
    assert!(matches!(err(a, "link_open", json!({ "doc": password_source, "page": 1, "index": 1 })), ToolError::Failed(_)));
    assert_eq!(a.session().docs().len(), documents_before);
    assert!(matches!(err(a, "link_open", json!({ "doc": password_source, "page": 1, "index": 1, "password": "wrong" })), ToolError::Failed(_)));
    assert_eq!(a.session().docs().len(), documents_before);
    let opened = ok(a, "link_open", json!({ "doc": password_source, "page": 1, "index": 1, "password": "secret" }));
    assert_eq!(opened["document"]["encrypted"], true);
}

fn assert_unsupported_gotor_targets(a: &mut Automation) {
    for source_path in ["url.pdf", "missing-source.pdf", "non-pdf-source.pdf"] {
        let source = ok(a, "doc_open", json!({ "path": source_path }))["doc"].as_u64().unwrap();
        let documents_before = a.session().docs().len();
        assert!(matches!(err(a, "link_open", json!({ "doc": source, "page": 1, "index": 1 })), ToolError::Failed(_)));
        assert_eq!(a.session().docs().len(), documents_before, "a rejected target is not opened: {source_path}");
    }
}

fn assert_gotor_root_boundaries(a: &mut Automation) {
    for source_path in ["sub/escape.pdf", "symlink.pdf"] {
        let source = ok(a, "doc_open", json!({ "path": source_path }))["doc"].as_u64().unwrap();
        let documents_before = a.session().docs().len();
        assert!(matches!(err(a, "link_open", json!({ "doc": source, "page": 1, "index": 1 })), ToolError::Failed(m) if m.contains("outside")));
        assert_eq!(a.session().docs().len(), documents_before, "a refused target is not opened");
    }
}

fn assert_url_links_cannot_open_as_gotor(a: &mut Automation) {
    let missing_source = ok(a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(a, "link_add", json!({ "doc": missing_source, "page": 1, "rect": [20, 30, 80, 50], "url": "https://example.org" }));
    assert!(
        matches!(err(a, "link_open", json!({ "doc": missing_source, "page": 1, "index": 1 })), ToolError::InvalidArgs(m) if m.contains("not a supported local PDF GoToR"))
    );
}

fn object_has_name(obj: &pdfcraft_cos::Object, name: &[u8]) -> bool {
    match obj {
        pdfcraft_cos::Object::Name(n) => n.as_slice() == name,
        pdfcraft_cos::Object::Array(items) => items.iter().any(|o| object_has_name(o, name)),
        pdfcraft_cos::Object::Dict(d) => d.iter().any(|(_, o)| object_has_name(o, name)),
        pdfcraft_cos::Object::Stream(s) => s.dict.iter().any(|(_, o)| object_has_name(o, name)),
        _ => false,
    }
}

fn create_signing_identity(a: &mut Automation) {
    let id = ok(
        a,
        "sign_id_create",
        json!({ "name": "Ada Lovelace", "organization": "Analytical Engines", "email": "ada@example.com", "country": "GB", "key": "p256", "password": "secret12", "path": "ada.p12" }),
    );
    assert_eq!(id["certificate"]["subject"], "C=GB, O=Analytical Engines, CN=Ada Lovelace, E=ada@example.com");
    assert_eq!(id["certificate"]["self_signed"], true);
    assert!(matches!(a.call("sign_id_create", &json!({ "name": "X", "password": "short", "path": "x.p12" })), Err(ToolError::InvalidArgs(_))));
    // Eight characters at least (#1847): seven is refused.
    assert!(matches!(a.call("sign_id_create", &json!({ "name": "X", "password": "seven77", "path": "x.p12" })), Err(ToolError::InvalidArgs(_))));
}

fn sign_document_and_verify_trust(a: &mut Automation, dir: &Path) -> u64 {
    let doc = ok(a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(a, "sign_list", json!({ "doc": doc }))["count"], 0);
    // A Keychain identity that doesn't exist (macOS) or Keychains at all (elsewhere).
    assert!(matches!(a.call("sign_document", &json!({ "doc": doc, "id": "keychain:No Such Signer", "out": "k.pdf" })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("sign_document", &json!({ "doc": doc, "id": "windows:No Such Signer", "out": "w.pdf" })), Err(ToolError::Failed(_))));
    let store = ok(a, "sign_windows_ids", json!({}));
    let ids = store["ids"].as_array().unwrap();
    assert_eq!(store["count"].as_u64().unwrap(), ids.len() as u64);
    assert!(ids.iter().all(|id| id["id"].as_str().unwrap().starts_with("windows:")));
    // Store certificates that can't sign are explained, not dropped (issue #179).
    let unusable = store["unusable"].as_array().unwrap();
    assert!(unusable.iter().all(|u| u["subject"].is_string() && !u["reason"].as_str().unwrap().is_empty() && u["no_private_key"].is_boolean()));
    #[cfg(not(windows))]
    assert!(ids.is_empty() && unusable.is_empty());
    assert!(matches!(
        a.call("sign_document", &json!({ "doc": doc, "id": "ada.p12", "password": "wrong!", "out": "signed.pdf" })),
        Err(ToolError::InvalidArgs(_))
    ));
    let r = ok(
        a,
        "sign_document",
        json!({ "doc": doc, "id": "ada.p12", "password": "secret12", "page": 2, "rect": [20, 200, 180, 250], "reason": "Approved", "location": "London", "out": "signed.pdf" }),
    );
    assert_eq!(r["signature"]["status"], "unknown");
    assert_eq!(r["signature"]["signer"], "Ada Lovelace");
    assert_eq!(r["signature"]["page"], 2);
    let rect: Vec<f64> = r["signature"]["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!((rect[0] - 20.0).abs() < 0.5 && (rect[3] - 250.0).abs() < 0.5, "{rect:?}");
    assert!(dir.join("signed.pdf").exists());

    // Trust the ID: valid; comment afterwards: still valid, change allowed.
    let t = ok(a, "sign_trust", json!({ "paths": ["ada.p12"], "password": "secret12" }));
    assert_eq!(t["trusted"].as_array().unwrap().len(), 1);
    let list = ok(a, "sign_list", json!({ "doc": doc }));
    assert_eq!((list["all_valid"].as_bool(), list["signatures"][0]["status"].as_str()), (Some(true), Some("valid")));
    ok(a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [20, 20], "contents": "ok" }));
    let s = &ok(a, "sign_list", json!({ "doc": doc }))["signatures"][0];
    assert_eq!((s["status"].as_str(), s["modification"].as_str()), (Some("valid"), Some("allowed")));
    doc
}

fn verify_signature_revisions_and_bundle(a: &mut Automation, doc: u64, dir: &Path) {
    // A full rewrite is refused for a signed document.
    assert!(matches!(a.call("doc_save", &json!({ "doc": doc, "path": "copy.pdf", "full": true })), Err(ToolError::Failed(_))));
    ok(a, "doc_save", json!({ "doc": doc }));
    // Three revisions: the original, the signature, the comment; the middle one is signed.
    let revs = ok(a, "doc_revisions", json!({ "doc": doc }))["revisions"].as_array().cloned().unwrap();
    assert_eq!(revs.len(), 3, "{revs:?}");
    assert!(revs[0]["signed_by"].as_array().unwrap().is_empty() && revs[1]["signed_by"].as_array().unwrap().len() == 1, "{revs:?}");
    let old = ok(a, "doc_open_revision", json!({ "doc": doc, "revision": 2 }))["doc"].as_u64().unwrap();
    assert_eq!(ok(a, "comment_list", json!({ "doc": old }))["comments"].as_array().map(Vec::len), Some(0), "before the comment");
    assert_eq!(ok(a, "sign_list", json!({ "doc": old }))["count"], 1);
    assert!(matches!(a.call("doc_open_revision", &json!({ "doc": doc, "revision": 4 })), Err(ToolError::Failed(_))));
    assert_eq!(ok(a, "sign_trust", json!({ "clear": true }))["trusted"].as_array().unwrap().len(), 0);
    assert_eq!(ok(a, "sign_list", json!({ "doc": doc }))["signatures"][0]["status"], "unknown");
    // The same certificate from a PKCS #7 bundle (.p7b), as Windows, Acrobat and CAs export one.
    use pdfcraft_engine::sign::{cms, der, pkcs12};
    let cert = pkcs12::open(&std::fs::read(dir.join("ada.p12")).unwrap(), "secret12").unwrap().certificate.raw;
    let sd = der::seq(&[&der::int(1), &der::set_of(&[]), &der::seq(&[&der::oid(cms::DATA)]), &der::explicit(0, &cert), &der::set_of(&[])]);
    std::fs::write(dir.join("ada.p7b"), der::seq(&[&der::oid(cms::SIGNED_DATA), &der::explicit(0, &sd)])).unwrap();
    assert_eq!(ok(a, "sign_trust", json!({ "paths": ["ada.p7b"] }))["trusted"].as_array().unwrap().len(), 1);
    assert_eq!(ok(a, "sign_list", json!({ "doc": doc }))["signatures"][0]["status"], "valid");
}

/// The decoded text of every visible signature appearance (Helvetica forms) in a signed file.
fn signature_appearances(path: &Path) -> String {
    use pdfcraft_cos::{Document, ObjRef, Object};
    let doc = Document::open(std::sync::Arc::new(std::fs::read(path).unwrap())).unwrap();
    let mut text = String::new();
    for num in doc.object_numbers() {
        if let Object::Stream(s) = &*doc.get(ObjRef { num, generation: 0 })
            && s.dict.name(b"Subtype") == Some(b"Form")
        {
            let content = String::from_utf8_lossy(&s.decoded().unwrap()).into_owned();
            if content.contains("/Helv") {
                text.push_str(&content);
            }
        }
    }
    text
}

/// `data` as LZW with one 9-bit literal code per byte (clear-table first, EOD last): valid for
/// fewer than 250 bytes, before the code width grows.
fn lzw_literals(data: &[u8]) -> Vec<u8> {
    let codes: Vec<u16> = std::iter::once(256).chain(data.iter().map(|&b| u16::from(b))).chain([257]).collect();
    let bits: Vec<bool> = codes.iter().flat_map(|c| (0..9).rev().map(move |i| c >> i & 1 == 1)).collect();
    bits.chunks(8).map(|byte| byte.iter().enumerate().fold(0u8, |acc, (i, &b)| acc | (u8::from(b) << (7 - i)))).collect()
}

/// A one-page PDF drawing a 1-bit scan of text (glyph strokes with ragged edges, as a scanner
/// leaves them) of 1000 × 400 samples, stored with Flate.
fn scanned_page() -> Vec<u8> {
    scan_drawn(b"q 500 0 0 200 50 500 cm /Scan Do Q")
}

/// [`scanned_page`]'s scan, drawn by `content` (which names it `/Scan`).
fn scan_drawn(content: &[u8]) -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_full};
    let (w, h) = (1000usize, 400usize);
    let stride = w.div_ceil(8);
    let mut data = vec![0xFFu8; stride * h];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for line in (4..h - 20).step_by(24) {
        let mut x = 8;
        while x + 12 < w {
            let wide = 3 + (next() % 5) as usize;
            let (top, bottom) = (line + (next() % 4) as usize, line + 14);
            for y in top..bottom {
                let left = x + usize::from(next() % 8 == 0);
                let right = x + wide - usize::from(next() % 8 == 0);
                for xx in left..right {
                    data[y * stride + xx / 8] &= !(0x80 >> (xx % 8));
                }
            }
            x += wide + 2 + (next() % 4) as usize;
        }
    }
    let mut doc = Document::new_empty();
    let mut d = Dict::new();
    d.set(b"Type".to_vec(), Object::name("XObject"));
    d.set(b"Subtype".to_vec(), Object::name("Image"));
    d.set(b"Width".to_vec(), Object::Int(w as i64));
    d.set(b"Height".to_vec(), Object::Int(h as i64));
    d.set(b"BitsPerComponent".to_vec(), Object::Int(1));
    d.set(b"ColorSpace".to_vec(), Object::name("DeviceGray"));
    let image = doc.add(Object::Stream(Stream::flate(d, &data)));
    let pages = doc.root().and_then(|r| doc.get(r).as_dict().and_then(|d| d.reference(b"Pages"))).unwrap();
    let mut x = Dict::new();
    x.set(b"Scan".to_vec(), Object::Ref(image));
    let mut res = Dict::new();
    res.set(b"XObject".to_vec(), Object::Dict(x));
    let content = doc.add(Object::Stream(Stream::from_raw(Dict::new(), content.to_vec())));
    let mut p = Dict::new();
    p.set(b"Type".to_vec(), Object::name("Page"));
    p.set(b"Parent".to_vec(), Object::Ref(pages));
    p.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
    p.set(b"Resources".to_vec(), Object::Dict(res));
    p.set(b"Contents".to_vec(), Object::Ref(content));
    let page = doc.add(Object::Dict(p));
    doc.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(vec![Object::Ref(page)]));
        d.set(b"Count".to_vec(), Object::Int(1));
    })
    .unwrap();
    write_full(&doc, &SaveOptions::default()).unwrap()
}

/// The width and height of every image in the PDF at `path`.
fn image_sizes(path: &std::path::Path) -> Vec<(Option<i64>, Option<i64>)> {
    let written = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(path).unwrap())).unwrap();
    written
        .object_numbers()
        .into_iter()
        .filter_map(|num| match &*written.get(pdfcraft_cos::ObjRef { num, generation: written.generation(num) }) {
            pdfcraft_cos::Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Image") => Some((s.dict.int(b"Width"), s.dict.int(b"Height"))),
            _ => None,
        })
        .collect()
}

/// A one-page PDF whose page picture is drawn as `rows` grey images one pixel row high, 600 px
/// wide on a 72 pt square (600 ppi), as some PDF printers write it.
fn printed_in_strips(rows: usize) -> Vec<u8> {
    let mut objs: Vec<Vec<u8>> = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()];
    let names: String = (0..rows).map(|r| format!("/R{r} {} 0 R ", 5 + r)).collect();
    objs.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Resources << /XObject << {names}>> >> /Contents 4 0 R >>").into_bytes());
    let k = 72.0 / rows as f64;
    let content: String = (0..rows)
        .map(|r| {
            format!(
                "q 72 0 0 {k} 0 {} cm /R{r} Do Q
",
                (rows - 1 - r) as f64 * k
            )
        })
        .collect();
    objs.push(
        format!(
            "<< /Length {} >>
stream
{content}
endstream",
            content.len() + 1
        )
        .into_bytes(),
    );
    for r in 0..rows {
        let px: Vec<u8> = (0..600).map(|x| ((x + r * 7) % 256) as u8).collect();
        let mut o = format!(
            "<< /Type /XObject /Subtype /Image /Width 600 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>
stream
",
            px.len()
        )
        .into_bytes();
        o.extend_from_slice(&px);
        o.extend_from_slice(
            b"
endstream",
        );
        objs.push(o);
    }
    let mut out = b"%PDF-1.5
"
    .to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(
            format!(
                "{} 0 obj
",
                i + 1
            )
            .as_bytes(),
        );
        out.extend_from_slice(o);
        out.extend_from_slice(
            b"
endobj
",
        );
    }
    let x = out.len();
    out.extend_from_slice(
        format!(
            "xref
0 {}
0000000000 65535 f 
",
            objs.len() + 1
        )
        .as_bytes(),
    );
    for o in offs {
        out.extend_from_slice(
            format!(
                "{o:010} 00000 n 
"
            )
            .as_bytes(),
        );
    }
    out.extend_from_slice(
        format!(
            "trailer
<< /Size {} /Root 1 0 R >>
startxref
{x}
%%EOF
",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// #3120: a PDF whose pages draw "Head", then a chain of `depths[page]` nested forms whose
/// innermost writes "TAIL".
fn nested_forms_pdf(depths: &[usize]) -> Vec<u8> {
    let mut objs: Vec<String> =
        vec!["<< /Type /Catalog /Pages 2 0 R >>".into(), String::new(), "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into()];
    let mut kids = Vec::new();
    for &depth in depths {
        // The object number of D1, D2 and so on.
        let first = objs.len() + 1;
        let xobjects: String = (1..=depth).map(|i| format!("/D{i} {} 0 R ", first + i - 1)).collect();
        let resources = format!("<< /Font << /F1 3 0 R >> /XObject << {xobjects}>> >>");
        for i in 1..=depth {
            let body = if i == depth { "BT /F1 24 Tf 20 150 Td (TAIL) Tj ET".to_string() } else { format!("/D{} Do", i + 1) };
            objs.push(format!(
                "<< /Type /XObject /Subtype /Form /BBox [0 0 200 300] /Resources {resources} /Length {} >>\nstream\n{body}\nendstream",
                body.len()
            ));
        }
        let body = "BT /F1 24 Tf 20 250 Td (Head) Tj ET /D1 Do";
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources {resources} >>", objs.len()));
        kids.push(format!("{} 0 R", objs.len()));
    }
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} /MediaBox [0 0 200 300] >>", kids.join(" "), kids.len());
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

mod close_argument_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Create exclusively and remove only this test's directory, never a pre-existing one.
    struct CloseDir(PathBuf);

    impl CloseDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let parent = std::env::temp_dir();
            for _ in 0..128 {
                let serial = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = parent.join(format!("pdfcraft-close-{}-{serial}", std::process::id()));
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        let dir = Self(path);
                        std::fs::write(dir.0.join("a.pdf"), fixture(3)).unwrap();
                        std::fs::write(dir.0.join("b.pdf"), fixture(2)).unwrap();
                        return dir;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("creating close test directory: {error}"),
                }
            }
            panic!("no unused close test directory after 128 attempts");
        }
    }

    impl Drop for CloseDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn close_rejects_invalid_discard_types_without_changing_clean_or_dirty_documents() {
        let dir = CloseDir::new();
        let source = std::fs::read(dir.0.join("a.pdf")).unwrap();
        let other_source = std::fs::read(dir.0.join("b.pdf")).unwrap();
        let mut a = auto(&dir.0);
        let other = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
        for dirty in [false, true] {
            let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
            if dirty {
                ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Unsaved title" }));
            }
            let list = ok(&mut a, "doc_list", json!({}));
            let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
            let text = ok(&mut a, "text_extract", json!({ "doc": doc }));
            let bytes = a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes.clone();
            for invalid in [json!("false"), json!(0), json!([]), json!({})] {
                let error = a.call("doc_close", &json!({ "doc": doc, "discard_changes": invalid })).unwrap_err();
                assert!(matches!(error, ToolError::InvalidArgs(ref message) if message == "discard_changes must be true or false"));
                assert_eq!(ok(&mut a, "doc_list", json!({})), list, "all document identities, order, paths and history stay unchanged");
                assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc })), info);
                assert_eq!(ok(&mut a, "text_extract", json!({ "doc": doc })), text);
                let current = a.session().get(pdfcraft_engine::DocId(doc)).unwrap();
                assert_eq!(current.bytes, bytes);
                assert_eq!(current.dirty, dirty);
                assert_eq!(std::fs::read(dir.0.join("a.pdf")).unwrap(), source);
                assert_eq!(std::fs::read(dir.0.join("b.pdf")).unwrap(), other_source);
            }
            assert_eq!(ok(&mut a, "doc_close", json!({ "doc": doc, "discard_changes": true }))["closed"], doc);
            assert!(a.session().get(pdfcraft_engine::DocId(doc)).is_none());
            assert!(a.session().get(pdfcraft_engine::DocId(other)).is_some());
        }
    }

    #[test]
    fn close_keeps_default_and_boolean_discard_controls() {
        let dir = CloseDir::new();
        let source = std::fs::read(dir.0.join("a.pdf")).unwrap();
        let mut a = auto(&dir.0);
        let other = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
        for discard in [None, Some(Value::Null), Some(json!(false)), Some(json!(true))] {
            let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
            let mut args = json!({ "doc": doc });
            if let Some(discard) = discard {
                args["discard_changes"] = discard;
            }
            assert_eq!(ok(&mut a, "doc_close", args)["closed"], doc);
            assert!(a.session().get(pdfcraft_engine::DocId(doc)).is_none());
            assert!(a.session().get(pdfcraft_engine::DocId(other)).is_some());
        }
        let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
        ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Unsaved title" }));
        let before = ok(&mut a, "doc_list", json!({}));
        let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
        let bytes = a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes.clone();
        for args in [json!({ "doc": doc }), json!({ "doc": doc, "discard_changes": null }), json!({ "doc": doc, "discard_changes": false })] {
            let error = a.call("doc_close", &args).unwrap_err();
            assert!(matches!(error, ToolError::Failed(ref message) if message.contains("unsaved changes")));
            assert_eq!(ok(&mut a, "doc_list", json!({})), before);
            assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc })), info);
            assert_eq!(a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes, bytes);
        }
        assert_eq!(ok(&mut a, "doc_close", json!({ "doc": doc, "discard_changes": true }))["closed"], doc);
        assert!(a.session().get(pdfcraft_engine::DocId(doc)).is_none());
        assert!(a.session().get(pdfcraft_engine::DocId(other)).is_some());
        assert_eq!(std::fs::read(dir.0.join("a.pdf")).unwrap(), source);
    }
}

mod combine_argument_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Own only a newly created directory; the bounded runner provides project-local TMPDIR.
    struct CombineDir(PathBuf);

    impl CombineDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let parent = std::env::temp_dir();
            for _ in 0..128 {
                let serial = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = parent.join(format!("pdfcraft-combine-{}-{serial}", std::process::id()));
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        let dir = Self(path);
                        std::fs::write(dir.0.join("a.pdf"), fixture(3)).unwrap();
                        std::fs::write(dir.0.join("b.pdf"), fixture(2)).unwrap();
                        return dir;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("creating combine test directory: {error}"),
                }
            }
            panic!("no unused combine test directory after 128 attempts");
        }
    }

    impl Drop for CombineDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn invalid_combine_selectors_preserve_outputs_inputs_and_open_documents() {
        let dir = CombineDir::new();
        let first = std::fs::read(dir.0.join("a.pdf")).unwrap();
        let second = std::fs::read(dir.0.join("b.pdf")).unwrap();
        let sentinel = b"existing destination";
        std::fs::write(dir.0.join("combined.pdf"), sentinel).unwrap();
        let mut a = auto(&dir.0);
        let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
        ok(&mut a, "doc_set_info", json!({ "doc": doc, "key": "Title", "value": "Unsaved title" }));
        ok(&mut a, "doc_open", json!({ "path": "b.pdf" }));
        let list = ok(&mut a, "doc_list", json!({}));
        let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
        let text = page_text(&mut a, doc);
        let bytes = a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes.clone();
        for invalid in [json!(1), json!(true), json!({ "page": 1 }), json!(["1"])] {
            for (index, pages) in [json!([invalid, null]), json!([null, invalid])].into_iter().enumerate() {
                for out in ["combined.pdf", "not-created.pdf"] {
                    let error = a.call("doc_combine", &json!({ "paths": ["a.pdf", "b.pdf"], "pages": pages, "out": out, "open": true })).unwrap_err();
                    assert!(
                        matches!(error, ToolError::InvalidArgs(ref message) if message == &format!("pages[{index}] must be a range string or null"))
                    );
                    assert_eq!(std::fs::read(dir.0.join("combined.pdf")).unwrap(), sentinel);
                    assert!(!dir.0.join("not-created.pdf").exists());
                    assert_eq!(ok(&mut a, "doc_list", json!({})), list);
                    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc })), info);
                    assert_eq!(page_text(&mut a, doc), text);
                    assert_eq!(a.session().get(pdfcraft_engine::DocId(doc)).unwrap().bytes, bytes);
                    assert_eq!(std::fs::read(dir.0.join("a.pdf")).unwrap(), first);
                    assert_eq!(std::fs::read(dir.0.join("b.pdf")).unwrap(), second);
                }
            }
        }
        let error = a
            .call("doc_combine", &json!({ "paths": ["missing-a.pdf", "missing-b.pdf"], "pages": [null, false], "out": "not-created.pdf" }))
            .unwrap_err();
        assert!(matches!(error, ToolError::InvalidArgs(ref message) if message == "pages[1] must be a range string or null"));
        assert_eq!(ok(&mut a, "doc_list", json!({})), list);
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 3, "no output or staging files are created");
    }

    #[test]
    fn valid_combine_selectors_keep_all_pages_and_selected_order_after_reopen() {
        let dir = CombineDir::new();
        let first = std::fs::read(dir.0.join("a.pdf")).unwrap();
        let second = std::fs::read(dir.0.join("b.pdf")).unwrap();
        let mut a = auto(&dir.0);
        let all = vec!["Page 1", "Page 2", "Page 3", "Page 1", "Page 2"];
        for (pages, expected) in [
            (None, all.clone()),
            (Some(Value::Null), all.clone()),
            (Some(json!([null, null])), all.clone()),
            (Some(json!(["", null])), all),
            (Some(json!(["3, 1", null])), vec!["Page 3", "Page 1", "Page 1", "Page 2"]),
            (Some(json!([null, "2"])), vec!["Page 1", "Page 2", "Page 3", "Page 2"]),
        ] {
            let mut args = json!({ "paths": ["a.pdf", "b.pdf"], "out": "combined.pdf", "open": false });
            if let Some(pages) = pages {
                args["pages"] = pages;
            }
            let result = ok(&mut a, "doc_combine", args);
            assert!(result["bytes"].as_u64().unwrap() > 0);
            assert!(result.get("document").is_none());
            assert!(a.session().docs().is_empty());
            let mut fresh = auto(&dir.0);
            let opened = ok(&mut fresh, "doc_open", json!({ "path": "combined.pdf" }));
            assert_eq!(opened["pages"].as_u64().unwrap(), expected.len() as u64);
            assert_eq!(page_text(&mut fresh, opened["doc"].as_u64().unwrap()), expected);
            assert_eq!(std::fs::read(dir.0.join("a.pdf")).unwrap(), first);
            assert_eq!(std::fs::read(dir.0.join("b.pdf")).unwrap(), second);
        }
    }
}

/// A two-page PDF with the same three-row table ("Name Qty / Apple 12 / Pear 7"); the second
/// page is turned with /Rotate 90.
fn table_pdf() -> Vec<u8> {
    let body = "BT /F1 12 Tf 20 250 Td (Name) Tj 130 0 Td (Qty) Tj -130 -20 Td (Apple) Tj 130 0 Td (12) Tj -130 -20 Td (Pear) Tj 130 0 Td (7) Tj ET";
    let stream = format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len());
    pdf_of(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 /MediaBox [0 0 300 300] >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        "<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
        "<< /Type /Page /Parent 2 0 R /Rotate 90 /Contents 6 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
        &stream,
    ])
}

/// A 2-page 200×300 pt PDF in the `fixture()` style for the reading-colours test: bold black
/// "Page N" text on a white page; page 2 also draws a 4×4 uncompressed `/DeviceRGB` image,
/// scaled to 40×40 pt at 120, 200.
fn reading_colors_fixture() -> Vec<u8> {
    let body1 = "BT /F1 48 Tf 20 150 Td (Page 1) Tj ET";
    let body2 = "BT /F1 48 Tf 20 150 Td (Page 2) Tj ET\nq 40 0 0 40 120 200 cm /Im1 Do Q";
    // 16 opaque pure-red pixels; images are never remapped by the reading-colours setting.
    let image: Vec<u8> = (0..16).flat_map(|_| [0xff_u8, 0x00, 0x00]).collect();
    let mut objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 7 0 R >> >> >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> /XObject << /Im1 8 0 R >> >> >>".to_vec(),
        format!("<< /Length {} >>\nstream\n{body1}\nendstream", body1.len()).into_bytes(),
        format!("<< /Length {} >>\nstream\n{body2}\nendstream", body2.len()).into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_vec(),
    ];
    let mut image_obj = format!(
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
        image.len()
    )
    .into_bytes();
    image_obj.extend_from_slice(&image);
    image_obj.extend_from_slice(b"\nendstream");
    objs.push(image_obj);
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

/// A fresh directory whose `a.pdf` is `reading_colors_fixture()`.
fn reading_colors_workdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfcraft-automation-reading-colors-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.pdf"), reading_colors_fixture()).unwrap();
    dir
}

/// Decodes a `page_render` PNG (pages here are 200×300 pt) into RGBA pixels. Opaque pages
/// come as RGB; their alpha is 255.
fn png_pixels(content: &Content) -> Vec<u8> {
    let Content::Png { data, width, height } = content else { panic!("expected a PNG") };
    assert_eq!((*width, *height), (200, 300));
    let mut reader = png::Decoder::new(std::io::Cursor::new(data)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (200, 300));
    match info.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => panic!("unexpected colour type {other:?}"),
    }
}

/// The RGBA pixel at (`x`, `y`), top-left origin, of a 200-wide image.
fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * 200 + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}

/// A 300 × 100 page: red in layer "Walls" (left), green in "Doors" (middle), blue in a second
/// group also named "Walls" (right), and a black Square comment along the bottom.
fn layered_pdf() -> Vec<u8> {
    let content = b"/OC /W1 BDC 1 0 0 rg 10 30 80 60 re f EMC /OC /D BDC 0 1 0 rg 110 30 80 60 re f EMC /OC /W2 BDC 0 0 1 rg 210 30 80 60 re f EMC";
    let ap = b"0 g 0 0 280 10 re f";
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R 7 0 R] /D << /Order [5 0 R 6 0 R 7 0 R] >> >> >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Contents 4 0 R /Annots [8 0 R] /Resources << /Properties << /W1 5 0 R /D 6 0 R /W2 7 0 R >> >> >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(), content, b"\nendstream"].concat(),
        b"<< /Type /OCG /Name (Walls) >>".to_vec(),
        b"<< /Type /OCG /Name (Doors) >>".to_vec(),
        b"<< /Type /OCG /Name (Walls) >>".to_vec(),
        b"<< /Type /Annot /Subtype /Square /Rect [10 2 290 12] /C [0 0 0] /AP << /N 9 0 R >> >>".to_vec(),
        [format!("<< /Type /XObject /Subtype /Form /BBox [0 0 280 10] /Length {} >>\nstream\n", ap.len()).as_bytes(), ap, b"\nendstream"].concat(),
    ];
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

/// Three paragraphs: Alpha (two rows), Bravo (one row) and Charlie (two rows), at the leading
/// of #2516. Alpha's last baseline is 35.6 pt above Bravo's; moving Alpha down 21.2 pt leaves
/// 14.4 pt, Alpha's own leading, so the next read groups the two as one paragraph.
fn three_paragraph_pdf() -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, Stream, write_full};
    use std::sync::Arc;

    let content = "BT /F1 12 Tf 14.4 TL 1 0 0 1 10 250 Tm (Alpha paragraph, first row.) Tj T* (Alpha paragraph, second row.) Tj ET \
                   BT /F1 12 Tf 14.4 TL 1 0 0 1 10 200 Tm (Bravo paragraph, a single row.) Tj ET \
                   BT /F1 12 Tf 14.4 TL 1 0 0 1 10 150 Tm (Charlie paragraph, first row.) Tj T* (Charlie paragraph, second row.) Tj ET";
    let mut doc = Document::open(Arc::new(fixture(1))).unwrap();
    doc.set(ObjRef::new(5, 0), Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    write_full(&doc, &SaveOptions::default()).unwrap()
}

/// A one-page PDF whose second paragraph is drawn over the first one's box, as an OCR text layer
/// or a stamp would be.
fn overlaid_paragraph_pdf() -> Vec<u8> {
    use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, Stream, write_full};
    use std::sync::Arc;

    let content = "BT /F1 12 Tf 14.4 TL 1 0 0 1 10 250 Tm (Alpha paragraph, first row.) Tj T* (Alpha paragraph, second row.) Tj ET \
                   BT /F1 12 Tf 14.4 TL 1 0 0 1 10 243 Tm (Zulu overlay.) Tj ET";
    let mut doc = Document::open(Arc::new(fixture(1))).unwrap();
    doc.set(ObjRef::new(5, 0), Object::Stream(Stream::flate(Dict::new(), content.as_bytes())));
    write_full(&doc, &SaveOptions::default()).unwrap()
}

/// Where the first bookmark on `path` goes, as `bookmark_list` reports it: points from the
/// top-left of the displayed page.
fn bookmark_destination(a: &mut Automation, doc: u64, title: &str) -> Value {
    let list = ok(a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    let row = list.as_array().unwrap().iter().find(|r| r["title"] == title).unwrap_or_else(|| panic!("{title} in {list}"));
    row["destination"].clone()
}
/// `/I` of the top-level field `name` in the saved file `path`.
fn saved_selection_indices(path: &Path, name: &str) -> Option<Vec<i64>> {
    let d = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(path).unwrap())).unwrap();
    let root = d.get(d.root().unwrap()).as_dict().cloned().unwrap();
    let form = d.resolve(root.get(b"AcroForm").unwrap()).as_dict().cloned().unwrap();
    let all = d.resolve(form.get(b"Fields").unwrap()).as_array().cloned().unwrap();
    let f = all
        .iter()
        .filter_map(|o| d.resolve(o).as_dict().cloned())
        .find(|f| f.get(b"T").and_then(|t| d.resolve(t).as_string().map(|s| s.to_text())).as_deref() == Some(name))
        .unwrap();
    f.get(b"I").map(|i| d.resolve(i).as_array().unwrap().iter().map(|o| o.as_int().unwrap()).collect())
}

fn paragraph_texts(a: &mut Automation, doc: u64) -> Vec<String> {
    ok(a, "text_paragraphs", json!({ "doc": doc, "page": 1 }))["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap().to_string())
        .collect()
}

/// Unambiguous separated one-pixel rules test the requested scale independently of the
/// conservative local-tone guard. Ragged text can legitimately be retained by that guard.
fn simple_rule_scan_drawn(content: &[u8]) -> Vec<u8> {
    use pdfcraft_cos::{Document, Object, SaveOptions, Stream, write_full};
    let mut doc = Document::open(std::sync::Arc::new(scan_drawn(content))).unwrap();
    for number in doc.object_numbers() {
        let reference = pdfcraft_cos::ObjRef::new(number, doc.generation(number));
        let object = doc.get(reference);
        let Object::Stream(image) = &*object else { continue };
        if image.dict.name(b"Subtype") != Some(b"Image") {
            continue;
        }
        let mut data = vec![0xFF; 125 * 400];
        for y in 0..400 {
            data[y * 125] &= 0x7F;
            data[y * 125 + 124] &= 0xFE;
        }
        doc.set(reference, Object::Stream(Stream::flate(image.dict.clone(), &data)));
    }
    write_full(&doc, &SaveOptions::default()).unwrap()
}

/// A one-page document whose outline has an untitled item, a /FitR destination with three of
/// its four numbers, and a chain nested `deep` levels below them.
fn odd_outline(deep: usize) -> Vec<u8> {
    let first_deep = 7;
    let mut objs = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".into(),
        "<< /Type /Page /Parent 2 0 R >>".into(),
        "<< /Type /Outlines /First 5 0 R /Last 6 0 R >>".into(),
        "<< /Title () /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>".into(),
        format!(
            "<< /Title (Short) /Parent 4 0 R /Prev 5 0 R /Dest [3 0 R /FitR 0 0 10]{} >>",
            if deep > 0 { format!(" /First {first_deep} 0 R /Last {first_deep} 0 R") } else { String::new() }
        ),
    ];
    for i in 0..deep {
        let (me, parent) = (first_deep + i, if i == 0 { 6 } else { first_deep + i - 1 });
        let child = if i + 1 < deep { format!(" /First {0} 0 R /Last {0} 0 R", me + 1) } else { String::new() };
        objs.push(format!("<< /Title (Level) /Parent {parent} 0 R /Dest [3 0 R /Fit]{child} >>"));
    }
    pdf_of(&objs.iter().map(String::as_str).collect::<Vec<_>>())
}
