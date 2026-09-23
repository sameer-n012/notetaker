mod ast;
mod defs;
mod fmt;
mod numbering;
mod parser;
mod render_html;
mod render_latex;
mod render_markdown;
mod watch;

use anyhow::Result;
use clap::{Parser, Subcommand};
use defs::LabelMap;
use std::path::PathBuf;
use watch::Outputs;

#[derive(Parser)]
#[command(name = "notetaker")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser)]
struct Args {
    /// A single .note file, or a directory of .note files, to render.
    /// There is no recursive search in a directory.
    path: PathBuf,

    /// Directory to write generated output into (.html/.tex/.pdf/.md files land
    /// directly in it, alongside each other).
    #[arg(long, default_value = ".")]
    out: PathBuf,

    /// Directory holding label `.def` files and their index, `_labels.json`.
    /// If omitted, no label definitions are loaded at all — every block
    /// falls back to a generic, unstyled environment/div. You're
    /// responsible for pointing this at a defs directory you maintain.
    #[arg(long)]
    defs: Option<PathBuf>,

    /// Generate HTML. If none of the output types are given, nothing is
    /// generated. You must pass at least one to produce output.
    #[arg(long)]
    html: bool,

    /// Generate LaTeX. If none of the output types are given, nothing is
    /// generated. You must pass at least one to produce output.
    #[arg(long)]
    latex: bool,

    /// Generate PDF (using latexmk). Also writes the .tex the PDF compiles from.
    /// If none of the output types are given, nothing is
    /// generated. You must pass at least one to produce output.
    #[arg(long)]
    pdf: bool,

    /// Generate Markdown (GitHub-flavored, with $...$ / $$...$$ math).
    /// If none of the output types are given, nothing is
    /// generated. You must pass at least one to produce output.
    #[arg(long, visible_alias = "md")]
    markdown: bool,
}

#[derive(Parser)]
struct FmtArgs {
    /// A single .note file, or a directory of .note files, to format.
    /// If omitted, reads source from stdin and writes formatted output to
    /// stdout
    path: Option<PathBuf>,

    /// Rewrite the file(s) in place instead of printing to stdout.
    #[arg(long, short)]
    write: bool,

    /// Check that the file(s) are already formatted; print nothing, and
    /// exit non-zero if any aren't.
    #[arg(long)]
    check: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Build the specified .note files once.
    Build(Args),

    /// Build the specified .note files once, then watch for any changes
    /// of .note files or the definitions folder Rebuilds all .note files
    /// on any change.
    Watch(Args),

    /// Reformat .note source into canonical layout.
    Fmt(FmtArgs),
}

fn render(args: &Args, watch_after: bool) -> Result<()> {
    let outputs = Outputs {
        html: args.html,
        latex: args.latex,
        pdf: args.pdf,
        markdown: args.markdown,
    };
    if !outputs.html && !outputs.latex && !outputs.pdf && !outputs.markdown {
        eprintln!("No output format requested.");
    }

    let labels_json = args.defs.as_ref().map(|d| d.join("_labels.json"));

    if watch_after {
        watch::watch(&args.path, &args.out, labels_json.as_deref(), &outputs)
    } else {
        let label_defs = match &labels_json {
            Some(p) => defs::load_all(p)?,
            None => LabelMap::new(),
        };
        watch::build_all(&args.path, &args.out, &label_defs, &outputs)
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Build(args) => render(args, false),
        Command::Watch(args) => render(args, true),
        Command::Fmt(args) => fmt::run(args.path.as_deref(), args.write, args.check),
    }
}
