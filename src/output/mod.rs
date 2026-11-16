//! Writing the PDFs: the existing-file check, thin writes, and the pure merge in `merge.rs`.

mod merge;

use std::path::{Path, PathBuf};

use crate::errors::{Error, Result};

pub(crate) use merge::merge_pdfs;

/// The targets that already exist (I/O).
pub(crate) fn find_existing(targets: &[&Path]) -> Vec<PathBuf> {
    targets
        .iter()
        .filter(|path| path.exists())
        .map(|path| path.to_path_buf())
        .collect()
}

/// Fails when files exist and replacing them was not allowed.
pub(crate) fn refuse_existing(existing: Vec<PathBuf>, force: bool) -> Result<()> {
    match force || existing.is_empty() {
        true => Ok(()),
        false => Err(Error::OutputExists { paths: existing }),
    }
}

/// Creates `dir` and its parents when missing. An empty path is the current directory.
pub(crate) fn create_dir(dir: &Path) -> Result<()> {
    match dir.as_os_str().is_empty() {
        true => Ok(()),
        false => std::fs::create_dir_all(dir).map_err(|source| Error::CreateDir {
            path: dir.to_owned(),
            source,
        }),
    }
}

pub(crate) fn write_pdf(path: &Path, pdf: &[u8]) -> Result<()> {
    std::fs::write(path, pdf).map_err(|source| Error::WriteOutput {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("msvg-test-out-{name}-{}", std::process::id()))
    }

    #[test]
    fn existing_files_are_refused_without_force() {
        assert!(refuse_existing(vec![], false).is_ok());
        assert!(refuse_existing(vec!["a.pdf".into()], true).is_ok());
        assert!(matches!(
            refuse_existing(vec!["a.pdf".into()], false),
            Err(Error::OutputExists { .. })
        ));
    }

    #[test]
    fn finds_existing_and_writes() {
        let dir = temp("write").join("nested");
        create_dir(&dir).unwrap();
        create_dir(Path::new("")).unwrap();
        let file = dir.join("a.pdf");
        assert!(find_existing(&[&file]).is_empty());
        write_pdf(&file, b"%PDF").unwrap();
        assert_eq!(find_existing(&[&file]), std::slice::from_ref(&file));
        let written = std::fs::read(&file).unwrap();
        std::fs::remove_dir_all(temp("write")).unwrap();
        assert_eq!(written, b"%PDF");
    }

    #[test]
    fn write_errors_carry_the_path() {
        let missing = temp("missing-dir").join("a.pdf");
        assert!(matches!(
            write_pdf(&missing, b"x"),
            Err(Error::WriteOutput { .. })
        ));
    }
}
