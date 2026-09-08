use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// A rendering definition for one block label, loaded from a `.def` file.
#[derive(Debug, Clone)]
pub struct LabelDef {
    pub numbered: bool,
    pub latex_template: String,
    pub html_template: String,
    pub style: Option<String>,
}

pub type LabelMap = HashMap<String, LabelDef>;

/// Loads `labels.json` (label -> path to a `.def` file, resolved relative to
/// the json file's own directory) and parses every referenced `.def` file.
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

/// A `.def` file has a `numbered: true|false` line and `latex { ... }` /
/// `html { ... }` / `style { ... }` sections, using the same "`{` at
/// end-of-line, lone `}` closes it" rule as note files.
fn parse(source: &str) -> Result<LabelDef> {
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    let mut numbered = false;
    let mut latex_template = None;
    let mut html_template = None;
    let mut style = None;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.is_empty() {
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
                i += 1; // consume closing brace
            }
            let content = body_lines.join("\n");
            match name.as_str() {
                "latex" => latex_template = Some(content),
                "html" => html_template = Some(content),
                "style" => style = Some(content),
                other => bail!("unknown section '{other}' in def file"),
            }
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':') {
            match key.trim() {
                "numbered" => numbered = value.trim() == "true",
                other => bail!("unknown key '{other}' in def file"),
            }
            i += 1;
            continue;
        }

        bail!("unexpected line in def file: {trimmed}");
    }

    Ok(LabelDef {
        numbered,
        latex_template: latex_template.context("def file missing a 'latex {' section")?,
        html_template: html_template.context("def file missing an 'html {' section")?,
        style,
    })
}

/// Substitutes `$1`, `$2`, ... with positional args (from `label(arg1, arg2)`),
/// `$body` with the block's rendered children, `$id` with its id (if any),
/// `$id_attr` with ` id="..."` (or nothing, if there's no id — use this for
/// an HTML attribute so a missing id doesn't leave `id=""`), and `$n` with
/// its number, e.g. `"2.2.1"` (if the label is numbered). `${N?text}` or
/// `${id?text}` renders
/// `text` (itself substituted) only if arg N (or the id) was given, otherwise
/// nothing — use this to keep an optional title, or a `\label{}`, out of the
/// output entirely instead of leaving a literal `$1` behind (which, e.g.,
/// would break LaTeX by opening math mode) or an empty, duplicate `\label{}`.
/// `extra` supplies any additional named values a caller wants to expose as
/// `$name` (e.g. a precomputed, renderer-specific helper string) — checked
/// after the built-ins, so it can't shadow `body`/`id`/`id_attr`/`n`. Any
/// other `$word` is left as-is, so stray `$` signs elsewhere in a template
/// stay literal.
pub fn substitute(template: &str, args: &[String], body: &str, id: Option<&str>, n: Option<&str>, extra: &[(&str, &str)]) -> String {
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
                            let idx: usize = chars[cond_start..cond_end].iter().collect::<String>().parse().unwrap();
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
                            // out of range: not a real placeholder, keep literal (e.g. "$5" as currency)
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

/// Finds the `}` that closes a `${...}` conditional group's own already-open
/// brace, accounting for any nested `{`/`}` pairs inside its content (e.g.
/// `${id?\label{$id}}` has a `\label{...}` nested inside).
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
        assert!(def.latex_template.contains("$body"));
        assert!(def.html_template.contains("$n"));
        assert!(def.style.is_none());
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
        let with_arg = substitute("Theorem${1? ($1)}.", &["Foo".to_string()], "", None, None, &[]);
        assert_eq!(with_arg, "Theorem (Foo).");

        let without_arg = substitute("Theorem${1? ($1)}.", &[], "", None, None, &[]);
        assert_eq!(without_arg, "Theorem.");
    }

    #[test]
    fn conditional_group_id_with_nested_braces() {
        // `\label{$id}` nests its own `{`/`}` inside the conditional group,
        // so the group's closing `}` must be found by brace-depth matching,
        // not by scanning for the first `}`.
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

        // an extra can't override a built-in name
        let shadow_attempt = substitute("$body", &[], "real body", None, None, &[("body", "fake")]);
        assert_eq!(shadow_attempt, "real body");
    }
}
