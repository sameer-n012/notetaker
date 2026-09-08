use crate::ast::{Document, Inline, Node};

/// Structural rule: a line ending in `{` (after trimming trailing whitespace)
/// opens a block; a line whose trimmed content is exactly `}` closes one.
/// Any other `{`/`}` (inside math, code, or prose) is just literal text.
pub fn parse(source: &str) -> Document {
    let lines: Vec<&str> = source.lines().collect();
    let mut pos = 0;
    let nodes = parse_nodes(&lines, &mut pos, false);
    Document { nodes }
}

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
            nodes.push(Node::Paragraph(vec![Inline::MathDisplay(dedent(&math_lines))]));
            continue;
        }

        if let Some(lang) = trimmed.strip_prefix("```") {
            *pos += 1;
            let lang = if lang.is_empty() { None } else { Some(lang.trim().to_string()) };
            let mut code_lines: Vec<&str> = Vec::new();
            while *pos < lines.len() && lines[*pos].trim() != "```" {
                code_lines.push(lines[*pos]);
                *pos += 1;
            }
            if *pos < lines.len() {
                *pos += 1; // consume closing fence
            }
            nodes.push(Node::Code {
                lang,
                source: dedent(&code_lines),
            });
            continue;
        }

        // `label(...) {}` with an empty body, entirely on one line — safe to
        // support unlike general one-line nesting, since there's no content
        // between the braces to create the ambiguity that rule avoids.
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

            // `mlmath { ... }` aliases the `$$$` display-math fence, and
            // `code(lang) { ... }` aliases the ``` fence: both take their
            // content raw (no nested block/paragraph parsing).
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
                    nodes.push(Node::Paragraph(vec![Inline::MathDisplay(dedent(&math_lines))]));
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

        // every non-blank line is its own paragraph (no reflow-joining)
        nodes.push(Node::Paragraph(parse_inline(trimmed)));
        *pos += 1;
    }

    nodes
}

/// Strips the common leading whitespace shared by every non-blank line, so a
/// fenced code/math block written indented (either matching its opening
/// marker's indent, or one level deeper, as people naturally write it)
/// doesn't carry that indentation into its content. Relative indentation
/// between lines (e.g. a nested `if` in Python) is preserved.
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
        } else if chars[i] == '@' && chars.get(i + 1).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_' || chars[end] == '-') {
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
    chars[start..].iter().position(|c| *c == target).map(|i| start + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_block() {
        let doc = parse("theorem(Cauchy-Schwarz) #cs {\nSome text $x$.\n}\n");
        assert_eq!(doc.nodes.len(), 1);
        match &doc.nodes[0] {
            Node::Block { label, args, id, children } => {
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
            Node::Block { label, args, id, children } => {
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
        assert_eq!(children[0], Node::Paragraph(vec![Inline::Text("line one".to_string())]));
        assert_eq!(children[1], Node::Paragraph(vec![Inline::Text("line two".to_string())]));
        assert_eq!(children[2], Node::Paragraph(vec![Inline::Text("line three".to_string())]));
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
            vec![Node::Paragraph(vec![Inline::MathDisplay("c = d".to_string())])]
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
            &vec![Node::Paragraph(vec![Inline::MathDisplay("a = b".to_string())])]
        );
    }
}
