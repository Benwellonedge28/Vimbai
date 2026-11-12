//! Condition operators and how they compare an attribute to a rule's value.
//!
//! The service it replaced compared with Python's `>` and friends inside a
//! `try/except TypeError: pass`, so a rule with mismatched types silently never
//! fired. Here a type mismatch is still a non-match, but it is reported, and an
//! unknown operator is refused when the rule is created rather than lying
//! dormant.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Ne,
    Contains,
    StartsWith,
    EndsWith,
    In,
    NotIn,
    Between,
    Exists,
    NotExists,
}

impl Operator {
    pub fn parse(text: &str) -> Option<Operator> {
        match text.trim() {
            ">" | "gt" => Some(Operator::Gt),
            ">=" | "gte" => Some(Operator::Gte),
            "<" | "lt" => Some(Operator::Lt),
            "<=" | "lte" => Some(Operator::Lte),
            "==" | "eq" => Some(Operator::Eq),
            "!=" | "ne" => Some(Operator::Ne),
            "contains" => Some(Operator::Contains),
            "starts_with" | "startsWith" => Some(Operator::StartsWith),
            "ends_with" | "endsWith" => Some(Operator::EndsWith),
            "in" => Some(Operator::In),
            "not_in" | "notIn" => Some(Operator::NotIn),
            "between" => Some(Operator::Between),
            "exists" => Some(Operator::Exists),
            "not_exists" | "notExists" => Some(Operator::NotExists),
            _ => None,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            Operator::Gt => ">",
            Operator::Gte => ">=",
            Operator::Lt => "<",
            Operator::Lte => "<=",
            Operator::Eq => "==",
            Operator::Ne => "!=",
            Operator::Contains => "contains",
            Operator::StartsWith => "starts_with",
            Operator::EndsWith => "ends_with",
            Operator::In => "in",
            Operator::NotIn => "not_in",
            Operator::Between => "between",
            Operator::Exists => "exists",
            Operator::NotExists => "not_exists",
        }
    }

    /// How a rule's configured value is read: `in`, `not_in` and `between` take
    /// lists, `exists` takes nothing.
    pub fn expects_list(self) -> bool {
        matches!(self, Operator::In | Operator::NotIn | Operator::Between)
    }

    pub fn expects_nothing(self) -> bool {
        matches!(self, Operator::Exists | Operator::NotExists)
    }
}

/// The outcome of one comparison, with enough detail to explain a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    Matched,
    DidNotMatch,
    /// The attribute was absent, or explicitly null.
    Missing,
    /// The two sides are not comparable. Never a silent match.
    TypeMismatch {
        attribute: &'static str,
        rule: &'static str,
    },
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        // A number that arrived as a string, which JSON often does, still orders.
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn as_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn scalar_equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(_), Value::Number(_)) => as_number(left)
            .zip(as_number(right))
            .is_some_and(|(l, r)| l == r),
        (Value::Number(_), Value::String(_)) | (Value::String(_), Value::Number(_)) => {
            match (as_number(left), as_number(right)) {
                (Some(l), Some(r)) => l == r,
                _ => false,
            }
        }
        (Value::Bool(_), Value::String(text)) => match as_text(left) {
            Some(rendered) => rendered.eq_ignore_ascii_case(text),
            None => false,
        },
        (Value::String(text), Value::Bool(_)) => match as_text(right) {
            Some(rendered) => text.eq_ignore_ascii_case(&rendered),
            None => false,
        },
        _ => left == right,
    }
}

fn ordered_compare(operator: Operator, left: f64, right: f64) -> bool {
    match operator {
        Operator::Gt => left > right,
        Operator::Gte => left >= right,
        Operator::Lt => left < right,
        Operator::Lte => left <= right,
        _ => false,
    }
}

/// Compare one attribute value against the rule's value.
///
/// A list attribute matches if any element matches, which is how a person in
/// several departments is handled without writing one rule per department.
pub fn compare(operator: Operator, attribute: &Value, rule_value: &Value) -> Comparison {
    if attribute.is_null() {
        return match operator {
            Operator::NotExists => Comparison::Matched,
            Operator::Exists => Comparison::DidNotMatch,
            _ => Comparison::Missing,
        };
    }
    if matches!(operator, Operator::Exists) {
        return Comparison::Matched;
    }
    if matches!(operator, Operator::NotExists) {
        return Comparison::DidNotMatch;
    }

    if let Value::Array(items) = attribute {
        if operator == Operator::NotIn {
            let allowed = match rule_value {
                Value::Array(list) => list,
                _ => return comparison_type_mismatch(attribute, rule_value),
            };
            let overlap = items.iter().any(|item| {
                allowed
                    .iter()
                    .any(|candidate| scalar_equals(item, candidate))
            });
            return if overlap {
                Comparison::DidNotMatch
            } else {
                Comparison::Matched
            };
        }
        let mut saw_mismatch = false;
        for item in items {
            match compare(operator, item, rule_value) {
                Comparison::Matched => return Comparison::Matched,
                Comparison::TypeMismatch { .. } => saw_mismatch = true,
                _ => {}
            }
        }
        return if saw_mismatch {
            comparison_type_mismatch(attribute, rule_value)
        } else {
            Comparison::DidNotMatch
        };
    }

    match operator {
        Operator::Eq => matched_if(scalar_equals(attribute, rule_value)),
        Operator::Ne => matched_if(!scalar_equals(attribute, rule_value)),
        Operator::Gt | Operator::Gte | Operator::Lt | Operator::Lte => {
            match (as_number(attribute), as_number(rule_value)) {
                (Some(left), Some(right)) => matched_if(ordered_compare(operator, left, right)),
                _ => comparison_type_mismatch(attribute, rule_value),
            }
        }
        Operator::Contains => {
            let haystack = as_text(attribute).unwrap_or_default();
            let needle = as_text(rule_value).unwrap_or_default();
            matched_if(haystack.contains(&needle))
        }
        Operator::StartsWith => {
            let haystack = as_text(attribute).unwrap_or_default();
            let needle = as_text(rule_value).unwrap_or_default();
            matched_if(haystack.starts_with(&needle))
        }
        Operator::EndsWith => {
            let haystack = as_text(attribute).unwrap_or_default();
            let needle = as_text(rule_value).unwrap_or_default();
            matched_if(haystack.ends_with(&needle))
        }
        Operator::In => match rule_value {
            Value::Array(list) => matched_if(
                list.iter()
                    .any(|candidate| scalar_equals(attribute, candidate)),
            ),
            _ => comparison_type_mismatch(attribute, rule_value),
        },
        Operator::NotIn => match rule_value {
            Value::Array(list) => matched_if(
                !list
                    .iter()
                    .any(|candidate| scalar_equals(attribute, candidate)),
            ),
            _ => comparison_type_mismatch(attribute, rule_value),
        },
        Operator::Between => match rule_value {
            Value::Array(range) if range.len() == 2 => {
                match (
                    as_number(attribute),
                    as_number(&range[0]),
                    as_number(&range[1]),
                ) {
                    (Some(value), Some(low), Some(high)) => {
                        matched_if(value >= low.min(high) && value <= low.max(high))
                    }
                    _ => comparison_type_mismatch(attribute, rule_value),
                }
            }
            _ => comparison_type_mismatch(attribute, rule_value),
        },
        Operator::Exists | Operator::NotExists => Comparison::DidNotMatch,
    }
}

fn matched_if(condition: bool) -> Comparison {
    if condition {
        Comparison::Matched
    } else {
        Comparison::DidNotMatch
    }
}

fn comparison_type_mismatch(attribute: &Value, rule_value: &Value) -> Comparison {
    Comparison::TypeMismatch {
        attribute: type_name(attribute),
        rule: type_name(rule_value),
    }
}
