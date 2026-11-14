#![forbid(unsafe_code)]
//! Moving a Book's records out of one store and into another.
//!
//! The store is a trait so a deployment can be given a different database without the 138 services noticing. That is
//! only worth anything if the data can move too, and moving it is not the same problem as reading it: a dump has to be
//! faithful without knowing what any record means.
//!
//! So this module records records and edges as they are, and restores them as they were. It does not re-derive
//! relationships from fields. Two reasons, both learned the hard way elsewhere in this tree: a re-derivation is a second
//! copy of a rule, and a copy drifts; and a dump that only carries what today's rules would rebuild cannot restore a
//! store written by yesterday's.
//!
//! The format is JSON Lines - one record per line, then one edge per line - because it streams, it diffs, and a human
//! can read it when something goes wrong at three in the morning.

use serde_json::{json, Value};

use crate::*;
use crate::{Graph, PlatformRecords, RecordKind, StoreError};

/// Every record kind the platform stores.
///
/// A kind missing from here is a kind a swap silently drops, so the list is held by a test that reads the constants out
/// of this crate's source rather than trusting anybody to remember.
pub const KINDS: &[RecordKind] = &[
    IDEMPOTENCY_CLAIM,
    PRODUCT_COST,
    OVERHEAD_ABSORPTION,
    JOB,
    JOB_COST_ENTRY,
    INSURANCE_CLAIM,
    APPROPRIATION,
    APPROPRIATION_TX,
    TRADING_ACCOUNT,
    CASH_ACCOUNT,
    CASH_SUGGESTION,
    TAX_RATE,
    TAX_REGISTRATION,
    TAX_TRANSACTION,
    TAX_RETURN,
    WITHHOLDING_ENTRY,
    TAX_REPORT,
    DEFERRED_TAX,
    TAX_OBLIGATION,
    TAX_STRATEGY,
    EQUITY_TRANSACTION,
    EQUITY_STATEMENT,
    CASH_FLOW_STATEMENT,
    TREASURY_SNAPSHOT,
    COST_CALCULATION,
    APPORTIONMENT_RULE,
    APPORTIONMENT_RESULT,
    ABSORPTION_RATE,
    OAR_ANALYSIS,
    CVP_ANALYSIS,
    FIXED_ASSET,
    DEPRECIATION_ENTRY,
    ASSET_DISPOSAL,
    INTANGIBLE_ASSET,
    AMORTIZATION_ENTRY,
    DISPOSAL_GROUP,
    DISPOSAL_ASSET,
    IMPAIRMENT_TEST,
    LEASE_TERMINATION,
    TERMINATION_SETTLEMENT,
    DEBT_LOAN,
    TREASURY_COMPLIANCE_CHECK,
    SOVEREIGN_ACCOUNT,
    SOVEREIGN_DEBT,
    FISCAL_POSITION,
    TREASURY_REPORT,
    RISK_EXPOSURE,
    VAR_RESULT,
    STRESS_SCENARIO,
    STRESS_RESULT,
    TREASURY_POLICY,
    POLICY_LIMIT,
    POLICY_CHECK,
    TREASURY_CASH_FLOW,
    TREASURY_CASH_POSITION,
    CASH_TRANSFER,
    LIQUIDITY_POSITION,
    PETTY_CASH_FUND,
    ROLLING_FORECAST,
    SCENARIO,
    SENSITIVITY_ANALYSIS,
    SCENARIO_MODEL,
    MODELING_RULE,
    WHAT_IF_RESULT,
    SENSITIVITY_MODEL_RESULT,
    ZERO_BASED_PACKAGE,
    PERSONAL_RECURRING,
    PERSONAL_DEBT,
    PERSONAL_PAYMENT,
    PERSONAL_INVESTMENT,
    PERSONAL_TRADE,
    PERSONAL_TAX_BRACKET,
    FINANCIAL_GOAL,
    INCOME_SOURCE,
    LEGACY_DEBT_ITEM,
    INVESTMENT_PORTFOLIO,
    INVESTMENT_HOLDING,
    PERFORMANCE_METRIC,
    TRADE_INSTRUMENT,
    CONTINUE_SHUTDOWN_DECISION,
    PETTY_CASH_ENTRY,
    PETTY_CASH_TRANSACTION,
    PETTY_CASH_VOUCHER,
    PETTY_CASH_REPLENISHMENT,
    BANK_RECONCILIATION,
    BANK_STATEMENT,
    BANK_REC_CASH_ENTRY,
    BANK_REC_RECONCILIATION,
    BANK_CONNECTION,
    BANK_FEED_TRANSACTION,
    BANK_FEED_RULE,
    BANK_SYNC,
    BANK_RELATIONSHIP,
    BANK_QUALITY_METRIC,
    CURRENCY,
    EXCHANGE_RATE,
    STATEMENT_OF_AFFAIRS,
    CAPITAL_CALCULATION,
    SOX_CONTROL,
    SOX_CONTROL_TEST,
    SOX_DEFICIENCY,
    RISK_ITEM,
    AUDIT_ENGAGEMENT,
    CONTROL_ACCOUNT,
    RECEIPTS_PAYMENTS,
    SINGLE_ENTRY_CONVERSION,
    PROFIT_ESTIMATION,
    SALES_JOURNAL,
    PURCHASES_JOURNAL,
    CASH_RECEIPTS_JOURNAL,
    CASH_DISBURSEMENTS_JOURNAL,
    SALES_RETURNS_JOURNAL,
    PURCHASES_RETURNS_JOURNAL,
    VENDOR_BILL,
    LEDGER_ACCOUNT,
    JOURNAL_ENTRY,
    STANDARDS_CONFIG,
    ACCOUNT_MAPPING,
    ACCOUNTING_POLICY,
    COMPLIANCE_CHECK,
    DEPARTMENT,
    DEPARTMENT_ALLOCATION_RULE,
    DEPARTMENT_COST_POOL,
    DEPARTMENT_ALLOCATION,
    INTER_DEPARTMENT_BILL,
    DEPARTMENT_FINANCIALS,
    COMPANY,
    COMPANY_SHARE_CAPITAL,
    CAPITAL_TRANSACTION,
    DIVIDEND,
    DIVIDEND_PAYMENT,
    RETAINED_EARNINGS,
    COMPANY_RESERVE,
    CASH_BANK_ACCOUNT,
    CASH_ENTRY,
    CASH_RECONCILIATION,
    CASH_FLOW_ENTRY,
    IFRS_REPORT,
    DISCLOSURE_NOTE,
    INTERCOMPANY_ENTITY,
    INTERCOMPANY_TRANSACTION,
    INTERCOMPANY_ELIMINATION,
    CASH_FLOW_SUMMARY,
    BALANCE_SHEET,
    FUND,
    FUND_TRANSACTION,
    REVENUE_CONTRACT,
    CAPITAL_RECONSTRUCTION,
    RECONSTRUCTION_ADJUSTMENT,
    RESERVE_CONVERSION,
    DEBENTURE_CLASS,
    DEBENTURE_ISSUE,
    DEBENTURE_INTEREST,
    DEBENTURE_REDEMPTION,
    SHARE_ISSUE,
    SHAREHOLDER,
    INITIAL_INVESTMENT,
    SHARE_REDEMPTION,
    FRESH_ISSUE,
    CRR_REQUIREMENT,
    REDEMPTION_TRANSACTION,
    CRR_CREATION,
    CRR_UTILIZATION,
    SHARE_PREMIUM_ENTRY,
    PREMIUM_UTILIZATION,
    PREMIUM_ADJUSTMENT,
    PROFIT_APPROPRIATION,
    INTERIM_DIVIDEND,
    PRIOR_YEAR_ADJUSTMENT,
    GENERAL_RESERVE,
    RESERVE_ALLOCATION,
    RESERVE_UTILIZATION,
    BONUS_ISSUE,
    RIGHT_ISSUE,
    RENUNCIATION,
    ORDINARY_DIVIDEND,
    PREFERENCE_CLASS,
    PREFERENCE_DIVIDEND,
    PREFERENCE_REDEMPTION,
    SHARE_CLASS,
    SHARE_ISSUANCE,
    SHARE_BUYBACK,
    REVALUATION_ENTRY,
    REVALUATION_UTILIZATION,
    CUMULATIVE_REVALUATION,
    ASSET_SCHEDULE,
    TRADING_ACCOUNT_STATEMENT,
    PRODUCTION_COST_STATEMENT,
    NPO_SCALE_ORG,
    NPO_SCALE_BRANCH,
    NPO_SCALE_VENDOR,
    NPO_SCALE_PURCHASE,
    NPO_SCALE_REVENUE,
    NPO_SCALE_EXPENSE,
    NPO_SCALE_RECEIPT,
    NPO_SCALE_DONOR,
    NPO_SCALE_DONATION,
    NPO_SCALE_PLEDGE,
    NPO_SCALE_SHAREHOLDER,
    NPO_SCALE_DIRECTOR,
    NPO_SCALE_DIVIDEND,
    NPO_SCALE_PARTNER,
    NPO_SCALE_PARTNER_DRAW,
    NPO_SCALE_BUDGET,
    NPO_SCALE_COMPLIANCE,
    NPO_SCALE_BALANCE_ITEM,
    NPO_FUND,
    NPO_FUND_TRANSACTION,
    NPO_FUND_RESTRICTION,
    NPO_NET_ASSETS,
    NPO_DONATION,
    NPO_GRANT,
    NPO_GRANT_DRAWDOWN,
    NPO_PROJECT,
    NPO_PROGRAM,
    NPO_DONOR,
    NPO_BUDGET,
    NPO_BUDGET_LINE,
    NPO_INTERNAL_CONTROL,
    NPO_AUDIT_REPORT,
    NPO_PROGRAM_METRIC,
    NPO_IMPACT_MEASUREMENT,
    NPO_VOLUNTEER_RECORD,
    NPO_STATEMENT_OF_ACTIVITIES,
    NPO_STATEMENT_OF_POSITION,
    CREDITOR_TRANSACTION,
    EXPENSE,
    COMMUNITY_GROUP,
    GROUP_MEMBER,
    GROUP_CONTRIBUTION,
    GROUP_PAYOUT,
    PARTNERSHIP_AGREEMENT,
    PARTNER_CHANGE,
    DISSOLUTION_REPORT,
    PARTNERSHIP_REVALUATION,
    BAD_DEBT_RECORD,
    RECOVERY_JOURNAL,
    PROVISION_POLICY,
    DEBTOR_RECORD,
    PROVISION_JOURNAL,
    DOUBTFUL_DEBTOR,
    DOUBTFUL_PROVISION,
    DEBTOR_TRANSACTION,
    RECONCILIATION_ERROR,
    RECONCILIATION_REPORT,
    INTEGRITY_CHECK,
    FINANCIAL_DOCUMENT,
    ACTIVITY,
    ACTIVITY_BUDGET,
    SELLING_PRICE_ANALYSIS,
    REPORT_TEMPLATE,
    REPORT_SCHEDULE,
    GENERATED_REPORT,
    DISTRIBUTION_LIST,
    REPORT_SUBSCRIPTION,
    JOURNAL_LINE,
    DISTRIBUTION_LOG,
];

/// What a dump holds, in the order it was taken.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dump {
    pub records: Vec<Entry>,
    pub edges: Vec<Edge>,
}

/// One stored record: its kind, and its properties exactly as the store held them.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub kind: String,
    pub record: Value,
}

/// One stored edge: what it joins, and what it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: String,
    pub label: String,
    pub to: String,
}

/// How much a restore put back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Restored {
    pub records: usize,
    pub edges: usize,
}

impl Dump {
    /// Take everything the store holds, newest first within each kind.
    ///
    /// `limit_per_kind` caps each kind rather than the whole dump: a cap on the total would stop mid-kind and leave a
    /// dump that looks complete and is not.
    pub fn take<G: Graph + ?Sized>(graph: &G, limit_per_kind: usize) -> Result<Self, StoreError> {
        let mut records = Vec::new();
        for kind in KINDS {
            let rows = PlatformRecords::new(graph, *kind).list(limit_per_kind)?;
            for record in rows {
                records.push(Entry {
                    kind: kind.label.to_string(),
                    record,
                });
            }
        }
        let edges = graph
            .all_edges()?
            .into_iter()
            .map(|(from, label, to)| Edge { from, label, to })
            .collect();
        Ok(Self { records, edges })
    }

    /// The dump as text, one JSON object per line.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for entry in &self.records {
            text.push_str(&json!({"record": entry.kind, "fields": entry.record}).to_string());
            text.push('\n');
        }
        for edge in &self.edges {
            text.push_str(
                &json!({"edge": edge.label, "from": edge.from, "to": edge.to}).to_string(),
            );
            text.push('\n');
        }
        text
    }

    /// Read a dump back from text. Every line is checked, so a truncated or edited file is refused rather than half
    /// loaded.
    pub fn from_text(text: &str) -> Result<Self, StoreError> {
        let mut dump = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parsed: Value = serde_json::from_str(line)
                .map_err(|_| StoreError::Invalid(format!("line {} is not JSON", number + 1)))?;
            if let Some(kind) = parsed.get("record").and_then(Value::as_str) {
                let fields = parsed.get("fields").cloned().ok_or_else(|| {
                    StoreError::Invalid(format!(
                        "line {} names a record with no fields",
                        number + 1
                    ))
                })?;
                dump.records.push(Entry {
                    kind: kind.to_string(),
                    record: fields,
                });
            } else if let Some(label) = parsed.get("edge").and_then(Value::as_str) {
                let from = parsed.get("from").and_then(Value::as_str).ok_or_else(|| {
                    StoreError::Invalid(format!("line {} is an edge from nowhere", number + 1))
                })?;
                let to = parsed.get("to").and_then(Value::as_str).ok_or_else(|| {
                    StoreError::Invalid(format!("line {} is an edge to nowhere", number + 1))
                })?;
                dump.edges.push(Edge {
                    from: from.to_string(),
                    label: label.to_string(),
                    to: to.to_string(),
                });
            } else {
                return Err(StoreError::Invalid(format!(
                    "line {} is neither a record nor an edge",
                    number + 1
                )));
            }
        }
        Ok(dump)
    }

    /// Write the dump into a store, as it was.
    ///
    /// Records go in with their own ids and their own owners, because a restore is not a client creating something: it is
    /// the same rows arriving somewhere else. Edges follow, since an edge needs both ends to exist first.
    pub fn restore<G: Graph + ?Sized>(&self, graph: &G) -> Result<Restored, StoreError> {
        let mut restored = Restored::default();
        for entry in &self.records {
            let kind = KINDS
                .iter()
                .find(|kind| kind.label == entry.kind)
                .ok_or_else(|| {
                    StoreError::Invalid(format!(
                        "this dump holds a {}, which this build does not know",
                        entry.kind
                    ))
                })?;
            PlatformRecords::new(graph, *kind).reinstate(&entry.record)?;
            restored.records += 1;
        }
        for edge in &self.edges {
            graph.create_edge(&edge.from, &edge.label, &edge.to)?;
            restored.edges += 1;
        }
        Ok(restored)
    }

    /// Compare two dumps as sets, so a restore can be checked against what it came from.
    ///
    /// Order is deliberately ignored: a store is free to hand records back in its own order, and a comparison that
    /// cared would fail for a reason that is not a data loss.
    pub fn differences(&self, other: &Self) -> Vec<String> {
        let mut found = Vec::new();
        let key = |entry: &Entry| {
            (
                entry.kind.clone(),
                entry
                    .record
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        };
        let mine: std::collections::BTreeMap<_, _> =
            self.records.iter().map(|e| (key(e), e)).collect();
        let theirs: std::collections::BTreeMap<_, _> =
            other.records.iter().map(|e| (key(e), e)).collect();
        for (id, entry) in &mine {
            match theirs.get(id) {
                None => found.push(format!("{} {} did not arrive", entry.kind, id.1)),
                Some(arrived) if arrived.record != entry.record => {
                    found.push(format!("{} {} arrived changed", entry.kind, id.1))
                }
                Some(_) => {}
            }
        }
        for (id, entry) in &theirs {
            if !mine.contains_key(id) {
                found.push(format!("{} {} was not in the source", entry.kind, id.1));
            }
        }
        let mut left = self.edges.clone();
        let mut right = other.edges.clone();
        let order =
            |a: &Edge, b: &Edge| (&a.from, &a.label, &a.to).cmp(&(&b.from, &b.label, &b.to));
        left.sort_by(order);
        right.sort_by(order);
        if left != right {
            found.push(format!(
                "edges differ: {} left, {} arrived",
                left.len(),
                right.len()
            ));
        }
        found
    }
}
