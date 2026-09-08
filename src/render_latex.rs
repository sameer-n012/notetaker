/*
 * Manages rendering of a Document into LaTeX, using the definitions in a
 * LabelMap.
 */

use crate::ast::{Document, Inline, Node};
use crate::defs::{substitute, LabelMap};
use crate::numbering::{format_number, Counters};

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
                        out.push_str(&substitute(&def.latex_template, &[], m, None, None, &[]))
                    }
                    None => out.push_str(&format!("\n\\[{m}\\]\n")),
                }
                out.push_str("\n\n");
            } else {
                render_inlines(inlines, counters, out);
                out.push_str("\n\n");
            }
        }
        Node::Code { lang, source } => {
            // Fallback to plain code if the language given is not supported
            // by the listings package.
            let safe_lang = lang.as_deref().filter(|l| is_listings_language(l));
            match defs.get("code") {
                Some(def) => {
                    let lang_opt = safe_lang
                        .map(|l| format!("[language={l}]"))
                        .unwrap_or_default();
                    let args: Vec<String> = lang.iter().cloned().collect();
                    out.push_str(&substitute(
                        &def.latex_template,
                        &args,
                        source,
                        None,
                        None,
                        &[("lang_opt", lang_opt.as_str())],
                    ));
                    out.push_str("\n\n");
                }
                None => match safe_lang {
                    Some(l) => out.push_str(&format!(
                        "\\begin{{lstlisting}}[language={l}]\n{source}\n\\end{{lstlisting}}\n\n"
                    )),
                    None => out.push_str(&format!(
                        "\\begin{{lstlisting}}\n{source}\n\\end{{lstlisting}}\n\n"
                    )),
                },
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
                        &def.latex_template,
                        args,
                        body.trim_end(),
                        id.as_deref(),
                        number.as_deref(),
                        &[],
                    ));
                    out.push_str("\n\n");
                }
                None => {
                    // No def file for this label, fall back to a LaTeX
                    // environment with this name.
                    out.push_str(&format!("\\begin{{{label}}}\n"));
                    if let Some(id) = id {
                        out.push_str(&format!("\\label{{{id}}}\n"));
                    }
                    out.push_str(&body);
                    out.push_str(&format!("\n\\end{{{label}}}\n\n"));
                }
            }
        }
    }
}

/*
 * Listings package will crash on compilation if the language is not one of
 * its supported ones. This function returns true if the given language is
 * supported, false otherwise.
 */
fn is_listings_language(lang: &str) -> bool {
    matches!(
        lang.to_ascii_lowercase().as_str(),
        "ada"
            | "awk"
            | "bash"
            | "c"
            | "c++"
            | "csh"
            | "fortran"
            | "html"
            | "java"
            | "lisp"
            | "make"
            | "matlab"
            | "ocaml"
            | "pascal"
            | "perl"
            | "php"
            | "prolog"
            | "python"
            | "r"
            | "ruby"
            | "sh"
            | "sql"
            | "tex"
            | "verilog"
            | "vhdl"
            | "xml"
    )
}

fn render_inlines(inlines: &[Inline], counters: &Counters, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(c) => out.push_str(&format!("\\texttt{{\\detokenize{{{c}}}}}")),
            Inline::MathInline(m) => out.push_str(&format!("${m}$")),
            Inline::MathDisplay(m) => out.push_str(&format!("\n\\[{m}\\]\n")),

            // Use hyperref because \ref doesn't work with custom numbering.
            Inline::Ref(id) => match counters.ids.get(id) {
                Some(n) => out.push_str(&format!("\\hyperref[{id}]{{{n}}}")),
                None => out.push_str(&format!("?{id}")),
            },
        }
    }
}
