/*
 * Reformats .note source into a canonical layout, by parsing it into a
 * document and pretty-printing that back out.
 */

use crate::ast::{Document, Inline, Node};
use crate::parser;
use crate::watch::collect_note_files;
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::Path;

const INDENT_UNIT: &str = "    ";

pub fn run(path: Option<&Path>, write: bool, check: bool) -> Result<()> {
    match path {
        None => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .context("reading stdin")?;
            let formatted = format_source(&input);
            if check {
                if formatted != input {
                    bail!("input is not formatted");
                }
            } else {
                print!("{formatted}");
            }
            Ok(())
        }
        Some(path) => {
            let files = collect_note_files(path)?;
            let mut unformatted = Vec::new();

            for file in files {
                let input = std::fs::read_to_string(&file)
                    .with_context(|| format!("reading {}", file.display()))?;
                let formatted = format_source(&input);
                let already_formatted = formatted == input;

                if check {
                    if !already_formatted {
                        unformatted.push(file);
                    }
                } else if write {
                    if !already_formatted {
                        std::fs::write(&file, &formatted)
                            .with_context(|| format!("writing {}", file.display()))?;
                        println!("formatted {}", file.display());
                    }
                } else {
                    print!("{formatted}");
                }
            }

            if check && !unformatted.is_empty() {
                for file in &unformatted {
                    eprintln!("not formatted: {}", file.display());
                }
                bail!("{} file(s) are not formatted", unformatted.len());
            }
            Ok(())
        }
    }
}

fn format_source(input: &str) -> String {
    format_document(&parser::parse(input))
}

fn format_document(doc: &Document) -> String {
    let mut out = String::new();
    format_nodes(&doc.nodes, 0, &mut out);
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

fn format_nodes(nodes: &[Node], depth: usize, out: &mut String) {
    for (i, node) in nodes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_node(node, depth, out);
    }
}

fn format_node(node: &Node, depth: usize, out: &mut String) {
    let indent = INDENT_UNIT.repeat(depth);

    match node {
        Node::Paragraph(inlines) => {
            if let [Inline::MathDisplay(m)] = inlines.as_slice() {
                write_fence(&indent, "$$$", "", m, out);
            } else {
                out.push_str(&indent);
                out.push_str(&format_inlines(inlines));
                out.push('\n');
            }
        }
        Node::Code { lang, source } => {
            write_fence(&indent, "```", lang.as_deref().unwrap_or(""), source, out);
        }
        Node::Block {
            label,
            args,
            id,
            children,
        } => {
            out.push_str(&indent);
            out.push_str(label);
            if !args.is_empty() {
                out.push('(');
                out.push_str(&args.join(", "));
                out.push(')');
            }
            if let Some(id) = id {
                out.push_str(" #");
                out.push_str(id);
            }
            if children.is_empty() {
                out.push_str(" {}\n");
            } else {
                out.push_str(" {\n");
                format_nodes(children, depth + 1, out);
                out.push_str(&indent);
                out.push_str("}\n");
            }
        }
    }
}

fn write_fence(indent: &str, fence: &str, tag: &str, content: &str, out: &mut String) {
    out.push_str(indent);
    out.push_str(fence);
    out.push_str(tag);
    out.push('\n');
    for line in content.lines() {
        if !line.is_empty() {
            out.push_str(indent);
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(indent);
    out.push_str(fence);
    out.push('\n');
}

fn format_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Code(c) => {
                s.push('`');
                s.push_str(c);
                s.push('`');
            }
            Inline::MathInline(m) => {
                s.push('$');
                s.push_str(m);
                s.push('$');
            }
            Inline::MathDisplay(m) => {
                s.push_str("$$$\n");
                s.push_str(m);
                s.push_str("\n$$$");
            }
            Inline::Ref(id) => {
                s.push('@');
                s.push_str(id);
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_messy_block_into_canonical_layout() {
        let src = "theorem(  Cauchy-Schwarz  ,x)   #cs{\n   For all $u,v$: text.\n}\n";
        let formatted = format_source(src);
        assert_eq!(
            formatted,
            "theorem(Cauchy-Schwarz, x) #cs {\n    For all $u,v$: text.\n}\n"
        );
    }

    #[test]
    fn formatting_is_idempotent() {
        let src =
            "section(Intro) {\ntheorem #t {\nproof {\nsome text $a=b$ and `code` and @t\n}\n}\n}\n";
        let once = format_source(src);
        let twice = format_source(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn empty_body_stays_one_liner() {
        assert_eq!(format_source("toc {}\n"), "toc {}\n");
        assert_eq!(format_source("toc {\n}\n"), "toc {}\n");
    }

    #[test]
    fn code_and_math_fences_round_trip() {
        let src = "```python\ndef f():\n    return 1\n```\n\n$$$\nx = y\n$$$\n";
        assert_eq!(format_source(src), src);
    }
}
