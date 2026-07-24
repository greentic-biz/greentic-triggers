//! `TriggerDef`: a schedule bound to the business event it emits, plus
//! validation and the fire -> business-event helper.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schedule::TriggerSchedule;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{TimeOfDay, TriggerSchedule};

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
}
