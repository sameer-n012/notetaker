use anyhow::{bail, Context, Result};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::ast::{Document, Node};
use crate::defs::{self, LabelMap};
use crate::{parser, render_html, render_latex};

// Templates are included in the binary
const PREAMBLE_TEX: &str = include_str!("../templates/preamble.tex");
const STYLE_CSS: &str = include_str!("../templates/style.css");

pub struct Outputs {
    pub html: bool,
    pub latex: bool,
    pub pdf: bool,
}

/*
 * Resolves `path` to the list of `.note` files to render. This is either
 * itself, if it is a file, or every `.note` file directly inside it
 * (non-recursively), if it is a directory.
 *
 * @param path The path to a single note file, or a directory of them.
 *
 * @returns A list of note file paths to render.
 */
fn collect_note_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_dir() {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(path)
            .with_context(|| format!("reading notes dir {}", path.display()))?
        {
            let entry_path = entry?.path();
            if entry_path.extension().and_then(|e| e.to_str()) == Some("note") {
                files.push(entry_path);
            }
        }
        Ok(files)
    } else {
        Ok(vec![path.to_path_buf()])
    }
}

pub fn build_all(path: &Path, out_dir: &Path, defs: &LabelMap, outputs: &Outputs) -> Result<()> {
    let css = if outputs.html {
        Some(build_css(defs))
    } else {
        None
    };

    for note_path in collect_note_files(path)? {
        build_one(&note_path, out_dir, defs, outputs, css.as_deref())?;
    }
    Ok(())
}

/*
 * Watches the notes path and the definitions directory for changes.
 * Any change in either triggers a rebuild of all notes.
 *
 * @param path The path to a single note file, or a directory of them.
 * @param out_dir The output directory for rendered files.
 * @param labels_json_path The path to the labels.json file, if any.
 * @param outputs The outputs to generate (html, latex, pdf).
 *
 * @returns A Result indicating success or failure.
 */
pub fn watch(
    path: &Path,
    out_dir: &Path,
    labels_json_path: Option<&Path>,
    outputs: &Outputs,
) -> Result<()> {
    let mut label_defs = match labels_json_path {
        Some(p) => defs::load_all(p)?,
        None => LabelMap::new(),
    };
    build_all(path, out_dir, &label_defs, outputs)?;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(300), tx)?;
    debouncer
        .watcher()
        .watch(path, notify::RecursiveMode::Recursive)?;

    let labels_root = labels_json_path
        .and_then(|p| p.parent())
        .filter(|p| !p.as_os_str().is_empty());
    if let Some(root) = labels_root {
        if root != path {
            debouncer
                .watcher()
                .watch(root, notify::RecursiveMode::Recursive)?;
        }
    }

    println!(
        "watching {} and defs for changes (ctrl-c to stop)...",
        path.display()
    );
    for res in rx {
        let events = match res {
            Ok(events) => events,
            Err(e) => {
                eprintln!("watch error: {e:?}");
                continue;
            }
        };

        if !events
            .iter()
            .any(|e| e.kind == DebouncedEventKind::Any && is_source_file(&e.path))
        {
            continue;
        }

        if let Some(p) = labels_json_path {
            match defs::load_all(p) {
                Ok(fresh) => label_defs = fresh,
                Err(e) => {
                    eprintln!("error reloading label defs: {e}");
                    continue;
                }
            }
        }
        match build_all(path, out_dir, &label_defs, outputs) {
            Ok(()) => println!("rebuilt all notes"),
            Err(e) => eprintln!("error rebuilding notes: {e}"),
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("note") | Some("def") => true,
        _ => path.file_name().and_then(|f| f.to_str()) == Some("_labels.json"),
    }
}

fn build_one(
    path: &Path,
    out_dir: &Path,
    defs: &LabelMap,
    outputs: &Outputs,
    css: Option<&str>,
) -> Result<()> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed = parser::parse(&source);
    let (title, doc) = extract_title(parsed);
    let stem = path
        .file_stem()
        .context("note file has no stem")?
        .to_string_lossy()
        .into_owned();

    if outputs.latex || outputs.pdf {
        let tex_path = write_tex(&doc, &stem, out_dir, defs, title.as_deref())?;
        if outputs.pdf {
            compile_pdf(&tex_path, out_dir, &stem)?;
        }
    }
    if outputs.html {
        write_html(
            &doc,
            &stem,
            out_dir,
            defs,
            title.as_deref(),
            css.unwrap_or_default(),
        )?;
    }
    Ok(())
}

/// Pulls a top-level `title(...) { }` block's first arg out as the document
/// title, dropping it from the body so it isn't also rendered as content.

/*
 * Gets the title of the document from a top-level `title(...) { }` block,
 * if it exists.
 *
 * @param doc The parsed document to extract the title from.
 *
 * @returns A tuple containing the optional title and the document with
 * the title block removed.
 */
fn extract_title(doc: Document) -> (Option<String>, Document) {
    let mut title = None;
    let mut nodes = Vec::with_capacity(doc.nodes.len());
    for node in doc.nodes {
        if let Node::Block { label, args, .. } = &node {
            if label == "title" {
                if title.is_none() {
                    title = args.first().cloned();
                }
                continue;
            }
        }
        nodes.push(node);
    }
    (title, Document { nodes })
}

fn write_tex(
    doc: &Document,
    stem: &str,
    out_dir: &Path,
    defs: &LabelMap,
    title: Option<&str>,
) -> Result<PathBuf> {
    let body = render_latex::render(doc, defs);
    let full = PREAMBLE_TEX.replace("% NOTETAKER:CONTENT", &body).replacen(
        "\\title{}",
        &format!("\\title{{{}}}", title.unwrap_or(stem)),
        1,
    );

    let path: PathBuf = out_dir.join(format!("{stem}.tex"));
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(&path, full)?;
    Ok(path)
}

/*
 * Compiles a .tex file to PDF using latexmk.
 */
fn compile_pdf(tex_path: &Path, out_dir: &Path, stem: &str) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;

    let status = Command::new("latexmk")
        .arg("-pdf")
        .arg("-interaction=nonstopmode")
        .arg("-halt-on-error")
        .arg(format!("-outdir={}", out_dir.display()))
        .arg(tex_path)
        .status()
        .context("Running latexmk")?;

    if !status.success() {
        bail!(
            "latexmk failed for {stem}. See {} for details",
            out_dir.join(format!("{stem}.log")).display()
        );
    }
    Ok(())
}

fn write_html(
    doc: &Document,
    stem: &str,
    out_dir: &Path,
    defs: &LabelMap,
    title: Option<&str>,
    css: &str,
) -> Result<()> {
    let body = render_html::render(doc, defs);
    let page_title = title.unwrap_or(stem);
    let heading = title
        .map(|t| format!("<h1>{}</h1>\n", render_html::escape(t)))
        .unwrap_or_default();

    let page = format!(
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>{page_title}</title>
<style>
{css}
</style>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css">
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js"></script>
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/contrib/auto-render.min.js"
  onload="renderMathInElement(document.body, {{delimiters: [
    {{left: '\\[', right: '\\]', display: true}},
    {{left: '\\(', right: '\\)', display: false}}
  ]}});"></script>
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github.min.css">
<script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
<script>document.addEventListener('DOMContentLoaded', () => hljs.highlightAll());</script>
</head>
<body>
{heading}{body}
</body>
</html>
"#
    );

    let path: PathBuf = out_dir.join(format!("{stem}.html"));
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(&path, page)?;
    Ok(())
}

/*
 * Concatenates the base page stylesheet with every label's `style { ... }`
 * block, so each note's HTML page can inline all styling in one `<style>` tag.
 * No relative `<link>` is needed.
 *
 * @param defs The label definitions to extract styles from.
 *
 * @returns A string containing the concatenated CSS.
 */
fn build_css(defs: &LabelMap) -> String {
    let mut labels: Vec<&String> = defs.keys().collect();
    labels.sort();

    let mut css = STYLE_CSS.to_string();
    for label in labels {
        if let Some(style) = &defs[label].style {
            css.push('\n');
            css.push_str(style);
        }
    }
    css
}
