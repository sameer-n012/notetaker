use anyhow::{bail, Context, Result};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::ast::{Document, Node};
use crate::defs::{self, LabelMap};
use crate::{parser, render_html, render_latex};

/// Which output formats to generate. `pdf` implies compiling the `.tex` that
/// `latex` would produce, even if `latex` itself wasn't requested — the
/// `.tex` file is written either way as PDF compile input.
pub struct Outputs {
    pub html: bool,
    pub latex: bool,
    pub pdf: bool,
}

pub fn build_all(notes_dir: &Path, out_dir: &Path, template_dir: &Path, defs: &LabelMap, outputs: &Outputs) -> Result<()> {
    let css = if outputs.html { Some(build_css(defs, template_dir)?) } else { None };

    for entry in std::fs::read_dir(notes_dir)
        .with_context(|| format!("reading notes dir {}", notes_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("note") {
            build_one(&path, out_dir, template_dir, defs, outputs, css.as_deref())?;
        }
    }
    Ok(())
}

/// Watches both the notes directory and the directory holding `labels.json`
/// (which also covers `defs/`, since it normally lives alongside it). Any
/// change reloads the label definitions fresh and rebuilds every note, since
/// a def-file edit can affect blocks in any note.
pub fn watch(notes_dir: &Path, out_dir: &Path, template_dir: &Path, labels_json_path: &Path, outputs: &Outputs) -> Result<()> {
    let mut label_defs = defs::load_all(labels_json_path)?;
    build_all(notes_dir, out_dir, template_dir, &label_defs, outputs)?;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(300), tx)?;
    debouncer
        .watcher()
        .watch(notes_dir, notify::RecursiveMode::Recursive)?;

    let labels_root = labels_json_path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(root) = labels_root {
        if root != notes_dir {
            debouncer.watcher().watch(root, notify::RecursiveMode::Recursive)?;
        }
    }

    println!("watching {} and defs for changes (ctrl-c to stop)...", notes_dir.display());
    for res in rx {
        let events = match res {
            Ok(events) => events,
            Err(e) => {
                eprintln!("watch error: {e:?}");
                continue;
            }
        };
        if !events.iter().any(|e| e.kind == DebouncedEventKind::Any) {
            continue;
        }

        match defs::load_all(labels_json_path) {
            Ok(fresh) => label_defs = fresh,
            Err(e) => {
                eprintln!("error reloading label defs: {e}");
                continue;
            }
        }
        match build_all(notes_dir, out_dir, template_dir, &label_defs, outputs) {
            Ok(()) => println!("rebuilt all notes"),
            Err(e) => eprintln!("error rebuilding notes: {e}"),
        }
    }
    Ok(())
}

fn build_one(
    path: &Path,
    out_dir: &Path,
    template_dir: &Path,
    defs: &LabelMap,
    outputs: &Outputs,
    css: Option<&str>,
) -> Result<()> {
    let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed = parser::parse(&source);
    let (title, doc) = extract_title(parsed);
    let stem = path
        .file_stem()
        .context("note file has no stem")?
        .to_string_lossy()
        .into_owned();

    if outputs.latex || outputs.pdf {
        let tex_path = write_tex(&doc, &stem, out_dir, template_dir, defs, title.as_deref())?;
        if outputs.pdf {
            compile_pdf(&tex_path, out_dir, &stem)?;
        }
    }
    if outputs.html {
        write_html(&doc, &stem, out_dir, defs, title.as_deref(), css.unwrap_or_default())?;
    }
    Ok(())
}

/// Pulls a top-level `title(...) { }` block's first arg out as the document
/// title, dropping it from the body so it isn't also rendered as content.
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
    template_dir: &Path,
    defs: &LabelMap,
    title: Option<&str>,
) -> Result<PathBuf> {
    let body = render_latex::render(doc, defs);
    let preamble = std::fs::read_to_string(template_dir.join("preamble.tex"))
        .context("reading templates/preamble.tex")?;
    let full = preamble
        .replace("% NOTETAKER:CONTENT", &body)
        .replacen("\\title{}", &format!("\\title{{{}}}", title.unwrap_or(stem)), 1);

    let path: PathBuf = out_dir.join(format!("{stem}.tex"));
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(&path, full)?;
    Ok(path)
}

/// Compiles a `.tex` file to PDF via `latexmk`, which reruns as many times
/// as needed to settle cross-references and the table of contents.
fn compile_pdf(tex_path: &Path, out_dir: &Path, stem: &str) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;

    let status = Command::new("latexmk")
        .arg("-pdf")
        .arg("-interaction=nonstopmode")
        .arg("-halt-on-error")
        .arg(format!("-outdir={}", out_dir.display()))
        .arg(tex_path)
        .status()
        .context("running latexmk (is a LaTeX toolchain with latexmk on PATH?)")?;

    if !status.success() {
        bail!(
            "latexmk failed for {stem} — see {} for details",
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

/// Reads the base page stylesheet and concatenates every label's
/// `style { ... }` block after it, so each note's HTML page can inline all
/// styling in one `<style>` tag — no relative `<link>` needed, which would
/// otherwise have to know where `template_dir` sits relative to `out_dir`
/// (which varies with `--out`).
fn build_css(defs: &LabelMap, template_dir: &Path) -> Result<String> {
    let base = std::fs::read_to_string(template_dir.join("style.css"))
        .context("reading templates/style.css")?;

    let mut labels: Vec<&String> = defs.keys().collect();
    labels.sort();

    let mut css = base;
    for label in labels {
        if let Some(style) = &defs[label].style {
            css.push('\n');
            css.push_str(style);
        }
    }
    Ok(css)
}
