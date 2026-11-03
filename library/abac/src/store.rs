//! The facade: a rule set per organisation, evaluation, and health.

use serde_json::{json, Value};

use crate::decision::{evaluate, Decision, EvaluationContext};
use crate::rule::{Effect, Rule, RuleError, RuleSet};

#[derive(Debug, Clone)]
pub struct AbacStore {
    pub rules: RuleSet,
    /// What applies when no rule fires. Deny unless an organisation says
    /// otherwise, so a forgotten rule cannot become an open door.
    pub default_effect: Effect,
}

impl Default for AbacStore {
    fn default() -> Self {
        AbacStore {
            rules: RuleSet::new(),
            default_effect: Effect::Deny,
        }
    }
}

impl AbacStore {
    pub fn new() -> Self {
        AbacStore::default()
    }

    pub fn with_default_effect(mut self, effect: Effect) -> Self {
        self.default_effect = effect;
        self
    }

    pub fn add_rule(&mut self, rule: Rule) -> Result<Rule, RuleError> {
        self.rules.add(rule)
    }

    pub fn update_rule(&mut self, rule_id: &str, rule: Rule) -> Result<Rule, RuleError> {
        self.rules.update(rule_id, rule)
    }

    pub fn delete_rule(&mut self, rule_id: &str) -> Result<Rule, RuleError> {
        self.rules.remove(rule_id)
    }

    pub fn set_enabled(&mut self, rule_id: &str, enabled: bool) -> Result<(), RuleError> {
        self.rules.set_enabled(rule_id, enabled)
    }

    pub fn rules_for(&self, organization_id: &str, resource_type: &str) -> Vec<Rule> {
        self.rules.for_resource(organization_id, resource_type)
    }

    pub fn decide(&self, context: &EvaluationContext) -> Decision {
        evaluate(&self.rules, context, self.default_effect)
    }

    pub fn health(&self) -> Value {
        json!({
            "status": "healthy",
            "service": "policy-engine",
            "version": "3.0.0",
            "rules": self.rules.len(),
            "default_effect": self.default_effect.value(),
        })
    }
}
