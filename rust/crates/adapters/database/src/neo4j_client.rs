//! The Neo4j driver adapter. Everything the rest of the crate knows about Bolt lives here.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use neo4j::address::Address;
use neo4j::driver::auth::AuthToken;
use neo4j::driver::Record;
use neo4j::driver::{ConnectionConfig, Driver, DriverConfig};
use neo4j::retry::ExponentialBackoff;
use neo4j::{ValueReceive, ValueSend};
use serde_json::{json, Map, Value};

use crate::{Graph, StoreError};

/// Where to connect, and as whom. Read from the environment so a deployment configures it the
/// same way it configures every other service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neo4jConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl Neo4jConfig {
    /// `NEO4J_URI` (bolt://host:port or host:port) with `NEO4J_USER`, `NEO4J_PASSWORD` and
    /// `NEO4J_DATABASE`, defaulting to the same values the Python services use.
    pub fn from_env() -> Result<Self, StoreError> {
        let uri = std::env::var("NEO4J_URI")
            .or_else(|_| std::env::var("NEO4J_URL"))
            .unwrap_or_else(|_| "bolt://localhost:7687".to_owned());
        let password = std::env::var("NEO4J_PASSWORD")
            .map_err(|_| StoreError::Invalid("NEO4J_PASSWORD is not set".into()))?;
        let mut config = Self::parse_uri(&uri)?;
        config.user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_owned());
        config.password = password;
        config.database = std::env::var("NEO4J_DATABASE").unwrap_or_else(|_| "neo4j".to_owned());
        Ok(config)
    }

    pub fn parse_uri(uri: &str) -> Result<Self, StoreError> {
        let without_scheme = uri
            .strip_prefix("bolt://")
            .or_else(|| uri.strip_prefix("neo4j://"))
            .unwrap_or(uri);
        let (host, port) = match without_scheme.rsplit_once(':') {
            Some((host, port)) => (
                host.to_owned(),
                port.trim_end_matches('/')
                    .parse::<u16>()
                    .map_err(|_| StoreError::Invalid(format!("unusable port in {uri}")))?,
            ),
            None => (without_scheme.trim_end_matches('/').to_owned(), 7687),
        };
        if host.is_empty() {
            return Err(StoreError::Invalid(format!("no host in {uri}")));
        }
        Ok(Self {
            host,
            port,
            user: "neo4j".to_owned(),
            password: String::new(),
            database: "neo4j".to_owned(),
        })
    }
}

/// A connected driver, held for the life of the process: the driver pools its own connections.
pub struct Neo4jGraph {
    driver: Driver,
    database: std::sync::Arc<String>,
}

impl Neo4jGraph {
    pub fn connect(config: &Neo4jConfig) -> Self {
        let address = Address::from((config.host.as_str(), config.port));
        let auth = AuthToken::new_basic_auth(config.user.as_str(), config.password.as_str());
        let driver = Driver::new(
            ConnectionConfig::new(address),
            DriverConfig::new().with_auth(std::sync::Arc::new(auth)),
        );
        Self {
            driver,
            database: std::sync::Arc::new(config.database.clone()),
        }
    }

    pub fn from_env() -> Result<Self, StoreError> {
        Ok(Self::connect(&Neo4jConfig::from_env()?))
    }
}

impl Graph for Neo4jGraph {
    fn run(&self, cypher: &str, params: Value) -> Result<Vec<Value>, StoreError> {
        let result = self
            .driver
            .execute_query(cypher)
            .with_database(self.database.clone())
            .with_parameters(to_send_map(&params)?)
            .run_with_retry(ExponentialBackoff::default())
            .map_err(|error| StoreError::Graph(error.to_string()))?;

        Ok(result.records.into_iter().map(row_to_json).collect())
    }

    fn all_edges(&self) -> Result<Vec<(String, String, String)>, StoreError> {
        // `a.id`/`b.id` rather than the nodes: the id is the platform's identity, and it is what a dump can name
        let rows = self.run(
            "MATCH (a)-[r]->(b)\nRETURN a.id AS from_id, type(r) AS label, b.id AS to_id",
            json!({}),
        )?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_owned);
                Some((text("from_id")?, text("label")?, text("to_id")?))
            })
            .collect())
    }

    fn create_edge(&self, from: &str, label: &str, to: &str) -> Result<(), StoreError> {
        // the label is an identifier in Cypher and cannot be a parameter, so it is checked instead of interpolated
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(StoreError::Invalid(format!("{label} is not an edge name")));
        }
        self.run(
            &format!(
                "MATCH (a)\nWHERE a.id = $from\nMATCH (b)\nWHERE b.id = $to\nMERGE (a)-[:{label}]->(b)"
            ),
            json!({"from": from, "to": to}),
        )?;
        Ok(())
    }
}

fn to_send_map(params: &Value) -> Result<HashMap<String, ValueSend>, StoreError> {
    let object = params
        .as_object()
        .ok_or_else(|| StoreError::Invalid("parameters must be an object".into()))?;
    Ok(object
        .iter()
        .map(|(key, value)| (key.clone(), to_send(value)))
        .collect())
}

fn to_send(value: &Value) -> ValueSend {
    match value {
        Value::Null => ValueSend::Null,
        Value::Bool(flag) => ValueSend::Boolean(*flag),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                ValueSend::Integer(int)
            } else {
                ValueSend::Float(number.as_f64().unwrap_or_default())
            }
        }
        Value::String(text) => ValueSend::String(text.clone()),
        Value::Array(items) => ValueSend::List(items.iter().map(to_send).collect()),
        Value::Object(map) => {
            ValueSend::Map(to_send_map(&Value::Object(map.clone())).unwrap_or_default())
        }
    }
}

/// One record as a JSON object, so the store above never sees a driver type.
fn row_to_json(mut record: Record) -> Value {
    let keys: Vec<std::sync::Arc<String>> = record.keys().collect();
    let mut object = Map::new();
    for key in keys {
        if let Some(value) = record.take_value(key.as_str()) {
            object.insert(key.as_str().to_owned(), to_json(value));
        }
    }
    Value::Object(object)
}

fn to_json(value: ValueReceive) -> Value {
    match value {
        ValueReceive::Null => Value::Null,
        ValueReceive::Boolean(flag) => Value::Bool(flag),
        ValueReceive::Integer(number) => Value::from(number),
        ValueReceive::Float(number) => Value::from(number),
        ValueReceive::List(items) => Value::Array(items.into_iter().map(to_json).collect()),
        ValueReceive::Map(map) => {
            let mut object = Map::new();
            for (key, item) in map {
                object.insert(key, to_json(item));
            }
            Value::Object(object)
        }
        ValueReceive::Node(node) => {
            let mut object = Map::new();
            for (key, item) in node.properties {
                object.insert(key, to_json(item));
            }
            Value::Object(object)
        }
        // Dates, points, paths and the like are not something a caller can act on yet, so they
        // are reported as text rather than guessed at.
        other => Value::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uri_is_read_with_or_without_a_scheme() {
        let with = Neo4jConfig::parse_uri("bolt://neo4j.internal:7688").unwrap();
        assert_eq!(with.host, "neo4j.internal");
        assert_eq!(with.port, 7688);
        let without = Neo4jConfig::parse_uri("neo4j.internal").unwrap();
        assert_eq!(without.port, 7687);
        let missing = Neo4jConfig::parse_uri("bolt://");
        assert!(missing.is_err(), "a host is required");
    }

    #[test]
    fn parameters_cross_the_boundary_as_typed_values() {
        let params = serde_json::json!({"id": "abc", "limit": 50, "nested": {"cost": 1.5}});
        let sent = to_send_map(&params).unwrap();
        assert_eq!(sent["id"], ValueSend::String("abc".into()));
        assert_eq!(sent["limit"], ValueSend::Integer(50));
        assert!(matches!(sent["nested"], ValueSend::Map(_)));
    }
}
