//! `TriggerDef`: a schedule bound to the business event it emits, plus
//! validation and the fire -> business-event helper.

use chrono::{DateTime, Utc};
use core::str::FromStr;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use greentic_types::events::BusinessEventBuilder;
use greentic_types::{EventEnvelope, TenantCtx};

use crate::schedule::{TimeOfDay, TriggerSchedule};

/// A schedule bound to the business event it emits when it fires.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriggerDef {
    /// Stable, slug-safe identifier, unique within a pack.
    pub id: String,
    /// When it fires.
    pub schedule: TriggerSchedule,
    /// Business-event type to emit: `cap://greentic/events/{domain}/{name}`
    /// or the dotted `{domain}.{name}` form.
    pub emits: String,
    /// JSON payload for the emitted event. `{{fire_time}}` string tokens are
    /// substituted with the RFC 3339 fire instant at emit time.
    #[serde(default)]
    pub payload_template: Value,
}

/// Validate a trigger, returning EVERY violation (accumulator) so an authoring
/// loop can surface them all at once.
pub fn validate_trigger(def: &TriggerDef) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if def.id.trim().is_empty() {
        errors.push("id must be non-empty".to_string());
    } else if !def
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        errors.push(format!(
            "id '{}' must be slug-safe (alnum, '_' or '-')",
            def.id
        ));
    }

    validate_schedule(&def.schedule, &mut errors);

    let cap = to_cap_uri(&def.emits);
    if let Err(err) = greentic_types::events::parse_business_event_type(&cap) {
        errors.push(format!(
            "emits '{}' is not a valid business-event type: {err}",
            def.emits
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_schedule(schedule: &TriggerSchedule, errors: &mut Vec<String>) {
    let check_tod = |at: &TimeOfDay, errors: &mut Vec<String>| {
        if at.hour > 23 {
            errors.push(format!("hour {} out of range 0-23", at.hour));
        }
        if at.minute > 59 {
            errors.push(format!("minute {} out of range 0-59", at.minute));
        }
    };
    match schedule {
        TriggerSchedule::EveryMinute | TriggerSchedule::OnceAt { .. } => {}
        TriggerSchedule::Hourly { minute } => {
            if *minute > 59 {
                errors.push(format!("minute {minute} out of range 0-59"));
            }
        }
        TriggerSchedule::Daily { at } | TriggerSchedule::Weekly { at, .. } => check_tod(at, errors),
        TriggerSchedule::Monthly { day, at } => {
            if !(1..=31).contains(day) {
                errors.push(format!("monthly day {day} out of range 1-31"));
            }
            check_tod(at, errors);
        }
        TriggerSchedule::Yearly { month, day, at } => {
            if !(1..=12).contains(month) {
                errors.push(format!("month {month} out of range 1-12"));
            }
            if !(1..=31).contains(day) {
                errors.push(format!("yearly day {day} out of range 1-31"));
            }
            check_tod(at, errors);
        }
        TriggerSchedule::Cron { expr } => {
            if cron::Schedule::from_str(expr).is_err() {
                errors.push(format!("cron expression '{expr}' is not parseable"));
            }
        }
    }
}

/// Lift a dotted `{domain}.{name}` topic to a cap URI; pass cap URIs through.
pub(crate) fn to_cap_uri(emits: &str) -> String {
    if emits.starts_with("cap://greentic/events/") {
        emits.to_string()
    } else if let Some((domain, name)) = emits.split_once('.') {
        format!("cap://greentic/events/{domain}/{name}")
    } else {
        emits.to_string()
    }
}

/// Build the business-event envelope for a fired trigger. `{{fire_time}}` string
/// tokens in the payload template are replaced with the RFC 3339 fire instant.
pub fn business_event_for_fire(
    def: &TriggerDef,
    fire_time: DateTime<Utc>,
    tenant: &TenantCtx,
) -> Result<EventEnvelope, Vec<String>> {
    let cap = to_cap_uri(&def.emits);
    let (domain, name) = greentic_types::events::parse_business_event_type(&cap)
        .map_err(|e| vec![format!("emits '{}' invalid: {e}", def.emits)])?;
    let payload = substitute_fire_time(&def.payload_template, &fire_time.to_rfc3339());
    let event_id = format!("{}-{}", def.id, fire_time.timestamp_millis());

    BusinessEventBuilder::new(domain, name, tenant.clone())
        .producer(format!("trigger:{}", def.id))
        .schema_version("1")
        .payload(payload)
        .time(fire_time)
        .id(event_id)
        .build()
}

/// Recursively replace any string exactly equal to `{{fire_time}}` with `value`.
fn substitute_fire_time(template: &Value, value: &str) -> Value {
    match template {
        Value::String(s) if s == "{{fire_time}}" => Value::String(value.to_string()),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|i| substitute_fire_time(i, value))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), substitute_fire_time(v, value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{TimeOfDay, TriggerSchedule};

    fn test_tenant() -> greentic_types::TenantCtx {
        let env = greentic_types::EnvId::try_from("prod").expect("env");
        let tenant = greentic_types::TenantId::try_from("acme").expect("tenant");
        greentic_types::TenantCtx::new(env, tenant)
    }

    #[test]
    fn business_event_for_fire_builds_envelope() {
        use chrono::TimeZone;
        let def = TriggerDef {
            id: "daily_rent_reminder".into(),
            schedule: TriggerSchedule::Daily {
                at: TimeOfDay { hour: 6, minute: 0 },
            },
            emits: "tenancy.daily-rent-reminder".into(),
            payload_template: serde_json::json!({ "msg": "due", "at": "{{fire_time}}" }),
        };
        let fire = chrono::Utc.with_ymd_and_hms(2026, 7, 24, 6, 0, 0).unwrap();
        let env = business_event_for_fire(&def, fire, &test_tenant()).expect("build");
        assert_eq!(
            env.r#type,
            "cap://greentic/events/tenancy/daily-rent-reminder"
        );
        assert_eq!(env.topic, "tenancy");
        assert_eq!(env.time, fire);
        assert_eq!(
            env.metadata.get("producer").map(String::as_str),
            Some("trigger:daily_rent_reminder")
        );
        assert_eq!(
            env.payload["at"],
            serde_json::Value::String(fire.to_rfc3339())
        );
        assert_eq!(env.payload["msg"], "due");
    }

    #[test]
    fn trigger_def_roundtrips_json() {
        let def = TriggerDef {
            id: "daily_rent_reminder".to_string(),
            schedule: TriggerSchedule::Daily {
                at: TimeOfDay { hour: 6, minute: 0 },
            },
            emits: "cap://greentic/events/tenancy/daily-rent-reminder".to_string(),
            payload_template: serde_json::json!({ "fired_at": "{{fire_time}}" }),
        };
        let json = serde_json::to_value(&def).expect("serialize");
        assert_eq!(json["schedule"]["kind"], "daily");
        assert_eq!(json["schedule"]["at"]["hour"], 6);
        let back: TriggerDef = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, def);
    }

    #[test]
    fn validate_trigger_accepts_valid() {
        let def = TriggerDef {
            id: "daily_rent_reminder".into(),
            schedule: TriggerSchedule::Daily {
                at: TimeOfDay { hour: 6, minute: 0 },
            },
            emits: "cap://greentic/events/tenancy/daily-rent-reminder".into(),
            payload_template: serde_json::json!({}),
        };
        assert!(validate_trigger(&def).is_ok());
    }

    #[test]
    fn validate_trigger_accumulates_all_violations() {
        let def = TriggerDef {
            id: "".into(),
            schedule: TriggerSchedule::Hourly { minute: 99 },
            emits: "not-a-cap".into(),
            payload_template: serde_json::json!({}),
        };
        let errs = validate_trigger(&def).unwrap_err();
        assert!(
            errs.len() >= 3,
            "expected id + schedule + emits errors, got {errs:?}"
        );
        assert!(errs.iter().any(|e| e.contains("id")));
        assert!(errs.iter().any(|e| e.contains("minute")));
        assert!(errs.iter().any(|e| e.contains("emits")));
    }

    #[test]
    fn validate_trigger_rejects_unparseable_cron() {
        let def = TriggerDef {
            id: "x".into(),
            schedule: TriggerSchedule::Cron {
                expr: "not a cron".into(),
            },
            emits: "a.b".into(),
            payload_template: serde_json::json!({}),
        };
        assert!(
            validate_trigger(&def)
                .unwrap_err()
                .iter()
                .any(|e| e.contains("cron"))
        );
    }
}
