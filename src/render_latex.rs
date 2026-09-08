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

fn render_nodes(nodes: &[Node], defs: &LabelMap, counters: &Counters, prefix: &[u32], counter: &mut u32, out: &mut String) {
    for node in nodes {
        render_node(node, defs, counters, prefix, counter, out);
    }
}

fn render_node(node: &Node, defs: &LabelMap, counters: &Counters, prefix: &[u32], counter: &mut u32, out: &mut String) {
    match node {
        Node::Paragraph(inlines) => {
            if let [Inline::MathDisplay(m)] = inlines.as_slice() {
                match defs.get("mlmath") {
                    Some(def) => out.push_str(&substitute(&def.latex_template, &[], m, None, None, &[])),
                    None => out.push_str(&format!("\n\\[{m}\\]\n")),
                }
                out.push_str("\n\n");
            } else {
                render_inlines(inlines, counters, out);
                out.push_str("\n\n");
            }
        }
        Node::Code { lang, source } => {
            // `listings` only ships highlighting rules for a fixed, dated
            // set of languages (no Rust, Go, TypeScript, ...); passing an
            // unsupported name as `language=` is a hard compile error, so
            // fall back to a plain (uncolored) listing instead of failing
            // the whole document.
            let safe_lang = lang.as_deref().filter(|l| is_listings_language(l));
            match defs.get("code") {
                Some(def) => {
                    let lang_opt = safe_lang.map(|l| format!("[language={l}]")).unwrap_or_default();
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
                    None => out.push_str(&format!("\\begin{{lstlisting}}\n{source}\n\\end{{lstlisting}}\n\n")),
                },
            }
        }
        Node::Block { label, args, id, children } => {
            let numbered = defs.get(label).is_some_and(|d| d.numbered);

            let (number, body) = if numbered {
                *counter += 1;
                let mut path = prefix.to_vec();
                path.push(*counter);
                let mut child_counter = 0u32;
                let mut body = String::new();
                render_nodes(children, defs, counters, &path, &mut child_counter, &mut body);
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
                    // no def file for this label: fall back to a same-named environment
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

fn is_listings_language(lang: &str) -> bool {
    matches!(
        lang.to_ascii_lowercase().as_str(),
        "ada" | "awk" | "bash" | "c" | "c++" | "csh" | "fortran" | "html" | "java" | "lisp" | "make" | "matlab"
            | "ocaml" | "pascal" | "perl" | "php" | "prolog" | "python" | "r" | "ruby" | "sh" | "sql" | "tex"
            | "verilog" | "vhdl" | "xml"
    )
}

fn render_inlines(inlines: &[Inline], counters: &Counters, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(c) => out.push_str(&format!("\\texttt{{\\detokenize{{{c}}}}}")),
            Inline::MathInline(m) => out.push_str(&format!("${m}$")),
            Inline::MathDisplay(m) => out.push_str(&format!("\n\\[{m}\\]\n")),
            // Numbers are our own hierarchical scheme, not LaTeX's native
            // theorem counters, so `\ref{}` wouldn't show the right value;
            // use \hyperref to a \label instead, keeping the PDF clickable.
            Inline::Ref(id) => match counters.ids.get(id) {
                Some(n) => out.push_str(&format!("\\hyperref[{id}]{{{n}}}")),
                None => out.push_str(&format!("?{id}")),
            },
        }
    }
}
