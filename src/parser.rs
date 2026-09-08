/*
 * Manages the parsing of the note source into an AST. The parser is a simple
 * line-by-line parser that recognizes blocks, code blocks, math blocks,
 * and inline math, code, and references.
 *
 * Notes:
 * - The parser does not perform any reflowing of paragraphs, so each non-blank
 * line is its own paragraph.
 * - A line ending in { opens a block. A line that only contains } closes a
 * block. Any other { or } is treated as literal text.
 * - label(...) {} is treated as a block with an empty body. Note that this
 * causes no conflicts.
 */

use crate::ast::{Document, Inline, Node};

pub fn parse(source: &str) -> Document {
    let lines: Vec<&str> = source.lines().collect();
    let mut pos = 0;
    let nodes = parse_nodes(&lines, &mut pos, false);
    Document { nodes }
}

/*
 * Parses a slice of lines into a vector of nodes. The `pos` parameter is a
 * mutable reference to the current position in the lines slice. The `in_block`
 * parameter indicates whether the parser is currently inside a block. If it is,
 * the parser will stop parsing when it encounters a line that only contains a
 * closing brace (}). If it is not, the parser will continue parsing until it
 * reaches the end of the lines slice.
 *
 * @param lines: the slice of lines to parse
 * @param pos: a mutable reference to the current position in the lines slice
 * @param in_block: whether the parser is currently inside a block
 *
 * @return: a vector of nodes parsed from the lines slice
 */
fn parse_nodes(lines: &[&str], pos: &mut usize, in_block: bool) -> Vec<Node> {
    let mut nodes = Vec::new();

    while *pos < lines.len() {
        let line = lines[*pos];
        let trimmed = line.trim();

        if in_block && trimmed == "}" {
            *pos += 1;
            return nodes;
        }

        if trimmed.is_empty() {
            *pos += 1;
            continue;
        }

        if trimmed == "$$$" {
            *pos += 1;
            let mut math_lines: Vec<&str> = Vec::new();
            while *pos < lines.len() && lines[*pos].trim() != "$$$" {
                math_lines.push(lines[*pos]);
                *pos += 1;
            }
            if *pos < lines.len() {
                *pos += 1; // consume closing fence
            }
            nodes.push(Node::Paragraph(vec![Inline::MathDisplay(dedent(
                &math_lines,
            ))]));
            continue;
        }

        if let Some(lang) = trimmed.strip_prefix("```") {
            *pos += 1;
            let lang = if lang.is_empty() {
                None
            } else {
                Some(lang.trim().to_string())
            };
            let mut code_lines: Vec<&str> = Vec::new();
            while *pos < lines.len() && lines[*pos].trim() != "```" {
                code_lines.push(lines[*pos]);
                *pos += 1;
            }
            if *pos < lines.len() {
                *pos += 1;
            }
            nodes.push(Node::Code {
                lang,
                source: dedent(&code_lines),
            });
            continue;
        }

        // `label(...) {}` case
        if let Some(header) = line.trim_end().strip_suffix("{}") {
            let (label, args, id) = parse_header(header.trim());
            *pos += 1;
            match label.as_str() {
                "mlmath" => nodes.push(Node::Paragraph(vec![Inline::MathDisplay(String::new())])),
                "code" => nodes.push(Node::Code {
                    lang: args.into_iter().next(),
                    source: String::new(),
                }),
                _ => nodes.push(Node::Block {
                    label,
                    args,
                    id,
                    children: Vec::new(),
                }),
            }
            continue;
        }

        if let Some(header) = line.trim_end().strip_suffix('{') {
            let (label, args, id) = parse_header(header.trim());
            *pos += 1;

            // `mlmath { ... }` aliases `$$$` and `code(...) { ... }` aliases
            // ```...```
            match label.as_str() {
                "mlmath" => {
                    let mut math_lines: Vec<&str> = Vec::new();
                    while *pos < lines.len() && lines[*pos].trim() != "}" {
                        math_lines.push(lines[*pos]);
                        *pos += 1;
                    }
                    if *pos < lines.len() {
                        *pos += 1;
                    }
                    nodes.push(Node::Paragraph(vec![Inline::MathDisplay(dedent(
                        &math_lines,
                    ))]));
                }
                "code" => {
                    let mut code_lines: Vec<&str> = Vec::new();
                    while *pos < lines.len() && lines[*pos].trim() != "}" {
                        code_lines.push(lines[*pos]);
                        *pos += 1;
                    }
                    if *pos < lines.len() {
                        *pos += 1;
                    }
                    nodes.push(Node::Code {
                        lang: args.into_iter().next(),
                        source: dedent(&code_lines),
                    });
                }
                _ => {
                    let children = parse_nodes(lines, pos, true);
                    nodes.push(Node::Block {
                        label,
                        args,
                        id,
                        children,
                    });
                }
            }
            continue;
        }

        nodes.push(Node::Paragraph(parse_inline(trimmed)));
        *pos += 1;
    }

    nodes
}

/*
 * Strips the common leading whitespace on every non-blank line in a slice
 * of lines. This is used for code and math blocks, which are often indented
 * to match the surrounding text, but should not carry that indentation into
 * the block content. Note that relative indentation is preserved.
 *
 * @param lines: the slice of lines to dedent
 */
fn dedent(lines: &[&str]) -> String {
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                ""
            } else {
                l.get(min_indent..).unwrap_or(l)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parses `label(arg1, arg2, ...) #id` (args and id both optional) into its parts.

/*
 * Parses a header string of the form `label(arg1, arg2, ...) #id` into its
 * constituent parts: the label, an optional vector of arguments, and an
 * optional ID.
 *
 * @param s: the header string to parse
 *
 * @return: a tuple containing the label, a vector of arguments, and an
 * optional ID
 */
fn parse_header(s: &str) -> (String, Vec<String>, Option<String>) {
    let mut s = s.trim();
    let mut id = None;

    if let Some(hash_pos) = s.rfind('#') {
        let before = &s[..hash_pos];
        if before.is_empty() || before.ends_with(char::is_whitespace) {
            id = Some(s[hash_pos + 1..].trim().to_string());
            s = before.trim();
        }
    }

    if let (Some(open), Some(close)) = (s.find('('), s.rfind(')')) {
        if open < close {
            let label = s[..open].trim().to_string();
            let args_str = s[open + 1..close].trim();
            let args = if args_str.is_empty() {
                Vec::new()
            } else {
                args_str.split(',').map(|a| a.trim().to_string()).collect()
            };
            return (label, args, id);
        }
    }

    (s.to_string(), Vec::new(), id)
}

/*
 * Parses inline elements in a string, including text, inline math, code,
 * and references.
 *
 * @param text: the string to parse
 *
 * @return: a vector of Inline elements parsed from the string
 */
pub fn parse_inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut result = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            let start = i + 1;
            if let Some(end) = find_char(&chars, start, '$') {
                if !buf.is_empty() {
                    result.push(Inline::Text(std::mem::take(&mut buf)));
                }
                let content: String = chars[start..end].iter().collect();
                result.push(Inline::MathInline(content));
                i = end + 1;
                continue;
            }
        } else if chars[i] == '`' {
            let start = i + 1;
            if let Some(end) = find_char(&chars, start, '`') {
                if !buf.is_empty() {
                    result.push(Inline::Text(std::mem::take(&mut buf)));
                }
                let content: String = chars[start..end].iter().collect();
                result.push(Inline::Code(content));
                i = end + 1;
                continue;
            }
        } else if chars[i] == '@'
            && chars
                .get(i + 1)
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            let start = i + 1;
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '-')
            {
                end += 1;
            }
            if !buf.is_empty() {
                result.push(Inline::Text(std::mem::take(&mut buf)));
            }
            result.push(Inline::Ref(chars[start..end].iter().collect()));
            i = end;
            continue;
        }
        buf.push(chars[i]);
        i += 1;
    }

    if !buf.is_empty() {
        result.push(Inline::Text(buf));
    }
    result
}

fn find_char(chars: &[char], start: usize, target: char) -> Option<usize> {
    chars[start..]
        .iter()
        .position(|c| *c == target)
        .map(|i| start + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_block() {
        let doc = parse("theorem(Cauchy-Schwarz) #cs {\nSome text $x$.\n}\n");
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Block {
                label,
                args,
                id,
                children,
            } => {
                assert_eq!(label, "theorem");
                assert_eq!(args, &vec!["Cauchy-Schwarz".to_string()]);
                assert_eq!(id.as_deref(), Some("cs"));
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    #[test]
    fn one_liner_empty_block() {
        let doc = parse("title(2026-09-07 Notes) {}\n");
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Block {
                label,
                args,
                id,
                children,
            } => {
                assert_eq!(label, "title");
                assert_eq!(args, &vec!["2026-09-07 Notes".to_string()]);
                assert_eq!(id, &None);
                assert!(children.is_empty());
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    #[test]
    fn bare_label_and_empty_parens_are_equivalent() {
        assert_eq!(parse("toc {}\n"), parse("toc() {}\n"));
        assert_eq!(parse("proof {}\n"), parse("proof() {}\n"));
    }

    #[test]
    fn one_liner_code_and_mlmath_are_empty() {
        let doc = parse("code(python) {}\n\nmlmath {}\n");
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Code {
                lang: Some("python".to_string()),
                source: String::new(),
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![Inline::MathDisplay(String::new())])
        );
    }

    #[test]
    fn parses_multiple_args() {
        let doc = parse("section(Intro, short) {\ntext\n}\n");
        match &doc.nodes[0] {
            Node::Block { args, .. } => {
                assert_eq!(args, &vec!["Intro".to_string(), "short".to_string()]);
            }
            other => panic!("expected block, got {other:?}"),
        }
    }

    #[test]
    fn consecutive_lines_are_separate_paragraphs() {
        let doc = parse("proof {\nline one\nline two\nline three\n}\n");
        let Node::Block { children, .. } = &doc.nodes[0] else {
            panic!("expected block")
        };
        assert_eq!(children.len(), 3);
        assert_eq!(
            children[0],
            Node::Paragraph(vec![Inline::Text("line one".to_string())])
        );
        assert_eq!(
            children[1],
            Node::Paragraph(vec![Inline::Text("line two".to_string())])
        );
        assert_eq!(
            children[2],
            Node::Paragraph(vec![Inline::Text("line three".to_string())])
        );
    }

    #[test]
    fn brace_in_math_is_literal() {
        let doc = parse("proof {\nNote that $\\{1,2,3\\}$ is a set.\n}\n");
        let Node::Block { children, .. } = &doc.nodes[0] else {
            panic!("expected block")
        };
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn brace_inside_code_fence_is_ignored() {
        let src = "proof {\n```python\ndef f():\n    return {1}\n```\n}\n";
        let doc = parse(src);
        let Node::Block { children, .. } = &doc.nodes[0] else {
            panic!("expected block")
        };
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0], Node::Code { .. }));
    }

    #[test]
    fn inline_math_and_refs() {
        let inlines = parse_inline("See @cs: $a+b$.");
        assert_eq!(
            inlines,
            vec![
                Inline::Text("See ".to_string()),
                Inline::Ref("cs".to_string()),
                Inline::Text(": ".to_string()),
                Inline::MathInline("a+b".to_string()),
                Inline::Text(".".to_string()),
            ]
        );
    }

    #[test]
    fn display_math_fence() {
        let doc = parse("$$$\nc = d\n$$$\n");
        assert_eq!(
            doc.nodes,
            vec![Node::Paragraph(vec![Inline::MathDisplay(
                "c = d".to_string()
            )])]
        );
    }

    #[test]
    fn display_math_fence_inside_block() {
        let doc = parse("theorem {\n$$$\na = b\n$$$\n}\n");
        let Node::Block { children, .. } = &doc.nodes[0] else {
            panic!("expected block")
        };
        assert_eq!(
            children,
            &vec![Node::Paragraph(vec![Inline::MathDisplay(
                "a = b".to_string()
            )])]
        );
    }
}
