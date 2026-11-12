//! The record store every service crate reads and writes through.
//!
//! The calculation crates in `crates/calculations` compute and never touch a database. The service
//! crates in `crates/domains` hold records, and they all do it through this one crate rather than
//! each carrying its own copy of the connection and the scoping rules. This crate is the machinery
//! only: the record shapes, the Book scoping and the Neo4j driver. No service's arithmetic lives
//! here. The Python layer wrote the same shape, so existing data stays readable:
//!
//! ```text
//! (u:User {id: $user_id})-[:OWNS_X]->(x:Label {..., user_id, book_id})
//! ```
//!
//! Two rules are enforced here rather than left to each caller:
//!
//! * every write is stamped with the caller's `user_id` and `book_id`, and a payload cannot
//!   overwrite either, so one Book can never write into another;
//! * every read is filtered by the same pair, so a record belonging to another Book is a miss
//!   rather than a leak.
//!
//! Values travel as query parameters, never as formatted Cypher. That is what keeps a product
//! name like `Menswear (Q3)` from breaking the statement or injecting one.

#![forbid(unsafe_code)]

use serde_json::{json, Map, Value};

pub mod amounts;
/// A graph that records what it was asked and answers from a script, for the tests of the crates
/// that sit on this one.
#[doc(hidden)]
pub mod fake;
pub mod neo4j_client;

pub use neo4j_client::Neo4jGraph;

/// Who is asking, and which Book they are asking about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookScope {
    pub user_id: String,
    pub book_id: String,
}

impl BookScope {
    pub fn new(user_id: impl Into<String>, book_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            book_id: book_id.into(),
        }
    }

    fn params(&self) -> Value {
        json!({"user_id": self.user_id, "book_id": self.book_id})
    }
}

/// A stored record type: the node label and the edge the owning user reaches it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordKind {
    pub label: &'static str,
    pub edge: &'static str,
}

impl RecordKind {
    pub const fn new(label: &'static str, edge: &'static str) -> Self {
        Self { label, edge }
    }
}

/// The records absorption-costing-service used to own.
pub const PRODUCT_COST: RecordKind = RecordKind::new("ProductCost", "OWNS_PRODUCT_COST");
pub const OVERHEAD_ABSORPTION: RecordKind =
    RecordKind::new("OverheadAbsorption", "OWNS_OVERHEAD_ABSORPTION");

/// The records job-costing-service used to own.
pub const JOB: RecordKind = RecordKind::new("Job", "OWNS_JOB");
pub const JOB_COST_ENTRY: RecordKind = RecordKind::new("JobCostEntry", "HAS_COST");

/// The records insurance-claims-service used to own.
pub const INSURANCE_CLAIM: RecordKind = RecordKind::new("InsuranceClaim", "OWNS_CLAIM");

/// The records treasury-analytics-service used to own.
/// The records cash-flow-statement-service used to own.
/// The records equity-changes-service used to own.
/// The records cash-optimization-service used to own.
/// The records trading-account-service used to own.
/// The records appropriation-control-service used to own.
pub const APPROPRIATION: RecordKind = RecordKind::new("Appropriation", "OWNS_APPROPRIATION");
pub const APPROPRIATION_TX: RecordKind =
    RecordKind::new("AppropriationTransaction", "OWNS_APPROPRIATION_TX");

pub const TRADING_ACCOUNT: RecordKind = RecordKind::new("TradingAccount", "OWNS_TRADING_ACCOUNT");

pub const CASH_ACCOUNT: RecordKind = RecordKind::new("CashAccount", "OWNS_ACCOUNT");
pub const CASH_SUGGESTION: RecordKind =
    RecordKind::new("OptimizationSuggestion", "OWNS_SUGGESTION");

pub const EQUITY_TRANSACTION: RecordKind = RecordKind::new("EquityTransaction", "OWNS_TRANSACTION");
pub const EQUITY_STATEMENT: RecordKind = RecordKind::new("EquityStatement", "OWNS_STATEMENT");

pub const CASH_FLOW_STATEMENT: RecordKind = RecordKind::new("CashFlowStatement", "OWNS_STATEMENT");

pub const TREASURY_SNAPSHOT: RecordKind =
    RecordKind::new("TreasuryAnalyticsSnapshot", "OWNS_SNAPSHOT");

/// The records process-costing-service and product-costing-service both own.
///
/// The two services are the same thing twice: same models, same endpoints, same label, and a list
/// query that filters by company only, so each one already answers with the other's records. One
/// constant serves both, because splitting the label would invent a distinction the data never made.
pub const COST_CALCULATION: RecordKind = RecordKind::new("CostCalculation", "OWNS_CALCULATION");

/// The apportionment rules basis-apportionment-service owns.
pub const APPORTIONMENT_RULE: RecordKind =
    RecordKind::new("ApportionmentRule", "OWNS_APPORTIONMENT_RULE");

/// The apportionments that service has made.
pub const APPORTIONMENT_RESULT: RecordKind =
    RecordKind::new("ApportionmentResult", "OWNS_APPORTIONMENT_RESULT");

/// The absorption rates overhead-absorption-rate-service owns.
pub const ABSORPTION_RATE: RecordKind =
    RecordKind::new("OverheadAbsorptionRate", "OWNS_ABSORPTION_RATE");

/// The rate calculations that same service records beside them.
pub const OAR_ANALYSIS: RecordKind = RecordKind::new("OARAnalysis", "OWNS_OAR_ANALYSIS");

/// The cost-volume-profit analyses cvp-analysis-service owns.
pub const CVP_ANALYSIS: RecordKind = RecordKind::new("CVPAnalysis", "OWNS_CVP_ANALYSIS");

/// The fixed asset register fixed-assets-register-service owns.
pub const FIXED_ASSET: RecordKind = RecordKind::new("FixedAsset", "OWNS_FIXED_ASSET");
pub const DEPRECIATION_ENTRY: RecordKind =
    RecordKind::new("DepreciationEntry", "OWNS_DEPRECIATION_ENTRY");
pub const ASSET_DISPOSAL: RecordKind = RecordKind::new("AssetDisposal", "OWNS_ASSET_DISPOSAL");
/// The intangibles intangible-assets-service owns.
pub const INTANGIBLE_ASSET: RecordKind =
    RecordKind::new("IntangibleAsset", "OWNS_INTANGIBLE_ASSET");
pub const AMORTIZATION_ENTRY: RecordKind =
    RecordKind::new("AmortizationEntry", "OWNS_AMORTIZATION_ENTRY");

/// The records revaluation-reserve-service owns.
pub const REVALUATION_ENTRY: RecordKind =
    RecordKind::new("RevaluationEntry", "OWNS_REVALUATION_ENTRY");
pub const REVALUATION_UTILIZATION: RecordKind =
    RecordKind::new("RevaluationUtilization", "OWNS_REVALUATION_UTILIZATION");
pub const CUMULATIVE_REVALUATION: RecordKind =
    RecordKind::new("CumulativeRevaluation", "OWNS_CUMULATIVE_REVALUATION");

/// The schedules fixed-assets-schedule-service generated from that register.
pub const ASSET_SCHEDULE: RecordKind = RecordKind::new("AssetSchedule", "OWNS_ASSET_SCHEDULE");

/// The price reduction decisions selling-price-reduction-service owns.
pub const SELLING_PRICE_ANALYSIS: RecordKind =
    RecordKind::new("PriceReductionAnalysis", "OWNS_PRICE_ANALYSIS");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The record does not exist, or belongs to another Book. Deliberately the same answer.
    NotFound,
    Graph(String),
    Invalid(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "record not found"),
            Self::Graph(message) => write!(f, "graph error: {message}"),
            Self::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// The one thing the store needs from a database: run a parameterised statement, get rows back.
///
/// Keeping this to a single method is what lets the store be tested without a server, and keeps
/// the driver quiet in exactly one file.
pub trait Graph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError>;
}

/// Book-scoped CRUD over one record kind.
pub struct Records<'a, G: Graph + ?Sized> {
    graph: &'a G,
    kind: RecordKind,
}

impl<'a, G: Graph + ?Sized> Records<'a, G> {
    pub fn new(graph: &'a G, kind: RecordKind) -> Self {
        Self { graph, kind }
    }

    /// Store a new record, stamped with the caller's scope.
    ///
    /// A payload that tries to set `user_id` or `book_id` is rejected rather than silently
    /// overwritten: a caller that thinks it is writing somewhere else should hear about it.
    pub fn create(&self, scope: &BookScope, fields: &Value) -> Result<Value, StoreError> {
        let mut props = self.clean_fields(fields)?;
        props.insert("user_id".into(), json!(scope.user_id));
        props.insert("book_id".into(), json!(scope.book_id));
        let id = props
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uuid_like(&scope.book_id, props.len()));
        props.insert("id".into(), json!(id));

        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})\n\
             CREATE (x:{label})\n\
             SET x = $props\n\
             CREATE (u)-[:{edge}]->(x)\n\
             RETURN x",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["props"] = Value::Object(props);
        self.one(cypher, params)
    }

    pub fn get(&self, scope: &BookScope, id: &str) -> Result<Value, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             RETURN x",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        self.one(cypher, params)
    }

    pub fn list(&self, scope: &BookScope, limit: usize) -> Result<Vec<Value>, StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.book_id = $book_id\n\
             RETURN x\n\
             ORDER BY x.created_at DESC\n\
             LIMIT $limit",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["limit"] = json!(limit as i64);
        let rows = self.graph.run(&cypher, params)?;
        Ok(rows.into_iter().map(unwrap_node).collect())
    }

    /// Merge the supplied fields into a record that already belongs to this Book.
    pub fn update(&self, scope: &BookScope, id: &str, fields: &Value) -> Result<Value, StoreError> {
        let patch = self.clean_fields(fields)?;
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             SET x += $patch\n\
             RETURN x",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        params["patch"] = Value::Object(patch);
        self.one(cypher, params)
    }

    pub fn delete(&self, scope: &BookScope, id: &str) -> Result<(), StoreError> {
        let cypher = format!(
            "MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})\n\
             WHERE x.id = $id AND x.book_id = $book_id\n\
             DETACH DELETE x\n\
             RETURN count(x) AS deleted",
            label = self.kind.label,
            edge = self.kind.edge
        );
        let mut params = scope.params();
        params["id"] = json!(id);
        let rows = self.graph.run(&cypher, params)?;
        // DETACH DELETE returns no row for a miss, so an empty answer is the same NotFound as get.
        if rows.is_empty() {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    fn clean_fields(&self, fields: &Value) -> Result<Map<String, Value>, StoreError> {
        let object = fields
            .as_object()
            .ok_or_else(|| StoreError::Invalid("a record must be a JSON object".into()))?;
        if object.contains_key("user_id") || object.contains_key("book_id") {
            return Err(StoreError::Invalid(
                "user_id and book_id come from the caller's scope, not the payload".into(),
            ));
        }
        Ok(object.clone())
    }

    fn one(&self, cypher: impl AsRef<str>, params: Value) -> Result<Value, StoreError> {
        let rows = self.graph.run(cypher.as_ref(), params)?;
        rows.into_iter()
            .next()
            .map(unwrap_node)
            .ok_or(StoreError::NotFound)
    }
}

/// A row is `{"x": {...}}`; the record is what is inside it.
fn unwrap_node(row: Value) -> Value {
    if let Some(object) = row.as_object() {
        if object.len() == 1 {
            if let Some((_, node)) = object.iter().next() {
                return node.clone();
            }
        }
    }
    row
}

/// A stable local id. Not a real UUID: the store only needs uniqueness within a Book, and
/// pulling in a uuid dependency for that is not worth it.
fn uuid_like(book_id: &str, salt: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mixed = (nanos as u64) ^ ((salt as u64) << 32) ^ book_id.len() as u64;
    format!("{mixed:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records every statement so the test can assert on the Cypher and the parameters.
    struct FakeGraph {
        rows: RefCell<Vec<Value>>,
        seen: RefCell<Vec<(String, Value)>>,
    }

    impl FakeGraph {
        fn returning(rows: Vec<Value>) -> Self {
            Self {
                rows: RefCell::new(rows),
                seen: RefCell::new(Vec::new()),
            }
        }

        fn last(&self) -> (String, Value) {
            self.seen
                .borrow()
                .last()
                .cloned()
                .expect("no statement ran")
        }
    }

    impl Graph for FakeGraph {
        fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
            self.seen.borrow_mut().push((cypher.to_owned(), params));
            Ok(self.rows.borrow().clone())
        }
    }

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_new_record_is_stamped_with_the_callers_scope() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "abc", "book_id": "book-1"}})]);
        let store = Records::new(&graph, PRODUCT_COST);
        let saved = store
            .create(&scope(), &json!({"name": "Menswear (Q3)", "cost": 1200.0}))
            .unwrap();
        assert_eq!(saved["book_id"], json!("book-1"));

        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:ProductCost)"), "{cypher}");
        assert!(cypher.contains("[:OWNS_PRODUCT_COST]"), "{cypher}");
        assert_eq!(params["props"]["user_id"], json!("user-1"));
        assert_eq!(params["props"]["book_id"], json!("book-1"));
    }

    #[test]
    fn a_payload_cannot_choose_its_own_book() {
        let graph = FakeGraph::returning(vec![]);
        let store = Records::new(&graph, PRODUCT_COST);
        let err = store
            .create(&scope(), &json!({"book_id": "someone-elses-book"}))
            .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{err:?}");
        assert_eq!(
            graph.seen.borrow().len(),
            0,
            "nothing should reach the database"
        );
    }

    #[test]
    fn values_never_land_in_the_statement_text() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "abc"}})]);
        let store = Records::new(&graph, PRODUCT_COST);
        let tricky = "Menswear'); MATCH (n) DETACH DELETE n //";
        store.create(&scope(), &json!({"name": tricky})).unwrap();
        let (cypher, params) = graph.last();
        assert!(!cypher.contains("Menswear"), "the value was interpolated");
        assert_eq!(params["props"]["name"], json!(tricky));
    }

    #[test]
    fn reads_are_confined_to_the_callers_book() {
        let graph = FakeGraph::returning(vec![]);
        let store = Records::new(&graph, JOB);
        assert_eq!(
            store.get(&scope(), "job-9").unwrap_err(),
            StoreError::NotFound
        );
        let (cypher, params) = graph.last();
        assert!(cypher.contains("x.book_id = $book_id"), "{cypher}");
        assert_eq!(params["book_id"], json!("book-1"));
        assert_eq!(params["user_id"], json!("user-1"));
    }

    #[test]
    fn an_update_cannot_move_a_record_between_books() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "job-9"}})]);
        let store = Records::new(&graph, JOB);
        let err = store
            .update(&scope(), "job-9", &json!({"book_id": "book-2"}))
            .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{err:?}");
    }

    #[test]
    fn a_delete_that_matches_nothing_is_a_miss_not_a_success() {
        let graph = FakeGraph::returning(vec![]);
        let store = Records::new(&graph, JOB_COST_ENTRY);
        assert_eq!(
            store.delete(&scope(), "entry-1").unwrap_err(),
            StoreError::NotFound
        );
    }

    #[test]
    fn listing_is_newest_first_and_bounded() {
        let graph =
            FakeGraph::returning(vec![json!({"x": {"id": "a"}}), json!({"x": {"id": "b"}})]);
        let store = Records::new(&graph, OVERHEAD_ABSORPTION);
        let rows = store.list(&scope(), 50).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], json!("a"));
        let (cypher, params) = graph.last();
        assert!(cypher.contains("ORDER BY x.created_at DESC"), "{cypher}");
        assert_eq!(params["limit"], json!(50));
    }
}
