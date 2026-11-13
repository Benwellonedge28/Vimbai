#![forbid(unsafe_code)]
//! performance-benchmarking-service: the metrics an industry is measured on, where one Book falls against them, and the
//! peers it is compared with.
//!
//! The comparison is the point, so the ladder that places a Book is kept exactly as the service drew it, including the
//! places where it is generous with itself:
//!
//! * the bands are inclusive at their lower edge, so a figure exactly on the industry top quartile is top quartile and
//!   not merely above average. A Book that matches the leaders is reported as matching them;
//! * each band's span is measured against the NEXT band's edge, and a guard of one keeps a flat industry - where the
//!   quartiles sit on top of each other - from dividing by nothing. A flat industry therefore places a Book inside the
//!   band rather than at infinity;
//! * the top band is NOT clamped at a hundred. A Book far above its industry reports a percentile above a hundred, and
//!   that is pinned here rather than capped: capping would make the leaders indistinguishable, and a reader who wants to
//!   know how far ahead the Book is would lose the only number that says so;
//! * the bottom band IS clamped at zero, because a negative percentile says nothing a reader can act on;
//! * a peer figure is accepted for a metric nobody has defined. That looks like a gap and is kept: peers are other
//!   businesses' numbers, often gathered before the metric was written down here, and refusing them would throw away
//!   evidence that arrived first.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{number, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A metric an industry is measured on.
pub const METRIC: RecordKind = RecordKind::new("BenchmarkMetric", "OWNS_METRIC");
/// One Book's figure placed against a metric.
pub const RESULT: RecordKind = RecordKind::new("BenchmarkResult", "OWNS_RESULT");
/// Another business's figure on the same metric.
pub const PEER: RecordKind = RecordKind::new("PeerComparison", "OWNS_PEER");

/// The four ways a metric is classified. Anything else is refused by name.
pub const CATEGORIES: [&str; 4] = ["financial", "operational", "customer", "growth"];

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

/// Where a figure falls against a metric, as the shipped service placed it.
///
/// The rating is the band the figure landed in and the percentile is its position inside that band. Returned together
/// because they are one answer and splitting them invites the two to disagree.
pub fn place(org_value: f64, median: f64, top: f64, bottom: f64) -> (f64, &'static str) {
    let (percentile, rating) = if org_value >= top {
        (
            75.0 + ((org_value - top) / top.max(1.0)) * 25.0,
            "top_quartile",
        )
    } else if org_value >= median {
        (
            50.0 + ((org_value - median) / (top - median).max(1.0)) * 25.0,
            "above_average",
        )
    } else if org_value >= bottom {
        (
            25.0 + ((org_value - bottom) / (median - bottom).max(1.0)) * 25.0,
            "average",
        )
    } else {
        // a negative percentile says nothing a reader can act on, so the floor is zero
        (
            (((org_value - bottom) / bottom.abs().max(1.0)) * 25.0).max(0.0),
            "below_average",
        )
    };
    // two places, as shipped: a benchmark is read, not recomputed
    let rounded = (percentile * 100.0).round() / 100.0;
    (rounded, rating)
}

/// `POST /metrics`: define a benchmark metric.
pub fn create_metric(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let category = text(payload, "category");
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(StoreError::BadRequest(format!(
            "Invalid category. Must be one of {CATEGORIES:?}"
        )));
    }
    Records::new(graph, METRIC).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": text(payload, "name"),
            "category": category,
            "unit": text(payload, "unit"),
            "industry_median": number(payload, "industry_median"),
            "industry_top_quartile": number(payload, "industry_top_quartile"),
            "industry_bottom_quartile": number(payload, "industry_bottom_quartile"),
            "description": text(payload, "description"),
            "created_at": stamp(payload),
        }),
    )
}

/// `GET /metrics`: the metrics this Book keeps, narrowed by category when asked.
pub fn benchmark_metrics(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "category");
    let rows = Records::new(graph, METRIC).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted.is_empty() || text(row, "category") == wanted)
            .collect(),
    ))
}

/// `POST /benchmark`: place a Book's figure against a metric it defined.
pub fn benchmark(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let metric_id = text(payload, "metric_id");
    let held = Records::new(graph, METRIC).get(scope, &metric_id)?;
    let org_value = number(payload, "organization_value");
    let (percentile, rating) = place(
        org_value,
        number(&held, "industry_median"),
        number(&held, "industry_top_quartile"),
        number(&held, "industry_bottom_quartile"),
    );
    Records::new(graph, RESULT).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "metric_id": metric_id,
            "organization_value": org_value,
            "industry_median": number(&held, "industry_median"),
            "percentile_rank": percentile,
            "rating": rating,
            "period": text(payload, "period"),
            "created_at": stamp(payload),
        }),
    )
}

/// `GET /results`: the placements this Book made, newest slice last.
///
/// The shipped service listed, then kept the TRAILING `limit` rows. That is kept rather than corrected: a caller asking
/// for the latest five of a listing that arrives oldest first only gets the latest five this way.
pub fn benchmark_results(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "metric_id");
    let limit = match payload.get("limit") {
        Some(_) => number(payload, "limit").max(0.0) as usize,
        None => 50,
    };
    let rows = Records::new(graph, RESULT).list(scope, 500)?;
    let matching: Vec<Value> = rows
        .into_iter()
        .filter(|row| wanted.is_empty() || text(row, "metric_id") == wanted)
        .collect();
    let start = matching.len().saturating_sub(limit);
    Ok(Value::Array(matching[start..].to_vec()))
}

/// `POST /peer-comparisons`: record another business's figure on a metric.
pub fn add_peer_comparison(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, PEER).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "metric_id": text(payload, "metric_id"),
            "organization_name": text(payload, "organization_name"),
            "value": number(payload, "value"),
            "period": text(payload, "period"),
            "created_at": stamp(payload),
        }),
    )
}

/// `GET /peer-comparisons/{metric_id}`: the peers held for one metric, narrowed by period when asked.
pub fn peer_comparisons(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    metric_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted_period = text(payload, "period");
    let rows = Records::new(graph, PEER).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| text(row, "metric_id") == metric_id)
            .filter(|row| wanted_period.is_empty() || text(row, "period") == wanted_period)
            .collect(),
    ))
}

/// A label for a figure, so a reading of several metrics can be sorted without re-deriving the band.
pub fn rating_label(percentile: f64) -> &'static str {
    if percentile >= 75.0 {
        "top_quartile"
    } else if percentile >= 50.0 {
        "above_average"
    } else if percentile >= 25.0 {
        "average"
    } else {
        "below_average"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn metric(graph: &MemoryGraph) -> Value {
        create_metric(
            graph,
            &talked(),
            &json!({"name": "return on assets", "category": "financial", "unit": "percentage",
                    "industry_median": 10.0, "industry_top_quartile": 20.0,
                    "industry_bottom_quartile": 5.0, "now": 1000.0}),
        )
        .expect("a metric")
    }

    #[test]
    fn a_figure_is_placed_in_the_band_it_lands_in() {
        // the bands are inclusive at their lower edge, so matching the leaders is top quartile
        assert_eq!(place(20.0, 10.0, 20.0, 5.0), (75.0, "top_quartile"));
        assert_eq!(place(10.0, 10.0, 20.0, 5.0), (50.0, "above_average"));
        assert_eq!(place(5.0, 10.0, 20.0, 5.0), (25.0, "average"));
        // the middle of each band is the middle of its span
        assert_eq!(place(15.0, 10.0, 20.0, 5.0), (62.5, "above_average"));
        assert_eq!(place(7.5, 10.0, 20.0, 5.0), (37.5, "average"));
        // far ahead of the industry the percentile runs past a hundred, and is not capped:
        // capping would make the leaders indistinguishable
        let (ahead, rating) = place(30.0, 10.0, 20.0, 5.0);
        assert_eq!(rating, "top_quartile");
        assert_eq!(ahead, 87.5);
        let (far, _) = place(60.0, 10.0, 20.0, 5.0);
        assert!(far > 100.0, "{far}");
        // behind the bottom quartile the floor is zero, because a negative percentile is not readable
        assert_eq!(place(0.0, 10.0, 20.0, 5.0), (0.0, "below_average"));
        assert_eq!(place(-5.0, 10.0, 20.0, 5.0), (0.0, "below_average"));
        // a flat industry does not divide by nothing: the guard of one holds every band
        let flat = place(50.0, 50.0, 50.0, 50.0);
        assert_eq!(flat, (75.0, "top_quartile"));
        let (near, rating) = place(49.0, 50.0, 50.0, 50.0);
        assert_eq!(rating, "below_average");
        assert_eq!(near, 0.0);
    }

    #[test]
    fn a_book_is_benchmarked_against_a_metric_it_defined() {
        let graph = MemoryGraph::new();
        let defined = metric(&graph);
        let id = text(&defined, "id");
        assert_eq!(defined["industry_median"], json!(10.0));
        assert_eq!(defined["category"], json!("financial"));

        let placed = benchmark(
            &graph,
            &talked(),
            &json!({"metric_id": id, "organization_value": 15.0, "period": "2026-Q1", "now": 2000.0}),
        )
        .expect("a placement");
        assert_eq!(placed["percentile_rank"], json!(62.5));
        assert_eq!(placed["rating"], json!("above_average"));
        assert_eq!(placed["industry_median"], json!(10.0));
        assert_eq!(placed["organization_value"], json!(15.0));
        assert_eq!(placed["period"], json!("2026-Q1"));

        // a metric nobody defined cannot place anything, and another Book's metric is nobody's here
        assert!(benchmark(
            &graph,
            &talked(),
            &json!({"org_value": 1.0, "metric_id": "nope"})
        )
        .is_err());
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(benchmark(
            &graph,
            &elsewhere,
            &json!({"metric_id": id, "organization_value": 15.0})
        )
        .is_err());
        assert!(
            benchmark_metrics(&graph, &elsewhere, &json!({}))
                .expect("metrics")
                .as_array()
                .map(Vec::len)
                == Some(0)
        );

        // a category nobody agreed on is a bad request, and the listing narrows by the four that stand
        assert!(matches!(
            create_metric(
                &graph,
                &talked(),
                &json!({"name": "x", "category": "sentiment"})
            ),
            Err(StoreError::BadRequest(_))
        ));
        create_metric(
            &graph,
            &talked(),
            &json!({"name": "churn", "category": "customer"}),
        )
        .expect("a metric");
        let financial = benchmark_metrics(&graph, &talked(), &json!({"category": "financial"}))
            .expect("metrics");
        assert_eq!(financial.as_array().map(Vec::len), Some(1));
        let all = benchmark_metrics(&graph, &talked(), &json!({})).expect("metrics");
        assert_eq!(all.as_array().map(Vec::len), Some(2));

        // the results listing keeps the trailing slice, and narrows by metric
        for value in [1.0, 2.0, 3.0] {
            benchmark(
                &graph,
                &talked(),
                &json!({"metric_id": id, "organization_value": value, "period": "2026-Q2"}),
            )
            .expect("a placement");
        }
        let latest = benchmark_results(&graph, &talked(), &json!({"limit": 2})).expect("results");
        let latest = latest.as_array().expect("results");
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0]["organization_value"], json!(2.0));
        assert_eq!(latest[1]["organization_value"], json!(3.0));
        let by_metric =
            benchmark_results(&graph, &talked(), &json!({"metric_id": "other"})).expect("results");
        assert_eq!(by_metric.as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn a_peer_figure_survives_the_metric_it_arrived_before() {
        let graph = MemoryGraph::new();
        // peers are other businesses' numbers, often gathered before the metric was written down here
        let recorded = add_peer_comparison(
            &graph,
            &talked(),
            &json!({"metric_id": "roa", "organization_name": "Moyo Traders", "value": 12.5,
                    "period": "2026-Q1", "now": 1000.0}),
        )
        .expect("a peer");
        assert_eq!(recorded["organization_name"], json!("Moyo Traders"));
        assert_eq!(recorded["value"], json!(12.5));
        add_peer_comparison(
            &graph,
            &talked(),
            &json!({"metric_id": "roa", "organization_name": "Chuma Ltd", "value": 8.0, "period": "2026-Q2"}),
        )
        .expect("a peer");

        // the metric is only defined now, and the peers collected earlier are still answered
        let defined = metric(&graph);
        assert_eq!(text(&defined, "name"), "return on assets");
        let peers = peer_comparisons(&graph, &talked(), "roa", &json!({})).expect("peers");
        assert_eq!(peers.as_array().map(Vec::len), Some(2));
        let first_quarter =
            peer_comparisons(&graph, &talked(), "roa", &json!({"period": "2026-Q1"}))
                .expect("peers");
        assert_eq!(first_quarter.as_array().map(Vec::len), Some(1));
        let unknown = peer_comparisons(&graph, &talked(), "ebitda", &json!({})).expect("peers");
        assert_eq!(unknown.as_array().map(Vec::len), Some(0));
        // and one Book never reads another's peers
        let elsewhere = BookScope::new("user-2", "book-2");
        assert_eq!(
            peer_comparisons(&graph, &elsewhere, "roa", &json!({}))
                .expect("peers")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        // a label says what a percentile means, without re-deriving the band
        assert_eq!(rating_label(90.0), "top_quartile");
        assert_eq!(rating_label(60.0), "above_average");
        assert_eq!(rating_label(30.0), "average");
        assert_eq!(rating_label(10.0), "below_average");
    }
}
