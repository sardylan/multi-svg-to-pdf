//! Pure checks before rendering: every placeholder is known, every output name is valid and
//! unique. Produces the list of jobs to render.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::defaults::ROW_VARIABLE;
use crate::data::{Row, Table};
use crate::errors::{Error, Result};
use crate::template::{Escape, Template};

/// One row to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Job {
    /// 1-based row number.
    pub(crate) row: usize,
    /// The row's values plus the built-in ones.
    pub(crate) values: Row,
    /// Where the PDF goes; `None` when all rows are merged into one file.
    pub(crate) file: Option<PathBuf>,
}

fn check_reserved(table: &Table) -> Result<()> {
    table
        .columns
        .iter()
        .find(|c| c.starts_with('_'))
        .map_or(Ok(()), |name| {
            Err(Error::ReservedColumn { name: name.clone() })
        })
}

fn check_names(what: &'static str, template: &Template, known: &[String]) -> Result<()> {
    let unknown: Vec<String> = template
        .names()
        .into_iter()
        .filter(|name| !known.iter().any(|k| k == name))
        .map(str::to_owned)
        .collect();
    match unknown.is_empty() {
        true => Ok(()),
        false => Err(Error::UnknownPlaceholders {
            what,
            names: unknown,
            known: known.to_vec(),
        }),
    }
}

fn file_name_problem(name: &str) -> Option<&'static str> {
    match name {
        "" => Some("it is empty"),
        "." | ".." => Some("it is not a file name"),
        _ if name.contains('/') => Some("it contains '/'"),
        _ if name.contains('\0') => Some("it contains a NUL character"),
        _ => None,
    }
}

fn check_duplicates(names: &[(usize, String)]) -> Result<()> {
    let by_name: BTreeMap<&str, Vec<usize>> =
        names
            .iter()
            .fold(BTreeMap::new(), |mut groups, (row, name)| {
                groups
                    .entry(name.as_str())
                    .or_insert_with(Vec::new)
                    .push(*row);
                groups
            });
    by_name
        .into_iter()
        .find(|(_, rows)| rows.len() > 1)
        .map_or(Ok(()), |(name, rows)| {
            Err(Error::DuplicateFileName {
                name: name.to_owned(),
                rows,
            })
        })
}

fn file_names(name: &Template, rows: &[(usize, Row)]) -> Result<Vec<(usize, String)>> {
    let names = rows
        .iter()
        .map(|(row, values)| {
            let rendered = name.render(values, Escape::None);
            file_name_problem(&rendered).map_or(Ok((*row, rendered.clone())), |reason| {
                Err(Error::InvalidFileName {
                    row: *row,
                    name: rendered,
                    reason,
                })
            })
        })
        .collect::<Result<Vec<_>>>()?;
    check_duplicates(&names)?;
    Ok(names)
}

/// Checks the inputs and lists the jobs. `name` is the output name pattern, or `None` when
/// all rows are merged into one file.
pub(crate) fn plan(
    template: &Template,
    name: Option<&Template>,
    table: &Table,
    dir: &Path,
) -> Result<Vec<Job>> {
    check_reserved(table)?;
    let known: Vec<String> = table
        .columns
        .iter()
        .cloned()
        .chain([ROW_VARIABLE.to_owned()])
        .collect();
    check_names("template", template, &known)?;
    name.map_or(Ok(()), |name| check_names("output name", name, &known))?;
    match table.rows.is_empty() {
        true => Err(Error::NoRows),
        false => Ok(()),
    }?;

    let rows: Vec<(usize, Row)> = table
        .rows
        .iter()
        .enumerate()
        .map(|(i, values)| {
            let row = i + 1;
            let mut values = values.clone();
            values.insert(ROW_VARIABLE.to_owned(), row.to_string());
            (row, values)
        })
        .collect();

    let files: Vec<Option<PathBuf>> = match name {
        Some(name) => file_names(name, &rows)?
            .into_iter()
            .map(|(_, file)| Some(dir.join(file)))
            .collect(),
        None => vec![None; rows.len()],
    };

    Ok(rows
        .into_iter()
        .zip(files)
        .map(|((row, values), file)| Job { row, values, file })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "{{surname}}_{{name}}{{#call}}_{{call}}{{/call}}.pdf";

    fn parse(text: &str) -> Template {
        Template::parse(text, "{{", "}}", "test").unwrap()
    }

    fn table(columns: &[&str], rows: &[&[&str]]) -> Table {
        Table {
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            rows: rows
                .iter()
                .map(|r| {
                    columns
                        .iter()
                        .zip(r.iter())
                        .map(|(c, v)| ((*c).to_owned(), (*v).to_owned()))
                        .collect()
                })
                .collect(),
        }
    }

    fn people() -> Table {
        table(
            &["name", "surname", "call"],
            &[&["A", "B", "X0TEST"], &["C", "D", ""]],
        )
    }

    #[test]
    fn plans_one_file_per_row() {
        let jobs = plan(
            &parse("{{name}} {{_row}}"),
            Some(&parse(NAME)),
            &people(),
            Path::new("out"),
        )
        .unwrap();
        let files: Vec<_> = jobs.iter().map(|j| j.file.clone().unwrap()).collect();
        assert_eq!(
            files,
            [
                PathBuf::from("out/B_A_X0TEST.pdf"),
                PathBuf::from("out/D_C.pdf")
            ]
        );
        assert_eq!(jobs[1].row, 2);
        assert_eq!(jobs[1].values["_row"], "2");
    }

    #[test]
    fn merge_mode_has_no_files_and_ignores_the_name() {
        let jobs = plan(&parse("{{name}}"), None, &people(), Path::new("out")).unwrap();
        assert!(jobs.iter().all(|j| j.file.is_none()));
    }

    #[test]
    fn unknown_placeholders_are_listed() {
        match plan(
            &parse("{{name}}{{x}}{{#y}}{{/y}}"),
            None,
            &people(),
            Path::new("o"),
        ) {
            Err(Error::UnknownPlaceholders { what, names, known }) => {
                assert_eq!(what, "template");
                assert_eq!(names, ["x", "y"]);
                assert!(known.contains(&"_row".to_owned()));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            plan(
                &parse("{{name}}"),
                Some(&parse("{{nope}}.pdf")),
                &people(),
                Path::new("o")
            ),
            Err(Error::UnknownPlaceholders {
                what: "output name",
                ..
            })
        ));
    }

    #[test]
    fn reserved_columns_and_empty_data_are_errors() {
        let reserved = table(&["_row"], &[&["1"]]);
        assert!(matches!(
            plan(&parse(""), None, &reserved, Path::new("o")),
            Err(Error::ReservedColumn { .. })
        ));
        let empty = table(&["name"], &[]);
        assert!(matches!(
            plan(&parse(""), None, &empty, Path::new("o")),
            Err(Error::NoRows)
        ));
    }

    #[test]
    fn invalid_file_names_are_errors() {
        let t = table(&["n"], &[&["ok"], &["a/b"]]);
        assert!(matches!(
            plan(&parse(""), Some(&parse("{{n}}")), &t, Path::new("o")),
            Err(Error::InvalidFileName { row: 2, .. })
        ));
        ["", ".", "..", "a\0b"]
            .iter()
            .for_each(|bad| assert!(file_name_problem(bad).is_some()));
        assert_eq!(file_name_problem("ANNA Erminia.pdf"), None);
    }

    #[test]
    fn duplicate_file_names_are_errors() {
        let t = table(&["n"], &[&["x"], &["y"], &["x"]]);
        match plan(&parse(""), Some(&parse("{{n}}.pdf")), &t, Path::new("o")) {
            Err(Error::DuplicateFileName { name, rows }) => {
                assert_eq!((name.as_str(), rows), ("x.pdf", vec![1, 3]));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
