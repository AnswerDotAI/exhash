//! Markdown outline scan: headings, fenced-block handling, and the inline-link
//! table, matching the semantics established by toolslm's read_md.

use regex::Regex;
use std::sync::LazyLock;

static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*#*\s*$").unwrap());
static FENCE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(`{3,}|~{3,})").unwrap());
static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\(([^)\s]+)\)").unwrap());

/// One ATX heading: level, title, and the span of its section.
/// Lines are 1-based; `end_line` is the last non-blank line before the next
/// heading of the same or a higher level.
pub struct HeadingRow {
    pub level: usize,
    pub title: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// One inline link (images excluded), numbered in document reading order.
/// `tail` is the rest of the line after the link, up to the next link.
pub struct LinkRow {
    pub n: usize,
    pub txt: String,
    pub url: String,
    pub tail: String,
    pub line: usize,
}

fn clean_tail(tail: &str) -> String {
    tail.trim().trim_start_matches(':').trim().to_string()
}

/// Scan Markdown into heading and link tables. With `rm_fenced`, backtick and
/// tilde fenced blocks (and the fence lines themselves) are invisible to both.
pub fn scan_md(text: &str, rm_fenced: bool) -> (Vec<HeadingRow>, Vec<LinkRow>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut headings: Vec<(usize, String, usize)> = Vec::new();
    let mut links: Vec<LinkRow> = Vec::new();
    let mut fence: Option<char> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(m) = FENCE_RE.captures(line) {
            let marker = m[1].chars().next().unwrap();
            match fence {
                Some(f) if f == marker => fence = None,
                None => fence = Some(marker),
                _ => {}
            }
            if rm_fenced {
                continue;
            }
        }
        if fence.is_some() && rm_fenced {
            continue;
        }
        if let Some(m) = HEADING_RE.captures(line) {
            headings.push((m[1].len(), m[2].to_string(), i));
        }
        let ms: Vec<regex::Captures> = LINK_RE
            .captures_iter(line)
            .filter(|c| {
                let start = c.get(0).unwrap().start();
                start == 0 || line.as_bytes()[start - 1] != b'!'
            })
            .collect();
        for (j, m) in ms.iter().enumerate() {
            let end = m.get(0).unwrap().end();
            let tail_end = ms
                .get(j + 1)
                .map(|nm| nm.get(0).unwrap().start())
                .unwrap_or(line.len());
            links.push(LinkRow {
                n: links.len() + 1,
                txt: m[1].to_string(),
                url: m[2].to_string(),
                tail: clean_tail(&line[end..tail_end]),
                line: i + 1,
            });
        }
    }
    let rows = headings
        .iter()
        .enumerate()
        .map(|(i, (level, title, start))| {
            let end_excl = headings[i + 1..]
                .iter()
                .find(|(l, _, _)| l <= level)
                .map(|(_, _, s)| *s)
                .unwrap_or(lines.len());
            let mut end = end_excl;
            while end > *start + 1 && lines[end - 1].trim().is_empty() {
                end -= 1;
            }
            HeadingRow {
                level: *level,
                title: title.clone(),
                start_line: start + 1,
                end_line: end,
            }
        })
        .collect();
    (rows, links)
}

use tree_sitter::{Language, Node, Parser};

fn lang_of(name: &str) -> Option<Language> {
    Some(match name {
        "python" => tree_sitter_python::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "zig" => tree_sitter_zig::LANGUAGE.into(),
        "swift" => tree_sitter_swift::LANGUAGE.into(),
        _ => return None,
    })
}

fn section_kinds(lang: &str) -> &'static [&'static str] {
    match lang {
        "python" => &["function_definition", "class_definition"],
        "javascript" | "typescript" | "tsx" => &[
            "function_declaration",
            "generator_function_declaration",
            "class_declaration",
            "abstract_class_declaration",
            "method_definition",
            "interface_declaration",
            "type_alias_declaration",
            "enum_declaration",
        ],
        "rust" => &[
            "function_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "impl_item",
            "mod_item",
            "union_item",
        ],
        "zig" => &["function_declaration", "test_declaration"],
        "swift" => &[
            "function_declaration",
            "class_declaration",
            "protocol_declaration",
            "protocol_function_declaration",
            "init_declaration",
        ],
        _ => &[],
    }
}

fn node_text(node: Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or_default().to_string()
}

/// A code section's `(title, start_line, end_line)`, if `node` starts one.
fn section_of(node: Node, src: &[u8], lang: &str) -> Option<(String, usize, usize)> {
    let kind = node.kind();
    let (start, end) = (node.start_position().row + 1, node.end_position().row + 1);
    if matches!(lang, "javascript" | "typescript" | "tsx") && kind == "variable_declarator" {
        let value = node.child_by_field_name("value")?;
        if !matches!(value.kind(), "arrow_function" | "function_expression") {
            return None;
        }
        let name = node_text(node.child_by_field_name("name")?, src);
        let stmt = node
            .parent()
            .filter(|p| matches!(p.kind(), "lexical_declaration" | "variable_declaration"));
        let start = stmt.map(|p| p.start_position().row + 1).unwrap_or(start);
        return Some((name, start, end));
    }
    if !section_kinds(lang).contains(&kind) {
        return None;
    }
    if lang == "zig" && kind == "test_declaration" {
        let mut cursor = node.walk();
        let title = node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "string")
            .map(|s| format!("test {}", node_text(s, src)))
            .unwrap_or_else(|| "test".to_string());
        return Some((title, start, end));
    }
    if lang == "rust" && kind == "impl_item" {
        let ty = node_text(node.child_by_field_name("type")?, src);
        let title = match node.child_by_field_name("trait") {
            Some(t) => format!("impl {} for {ty}", node_text(t, src)),
            None => format!("impl {ty}"),
        };
        return Some((title, start, end));
    }
    let name = node_text(node.child_by_field_name("name")?, src);
    let start = match node.parent() {
        Some(p) if p.kind() == "decorated_definition" => p.start_position().row + 1,
        _ => start,
    };
    Some((name, start, end))
}

fn walk_code(node: Node, src: &[u8], lang: &str, depth: usize, rows: &mut Vec<HeadingRow>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some((title, start_line, end_line)) = section_of(child, src, lang) {
            rows.push(HeadingRow {
                level: depth,
                title,
                start_line,
                end_line,
            });
            walk_code(child, src, lang, depth + 1, rows);
        } else {
            walk_code(child, src, lang, depth, rows);
        }
    }
}

/// Scan source code into preorder section rows using the language's tree-sitter
/// grammar. `level` is section nesting depth, not AST depth.
pub fn scan_code(text: &str, lang: &str) -> Result<Vec<HeadingRow>, String> {
    let language = lang_of(lang).ok_or_else(|| format!("unsupported language: {lang}"))?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| format!("loading {lang} grammar: {e}"))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| format!("parsing {lang} source failed"))?;
    let mut rows = Vec::new();
    walk_code(tree.root_node(), text.as_bytes(), lang, 1, &mut rows);
    Ok(rows)
}
