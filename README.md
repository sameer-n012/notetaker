# Notetaker

A CLI that renders a lightweight `.note` markup format into HTML, LaTeX, and PDF.
Block types are user-defined via `.def` files, so the rendering of each block is fully configurable.

## Note grammar

Files must have the `.note` extension to be processed.

- **Blocks**: `label(arg1, arg2, ...) #id {` ... `}`.
    - The arguments and ID are optional.
    - A line ending in `{` opens a block; a line containing only `}` closes it. Any other `{`/`}` is literal text.
    - A one-liner `label(...) {}` is a block with an empty body.
    - `label` alone (no parens) is equivalent to `label()`.
- **Inline math**: `$...$`
- **Inline code**: `` `...` ``
- **Display math**: a `$$$` on its own line, or a `mlmath { ... }` block are equivalent multi-line math blocks.
- **Code**: a ` ``` ` or ` ```lang ` or `code(lang) { ... }` block are equivalent multi-line code blocks.
- **References**: `@id` resolves to the numbered target's number as a hyperlink.

See [examples/](examples/) for more.

## Running

```
notetaker build [--out DIR] [--defs DIR] [--html] [--latex] [--pdf] <PATH>
notetaker watch [--out DIR] [--defs DIR] [--html] [--latex] [--pdf] <PATH>
```

- `build` renders once and exits.
- `watch` renders once immediately, then re-renders everything whenever files in the note path or definitions directory change.

- `<PATH>` is a single `.note` file or a directory of them.
    - Required
    - The directory is not scanned recursively; only files directly inside it are processed.
- `--out` is the desired output directory.
    - Defaults to the current working directory
- `--defs` is a directory of `.def` files.
    - If omitted, no label definitions load, and every block falls back to a generic, unstyled environment/div.
- `--pdf`, `--html`, `--latex` specify which outputs to generate.
    - Any combination of them can be passed.
    - If omitted, no output is generated.
    - `--pdf` requires `latexmk` on `PATH`.

## Definition formats

Definitions are contained in `.def` files and pointed to by `_labels.json`. Both of these must be contained in the
directory passed to `--defs`.

The `_labels.json` file maps each label name to its `.def` file (path relative to `DIR`):
```json
{
    "theorem": "theorem.def",
    "section": "section.def"
}
```

A `.def` file has property lines and template sections:
```
numbered: true
toc: true

latex {
\begin{theorem}${1?[$1]}
$body
\end{theorem}
}

html {
<div class="theorem">$n${1? ($1)}: $body</div>
}

style {
.theorem { border-left: 3px solid blue; }
}
```

- `numbered: true|false` (default `false`): gives the block a hierarchical number if true.
- `toc: true|false` (default `false`): gives the block a table of contents entry if true.
- `latex { }` / `html { }` (required): templates for each output type.
- `style { }` (default empty): optional CSS for the block.

Template placeholders:
- `$1`, `$2`, ...: block arguments.
- `$body`: rendered children.
- `$id`/`$id_attr`: the block's ID.

See [defs/](defs/) for more.

## Building

```
cargo build --release
```

Produces `target/release/notetaker`. Templates (LaTeX preamble, base CSS) are compiled into the binary so no external template files needed at runtime.
