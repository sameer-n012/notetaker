/*
 * Manages rendering of a Document into Markdown, using the definitions in a
 * LabelMap.
 */

use crate::ast::{Document, Inline, Node};
use crate::defs::{substitute, LabelMap};
use crate::numbering::{format_number, Counters, TocEntry};
use std::time::{SystemTime, UNIX_EPOCH};

struct Ctx<'a> {
    defs: &'a LabelMap,
    counters: Counters,
    today: String,
}

pub fn render(doc: &Document, defs: &LabelMap) -> String {
    let ctx = Ctx {
        defs,
        counters: Counters::build(&doc.nodes, defs),
        today: today_utc(),
    };
    let mut counter = 0u32;
    let mut out = String::new();
    render_nodes(&doc.nodes, &ctx, &[], &mut counter, &mut out);

    let mut out = out.trim_end().to_string();
    out.push('\n');
    out
}

fn render_nodes(nodes: &[Node], ctx: &Ctx, prefix: &[u32], counter: &mut u32, out: &mut String) {
    for node in nodes {
        render_node(node, ctx, prefix, counter, out);
    }
}

fn render_node(node: &Node, ctx: &Ctx, prefix: &[u32], counter: &mut u32, out: &mut String) {
    let today = ("today", ctx.today.as_str());
    match node {
        Node::Paragraph(inlines) => {
            if let [Inline::MathDisplay(m)] = inlines.as_slice() {
                let rendered = match template(ctx.defs, "mlmath") {
                    Some(tpl) => substitute(tpl, &[], m, None, None, &[today]),
                    None => format!("$$\n{m}\n$$"),
                };
                push_block(out, &rendered);
            } else {
                let mut para = String::new();
                render_inlines(inlines, &ctx.counters, &mut para);
                push_block(out, &para);
            }
        }
        Node::Code { lang, source } => {
            let fence = "`".repeat(longest_backtick_run(source).max(2) + 1);
            let rendered = match template(ctx.defs, "code") {
                Some(tpl) => {
                    let args: Vec<String> = lang.iter().cloned().collect();
                    substitute(
                        tpl,
                        &args,
                        source,
                        None,
                        None,
                        &[("fence", fence.as_str()), today],
                    )
                }
                None => format!(
                    "{fence}{}\n{source}\n{fence}",
                    lang.as_deref().unwrap_or("")
                ),
            };
            push_block(out, &rendered);
        }
        Node::Block {
            label, args, id, ..
        } if label == "toc" => {
            if let Some(tpl) = template(ctx.defs, "toc") {
                let mut toc = String::new();
                render_toc(&ctx.counters.toc, 0, &mut toc);
                push_block(
                    out,
                    &substitute(tpl, args, toc.trim_end(), id.as_deref(), None, &[today]),
                );
            }
        }
        Node::Block {
            label,
            args,
            id,
            children,
        } => {
            let def = ctx.defs.get(label);
            let numbered = def.is_some_and(|d| d.numbered);
            let join_sep = def.and_then(|d| d.markdown_join.as_deref());
            let indent = def.and_then(|d| d.markdown_indent.as_deref());

            let (number, parts) = if numbered {
                *counter += 1;
                let mut path = prefix.to_vec();
                path.push(*counter);
                let mut child_counter = 0u32;
                let parts = render_parts(children, ctx, &path, &mut child_counter);
                (Some(format_number(&path)), parts)
            } else {
                (None, render_parts(children, ctx, prefix, counter))
            };

            let joined = join_parts(&parts, join_sep);
            let body = match indent {
                Some(p) => indent_after_first(joined.trim_end(), p),
                None => joined.trim_end().to_string(),
            };

            let rendered = match template(ctx.defs, label) {
                Some(tpl) => {
                    let head = parts.first().map(|p| p.trim()).unwrap_or_default();
                    let rest = join_parts(parts.get(1..).unwrap_or_default(), join_sep);
                    let rule = md_rule(args.first().map(String::as_str));
                    substitute(
                        tpl,
                        args,
                        &body,
                        id.as_deref(),
                        number.as_deref(),
                        &[
                            ("head", head),
                            ("rest", rest.trim_end()),
                            ("md_rule", rule.as_str()),
                            today,
                        ],
                    )
                }
                None => match id {
                    Some(id) => format!("<a id=\"{id}\"></a>\n\n{body}"),
                    None => body,
                },
            };
            push_block(out, &rendered);
        }
    }
}

/*
 * Renders each child. Without a separator, the parts concatenate to the same
 * text as rendering the children in one pass.
 */
fn render_parts(children: &[Node], ctx: &Ctx, prefix: &[u32], counter: &mut u32) -> Vec<String> {
    children
        .iter()
        .map(|child| {
            let mut part = String::new();
            render_node(child, ctx, prefix, counter, &mut part);
            part
        })
        .collect()
}

fn join_parts(parts: &[String], join_sep: Option<&str>) -> String {
    match join_sep {
        None => parts.concat(),
        Some(sep) => parts.iter().map(|p| p.trim()).collect::<Vec<_>>().join(sep),
    }
}

/*
 * Appends one rendered block, followed by a blank line to separate it from
 * the next block. Leading newlines (e.g. from a `${id?...}` group that
 * rendered nothing on a template's first line) are dropped. Leading spaces are
 * kept.
 */
fn push_block(out: &mut String, rendered: &str) {
    let rendered = rendered.trim_start_matches('\n').trim_end();
    if !rendered.is_empty() {
        out.push_str(rendered);
        out.push_str("\n\n");
    }
}

/*
 * Puts `prefix` before every line of `text` except the first.
 */
fn indent_after_first(text: &str, prefix: &str) -> String {
    let mut lines = text.split('\n');
    let mut out = lines.next().unwrap_or_default().to_string();
    for line in lines {
        out.push('\n');
        if line.trim().is_empty() {
            out.push_str(prefix.trim_end());
        } else {
            out.push_str(prefix);
            out.push_str(line);
        }
    }
    out
}

fn template<'a>(defs: &'a LabelMap, label: &str) -> Option<&'a str> {
    defs.get(label).and_then(|d| d.markdown_template.as_deref())
}

/*
 * Builds a Markdown table delimiter row from a LaTeX column spec, `lcr`
 * gives `| --- | :-: | --: |`. `|`.
 *
 * @param spec The column spec (a table's argument 1), if given.
 *
 * @return The delimiter row, or an empty string if `spec` is missing or has
 * any other character (so the arg is not a column spec).
 */
fn md_rule(spec: Option<&str>) -> String {
    let cells: Option<Vec<&str>> = spec
        .unwrap_or_default()
        .chars()
        .filter(|c| *c != '|' && !c.is_whitespace())
        .map(|c| match c {
            'l' => Some("---"),
            'c' => Some(":-:"),
            'r' => Some("--:"),
            _ => None,
        })
        .collect();
    match cells {
        Some(cells) if !cells.is_empty() => format!("| {} |", cells.join(" | ")),
        _ => String::new(),
    }
}

fn render_toc(entries: &[TocEntry], depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    for entry in entries {
        let text = format!("{} {}", entry.number, entry.title);
        match &entry.id {
            Some(id) => out.push_str(&format!("{pad}- [{text}](#{id})\n")),
            None => out.push_str(&format!("{pad}- {text}\n")),
        }
        render_toc(&entry.children, depth + 1, out);
    }
}

fn render_inlines(inlines: &[Inline], counters: &Counters, out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(c) => out.push_str(&code_span(c)),
            Inline::MathInline(m) => out.push_str(&format!("${m}$")),
            Inline::MathDisplay(m) => out.push_str(&format!("$${m}$$")),
            Inline::Ref(id) => match counters.ids.get(id) {
                Some(n) => out.push_str(&format!("[{n}](#{id})")),
                None => out.push_str(&format!("?{id}")),
            },
        }
    }
}

/*
 * Wraps inline code in a backtick run longer than any run inside it. Content
 * that starts or ends with a backtick is padded with a space.
 */
fn code_span(code: &str) -> String {
    let ticks = "`".repeat(longest_backtick_run(code) + 1);
    if code.starts_with('`') || code.ends_with('`') {
        format!("{ticks} {code} {ticks}")
    } else {
        format!("{ticks}{code}{ticks}")
    }
}

fn longest_backtick_run(s: &str) -> usize {
    s.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/*
 * Formats the current UTC date like the HTML `today` output, e.g.
 * `September 23, 2026`.
 */
fn today_utc() -> String {
    // A system clock set before 1970 is a broken environment; show the epoch
    // date there, not fail the whole build over a date string.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    format!("{} {d}, {y}", MONTHS[(m - 1) as usize])
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era, [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of shifted year
    let mp = (5 * doy + 2) / 153; // shifted month, March = 0
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defs;
    use crate::parser::parse;
    use std::path::Path;

    fn repo_defs() -> LabelMap {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("defs/_labels.json");
        defs::load_all(&path).unwrap()
    }

    #[test]
    fn fallbacks_without_defs() {
        let src = "plain `a` and $x$ see @t\n\nfoo #t {\ninside\n}\n\n$$$\ny = 1\n$$$\n\n```rust\nfn main() {}\n```\n";
        let out = render(&parse(src), &LabelMap::new());
        assert_eq!(
            out,
            "plain `a` and $x$ see ?t\n\n<a id=\"t\"></a>\n\ninside\n\n$$\ny = 1\n$$\n\n```rust\nfn main() {}\n```\n"
        );
    }

    #[test]
    fn def_without_markdown_section_uses_fallback_but_keeps_numbering() {
        let src = "thing #a {\nfirst\n}\n\nthing #b {\nsecond\n}\n\nsee @b\n";
        let mut defs = LabelMap::new();
        let mut def = repo_defs()["theorem"].clone();
        def.markdown_template = None;
        defs.insert("thing".to_string(), def);
        let out = render(&parse(src), &defs);
        assert!(out.contains("<a id=\"b\"></a>\n\nsecond"));
        assert!(out.contains("see [2](#b)"));
    }

    #[test]
    fn sections_theorems_and_refs() {
        let src = "section(Intro) #s1 {\ntheorem(CS) #cs {\nholds\nproof {\ntrivial\n}\n}\n}\n\nBy @cs.\n";
        let out = render(&parse(src), &repo_defs());
        assert!(out.contains("## 1. Intro <a id=\"s1\"></a>\n\n"));
        assert!(out.contains("**Theorem 1.1 (CS).**<a id=\"cs\"></a>\nholds\n\n*Proof.* trivial"));
        assert!(out.contains("By [1.1](#cs)."));
    }

    #[test]
    fn nested_lists_indent_under_their_item() {
        let src = "ulist {\nli {\none\nolist {\nli {\ninner a\n}\nli {\ninner b\n}\n}\n}\nli {\ntwo\n}\n}\n";
        let out = render(&parse(src), &repo_defs());
        assert_eq!(out, "- one\n\n   1. inner a\n   1. inner b\n- two\n");
    }

    #[test]
    fn table_puts_delimiter_row_after_header() {
        let src = "table(lr) {\ntr {\nth {\nName\n}\nth {\nScore\n}\n}\ntr {\ntd {\nAlice\n}\ntd {\n90\n}\n}\n}\n";
        let out = render(&parse(src), &repo_defs());
        assert_eq!(out, "| Name | Score |\n| --- | --: |\n| Alice | 90 |\n");
    }

    #[test]
    fn code_fence_outgrows_backticks_in_source() {
        let src = "code(md) {\n````\n}\n";
        let out = render(&parse(src), &repo_defs());
        assert_eq!(out, "`````md\n````\n`````\n");
    }

    #[test]
    fn toc_lists_linked_entries() {
        let src = "toc {}\n\nsection(A) #a {\nsubsection(B) {\n}\n}\n";
        let out = render(&parse(src), &repo_defs());
        assert!(out.starts_with("**Table of Contents**\n\n- [1 A](#a)\n  - 1.1 B\n\n"));
    }

    #[test]
    fn md_rule_accepts_only_column_specs() {
        assert_eq!(md_rule(Some("|l|c|r|")), "| --- | :-: | --: |");
        assert_eq!(md_rule(Some("p{3cm}")), "");
        assert_eq!(md_rule(None), "");
    }

    #[test]
    fn code_span_handles_backticks() {
        assert_eq!(code_span("a"), "`a`");
        assert_eq!(code_span("a`b"), "``a`b``");
        assert_eq!(code_span("`x"), "`` `x ``");
    }

    #[test]
    fn indent_after_first_line() {
        assert_eq!(indent_after_first("a\n\nb", "   "), "a\n\n   b");
        assert_eq!(indent_after_first("a\n\nb", "> "), "a\n>\n> b");
    }

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_719), (2026, 9, 23));
    }
}
