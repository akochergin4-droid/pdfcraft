//! End-to-end tests of the automation tools and the MCP server, on synthetic PDFs (part 2 of 8).

#[allow(unused_imports)]
use super::*;

#[cfg(feature = "mcp")]
#[test]
fn mcp_errors() {
    let mut s = McpServer::new(Automation::new());
    assert_eq!(rpc(&mut s, 1, "initialize", json!({ "protocolVersion": "1999-01-01" }))["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(rpc(&mut s, 2, "ping", json!({}))["result"], json!({}));
    assert_eq!(rpc(&mut s, 3, "prompts/list", json!({}))["error"]["code"], -32601);
    assert_eq!(rpc(&mut s, 6, "resources/read", json!({ "uri": "pdfcraft://doc/9/info" }))["error"]["code"], -32602);
    assert_eq!(rpc(&mut s, 4, "tools/call", json!({ "name": "nope" }))["error"]["code"], -32602);
    let failed = rpc(&mut s, 5, "tools/call", json!({ "name": "doc_open", "arguments": { "path": "/definitely/not/here.pdf" } }));
    assert_eq!(failed["result"]["isError"], true);
    assert!(failed["result"]["content"][0]["text"].as_str().unwrap().contains("here.pdf"));
    let bad: Value = serde_json::from_str(&s.handle_line("{not json").unwrap()).unwrap();
    assert_eq!(bad["error"]["code"], -32700);
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_resources_expose_open_documents() {
    let dir = workdir("mcp-resources");
    let mut s = McpServer::new(auto(&dir));
    assert!(rpc(&mut s, 1, "initialize", json!({}))["result"]["capabilities"]["resources"].is_object());
    assert_eq!(rpc(&mut s, 2, "resources/list", json!({}))["result"]["resources"].as_array().unwrap().len(), 2);
    assert_eq!(rpc(&mut s, 3, "resources/templates/list", json!({}))["result"]["resourceTemplates"].as_array().unwrap().len(), 4);
    rpc(&mut s, 4, "tools/call", json!({ "name": "doc_open", "arguments": { "path": "a.pdf" } }));
    let list = rpc(&mut s, 5, "resources/list", json!({}))["result"]["resources"].as_array().cloned().unwrap();
    assert_eq!(list.len(), 2 + 3 + 2, "info, text, three page images and two session resources");
    assert_eq!(list[0]["uri"], "pdfcraft://doc/1/info");
    let read = |s: &mut McpServer, uri: &str| rpc(s, 6, "resources/read", json!({ "uri": uri }))["result"]["contents"][0].clone();
    let text = read(&mut s, "pdfcraft://doc/1/text");
    assert_eq!(text["mimeType"], "text/plain");
    assert!(text["text"].as_str().unwrap().contains("Page 2\nPage 2"), "{text}");
    assert_eq!(read(&mut s, "pdfcraft://doc/1/page/3/text")["text"], "Page 3");
    let info: Value = serde_json::from_str(read(&mut s, "pdfcraft://doc/1/info")["text"].as_str().unwrap()).unwrap();
    assert_eq!(info["pages"].as_array().unwrap().len(), 3);
    let img = read(&mut s, "pdfcraft://doc/1/page/1/image?dpi=36");
    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD.decode(img["blob"].as_str().unwrap()).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    assert_eq!(rpc(&mut s, 7, "resources/read", json!({ "uri": "pdfcraft://doc/1/page/9/image" }))["error"]["code"], -32602);
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_default_lists_every_tool_and_has_no_meta_tools() {
    let mut s = McpServer::new(Automation::new());
    let names = tool_names(&rpc(&mut s, 1, "tools/list", json!({})));
    assert_eq!(names.len(), tools().len());
    assert!(!names.iter().any(|n| n == "tool_search" || n == "tool_call"));
    assert!(!rpc(&mut s, 2, "initialize", json!({}))["result"]["instructions"].as_str().unwrap().contains("tool_search"));
    // Without --compact the meta tools do not exist.
    assert_eq!(rpc(&mut s, 3, "tools/call", json!({ "name": "tool_search" }))["error"]["code"], -32602);
}

#[cfg(feature = "mcp")]
#[test]
/// `tools/list` advertises exactly the Search & Redact patterns the registry supports, in `pattern`
/// and by name in the description, so a schema-obeying client can select every one of them.
fn the_advertised_redact_mark_schema_offers_every_pattern() {
    use pdfcraft_engine::REDACT_PATTERNS;
    let registry: Vec<&str> = REDACT_PATTERNS.iter().map(|p| p.id()).collect();
    let mut s = McpServer::new(Automation::new());
    let list = rpc(&mut s, 1, "tools/list", json!({}));
    let mark = list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "redact_mark").expect("redact_mark is listed");
    let advertised: Vec<&str> =
        mark["inputSchema"]["properties"]["pattern"]["enum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(advertised, registry, "the advertised pattern enum must be the registry's ids");
    let description = mark["description"].as_str().unwrap();
    for id in &registry {
        assert!(description.contains(id), "the description must name {id}: {description}");
    }
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_lists_the_core_tools_and_two_meta_tools() {
    let mut s = McpServer::new(Automation::new()).with_compact(true);
    let list = rpc(&mut s, 1, "tools/list", json!({}));
    let mut expected: Vec<String> = pdfcraft_automation::mcp::COMPACT_CORE_TOOLS.iter().map(|n| n.to_string()).collect();
    expected.extend(["tool_search".to_string(), "tool_call".to_string()]);
    assert_eq!(tool_names(&list), expected);
    let all: Vec<&str> = tools().iter().map(|t| t.name).collect();
    for core in pdfcraft_automation::mcp::COMPACT_CORE_TOOLS {
        assert!(all.contains(core), "core tool {core} is not in the tool table");
    }
    // Core tools are the real definitions, and the list is far smaller than the full one.
    let full = rpc(&mut McpServer::new(Automation::new()), 2, "tools/list", json!({}));
    let core_open = list["result"]["tools"][0].clone();
    assert_eq!(Some(&core_open), full["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "doc_open"));
    assert!(list.to_string().len() * 5 < full.to_string().len(), "compact {} vs full {}", list.to_string().len(), full.to_string().len());
    for meta in &list["result"]["tools"].as_array().unwrap()[pdfcraft_automation::mcp::COMPACT_CORE_TOOLS.len()..] {
        assert_eq!(meta["inputSchema"]["type"], "object");
    }
    let instructions = rpc(&mut s, 3, "initialize", json!({}))["result"]["instructions"].as_str().unwrap().to_string();
    assert!(instructions.contains("tool_search") && instructions.contains("tool_call"), "{instructions}");
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_tool_search_filters_by_query_and_category() {
    let mut s = McpServer::new(Automation::new()).with_compact(true);
    let mut search = |args: Value| rpc(&mut s, 1, "tools/call", json!({ "name": "tool_search", "arguments": args }));

    let everything = search(json!({}));
    assert_eq!(everything["result"]["isError"], false);
    assert_eq!(everything["result"]["structuredContent"]["count"], tools().len());
    let first = &everything["result"]["structuredContent"]["tools"][0];
    assert!(first["name"].is_string() && first["description"].is_string() && first["read_only"].is_boolean(), "{first}");

    let rotate = search(json!({ "query": "ROTATE" }))["result"]["structuredContent"].clone();
    let names: Vec<&str> = rotate["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"page_rotate"), "{names:?}");
    assert!(names.len() < tools().len());

    let docs = search(json!({ "category": "doc" }))["result"]["structuredContent"].clone();
    let expected = tools().iter().filter(|t| t.name.starts_with("doc_")).count();
    assert!(expected > 0);
    assert_eq!(docs["count"], expected);
    assert!(docs["tools"].as_array().unwrap().iter().all(|t| t["name"].as_str().unwrap().starts_with("doc_") && t["category"] == "doc"));

    let both = search(json!({ "category": "page", "query": "rotate" }))["result"]["structuredContent"].clone();
    assert!(both["tools"].as_array().unwrap().iter().all(|t| t["name"].as_str().unwrap().starts_with("page_")));
    assert!(both["tools"].as_array().unwrap().iter().any(|t| t["name"] == "page_rotate"));

    // No match is an empty result that lists the categories, not an error.
    let none = search(json!({ "category": "nonsense" }));
    assert_eq!(none["result"]["isError"], false);
    assert_eq!(none["result"]["structuredContent"]["count"], 0);
    assert!(none["result"]["structuredContent"]["categories"].as_array().unwrap().iter().any(|c| c == "page"));

    // One tool in full.
    let one = search(json!({ "name": "page_rotate" }))["result"]["structuredContent"]["tool"].clone();
    assert_eq!(one["name"], "page_rotate");
    assert_eq!(one["input_schema"]["type"], "object");
    let def = tools().into_iter().find(|t| t.name == "page_rotate").unwrap();
    assert_eq!(one["input_schema"], def.input_schema);
    assert_eq!(one["description"], def.description);

    // Bad arguments are tool errors.
    for bad in [json!({ "name": "no_such_tool" }), json!({ "query": 3 }), json!({ "category": ["doc"] }), json!("doc")] {
        let r = search(bad.clone());
        assert_eq!(r["result"]["isError"], true, "{bad}");
        assert!(r["result"]["content"][0]["text"].as_str().is_some_and(|t| !t.is_empty()));
    }
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_tool_call_dispatches_to_any_tool() {
    let dir = workdir("mcp-compact");
    let mut s = compact_server(&dir);
    let opened = via_tool_call(&mut s, 1, "doc_open", json!({ "path": "a.pdf" }));
    assert_eq!(opened["result"]["isError"], false);
    assert_eq!(opened["result"]["structuredContent"]["pages"], 3);

    // A tool outside the core list, through the meta tool.
    let deleted = via_tool_call(&mut s, 2, "page_delete", json!({ "doc": 1, "pages": [3] }));
    assert_eq!(deleted["result"]["isError"], false, "{deleted}");
    assert_eq!(deleted["result"]["structuredContent"]["pages"], 2);

    // The same tool called by name directly still works, and sees the same session.
    let listed = rpc(&mut s, 3, "tools/call", json!({ "name": "doc_list", "arguments": {} }));
    assert_eq!(listed["result"]["isError"], false);
    assert_eq!(listed["result"]["structuredContent"]["documents"][0]["pages"], 2);

    // Images come back as images.
    let png = via_tool_call(&mut s, 4, "page_render", json!({ "doc": 1, "page": 1, "dpi": 36 }));
    assert_eq!(png["result"]["content"][0]["type"], "image");

    // `arguments` may be left out for tools that need none.
    assert_eq!(via_tool_call(&mut s, 5, "doc_list", Value::Null)["result"]["isError"], false);
    let no_args = rpc(&mut s, 6, "tools/call", json!({ "name": "tool_call", "arguments": { "name": "doc_list" } }));
    assert_eq!(no_args["result"]["isError"], false);

    // The result equals what the direct call returns.
    let direct = rpc(&mut s, 7, "tools/call", json!({ "name": "doc_info", "arguments": { "doc": 1 } }));
    let wrapped = via_tool_call(&mut s, 8, "doc_info", json!({ "doc": 1 }));
    assert_eq!(direct["result"], wrapped["result"]);
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_tool_call_reports_errors_without_panicking() {
    let dir = workdir("mcp-compact-errors");
    let mut s = compact_server(&dir);
    let text = |r: &Value| r["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();

    let unknown = via_tool_call(&mut s, 1, "no_such_tool", json!({}));
    assert_eq!(unknown["result"]["isError"], true);
    assert!(text(&unknown).contains("no_such_tool") && text(&unknown).contains("tool_search"), "{}", text(&unknown));

    // The tool's own validation errors pass through.
    let missing = via_tool_call(&mut s, 2, "doc_open", json!({}));
    assert_eq!(missing["result"]["isError"], true);
    let outside = via_tool_call(&mut s, 3, "doc_open", json!({ "path": "/definitely/not/here.pdf" }));
    assert_eq!(outside["result"]["isError"], true);
    let wrong_type = via_tool_call(&mut s, 4, "page_delete", json!({ "doc": "one", "pages": "all" }));
    assert_eq!(wrong_type["result"]["isError"], true);

    // Malformed meta-tool arguments.
    for args in [
        json!({}),
        json!({ "name": 7 }),
        json!({ "name": "doc_list", "arguments": [1] }),
        json!("doc_list"),
        Value::Null,
        json!({ "name": "tool_call" }),
        json!({ "name": "tool_search" }),
    ] {
        let r = rpc(&mut s, 5, "tools/call", json!({ "name": "tool_call", "arguments": args }));
        assert_eq!(r["result"]["isError"], true, "{args}");
        assert!(!text(&r).is_empty());
    }

    // Direct calls keep the usual protocol error for an unknown tool, and the session survives all of it.
    assert_eq!(rpc(&mut s, 6, "tools/call", json!({ "name": "nope" }))["error"]["code"], -32602);
    assert_eq!(via_tool_call(&mut s, 7, "doc_open", json!({ "path": "a.pdf" }))["result"]["isError"], false);
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_tool_search_summarizes_the_first_sentence() {
    let mut s = McpServer::new(Automation::new()).with_compact(true);
    let mut summary = |tool: &str| {
        let found = rpc(
            &mut s,
            1,
            "tools/call",
            json!({ "name": "tool_search", "arguments": { "query": tool, "category": tool.split('_').next().unwrap() } }),
        );
        found["result"]["structuredContent"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == tool).unwrap()["description"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // Abbreviations such as "e.g." do not end the summary.
    assert_eq!(summary("page_number"), "Label a range of pages (e.g. i, ii, iii for front matter, or A-1, A-2 for an appendix).");
    assert_eq!(
        summary("accessibility_fix"),
        "Apply the checker's automatic fix for a rule: primary-language (value: the language, e.g. en-US), title (value: the title; default the current title or file name; also shows it in the title bar) or tab-order (every page tabs in structure order)."
    );
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_meta_tools_reject_unknown_arguments() {
    let dir = workdir("mcp-compact-keys");
    let mut s = compact_server(&dir);
    let list = rpc(&mut s, 1, "tools/list", json!({}));
    let metas = &list["result"]["tools"].as_array().unwrap()[pdfcraft_automation::mcp::COMPACT_CORE_TOOLS.len()..];
    assert_eq!(metas.len(), 2);
    for meta in metas {
        assert_eq!(meta["inputSchema"]["additionalProperties"], false, "{}", meta["name"]);
    }
    let text = |r: &Value| r["error"]["message"].as_str().or_else(|| r["result"]["content"][0]["text"].as_str()).unwrap_or_default().to_string();

    // A misspelled filter is an error, not an unfiltered listing.
    let typo = rpc(&mut s, 2, "tools/call", json!({ "name": "tool_search", "arguments": { "qurey": "rotate" } }));
    assert_eq!(typo["error"]["code"], -32602, "{typo}");
    assert!(text(&typo).contains("\"qurey\"") && text(&typo).contains("query"), "{}", text(&typo));
    let extra = rpc(&mut s, 3, "tools/call", json!({ "name": "tool_search", "arguments": { "query": "rotate", "extra": 1 } }));
    assert_eq!(extra["error"]["code"], -32602, "{extra}");
    assert!(text(&extra).contains("\"extra\""), "{}", text(&extra));

    // tool_call rejects extra outer keys before running anything.
    let misspelled =
        rpc(&mut s, 4, "tools/call", json!({ "name": "tool_call", "arguments": { "name": "doc_open", "argumentz": { "path": "a.pdf" } } }));
    assert_eq!(misspelled["error"]["code"], -32602, "{misspelled}");
    assert!(text(&misspelled).contains("\"argumentz\""), "{}", text(&misspelled));
    let extra = rpc(&mut s, 5, "tools/call", json!({ "name": "tool_call", "arguments": { "name": "doc_list", "unexpected": "value" } }));
    assert_eq!(extra["error"]["code"], -32602, "{extra}");
    assert!(text(&extra).contains("\"unexpected\""), "{}", text(&extra));
    assert_eq!(rpc(&mut s, 6, "tools/call", json!({ "name": "doc_list", "arguments": {} }))["result"]["structuredContent"]["documents"], json!([]));

    // Keys inside "arguments" are left to the target tool, which rejects its unknown ones itself.
    let inner = via_tool_call(&mut s, 7, "doc_list", json!({ "unexpected": "value" }));
    assert_eq!(inner["result"]["isError"], true, "{inner}");
    assert!(text(&inner).contains("doc_list: unknown argument \"unexpected\""), "{}", text(&inner));
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_compact_tool_call_keeps_the_root_confinement() {
    let root = workdir("mcp-compact-root");
    let outside_dir = workdir("mcp-compact-outside");
    let outside = outside_dir.join("a.pdf").to_string_lossy().into_owned();
    let text = |r: &Value| r["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();

    // Direct call and tool_call both refuse a real PDF outside the root, with the same words.
    let mut confined = compact_server(&root);
    let direct = rpc(&mut confined, 1, "tools/call", json!({ "name": "doc_open", "arguments": { "path": outside } }));
    assert_eq!(direct["result"]["isError"], true, "{direct}");
    assert!(text(&direct).contains("is outside the allowed directory"), "{}", text(&direct));
    let wrapped = via_tool_call(&mut confined, 2, "doc_open", json!({ "path": outside }));
    assert_eq!(wrapped["result"]["isError"], true, "{wrapped}");
    assert_eq!(text(&wrapped), text(&direct));
    // Nothing was opened.
    let listed = rpc(&mut confined, 3, "tools/call", json!({ "name": "doc_list", "arguments": {} }));
    assert_eq!(listed["result"]["structuredContent"]["documents"], json!([]));

    // The same file opens through tool_call on a server without a root, so the refusal came from the root.
    let mut open = McpServer::new(Automation::new()).with_compact(true);
    let opened = via_tool_call(&mut open, 4, "doc_open", json!({ "path": outside }));
    assert_eq!(opened["result"]["isError"], false, "{opened}");
    assert_eq!(opened["result"]["structuredContent"]["pages"], 3);

    // And a file inside the root still opens through the confined server.
    assert_eq!(via_tool_call(&mut confined, 5, "doc_open", json!({ "path": "a.pdf" }))["result"]["isError"], false);
}

#[test]
fn parallel_text_extraction_keeps_page_order_and_follows_edits() {
    let dir = workdir("parallel");
    std::fs::write(dir.join("long.pdf"), fixture(40)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "long.pdf" }))["doc"].as_u64().unwrap();
    let expected: Vec<String> = (1..=40).map(|i| format!("Page {i}")).collect();
    assert_eq!(page_text(&mut a, doc), expected);
    assert_eq!(ok(&mut a, "text_find", json!({ "doc": doc, "query": "page 3" }))["count"], 11); // 3, 30–39
    ok(&mut a, "page_delete", json!({ "doc": doc, "pages": [3] }));
    assert_eq!(ok(&mut a, "text_find", json!({ "doc": doc, "query": "page 3" }))["count"], 10);
    assert_eq!(page_text(&mut a, doc)[2], "Page 4");
}

#[test]
fn bookmarks_through_tools() {
    let dir = workdir("bookmarks");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"], json!([]));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Intro", "page": 1 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Body", "page": 2 }));
    ok(&mut a, "bookmark_add", json!({ "doc": doc, "title": "Detail", "page": 3, "parent": [2] }));
    ok(&mut a, "bookmark_move", json!({ "doc": doc, "path": [1], "parent": [2], "position": 1 })); // Intro under Body
    ok(&mut a, "bookmark_rename", json!({ "doc": doc, "path": [1, 2], "title": "Details" }));
    ok(&mut a, "bookmark_set_page", json!({ "doc": doc, "path": [1, 1], "page": 3 }));
    let list = ok(&mut a, "bookmark_list", json!({ "doc": doc }))["bookmarks"].clone();
    assert_eq!(list[0]["title"], "Body");
    assert_eq!(list[0]["children"][0]["title"], "Intro");
    assert_eq!(list[0]["children"][0]["page"], 3);
    assert_eq!(list[0]["children"][0]["path"], json!([1, 1]));
    assert_eq!(list[0]["children"][1]["title"], "Details");
    assert!(matches!(a.call("bookmark_delete", &json!({ "doc": doc, "path": [9] })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("bookmark_delete", &json!({ "doc": doc, "path": [0] })), Err(ToolError::InvalidArgs(_))));
    ok(&mut a, "bookmark_delete", json!({ "doc": doc, "path": [1, 1] }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "marked.pdf" }));
    let mut b = auto(&dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "marked.pdf" }))["doc"].as_u64().unwrap();
    let list = ok(&mut b, "bookmark_list", json!({ "doc": re }))["bookmarks"].clone();
    assert_eq!((list[0]["title"].as_str(), list[0]["children"][0]["title"].as_str()), (Some("Body"), Some("Details")));
}

#[test]
fn bookmark_list_pages_reach_bookmarks_past_the_hundred_thousandth() {
    let dir = workdir("bookmarks-large");
    std::fs::write(dir.join("large.pdf"), flat_outline_pdf(100_005)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "large.pdf" }))["doc"].as_u64().unwrap();
    // The default call is the first page of 100, with the offset of the next one.
    let first = ok(&mut a, "bookmark_list", json!({ "doc": doc }));
    assert_eq!((first["bookmarks"].as_array().unwrap().len(), first["next"].clone()), (100, json!(100)));
    assert_eq!((first["bookmarks"][0]["title"].clone(), first["bookmarks"][0]["path"].clone()), (json!("Item 0"), json!([1])));
    let late = ok(&mut a, "bookmark_list", json!({ "doc": doc, "offset": 100_000, "limit": 100 }));
    let titles: Vec<&str> = late["bookmarks"].as_array().unwrap().iter().filter_map(|b| b["title"].as_str()).collect();
    assert_eq!((titles.first().copied(), titles.last().copied(), late["next"].clone()), (Some("Item 100000"), Some("Item 100004"), json!(null)));
    assert!(matches!(a.call("bookmark_list", &json!({ "doc": doc, "limit": 1001 })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("bookmark_list", &json!({ "doc": doc, "offset": -1 })), Err(ToolError::InvalidArgs(_))));
}

/// Bookmarks and links say where on the page they go, in the same top-left points as rect, and a
/// link through a named destination says which page that is.
#[test]
fn destinations_are_listed_with_their_position() {
    let dir = workdir("destination-positions");
    let objs: Vec<&str> = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R /Names << /Dests << /Names [(sec-b) [4 0 R /XYZ 72 500 0]] >> >> >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [8 0 R 9 0 R 10 0 R] >>",
        "<< /Type /Page /Parent 2 0 R >>",
        "<< /Type /Outlines /First 6 0 R /Last 7 0 R /Count 2 >>",
        "<< /Title (Section B) /Parent 5 0 R /Next 7 0 R /Dest [4 0 R /XYZ 72 500 0] >>",
        "<< /Title (Wide view) /Parent 5 0 R /Prev 6 0 R /Dest [4 0 R /FitH 400] >>",
        "<< /Type /Annot /Subtype /Link /Rect [72 700 300 716] /Border [0 0 0] /Dest [4 0 R /XYZ 72 500 2] >>",
        "<< /Type /Annot /Subtype /Link /Rect [72 670 300 686] /Border [0 0 0] /Dest (sec-b) >>",
        "<< /Type /Annot /Subtype /Link /Rect [72 640 300 656] /Border [0 0 0] /A << /S /URI /URI (https://example.com) >> >>",
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
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    std::fs::write(dir.join("dests.pdf"), pdf).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "dests.pdf" }))["doc"].as_u64().unwrap();
    let marks = ok(&mut a, "bookmark_list", json!({ "doc": doc }));
    // 792 − 500 = 292 points from the top; a zoom of 0 keeps the reader's zoom.
    assert_eq!(marks["bookmarks"][0]["destination"], json!({ "fit": "XYZ", "left": 72.0, "top": 292.0, "zoom": null }), "{marks}");
    assert_eq!(marks["bookmarks"][1]["destination"], json!({ "fit": "FitH", "left": null, "top": 392.0 }), "{marks}");
    let links = ok(&mut a, "link_list", json!({ "doc": doc }));
    let explicit = &links["links"][0];
    assert_eq!(
        (explicit["to_page"].clone(), explicit["destination"].clone()),
        (json!(2), json!({ "fit": "XYZ", "left": 72.0, "top": 292.0, "zoom": 2.0 }))
    );
    let named = &links["links"][1];
    assert_eq!((named["to_page"].clone(), named["named"].clone(), named.get("other")), (json!(2), json!(true), None), "{links}");
    assert_eq!(named["destination"]["top"], 292.0);
    assert_eq!((links["links"][2]["url"].clone(), links["links"][2].get("destination")), (json!("https://example.com"), None));
}

#[test]
fn bookmarks_from_structure_through_tools() {
    let dir = workdir("bookmarks-structure");
    std::fs::write(
        dir.join("tagged.pdf"),
        b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 200] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R >> endobj
4 0 obj << /Type /Page /Parent 2 0 R >> endobj
5 0 obj << /Type /StructTreeRoot /K 6 0 R /RoleMap << /Heading /H1 >> >> endobj
6 0 obj << /S /Document /K [7 0 R << /S /Sect /K 8 0 R >>] >> endobj
7 0 obj << /S /Heading /Pg 3 0 R /ActualText (Report) >> endobj
8 0 obj << /S /H2 /Pg 4 0 R /Alt (Findings) >> endobj
trailer << /Root 1 0 R >>
%%EOF"
            .as_slice(),
    )
    .unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "tagged.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "bookmark_from_structure", json!({ "doc": doc }));
    let top = &r["bookmarks"][0];
    assert_eq!(
        (top["title"].as_str(), top["children"][0]["title"].as_str(), top["children"][0]["page"].as_u64()),
        (Some("Untitled"), Some("Report"), Some(1))
    );
    assert_eq!(top["children"][0]["children"][0]["title"], "Findings");
    assert_eq!(top["children"][0]["children"][0]["path"], json!([1, 1, 1]));
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "New bookmarks from structure");
    // An untagged document: an error that says why.
    let plain = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert!(matches!(a.call("bookmark_from_structure", &json!({ "doc": plain })), Err(ToolError::Failed(m)) if m.contains("no tagged headings")));
}

#[test]
fn numbering_pages_through_tools() {
    let dir = workdir("labels");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "page_number", json!({ "doc": doc, "from": 1, "to": 1, "style": "upper-roman" }));
    assert_eq!(r["labels"], json!(["I", "2", "3"]));
    let r = ok(&mut a, "page_number", json!({ "doc": doc, "from": 2, "to": 3, "prefix": "B-", "start": 5 }));
    assert_eq!(r["labels"], json!(["I", "B-5", "B-6"]));
    assert!(matches!(a.call("page_number", &json!({ "doc": doc, "from": 2, "to": 4 })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("page_number", &json!({ "doc": doc, "from": 1, "to": 1, "style": "klingon" })), Err(ToolError::InvalidArgs(_))));
}

/// Typed text and text boxes take an oval or a circle for their border through the tools;
/// comment_list reports it (a rectangle is not reported). Only typed text, dates and text boxes
/// have one, and only the three shapes.
#[test]
fn text_borders_take_an_oval_or_a_circle_through_the_tools() {
    let dir = workdir("border-shape");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(
        &mut a,
        "fill_sign_add",
        json!({ "doc": doc, "page": 1, "type": "text", "at": [20, 40], "text": "12", "border": 1, "border_shape": "circle" }),
    );
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [10, 200, 190, 240], "contents": "Hello", "width": 1, "border_shape": "Oval" }),
    );
    let list = |a: &mut Automation| ok(a, "comment_list", json!({ "doc": doc, "page": 1 }))["comments"].as_array().unwrap().clone();
    let shapes: Vec<Value> = list(&mut a).iter().map(|c| c["border_shape"].clone()).collect();
    assert_eq!(shapes, [json!("circle"), json!("oval")]);
    let typed = list(&mut a)[0].clone();
    // Snug around "12": the box no wider than the text needs, the circle as wide as that box.
    let r: Vec<f64> = typed["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let w = pdfcraft_engine::annot_text::text_width("12", 10.0) + 10.0;
    assert!((r[2] - r[0] - w).abs() < 0.05 && (r[3] - r[1] - w).abs() < 0.05, "{r:?}");
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": typed["page"], "index": typed["index"], "border_shape": "oval" }));
    assert_eq!(list(&mut a)[0]["border_shape"], "oval");
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": typed["page"], "index": typed["index"], "border_shape": "rectangle" }));
    assert!(list(&mut a)[0].get("border_shape").is_none(), "a rectangle is not reported");
    for (tool, args) in [
        ("fill_sign_add", json!({ "doc": doc, "page": 1, "type": "check", "at": [60, 60], "border_shape": "oval" })),
        ("fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "text": "7", "at": [60, 60], "border_shape": "circle" })),
        ("comment_add", json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [80, 10, 120, 40], "border_shape": "oval" })),
        ("comment_edit", json!({ "doc": doc, "page": typed["page"], "index": typed["index"], "border_shape": "star" })),
    ] {
        assert!(matches!(a.call(tool, &args), Err(ToolError::InvalidArgs(_))), "{args}");
    }
    let callout = ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "callout", "rect": [110, 100, 190, 140], "to": [40, 60], "contents": "Look" }),
    );
    assert!(matches!(
        a.call("comment_edit", &json!({ "doc": doc, "page": 1, "index": callout["comment"]["index"], "border_shape": "oval" })),
        Err(ToolError::Failed(_))
    ));
}

/// A rectangle's fill and a text box's text each take an opacity of their own through the tools
/// (the line or border keeps the comment's `opacity`); `opacity` sets the whole comment's again.
#[test]
fn comment_parts_take_opacities_of_their_own_through_the_tools() {
    let dir = workdir("part-opacity");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [10, 10, 60, 40], "fill": "yellow", "fill_opacity": 0.25 }),
    );
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [10, 200, 190, 240], "contents": "Hello", "fill": "#FFFF99", "width": 1, "fill_opacity": 0.2, "text_opacity": 1 }),
    );
    let find = |a: &mut Automation, kind: &str| {
        let list = ok(a, "comment_list", json!({ "doc": doc, "page": 1 }));
        list["comments"].as_array().unwrap().iter().find(|c| c["type"] == kind).unwrap().clone()
    };
    let rect = find(&mut a, "Square");
    assert_eq!((rect["opacity"].as_f64(), rect["fill_opacity"].as_f64(), rect.get("text_opacity")), (Some(1.0), Some(0.25), None));
    let tb = find(&mut a, "FreeText");
    assert_eq!((tb["opacity"].as_f64(), tb["fill_opacity"].as_f64(), tb["text_opacity"].as_f64()), (Some(1.0), Some(0.2), Some(1.0)));
    // The fill changes, the text stays; an undo step.
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": tb["page"], "index": tb["index"], "fill_opacity": 0.5 }));
    let tb = find(&mut a, "FreeText");
    assert_eq!((tb["fill_opacity"].as_f64(), tb["text_opacity"].as_f64()), (Some(0.5), Some(1.0)));
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Change comment properties");
    ok(&mut a, "edit_redo", json!({ "doc": doc }));
    // The whole comment's opacity: everything at it again.
    ok(&mut a, "comment_edit", json!({ "doc": doc, "page": rect["page"], "index": rect["index"], "opacity": 0.6 }));
    let rect = find(&mut a, "Square");
    assert_eq!((rect["opacity"].as_f64(), rect["fill_opacity"].as_f64()), (Some(0.6), Some(0.6)));
    // Only the parts a comment has, at 0–1.
    for (tool, args) in [
        ("comment_add", json!({ "doc": doc, "page": 1, "type": "line", "from": [0, 0], "to": [50, 50], "fill_opacity": 0.5 })),
        ("comment_add", json!({ "doc": doc, "page": 1, "type": "rectangle", "rect": [80, 10, 120, 40], "text_opacity": 0.5 })),
        ("comment_edit", json!({ "doc": doc, "page": rect["page"], "index": rect["index"], "fill_opacity": 2 })),
    ] {
        assert!(matches!(a.call(tool, &args), Err(ToolError::InvalidArgs(_))), "{args}");
    }
    assert!(matches!(
        a.call("comment_edit", &json!({ "doc": doc, "page": rect["page"], "index": rect["index"], "text_opacity": 0.5 })),
        Err(ToolError::Failed(_))
    ));
}

#[test]
fn comments_through_tools() {
    let dir = workdir("comments");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let id = add_comment_examples(&mut a, doc);
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let rect = verify_comment_listing(&list, &id);
    edit_rectangle_comment(&mut a, doc, &rect);
    edit_text_comment(&mut a, doc, &list);
    verify_comment_errors(&mut a, doc, &id);
    save_and_reopen_comments(&dir, &mut a, doc);
}

#[test]
fn freehand_highlight_through_tools() {
    let dir = workdir("freehand-highlight");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(
        &mut a,
        "comment_add",
        json!({ "doc": doc, "page": 1, "type": "freehand_highlight", "width": 100,
        "strokes": [[[20, 100], [120, 100]]] }),
    );
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 1);
    assert_eq!(list["comments"][0]["type"], "Ink");
    assert_eq!(list["comments"][0]["intent"], "FreehandHighlight");
    assert_eq!(list["comments"][0]["color"], "#FFF000");
    let undone = ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(undone["undone"], "Add freehand highlight");
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["count"], 0);
}

/// #806: a reply to another reply stays in the listing, at any depth.
#[test]
fn nested_replies_stay_in_the_listing() {
    let dir = workdir("nested-replies");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let root = ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [40, 40], "contents": "ROOT_NOTE", "author": "A" }));
    let root_id = root["comment"]["id"].as_str().unwrap().to_string();
    ok(&mut a, "comment_reply", json!({ "doc": doc, "id": root_id, "text": "DIRECT_REPLY", "author": "B" }));
    let listed = ok(&mut a, "comment_list", json!({ "doc": doc }));
    let root_comment = listed["comments"].as_array().unwrap().iter().find(|c| c["contents"] == "ROOT_NOTE").unwrap().clone();
    let direct_id =
        root_comment["replies"].as_array().unwrap().iter().find(|r| r["contents"] == "DIRECT_REPLY").unwrap()["id"].as_str().unwrap().to_string();
    ok(&mut a, "comment_reply", json!({ "doc": doc, "id": direct_id, "text": "NESTED_REPLY", "author": "C" }));
    ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [120, 120], "contents": "CONTROL_NOTE", "author": "D" }));

    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 2, "the two notes stay roots: {list}");
    let root = list["comments"].as_array().unwrap().iter().find(|c| c["contents"] == "ROOT_NOTE").unwrap().clone();
    let replies: Vec<&str> = root["replies"].as_array().unwrap().iter().filter_map(|r| r["contents"].as_str()).collect();
    assert!(replies.contains(&"DIRECT_REPLY") && replies.contains(&"NESTED_REPLY"), "the nested reply is listed: {replies:?}");
}

/// comment_delete on a reply deletes that reply and the replies to it, as the Comments panel's
/// reply menu does: the comment and its other replies stay, and one undo brings it back.
#[test]
fn deleting_a_reply_keeps_its_comment_and_the_other_replies() {
    let dir = workdir("delete-reply");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let root = ok(&mut a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [40, 40], "contents": "ROOT_NOTE", "author": "A" }));
    let root_id = root["comment"]["id"].as_str().unwrap().to_string();
    ok(&mut a, "comment_reply", json!({ "doc": doc, "id": root_id, "text": "WRONG_REPLY", "author": "B" }));
    ok(&mut a, "comment_reply", json!({ "doc": doc, "id": root_id, "text": "RIGHT_REPLY", "author": "C" }));
    let replies = |a: &mut Automation| -> Vec<Value> {
        let list = ok(a, "comment_list", json!({ "doc": doc }));
        assert_eq!(list["count"], 1, "{list}");
        list["comments"][0]["replies"].as_array().unwrap().clone()
    };
    let wrong = replies(&mut a).into_iter().find(|r| r["contents"] == "WRONG_REPLY").unwrap();
    ok(&mut a, "comment_reply", json!({ "doc": doc, "id": wrong["id"], "text": "ANSWER_TO_WRONG", "author": "D" }));
    let texts = |a: &mut Automation| -> Vec<String> { replies(a).iter().filter_map(|r| r["contents"].as_str().map(str::to_owned)).collect() };
    assert_eq!(texts(&mut a), ["WRONG_REPLY", "RIGHT_REPLY", "ANSWER_TO_WRONG"]);

    ok(&mut a, "comment_delete", json!({ "doc": doc, "page": wrong["page"], "index": wrong["index"] }));
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["comments"][0]["contents"], "ROOT_NOTE", "{list}");
    assert_eq!(texts(&mut a), ["RIGHT_REPLY"], "the reply took the answer to it along");
    assert_eq!(ok(&mut a, "edit_undo", json!({ "doc": doc }))["undone"], "Delete comment");
    assert_eq!(texts(&mut a), ["WRONG_REPLY", "RIGHT_REPLY", "ANSWER_TO_WRONG"]);
}

/// Replies thread by their `/IRT` reference, not `/NM`: PyMuPDF names the first comment on every
/// page `fitz-A0`. A shared id is refused rather than taken to mean page 1's comment.
#[test]
fn replies_thread_by_reference_when_ids_repeat_across_pages() {
    let dir = workdir("repeated-ids");
    // Contributor-original PDF, shaped like PyMuPDF output.
    std::fs::write(
        dir.join("fitz.pdf"),
        b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots [5 0 R] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots [6 0 R 7 0 R] >> endobj
5 0 obj << /Type /Annot /Subtype /Text /Rect [20 140 40 160] /NM (fitz-A0) /T (Alpha) /Contents (COVER) >> endobj
6 0 obj << /Type /Annot /Subtype /Text /Rect [20 140 40 160] /NM (fitz-A0) /T (Beta) /Contents (SECOND) >> endobj
7 0 obj << /Type /Annot /Subtype /Text /Rect [20 100 40 120] /NM (fitz-A1) /IRT 6 0 R /T (Gamma) /Contents (ANSWER) >> endobj
trailer << /Root 1 0 R >>
%%EOF",
    )
    .unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "fitz.pdf" }))["doc"].as_u64().unwrap();
    let replies = |a: &mut Automation| -> Vec<(String, Vec<String>)> {
        let list = ok(a, "comment_list", json!({ "doc": doc }));
        list["comments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let mut texts: Vec<String> =
                    c["replies"].as_array().unwrap().iter().filter_map(|r| r["contents"].as_str().map(String::from)).collect();
                texts.sort();
                (c["contents"].as_str().unwrap().to_string(), texts)
            })
            .collect()
    };
    assert_eq!(replies(&mut a), [("COVER".into(), vec![]), ("SECOND".into(), vec!["ANSWER".to_string()])]);
    let refused = err(&mut a, "comment_reply", json!({ "doc": doc, "id": "fitz-A0", "text": "LATER" }));
    assert!(refused.to_string().contains("several comments"), "{refused}");
    ok(&mut a, "comment_reply", json!({ "doc": doc, "page": 2, "index": 1, "text": "LATER" }));
    assert_eq!(replies(&mut a), [("COVER".into(), vec![]), ("SECOND".into(), vec!["ANSWER".to_string(), "LATER".to_string()])]);
}

/// A status or checkmark belongs to the annotation its `/IRT` names: one set on a reply is not
/// its comment's, as the Comments panel shows it, and is reported on the reply. One whose
/// annotation can't be found is not a comment of its own.
#[test]
fn a_checkmark_on_a_reply_is_not_its_comments() {
    let dir = workdir("reply-checkmark");
    // Contributor-original PDF: a comment, a reply, a checkmark and a status on the reply, and a
    // status whose annotation (object 9) doesn't exist.
    std::fs::write(
        dir.join("checked.pdf"),
        b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots [4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] >> endobj
4 0 obj << /Type /Annot /Subtype /Text /Rect [100 100 120 120] /T (Alpha) /Contents (COMMENT) >> endobj
5 0 obj << /Type /Annot /Subtype /Text /Rect [100 60 120 80] /IRT 4 0 R /T (Beta) /Contents (REPLY) >> endobj
6 0 obj << /Type /Annot /Subtype /Text /Rect [100 60 120 80] /IRT 5 0 R /T (Beta) /StateModel /Marked /State /Marked >> endobj
7 0 obj << /Type /Annot /Subtype /Text /Rect [100 60 120 80] /IRT 5 0 R /T (Beta) /StateModel /Review /State /Accepted >> endobj
8 0 obj << /Type /Annot /Subtype /Text /Rect [200 20 220 40] /F 30 /IRT 9 0 R /T (Gamma) /StateModel /Review /State /Rejected >> endobj
trailer << /Root 1 0 R >>
%%EOF",
    )
    .unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "checked.pdf" }))["doc"].as_u64().unwrap();
    let list = ok(&mut a, "comment_list", json!({ "doc": doc }));
    assert_eq!(list["count"], 1, "{list}");
    let c = &list["comments"][0];
    assert_eq!((&c["marked"], &c["status"]), (&json!(false), &Value::Null), "{c}");
    assert_eq!(c["replies"].as_array().map(Vec::len), Some(1), "the reply, not its states: {c}");
    let reply = &c["replies"][0];
    assert_eq!((&reply["marked"], &reply["status"]), (&json!(true), &json!("Accepted")), "{reply}");
}

#[test]
fn protecting_through_tools() {
    let dir = workdir("protect");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["security"]["protected"], false);
    assert!(matches!(a.call("doc_protect", &json!({ "doc": doc })), Err(ToolError::Failed(_))), "a password is required");
    let r = ok(
        &mut a,
        "doc_protect",
        json!({ "doc": doc, "open_password": "open", "permissions_password": "boss", "printing": "low", "changes": "comment-fill-sign" }),
    );
    assert_eq!(r["security"]["pending"], true);
    assert_eq!(
        (r["security"]["printing"].as_str(), r["security"]["annotate"].as_bool(), r["security"]["copy"].as_bool()),
        (Some("low"), Some(true), Some(false))
    );
    assert!(!r.to_string().contains("boss"), "passwords are never echoed");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "locked.pdf" }));
    // Another session: the open password is required, and the restrictions hold.
    let mut b = auto(&dir);
    assert!(matches!(b.call("doc_open", &json!({ "path": "locked.pdf" })), Err(ToolError::Failed(_))));
    let re = ok(&mut b, "doc_open", json!({ "path": "locked.pdf", "password": "open" }))["doc"].as_u64().unwrap();
    assert!(matches!(b.call("page_delete", &json!({ "doc": re, "pages": [1] })), Err(ToolError::Failed(_))));
    assert!(matches!(b.call("doc_unprotect", &json!({ "doc": re })), Err(ToolError::Failed(_))));
    ok(&mut b, "comment_add", json!({ "doc": re, "page": 1, "type": "note", "at": [10, 10], "contents": "allowed" }));
    // Copying wasn't allowed either (#568): no text or images come out, though search works.
    for (tool, args) in [
        ("text_extract", json!({ "doc": re })),
        ("text_extract", json!({ "doc": re, "format": "html" })),
        ("text_word_count", json!({ "doc": re })),
        ("text_paragraphs", json!({ "doc": re, "page": 1 })),
        ("doc_export_text", json!({ "doc": re, "path": "out.txt" })),
        ("doc_export_images", json!({ "doc": re, "folder": "pages" })),
        ("doc_export_all_images", json!({ "doc": re, "folder": "images" })),
        ("doc_export_office", json!({ "doc": re, "path": "out.docx" })),
    ] {
        match b.call(tool, &args) {
            Err(ToolError::Failed(m)) => assert!(m.contains("don't allow copying"), "{tool}: {m}"),
            other => panic!("{tool} is refused: {other:?}"),
        }
    }
    assert!(!dir.join("out.txt").exists() && !dir.join("pages").exists() && !dir.join("out.docx").exists());
    assert_eq!(ok(&mut b, "text_find", json!({ "doc": re, "query": "Page" }))["count"], 3);
    // With the permissions password everything is possible, including removing security.
    let owner = ok(&mut b, "doc_open", json!({ "path": "locked.pdf", "password": "boss" }))["doc"].as_u64().unwrap();
    assert!(ok(&mut b, "text_extract", json!({ "doc": owner }))["pages"][0]["text"].as_str().is_some_and(|t| t.contains("Page")));
    assert_eq!(ok(&mut b, "doc_unprotect", json!({ "doc": owner }))["security"]["protected"], false);
    ok(&mut b, "doc_save", json!({ "doc": owner, "path": "open.pdf" }));
    let mut c = auto(&dir);
    ok(&mut c, "doc_open", json!({ "path": "open.pdf" }));
}

/// A document that allows changes but not copying: the tools that return a page's text refuse,
/// or leave the text out where the tool also changes the page (a restyled paragraph, recognized
/// words).
#[test]
fn copy_restricted_text_stays_in_the_document() {
    let dir = workdir("copy-restricted-text");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let locked = json!({ "open_password": "open", "permissions_password": "boss", "changes": "any-except-extract" });
    let mut protect = locked.clone();
    protect["doc"] = json!(doc);
    ok(&mut a, "doc_protect", protect);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "locked.pdf" }));
    let re = ok(&mut a, "doc_open", json!({ "path": "locked.pdf", "password": "open" }))["doc"].as_u64().unwrap();
    match a.call("text_lines", &json!({ "doc": re, "page": 1 })) {
        Err(ToolError::Failed(m)) => assert!(m.contains("don't allow copying"), "{m}"),
        other => panic!("text_lines is refused: {other:?}"),
    }
    let restyled = ok(&mut a, "text_edit", json!({ "doc": re, "page": 1, "paragraph": 1, "size": 30 }));
    assert!(!restyled.to_string().contains("Page"), "{restyled}");
    // A path that would carry the page's words OUT of the document — the editable output, or
    // the review word list — is refused on a copy-restricted document (no engines needed).
    match a.call("ocr_recognize", &json!({ "doc": re, "output_mode": "editable", "out": "copy.pdf" })) {
        Err(ToolError::Failed(m)) => assert!(m.contains("don't allow copying"), "{m}"),
        other => panic!("the editable output is refused: {other:?}"),
    }
    match a.call("ocr_words", &json!({ "doc": re })) {
        Err(ToolError::Failed(m)) => assert!(m.contains("don't allow copying"), "{m}"),
        other => panic!("ocr_words is refused: {other:?}"),
    }
    if ok(&mut a, "ocr_status", json!({}))["available"] != true {
        eprintln!("skipped the OCR part: OCR models not installed");
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Recognised words stay private" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 150, "pages": [1] }));
    let png = std::fs::read_dir(dir.join("scan")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    let mut protect = locked;
    protect["doc"] = json!(scan);
    ok(&mut a, "doc_protect", protect);
    ok(&mut a, "doc_save", json!({ "doc": scan, "path": "scan.pdf" }));
    let scan = ok(&mut a, "doc_open", json!({ "path": "scan.pdf", "password": "open" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "ocr_recognize", json!({ "doc": scan }));
    assert!(r["words"].as_u64().unwrap() >= 3, "{r}");
    assert!(r["pages"][0].get("text").is_none() && !r.to_string().to_lowercase().contains("private"), "{r}");
}

/// A form secured with "fill-sign" changes and no open password (how secured forms are usually
/// distributed): its existing signature field can be signed without the permissions password,
/// the update keeps the encryption, and the signature validates after reopening. A new field
/// needs the permissions password.
#[test]
fn signing_an_encrypted_form_through_tools() {
    let dir = workdir("sign-encrypted");
    let mut a = auto(&dir);
    ok(&mut a, "sign_id_create", json!({ "name": "Ada Lovelace", "key": "p256", "password": "secret12", "path": "ada.p12" }));
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let field = ok(&mut a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "signature", "rect": [20, 200, 180, 250] }))["field"]
        .as_str()
        .unwrap()
        .to_owned();
    ok(&mut a, "doc_protect", json!({ "doc": doc, "permissions_password": "boss", "changes": "fill-sign" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "form.pdf" }));
    let mut b = auto(&dir);
    let form = ok(&mut b, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut b, "doc_info", json!({ "doc": form }))["security"]["protected"], true);
    match b.call(
        "sign_document",
        &json!({ "doc": form, "id": "ada.p12", "password": "secret12", "page": 1, "rect": [200, 200, 380, 250], "out": "new.pdf" }),
    ) {
        Err(ToolError::Failed(m)) => assert!(m.contains("adding new signature fields"), "{m}"),
        other => panic!("a new field needs the permissions password: {other:?}"),
    }
    let r = ok(&mut b, "sign_document", json!({ "doc": form, "id": "ada.p12", "password": "secret12", "field": field, "out": "signed.pdf" }));
    assert_eq!((r["signature"]["signer"].as_str(), r["signature"]["status"].as_str()), (Some("Ada Lovelace"), Some("unknown")));
    let original = std::fs::read(dir.join("form.pdf")).unwrap();
    let signed = std::fs::read(dir.join("signed.pdf")).unwrap();
    assert!(signed.starts_with(&original), "an incremental update");
    let mut c = auto(&dir);
    let re = ok(&mut c, "doc_open", json!({ "path": "signed.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut c, "doc_info", json!({ "doc": re }))["security"]["protected"], true, "still encrypted");
    ok(&mut c, "sign_trust", json!({ "paths": ["ada.p12"], "password": "secret12" }));
    let list = ok(&mut c, "sign_list", json!({ "doc": re }));
    assert_eq!((list["all_valid"].as_bool(), list["signatures"][0]["status"].as_str()), (Some(true), Some("valid")), "{list}");
}

#[test]
fn combine_opens_protected_files_with_their_passwords() {
    let dir = workdir("combine-passwords");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    // Opens with "open"; only "boss" may assemble pages.
    ok(&mut a, "doc_protect", json!({ "doc": doc, "open_password": "open", "permissions_password": "boss", "changes": "none" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "locked.pdf" }));
    let combine = |a: &mut Automation, passwords: Value| {
        a.call("doc_combine", &json!({ "paths": ["locked.pdf", "b.pdf"], "passwords": passwords, "open": true }))
    };
    let err = |r: Result<_, ToolError>| match r {
        Err(ToolError::Failed(m)) => m,
        Err(other) => panic!("expected a failure, got {other:?}"),
        Ok(_) => panic!("expected a failure"),
    };
    assert!(err(combine(&mut a, Value::Null)).contains("password-protected"));
    assert!(err(combine(&mut a, json!(["wrong", null]))).contains("password is wrong"));
    assert!(err(combine(&mut a, json!(["open", null]))).contains("don't allow copying pages"));
    assert!(matches!(combine(&mut a, json!(["boss"])), Err(ToolError::InvalidArgs(_))), "one per path");
    let done = ok(&mut a, "doc_combine", json!({ "paths": ["locked.pdf", "b.pdf"], "passwords": ["boss", null], "open": true }));
    assert!(!done.to_string().contains("boss"), "passwords are never echoed");
    let out = done["document"]["doc"].as_u64().unwrap();
    assert_eq!(page_text(&mut a, out), ["Page 1", "Page 2", "Page 3", "Page 1", "Page 2"]);
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": out }))["security"]["protected"], false, "the result is not encrypted");
}

/// Restrictions exist only behind a permissions password: open_password alone encrypts and
/// restricts nothing, and asking for a restriction without one is refused rather than ignored (#134).
#[test]
fn protecting_with_an_open_password_alone_restricts_nothing() {
    let dir = workdir("protect-open-only");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    for (key, value) in [("copy", json!(false)), ("changes", json!("none")), ("printing", json!("none")), ("accessibility", json!(false))] {
        match a.call("doc_protect", &json!({ "doc": doc, "open_password": "openme", key: value })) {
            Err(ToolError::InvalidArgs(m)) => assert!(m.contains(key) && m.contains("permissions_password"), "{key}: {m}"),
            other => panic!("{key} without permissions_password: {other:?}"),
        }
    }
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["security"]["protected"], false, "a refused call changes nothing");
    let r = ok(&mut a, "doc_protect", json!({ "doc": doc, "open_password": "openme" }));
    assert_eq!(
        (r["security"]["protected"].as_bool(), r["security"]["copy"].as_bool(), r["security"]["modify"].as_bool()),
        (Some(true), Some(true), Some(true))
    );
    assert_eq!(r["security"]["printing"], "high");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "open-only.pdf" }));
    let mut b = auto(&dir);
    let re = ok(&mut b, "doc_open", json!({ "path": "open-only.pdf", "password": "openme" }))["doc"].as_u64().unwrap();
    let s = ok(&mut b, "doc_info", json!({ "doc": re }))["security"].clone();
    assert_eq!(
        (s["protected"].as_bool(), s["copy"].as_bool(), s["modify"].as_bool(), s["printing"].as_str()),
        (Some(true), Some(true), Some(true), Some("high"))
    );
    ok(&mut b, "page_delete", json!({ "doc": re, "pages": [1] }));
    // The schema says so too.
    let def = tools().into_iter().find(|t| t.name == "doc_protect").unwrap();
    assert!(def.description.contains("permissions_password"), "{}", def.description);
    for key in ["printing", "changes", "copy", "accessibility"] {
        let desc = def.input_schema["properties"][key]["description"].as_str().unwrap();
        assert!(desc.contains("permissions_password"), "{key}: {desc}");
    }
}

/// AES-256 keys on the SASLprep form of a password, which drops soft hyphens: a password made
/// only of them is no password at all (as the permissions password, it would give everyone owner
/// rights), so it is refused rather than written.
#[test]
fn protecting_with_a_password_encryption_ignores_is_refused() {
    let dir = workdir("protect-ignored");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let answers: Vec<String> = ["open_password", "permissions_password"]
        .into_iter()
        .map(|key| match a.call("doc_protect", &json!({ "doc": doc, key: "\u{AD}" })) {
            Err(ToolError::Failed(m)) => m,
            other => format!("{key}: {other:?}"),
        })
        .collect();
    assert!(answers.iter().all(|m| m.contains("soft hyphens")), "{answers:?}");
    assert_eq!(ok(&mut a, "doc_info", json!({ "doc": doc }))["security"]["protected"], false, "a refused call changes nothing");
}

#[test]
fn forms_through_tools() {
    let dir = workdir("forms");
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dist/demo/pdfcraft-showcase.pdf");
    if !src.exists() {
        eprintln!("skipped: run `cargo xtask demo-pdf` for the showcase form");
        return;
    }
    std::fs::copy(&src, dir.join("show.pdf")).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "show.pdf" }))["doc"].as_u64().unwrap();
    let list = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let fields = list["fields"].as_array().unwrap().clone();
    assert!(fields.len() >= 5, "{list}");
    let pick = |t: &str| fields.iter().find(|f| f["type"] == t && f["read_only"] == false).cloned();
    let text = pick("text").expect("a text field");
    let mut values = serde_json::Map::new();
    values.insert(text["name"].as_str().unwrap().into(), json!("Filled by an agent"));
    if let Some(cb) = pick("checkbox") {
        values.insert(cb["name"].as_str().unwrap().into(), json!(true));
    }
    if let Some(combo) = pick("combo") {
        values.insert(combo["name"].as_str().unwrap().into(), combo["options"][1]["label"].clone());
    }
    let r = ok(&mut a, "form_fill", json!({ "doc": doc, "values": values }));
    assert_eq!(r["undo"], "Fill in form");
    let after = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let get = |name: &str| after["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).unwrap()["value"].clone();
    assert_eq!(get(text["name"].as_str().unwrap()), "Filled by an agent");
    // The page shows it.
    let page = text["page"].as_u64().unwrap();
    let found = ok(&mut a, "text_find", json!({ "doc": doc, "query": "Filled by an agent" }));
    assert_eq!(found["count"], 1, "rendered on page {page}");
    assert!(matches!(a.call("form_fill", &json!({ "doc": doc, "values": { "no such field": "x" } })), Err(ToolError::Failed(_))));
    ok(&mut a, "form_reset", json!({ "doc": doc }));
    let reset = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let v = reset["fields"].as_array().unwrap().iter().find(|f| f["name"] == text["name"]).unwrap()["value"].clone();
    assert_ne!(v, "Filled by an agent");
}

/// #4018: `form_fill` warns about a value its box cuts off at the field's fixed font size (the
/// value is saved whole), and not about one that fits.
#[test]
fn form_fill_warns_when_a_value_does_not_fit() {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 400 300] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R] >>",
        "<< /Fields [5 0 R 6 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (narrow) /Rect [10 250 150 268] /P 3 0 R >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (wide) /Rect [10 200 310 218] /P 3 0 R >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    ];
    let dir = workdir("form-overflow");
    std::fs::write(dir.join("form.pdf"), pdf_of(&objs)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    let long = "Doe, Jane (represented by John Doe)";
    let r = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "narrow": long, "wide": long } }));
    let warnings: Vec<&str> = r["warnings"].as_array().map(|w| w.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    assert_eq!(warnings.len(), 1, "{r}");
    assert!(warnings[0].starts_with("form field \"narrow\" (page 1): the value does not fit"), "{}", warnings[0]);
    let after = ok(&mut a, "form_fields", json!({ "doc": doc }));
    assert_eq!(after["fields"][0]["value"], long, "saved whole");
    let r = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "narrow": "Ada" } }));
    assert!(r.get("warnings").is_none(), "{r}");
}

/// A form whose fields are only page widgets (the `/Fields` list is empty) still lists, fills and
/// resets through the tools, and the leniency shows up as a repair note.
#[test]
fn orphan_form_fields_through_tools() {
    /// One page with two text fields that appear only as page widgets: the AcroForm lists none.
    fn orphan_form() -> Vec<u8> {
        let objs: Vec<&str> = vec![
            "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",                                     // 1
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>",                     // 2
            "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R] >>",                                 // 3
            "<< /Fields [] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 7 0 R >> >> >>",               // 4
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (alpha) /Rect [10 200 90 220] /P 3 0 R >>", // 5
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (beta) /Rect [10 150 90 170] /P 3 0 R >>",  // 6
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",                                // 7
        ];
        pdf_of(&objs)
    }
    let dir = workdir("orphan-forms");
    std::fs::write(dir.join("orphan.pdf"), orphan_form()).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "orphan.pdf" }))["doc"].as_u64().unwrap();
    let list = ok(&mut a, "form_fields", json!({ "doc": doc }));
    assert_eq!(list["count"], 2, "{list}");
    let r = ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "alpha": "Filled by an agent" } }));
    assert_eq!(r["undo"], "Fill in alpha");
    let after = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let alpha = after["fields"].as_array().unwrap().iter().find(|f| f["name"] == "alpha").unwrap();
    assert_eq!(alpha["value"], "Filled by an agent");
    // The page shows it.
    assert_eq!(ok(&mut a, "text_find", json!({ "doc": doc, "query": "Filled by an agent" }))["count"], 1);
    // The leniency is not silent: the repair note names the adopted fields.
    let info = ok(&mut a, "doc_info", json!({ "doc": doc }));
    assert!(info["repairs"].as_array().unwrap().iter().any(|r| r.as_str().unwrap_or("").contains("page annotations")), "{}", info["repairs"]);
    ok(&mut a, "form_reset", json!({ "doc": doc }));
    let reset = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let v = reset["fields"].as_array().unwrap().iter().find(|f| f["name"] == "alpha").unwrap()["value"].clone();
    assert_ne!(v, "Filled by an agent");
}

/// An auto-sized (`0 Tf`) single-line field fills to the size its height gives, as Acrobat
/// draws it, not a 12 pt cap: 30 pt tall, less border and padding, is about 21 pt text.
#[test]
fn auto_sized_text_follows_a_tall_field() {
    let objs: Vec<&str> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",                                     // 1
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 300] >>",                     // 2
        "<< /Type /Page /Parent 2 0 R /Annots [5 0 R] >>",                                       // 3
        "<< /Fields [5 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> >>",          // 4
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (tall) /Rect [10 200 290 230] /P 3 0 R >>", // 5
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",     // 6
    ];
    let dir = workdir("auto-size-tall");
    std::fs::write(dir.join("form.pdf"), pdf_of(&objs)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "form_fill", json!({ "doc": doc, "values": { "tall": "Hi" } }));
    let found = ok(&mut a, "text_find", json!({ "doc": doc, "query": "Hi" }));
    let r = &found["matches"][0]["rects"][0];
    let height = r[3].as_f64().unwrap() - r[1].as_f64().unwrap();
    assert!(height > 16.0, "12 pt text would be about 12 tall, ~21 pt about 21: {height} ({found})");
}

#[test]
fn reversing_pages_through_tools() {
    let dir = workdir("reverse");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "page_reverse", json!({ "doc": doc }));
    assert_eq!(page_text(&mut a, doc), ["Page 3", "Page 2", "Page 1"], "no pages reverses the whole document");
    ok(&mut a, "page_reverse", json!({ "doc": doc, "pages": [1, 2] }));
    assert_eq!(page_text(&mut a, doc), ["Page 2", "Page 3", "Page 1"], "only the listed pages swap");
    ok(&mut a, "edit_undo", json!({ "doc": doc }));
    assert_eq!(page_text(&mut a, doc), ["Page 3", "Page 2", "Page 1"]);
    assert!(matches!(a.call("page_reverse", &json!({ "doc": doc, "pages": [4] })), Err(ToolError::InvalidArgs(_))));
    assert!(matches!(a.call("page_reverse", &json!({ "doc": doc, "pages": [2] })), Err(ToolError::Failed(_))), "one page has nothing to reverse");
}

#[test]
fn page_duplicate_at_inserts_the_copies_there() {
    let dir = workdir("duplicate-at");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "page_duplicate", json!({ "doc": doc, "pages": [2], "at": 1 }));
    assert_eq!(r["pages"], 4);
    assert_eq!(page_text(&mut a, doc), ["Page 2", "Page 1", "Page 2", "Page 3"]);
    // `at` is 1-based and within 1..=pages+1, as for the other page inserts.
    assert!(matches!(a.call("page_duplicate", &json!({ "doc": doc, "pages": [1], "at": 0 })), Err(ToolError::InvalidArgs(_))), "1-based");
    assert!(matches!(a.call("page_duplicate", &json!({ "doc": doc, "pages": [1], "at": 6 })), Err(ToolError::InvalidArgs(_))), "past the end");
}

#[test]
fn duplicating_and_cropping_through_tools() {
    let dir = workdir("boxes");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "page_duplicate", json!({ "doc": doc, "pages": [2] }));
    assert_eq!(r["pages"], 4);
    assert_eq!(page_text(&mut a, doc), ["Page 1", "Page 2", "Page 2", "Page 3"]);
    // Crop page 1 by margins, page 2 to a rect drawn from the top-left.
    let r = ok(&mut a, "page_set_box", json!({ "doc": doc, "pages": [1], "margins": [10, 20, 30, 40] }));
    assert_eq!(r["page_sizes"][0], json!([160.0, 240.0]));
    let r = ok(&mut a, "page_set_box", json!({ "doc": doc, "pages": [2], "rect": [0, 0, 100, 150] }));
    assert_eq!(r["page_sizes"][1], json!([100.0, 150.0]));
    assert_eq!(page_text(&mut a, doc)[1], "Page 2", "the text at y = 150 is still inside");
    // Reset and errors.
    let r = ok(&mut a, "page_set_box", json!({ "doc": doc, "pages": [1] }));
    assert_eq!(r["page_sizes"][0], json!([200.0, 300.0]));
    assert!(matches!(a.call("page_set_box", &json!({ "doc": doc, "margins": [150, 0, 150, 0] })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("page_set_box", &json!({ "doc": doc, "rect": [0, 0, 10, 10] })), Err(ToolError::InvalidArgs(_))));
}

#[test]
fn headers_watermarks_and_backgrounds_through_tools() {
    let dir = workdir("marks");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_header_footer", json!({ "doc": doc, "footer_center": "<<Page 1 of n>>", "header_right": "ACME" }));
    let texts = page_text(&mut a, doc);
    assert!(texts[2].contains("Page 3 of 3") && texts[0].contains("ACME"), "{texts:?}");
    ok(&mut a, "doc_watermark", json!({ "doc": doc, "pages": [1], "text": "DRAFT", "opacity": 0.2 }));
    assert!(page_text(&mut a, doc)[0].contains("DRAFT"));
    ok(&mut a, "doc_background", json!({ "doc": doc, "color": "yellow" }));
    ok(&mut a, "doc_remove_marks", json!({ "doc": doc, "kind": "watermark" }));
    assert!(!page_text(&mut a, doc)[0].contains("DRAFT"));
    assert!(matches!(a.call("doc_remove_marks", &json!({ "doc": doc, "kind": "watermark" })), Err(ToolError::Failed(_))));
    assert!(matches!(a.call("doc_header_footer", &json!({ "doc": doc })), Err(ToolError::Failed(_))), "no text");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "marked.pdf" }));
    // A standard face other than Helvetica (#3754), kept through save and reopen; one it doesn't
    // offer is refused with the faces it does.
    let mono = ok(&mut a, "doc_open", json!({ "path": "b.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_header_footer", json!({ "doc": mono, "header_left": "MONO", "font": "Courier" }));
    assert!(page_text(&mut a, mono)[0].contains("MONO"));
    ok(&mut a, "doc_save", json!({ "doc": mono, "path": "courier.pdf" }));
    let saved = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(dir.join("courier.pdf")).unwrap())).unwrap();
    let page = &pdfcraft_model::pages(&saved)[0];
    let res = saved.resolve(page.dict.get(b"Resources").unwrap());
    let fonts = saved.resolve(res.as_dict().unwrap().get(b"Font").unwrap()).as_dict().cloned().unwrap();
    let face = saved.resolve(fonts.get(b"PCCour").expect("Courier is added")).as_dict().cloned().unwrap();
    assert_eq!(face.name(b"BaseFont"), Some(&b"Courier"[..]));
    let refused = a.call("doc_header_footer", &json!({ "doc": mono, "header_left": "X", "font": "Arial" }));
    assert!(matches!(refused, Err(ToolError::InvalidArgs(e)) if e.contains("Courier")));
}

#[test]
fn bates_numbers_continue_across_files_and_report_preflight_and_permission_failures() {
    let dir = workdir("bates-multi");
    let files = json!([
        {"input":"b.pdf","output":"b-numbered.pdf"},
        {"input":"a.pdf","output":"a-numbered.pdf"}
    ]);
    let mut a = auto(&dir);
    let allocation = ok(&mut a, "doc_bates_multi", json!({"files":files,"start_number":50,"dry_run":true}));
    assert_eq!(allocation["files"][0]["first"], 50);
    assert_eq!(allocation["files"][0]["last"], 51);
    assert_eq!(allocation["files"][1]["first"], 52);
    assert_eq!(allocation["files"][1]["last"], 54);
    assert!(!dir.join("a-numbered.pdf").exists());

    let applied = ok(&mut a, "doc_bates_multi", json!({"files":files,"start_number":17,"prefix":"CASE-"}));
    assert_eq!(applied["completed"], 2);
    let mut out = auto(&dir);
    let first = ok(&mut out, "doc_open", json!({"path":"a-numbered.pdf"}))["doc"].as_u64().unwrap();
    let second = ok(&mut out, "doc_open", json!({"path":"b-numbered.pdf"}))["doc"].as_u64().unwrap();
    assert!(page_text(&mut out, second)[0].contains("CASE-000017"));
    assert!(page_text(&mut out, first)[0].contains("CASE-000019"));

    let overflow = err(&mut a, "doc_bates_multi", json!({"files":[{"input":"b.pdf","output":"overflow.pdf"}],"start_number":u64::MAX}));
    assert!(overflow.to_string().contains("overflows"));
    assert!(!dir.join("overflow.pdf").exists());
    // A range may end at u64::MAX itself.
    let at_max = ok(&mut a, "doc_bates_multi", json!({"files":[{"input":"b.pdf","output":"max.pdf"}],"start_number":u64::MAX - 1,"dry_run":true}));
    assert_eq!(at_max["files"][0]["last"], u64::MAX);
    for bad in
        [json!({"start_number":-3}), json!({"start_number":2.5}), json!({"start_number":"7"}), json!({"prefix":"CASE>>"}), json!({"suffix":"A>>B"})]
    {
        let mut args = json!({"files":[{"input":"b.pdf","output":"bad.pdf"}],"dry_run":true});
        args.as_object_mut().unwrap().extend(bad.as_object().unwrap().clone());
        assert!(a.call("doc_bates_multi", &args).is_err(), "{bad}");
    }

    use pdfcraft_engine::{Changes, Edit, Protection, Session};
    let mut session = Session::new();
    let id = session.open("locked.pdf", None, std::sync::Arc::new(fixture(1)), None).unwrap();
    session
        .apply(
            id,
            Edit::Protect(Protection {
                open_password: Some("open".into()),
                permissions_password: Some("owner".into()),
                changes: Changes::None,
                ..Default::default()
            }),
        )
        .unwrap();
    std::fs::write(dir.join("locked.pdf"), session.save_bytes(id).unwrap().as_ref()).unwrap();
    let result = ok(
        &mut a,
        "doc_bates_multi",
        json!({"files":[
            {"input":"b.pdf","output":"partial-numbered.pdf"},
            {"input":"locked.pdf","output":"locked-numbered.pdf","password":"open"}
        ]}),
    );
    assert_eq!(result["completed"], 1);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["files"][1]["first"], 3);
    assert!(result["files"][1]["error"].as_str().unwrap().contains("owner password"), "{result}");
    assert!(dir.join("partial-numbered.pdf").exists());
    assert!(!dir.join("locked-numbered.pdf").exists());
}

#[test]
fn a_shrinking_header_and_footer_moves_the_page_content_out_of_their_way_through_tools() {
    let dir = workdir("hf-shrink");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    // The box of "Page 1" (left, top, width, height in points), read back from a saved copy.
    let mut saved = 0;
    let mut text_box = |a: &mut Automation| -> [f64; 4] {
        saved += 1;
        let path = format!("shrunk{saved}.pdf");
        ok(a, "doc_save", json!({ "doc": doc, "path": path }));
        let copy = ok(a, "doc_open", json!({ "path": path }))["doc"].as_u64().unwrap();
        let found = ok(a, "text_find", json!({ "doc": copy, "query": "Page 1" }));
        let r = found["matches"][0]["rects"][0].as_array().unwrap_or_else(|| panic!("Page 1 is on the page: {found}"));
        let v: Vec<f64> = r.iter().map(|x| x.as_f64().unwrap()).collect();
        [v[0], v[1], v[2] - v[0], v[3] - v[1]]
    };
    let before = text_box(&mut a);
    ok(
        &mut a,
        "doc_header_footer",
        json!({ "doc": doc, "pages": [1], "header_center": "ACME", "footer_right": "<<Bates Number#6#1##>>", "shrink": true }),
    );
    let after = text_box(&mut a);
    // 200 × 300 page, 8 pt text and 36 pt margins: the header takes 48 pt, the footer 46.4 pt.
    let k = (300.0 - 48.0 - 46.4) / 300.0;
    assert!((after[2] / before[2] - k).abs() < 0.02 && (after[3] / before[3] - k).abs() < 0.02, "scaled by {k}: {before:?} → {after:?}");
    assert!((after[0] - (20.0 * k + (200.0 - 200.0 * k) / 2.0)).abs() < 1.0, "centred across: {before:?} → {after:?}");
    assert!(page_text(&mut a, doc)[0].contains("000001"), "the Bates number is drawn");
    // Margins that leave no room are refused; Remove brings the content back to full size.
    let full = json!({ "doc": doc, "header_center": "X", "margins": [295, 36, 72, 72], "shrink": true, "replace": true });
    assert!(matches!(a.call("doc_header_footer", &full), Err(ToolError::Failed(_))), "no room");
    ok(&mut a, "doc_remove_marks", json!({ "doc": doc, "kind": "header_footer" }));
    let restored = text_box(&mut a);
    assert!(restored.iter().zip(before).all(|(r, b)| (r - b).abs() < 0.01), "{before:?} → {restored:?}");
}

#[test]
fn watermarks_move_from_the_page_centre_through_tools() {
    let dir = workdir("wm-offset");
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "a.pdf" }))["doc"].as_u64().unwrap();
    // The top-left corner of "DRAFT" on page 1, in points (y grows downwards).
    // Saved and reopened, so the search reads the file as another program would.
    let mut saved = 0;
    let mut corner = |a: &mut Automation| -> (f64, f64) {
        saved += 1;
        let path = format!("moved{saved}.pdf");
        ok(a, "doc_save", json!({ "doc": doc, "path": path }));
        let copy = ok(a, "doc_open", json!({ "path": path }))["doc"].as_u64().unwrap();
        let found = ok(a, "text_find", json!({ "doc": copy, "query": "DRAFT" }));
        let rect =
            found["matches"][0]["rects"][0].as_array().unwrap_or_else(|| panic!("DRAFT is on the page: {found}; text: {:?}", page_text(a, copy)));
        (rect[0].as_f64().unwrap(), rect[1].as_f64().unwrap())
    };
    // Both sit above the fixture's "Page 1" (at the page centre), or their letters would mix.
    ok(&mut a, "doc_watermark", json!({ "doc": doc, "pages": [1], "text": "DRAFT", "rotation": 0, "font_size": 20, "offset_y": 60 }));
    let centred = corner(&mut a);
    ok(
        &mut a,
        "doc_watermark",
        json!({ "doc": doc, "pages": [1], "text": "DRAFT", "rotation": 0, "font_size": 20, "offset_x": 30, "offset_y": 100, "replace": true }),
    );
    let moved = corner(&mut a);
    assert!((moved.0 - centred.0 - 30.0).abs() < 1.0, "right: {centred:?} → {moved:?}");
    assert!((centred.1 - moved.1 - 40.0).abs() < 1.0, "up is towards the top: {centred:?} → {moved:?}");
    // Huge offsets are clamped, and a value that isn't a number is refused.
    ok(&mut a, "doc_watermark", json!({ "doc": doc, "pages": [1], "text": "DRAFT", "offset_y": 1e300, "replace": true }));
    assert!(matches!(a.call("doc_watermark", &json!({ "doc": doc, "text": "X", "offset_x": "left" })), Err(ToolError::InvalidArgs(_))));
}
