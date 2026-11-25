//! What a step of a rule actually does.
//!
//! A rule used to be a list of steps that were written down as completed. Nothing was done: a rule could name
//! `post_depreciation` and the run recorded that it had, without a single entry reaching the ledger. The engine cannot
//! know what every action means - posting depreciation belongs to the asset register, closing a month to the periods
//! module - so a step is handed to a [`StepHandler`] the caller supplies. The rule stays a description; the handler is
//! the hands.
//!
//! A handler refuses in a sentence. A refusal stops the run, is recorded as the reason, and is shown to the person whose
//! automation it was. It never becomes a silent success, because a rule that quietly did nothing looks exactly like a
//! rule that worked.
#![forbid(unsafe_code)]

use serde_json::Value;

/// One step of a rule, together with the run it belongs to.
///
/// The run is part of it because a step that posts must be able to name itself to the ledger. `reference()` is that
/// name: the run, the step, and nothing else, so the same step of the same due instant cannot post twice even if a
/// worker is restarted and the rule is walked again.
#[derive(Debug, Clone, Copy)]
pub struct ActionRequest<'a> {
    /// The run this step belongs to: the rule and the instant it was due.
    pub run_id: &'a str,
    /// The step within the rule.
    pub step_id: &'a str,
    /// What the step asks for, as the rule wrote it.
    pub action: &'a str,
    /// The step's own settings.
    pub params: &'a Value,
}

impl ActionRequest<'_> {
    /// The reference a posting carries, so a repeat is refused by the ledger rather than counted twice.
    pub fn reference(&self) -> String {
        format!("{}-{}", self.run_id, self.step_id)
    }

    /// A setting the step carries, as text, or empty when it says nothing.
    pub fn setting(&self, name: &str) -> String {
        self.params
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    }
}

/// The hands a rule is given.
pub trait StepHandler {
    /// Do what the step names, and answer what it did. A refusal is a sentence a person can act on.
    fn act(&self, request: &ActionRequest<'_>) -> Result<Value, String>;
}

/// A handler for a caller that only records: every action is refused by name.
///
/// This exists so a test can walk a rule without a Book, and so the engine has one honest answer for a runner that has
/// no hands: not "completed", but "nothing here can do that".
pub struct NoActions;

impl StepHandler for NoActions {
    fn act(&self, request: &ActionRequest<'_>) -> Result<Value, String> {
        Err(format!(
            "Nothing here can do \"{}\", so step {} was not taken.",
            request.action, request.step_id
        ))
    }
}
