mod ast;
mod defs;
mod numbering;
mod parser;
mod render_html;
mod render_latex;
mod watch;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use watch::Outputs;

#[derive(Parser)]
#[command(name = "notetaker")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "notes")]
    notes_dir: PathBuf,

    /// Directory to write generated output into (html/, latex/, pdf/
    /// subdirectories are created under it as needed).
    #[arg(long, default_value = ".")]
    out: PathBuf,

    #[arg(long, default_value = "templates")]
    template_dir: PathBuf,

    #[arg(long, default_value = "labels.json")]
    labels: PathBuf,

    /// Generate HTML. If none of --html/--latex/--pdf are given, nothing is
    /// generated — pass at least one to produce output.
    #[arg(long)]
    html: bool,

    /// Generate LaTeX (.tex).
    #[arg(long)]
    latex: bool,

    /// Generate PDF (via latexmk). Implies writing the .tex it compiles from.
    #[arg(long)]
    pdf: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Render every note once.
    Build,
    /// Render every note, then keep rebuilding on note or def-file changes.
    Watch,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let outputs = Outputs { html: cli.html, latex: cli.latex, pdf: cli.pdf };
    if !outputs.html && !outputs.latex && !outputs.pdf {
        eprintln!("no output format requested — pass --html, --latex, and/or --pdf");
    }

    match cli.command {
        Command::Build => {
            let label_defs = defs::load_all(&cli.labels)?;
            watch::build_all(&cli.notes_dir, &cli.out, &cli.template_dir, &label_defs, &outputs)?;
        }
        Command::Watch => watch::watch(&cli.notes_dir, &cli.out, &cli.template_dir, &cli.labels, &outputs)?,
    }
    Ok(())
}
