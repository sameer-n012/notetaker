/*
 * Manages the label definitions and template substitution for blocks.
 *
 * A .def file defines how a block with a given label is rendered in LaTeX,
 * HTML and Markdown, and whether it is numbered. The `_labels.json` file maps
 * labels to their corresponding .def files. A .def file can have 5 sections,
 * plus comment lines: any top-level line starting with `#` is ignored.
 * Comments are not recognized inside `latex`/`html`/`markdown`/`style` bodies,
 * which are raw template text.
 * - `property: value` lines: `numbered: true|false` and `toc: true|false`
 *   (whether this label's numbered blocks also get a table-of-contents entry),
 *   plus the optional string properties `latex_join: "sep"` / `html_join:
 *   "sep"` / `markdown_join: "sep"`, which render that output's `$body` as
 *   this block's children joined by `sep` (each trimmed of surrounding
 *   whitespace), and `markdown_indent: "prefix"`, which puts `prefix` before
 *   every line of the Markdown `$body` except the first (for list items and
 *   other line-based nesting).
 * - `latex { ... }` section for the LaTeX template
 * - `html { ... }` section for the HTML template
 * - `markdown { ... }` section for the Markdown template
 * - `style { ... }` section for the CSS style
 * Each renderer treats a missing template section the same as a label with no
 * def at all, and uses its generic fallback for that output.
 * See `defs/` for examples of `.def` files and `_labels.json` for the mapping.
 */

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LabelDef {
    pub numbered: bool,
    pub toc: bool,
    pub latex_join: Option<String>,
    pub html_join: Option<String>,
    pub markdown_join: Option<String>,
    pub markdown_indent: Option<String>,
    pub latex_template: Option<String>,
    pub html_template: Option<String>,
    pub markdown_template: Option<String>,
    pub style: Option<String>,
}

pub type LabelMap = HashMap<String, LabelDef>;

/*
 * Default values for each property in a .def file. If a property is not
 * in the mapping, it is rejected.
 *
 * @return A HashMap of property names to their default values.
 */
fn default_properties() -> HashMap<&'static str, bool> {
    HashMap::from([("numbered", false), ("toc", false)])
}

/*
 * Loads `_labels.json` file and parses every referenced `.def` file.
 *
 * @param labels_json_path The path to the `_labels.json` file.
 *
 * @return A LabelMap mapping labels to their definitions.
 */
pub fn load_all(labels_json_path: &Path) -> Result<LabelMap> {
    let text = std::fs::read_to_string(labels_json_path)
        .with_context(|| format!("reading {}", labels_json_path.display()))?;
    let paths: HashMap<String, String> = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", labels_json_path.display()))?;

    let base = labels_json_path.parent().unwrap_or_else(|| Path::new("."));
    let mut defs = HashMap::new();
    for (label, rel_path) in paths {
        let def_path = base.join(&rel_path);
        let source = std::fs::read_to_string(&def_path)
            .with_context(|| format!("reading {}", def_path.display()))?;
        let def = parse(&source).with_context(|| format!("parsing {}", def_path.display()))?;
        defs.insert(label, def);
    }
    Ok(defs)
}

/*
 * A .def file has property lines and optional `latex { ... }` /
 * `html { ... }` / `markdown { ... }` / `style { ... }` sections.
 *
 * @param source The contents of a .def file.
 *
 * @return A LabelDef struct with the parsed properties and templates.
 */
fn parse(source: &str) -> Result<LabelDef> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    let mut properties = default_properties();
    let mut latex_join = None;
    let mut html_join = None;
    let mut markdown_join = None;
    let mut markdown_indent = None;
    let mut latex_template = None;
    let mut html_template = None;
    let mut markdown_template = None;
    let mut style = None;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        if let Some(name) = trimmed.strip_suffix('{') {
            let name = name.trim().to_string();
            i += 1;
            let mut body_lines = Vec::new();
            while i < lines.len() && lines[i].trim() != "}" {
                body_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            let content = body_lines.join("\n");
            match name.as_str() {
                "latex" => latex_template = Some(content),
                "html" => html_template = Some(content),
                "markdown" => markdown_template = Some(content),
                "style" => style = Some(content),
                other => bail!("Unknown section '{other}' in def file"),
            }
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "latex_join" => latex_join = Some(value.trim_matches('"').to_string()),
                "html_join" => html_join = Some(value.trim_matches('"').to_string()),
                "markdown_join" => markdown_join = Some(unquote_escaped(value)),
                "markdown_indent" => markdown_indent = Some(unquote_escaped(value)),
                _ if properties.contains_key(key) => {
                    properties.insert(key, value == "true");
                }
                _ => bail!("Unknown key '{key}' in def file"),
            }
            i += 1;
            continue;
        }

        bail!("Unexpected line in def file: {trimmed}");
    }

    Ok(LabelDef {
        numbered: properties["numbered"],
        toc: properties["toc"],
        latex_join,
        html_join,
        markdown_join,
        markdown_indent,
        latex_template,
        html_template,
        markdown_template,
        style,
    })
}

/*
 * Strips the surrounding `"` quotes from a string property value and expands
 * `\n` to a newline.
 *
 * @param value The raw property value.
 *
 * @return The unquoted value with `\n` expanded.
 */
fn unquote_escaped(value: &str) -> String {
    value.trim_matches('"').replace("\\n", "\n")
}

/*
 * Substitutes `$1`, `$2`, ... with positional args (from `label(arg1, arg2)`),
 * `$body` with the block's rendered children, `$id` with its id (if any),
 * `$id_attr` with ` id="..."` (or nothing, if there's no id), and `$n` with
 * its number, e.g. `"2.2.1"` (if the label is numbered). `${N?text}` or
 * `${id?text}` renders `text` (itself substituted) only if arg N (or the id)
 * was given, otherwise nothing. `extra` supplies any additional named values a
 * caller wants to expose as `$name` (e.g. a precomputed, renderer-specific
 * helper string). Any other `$word` is left as-is, so stray `$` signs
 * elsewhere in a template stay literal.
 *
 * @param template The template string to substitute into.
 * @param args The positional arguments to substitute for `$1`, `$2`, etc.
 * @param body The block's rendered children to substitute for `$body`.
 * @param id The block's id to substitute for `$id` and `$id_attr`.
 * @param n The block's number to substitute for `$n`.
 * @param extra Any additional named values to substitute for `$name`.
 *
 * @return The template string with all substitutions made.
 */
pub fn substitute(
    template: &str,
    args: &[String],
    body: &str,
    id: Option<&str>,
    n: Option<&str>,
    extra: &[(&str, &str)],
) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            if chars.get(i + 1) == Some(&'{') {
                let cond_start = i + 2;
                let mut cond_end = cond_start;
                while cond_end < chars.len() && chars[cond_end].is_ascii_digit() {
                    cond_end += 1;
                }
                let is_digit_cond = cond_end > cond_start;
                if !is_digit_cond {
                    if let Some(['i', 'd']) = chars.get(cond_start..cond_start + 2) {
                        cond_end = cond_start + 2;
                    }
                }
                if cond_end > cond_start && chars.get(cond_end) == Some(&'?') {
                    let content_start = cond_end + 1;
                    if let Some(close) = find_matching_brace(&chars, content_start) {
                        let present = if is_digit_cond {
                            let idx: usize = chars[cond_start..cond_end]
                                .iter()
                                .collect::<String>()
                                .parse()
                                .unwrap();
                            idx.checked_sub(1).and_then(|i| args.get(i)).is_some()
                        } else {
                            id.is_some()
                        };
                        if present {
                            let inner: String = chars[content_start..close].iter().collect();
                            out.push_str(&substitute(&inner, args, body, id, n, extra));
                        }
                        i = close + 1;
                        continue;
                    }
                }
            }
            if let Some(&next) = chars.get(i + 1) {
                if next.is_ascii_digit() {
                    let start = i + 1;
                    let mut end = start;
                    while end < chars.len() && chars[end].is_ascii_digit() {
                        end += 1;
                    }
                    let digits: String = chars[start..end].iter().collect();
                    let idx: usize = digits.parse().unwrap();
                    match idx.checked_sub(1).and_then(|i| args.get(i)) {
                        Some(arg) => out.push_str(arg),
                        None => {
                            // argument number out of range, leave as literal
                            out.push('$');
                            out.push_str(&digits);
                        }
                    }
                    i = end;
                    continue;
                }
                if next.is_alphabetic() || next == '_' {
                    let start = i + 1;
                    let mut end = start;
                    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                        end += 1;
                    }
                    let name: String = chars[start..end].iter().collect();
                    match name.as_str() {
                        "body" => out.push_str(body),
                        "id" => {
                            if let Some(id) = id {
                                out.push_str(id);
                            }
                        }
                        "id_attr" => {
                            if let Some(id) = id {
                                out.push_str(&format!(" id=\"{id}\""));
                            }
                        }
                        "n" => {
                            if let Some(n) = n {
                                out.push_str(n);
                            }
                        }
                        other => match extra.iter().find(|(k, _)| *k == other) {
                            Some((_, v)) => out.push_str(v),
                            None => {
                                out.push('$');
                                out.push_str(&name);
                            }
                        },
                    }
                    i = end;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

/*
 * Finds the `}` that closes a `${...}` conditional group's own already-open
 * brace, accounting for any nested `{`/`}` pairs inside its content.
 *
 * @param chars The characters of the template string.
 * @param start The index of the first character after the opening `{`.
 *
 * @return The index of the matching `}`, or None if not found.
 */
fn find_matching_brace(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 1;
    for (i, &c) in chars.iter().enumerate().skip(start) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_def_file() {
        let src = "numbered: true\n\nlatex {\n\\begin{theorem}[$1]\n$body\n\\end{theorem}\n}\n\nhtml {\n<div>$n $1</div>\n}\n";
        let def = parse(src).unwrap();
        assert!(def.numbered);
        assert!(!def.toc);
        assert!(def.latex_template.unwrap().contains("$body"));
        assert!(def.html_template.unwrap().contains("$n"));
        assert!(def.markdown_template.is_none());
        assert!(def.style.is_none());
    }

    #[test]
    fn parses_markdown_section_and_properties() {
        let src = "markdown_join: \"\\n- \"\nmarkdown_indent: \"   \"\n\nmarkdown {\n- $body\n}\n";
        let def = parse(src).unwrap();
        assert_eq!(def.markdown_template.as_deref(), Some("- $body"));
        assert_eq!(def.markdown_join.as_deref(), Some("\n- "));
        assert_eq!(def.markdown_indent.as_deref(), Some("   "));
    }

    #[test]
    fn all_template_sections_are_optional() {
        let def = parse("numbered: true\n").unwrap();
        assert!(def.numbered);
        assert!(def.latex_template.is_none());
        assert!(def.html_template.is_none());
        assert!(def.markdown_template.is_none());
    }

    #[test]
    fn latex_join_backslashes_stay_literal() {
        let def = parse("latex_join: \" \\n \"\n").unwrap();
        assert_eq!(def.latex_join.as_deref(), Some(" \\n "));
    }

    #[test]
    fn parses_join_properties() {
        let src =
            "latex_join: \" & \"\nhtml_join: \", \"\n\nlatex {\n$body\n}\n\nhtml {\n$body\n}\n";
        let def = parse(src).unwrap();
        assert_eq!(def.latex_join.as_deref(), Some(" & "));
        assert_eq!(def.html_join.as_deref(), Some(", "));
    }

    #[test]
    fn omitted_properties_use_their_defaults() {
        let src = "latex {\n$body\n}\n\nhtml {\n$body\n}\n";
        let def = parse(src).unwrap();
        assert!(!def.numbered);
        assert!(!def.toc);
        assert!(def.latex_join.is_none());
        assert!(def.html_join.is_none());
        assert!(def.markdown_join.is_none());
        assert!(def.markdown_indent.is_none());
    }

    #[test]
    fn unknown_property_key_is_rejected() {
        let src = "foo: true\n\nlatex {\n$body\n}\n\nhtml {\n$body\n}\n";
        assert!(parse(src).is_err());
    }

    #[test]
    fn top_level_comment_lines_are_ignored() {
        let src = "# Theorem-like block.\n\
                   # numbered: false   <- not a real property line\n\
                   # latex {\n\
                   numbered: true\n\
                   \x20   # indented comment\n\
                   \n\
                   latex {\n$body\n}\n\
                   # between sections\n\
                   html {\n$body\n}\n";
        let def = parse(src).unwrap();
        assert!(def.numbered);
        assert_eq!(def.latex_template.as_deref(), Some("$body"));
        assert_eq!(def.html_template.as_deref(), Some("$body"));
    }

    #[test]
    fn hash_inside_section_bodies_is_preserved() {
        let src =
            "latex {\n# not a comment\n}\n\nhtml {\n$body\n}\n\nstyle {\n#id { color: red; }\n}\n";
        let def = parse(src).unwrap();
        assert_eq!(def.latex_template.as_deref(), Some("# not a comment"));
        assert_eq!(def.style.as_deref(), Some("#id { color: red; }"));
    }

    #[test]
    fn substitutes_args_body_id_n() {
        let out = substitute(
            "[$1|$2] $body #$id no$3 $n",
            &["a".to_string(), "b".to_string()],
            "BODY",
            Some("xyz"),
            Some("4"),
            &[],
        );
        assert_eq!(out, "[a|b] BODY #xyz no$3 4");
    }

    #[test]
    fn conditional_group_present_and_absent() {
        let with_arg = substitute(
            "Theorem${1? ($1)}.",
            &["Foo".to_string()],
            "",
            None,
            None,
            &[],
        );
        assert_eq!(with_arg, "Theorem (Foo).");

        let without_arg = substitute("Theorem${1? ($1)}.", &[], "", None, None, &[]);
        assert_eq!(without_arg, "Theorem.");
    }

    #[test]
    fn conditional_group_id_with_nested_braces() {
        let with_id = substitute("X${id?\\label{$id}}Y", &[], "", Some("cs"), None, &[]);
        assert_eq!(with_id, "X\\label{cs}Y");

        let without_id = substitute("X${id?\\label{$id}}Y", &[], "", None, None, &[]);
        assert_eq!(without_id, "XY");
    }

    #[test]
    fn unknown_placeholder_left_literal() {
        let out = substitute("cost: $5 today", &[], "", None, None, &[]);
        assert_eq!(out, "cost: $5 today");
    }

    #[test]
    fn extra_substitutions_resolve_and_dont_shadow_builtins() {
        let out = substitute(
            "<pre$class_attr>$body</pre>",
            &[],
            "code here",
            None,
            None,
            &[("class_attr", " class=\"language-rust\"")],
        );
        assert_eq!(out, "<pre class=\"language-rust\">code here</pre>");

        let shadow_attempt = substitute("$body", &[], "real body", None, None, &[("body", "fake")]);
        assert_eq!(shadow_attempt, "real body");
    }
}
