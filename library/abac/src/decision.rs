//! Evaluating a request against the rules that apply to it.
//!
//! Deny-overrides: one deny blocks, whatever else allows. Warnings and approval
//! requirements travel with the decision as obligations instead of blocking it,
//! which is what the service did and what an approvals workflow needs.

use serde_json::{json, Map, Value};

use crate::operator::Comparison;
use crate::rule::{Effect, Rule, RuleSet};

/// The request being decided: whoever is acting, the resource, and the facts.
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationContext {
    pub organization_id: String,
    pub resource_type: String,
    /// Optional action, matched against a rule's `action` when the rule names one.
    pub action: String,
    /// Attributes: `amount`, `currency`, `category`, `department`, ...
    pub attributes: Map<String, Value>,
    pub subject_id: Option<String>,
}

impl EvaluationContext {
    pub fn new(organization_id: &str, resource_type: &str) -> EvaluationContext {
        EvaluationContext {
            organization_id: organization_id.to_string(),
            resource_type: resource_type.to_string(),
            action: String::new(),
            attributes: Map::new(),
            subject_id: None,
        }
    }

    pub fn with_attribute(mut self, name: &str, value: Value) -> EvaluationContext {
        self.attributes.insert(name.to_string(), value);
        self
    }

    pub fn with_action(mut self, action: &str) -> EvaluationContext {
        self.action = action.to_string();
        self
    }

    pub fn with_subject(mut self, subject_id: &str) -> EvaluationContext {
        self.subject_id = Some(subject_id.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuleOutcome {
    pub rule_id: String,
    pub rule_name: String,
    pub effect: Effect,
    pub message: String,
    pub triggered: bool,
    /// Why it did, or did not, fire. `matched`, `missing_attribute`,
    /// `type_mismatch`, `action_scope`, `disabled`.
    pub reason: &'static str,
}

impl RuleOutcome {
    pub fn to_json(&self) -> Value {
        json!({
            "rule_id": self.rule_id,
            "rule_name": self.rule_name,
            "action": self.effect.value(),
            "message": self.message,
            "triggered": self.triggered,
            "reason": self.reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// Deny when blocked, Allow when an allow rule fired, otherwise the default.
    pub effect: Effect,
    pub allowed: bool,
    pub blocked: bool,
    /// Obligations the caller must honour: `warn`, `require_approval`.
    pub obligations: Vec<String>,
    pub evaluations: Vec<RuleOutcome>,
    pub triggered_count: usize,
    /// The rule that decided it, when one did.
    pub decided_by: Option<String>,
}

impl Decision {
    /// The body the evaluate endpoint returns. Field names and the
    /// `triggered_count`/`blocked`/`allowed` trio match the service.
    pub fn to_json(&self) -> Value {
        json!({
            "evaluations": self.evaluations.iter().map(RuleOutcome::to_json).collect::<Vec<_>>(),
            "triggered_count": self.triggered_count,
            "blocked": self.blocked,
            "allowed": self.allowed,
            "effect": self.effect.value(),
            "obligations": self.obligations,
            "decided_by": self.decided_by,
        })
    }
}

/// Evaluate one context. `default_effect` is what applies when no rule fires;
/// deny is the safe default and the one the service effectively used.
pub fn evaluate(rules: &RuleSet, context: &EvaluationContext, default_effect: Effect) -> Decision {
    let applicable = rules.for_resource(&context.organization_id, &context.resource_type);
    let mut evaluations = Vec::with_capacity(applicable.len());
    let mut triggered: Vec<&Rule> = Vec::new();

    for rule in &applicable {
        let outcome = assess(rule, context);
        if outcome.triggered {
            triggered.push(rule);
        }
        evaluations.push(outcome);
    }

    let deny = triggered
        .iter()
        .find(|rule| rule.effect == Effect::Deny)
        .copied();
    let approval = triggered
        .iter()
        .find(|rule| rule.effect == Effect::RequireApproval)
        .copied();
    let warning = triggered
        .iter()
        .find(|rule| rule.effect == Effect::Warn)
        .copied();
    let allow = triggered
        .iter()
        .find(|rule| rule.effect == Effect::Allow)
        .copied();

    let mut obligations: Vec<String> = triggered
        .iter()
        .filter_map(|rule| rule.effect.obligation().map(ToString::to_string))
        .collect();
    obligations.sort();
    obligations.dedup();

    // Report the strongest thing that happened: a deny, then an approval
    // requirement, then a warning, then an allow, then the default.
    let (effect, decided_by) = match (deny, approval, warning, allow) {
        (Some(rule), _, _, _) => (Effect::Deny, Some(rule.name.clone())),
        (None, Some(rule), _, _) => (Effect::RequireApproval, Some(rule.name.clone())),
        (None, None, Some(rule), _) => (Effect::Warn, Some(rule.name.clone())),
        (None, None, None, Some(rule)) => (Effect::Allow, Some(rule.name.clone())),
        (None, None, None, None) => (default_effect, None),
    };

    Decision {
        effect,
        allowed: !effect.blocks(),
        blocked: effect.blocks(),
        obligations,
        triggered_count: triggered.len(),
        evaluations,
        decided_by,
    }
}

fn assess(rule: &Rule, context: &EvaluationContext) -> RuleOutcome {
    let mut outcome = RuleOutcome {
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        effect: rule.effect,
        message: rule.message.clone(),
        triggered: false,
        reason: "matched",
    };

    if !rule.enabled {
        outcome.reason = "disabled";
        return outcome;
    }
    if !rule.action.is_empty() && rule.action != context.action {
        outcome.reason = "action_scope";
        return outcome;
    }
    let Some(operator) = rule.operator() else {
        // A stored rule with an operator this build does not know cannot be
        // evaluated; it must not be assumed to match.
        outcome.reason = "unknown_operator";
        return outcome;
    };
    // `exists` and `not_exists` are about presence itself, so an absent field is
    // their input rather than a reason to stop.
    let presence_operator = matches!(
        rule.operator(),
        Some(crate::operator::Operator::Exists | crate::operator::Operator::NotExists)
    );
    let attribute = match context.attributes.get(&rule.condition_field) {
        Some(value) => value,
        None if presence_operator => &serde_json::Value::Null,
        None => {
            // Parity: an absent field never triggers an ordinary comparison.
            outcome.reason = "missing_attribute";
            return outcome;
        }
    };

    match crate::operator::compare(operator, attribute, &rule.condition_value) {
        Comparison::Matched => {
            outcome.triggered = true;
            outcome.reason = "matched";
        }
        Comparison::DidNotMatch => outcome.reason = "no_match",
        Comparison::Missing => outcome.reason = "missing_attribute",
        Comparison::TypeMismatch {
            attribute,
            rule: rule_type,
        } => {
            let _ = (attribute, rule_type);
            outcome.reason = "type_mismatch";
        }
    }
    outcome
}
