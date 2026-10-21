//! Dates: the previous period of a named one, and the date range a chart bar stands for.

use chrono::{Datelike, Duration, NaiveDate};

/// The named period that comes right before `period`, when the report engine has a name for it.
pub fn previous_name(period: &str) -> Option<&'static str> {
    Some(match period {
        "today" => "yesterday",
        "this_week" => "last_week",
        "this_month" => "last_month",
        "this_quarter" => "last_quarter",
        "this_year" => "last_year",
        _ => return None,
    })
}

/// `[start, end)` of the stretch right before a "last N days" period, N days long.
pub fn previous_days(period: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let days: i64 = match period {
        "last_7_days" => 7,
        "last_30_days" => 30,
        "last_90_days" => 90,
        _ => return None,
    };
    let end = today - Duration::days(days - 1);
    Some((end - Duration::days(days), end))
}

/// The day a grain label (as `report::query` writes it) starts on, and the day after it ends.
///
/// `day` is `YYYY-MM-DD`, `week` is `YYYY-WW` (`%W`: Monday-first, days before the first Monday
/// are week 0), `month` is `YYYY-MM`, `quarter` is `YYYY-Qn` and `year` is `YYYY`.
pub fn grain_range(grain: &str, label: &str) -> Option<(NaiveDate, NaiveDate)> {
    let year = |text: &str| text.parse::<i32>().ok();
    match grain {
        "day" => {
            let day = NaiveDate::parse_from_str(label, "%Y-%m-%d").ok()?;
            Some((day, day + Duration::days(1)))
        }
        "week" => {
            let (y, w) = label.split_once('-')?;
            let (y, w) = (year(y)?, w.parse::<i64>().ok()?);
            let jan1 = NaiveDate::from_ymd_opt(y, 1, 1)?;
            let to_monday = (7 - jan1.weekday().num_days_from_monday() as i64) % 7;
            let first_monday = jan1 + Duration::days(to_monday);
            if w == 0 {
                Some((jan1, first_monday))
            } else {
                let start = first_monday + Duration::days((w - 1) * 7);
                Some((start, start + Duration::days(7)))
            }
        }
        "month" => {
            let (y, m) = label.split_once('-')?;
            let (y, m) = (year(y)?, m.parse::<u32>().ok()?);
            let start = NaiveDate::from_ymd_opt(y, m, 1)?;
            Some((start, add_months(start, 1)?))
        }
        "quarter" => {
            let (y, q) = label.split_once("-Q")?;
            let (y, q) = (year(y)?, q.parse::<u32>().ok()?);
            if !(1..=4).contains(&q) {
                return None;
            }
            let start = NaiveDate::from_ymd_opt(y, (q - 1) * 3 + 1, 1)?;
            Some((start, add_months(start, 3)?))
        }
        "year" => {
            let y = year(label)?;
            Some((NaiveDate::from_ymd_opt(y, 1, 1)?, NaiveDate::from_ymd_opt(y + 1, 1, 1)?))
        }
        _ => None,
    }
}

fn add_months(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    let index = date.year() * 12 + date.month0() as i32 + months as i32;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
}

/// A date as a field of `kind` (`date` or `datetime`) stores it.
pub fn bound(kind: &str, date: NaiveDate) -> String {
    if kind == "date" {
        date.format("%Y-%m-%d").to_string()
    } else {
        date.format("%Y-%m-%dT00:00:00Z").to_string()
    }
}

/// The finer grain that suits a period: days inside a month or less, months otherwise.
pub fn grain_for(period: &str) -> &'static str {
    match period {
        "today" | "this_week" | "last_7_days" | "this_month" | "last_30_days" => "day",
        _ => "month",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn named_periods_have_a_previous_one() {
        assert_eq!(previous_name("this_month"), Some("last_month"));
        assert_eq!(previous_name("last_30_days"), None);
        assert_eq!(previous_name("none"), None);
    }

    #[test]
    fn the_stretch_before_the_last_n_days_is_as_long() {
        // Last 7 days of 2026-03-10 are 03-04..=03-10, so the one before is 02-25..=03-03.
        assert_eq!(previous_days("last_7_days", d(2026, 3, 10)), Some((d(2026, 2, 25), d(2026, 3, 4))));
        assert_eq!(previous_days("this_month", d(2026, 3, 10)), None);
    }

    #[test]
    fn grain_labels_map_back_to_dates() {
        assert_eq!(grain_range("month", "2026-12"), Some((d(2026, 12, 1), d(2027, 1, 1))));
        assert_eq!(grain_range("quarter", "2026-Q3"), Some((d(2026, 7, 1), d(2026, 10, 1))));
        assert_eq!(grain_range("quarter", "2026-Q5"), None);
        assert_eq!(grain_range("year", "2026"), Some((d(2026, 1, 1), d(2027, 1, 1))));
        assert_eq!(grain_range("day", "2026-02-28"), Some((d(2026, 2, 28), d(2026, 3, 1))));
        assert_eq!(grain_range("month", "nonsense"), None);
    }

    #[test]
    fn weeks_follow_the_monday_first_numbering() {
        // 2026-01-01 is a Thursday: week 0 is Jan 1..Jan 4, week 1 starts Monday Jan 5.
        assert_eq!(grain_range("week", "2026-00"), Some((d(2026, 1, 1), d(2026, 1, 5))));
        assert_eq!(grain_range("week", "2026-01"), Some((d(2026, 1, 5), d(2026, 1, 12))));
        assert_eq!(grain_range("week", "2026-02"), Some((d(2026, 1, 12), d(2026, 1, 19))));
        // A year that starts on a Monday has no week 0 days.
        assert_eq!(grain_range("week", "2024-01"), Some((d(2024, 1, 1), d(2024, 1, 8))));
    }

    #[test]
    fn bounds_match_the_field_kind() {
        assert_eq!(bound("date", d(2026, 3, 1)), "2026-03-01");
        assert_eq!(bound("datetime", d(2026, 3, 1)), "2026-03-01T00:00:00Z");
    }

    #[test]
    fn short_periods_group_by_day() {
        assert_eq!(grain_for("this_month"), "day");
        assert_eq!(grain_for("this_year"), "month");
        assert_eq!(grain_for("none"), "month");
    }
}
