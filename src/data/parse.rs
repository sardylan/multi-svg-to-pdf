//! Pure parsing of CSV and JSON data into a [`Table`].

use std::collections::{BTreeMap, BTreeSet};

use crate::errors::{Error, Result};

/// One row: column name to value. Every row holds every column.
pub(crate) type Row = BTreeMap<String, String>;

/// The parsed data: column names (in file order for CSV, sorted for JSON) and rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Table {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Row>,
}

fn shape(row: Option<usize>, reason: impl Into<String>) -> Error {
    Error::DataShape {
        row,
        reason: reason.into(),
    }
}

fn check_columns(columns: &[String]) -> Result<()> {
    let empty = columns.iter().any(String::is_empty);
    let duplicate = columns
        .iter()
        .enumerate()
        .find(|(i, c)| columns[..*i].contains(c))
        .map(|(_, c)| c);
    match (empty, duplicate) {
        (true, _) => Err(shape(None, "a column name is empty")),
        (false, Some(name)) => Err(shape(None, format!("column {name:?} appears twice"))),
        (false, None) => Ok(()),
    }
}

/// Parses CSV with a header row. Line endings may be LF or CRLF; a UTF-8 BOM is ignored.
pub(crate) fn parse_csv(text: &str, delimiter: u8) -> Result<Table> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(text.as_bytes());
    let columns: Vec<String> = reader.headers()?.iter().map(str::to_owned).collect();
    check_columns(&columns)?;
    let rows = reader
        .records()
        .map(|record| {
            record.map(|record| {
                columns
                    .iter()
                    .cloned()
                    .zip(record.iter().map(str::to_owned))
                    .collect()
            })
        })
        .collect::<std::result::Result<Vec<Row>, csv::Error>>()?;
    Ok(Table { columns, rows })
}

fn json_value(row: usize, column: &str, value: &serde_json::Value) -> Result<String> {
    use serde_json::Value;
    match value {
        Value::Null => Ok(String::new()),
        Value::String(s) => Ok(s.clone()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Array(_) | Value::Object(_) => Err(shape(
            Some(row),
            format!("value of {column:?} is an array or object; only plain values are allowed"),
        )),
    }
}

/// Parses a JSON array of flat objects. A key missing from an object is an empty value.
pub(crate) fn parse_json(text: &str) -> Result<Table> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let items = value
        .as_array()
        .ok_or_else(|| shape(None, "JSON data must be an array of objects"))?;
    let objects = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            item.as_object()
                .ok_or_else(|| shape(Some(i + 1), "not a JSON object"))
        })
        .collect::<Result<Vec<_>>>()?;
    let columns: Vec<String> = objects
        .iter()
        .flat_map(|object| object.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    check_columns(&columns)?;
    let rows = objects
        .iter()
        .enumerate()
        .map(|(i, object)| {
            columns
                .iter()
                .map(|column| {
                    object
                        .get(column)
                        .map_or(Ok(String::new()), |v| json_value(i + 1, column, v))
                        .map(|value| (column.clone(), value))
                })
                .collect::<Result<Row>>()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Table { columns, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEOPLE_CSV: &str = include_str!("../../tests/fixtures/data/people.csv");
    const PEOPLE_JSON: &str = include_str!("../../tests/fixtures/data/people.json");

    #[test]
    fn csv_with_crlf_quotes_and_empty_values() {
        let table = parse_csv(PEOPLE_CSV, b',').unwrap();
        assert_eq!(
            table.columns,
            ["name", "surname", "call_upper", "call_lower"]
        );
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0]["call_upper"], "N0CALL");
        assert_eq!(table.rows[1]["call_upper"], "");
        assert_eq!(table.rows[2]["name"], "VERDI, JR");
        assert_eq!(table.rows[2]["surname"], "ANNA Erminia");
    }

    #[test]
    fn csv_other_delimiter_and_bom() {
        let table = parse_csv("\u{feff}a;b\n1;2\n", b';').unwrap();
        assert_eq!(table.columns, ["a", "b"]);
        assert_eq!(table.rows[0]["b"], "2");
    }

    #[test]
    fn csv_header_only_has_no_rows() {
        assert!(parse_csv("a,b\n", b',').unwrap().rows.is_empty());
    }

    #[test]
    fn csv_wrong_field_count_is_an_error() {
        assert!(matches!(
            parse_csv("a,b\n1,2,3\n", b','),
            Err(Error::Csv(_))
        ));
    }

    #[test]
    fn csv_bad_columns_are_errors() {
        assert!(matches!(
            parse_csv("a,a\n1,2\n", b','),
            Err(Error::DataShape { .. })
        ));
        assert!(matches!(
            parse_csv("a,\n1,2\n", b','),
            Err(Error::DataShape { .. })
        ));
    }

    #[test]
    fn json_plain_values_and_missing_keys() {
        let table = parse_json(PEOPLE_JSON).unwrap();
        assert_eq!(
            table.columns,
            ["age", "call_upper", "member", "name", "surname"]
        );
        assert_eq!(table.rows[0]["age"], "42");
        assert_eq!(table.rows[0]["member"], "true");
        assert_eq!(table.rows[1]["call_upper"], "");
        assert_eq!(table.rows[1]["age"], "");
    }

    #[test]
    fn json_shape_errors() {
        assert!(matches!(
            parse_json("{}"),
            Err(Error::DataShape { row: None, .. })
        ));
        assert!(matches!(
            parse_json("[1]"),
            Err(Error::DataShape { row: Some(1), .. })
        ));
        assert!(matches!(
            parse_json(r#"[{"a": "x"}, {"a": [1]}]"#),
            Err(Error::DataShape { row: Some(2), .. })
        ));
        assert!(matches!(parse_json("[{"), Err(Error::Json(_))));
        assert!(matches!(
            parse_json(r#"[{"": "x"}]"#),
            Err(Error::DataShape { .. })
        ));
    }

    #[test]
    fn json_empty_array_has_no_rows() {
        let table = parse_json("[]").unwrap();
        assert!(table.columns.is_empty() && table.rows.is_empty());
    }
}
