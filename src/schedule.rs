//! The trigger schedule vocabulary and its compilation to cron expressions.

use chrono::Weekday;
use serde::{Deserialize, Serialize};

/// Time of day in UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDay {
    pub hour: u8,
    pub minute: u8,
}

fn weekday_cron(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MON",
        Weekday::Tue => "TUE",
        Weekday::Wed => "WED",
        Weekday::Thu => "THU",
        Weekday::Fri => "FRI",
        Weekday::Sat => "SAT",
        Weekday::Sun => "SUN",
    }
}

/// A recurrence schedule, a one-shot, or a raw cron escape hatch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerSchedule {
    EveryMinute,
    Hourly { minute: u8 },
    Daily { at: TimeOfDay },
    Weekly { day: Weekday, at: TimeOfDay },
    Monthly { day: u8, at: TimeOfDay },
    Yearly { month: u8, day: u8, at: TimeOfDay },
    OnceAt { datetime: chrono::DateTime<chrono::Utc> },
    Cron { expr: String },
}

impl TriggerSchedule {
    /// Compile a recurring schedule to a 6-field cron expression
    /// (`sec min hour day-of-month month day-of-week`). Returns `None` for `OnceAt`.
    pub fn to_cron(&self) -> Option<String> {
        use TriggerSchedule::*;
        Some(match self {
            EveryMinute => "0 * * * * *".to_string(),
            Hourly { minute } => format!("0 {minute} * * * *"),
            Daily { at } => format!("0 {} {} * * *", at.minute, at.hour),
            Weekly { day, at } => format!("0 {} {} * * {}", at.minute, at.hour, weekday_cron(*day)),
            Monthly { day, at } => format!("0 {} {} {} * *", at.minute, at.hour, day),
            Yearly { month, day, at } => format!("0 {} {} {} {} *", at.minute, at.hour, day, month),
            OnceAt { .. } => return None,
            Cron { expr } => expr.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tod(hour: u8, minute: u8) -> TimeOfDay { TimeOfDay { hour, minute } }

    #[test]
    fn every_minute_compiles_to_wildcard_minute() {
        assert_eq!(TriggerSchedule::EveryMinute.to_cron().unwrap(), "0 * * * * *");
    }

    #[test]
    fn hourly_compiles_to_fixed_minute() {
        assert_eq!(TriggerSchedule::Hourly { minute: 30 }.to_cron().unwrap(), "0 30 * * * *");
    }

    #[test]
    fn daily_compiles_to_fixed_hour_minute() {
        assert_eq!(TriggerSchedule::Daily { at: tod(6, 0) }.to_cron().unwrap(), "0 0 6 * * *");
    }

    #[test]
    fn monthly_compiles_with_day_of_month() {
        assert_eq!(
            TriggerSchedule::Monthly { day: 15, at: tod(9, 5) }.to_cron().unwrap(),
            "0 5 9 15 * *"
        );
    }

    #[test]
    fn yearly_compiles_with_month_and_day() {
        assert_eq!(
            TriggerSchedule::Yearly { month: 1, day: 1, at: tod(0, 0) }.to_cron().unwrap(),
            "0 0 0 1 1 *"
        );
    }

    #[test]
    fn once_at_has_no_cron() {
        assert_eq!(
            TriggerSchedule::OnceAt { datetime: chrono::DateTime::UNIX_EPOCH }.to_cron(),
            None
        );
    }

    #[test]
    fn cron_escape_hatch_passes_through() {
        assert_eq!(
            TriggerSchedule::Cron { expr: "0 0 12 * * *".into() }.to_cron().unwrap(),
            "0 0 12 * * *"
        );
    }

    #[test]
    fn weekly_compiles_with_day_of_week() {
        let expr = TriggerSchedule::Weekly { day: Weekday::Mon, at: tod(8, 0) }.to_cron().unwrap();
        assert!(expr.starts_with("0 0 8 * * "), "{expr}");
    }
}
