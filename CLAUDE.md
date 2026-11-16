# CLAUDE.md

multi-svg-to-pdf is a Rust command-line tool that renders one PDF per data row from an SVG template: it
reads the SVG, replaces placeholders such as `{{name}}` with the values of each CSV or JSON row,
and renders each result to PDF.

## Documentation (read first)

- [doc/rust-guidelines.md](doc/rust-guidelines.md): **binding** coding rules. Every change must
  follow them: one `Error` enum in `src/errors.rs` with hand-written `Display`/`From` (no
  `thiserror`/`anyhow`), no `unwrap`/`expect` outside tests, `main()` → `program()`,
  configuration precedence CLI > env > TOML file > defaults, `tracing` for logs, functional
  style, pure logic apart from I/O, tests in every module.
- [doc/design.md](doc/design.md): product decisions, the planned pipeline, crates and the full
  list of settings. Update it when a decision changes.

## Layout

- `src/main.rs`: entry point (`main` → `program`), dispatches `--save-config`/`--dump-config`.
- `src/errors.rs`: the crate `Error` and `Result<T>`.
- `src/logging.rs`: `LogLevel` and the `tracing` subscriber (UTC, microsecond timestamps).
- `src/config/`: `cli.rs` (clap, split into command-line and environment layers), `file.rs`
  (TOML read/parse), `resolve.rs` (pure merge + validation), `write.rs` (render/write the file).
- `src/pipeline.rs`: the rendering run, step by step (see doc/design.md "Pipeline").
- `src/template/`: read the SVG; `parse.rs` parses `{{x}}`, `{{#x}}…{{/x}}`, `{{^x}}…{{/x}}`.
- `src/data/`: read file or stdin; `parse.rs` parses CSV and JSON into a `Table`.
- `src/check.rs`: pure validation, produces one `Job` per row.
- `src/render/`: `native.rs` (usvg + svg2pdf, resvg for PNG), `inkscape.rs` (child process,
  stdin/stdout, PDF or PNG export), `raster.rs` (PNG wrapped into a PDF page).
- `src/output/`: existing-file check and writes; `merge.rs` merges PDFs with lopdf.
- `tests/fixtures/`: sample inputs loaded with `include_str!`.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

`cargo clippy` with `-D warnings` and `cargo test` must pass before a change is done.

## Gotchas

- For the PDF backend (the `svg2pdf` crate), use the `usvg` it re-exports, not a separate
  `usvg` dependency, so the versions always match.
- svg2pdf's `PageOptions::dpi` defaults to 72 but usvg measures at 96 dpi: pass usvg's dpi,
  otherwise pages are 4/3 too large (a test checks the card size).
- Adding a setting touches: `Cli` (with `env` and `help_heading`), the `Partial*` structs,
  `resolve`, `write::render_toml`, `defaults`, the `full.toml` fixture and doc/design.md. The
  round-trip test in `write.rs` must keep passing.
- Tests must not read the real environment: use `cli::tests::command_without_env()`.
- Tests never run the real Inkscape: `render/inkscape.rs` tests use fake shell scripts, and
  native-engine tests load no system fonts.
- The native engine misplaces Inkscape flowed text (`shape-inside`, `text-align`); templates
  made in Inkscape need `--engine inkscape`.
- Inkscape instances in parallel abort on D-Bus; keep `DBUS_SESSION_BUS_ADDRESS=disabled:` on
  the child process.
- Toolchain: latest stable (`rust-toolchain.toml`); only CI/container builds pin a version.
