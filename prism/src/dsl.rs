//! The small text a chart is written in, turned into a `report::query` payload.
//!
//! Dimensions are a comma list: `stage, expected_close:month`.
//!
//! Measures are one per line (or separated by `;`), `name = expression`:
//!
//! ```text
//! deals   = count
//! value   = sum(expected_value)
//! won     = count_if({"closed_at": {"null": false}})
//! won_val = sum_if(expected_value, {"closed_at": {"null": false}})
//! slip    = avg_days(expected_close -> now)
//! win_pct = ratio(won, deals)
//! growth  = change(value)
//! profit  = metric(gross_profit)
//! ```
//!
//! A line without `name =` is named after its expression (`count` is `n`, `sum(amount)` is
//! `sum_amount`). `metric(code)` stands for the measure a saved metric holds.

use serde_json::{json, Map, Value};

use aether_sdk::{Error, Result};

pub const GRAINS: &[&str] = &["day", "week", "month", "quarter", "year"];

/// What the text of a chart means.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    /// Measures in the order written, as `report::query` takes them.
    pub measures: Map<String, Value>,
    /// Derived columns (`ratio`, `delta`, `share`, `prev`, `change`).
    pub formulas: Map<String, Value>,
    /// Every result name in the order written (measures and formulas).
    pub names: Vec<String>,
}

impl Parsed {
    /// The names a card shows. A name starting with `_` is a helper for a formula, not a figure.
    pub fn visible(&self) -> Vec<&str> {
        let shown: Vec<&str> = self.names.iter().map(String::as_str).filter(|name| !name.starts_with('_')).collect();
        if shown.is_empty() {
            self.names.iter().map(String::as_str).collect()
        } else {
            shown
        }
    }
}

/// A `report::query` dimension for each entry of `text`.
pub fn parse_dimensions(text: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for part in text.split(',').map(str::trim).filter(|part| !part.is_empty()) {
        let (field, grain) = match part.split_once(':') {
            Some((field, grain)) => (field.trim(), Some(grain.trim())),
            None => (part, None),
        };
        ident(field, "dimension")?;
        match grain {
            None => out.push(json!(field)),
            Some(grain) if GRAINS.contains(&grain) => out.push(json!({ "field": field, "grain": grain })),
            Some(grain) => {
                return Err(Error::msg(format!(
                    "`{grain}` is not a date grain; use day, week, month, quarter or year"
                )))
            }
        }
    }
    if out.len() > 4 {
        return Err(Error::msg("a chart groups by at most 4 fields"));
    }
    Ok(out)
}

/// The field name of a dimension as `parse_dimensions` returned it.
pub fn dimension_field(dimension: &Value) -> &str {
    match dimension {
        Value::String(field) => field,
        other => other.get("field").and_then(Value::as_str).unwrap_or(""),
    }
}

/// Parse the measure lines. `lookup` answers `metric(code)` with the text of that metric's
/// single measure expression.
pub fn parse_measures(text: &str, lookup: &dyn Fn(&str) -> Result<String>) -> Result<Parsed> {
    let mut parsed = Parsed::default();
    for line in text.split(['\n', ';']).map(str::trim).filter(|line| !line.is_empty()) {
        let (name, expression) = match split_name(line) {
            Some((name, expression)) => (Some(name), expression),
            None => (None, line),
        };
        let (function, args) = call(expression)?;
        let name = match name {
            Some(name) => name.to_string(),
            None => default_name(function, &args),
        };
        ident(&name, "measure name")?;
        if parsed.names.contains(&name) {
            return Err(Error::msg(format!("`{name}` is written twice")));
        }
        match function {
            "ratio" | "delta" => {
                let [a, b] = pair(function, &args)?;
                parsed.formulas.insert(name.clone(), json!({ function: [a, b] }));
            }
            "share" | "prev" | "change" => {
                let [one] = single(function, &args)?;
                parsed.formulas.insert(name.clone(), json!({ function: one }));
            }
            "metric" => {
                let [code] = single(function, &args)?;
                let inner = lookup(&code)?;
                let (function, args) = call(&inner)?;
                parsed.measures.insert(name.clone(), measure(function, &args)?);
            }
            other => {
                parsed.measures.insert(name.clone(), measure(other, &args)?);
            }
        }
        parsed.names.push(name);
    }
    if parsed.measures.is_empty() {
        return Err(Error::msg("a chart needs at least one measure, such as `count` or `sum(amount)`"));
    }
    for (name, formula) in &parsed.formulas {
        for source in formula_sources(formula) {
            if !parsed.measures.contains_key(&source) {
                return Err(Error::msg(format!("`{name}` uses `{source}`, which is not a measure of this chart")));
            }
        }
    }
    Ok(parsed)
}

fn formula_sources(formula: &Value) -> Vec<String> {
    let Some((_, body)) = formula.as_object().and_then(|object| object.iter().next()) else {
        return Vec::new();
    };
    match body {
        Value::Array(list) => list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Value::String(one) => vec![one.clone()],
        _ => Vec::new(),
    }
}

fn measure(function: &str, args: &[String]) -> Result<Value> {
    Ok(match function {
        "count" => json!("count"),
        "sum" | "avg" | "min" | "max" => {
            let [field] = single(function, args)?;
            ident(&field, "field")?;
            json!({ function: field })
        }
        "count_if" => {
            let [filter] = single(function, args)?;
            json!({ "count_if": json_object(function, &filter)? })
        }
        "sum_if" | "avg_if" => {
            let [field, filter] = pair(function, args)?;
            ident(&field, "field")?;
            let key = if function == "sum_if" { "sum_if" } else { "avg_if" };
            json!({ key: { "field": field, "where": json_object(function, &filter)? } })
        }
        "avg_days" => {
            let [range] = single(function, args)?;
            let (from, to) = range
                .split_once("->")
                .ok_or_else(|| Error::msg("avg_days is written avg_days(from_field -> to_field)"))?;
            let (from, to) = (from.trim(), to.trim());
            ident(from, "field")?;
            if to != "now" {
                ident(to, "field")?;
            }
            json!({ "avg_days": { "from": from, "to": to } })
        }
        other => {
            return Err(Error::msg(format!(
                "`{other}` is not a measure; use count, sum, avg, min, max, count_if, sum_if, avg_if, avg_days, ratio, delta, share, prev, change or metric"
            )))
        }
    })
}

fn json_object(function: &str, text: &str) -> Result<Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(value @ Value::Object(_)) => Ok(value),
        _ => Err(Error::msg(format!("{function} takes a filter written as JSON, such as {{\"state\": \"won\"}}"))),
    }
}

fn single(function: &str, args: &[String]) -> Result<[String; 1]> {
    match args {
        [one] => Ok([one.clone()]),
        _ => Err(Error::msg(format!("{function} takes one argument"))),
    }
}

fn pair(function: &str, args: &[String]) -> Result<[String; 2]> {
    match args {
        [a, b] => Ok([a.clone(), b.clone()]),
        _ => Err(Error::msg(format!("{function} takes two arguments"))),
    }
}

/// `name = expression` when the line starts with a name and `=` (not `==` or inside a call).
fn split_name(line: &str) -> Option<(&str, &str)> {
    let (name, rest) = line.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name, rest.trim()))
}

fn default_name(function: &str, args: &[String]) -> String {
    match (function, args.first()) {
        ("count", _) => "n".to_string(),
        (function, Some(first)) => {
            let first = first.split("->").next().unwrap_or(first).trim();
            let clean: String = first.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            format!("{function}_{clean}")
        }
        (function, None) => function.to_string(),
    }
}

/// Split `fn(a, b)` into `fn` and its arguments, honouring nested braces and quotes. A bare word
/// (`count`) is a call with no arguments.
fn call(expression: &str) -> Result<(&str, Vec<String>)> {
    let expression = expression.trim();
    let Some(open) = expression.find('(') else {
        ident(expression, "measure")?;
        return Ok((expression, Vec::new()));
    };
    if !expression.ends_with(')') {
        return Err(Error::msg(format!("`{expression}` is missing its closing bracket")));
    }
    let function = expression[..open].trim();
    ident(function, "measure")?;
    let inner = &expression[open + 1..expression.len() - 1];
    Ok((function, split_arguments(inner)))
}

fn split_arguments(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut quoted, mut escaped) = (0i32, false, false);
    let mut current = String::new();
    for c in inner.chars() {
        if quoted {
            current.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
            continue;
        }
        match c {
            '"' => {
                quoted = true;
                current.push(c);
            }
            '{' | '[' | '(' => {
                depth += 1;
                current.push(c);
            }
            '}' | ']' | ')' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() || !out.is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn ident(text: &str, what: &str) -> Result<()> {
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(Error::msg(format!("`{text}` is not a valid {what}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(code: &str) -> Result<String> {
        Err(Error::msg(format!("no metric `{code}`")))
    }

    #[test]
    fn dimensions_take_a_grain() {
        assert_eq!(
            parse_dimensions("stage, expected_close:month").unwrap(),
            vec![json!("stage"), json!({ "field": "expected_close", "grain": "month" })]
        );
        assert!(parse_dimensions("when:fortnight").is_err());
        assert!(parse_dimensions("a b").is_err());
        assert!(parse_dimensions("").unwrap().is_empty());
    }

    #[test]
    fn measures_are_named_after_their_line_or_expression() {
        let parsed = parse_measures("count\nsum(amount)\ntotal = avg(amount)", &none).unwrap();
        assert_eq!(parsed.names, vec!["n", "sum_amount", "total"]);
        assert_eq!(parsed.measures["n"], json!("count"));
        assert_eq!(parsed.measures["total"], json!({ "avg": "amount" }));
    }

    #[test]
    fn filters_inside_a_measure_keep_their_commas() {
        let parsed = parse_measures(
            r#"won = sum_if(amount, {"stage": {"in": ["won", "closed"]}, "open": false}); n = count"#,
            &none,
        )
        .unwrap();
        assert_eq!(
            parsed.measures["won"],
            json!({ "sum_if": { "field": "amount", "where": { "stage": { "in": ["won", "closed"] }, "open": false } } })
        );
        assert_eq!(parsed.names, vec!["won", "n"]);
    }

    #[test]
    fn formulas_must_name_measures_of_the_chart() {
        let parsed = parse_measures("n = count\nwon = count_if({\"won\": true})\nrate = ratio(won, n)", &none).unwrap();
        assert_eq!(parsed.formulas["rate"], json!({ "ratio": ["won", "n"] }));
        assert!(parse_measures("n = count\nrate = ratio(n, ghost)", &none).is_err());
    }

    #[test]
    fn helpers_are_hidden_from_the_card() {
        let parsed = parse_measures("_won = count_if({\"won\": true})\n_all = count\nrate = ratio(_won, _all)", &none).unwrap();
        assert_eq!(parsed.visible(), vec!["rate"]);
        let only_helpers = parse_measures("_n = count", &none).unwrap();
        assert_eq!(only_helpers.visible(), vec!["_n"]);
    }

    #[test]
    fn avg_days_reads_an_arrow_and_now() {
        let parsed = parse_measures("slip = avg_days(expected_close -> now)", &none).unwrap();
        assert_eq!(parsed.measures["slip"], json!({ "avg_days": { "from": "expected_close", "to": "now" } }));
        assert!(parse_measures("x = avg_days(expected_close)", &none).is_err());
    }

    #[test]
    fn a_metric_stands_for_its_saved_measure() {
        let lookup = |code: &str| -> Result<String> {
            if code == "won_value" {
                Ok(r#"sum_if(amount, {"won": true})"#.to_string())
            } else {
                none(code)
            }
        };
        let parsed = parse_measures("v = metric(won_value)", &lookup).unwrap();
        assert_eq!(parsed.measures["v"], json!({ "sum_if": { "field": "amount", "where": { "won": true } } }));
        assert!(parse_measures("v = metric(ghost)", &lookup).is_err());
    }

    #[test]
    fn mistakes_are_reported_in_plain_words() {
        assert!(parse_measures("", &none).is_err());
        assert!(parse_measures("n = median(amount)", &none).is_err());
        assert!(parse_measures("n = sum(amount", &none).is_err());
        assert!(parse_measures("n = count\nn = count", &none).is_err());
        assert!(parse_measures("n = count_if(not json)", &none).is_err());
    }
}
