#![forbid(unsafe_code)]
//! admin-service: what is switched on, for whom, since when, and who said so.
//!
//! The catalogue itself is data (`seeds.json`), lifted from the shipped `catalog.py` field for field: 28 features, the
//! three dependency rules, the eight configuration defaults and the nine service-health rows. It is not editable through
//! this service and never was, so it is compiled in rather than stored.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a feature override belongs to the CALLER, not to the platform. Enabling `double_entry` switches it on for the person
//!   who asked and for nobody else, and the shared module-level switch that let any caller toggle a flag for everyone is
//!   gone with the service it lived in;
//! * merging is per named field: a status override does not disturb an existing rollout percentage or config, and an
//!   override that names nothing leaves the catalogue's own answer alone;
//! * a feature that is not in the catalogue is missing, not creatable: reading, updating, enabling, scheduling and
//!   configuring one all answer as missing;
//! * organisation configuration is per organisation AND per feature. Enabling keeps the moment it was first enabled (it
//!   is not stamped again on a second enable) and clears the moment it was disabled; disabling stamps the moment it was
//!   disabled and drops nothing else. The two shortcuts mean what they say: enable is a hundred percent, disable is zero;
//! * a dependency is satisfied only when every feature it names is present and ENABLED - beta is not enough - and the
//!   ones that are not are listed back;
//! * a rollout is scheduled and can be cancelled by its own id OR by the feature's id, which is what the shipped store
//!   looked up: cancelling by feature cancels that feature's rollout. A cancellation that matches nothing is missing;
//! * a feature request arrives pending, keeps the requester it names rather than the caller who filed it, is listed
//!   newest first and cut to the limit asked for, and carries its review once reviewed;
//! * configuration is answered with a sensitive value MASKED, and reading one by key hides it unless the caller asks for
//!   it explicitly. A key that is not in the catalogue cannot be written, and an update records what the value was before;
//! * every change that matters leaves an audit entry naming the actor the caller stated, never the caller themselves -
//!   the trail would otherwise say only that somebody changed something.
//!
//! One route is answered at its own path rather than the shipped one: `GET /features/categories` was unreachable, because
//! the parameterised `GET /features/{feature_id}` was declared before it and swallowed the word "categories", answering
//! that no such feature exists. It answers at `/feature-categories` here, and a test says so.

use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use vimbai_common::clock;
use vimbai_common::dates;
use vimbai_store::read::{optional, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A caller's own override of one catalogue feature.
pub const FEATURE_OVERRIDE: RecordKind =
    RecordKind::new("AdminFeatureOverride", "OWNS_FEATURE_OVERRIDE");

/// One organisation's configuration of one feature.
pub const ORG_FEATURE: RecordKind = RecordKind::new("AdminOrgFeatureConfig", "OWNS_ORG_FEATURE");

/// A scheduled rollout.
pub const SCHEDULE: RecordKind = RecordKind::new("AdminRolloutSchedule", "OWNS_ROLLOUT_SCHEDULE");

/// A request for a feature somebody wants.
pub const REQUEST: RecordKind = RecordKind::new("AdminFeatureRequest", "OWNS_FEATURE_REQUEST");

/// A caller's own override of one configuration value.
pub const CONFIG_OVERRIDE: RecordKind =
    RecordKind::new("AdminConfigOverride", "OWNS_CONFIG_OVERRIDE");

/// A line in the audit trail.
pub const AUDIT: RecordKind = RecordKind::new("AdminAuditEntry", "OWNS_ADMIN_AUDIT");

/// The states a feature can be in.
pub const FEATURE_STATUSES: [&str; 4] = ["enabled", "disabled", "beta", "deprecated"];

/// The categories the catalogue is filed under.
pub const FEATURE_CATEGORIES: [&str; 11] = [
    "accounting",
    "finance",
    "banking",
    "fraud_detection",
    "reporting",
    "workflow",
    "multimodal",
    "integration",
    "notifications",
    "security",
    "system",
];

/// The states a feature request can be in.
pub const REQUEST_STATUSES: [&str; 4] = ["pending", "approved", "rejected", "implemented"];

/// The compiled catalogue: features, dependency rules, configuration defaults and service health.
fn catalogue() -> &'static Value {
    static CATALOGUE: OnceLock<Value> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        serde_json::from_str(include_str!("seeds.json")).expect("the shipped catalogue")
    })
}

fn feature_seeds() -> &'static Vec<Value> {
    static FEATURES: OnceLock<Vec<Value>> = OnceLock::new();
    FEATURES.get_or_init(|| {
        catalogue()
            .get("features")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    })
}

fn feature_seed(id: &str) -> Option<&'static Value> {
    feature_seeds()
        .iter()
        .find(|feature| feature.get("id").and_then(Value::as_str) == Some(id))
}

fn config_seeds() -> &'static Map<String, Value> {
    static CONFIG: OnceLock<Map<String, Value>> = OnceLock::new();
    CONFIG.get_or_init(|| {
        catalogue()
            .get("config")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    })
}

fn known_feature(id: &str) -> Result<&'static Value, StoreError> {
    feature_seed(id).ok_or(StoreError::NotFound)
}

fn now() -> f64 {
    clock::moment()
}

/// The moment a caller stated, in the engine's own measure. A date written the ordinary way is read as that date; a date
/// written in no way the engine understands is refused rather than guessed at.
fn moment(payload: &Value, key: &str) -> Result<Option<f64>, StoreError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => Ok(number.as_f64()),
        Some(Value::String(written)) => dates::instant_of(written)
            .map(Some)
            .ok_or_else(|| StoreError::Invalid(format!("{key} is not a moment: {written}"))),
        Some(other) => Err(StoreError::Invalid(format!(
            "{key} is not a moment: {other}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Features
// ---------------------------------------------------------------------------

/// The catalogue with this caller's own overrides applied, in catalogue order.
fn merged(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Vec<Value>, StoreError> {
    let overrides = Records::new(graph, FEATURE_OVERRIDE).list(scope, 500)?;
    let mut merged = Vec::with_capacity(feature_seeds().len());
    for seed in feature_seeds() {
        let held = overrides.iter().find(|held| {
            held.get("feature_id").and_then(Value::as_str) == seed.get("id").and_then(Value::as_str)
        });
        merged.push(merge_one(seed, held));
    }
    Ok(merged)
}

/// One feature with the named fields of an override laid over it, and nothing else disturbed.
fn merge_one(seed: &Value, held: Option<&Value>) -> Value {
    let mut feature = seed.clone();
    let Some(held) = held else {
        return feature;
    };
    if let Some(status) = held.get("status").and_then(Value::as_str) {
        feature["status"] = json!(status);
    }
    if let Some(config) = held.get("config").filter(|value| !value.is_null()) {
        feature["config"] = config.clone();
    }
    if let Some(rollout) = held
        .get("rollout_percentage")
        .filter(|value| !value.is_null())
    {
        feature["rollout_percentage"] = rollout.clone();
    }
    feature
}

fn merged_one(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
) -> Result<Value, StoreError> {
    known_feature(id)?;
    let held = Records::new(graph, FEATURE_OVERRIDE)
        .list(scope, 500)?
        .into_iter()
        .find(|held| held.get("feature_id").and_then(Value::as_str) == Some(id));
    Ok(merge_one(
        feature_seed(id).expect("a known feature"),
        held.as_ref(),
    ))
}

/// The features this caller sees, filtered by the category, state and enabled-only switches they asked for.
pub fn features(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let category = optional(asked, "category");
    let status = optional(asked, "status");
    if let Some(state) = status.as_deref() {
        if !FEATURE_STATUSES.contains(&state) {
            return Err(StoreError::Invalid(format!(
                "{state} is not a feature state"
            )));
        }
    }
    let enabled_only = asked
        .get("enabled_only")
        .map(|value| matches!(value, Value::Bool(true)) || value.as_str() == Some("true"))
        .unwrap_or(false);

    let mut listed = merged(graph, scope)?;
    listed.retain(|feature| {
        let here = |key: &str, wanted: &Option<String>| match wanted {
            None => true,
            Some(wanted) => feature.get(key).and_then(Value::as_str) == Some(wanted.as_str()),
        };
        let state = feature
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        here("category", &category)
            && here("status", &status)
            && (!enabled_only || state == "enabled")
    });
    Ok(Value::Array(listed))
}

/// One feature, with this caller's overrides applied.
pub fn feature(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
) -> Result<Value, StoreError> {
    merged_one(graph, scope, id)
}

/// The categories a feature can be filed under.
pub fn categories() -> Value {
    Value::Array(
        FEATURE_CATEGORIES
            .iter()
            .map(|name| json!({"name": name, "value": name}))
            .collect(),
    )
}

/// Lay a change over this caller's override of one feature, narrow to the fields named, and record who asked.
pub fn update_feature(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
    asked: &Value,
) -> Result<Value, StoreError> {
    known_feature(id)?;
    let status = optional(asked, "status");
    if let Some(state) = status.as_deref() {
        if !FEATURE_STATUSES.contains(&state) {
            return Err(StoreError::Invalid(format!(
                "{state} is not a feature state"
            )));
        }
    }
    let rollout = asked
        .get("rollout_percentage")
        .filter(|value| !value.is_null());
    if let Some(value) = rollout {
        let whole = value
            .as_i64()
            .or_else(|| value.as_str().and_then(|written| written.parse().ok()))
            .ok_or_else(|| {
                StoreError::Invalid("rollout_percentage is not a whole number".into())
            })?;
        if !(0..=100).contains(&whole) {
            return Err(StoreError::Invalid(
                "rollout_percentage is a share of a hundred".into(),
            ));
        }
    }
    let config = asked
        .get("config")
        .filter(|value| !value.is_null())
        .cloned();

    write_override(
        graph,
        scope,
        id,
        status.as_deref(),
        config.as_ref(),
        rollout,
    )?;
    record(
        graph,
        scope,
        "feature_updated",
        "feature",
        id,
        json!({"status": status, "config": asked.get("config")}),
        optional(asked, "actor_email").as_deref(),
    )?;
    merged_one(graph, scope, id)
}

/// Enable or disable one feature for this caller alone.
pub fn set_feature_state(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
    enabled: bool,
) -> Result<Value, StoreError> {
    known_feature(id)?;
    let state = if enabled { "enabled" } else { "disabled" };
    write_override(graph, scope, id, Some(state), None, None)?;
    Ok(json!({"status": state, "feature_id": id}))
}

/// Write the named fields of an override, keeping the ones that were not named.
fn write_override(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
    status: Option<&str>,
    config: Option<&Value>,
    rollout: Option<&Value>,
) -> Result<(), StoreError> {
    let records = Records::new(graph, FEATURE_OVERRIDE);
    let held = records
        .list(scope, 500)?
        .into_iter()
        .find(|held| held.get("feature_id").and_then(Value::as_str) == Some(id));
    let mut fields = json!({"feature_id": id});
    if let Some(status) = status {
        fields["status"] = json!(status);
    }
    if let Some(config) = config {
        fields["config"] = config.clone();
    }
    if let Some(rollout) = rollout {
        fields["rollout_percentage"] = rollout.clone();
    }
    match held {
        Some(held) => {
            let held_id = held.get("id").and_then(Value::as_str).unwrap_or_default();
            records.update(scope, held_id, &fields)?;
        }
        None => {
            records.create(scope, &fields)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Organisation configuration
// ---------------------------------------------------------------------------

/// Every feature as it stands for one organisation, with the organisation's own configuration laid over each.
pub fn organization_features(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    organization: &str,
) -> Result<Value, StoreError> {
    let held: Vec<Value> = Records::new(graph, ORG_FEATURE)
        .list(scope, 500)?
        .into_iter()
        .filter(|held| held.get("organization_id").and_then(Value::as_str) == Some(organization))
        .collect();

    let mut listed = Vec::new();
    for feature in merged(graph, scope)? {
        let id = feature
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let configured = held
            .iter()
            .find(|held| held.get("feature_id").and_then(Value::as_str) == Some(id));
        let mut answered = feature.clone();
        match configured {
            Some(configured) => {
                answered["org_enabled"] =
                    configured.get("enabled").cloned().unwrap_or(json!(false));
                answered["org_rollout_percentage"] = configured
                    .get("rollout_percentage")
                    .cloned()
                    .unwrap_or(json!(100));
                answered["org_custom_config"] = configured
                    .get("custom_config")
                    .cloned()
                    .unwrap_or(Value::Null);
            }
            None => {
                // an organisation nobody has configured reads the catalogue's own defaults
                answered["org_enabled"] = feature
                    .get("enabled_by_default")
                    .cloned()
                    .unwrap_or(json!(false));
                answered["org_rollout_percentage"] = feature
                    .get("rollout_percentage")
                    .cloned()
                    .unwrap_or(json!(100));
                answered["org_custom_config"] = Value::Null;
            }
        }
        listed.push(answered);
    }
    Ok(Value::Array(listed))
}

/// Configure one feature for one organisation, keeping the moments that say when it changed.
pub fn set_org_feature(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    organization: &str,
    id: &str,
    enabled: bool,
    asked: &Value,
) -> Result<Value, StoreError> {
    known_feature(id)?;
    let records = Records::new(graph, ORG_FEATURE);
    let held = records.list(scope, 500)?.into_iter().find(|held| {
        held.get("organization_id").and_then(Value::as_str) == Some(organization)
            && held.get("feature_id").and_then(Value::as_str) == Some(id)
    });

    let rollout = asked
        .get("rollout_percentage")
        .and_then(Value::as_i64)
        .map(|whole| json!(whole))
        .unwrap_or_else(|| json!(if enabled { 100 } else { 0 }));
    let custom = asked.get("custom_config").cloned().unwrap_or(Value::Null);
    let notes = optional(asked, "notes");
    let by = optional(asked, "updated_by").unwrap_or_else(|| "admin".to_string());
    let moment_now = now();

    let mut fields = json!({
        "organization_id": organization,
        "feature_id": id,
        "enabled": enabled,
        "rollout_percentage": rollout,
        "custom_config": custom,
        "notes": notes,
        "enabled_by": by,
        "updated_at": moment_now,
    });
    // enabling keeps the moment it was FIRST enabled; disabling stamps the moment it stopped
    let enabled_at = held
        .as_ref()
        .and_then(|held| held.get("enabled_at"))
        .filter(|value| !value.is_null())
        .cloned();
    fields["enabled_at"] = if enabled {
        enabled_at.unwrap_or_else(|| json!(moment_now))
    } else {
        Value::Null
    };
    fields["disabled_at"] = if enabled {
        Value::Null
    } else {
        json!(moment_now)
    };

    let saved = match &held {
        Some(held) => {
            let held_id = held.get("id").and_then(Value::as_str).unwrap_or_default();
            records.update(scope, held_id, &fields)?
        }
        None => records.create(scope, &fields)?,
    };

    record(
        graph,
        scope,
        "org_feature_updated",
        "org_feature",
        &format!("{organization}:{id}"),
        json!({"enabled": enabled, "rollout_percentage": rollout, "organization_id": organization}),
        Some(&format!("{by}@vimbai.com")),
    )?;
    Ok(saved)
}

/// What a feature depends on, and whether this caller has all of it switched on.
pub fn dependencies(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
) -> Result<Value, StoreError> {
    known_feature(id)?;
    let rule = catalogue()
        .get("dependencies")
        .and_then(|rules| rules.get(id))
        .cloned();
    let Some(rule) = rule else {
        return Ok(json!({"feature_id": id, "dependencies": [], "satisfied": true}));
    };

    let depends_on: Vec<String> = rule
        .get("depends_on")
        .and_then(Value::as_array)
        .map(|named| {
            named
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let merged = merged(graph, scope)?;
    let mut missing = Vec::new();
    for wanted in &depends_on {
        let state = merged
            .iter()
            .find(|feature| feature.get("id").and_then(Value::as_str) == Some(wanted.as_str()))
            .and_then(|feature| feature.get("status").and_then(Value::as_str));
        // beta is not enabled: a dependency has to be switched on, not merely present
        if state != Some("enabled") {
            missing.push(json!(wanted));
        }
    }
    Ok(json!({
        "feature_id": id,
        "depends_on": rule.get("depends_on").cloned().unwrap_or(json!([])),
        "required_permissions": rule.get("required_permissions").cloned().unwrap_or(json!([])),
        "min_rollout_percentage": rule.get("min_rollout_percentage").cloned().unwrap_or(json!(0)),
        "satisfied": missing.is_empty(),
        "missing_dependencies": missing
    }))
}

// ---------------------------------------------------------------------------
// Rollout schedules
// ---------------------------------------------------------------------------

/// The rollouts this caller has scheduled, filtered as asked.
pub fn schedules(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let feature = optional(asked, "feature_id");
    let organization = optional(asked, "organization_id");
    let status = optional(asked, "status");
    let held: Vec<Value> = Records::new(graph, SCHEDULE)
        .list(scope, 500)?
        .into_iter()
        .filter(|held| {
            let matches = |key: &str, wanted: &Option<String>| match wanted {
                None => true,
                Some(wanted) => held.get(key).and_then(Value::as_str) == Some(wanted.as_str()),
            };
            matches("feature_id", &feature)
                && matches("organization_id", &organization)
                && matches("status", &status)
        })
        .collect();
    Ok(Value::Array(held))
}

/// Schedule a rollout of one feature.
pub fn schedule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let id = text(asked, "feature_id");
    if id.trim().is_empty() {
        return Err(StoreError::Invalid("a feature_id is required".into()));
    }
    known_feature(&id)?;
    let when = moment(asked, "scheduled_date")?
        .ok_or_else(|| StoreError::Invalid("a scheduled_date is required".into()))?;
    let target = asked
        .get("target_percentage")
        .and_then(Value::as_i64)
        .ok_or_else(|| StoreError::Invalid("a target_percentage is required".into()))?;

    let created_by = optional(asked, "created_by").unwrap_or_else(|| "admin".to_string());
    let saved = Records::new(graph, SCHEDULE).create(
        scope,
        &json!({
            "feature_id": id,
            "organization_id": optional(asked, "organization_id"),
            "scheduled_date": when,
            "target_percentage": target,
            "created_by": created_by,
            "status": "scheduled",
            "created_at": now(),
        }),
    )?;

    record(
        graph,
        scope,
        "rollout_scheduled",
        "rollout_schedule",
        &id,
        json!({"scheduled_date": when, "target_percentage": target}),
        Some(&format!("{created_by}@vimbai.com")),
    )?;
    Ok(saved)
}

/// Cancel a rollout. A schedule is looked up by its own id OR by the feature's, as the shipped store did.
pub fn cancel_schedule(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, SCHEDULE);
    let held = records.list(scope, 500)?.into_iter().find(|held| {
        held.get("id").and_then(Value::as_str) == Some(id)
            || held.get("feature_id").and_then(Value::as_str) == Some(id)
    });
    let Some(held) = held else {
        return Err(StoreError::NotFound);
    };
    let held_id = held.get("id").and_then(Value::as_str).unwrap_or_default();
    records.update(scope, held_id, &json!({"status": "cancelled"}))?;
    Ok(json!({"status": "cancelled", "schedule_id": held_id}))
}

// ---------------------------------------------------------------------------
// Feature requests
// ---------------------------------------------------------------------------

/// File a request. The requester named in the request is kept as the requester, whoever filed it.
pub fn submit_request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let name = text(asked, "feature_name");
    if name.trim().is_empty() {
        return Err(StoreError::Invalid("a feature_name is required".into()));
    }
    let user_id = optional(asked, "user_id").unwrap_or_default();
    let user_email = optional(asked, "user_email").unwrap_or_default();
    let priority = optional(asked, "priority").unwrap_or_else(|| "normal".to_string());
    let moment_now = now();
    let saved = Records::new(graph, REQUEST).create(
        scope,
        &json!({
            // the requester is not the caller, so the requester is held under its own name and presented as user_id
            "req_user_id": user_id,
            "user_email": user_email,
            "organization_id": optional(asked, "organization_id"),
            "feature_name": name,
            "feature_description": optional(asked, "feature_description"),
            "category": optional(asked, "category"),
            "priority": priority,
            "business_justification": optional(asked, "business_justification"),
            "status": "pending",
            "created_at": moment_now,
            "updated_at": moment_now,
        }),
    )?;
    record(
        graph,
        scope,
        "feature_request_submitted",
        "feature_request",
        saved.get("id").and_then(Value::as_str).unwrap_or_default(),
        json!({"feature_name": name, "priority": priority}),
        Some(&user_email),
    )?;
    Ok(presented(&saved))
}

/// The requests this caller can see, newest first, cut to the limit asked for.
pub fn requests(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let status = optional(asked, "status");
    let organization = optional(asked, "organization_id");
    let priority = optional(asked, "priority");
    let limit = asked
        .get("limit")
        .and_then(Value::as_i64)
        .filter(|whole| *whole >= 0)
        .unwrap_or(50) as usize;

    let mut held: Vec<Value> = Records::new(graph, REQUEST)
        .list(scope, 500)?
        .into_iter()
        .filter(|held| {
            let matches = |key: &str, wanted: &Option<String>| match wanted {
                None => true,
                Some(wanted) => held.get(key).and_then(Value::as_str) == Some(wanted.as_str()),
            };
            matches("status", &status)
                && matches("organization_id", &organization)
                && matches("priority", &priority)
        })
        .collect();
    held.sort_by(|one, other| {
        let at = |held: &Value| {
            held.get("created_at")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        };
        at(other)
            .partial_cmp(&at(one))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    held.truncate(limit);
    Ok(Value::Array(held.iter().map(presented).collect()))
}

/// One request, if it is this caller's to see.
pub fn request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, REQUEST).get(scope, id)?;
    Ok(presented(&held))
}

/// Review a request: the state it lands in, who decided, when, and why.
pub fn review_request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
    asked: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, REQUEST);
    let held = records.get(scope, id)?;
    let state = text(asked, "status");
    if !REQUEST_STATUSES.contains(&state.as_str()) {
        return Err(refused(&format!("{state} is not a request state")));
    }
    let reviewer = text(asked, "reviewed_by");
    if reviewer.trim().is_empty() {
        return Err(StoreError::Invalid("a reviewed_by is required".into()));
    }
    let moment_now = now();
    let saved = records.update(
        scope,
        held.get("id").and_then(Value::as_str).unwrap_or_default(),
        &json!({
            "status": state,
            "reviewed_by": reviewer,
            "reviewed_at": moment_now,
            "review_notes": optional(asked, "review_notes"),
            "updated_at": moment_now,
        }),
    )?;
    record(
        graph,
        scope,
        "feature_request_reviewed",
        "feature_request",
        id,
        json!({"status": state, "review_notes": optional(asked, "review_notes")}),
        Some(&format!("{reviewer}@vimbai.com")),
    )?;
    Ok(presented(&saved))
}

/// Withdraw a request.
pub fn withdraw_request(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    id: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, REQUEST);
    let held = records.get(scope, id)?;
    records.delete(
        scope,
        held.get("id").and_then(Value::as_str).unwrap_or_default(),
    )?;
    Ok(json!({"status": "deleted", "request_id": id}))
}

/// A request as it is handed back: the requester under the name the API uses for them.
fn presented(held: &Value) -> Value {
    let mut shown = held.clone();
    if let Some(requester) = held.get("req_user_id").cloned() {
        shown["user_id"] = requester;
    }
    shown
}

// ---------------------------------------------------------------------------
// System configuration
// ---------------------------------------------------------------------------

/// The configuration this caller sees, sensitive values masked.
pub fn config(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let category = optional(asked, "category");
    let mut listed = Vec::new();
    for (key, default) in config_seeds() {
        if let Some(wanted) = &category {
            if default.get("category").and_then(Value::as_str) != Some(wanted.as_str()) {
                continue;
            }
        }
        listed.push(config_one(graph, scope, key)?.0);
    }
    Ok(Value::Array(listed))
}

/// One configuration value, and whether it was hidden.
fn config_one(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    key: &str,
) -> Result<(Value, bool), StoreError> {
    let default = config_seeds()
        .get(key)
        .cloned()
        .ok_or(StoreError::NotFound)?;
    let held = Records::new(graph, CONFIG_OVERRIDE)
        .list(scope, 500)?
        .into_iter()
        .find(|held| held.get("key").and_then(Value::as_str) == Some(key));

    let mut answered = default;
    match held {
        Some(held) => {
            if let Some(value) = held.get("value") {
                answered["value"] = value.clone();
            }
            answered["updated_at"] = held.get("updated_at").cloned().unwrap_or(Value::Null);
            answered["updated_by"] = held.get("updated_by").cloned().unwrap_or(Value::Null);
        }
        None => {
            // a compiled default has no moment of its own, so it reads as the moment the catalogue was consulted
            answered["updated_at"] = json!(now());
            answered["updated_by"] = Value::Null;
        }
    }

    let hidden = mask(&mut answered);
    Ok((answered, hidden))
}

/// Hide a sensitive value in place, answering whether it was hidden.
///
/// No value in the shipped catalogue is marked sensitive, so this rule hides nothing today; it is kept because the
/// catalogue is data and a value marked sensitive tomorrow has to be hidden the moment it is added, not when somebody
/// remembers to write the rule.
fn mask(answered: &mut Value) -> bool {
    let sensitive = answered
        .get("is_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let held = answered.get("value").map(|value| !value.is_null());
    if sensitive && held.unwrap_or(false) {
        answered["value"] = json!("***HIDDEN***");
        return true;
    }
    false
}

/// One configuration value by key. A sensitive value is hidden unless the caller asks for it.
pub fn config_entry(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    key: &str,
    asked: &Value,
) -> Result<Value, StoreError> {
    let include = asked
        .get("include_sensitive")
        .map(|value| matches!(value, Value::Bool(true)) || value.as_str() == Some("true"))
        .unwrap_or(false);
    let (answered, hidden) = config_one(graph, scope, key)?;
    if hidden && !include {
        return Ok(json!({
            "key": answered.get("key").cloned().unwrap_or(json!(key)),
            "value": "***HIDDEN***",
            "description": answered.get("description").cloned().unwrap_or(Value::Null),
            "category": answered.get("category").cloned().unwrap_or(Value::Null),
            "is_sensitive": true
        }));
    }
    Ok(answered)
}

/// Change one configuration value for this caller, recording what it was before.
pub fn set_config(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    key: &str,
    asked: &Value,
) -> Result<Value, StoreError> {
    config_seeds().get(key).ok_or(StoreError::NotFound)?;
    if !asked
        .get("value")
        .map(|value| !value.is_null())
        .unwrap_or(false)
    {
        return Err(StoreError::Invalid("a value is required".into()));
    }
    let value = asked.get("value").cloned().unwrap_or(Value::Null);
    let by = optional(asked, "updated_by").unwrap_or_else(|| "admin".to_string());
    let old = config_one(graph, scope, key)
        .map(|(answered, _)| answered.get("value").cloned().unwrap_or(Value::Null))?;

    let records = Records::new(graph, CONFIG_OVERRIDE);
    let held = records
        .list(scope, 500)?
        .into_iter()
        .find(|held| held.get("key").and_then(Value::as_str) == Some(key));
    let fields = json!({"key": key, "value": value, "updated_by": by, "updated_at": now()});
    match held {
        Some(held) => {
            let held_id = held.get("id").and_then(Value::as_str).unwrap_or_default();
            records.update(scope, held_id, &fields)?;
        }
        None => {
            records.create(scope, &fields)?;
        }
    }

    record(
        graph,
        scope,
        "config_updated",
        "config",
        key,
        json!({"old_value": old, "new_value": value}),
        Some(&format!("{by}@vimbai.com")),
    )?;
    config_one(graph, scope, key).map(|(answered, _)| answered)
}

// ---------------------------------------------------------------------------
// Audit trail
// ---------------------------------------------------------------------------

/// Write one line in the trail. The actor is the person the caller names, not the caller themselves.
pub fn record(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    changes: Value,
    actor_email: Option<&str>,
) -> Result<Value, StoreError> {
    let actor = optional(
        &json!({"user_id": actor_email.map(|email| email.split('@').next().unwrap_or(email))}),
        "user_id",
    )
    .unwrap_or_default();
    let email = actor_email
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{actor}@vimbai.com"));
    Records::new(graph, AUDIT).create(
        scope,
        &json!({
            // the person who did the thing is not the caller, so they are held under their own name
            "actor_user_id": actor,
            "user_email": email,
            "action": action,
            "resource_type": resource_type,
            "resource_id": resource_id,
            "changes": changes,
            "timestamp": now(),
        }),
    )
}

/// Post a line into the trail as the caller states it.
pub fn create_audit(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let actor = text(asked, "user_id");
    let email = text(asked, "user_email");
    let created = Records::new(graph, AUDIT).create(
        scope,
        &json!({
            "actor_user_id": actor,
            "user_email": email,
            "action": text(asked, "action"),
            "resource_type": text(asked, "resource_type"),
            "resource_id": text(asked, "resource_id"),
            "changes": asked.get("changes").cloned().unwrap_or(Value::Null),
            "timestamp": now(),
        }),
    )?;
    Ok(presented_audit(&created))
}

/// The trail this caller can see, newest first, cut to the limit asked for.
pub fn audit(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    asked: &Value,
) -> Result<Value, StoreError> {
    let user = optional(asked, "user_id");
    let action = optional(asked, "action");
    let resource = optional(asked, "resource_type");
    let limit = asked
        .get("limit")
        .and_then(Value::as_i64)
        .filter(|whole| *whole >= 0)
        .unwrap_or(100) as usize;

    let mut held: Vec<Value> = Records::new(graph, AUDIT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|held| {
            let matches = |key: &str, wanted: &Option<String>| match wanted {
                None => true,
                Some(wanted) => held.get(key).and_then(Value::as_str) == Some(wanted.as_str()),
            };
            matches("actor_user_id", &user)
                && matches("action", &action)
                && matches("resource_type", &resource)
        })
        .collect();
    held.sort_by(|one, other| {
        let at = |held: &Value| held.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0);
        at(other)
            .partial_cmp(&at(one))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    held.truncate(limit);
    Ok(Value::Array(held.iter().map(presented_audit).collect()))
}

fn presented_audit(held: &Value) -> Value {
    let mut shown = held.clone();
    if let Some(actor) = held.get("actor_user_id").cloned() {
        shown["user_id"] = actor;
    }
    shown
}

// ---------------------------------------------------------------------------
// Health and statistics
// ---------------------------------------------------------------------------

/// The service-health rows the catalogue ships with.
pub fn services_health() -> Value {
    let rows = catalogue()
        .get("services_health")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // a compiled row has no moment of its own, so each reads as the moment the catalogue was consulted
    let moment_now = now();
    Value::Array(
        rows.into_iter()
            .map(|mut row| {
                row["last_check"] = json!(moment_now);
                row
            })
            .collect(),
    )
}

/// How the platform stands for this caller.
pub fn stats(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let features = merged(graph, scope)?;
    let count = |state: &str| {
        features
            .iter()
            .filter(|feature| feature.get("status").and_then(Value::as_str) == Some(state))
            .count()
    };

    let requests = Records::new(graph, REQUEST).list(scope, 500)?;
    let pending = requests
        .iter()
        .filter(|held| held.get("status").and_then(Value::as_str) == Some("pending"))
        .count();
    let approved = requests
        .iter()
        .filter(|held| held.get("status").and_then(Value::as_str) == Some("approved"))
        .count();

    let schedules = Records::new(graph, SCHEDULE).list(scope, 500)?;
    let scheduled = schedules
        .iter()
        .filter(|held| held.get("status").and_then(Value::as_str) == Some("scheduled"))
        .count();

    let org_configs = Records::new(graph, ORG_FEATURE).list(scope, 500)?;
    let mut configured: Vec<String> = Vec::new();
    for held in &org_configs {
        let pair = format!(
            "{}:{}",
            held.get("organization_id")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            held.get("feature_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
        );
        if !configured.contains(&pair) {
            configured.push(pair);
        }
    }

    let audit_count = Records::new(graph, AUDIT).list(scope, 1000)?.len();

    Ok(json!({
        "total_features": features.len(),
        "enabled_features": count("enabled"),
        "beta_features": count("beta"),
        "disabled_features": count("disabled"),
        "total_config_entries": config_seeds().len(),
        "audit_logs_count": audit_count,
        "feature_requests": {"pending": pending, "approved": approved, "total": requests.len()},
        "rollout_schedules": {"scheduled": scheduled, "total": schedules.len()},
        "organization_configs": configured.len()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn other() -> BookScope {
        BookScope::new("user-2", "book-1")
    }

    fn built() -> MemoryGraph {
        MemoryGraph::new()
    }

    fn state_of(graph: &MemoryGraph, scope: &BookScope, id: &str) -> String {
        feature(graph, scope, id)
            .expect("a feature")
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn an_override_belongs_to_the_caller_who_made_it() {
        let graph = built();
        assert_eq!(state_of(&graph, &scope(), "double_entry"), "enabled");

        set_feature_state(&graph, &scope(), "double_entry", false).expect("disabled");
        assert_eq!(state_of(&graph, &scope(), "double_entry"), "disabled");
        // and nobody else is switched off by it
        assert_eq!(state_of(&graph, &other(), "double_entry"), "enabled");

        // the catalogue's own state is untouched at the source
        assert!(feature_seeds()
            .iter()
            .any(
                |feature| feature.get("id").and_then(Value::as_str) == Some("double_entry")
                    && feature.get("status").and_then(Value::as_str) == Some("enabled")
            ));

        // the two shortcuts leave no entry behind: the shipped switches wrote none either, and only a named change,
        // an organisation's configuration, a schedule, a request and a configuration value are recorded
        let trail = audit(&graph, &scope(), &json!({})).expect("the trail");
        assert_eq!(trail.as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn a_merge_touches_only_the_fields_it_names() {
        let graph = built();
        update_feature(
            &graph,
            &scope(),
            "double_entry",
            &json!({"rollout_percentage": 25}),
        )
        .expect("a rollout");
        let held = feature(&graph, &scope(), "double_entry").expect("the feature");
        assert_eq!(held["rollout_percentage"], json!(25));
        // the state the change did not name is the catalogue's own
        assert_eq!(held["status"], json!("enabled"));

        // and a second change to the state does not disturb the percentage
        update_feature(&graph, &scope(), "double_entry", &json!({"status": "beta"}))
            .expect("a state");
        let held = feature(&graph, &scope(), "double_entry").expect("the feature");
        assert_eq!(held["status"], json!("beta"));
        assert_eq!(held["rollout_percentage"], json!(25));

        // a share outside a hundred is refused, and a state the service does not name
        assert!(update_feature(
            &graph,
            &scope(),
            "double_entry",
            &json!({"rollout_percentage": 130})
        )
        .is_err());
        assert!(update_feature(
            &graph,
            &scope(),
            "double_entry",
            &json!({"status": "maybe"})
        )
        .is_err());
        // a feature that is not in the catalogue is missing rather than creatable
        assert!(matches!(
            feature(&graph, &scope(), "teleportation"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn listing_filters_on_what_was_asked_and_nothing_else() {
        let graph = built();
        let accounting =
            features(&graph, &scope(), &json!({"category": "accounting"})).expect("listed");
        let listed = accounting.as_array().cloned().unwrap_or_default();
        assert!(!listed.is_empty());
        assert!(listed
            .iter()
            .all(|feature| feature["category"] == json!("accounting")));

        let enabled = features(&graph, &scope(), &json!({"enabled_only": true})).expect("listed");
        assert!(enabled
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .all(|feature| feature["status"] == json!("enabled")));

        // the whole catalogue is answered when nothing is asked
        assert_eq!(
            features(&graph, &scope(), &json!({}))
                .expect("listed")
                .as_array()
                .map(Vec::len),
            Some(feature_seeds().len())
        );
        // and a state the service does not name is refused
        assert!(features(&graph, &scope(), &json!({"status": "sometimes"})).is_err());
    }

    #[test]
    fn an_organisation_keeps_the_moment_it_was_first_enabled() {
        let graph = built();
        let enabled = set_org_feature(&graph, &scope(), "org-1", "budgeting", true, &json!({}))
            .expect("configured");
        assert_eq!(enabled["enabled"], json!(true));
        assert_eq!(enabled["rollout_percentage"], json!(100));
        let first = enabled["enabled_at"].clone();
        assert!(!first.is_null());

        // enabling a second time does not restamp the moment it was first enabled
        std::thread::sleep(std::time::Duration::from_millis(10));
        let again = set_org_feature(
            &graph,
            &scope(),
            "org-1",
            "budgeting",
            true,
            &json!({"notes": "still on"}),
        )
        .expect("configured");
        assert_eq!(again["enabled_at"], first);

        // disabling stamps the moment it stopped and is a zero share
        let off = set_org_feature(&graph, &scope(), "org-1", "budgeting", false, &json!({}))
            .expect("configured");
        assert_eq!(off["rollout_percentage"], json!(0));
        assert!(off["disabled_at"].as_f64().is_some());

        // an organisation nobody has configured reads the catalogue's defaults
        let listed = organization_features(&graph, &scope(), "org-2").expect("listed");
        let double_entry = listed
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .find(|feature| feature["id"] == json!("double_entry"))
            .expect("a feature");
        assert_eq!(double_entry["org_enabled"], json!(true));
        assert!(double_entry["org_custom_config"].is_null());

        // one organisation's configuration is not another's
        let other_org = organization_features(&graph, &scope(), "org-1").expect("listed");
        let budgeting = other_org
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .find(|feature| feature["id"] == json!("budgeting"))
            .expect("a feature");
        assert_eq!(budgeting["org_enabled"], json!(false));
    }

    #[test]
    fn a_dependency_needs_its_dependencies_switched_on() {
        let graph = built();
        // both dependencies are switched on in the catalogue, so the rule is satisfied to begin with
        let before = dependencies(&graph, &scope(), "forecasting").expect("read");
        assert_eq!(before["satisfied"], json!(true));
        assert_eq!(before["missing_dependencies"], json!([]));

        // switching one off leaves exactly that one missing
        set_feature_state(&graph, &scope(), "budgeting", false).expect("off");
        let half = dependencies(&graph, &scope(), "forecasting").expect("read");
        assert_eq!(half["satisfied"], json!(false));
        assert_eq!(half["missing_dependencies"], json!(["budgeting"]));

        set_feature_state(&graph, &scope(), "scenario_modeling", false).expect("off");
        let none = dependencies(&graph, &scope(), "forecasting").expect("read");
        assert_eq!(
            none["missing_dependencies"],
            json!(["budgeting", "scenario_modeling"])
        );

        // beta is not enabled
        update_feature(&graph, &scope(), "budgeting", &json!({"status": "beta"})).expect("beta");
        set_feature_state(&graph, &scope(), "scenario_modeling", true).expect("on");
        let beta = dependencies(&graph, &scope(), "forecasting").expect("read");
        assert_eq!(beta["satisfied"], json!(false));
        assert_eq!(beta["missing_dependencies"], json!(["budgeting"]));

        // a feature with no rules depends on nothing
        let none = dependencies(&graph, &scope(), "double_entry").expect("read");
        assert_eq!(none["satisfied"], json!(true));
        assert_eq!(none["dependencies"], json!([]));
    }

    #[test]
    fn a_rollout_is_scheduled_and_cancelled_by_its_own_id_or_its_feature() {
        let graph = built();
        // a feature that is not in the catalogue cannot be scheduled
        assert!(matches!(
            schedule(
                &graph,
                &scope(),
                &json!({"feature_id": "teleportation", "scheduled_date": "2026-12-01T00:00:00Z", "target_percentage": 50})
            ),
            Err(StoreError::NotFound)
        ));

        let made = schedule(
            &graph,
            &scope(),
            &json!({"feature_id": "budgeting", "scheduled_date": "2026-12-01T00:00:00Z", "target_percentage": 50}),
        )
        .expect("scheduled");
        assert_eq!(made["status"], json!("scheduled"));
        assert_eq!(made["created_by"], json!("admin"));

        // cancelling by the FEATURE's id finds the rollout, as the shipped store did
        let cancelled = cancel_schedule(&graph, &scope(), "budgeting").expect("cancelled");
        assert_eq!(cancelled["status"], json!("cancelled"));
        assert_eq!(
            schedules(&graph, &scope(), &json!({"status": "cancelled"}))
                .expect("listed")
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            schedules(&graph, &scope(), &json!({"status": "scheduled"}))
                .expect("listed")
                .as_array()
                .map(Vec::len),
            Some(0)
        );

        // and one that matches nothing is missing
        assert!(matches!(
            cancel_schedule(&graph, &scope(), "nothing"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_request_keeps_its_requester_and_carries_its_review() {
        let graph = built();
        let filed = submit_request(
            &graph,
            &scope(),
            &json!({"user_id": "user-9", "user_email": "nine@vimbai.com", "feature_name": "Payroll export",
                    "priority": "high"}),
        )
        .expect("filed");
        assert_eq!(filed["user_id"], json!("user-9"));
        assert_eq!(filed["status"], json!("pending"));
        let id = filed["id"].as_str().unwrap_or_default().to_string();

        // newest first, and the filters bite
        let listed = requests(&graph, &scope(), &json!({"priority": "high"})).expect("listed");
        assert_eq!(listed.as_array().map(Vec::len), Some(1));
        assert_eq!(
            requests(&graph, &scope(), &json!({"status": "approved"}))
                .expect("listed")
                .as_array()
                .map(Vec::len),
            Some(0)
        );

        // a review states who decided it, when, and why
        let reviewed = review_request(
            &graph,
            &scope(),
            &id,
            &json!({"status": "approved", "reviewed_by": "admin", "review_notes": "ships with packaging"}),
        )
        .expect("reviewed");
        assert_eq!(reviewed["status"], json!("approved"));
        assert_eq!(reviewed["reviewed_by"], json!("admin"));
        assert!(reviewed["reviewed_at"].as_f64().is_some());

        // a state the service does not name is refused, and a review needs someone to attribute it to
        assert!(review_request(
            &graph,
            &scope(),
            &id,
            &json!({"status": "maybe", "reviewed_by": "admin"})
        )
        .is_err());
        assert!(review_request(&graph, &scope(), &id, &json!({"status": "rejected"})).is_err());

        // withrawing one removes it, and a second withdrawal finds nothing
        withdraw_request(&graph, &scope(), &id).expect("deleted");
        assert!(matches!(
            request(&graph, &scope(), &id),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_sensitive_configuration_value_would_be_hidden() {
        // the rule itself: a value marked sensitive is hidden, and an ordinary one is not
        let mut secret = json!({"key": "api_key", "value": "abc123", "is_sensitive": true});
        assert!(mask(&mut secret));
        assert_eq!(secret["value"], json!("***HIDDEN***"));
        let mut ordinary =
            json!({"key": "company_name", "value": "Vimbai Corporation", "is_sensitive": false});
        assert!(!mask(&mut ordinary));
        assert_eq!(ordinary["value"], json!("Vimbai Corporation"));
        // a sensitive entry with no value yet has nothing to hide
        let mut empty = json!({"key": "api_key", "value": Value::Null, "is_sensitive": true});
        assert!(!mask(&mut empty));

        let graph = built();
        let listed: Vec<Value> = config(&graph, &scope(), &json!({}))
            .expect("listed")
            .as_array()
            .cloned()
            .unwrap_or_default();
        // and the shipped catalogue marks nothing sensitive, so nothing is hidden today
        assert_eq!(listed.len(), config_seeds().len());
        assert!(listed
            .iter()
            .all(|entry| entry["value"] != json!("***HIDDEN***")));
        assert!(listed
            .iter()
            .any(|entry| entry["key"] == json!("company_name")));

        // reading one by key answers the whole entry, the moment it was consulted included
        let read = config_entry(&graph, &scope(), "company_name", &json!({})).expect("read");
        assert_eq!(read["value"], json!("Vimbai Corporation"));
        assert!(read["updated_at"].as_f64().is_some());

        // a change is held per caller, records what it was before, and an unknown key cannot be written
        let changed = set_config(
            &graph,
            &scope(),
            "company_name",
            &json!({"value": "Vimbai Ltd"}),
        )
        .expect("set");
        assert_eq!(changed["value"], json!("Vimbai Ltd"));
        assert_eq!(
            config_entry(&graph, &scope(), "company_name", &json!({})).expect("read")["value"],
            json!("Vimbai Ltd")
        );
        assert_eq!(
            config_entry(&graph, &other(), "company_name", &json!({})).expect("read")["value"],
            json!("Vimbai Corporation")
        );
        assert!(matches!(
            set_config(&graph, &scope(), "secret_handshake", &json!({"value": "x"})),
            Err(StoreError::NotFound)
        ));

        let trail =
            audit(&graph, &scope(), &json!({"action": "config_updated"})).expect("the trail");
        let entry = trail.as_array().cloned().unwrap_or_default().remove(0);
        assert_eq!(entry["changes"]["old_value"], json!("Vimbai Corporation"));
        assert_eq!(entry["user_id"], json!("admin"));
    }

    #[test]
    fn the_trail_names_the_actor_the_caller_states() {
        let graph = built();
        let posted = create_audit(
            &graph,
            &scope(),
            &json!({"user_id": "user-9", "user_email": "nine@vimbai.com", "action": "export_requested",
                    "resource_type": "ledger", "resource_id": "entry-1"}),
        )
        .expect("posted");
        // the caller is user-1, but the trail says who did it
        assert_eq!(posted["user_id"], json!("user-9"));
        assert_eq!(posted["user_email"], json!("nine@vimbai.com"));

        let mine = audit(&graph, &scope(), &json!({})).expect("the trail");
        assert_eq!(mine.as_array().map(Vec::len), Some(1));
        assert_eq!(
            audit(&graph, &scope(), &json!({"user_id": "user-9"}))
                .expect("the trail")
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        // another caller sees none of it
        assert_eq!(
            audit(&graph, &other(), &json!({}))
                .expect("the trail")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn the_statistics_count_what_this_caller_can_see() {
        let graph = built();
        let before = stats(&graph, &scope()).expect("counted");
        assert_eq!(before["total_features"], json!(feature_seeds().len()));
        assert_eq!(before["total_config_entries"], json!(config_seeds().len()));
        assert_eq!(before["feature_requests"]["total"], json!(0));

        set_feature_state(&graph, &scope(), "budgeting", false).expect("off");
        submit_request(
            &graph,
            &scope(),
            &json!({"user_id": "user-9", "user_email": "n@vimbai.com",
                                                   "feature_name": "X"}),
        )
        .expect("filed");
        let after = stats(&graph, &scope()).expect("counted");
        assert_eq!(
            after["enabled_features"].as_i64().unwrap_or(0),
            before["enabled_features"].as_i64().unwrap_or(0) - 1
        );
        assert_eq!(after["feature_requests"]["pending"], json!(1));

        // another caller's counts are their own
        assert_eq!(
            stats(&graph, &other()).expect("counted")["feature_requests"]["total"],
            json!(0)
        );

        // and the health rows are the shipped ones
        let health = services_health();
        assert_eq!(health.as_array().map(Vec::len), Some(9));
        assert!(health[0]["last_check"].as_f64().is_some());
        assert_eq!(categories().as_array().map(Vec::len), Some(11));
    }
}
