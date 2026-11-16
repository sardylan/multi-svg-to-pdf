# multi-svg-to-pdf

Render one PDF per data row from an SVG template.

Draw a card, badge, certificate or label once in an SVG editor, put placeholders such as
`{{name}}` where the data goes, and give multi-svg-to-pdf a CSV or JSON list: it fills the
template for every row and writes one PDF per row, or one multi-page PDF with all of them.

## Features

- **Placeholders** `{{column}}`, with configurable delimiters.
- **Conditional sections**: `{{#call}}…{{/call}}` only when `call` has a value,
  `{{^call}}…{{/call}}` only when it is empty.
- **Filters**: `{{name|upper}}`, `{{name|lower}}`, `{{name|slug}}` (lowercase, every other
  character than `a`-`z` and `0`-`9` becomes `_`), chained left to right.
- **File names from the data**, with the same syntax:
  `{{surname|slug}}_{{name|slug}}{{#call}}_{{call}}{{/call}}.pdf`.
- **Data** from CSV (any one-character delimiter, LF or CRLF), JSON (an array of flat objects),
  or standard input.
- **Two engines**: a built-in one (no dependencies) and [Inkscape](https://inkscape.org), for
  templates that use Inkscape-specific features such as flowed text.
- **Vector or raster** PDFs; raster at any resolution, keeping the template's physical size.
- **Merge** all rows into one PDF.
- **Safe by default**: unknown placeholders, invalid or duplicate file names and existing output
  files are reported before anything is rendered.
- **Parallel** rendering on all CPU cores.

## Install

Requires a recent stable [Rust](https://www.rust-lang.org/tools/install) toolchain.

```bash
git clone https://github.com/sardylan/multi-svg-to-pdf.git
cd multi-svg-to-pdf
cargo build --release
```

The program is `target/release/multi-svg-to-pdf`. For the Inkscape engine, Inkscape 1.2 or
later must be installed and on `PATH` (or set with `--inkscape`).

## Quick start

`card.svg`, made with any SVG editor:

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="87mm" height="57mm" viewBox="0 0 87 57">
  <rect width="87" height="57" fill="#dcdcec"/>
  <text x="5" y="48" font-family="sans-serif" font-size="5">{{name}} {{surname}}</text>
  <text x="82" y="16" font-family="sans-serif" font-size="6" text-anchor="end">{{call}}</text>
</svg>
```

`people.csv`:

```csv
name,surname,call
Ottavio,Rossi,N0CALL
Laura,Bianchi,
```

Render:

```bash
multi-svg-to-pdf -t card.svg -d people.csv -o cards \
  --output-name '{{surname|slug}}_{{name|slug}}{{#call}}_{{call}}{{/call}}.pdf'
```

Result: `cards/rossi_ottavio_N0CALL.pdf` and `cards/bianchi_laura.pdf`.

More examples:

```bash
# All rows as pages of one PDF
multi-svg-to-pdf -t card.svg -d people.csv --merge cards.pdf

# Template made with Inkscape, 300 dpi images like an Inkscape PNG export
multi-svg-to-pdf -t card.svg -d people.csv --engine inkscape --format raster --dpi 300

# Semicolon-separated data from standard input
cat people.csv | multi-svg-to-pdf -t card.svg -d - --csv-delimiter ';'
```

## Template syntax

| Syntax | Meaning |
|---|---|
| `{{name}}` | The value of column `name`. Spaces inside the braces are ignored. |
| `{{name\|slug}}` | The value through one or more filters: `lower`, `upper`, `slug`. |
| `{{#name}}…{{/name}}` | The content, only when `name` is not empty. Sections nest. |
| `{{^name}}…{{/name}}` | The content, only when `name` is empty. |
| `{{_row}}` | Built-in: the row number, starting at 1. |

Placeholders work anywhere in the SVG text: element text, attribute values, CSS. Values are
XML-escaped in the SVG, so `&` or `<` in the data cannot break it. Column names starting with
`_` are reserved for built-ins.

Change the delimiters with `--placeholder-open` and `--placeholder-close` when the template
already uses `{{` for something else.

## Engines and formats

| | `--engine native` (default) | `--engine inkscape` |
|---|---|---|
| Needs | nothing | Inkscape 1.2+ |
| SVG support | SVG 1.1 | everything Inkscape draws |
| Fonts | system fonts and `--font-dir` | fonts installed for Inkscape |

Templates made with Inkscape often use *flowed text* (text inside a shape, `text-align`), which
the native engine places in the wrong position: use the Inkscape engine for them.

`--format vector` (default) keeps shapes and text: sharp at any zoom, small files, selectable
text. `--format raster` puts one image per page, rendered at `--dpi` (default 300).

## Configuration

Every setting can be given on the command line, in an environment variable or in a TOML
configuration file, in that order of precedence. The file is `multi-svg-to-pdf.toml` in the
current directory unless `--config` says otherwise; it is optional.

```bash
# Write every current setting, with comments, to the configuration file
multi-svg-to-pdf -t card.svg -d people.csv --engine inkscape --dump-config

# From now on, in this directory, just run
multi-svg-to-pdf
```

| Setting (TOML) | Option | Environment variable | Default |
|---|---|---|---|
| `template.path` | `-t`, `--template` | `MULTI_SVG_TO_PDF_TEMPLATE_PATH` | `template.svg` |
| `template.placeholder_open` | `--placeholder-open` | `MULTI_SVG_TO_PDF_TEMPLATE_PLACEHOLDER_OPEN` | `{{` |
| `template.placeholder_close` | `--placeholder-close` | `MULTI_SVG_TO_PDF_TEMPLATE_PLACEHOLDER_CLOSE` | `}}` |
| `data.path` | `-d`, `--data` | `MULTI_SVG_TO_PDF_DATA_PATH` | `data.csv` (`-` is stdin) |
| `data.format` | `--data-format` | `MULTI_SVG_TO_PDF_DATA_FORMAT` | `auto` (`.json` is JSON, else CSV) |
| `data.csv_delimiter` | `--csv-delimiter` | `MULTI_SVG_TO_PDF_DATA_CSV_DELIMITER` | `,` (`tab` accepted) |
| `output.dir` | `-o`, `--output-dir` | `MULTI_SVG_TO_PDF_OUTPUT_DIR` | `output` |
| `output.name` | `--output-name` | `MULTI_SVG_TO_PDF_OUTPUT_NAME` | `{{_row}}.pdf` |
| `output.merge` | `-m`, `--merge` | `MULTI_SVG_TO_PDF_OUTPUT_MERGE` | unset |
| `output.force` | `-f`, `--force` | `MULTI_SVG_TO_PDF_OUTPUT_FORCE` | `false` |
| `render.engine` | `-e`, `--engine` | `MULTI_SVG_TO_PDF_RENDER_ENGINE` | `native` |
| `render.inkscape` | `--inkscape` | `MULTI_SVG_TO_PDF_RENDER_INKSCAPE` | `inkscape` |
| `render.format` | `--format` | `MULTI_SVG_TO_PDF_RENDER_FORMAT` | `vector` |
| `render.dpi` | `--dpi` | `MULTI_SVG_TO_PDF_RENDER_DPI` | `300` |
| `render.jobs` | `-j`, `--jobs` | `MULTI_SVG_TO_PDF_RENDER_JOBS` | `0` (one per core) |
| `render.font_dirs` | `--font-dir` | `MULTI_SVG_TO_PDF_RENDER_FONT_DIRS` | none |
| `log.level` | `--log-level` | `MULTI_SVG_TO_PDF_LOG_LEVEL` | `info` |

`--help` describes every option in detail.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Coding rules are in [doc/rust-guidelines.md](doc/rust-guidelines.md); design decisions and the
pipeline in [doc/design.md](doc/design.md).

## License

GNU General Public License v3.0 or later. See [LICENSE.md](LICENSE.md).
