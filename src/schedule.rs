//! The trigger schedule vocabulary and its compilation to cron expressions.

use chrono::{DateTime, Utc, Weekday};
use core::str::FromStr;
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
    Hourly {
        minute: u8,
    },
    Daily {
        at: TimeOfDay,
    },
    Weekly {
        day: Weekday,
        at: TimeOfDay,
    },
    Monthly {
        day: u8,
        at: TimeOfDay,
    },
    Yearly {
        month: u8,
        day: u8,
        at: TimeOfDay,
    },
    OnceAt {
        datetime: chrono::DateTime<chrono::Utc>,
    },
    Cron {
        expr: String,
    },
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

    /// The next instant STRICTLY AFTER `after` at which this schedule fires,
    /// or `None` if it never fires again (past `OnceAt`, or unparseable `Cron`).
    /// Pure: the reference instant is a parameter, so this is deterministic and
    /// clock-free for testing.
    pub fn next_fire(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            TriggerSchedule::OnceAt { datetime } => {
                if *datetime > after {
                    Some(*datetime)
                } else {
                    None
                }
            }
            other => {
                let expr = other.to_cron()?;
                let schedule = cron::Schedule::from_str(&expr).ok()?;
                schedule.after(&after).next()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tod(hour: u8, minute: u8) -> TimeOfDay {
        TimeOfDay { hour, minute }
    }

    #[test]
    fn every_minute_compiles_to_wildcard_minute() {
        assert_eq!(
            TriggerSchedule::EveryMinute.to_cron().unwrap(),
            "0 * * * * *"
        );
    }

    #[test]
    fn hourly_compiles_to_fixed_minute() {
        assert_eq!(
            TriggerSchedule::Hourly { minute: 30 }.to_cron().unwrap(),
            "0 30 * * * *"
        );
    }

    #[test]
    fn daily_compiles_to_fixed_hour_minute() {
        assert_eq!(
            TriggerSchedule::Daily { at: tod(6, 0) }.to_cron().unwrap(),
            "0 0 6 * * *"
        );
    }

    #[test]
    fn monthly_compiles_with_day_of_month() {
        assert_eq!(
            TriggerSchedule::Monthly {
                day: 15,
                at: tod(9, 5)
            }
            .to_cron()
            .unwrap(),
            "0 5 9 15 * *"
        );
    }

    #[test]
    fn yearly_compiles_with_month_and_day() {
        assert_eq!(
            TriggerSchedule::Yearly {
                month: 1,
                day: 1,
                at: tod(0, 0)
            }
            .to_cron()
            .unwrap(),
            "0 0 0 1 1 *"
        );
    }

    #[test]
    fn once_at_has_no_cron() {
        assert_eq!(
            TriggerSchedule::OnceAt {
                datetime: chrono::DateTime::UNIX_EPOCH
            }
            .to_cron(),
            None
        );
    }

    #[test]
    fn cron_escape_hatch_passes_through() {
        assert_eq!(
            TriggerSchedule::Cron {
                expr: "0 0 12 * * *".into()
            }
            .to_cron()
            .unwrap(),
            "0 0 12 * * *"
        );
    }

    #[test]
    fn weekly_compiles_with_day_of_week() {
        let expr = TriggerSchedule::Weekly {
            day: Weekday::Mon,
            at: tod(8, 0),
        }
        .to_cron()
        .unwrap();
        assert!(expr.starts_with("0 0 8 * * "), "{expr}");
    }

    use chrono::{DateTime, Datelike, TimeZone, Utc};

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn daily_next_fire_is_today_or_tomorrow() {
        let s = TriggerSchedule::Daily { at: tod(6, 0) };
        assert_eq!(
            s.next_fire(at(2026, 6, 7, 5, 0)).unwrap(),
            at(2026, 6, 7, 6, 0)
        );
        assert_eq!(
            s.next_fire(at(2026, 6, 7, 7, 0)).unwrap(),
            at(2026, 6, 8, 6, 0)
        );
    }

    #[test]
    fn every_minute_advances_one_minute() {
        let s = TriggerSchedule::EveryMinute;
        assert_eq!(
            s.next_fire(at(2026, 6, 7, 5, 0)).unwrap(),
            at(2026, 6, 7, 5, 1)
        );
    }

    #[test]
    fn weekly_lands_on_the_right_weekday() {
        // 2026-06-07 is a Sunday; next Monday 08:00 is 2026-06-08.
        let s = TriggerSchedule::Weekly {
            day: chrono::Weekday::Mon,
            at: tod(8, 0),
        };
        let next = s.next_fire(at(2026, 6, 7, 0, 0)).unwrap();
        assert_eq!(next.weekday(), chrono::Weekday::Mon);
        assert_eq!(next, at(2026, 6, 8, 8, 0));
    }

    #[test]
    fn monthly_day_31_skips_short_months() {
        // cron does not roll over; day 31 simply does not occur in June.
        // From June 1, the next 31st is July 31.
        let s = TriggerSchedule::Monthly {
            day: 31,
            at: tod(0, 0),
        };
        let next = s.next_fire(at(2026, 6, 1, 0, 0)).unwrap();
        assert_eq!(next, at(2026, 7, 31, 0, 0));
    }

    #[test]
    fn once_at_fires_once_then_never() {
        let when = at(2026, 6, 7, 12, 0);
        let s = TriggerSchedule::OnceAt { datetime: when };
        assert_eq!(s.next_fire(at(2026, 6, 7, 11, 0)), Some(when));
        assert_eq!(s.next_fire(at(2026, 6, 7, 12, 0)), None); // not strictly after
        assert_eq!(s.next_fire(at(2026, 6, 7, 13, 0)), None);
    }

    #[test]
    fn yearly_leap_day_only_in_leap_years() {
        let s = TriggerSchedule::Yearly {
            month: 2,
            day: 29,
            at: tod(0, 0),
        };
        // From 2026 (non-leap), next Feb 29 is 2028.
        let next = s.next_fire(at(2026, 3, 1, 0, 0)).unwrap();
        assert_eq!(next, at(2028, 2, 29, 0, 0));
    }

    #[test]
    fn bad_cron_yields_none() {
        let s = TriggerSchedule::Cron {
            expr: "not a cron".into(),
        };
        assert_eq!(s.next_fire(at(2026, 6, 7, 0, 0)), None);
    }

    #[test]
    fn cron_escape_hatch_computes_next_fire() {
        let s = TriggerSchedule::Cron {
            expr: "0 0 12 * * *".into(),
        };
        assert_eq!(
            s.next_fire(at(2026, 6, 7, 11, 0)).unwrap(),
            at(2026, 6, 7, 12, 0)
        );
    }
}
