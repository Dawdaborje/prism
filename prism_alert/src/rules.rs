//! The conditions an alert can watch for. Pure, so they can be tested without a database.

pub const CONDITIONS: &[&str] = &["above", "below", "rises_by", "falls_by", "is_empty"];

/// Seconds between checks.
pub fn interval_secs(frequency: &str) -> Option<u64> {
    match frequency {
        "hourly" => Some(3_600),
        "daily" => Some(86_400),
        "weekly" => Some(604_800),
        _ => None,
    }
}

/// Whether the alert is firing. `change` is a fraction (0.1 is ten per cent) against the previous
/// period; `Err` says what the chart must be given for this condition to be checked at all.
pub fn firing(condition: &str, limit: Option<f64>, value: Option<f64>, change: Option<f64>) -> Result<bool, String> {
    match condition {
        "is_empty" => Ok(value.is_none_or_zero()),
        "above" | "below" => {
            let limit = limit.ok_or("give the limit to compare with")?;
            let Some(value) = value else { return Ok(false) };
            Ok(if condition == "above" { value > limit } else { value < limit })
        }
        "rises_by" | "falls_by" => {
            let limit = limit.ok_or("give the percentage to compare with")?;
            let change = change.ok_or("turn on \"Compare with previous period\" (and set a date field) on the chart")?;
            let percent = change * 100.0;
            Ok(if condition == "rises_by" { percent > limit } else { -percent > limit })
        }
        other => Err(format!("`{other}` is not a condition")),
    }
}

trait EmptyOrZero {
    fn is_none_or_zero(&self) -> bool;
}

impl EmptyOrZero for Option<f64> {
    fn is_none_or_zero(&self) -> bool {
        self.map_or(true, |value| value == 0.0)
    }
}

/// Fill `{name}`, `{value}`, `{limit}` and `{change}` in a message.
pub fn fill(template: &str, name: &str, value: Option<f64>, limit: Option<f64>, change: Option<f64>) -> String {
    let number = |n: Option<f64>| n.map_or("none".to_string(), |n| format!("{}", (n * 100.0).round() / 100.0));
    let percent = change.map_or("n/a".to_string(), |c| format!("{}%", (c * 1000.0).round() / 10.0));
    template
        .replace("{name}", name)
        .replace("{value}", &number(value))
        .replace("{limit}", &number(limit))
        .replace("{change}", &percent)
}

/// The addresses in a comma or line separated list: at most ten, each plausible.
pub fn addresses(text: &str) -> Result<Vec<String>, String> {
    let list: Vec<String> = text
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    if list.len() > 10 {
        return Err("an alert emails at most 10 addresses".into());
    }
    for address in &list {
        let ok = address.split_once('@').is_some_and(|(user, host)| !user.is_empty() && host.contains('.') && !host.ends_with('.'));
        if !ok {
            return Err(format!("`{address}` is not an email address"));
        }
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn above_and_below_compare_with_the_limit() {
        assert_eq!(firing("above", Some(10.0), Some(11.0), None), Ok(true));
        assert_eq!(firing("above", Some(10.0), Some(10.0), None), Ok(false));
        assert_eq!(firing("below", Some(10.0), Some(9.0), None), Ok(true));
        assert_eq!(firing("below", Some(10.0), None, None), Ok(false));
    }

    #[test]
    fn a_missing_limit_is_said_in_words() {
        assert!(firing("above", None, Some(1.0), None).unwrap_err().contains("limit"));
    }

    #[test]
    fn rises_and_falls_are_percentages_of_the_previous_period() {
        assert_eq!(firing("rises_by", Some(10.0), Some(1.0), Some(0.25)), Ok(true));
        assert_eq!(firing("rises_by", Some(30.0), Some(1.0), Some(0.25)), Ok(false));
        assert_eq!(firing("falls_by", Some(20.0), Some(1.0), Some(-0.25)), Ok(true));
        assert_eq!(firing("falls_by", Some(20.0), Some(1.0), Some(0.25)), Ok(false));
        assert!(firing("falls_by", Some(20.0), Some(1.0), None).unwrap_err().contains("Compare"));
    }

    #[test]
    fn empty_means_no_figure_or_zero() {
        assert_eq!(firing("is_empty", None, None, None), Ok(true));
        assert_eq!(firing("is_empty", None, Some(0.0), None), Ok(true));
        assert_eq!(firing("is_empty", None, Some(3.0), None), Ok(false));
    }

    #[test]
    fn messages_fill_their_tokens() {
        let text = fill("{name}: {value} (limit {limit}, {change})", "Open deals", Some(12.0), Some(10.0), Some(0.2));
        assert_eq!(text, "Open deals: 12 (limit 10, 20%)");
    }

    #[test]
    fn addresses_are_checked_and_capped() {
        assert_eq!(addresses("a@b.co, c@d.org").unwrap().len(), 2);
        assert!(addresses("nope").unwrap_err().contains("not an email"));
        assert!(addresses(&vec!["a@b.co"; 11].join(",")).unwrap_err().contains("at most 10"));
        assert!(addresses("").unwrap().is_empty());
    }

    #[test]
    fn frequencies_have_an_interval() {
        assert_eq!(interval_secs("hourly"), Some(3_600));
        assert_eq!(interval_secs("never"), None);
    }
}
