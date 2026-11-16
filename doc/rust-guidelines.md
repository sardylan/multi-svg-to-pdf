# Rust Implementation Guidelines

These rules are binding for all Rust code in this repository. Code review enforces them.

"When possible" means that a rule can only be broken when following it is impractical, and the code
MUST say why in a short comment.

## 1. Errors and `Result`

### 1.1 One crate error type

Each crate MUST define exactly one error type, `enum Error`, in its `src/errors.rs`. The rules
below apply to every crate on its own; crates do not share an error type.

- Each failure the program can meet is a variant of `Error`. Variants carry the context needed to
  understand the failure, such as the path, the input or the source error.
- `Error` implements `std::fmt::Display` and `std::error::Error`. `source()` returns the wrapped
  error whenever a variant wraps one.
- `Error` derives `Debug`. A handwritten `Debug` is allowed when a variant holds secrets, which
  must be redacted.

### 1.2 Crate `Result` alias

`src/errors.rs` MUST declare:

```rust
pub(crate) type Result<T> = std::result::Result<T, Error>;
```

(`pub` instead of `pub(crate)` only in a library crate that exports it.)

Every fallible function in the crate MUST return this `Result<T>`. In particular:

- MUST NOT spell out `std::result::Result<T, Error>` where the alias applies;
- MUST NOT return `Box<dyn std::error::Error>`, `String` or `&'static str` as an error type;
- MUST NOT define module-local error enums. Add a variant to the crate `Error` instead.

The only exception is a signature fixed by someone else: a trait method (for example
`fmt::Display::fmt` returning `std::fmt::Result`, or serde's `Serialize`/`Deserialize`) or a
callback type a library imposes. In those places the fully qualified type is used.

Modules import the alias explicitly:

```rust
use crate::errors::{Error, Result};
```

### 1.3 No error-handling crates

`thiserror`, `anyhow`, `eyre`, `snafu` and similar crates MUST NOT be added. `Debug`, `Display`,
`std::error::Error` and every `From` are written by hand in `errors.rs`.

### 1.4 Foreign errors, context and `?`

- **Every foreign error type** the crate meets (from `std` or from a dependency) has its own
  variant that wraps it, and an `impl From<ForeignError> for Error`, so that `?` converts it
  without `map_err`.
- A foreign error is **never discarded** (`map_err(|_| ...)`, `.ok()` on a `Result` whose error
  matters) and **never flattened into a string** (`e.to_string()`, `format!("{e}")`). It is always
  kept as the source of a crate error, so the full cause chain survives. Turning an error into
  text is allowed only at the edge where it is shown (a log line, an API error body, a status
  field).
- When a failure needs **context** that `From` cannot know (a path, an address, the input being
  parsed), use a context variant that holds the original error as `source`. Either keep the
  foreign type (`source: std::io::Error`) or the converted crate error (`source: Box<Error>`,
  built with `.into()`).
- **Failures detected by our own checks**, with no foreign error behind them (an invalid input
  value, a malformed record, a remote service rejecting a request), get their own variants with
  structured fields and a reason instead of a source (for example
  `Invalid { what: &'static str, input: String, reason: String }`), not a bare `String`.

### 1.5 Reference shape

```rust
// src/errors.rs
use std::fmt;
use std::num::ParseIntError;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) enum Error {
    // Foreign errors, each with a `From` conversion.
    Io(std::io::Error),
    Int(ParseIntError),

    // Context around a foreign error.
    Read { path: PathBuf, source: std::io::Error },
    Parse { what: &'static str, input: String, source: Box<Error> },

    // Our own checks.
    Invalid { what: &'static str, input: String, reason: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Int(e) => write!(f, "invalid integer: {e}"),
            Error::Read { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Error::Parse { what, input, source } => write!(f, "cannot parse {what} from {input:?}: {source}"),
            Error::Invalid { what, input, reason } => write!(f, "invalid {what} {input:?}: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Int(e) => Some(e),
            Error::Read { source, .. } => Some(source),
            Error::Parse { source, .. } => Some(source.as_ref()),
            Error::Invalid { .. } => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Error::Int(e)
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
```

```rust
// Using it: `?` converts through `From`; context wraps the converted error.
fn count(input: &str) -> Result<u32> {
    input.trim().parse::<u32>().map_err(|e| Error::Parse {
        what: "count",
        input: input.to_owned(),
        source: Box::new(e.into()),
    })
}

fn read_config(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Read { path: path.to_owned(), source })
}
```

### 1.6 Panics

`unwrap()` and `expect()` MUST NOT be used outside tests. The only exception is an invariant the
compiler cannot see, and it needs a comment explaining why it cannot fail.

## 2. Entry point

`main()` does no work itself. It calls `program()`, then turns the result into a
`std::process::ExitCode`:

```rust
use std::process::ExitCode;

fn main() -> ExitCode {
    match program() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn program() -> crate::errors::Result<()> {
    // load configuration, run, shut down
    Ok(())
}
```

- Only `main()` decides the exit code. Nothing else calls `std::process::exit`.
- If different failures need different exit codes, add `Error::exit_code(&self) -> ExitCode` and
  use it in `main()`.
- `program()` MUST be synchronous. If it needs an async runtime, `program()` builds the runtime
  itself and blocks on it, instead of putting an attribute macro such as `#[tokio::main]` on
  `main()`.

## 3. Configuration

Each setting is resolved from these sources, in order of precedence. The first source that sets a
value wins:

1. command-line arguments;
2. environment variables;
3. the configuration file;
4. built-in default values.

- Every setting has a sensible default, so the program runs with no arguments, no environment
  variables and no configuration file.
- A missing configuration file is not an error. A file that exists but cannot be read or parsed is
  an error.
- The path of the configuration file is itself a setting (`--config`), which follows the same
  order except that it cannot come from the file.
- Resolve the configuration once at startup into one immutable `Config` struct. The rest of the
  program receives it, or the parts it needs, as arguments and never reads arguments, environment
  variables or files on its own.
- Keep merging pure: read each source into a partial struct of `Option` fields, then combine the
  layers with a pure function (for example `cli.or(env).or(file).unwrap_or(default)` for each
  field). Test the merge without touching the real environment or file system.
- Validate values while resolving them and report a bad one as a configuration error that names
  the setting and the source it came from.

### 3.1 Configuration file

- The configuration file is in TOML format.
- Keys use `snake_case`. Related settings are grouped in tables (`[web]`, `[storage]`, ...).
- Deserialize with `serde` into the partial struct. Reject unknown keys
  (`#[serde(deny_unknown_fields)]`), so that typos do not pass silently.

### 3.2 Environment variables

- Each environment variable name is the program name followed by the setting name, in upper case
  with underscores, for example `MYAPP_LISTEN_ADDRESS` or `MYAPP_WEB_PORT`.
- Every setting that can be set on the command line can also be set through an environment
  variable.

### 3.3 Command line

- Every option has a long form. Long names are lower case, with words joined by dashes: `--param`,
  `--param-name`, `--foo-param-name`.
- Long options take their value either way: `--param=value` or `--param value`.
- Only the most important options also have a short form of a single letter. Short options take
  their value either way: `-pvalue` or `-p value`.
- `--help` and `--version` are always available, and `--help` shows the default value and the
  environment variable of each option.

### 3.4 clap

Parse the command line with `clap`, using the `derive`, `env`, `color` and `wrap_help` features. Do
not parse `std::env::args()` by hand.

- **Colors:** give the parser an explicit `Styles` (headers, usage, literals, placeholders, errors
  and valid/invalid values) so that help and error output are coloured. Keep `ColorChoice::Auto`,
  so colours are turned off when the output is not a terminal or when `NO_COLOR` is set.
- **Long help:** every option and the command itself have a short help (the first doc-comment
  line, shown by `-h`) and a long help (the following paragraphs, shown by `--help`). The long help
  explains what the setting does, what values it accepts and when to change it. Use
  `after_long_help` for examples.
- **Emoji:** use one emoji at the start of the command description and of each help heading, to
  make the sections easy to tell apart. Group options under headings with `help_heading`. Do not
  put emoji in option names, values or error messages, because they must stay easy to copy and
  grep.

```rust
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::Parser;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Red.on_default());

/// 🚀 Short description of the program.
///
/// Longer description, shown only by --help: what the program does, how it
/// is configured and where to find more documentation.
#[derive(Debug, Parser)]
#[command(version, styles = STYLES, after_long_help = "📖 Examples:\n  myapp --listen-address 0.0.0.0:8080")]
struct Cli {
    /// Path of the TOML configuration file.
    ///
    /// Settings on the command line and in environment variables override the
    /// ones in this file. A missing file is not an error.
    #[arg(short, long, env = "MYAPP_CONFIG", help_heading = "⚙️  Configuration")]
    config: Option<std::path::PathBuf>,

    /// Address and port the web server listens on.
    #[arg(long, env = "MYAPP_LISTEN_ADDRESS", help_heading = "🌐 Web")]
    listen_address: Option<std::net::SocketAddr>,
}
```

### 3.5 Writing the configuration file

Two command-line flags write the configuration file from the resolved configuration:

- `--save-config` writes the file, then starts the program normally;
- `--dump-config` writes the file, then exits with status 0 without starting anything.

Both follow these rules:

- **Same sources, same order.** The configuration is resolved exactly as for a normal start
  (command line, environment, file, defaults, §3), so a value given on the command line or in the
  environment ends up in the file.
- **Validate first.** The configuration is fully validated before anything is written. A bad value
  is reported as in §3 and the file is left untouched.
- **Every setting is written**, defaults included, with the value the program would use, derived
  defaults resolved (for example a path built from another setting). A setting with no value at
  all (an optional one left unset) is written as a commented-out line, so that the file still
  lists it. Each key is preceded by a comment with its short help, taken from the same text as
  `--help`, so the two never disagree.
- **The output is a valid input.** Reading the written file back, with no other source, gives the
  same configuration. A test checks this round trip.
- **Where it goes.** The file is written to the configuration file path (`--config`, its
  environment variable, or its default, §3). Neither flag requires `--config`: without it, the file
  at the default path is created or replaced. A missing parent folder is created.
- **Overwrite, don't merge.** An existing file is replaced as a whole. Its comments and layout are
  not kept; its values are, because the file was one of the sources.
- So, with no other option, no environment variable and a path that does not exist,
  `--dump-config` creates a new file with every setting at its default value.
- Writing is a side effect at the edge (§6): a pure function renders the text from the `Config`,
  a thin function writes it.
- These flags (like `--help`, `--version` or a configuration check) are actions, not settings:
  they exist only on the command line, with no environment variable and no key in the file. They
  exclude each other.

### 3.6 Logging

- Log with the `tracing` crate (`error!`, `warn!`, `info!`, `debug!`, `trace!`) and print the
  lines with `tracing-subscriber`. Do not write log lines with `println!` / `eprintln!` or the
  `log` crate. The only direct writes are the program's own output (for example the result of a
  configuration check) and the final error printed by `main()` (§2).
- The level is an ordinary setting (§3): `--log-level`, the environment variable
  `<PROGRAM>_LOG_LEVEL` (§3.2, for example `MYAPP_LOG_LEVEL`) and the `level` key of the `[log]`
  table, default `info`.
- Accepted values are the canonical level names only: `off`, `error`, `warning`, `info`, `debug`,
  `trace`, in any letter case (`WARNING`, `Info`). Other spellings (`warn`, numbers, filter
  directives) are refused as an invalid value of the setting. A level shows its own messages and
  every more severe one; `off` shows nothing.
- The level applies to every log line of the program and of its libraries. `RUST_LOG` and other
  logging variables are not read, so the level has one place to be set and §3 stays the only
  order of precedence.
- Start logging once the configuration is resolved, since the level is part of it. Messages
  produced before that (for example "no configuration file") are returned to the caller and logged
  just after the start. An error that stops the program before logging starts is printed by
  `main()`.
- **Every line starts with a timestamp**: the time the event was logged, in **UTC**, RFC 3339 with
  **microseconds** (exactly six fraction digits) and a `Z` suffix, for example
  `2026-11-13T10:15:03.123456Z`. The format never depends on the host's time zone or locale, and
  is the same when the output goes to a terminal, a file or a log collector that adds its own
  time.
- Areas whose lines one may want to tell apart (an access log, the output of a child process) log
  under their own `target:`, documented with the program's logs.

## 4. Modules

There is one module per context: each area of the program owns its own types, logic and tests.

- Do not make catch-all modules such as `utils` or `helpers`. Put code in the context it belongs
  to.
- Inside a module, keep pure logic (parsing and computing) apart from I/O (files, sockets,
  subprocesses). A common split is `parse.rs` for pure code and `mod.rs` or `client.rs` for the
  I/O.
- Keep visibility as narrow as possible: private by default, `pub(crate)` when another module needs
  it. A binary crate needs no `pub` items.

## 5. Functional style

Use functional programming wherever it works:

- Prefer iterator chains (`map`, `filter`, `filter_map`, `fold`, `collect`) over hand-written loops
  that mutate accumulators.
- Prefer expressions over statements: `match` and `if` produce values, and `let` bindings are not
  reassigned.
- Bindings are immutable by default. Use `let mut` only when local mutation is clearly simpler, and
  keep it inside one function.
- Handle `Option` and `Result` with combinators (`map`, `and_then`, `ok_or`, `unwrap_or_default`)
  and `?`, not with nested `match` or `if let` ladders.
- Collect a sequence of fallible results with `collect::<Result<Vec<_>>>()`.
- Model states with enums and data with plain structs. Prefer making a new value over changing one
  in place.

```rust
// Good: pure, declarative, fallible as a whole
fn parse_records(input: &str) -> Result<Vec<Record>> {
    input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse_record)
        .collect()
}
```

## 6. Side effects

Functions are pure wherever possible: the output depends only on the inputs, and the function does
no I/O, mutates nothing shared, and does not read the clock or the environment.

- Push side effects to the edges. Read the raw input (I/O), hand it to a pure function, then write
  or publish the result (I/O). The middle step holds all the logic.
- Pass in what a function would otherwise fetch: take `now: SystemTime` as an argument instead of
  calling `SystemTime::now()` inside, and take a `&str` instead of a path.
- A function that must have side effects should look like it: name it with a verb that says so
  (`read_`, `fetch_`, `write_`, `publish_`), and keep it short and free of logic.
- Logging is the only side effect allowed in otherwise pure code. Even so, prefer returning
  information to the caller over logging it deep inside.

## 7. Tests

Every module has tests.

- Unit tests live in a `#[cfg(test)] mod tests` block in the same file as the code they test.
- Every pure function has tests. They cover the normal case, edge cases (empty input, missing
  fields, unexpected values) and the error variants it can return.
- `src/errors.rs` has tests that cover a non-empty `Display` for every variant, every `From`
  conversion, and `source()` for the variants that wrap another error.
- Parsers are tested against real sample input stored in `tests/fixtures/`, loaded with
  `include_str!`.
- I/O code is kept thin, so most behaviour can be tested through the pure functions it calls. Where
  I/O itself must be tested, use local fakes (a temporary file or a loopback listener), never real
  external systems.
- Tests may use `unwrap()` and `expect()`.
- Tests do not depend on wall-clock time, run order, or the real environment.
- `cargo test` passes with no network access and no external services.

## 8. Toolchain

### 8.1 Latest stable for development

Development MUST use the latest stable Rust. `rust-toolchain.toml` selects the `stable` channel;
run `rustup update stable` before starting a feature. Nothing but the builds in §8.2 may pin a Rust
version.

### 8.2 Pinned versions in builds

CI and container image builds MUST pin an exact Rust version, so builds are reproducible. The
pinned version overrides `rust-toolchain.toml` (for example through `RUSTUP_TOOLCHAIN`). Every pin
MUST name the same version. Raise them deliberately, together, after
`cargo clippy --all-targets -- -D warnings` and `cargo test` pass on the new version.
