# multi-svg-to-pdf design

multi-svg-to-pdf renders a series of PDF files from one SVG template and a list of data rows. For
each row it replaces the placeholders in the template with the row's values and renders the
resulting SVG to PDF.

Status: the whole pipeline is implemented: configuration, logging, template, data, check, render
(native and Inkscape engines) and output (one file per row, or merged).

## Decisions

| Topic | Decision |
|---|---|
| Placeholder syntax | `{{column}}` by default; open and close delimiters are configurable (`template.placeholder_open`, `template.placeholder_close`). Spaces around a name are ignored. |
| Sections | Mustache-like: `{{#col}}…{{/col}}` keeps its content only when `col` is not empty; `{{^col}}…{{/col}}` only when it is empty. Sections nest, and work in the SVG template and in the output name. |
| Filters | `{{col|filter}}`, chained left to right: `lower`, `upper`, `slug` (lowercase, then every character other than `a`-`z`, `0`-`9` becomes `_`). Applied before XML escaping. Not allowed on sections. |
| Unknown placeholder | A placeholder (or section) with no matching column is an **error**, reported before anything is rendered, listing every unknown name. |
| Value escaping | In the SVG every value is XML-escaped (`&`, `<`, `>`, `"`, `'`). In the output name values are written as they are. No raw insertion into the SVG. |
| Data formats | CSV (header row gives the column names, delimiter configurable, `tab` accepted, LF or CRLF, BOM ignored) and JSON (an array of flat objects). `data.path = "-"` reads standard input. `data.format = "auto"` picks JSON for a `.json` path and CSV otherwise, standard input included. |
| Built-in variables | `_row`: the 1-based row number. Column names starting with `_` are refused (reserved for built-ins). |
| Output | One PDF per row in `output.dir`, named by the `output.name` pattern (default `{{_row}}.pdf` with the configured delimiters). With `output.merge = "file.pdf"` all rows become pages of one PDF instead, in row order. |
| File names | A rendered name must not be empty, `.` or `..`, nor contain `/` or NUL. Spaces and other characters are kept. Two rows with the same name are an error. |
| Existing files | Refused unless `output.force` is set. The check runs for every target before any rendering. |
| Engines | `render.engine = "native"` (default): `usvg` + the `svg2pdf` crate, no external programs, plain SVG 1.1 only. `"inkscape"`: runs the Inkscape program (`render.inkscape`); needed for templates that use Inkscape flowed text (`shape-inside`, `text-align`), which the native engine places wrongly. |
| Inkscape runs | One `inkscape --pipe --export-type=pdf --export-filename=-` process per row, SVG on stdin, PDF on stdout, working directory = the template's directory. `DBUS_SESSION_BUS_ADDRESS` is set to `disabled:`: otherwise parallel instances abort with `Gio::DBus::Error` (seen on Inkscape 1.4; `--app-id-tag` alone is not enough). |
| Output format | `render.format = "vector"` (default): shapes and real text. `"raster"`: the card is rendered to one PNG at `render.dpi` (default 300; native with `resvg`, Inkscape with `--export-type=png`) and wrapped into a PDF whose page is the image at that resolution (1028 × 673 px at 300 dpi gives 246.72 × 161.52 pt). Image size is rounded to the nearest pixel, as Inkscape does. |
| Fonts | Native: system fonts plus `render.font_dirs`. Inkscape: fonts installed for fontconfig; `render.font_dirs` is ignored with a warning. |
| Parallelism | Rows are rendered in parallel with `rayon`; `render.jobs` limits the threads (0 = one per core). |
| Failures | The first failing row stops the run; the error names the row (1-based). Files already written for other rows stay, so a rerun needs `--force`. |
| License | GPL-3.0-or-later. |

## Pipeline

Each step is a context with its own module (doc/rust-guidelines.md §4). Pure logic is kept apart
from I/O. `src/pipeline.rs` runs the steps in order.

1. **`template`**: read the SVG text (I/O); parse placeholders and sections into a tree (pure,
   `template/parse.rs`). Scanning is plain text, not XML-aware, so a placeholder works in text,
   attribute values and CSS alike. The output name is parsed the same way.
2. **`data`**: read the CSV or JSON text, or stdin (I/O); parse it into a `Table` of rows
   `column -> value` (pure, `data/parse.rs`). JSON numbers and booleans are written as their JSON
   text, `null` and missing keys as empty; nested arrays or objects are an error. Empty or
   duplicate column names are an error.
3. **`check`** (pure): reserved columns, unknown placeholders, no rows, invalid or duplicate
   output names. Produces one `Job` per row (values plus `_row`, target file).
4. **`output`** (I/O): refuse existing targets unless forced.
5. **`render`**: per row, render the template with XML escaping (pure) and convert it with the
   configured engine, in parallel. The native engine's font database is built once.
6. **`output`**: create `output.dir` and write each PDF; or, for `merge`, combine the per-row PDFs
   with `lopdf` (pure, `output/merge.rs`: inherited page attributes are copied onto each page,
   the source catalogs and page trees are replaced by one) and write the result.

## Crates

| Crate | Use | Note |
|---|---|---|
| `clap` | command line | features `derive`, `env`, `color`, `wrap_help` |
| `serde`, `toml` | configuration file | |
| `tracing`, `tracing-subscriber`, `time` | logging, UTC timestamps | |
| `csv`, `serde_json` | data input | |
| `svg2pdf` | native engine | Use the `usvg` it re-exports (`svg2pdf::usvg`) so the versions always match. Its `ConversionError` does not implement `std::error::Error`, so `Error::Pdf` has no `source()`. `PageOptions::dpi` defaults to 72 while usvg measures in pixels at 96 dpi: always pass usvg's dpi, or pages come out 4/3 too large. |
| `lopdf` | merging PDFs | |
| `resvg`, `png`, `base64` | raster output (native) and the PNG-to-PDF wrapper | `resvg` and `png` versions must match the ones `svg2pdf` / `tiny-skia` use (0.45 / 0.17), so the types are shared. |
| `rayon` | parallel rendering | |

## Configuration

Settings, in TOML layout (environment variable `MULTI_SVG_TO_PDF_<TABLE>_<KEY>`, command-line
option in brackets):

```toml
[template]
path = "template.svg"        # MULTI_SVG_TO_PDF_TEMPLATE_PATH, -t/--template
placeholder_open = "{{"      # --placeholder-open
placeholder_close = "}}"     # --placeholder-close

[data]
path = "data.csv"            # -d/--data, "-" for stdin
format = "auto"              # --data-format: auto, csv, json
csv_delimiter = ","          # --csv-delimiter: one ASCII char, "tab" or "\t"

[output]
dir = "output"               # -o/--output-dir
name = "{{_row}}.pdf"        # --output-name
# merge = "all.pdf"          # -m/--merge
force = false                # -f/--force

[render]
jobs = 0                     # -j/--jobs, at most 1024
font_dirs = []               # --font-dir (repeatable; ':'-separated in the environment)
engine = "native"            # -e/--engine: native, inkscape
inkscape = "inkscape"        # --inkscape: the Inkscape program
format = "vector"            # --format: vector, raster
dpi = 300                    # --dpi: raster resolution, 1 to 2400

[log]
level = "info"               # --log-level: off, error, warning, info, debug, trace
```

The configuration file itself is `multi-svg-to-pdf.toml` in the current directory (`-c/--config`,
`MULTI_SVG_TO_PDF_CONFIG`). Relative paths are relative to the current directory.

## Example

A card per member, as 300 dpi images, named like `ottavio_rossi.pdf` (`ZETA'` becomes
`zeta_`, `ANNA ERMINIA` becomes `anna_erminia`). This reproduces the reference cards pixel for pixel:

```toml
[template]
path = "template.svg"

[data]
path = "list.csv"

[output]
name = "{{surname|slug}}_{{name|slug}}.pdf"

[render]
engine = "inkscape"
format = "raster"
dpi = 300
```

To append the call sign only when there is one:
`{{surname|slug}}_{{name|slug}}{{#callsign}}_{{callsign}}{{/callsign}}.pdf`.
