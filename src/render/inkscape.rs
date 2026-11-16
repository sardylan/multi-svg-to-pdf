//! The Inkscape engine: the SVG goes to `inkscape --pipe` on standard input, the PDF (or PNG)
//! comes back on standard output.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::errors::{Error, Result};

const ARGS: [&str; 2] = ["--pipe", "--export-filename=-"];

/// What Inkscape exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Export {
    Pdf,
    /// A PNG image at this resolution, in dots per inch.
    Png {
        dpi: u32,
    },
}

impl Export {
    fn args(self) -> Vec<String> {
        match self {
            Export::Pdf => vec!["--export-type=pdf".to_owned()],
            Export::Png { dpi } => vec![
                "--export-type=png".to_owned(),
                format!("--export-dpi={dpi}"),
            ],
        }
    }
}

pub(crate) struct Inkscape {
    program: PathBuf,
    /// Working directory, so relative links in the template (images) resolve as in Inkscape.
    dir: PathBuf,
}

impl Inkscape {
    pub(crate) fn new(program: &Path, dir: &Path) -> Inkscape {
        Inkscape {
            program: program.to_owned(),
            dir: dir.to_owned(),
        }
    }

    /// Runs Inkscape once for `svg` (I/O: a child process).
    pub(crate) fn run_inkscape(&self, svg: &str, export: Export) -> Result<Vec<u8>> {
        let spawn_error = |source| Error::RunInkscape {
            program: self.program.clone(),
            source,
        };
        let mut child = Command::new(&self.program)
            .args(ARGS)
            .args(export.args())
            // Inkscape registers itself on the D-Bus session as a single application, so
            // instances started in parallel abort with Gio::DBus::Error. Without a session bus
            // each run is independent.
            .env("DBUS_SESSION_BUS_ADDRESS", "disabled:")
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(spawn_error)?;
        let stdin = child.stdin.take();
        // Write on another thread while this one reads, so neither pipe can fill up and block.
        let (written, output) = std::thread::scope(|scope| {
            let writer = scope
                .spawn(move || stdin.map_or(Ok(()), |mut pipe| pipe.write_all(svg.as_bytes())));
            let output = child.wait_with_output();
            let written = writer
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            (written, output)
        });
        interpret(&self.program, written, output.map_err(spawn_error)?)
    }
}

/// Turns the child's result into the exported bytes or an error. A failed exit status wins over a
/// failed write, because a program that stops early also breaks the input pipe.
fn interpret(program: &Path, written: std::io::Result<()>, output: Output) -> Result<Vec<u8>> {
    let failed = |stderr: String| Error::InkscapeFailed {
        program: program.to_owned(),
        status: output.status,
        stderr,
    };
    match (output.status.success(), written, output.stdout.is_empty()) {
        (false, _, _) => Err(failed(String::from_utf8_lossy(&output.stderr).into_owned())),
        (true, Err(source), _) => Err(Error::RunInkscape {
            program: program.to_owned(),
            source,
        }),
        (true, Ok(()), true) => Err(failed("nothing was written to standard output".to_owned())),
        (true, Ok(()), false) => Ok(output.stdout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    /// A fake Inkscape: a shell script in a temporary directory.
    ///
    /// The script is written by a short-lived `sh` child, not by this process: a file this
    /// process holds open for writing leaks into children that other test threads fork at the
    /// same moment, and executing it then fails with "Text file busy" (ETXTBSY).
    fn fake(name: &str, body: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("msvg-test-ink-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("inkscape");
        let status = Command::new("/bin/sh")
            .args([
                "-c",
                "printf '%s\\n' \"$2\" > \"$1\" && chmod 755 \"$1\"",
                "sh",
            ])
            .arg(&program)
            .arg(format!("#!/bin/sh\n{body}"))
            .status()
            .unwrap();
        assert!(status.success());
        (dir, program)
    }

    #[test]
    fn export_arguments() {
        assert_eq!(Export::Pdf.args(), ["--export-type=pdf"]);
        assert_eq!(
            Export::Png { dpi: 300 }.args(),
            ["--export-type=png", "--export-dpi=300"]
        );
    }

    #[test]
    fn passes_the_export_arguments() {
        let (dir, program) = fake("args", "cat > /dev/null; printf %s \"$*\"");
        let result = Inkscape::new(&program, &dir).run_inkscape("<svg/>", Export::Png { dpi: 150 });
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            result.unwrap(),
            b"--pipe --export-filename=- --export-type=png --export-dpi=150"
        );
    }

    #[test]
    fn success_returns_stdout() {
        let pdf = interpret(Path::new("ink"), Ok(()), output(0, b"%PDF-1.5", b"")).unwrap();
        assert_eq!(pdf, b"%PDF-1.5");
    }

    #[test]
    fn failure_reports_stderr() {
        match interpret(Path::new("ink"), Ok(()), output(1, b"", b"bad svg")) {
            Err(Error::InkscapeFailed { stderr, .. }) => assert_eq!(stderr, "bad svg"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn empty_output_is_an_error() {
        assert!(matches!(
            interpret(Path::new("ink"), Ok(()), output(0, b"", b"")),
            Err(Error::InkscapeFailed { .. })
        ));
    }

    #[test]
    fn write_failure_is_an_error_when_the_program_succeeded() {
        let broken = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe");
        assert!(matches!(
            interpret(Path::new("ink"), Err(broken), output(0, b"x", b"")),
            Err(Error::RunInkscape { .. })
        ));
    }

    #[test]
    fn runs_the_program_with_svg_on_stdin() {
        let (dir, program) = fake("echo", "cat");
        let result = Inkscape::new(&program, &dir).run_inkscape("<svg/>", Export::Pdf);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(result.unwrap(), b"<svg/>");
    }

    #[test]
    fn runs_without_a_dbus_session() {
        let (dir, program) = fake(
            "dbus",
            "cat > /dev/null; printf %s \"$DBUS_SESSION_BUS_ADDRESS\"",
        );
        let result = Inkscape::new(&program, &dir).run_inkscape("<svg/>", Export::Pdf);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(result.unwrap(), b"disabled:");
    }

    #[test]
    fn failing_program_is_reported() {
        let (dir, program) = fake("fail", "cat > /dev/null; echo broken >&2; exit 3");
        let result = Inkscape::new(&program, &dir).run_inkscape("<svg/>", Export::Pdf);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(result, Err(Error::InkscapeFailed { .. })));
    }

    #[test]
    fn missing_program_is_reported() {
        let result = Inkscape::new(Path::new("/nonexistent/inkscape"), Path::new("."))
            .run_inkscape("<svg/>", Export::Pdf);
        assert!(matches!(result, Err(Error::RunInkscape { .. })));
    }
}
