//! Rules: one condition, one effect, one resource type.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::operator::Operator;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Says yes to a request nothing else decided.
    Allow,
    /// Blocks. Deny always wins over every allow.
    Deny,
    /// Passes with a warning attached; does not block.
    Warn,
    /// Passes but requires a second pair of eyes.
    RequireApproval,
}

impl Effect {
    pub fn value(self) -> &'static str {
        match self {
            Effect::Allow => "allow",
            Effect::Deny => "deny",
            Effect::Warn => "warn",
            Effect::RequireApproval => "require_approval",
        }
    }

    pub fn parse(text: &str) -> Option<Effect> {
        match text {
            "allow" => Some(Effect::Allow),
            "deny" => Some(Effect::Deny),
            "warn" => Some(Effect::Warn),
            "require_approval" => Some(Effect::RequireApproval),
            _ => None,
        }
    }

    pub fn blocks(self) -> bool {
        matches!(self, Effect::Deny)
    }

    /// An obligation the caller must honour: shown, or satisfied by approval.
    pub fn obligation(self) -> Option<&'static str> {
        match self {
            Effect::Warn => Some("warn"),
            Effect::RequireApproval => Some("require_approval"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub organization_id: String,
    /// `invoice`, `payment`, `journal`, `report`, ...
    pub resource_type: String,
    pub name: String,
    pub description: String,
    pub condition_field: String,
    pub condition_operator: String,
    pub condition_value: Value,
    pub effect: Effect,
    pub message: String,
    pub enabled: bool,
    /// Higher runs first in the explanation. Deny-overrides makes ordering a
    /// reporting concern rather than a correctness one.
    pub priority: i32,
    /// Optional action scope, e.g. `approve`. Empty means any action.
    pub action: String,
}

impl Rule {
    #[allow(clippy::too_many_arguments)] // one field per column of the rule form
    pub fn new(
        id: &str,
        organization_id: &str,
        resource_type: &str,
        name: &str,
        condition_field: &str,
        condition_operator: &str,
        condition_value: Value,
        effect: Effect,
    ) -> Rule {
        Rule {
            id: id.to_string(),
            organization_id: organization_id.to_string(),
            resource_type: resource_type.to_string(),
            name: name.to_string(),
            description: String::new(),
            condition_field: condition_field.to_string(),
            condition_operator: condition_operator.to_string(),
            condition_value,
            effect,
            message: String::new(),
            enabled: true,
            priority: 0,
            action: String::new(),
        }
    }

    pub fn with_message(mut self, message: &str) -> Rule {
        self.message = message.to_string();
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Rule {
        self.priority = priority;
        self
    }

    pub fn for_action(mut self, action: &str) -> Rule {
        self.action = action.to_string();
        self
    }

    pub fn disabled(mut self) -> Rule {
        self.enabled = false;
        self
    }

    pub fn operator(&self) -> Option<Operator> {
        Operator::parse(&self.condition_operator)
    }

    /// Validate the parts a client can get wrong. Done at creation, so a typo
    /// cannot sit in the store looking correct while never firing.
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.name.trim().is_empty() {
            return Err(RuleError::InvalidName);
        }
        if self.resource_type.trim().is_empty() {
            return Err(RuleError::MissingField("resource_type"));
        }
        if self.condition_field.trim().is_empty() {
            return Err(RuleError::MissingField("condition_field"));
        }
        let operator = Operator::parse(&self.condition_operator)
            .ok_or_else(|| RuleError::UnknownOperator(self.condition_operator.clone()))?;
        if operator.expects_list() {
            let valid = match (&self.condition_value, operator) {
                (Value::Array(items), Operator::Between) => items.len() == 2,
                (Value::Array(_), _) => true,
                _ => false,
            };
            if !valid {
                return Err(RuleError::InvalidConditionValue(
                    self.condition_operator.clone(),
                ));
            }
        }
        if operator.expects_nothing() && !self.condition_value.is_null() {
            // tolerated: providers send `true`, which is ignored
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    NotFound,
    DuplicateName(String),
    InvalidName,
    MissingField(&'static str),
    UnknownOperator(String),
    InvalidConditionValue(String),
}

impl RuleError {
    pub fn status_code(&self) -> u16 {
        match self {
            RuleError::NotFound => 404,
            RuleError::DuplicateName(_) => 409,
            _ => 400,
        }
    }

    pub fn message(&self) -> String {
        match self {
            RuleError::NotFound => "Rule not found".to_string(),
            RuleError::DuplicateName(name) => format!("Rule already exists: {name}"),
            RuleError::InvalidName => "Rule name is required".to_string(),
            RuleError::MissingField(field) => format!("Missing required field: {field}"),
            RuleError::UnknownOperator(operator) => {
                format!("Unsupported condition operator: {operator}")
            }
            RuleError::InvalidConditionValue(operator) => {
                format!("Condition value is not valid for operator: {operator}")
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: IndexMap<String, Rule>,
}

impl RuleSet {
    pub fn new() -> Self {
        RuleSet::default()
    }

    pub fn add(&mut self, rule: Rule) -> Result<Rule, RuleError> {
        rule.validate()?;
        if self.rules.values().any(|existing| {
            existing.id != rule.id
                && existing.organization_id == rule.organization_id
                && existing.name.eq_ignore_ascii_case(&rule.name)
        }) {
            return Err(RuleError::DuplicateName(rule.name.clone()));
        }
        self.rules.insert(rule.id.clone(), rule.clone());
        Ok(rule)
    }

    /// Replace a rule, keeping the original id.
    pub fn update(&mut self, rule_id: &str, mut replacement: Rule) -> Result<Rule, RuleError> {
        if !self.rules.contains_key(rule_id) {
            return Err(RuleError::NotFound);
        }
        replacement.id = rule_id.to_string();
        replacement.validate()?;
        self.rules.insert(rule_id.to_string(), replacement.clone());
        Ok(replacement)
    }

    pub fn remove(&mut self, rule_id: &str) -> Result<Rule, RuleError> {
        self.rules.shift_remove(rule_id).ok_or(RuleError::NotFound)
    }

    pub fn set_enabled(&mut self, rule_id: &str, enabled: bool) -> Result<(), RuleError> {
        self.rules
            .get_mut(rule_id)
            .map(|rule| rule.enabled = enabled)
            .ok_or(RuleError::NotFound)
    }

    pub fn get(&self, rule_id: &str) -> Option<&Rule> {
        self.rules.get(rule_id)
    }

    /// The rules that apply to a resource type, in evaluation order.
    pub fn for_resource(&self, organization_id: &str, resource_type: &str) -> Vec<Rule> {
        let mut matching: Vec<Rule> = self
            .rules
            .values()
            .filter(|rule| {
                rule.organization_id == organization_id
                    && rule.resource_type == resource_type
                    && rule.enabled
            })
            .cloned()
            .collect();
        matching.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.name.cmp(&right.name))
        });
        matching
    }

    pub fn all(&self, organization_id: &str) -> Vec<Rule> {
        self.rules
            .values()
            .filter(|rule| rule.organization_id == organization_id)
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
