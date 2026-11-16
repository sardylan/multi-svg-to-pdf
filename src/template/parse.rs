//! Pure parsing and rendering of templates with placeholders and sections.
//!
//! With the default delimiters:
//! - `{{name}}` is replaced with the value of `name`;
//! - `{{#name}}…{{/name}}` keeps its content only when `name` is not empty;
//! - `{{^name}}…{{/name}}` keeps its content only when `name` is empty.
//!
//! A value may pass through filters, applied left to right: `{{name|slug}}`, `{{name|upper}}`.
//! - `lower`: lowercase;
//! - `upper`: uppercase;
//! - `slug`: lowercase, then every character other than `a`-`z` and `0`-`9` becomes `_`.
//!
//! Sections nest. Spaces around a name and filters are ignored (`{{ name | slug }}`).

use std::collections::{BTreeMap, BTreeSet};

use crate::errors::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Value {
        name: String,
        filters: Vec<Filter>,
    },
    Section {
        name: String,
        inverted: bool,
        body: Vec<Part>,
    },
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Template {
    parts: Vec<Part>,
}

/// How values are written into the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Escape {
    /// For SVG: `&`, `<`, `>`, `"` and `'` become entities.
    Xml,
    /// For file names: values are written as they are.
    None,
}

/// A transformation applied to a value before it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Lower,
    Upper,
    Slug,
}

impl Filter {
    const NAMES: &'static str = "lower, upper, slug";

    fn parse(input: &str) -> Option<Filter> {
        match input {
            "lower" => Some(Filter::Lower),
            "upper" => Some(Filter::Upper),
            "slug" => Some(Filter::Slug),
            _ => None,
        }
    }

    fn apply(self, value: &str) -> String {
        match self {
            Filter::Lower => value.to_lowercase(),
            Filter::Upper => value.to_uppercase(),
            Filter::Slug => value
                .to_lowercase()
                .chars()
                .map(|c| match c {
                    'a'..='z' | '0'..='9' => c,
                    _ => '_',
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagKind {
    Value,
    Open { inverted: bool },
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    Text(&'a str),
    Tag {
        kind: TagKind,
        name: &'a str,
        filters: Vec<Filter>,
        line: usize,
    },
}

fn syntax(what: &'static str, line: usize, reason: impl Into<String>) -> Error {
    Error::TemplateSyntax {
        what,
        line,
        reason: reason.into(),
    }
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

fn parse_tag<'a>(inner: &'a str, what: &'static str, line: usize) -> Result<Token<'a>> {
    let inner = inner.trim();
    let (kind, name) = match inner.chars().next() {
        Some('#') => (TagKind::Open { inverted: false }, &inner[1..]),
        Some('^') => (TagKind::Open { inverted: true }, &inner[1..]),
        Some('/') => (TagKind::Close, &inner[1..]),
        _ => (TagKind::Value, inner),
    };
    let mut pieces = name.split('|').map(str::trim);
    // `split` always yields at least one piece.
    let name = pieces.next().unwrap_or_default();
    let filters = pieces
        .map(|filter| {
            Filter::parse(filter).ok_or_else(|| {
                syntax(
                    what,
                    line,
                    format!(
                        "unknown filter {filter:?}; known filters are {}",
                        Filter::NAMES
                    ),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    match (name.is_empty(), name.contains(char::is_whitespace)) {
        (true, _) => Err(syntax(what, line, "placeholder has no name")),
        (false, true) => Err(syntax(
            what,
            line,
            format!("placeholder name {name:?} contains spaces"),
        )),
        (false, false) if kind != TagKind::Value && !filters.is_empty() => Err(syntax(
            what,
            line,
            format!("section {name:?} cannot have filters"),
        )),
        (false, false) => Ok(Token::Tag {
            kind,
            name,
            filters,
            line,
        }),
    }
}

fn tokenize<'a>(
    text: &'a str,
    open: &str,
    close: &str,
    what: &'static str,
) -> Result<Vec<Token<'a>>> {
    // A cursor over the text is clearer than an iterator chain here.
    let mut tokens = Vec::new();
    let mut offset = 0;
    while let Some(start) = text[offset..].find(open).map(|i| offset + i) {
        let line = line_of(text, start);
        if start > offset {
            tokens.push(Token::Text(&text[offset..start]));
        }
        let inner_start = start + open.len();
        let inner_end = text[inner_start..]
            .find(close)
            .map(|i| inner_start + i)
            .ok_or_else(|| syntax(what, line, format!("{open} is never closed by {close}")))?;
        tokens.push(parse_tag(&text[inner_start..inner_end], what, line)?);
        offset = inner_end + close.len();
    }
    if offset < text.len() {
        tokens.push(Token::Text(&text[offset..]));
    }
    Ok(tokens)
}

/// Builds parts until the end of `tokens` or the close tag of `section`; returns the rest.
fn build<'t, 'a>(
    tokens: &'t [Token<'a>],
    section: Option<(&'a str, usize)>,
    what: &'static str,
) -> Result<(Vec<Part>, &'t [Token<'a>])> {
    let mut parts = Vec::new();
    let mut rest = tokens;
    while let Some((token, tail)) = rest.split_first() {
        rest = tail;
        match token {
            Token::Text(text) => parts.push(Part::Text((*text).to_owned())),
            Token::Tag {
                kind: TagKind::Value,
                name,
                filters,
                ..
            } => parts.push(Part::Value {
                name: (*name).to_owned(),
                filters: filters.clone(),
            }),
            &Token::Tag {
                kind: TagKind::Open { inverted },
                name,
                line,
                ..
            } => {
                let (body, after) = build(rest, Some((name, line)), what)?;
                parts.push(Part::Section {
                    name: name.to_owned(),
                    inverted,
                    body,
                });
                rest = after;
            }
            &Token::Tag {
                kind: TagKind::Close,
                name,
                line,
                ..
            } => {
                return match section {
                    Some((open, _)) if open == name => Ok((parts, rest)),
                    Some((open, _)) => Err(syntax(
                        what,
                        line,
                        format!("section {open:?} is closed by {name:?}"),
                    )),
                    None => Err(syntax(
                        what,
                        line,
                        format!("section {name:?} closed but never opened"),
                    )),
                };
            }
        }
    }
    match section {
        Some((name, line)) => Err(syntax(
            what,
            line,
            format!("section {name:?} is never closed"),
        )),
        None => Ok((parts, rest)),
    }
}

fn collect_names<'a>(parts: &'a [Part], names: BTreeSet<&'a str>) -> BTreeSet<&'a str> {
    parts.iter().fold(names, |mut names, part| match part {
        Part::Text(_) => names,
        Part::Value { name, .. } => {
            names.insert(name);
            names
        }
        Part::Section { name, body, .. } => {
            names.insert(name);
            collect_names(body, names)
        }
    })
}

fn render_parts(parts: &[Part], values: &BTreeMap<String, String>, escape: Escape) -> String {
    let value = |name: &str| values.get(name).map(String::as_str).unwrap_or_default();
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => text.clone(),
            Part::Value { name, filters } => {
                let filtered = filters
                    .iter()
                    .fold(value(name).to_owned(), |v, filter| filter.apply(&v));
                match escape {
                    Escape::Xml => escape_xml(&filtered),
                    Escape::None => filtered,
                }
            }
            Part::Section {
                name,
                inverted,
                body,
            } => match value(name).is_empty() == *inverted {
                true => render_parts(body, values, escape),
                false => String::new(),
            },
        })
        .collect()
}

/// Replaces the characters that are special in XML text and attribute values.
pub(crate) fn escape_xml(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

impl Template {
    /// Parses `text`. `what` names the template in error messages ("template", "output name").
    pub(crate) fn parse(
        text: &str,
        open: &str,
        close: &str,
        what: &'static str,
    ) -> Result<Template> {
        let tokens = tokenize(text, open, close, what)?;
        build(&tokens, None, what).map(|(parts, _)| Template { parts })
    }

    /// Every name used, in values and in sections.
    pub(crate) fn names(&self) -> BTreeSet<&str> {
        collect_names(&self.parts, BTreeSet::new())
    }

    /// Renders with `values`; a missing name counts as empty.
    pub(crate) fn render(&self, values: &BTreeMap<String, String>, escape: Escape) -> String {
        render_parts(&self.parts, values, escape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = include_str!("../../tests/fixtures/template/card.svg");

    fn parse(text: &str) -> Result<Template> {
        Template::parse(text, "{{", "}}", "template")
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn line_of_error(result: Result<Template>) -> usize {
        match result {
            Err(Error::TemplateSyntax { line, .. }) => line,
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn replaces_values() {
        let t = parse("Hello {{name}}, {{ surname }}!").unwrap();
        let out = t.render(
            &values(&[("name", "Ada"), ("surname", "Lovelace")]),
            Escape::None,
        );
        assert_eq!(out, "Hello Ada, Lovelace!");
    }

    #[test]
    fn text_without_placeholders_is_unchanged() {
        assert_eq!(
            parse("plain").unwrap().render(&values(&[]), Escape::Xml),
            "plain"
        );
        assert_eq!(parse("").unwrap().render(&values(&[]), Escape::Xml), "");
    }

    #[test]
    fn sections_follow_emptiness() {
        let t = parse("{{surname}}_{{name}}{{#call}}_{{call}}{{/call}}{{^call}}-none{{/call}}.pdf")
            .unwrap();
        let with = values(&[("name", "A"), ("surname", "B"), ("call", "X0ABC")]);
        let without = values(&[("name", "A"), ("surname", "B"), ("call", "")]);
        assert_eq!(t.render(&with, Escape::None), "B_A_X0ABC.pdf");
        assert_eq!(t.render(&without, Escape::None), "B_A-none.pdf");
    }

    #[test]
    fn sections_nest() {
        let t = parse("{{#a}}[{{#b}}{{b}}{{/b}}]{{/a}}").unwrap();
        assert_eq!(
            t.render(&values(&[("a", "1"), ("b", "2")]), Escape::None),
            "[2]"
        );
        assert_eq!(
            t.render(&values(&[("a", "1"), ("b", "")]), Escape::None),
            "[]"
        );
        assert_eq!(
            t.render(&values(&[("a", ""), ("b", "2")]), Escape::None),
            ""
        );
    }

    #[test]
    fn xml_escape_applies_to_values_only() {
        let t = parse("<text a=\"{{v}}\">{{v}}</text>").unwrap();
        let out = t.render(&values(&[("v", "R&D <\"x\"> 'y'")]), Escape::Xml);
        assert_eq!(
            out,
            "<text a=\"R&amp;D &lt;&quot;x&quot;&gt; &apos;y&apos;\">R&amp;D &lt;&quot;x&quot;&gt; &apos;y&apos;</text>"
        );
    }

    #[test]
    fn custom_delimiters() {
        let t = Template::parse("<%a%> and {{b}}", "<%", "%>", "template").unwrap();
        assert_eq!(t.names().into_iter().collect::<Vec<_>>(), ["a"]);
        assert_eq!(
            t.render(&values(&[("a", "x")]), Escape::None),
            "x and {{b}}"
        );
    }

    #[test]
    fn same_open_and_close_delimiter() {
        let t = Template::parse("%a% and %b%", "%", "%", "template").unwrap();
        assert_eq!(
            t.render(&values(&[("a", "1"), ("b", "2")]), Escape::None),
            "1 and 2"
        );
    }

    #[test]
    fn names_include_sections() {
        let t = parse("{{a}}{{#b}}{{c}}{{/b}}{{^d}}{{/d}}{{a}}").unwrap();
        assert_eq!(
            t.names().into_iter().collect::<Vec<_>>(),
            ["a", "b", "c", "d"]
        );
    }

    #[test]
    fn syntax_errors_report_the_line() {
        assert_eq!(line_of_error(parse("a\nb {{name")), 2);
        assert_eq!(line_of_error(parse("{{}}")), 1);
        assert_eq!(line_of_error(parse("{{a b}}")), 1);
        assert_eq!(line_of_error(parse("\n\n{{#a}}x")), 3);
        assert_eq!(line_of_error(parse("{{#a}}\n{{/b}}")), 2);
        assert_eq!(line_of_error(parse("x\n{{/a}}")), 2);
        assert_eq!(line_of_error(parse("{{#}}")), 1);
    }

    #[test]
    fn filters_transform_values() {
        let t = parse("{{surname|slug}}_{{ name | slug }}_{{call|lower}}_{{call|upper}}").unwrap();
        let v = values(&[
            ("surname", "ZETA'"),
            ("name", "ANNA Erminia"),
            ("call", "X0Abc"),
        ]);
        assert_eq!(t.render(&v, Escape::None), "zeta__anna_erminia_x0abc_X0ABC");
        assert_eq!(
            t.names().into_iter().collect::<Vec<_>>(),
            ["call", "name", "surname"]
        );
    }

    #[test]
    fn slug_turns_every_other_character_into_underscore() {
        assert_eq!(Filter::Slug.apply("Orrù-Già 2/b.é"), "orr__gi__2_b__");
        assert_eq!(Filter::Slug.apply(""), "");
    }

    #[test]
    fn filters_apply_left_to_right_before_escaping() {
        let t = parse("{{v|upper|lower}} {{v|slug|upper}}").unwrap();
        assert_eq!(
            t.render(&values(&[("v", "A&b")]), Escape::Xml),
            "a&amp;b A_B"
        );
    }

    #[test]
    fn filter_errors() {
        assert_eq!(line_of_error(parse("{{a|nope}}")), 1);
        assert_eq!(line_of_error(parse("{{a|}}")), 1);
        assert_eq!(line_of_error(parse("\n{{#a|slug}}{{/a}}")), 2);
        assert_eq!(line_of_error(parse("{{|slug}}")), 1);
    }

    #[test]
    fn fixture_card_has_the_expected_names() {
        let t = parse(CARD).unwrap();
        assert_eq!(
            t.names().into_iter().collect::<Vec<_>>(),
            ["call_lower", "call_upper", "name", "surname"]
        );
        let out = t.render(
            &values(&[
                ("name", "A&B"),
                ("surname", "C"),
                ("call_upper", "X0"),
                ("call_lower", ""),
            ]),
            Escape::Xml,
        );
        assert!(out.contains(">A&amp;B<") && out.contains(">X0<") && !out.contains("{{"));
    }
}
