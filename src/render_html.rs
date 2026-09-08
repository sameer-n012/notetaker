/*
 * Manages rendering of a Document into HTML, using the definitions in a
 * LabelMap.
 */

use crate::ast::{Document, Inline, Node};
use crate::defs::{substitute, LabelMap};
use crate::numbering::{format_number, Counters, TocEntry};

pub fn render(doc: &Document, defs: &LabelMap) -> String {
    let counters = Counters::build(&doc.nodes, defs);
    let mut counter = 0u32;
    let mut out = String::new();
    render_nodes(&doc.nodes, defs, &counters, &[], &mut counter, &mut out);
    out
}

fn render_nodes(
    nodes: &[Node],
    defs: &LabelMap,
    counters: &Counters,
    prefix: &[u32],
    counter: &mut u32,
    out: &mut String,
) {
    for node in nodes {
        render_node(node, defs, counters, prefix, counter, out);
    }
}

fn render_node(
    node: &Node,
    defs: &LabelMap,
    counters: &Counters,
    prefix: &[u32],
    counter: &mut u32,
    out: &mut String,
) {
    match node {
        Node::Paragraph(inlines) => {
            if let [Inline::MathDisplay(m)] = inlines.as_slice() {
                match defs.get("mlmath") {
                    Some(def) => {
                        out.push_str(&substitute(
                            &def.html_template,
                            &[],
                            &escape(m),
                            None,
                            None,
                            &[],
                        ));
                        out.push('\n');
                    }
                    None => out.push_str(&format!(
                        "<div class=\"math-display\">\\[{}\\]</div>\n",
                        escape(m)
                    )),
                }
            } else {
                out.push_str("<p>");
                render_inlines(inlines, counters, out);
                out.push_str("</p>\n");
            }
        }
        Node::Code { lang, source } => match defs.get("code") {
            Some(def) => {
                let class_attr = lang
                    .as_deref()
                    .map(|l| format!(" class=\"language-{l}\""))
                    .unwrap_or_default();
                let args: Vec<String> = lang.iter().cloned().collect();
                out.push_str(&substitute(
                    &def.html_template,
                    &args,
                    &escape(source),
                    None,
                    None,
                    &[("class_attr", class_attr.as_str())],
                ));
                out.push('\n');
            }
            None => {
                let class = lang
                    .as_deref()
                    .map(|l| format!(" class=\"language-{l}\""))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "<pre><code{class}>{}</code></pre>\n",
                    escape(source)
                ));
            }
        },
        Node::Block {
            label, args, id, ..
        } if label == "toc" => {
            if let Some(def) = defs.get("toc") {
                let toc_html = render_toc(&counters.toc);
                out.push_str(&substitute(
                    &def.html_template,
                    args,
                    &toc_html,
                    id.as_deref(),
                    None,
                    &[],
                ));
                out.push('\n');
            }
        }
        Node::Block {
            label,
            args,
            id,
            children,
        } => {
            let numbered = defs.get(label).is_some_and(|d| d.numbered);

            let (number, body) = if numbered {
                *counter += 1;
                let mut path = prefix.to_vec();
                path.push(*counter);
                let mut child_counter = 0u32;
                let mut body = String::new();
                render_nodes(
                    children,
                    defs,
                    counters,
                    &path,
                    &mut child_counter,
                    &mut body,
                );
                (Some(format_number(&path)), body)
            } else {
                let mut body = String::new();
                render_nodes(children, defs, counters, prefix, counter, &mut body);
                (None, body)
            };

            match defs.get(label) {
                Some(def) => {
                    out.push_str(&substitute(
                        &def.html_template,
                        args,
                        body.trim_end(),
                        id.as_deref(),
                        number.as_deref(),
                        &[],
                    ));
                    out.push('\n');
                }
                None => {
                    // if no definition, render as a div with the label as
                    // the class
                    let id_attr = id
                        .as_ref()
                        .map(|i| format!(" id=\"{i}\""))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "<div class=\"{label}\"{id_attr}>\n{body}\n</div>\n"
                    ));
                }
            }
        }
    }
}

fn render_toc(entries: &[TocEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from("<ul>\n");
    for entry in entries {
        out.push_str("<li>");
        let text = format!("{} {}", entry.number, escape(&entry.title));
        match &entry.id {
            Some(id) => out.push_str(&format!("<a href=\"#{id}\">{text}</a>")),
            None => out.push_str(&text),
        }
        if !entry.children.is_empty() {
            out.push('\n');
            out.push_str(&render_toc(&entry.children));
        }
        out.push_str("</li>\n");
    }
    out.push_str("</ul>\n");
    out
}

fn render_inlines(inlines: &[Inline], counters: &Counters, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape(t)),
            Inline::Code(c) => out.push_str(&format!("<code>{}</code>", escape(c))),
            Inline::MathInline(m) => out.push_str(&format!("\\({}\\)", escape(m))),
            Inline::MathDisplay(m) => out.push_str(&format!("\\[{}\\]", escape(m))),
            Inline::Ref(id) => match counters.ids.get(id) {
                Some(n) => out.push_str(&format!("<a href=\"#{id}\">{n}</a>")),
                None => out.push_str(&format!("<a href=\"#{id}\">?{id}</a>")),
            },
        }
    }
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
