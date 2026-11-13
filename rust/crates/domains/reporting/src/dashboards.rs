#![forbid(unsafe_code)]
//! dashboard-service: the figures a company looks at, laid out as widgets.
//!
//! The four dashboards are fixed layouts of named widgets, and every figure on them is nil: the shipped service
//! carried no query and no arithmetic, so it answered a shape rather than a reading. Ported as shipped, with that
//! said out loud in the answer rather than left for a reader to discover:
//!
//! * an unknown dashboard type falls back to the executive layout, as shipped;
//! * a widget the caller builds is kept exactly as it was given, including its position and data, and given a
//!   short id when it arrived without one;
//! * the period is echoed back and never used, because nothing here reads a period.

use serde_json::{json, Value};

use vimbai_store::read::{refused, text};
use vimbai_store::StoreError;

/// A widget on a dashboard: what it is called, what kind it is, and the figure it would hold.
fn widget(kind: &str, title: &str, data: Value, position: usize) -> Value {
    json!({
        "widget_id": format!("w{position}"),
        "type": kind,
        "title": title,
        "data": data,
        "position": position,
    })
}

fn kpi(title: &str, format: &str, position: usize) -> Value {
    widget(
        "kpi_card",
        title,
        json!({"value": 0, "format": format}),
        position,
    )
}

/// The executive layout: what the company earned, kept and holds.
fn executive_widgets() -> Vec<Value> {
    let mut revenue = kpi("Total Revenue", "currency", 1);
    revenue["data"]["trend"] = json!("up");
    let mut income = kpi("Net Income", "currency", 2);
    income["data"]["trend"] = json!("down");
    vec![
        revenue,
        income,
        kpi("Cash Position", "currency", 3),
        kpi("Profit Margin", "percentage", 4),
        widget(
            "chart",
            "Revenue vs Expenses",
            json!({"chart_type": "line", "periods": []}),
            5,
        ),
        widget("chart", "Cash Flow Trend", json!({"chart_type": "bar"}), 6),
    ]
}

/// The financial layout: the ratios read off the statements.
fn financial_widgets() -> Vec<Value> {
    vec![
        kpi("Current Ratio", "decimal", 1),
        kpi("Debt-to-Equity", "decimal", 2),
        kpi("ROE", "percentage", 3),
        widget(
            "chart",
            "Balance Sheet",
            json!({"chart_type": "stacked_bar"}),
            4,
        ),
        widget("table", "Top Accounts by Balance", json!({"rows": []}), 5),
    ]
}

/// The operations layout: how the working capital is turning.
fn operations_widgets() -> Vec<Value> {
    vec![
        kpi("Inventory Turnover", "decimal", 1),
        kpi("Days Sales Outstanding", "days", 2),
        widget(
            "gauge",
            "Capacity Utilization",
            json!({"value": 0, "max": 100}),
            3,
        ),
        widget(
            "chart",
            "Order Fulfillment",
            json!({"chart_type": "area"}),
            4,
        ),
    ]
}

/// The compliance layout: what has been scored, what is outstanding and what was found.
fn compliance_widgets() -> Vec<Value> {
    vec![
        kpi("Compliance Score", "percentage", 1),
        widget(
            "alert",
            "Pending Compliance Items",
            json!({"alerts": [], "count": 0}),
            2,
        ),
        widget("table", "Audit Findings", json!({"rows": []}), 3),
    ]
}

/// The layout a dashboard type asks for, falling back to the executive one as shipped.
pub fn widgets_for(dashboard_type: &str) -> Vec<Value> {
    match dashboard_type {
        "financial" => financial_widgets(),
        "operations" => operations_widgets(),
        "compliance" => compliance_widgets(),
        // an unknown type is not refused: the shipped service answered the executive layout
        _ => executive_widgets(),
    }
}

fn dashboard_id(prefix: &str, company_id: &str, dashboard_type: &str, stamp: &str) -> String {
    format!("{prefix}-{company_id}-{dashboard_type}-{stamp}")
}

/// `POST /load`: the layout for a company, with the period it was asked about echoed back.
pub fn load_dashboard(payload: &Value) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let dashboard_type = text(payload, "dashboard_type");
    let refreshed = match payload.get("today").and_then(Value::as_str) {
        Some(stated) if !stated.trim().is_empty() => stated.trim().to_string(),
        _ => return Err(refused("today is required")),
    };
    let stamp: String = refreshed
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .take(8)
        .collect();
    let dashboard_type = if dashboard_type.is_empty() {
        "executive".to_string()
    } else {
        dashboard_type
    };
    Ok(json!({
        "dashboard_id": dashboard_id("DASH", &company_id, &dashboard_type, &stamp),
        "company_id": company_id,
        "dashboard_type": dashboard_type,
        "widgets": widgets_for(&dashboard_type),
        "last_refreshed": refreshed,
        "period_start": text(payload, "period_start"),
        "period_end": text(payload, "period_end"),
        // every figure on these layouts is nil: the shipped service read nothing
        "placeholder": true,
    }))
}

/// `POST /custom`: a layout the caller states, kept as stated.
pub fn create_custom_dashboard(payload: &Value) -> Result<Value, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(refused("company_id is required"));
    }
    let dashboard_type = text(payload, "dashboard_type");
    let refreshed = match payload.get("today").and_then(Value::as_str) {
        Some(stated) if !stated.trim().is_empty() => stated.trim().to_string(),
        _ => return Err(refused("today is required")),
    };
    let asked = payload
        .get("widgets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let widgets: Vec<Value> = asked
        .into_iter()
        .enumerate()
        .map(|(index, stated)| {
            // a widget arrives as the caller built it; only the id it did not state is supplied
            let mut kept = stated.clone();
            let named = text(&stated, "widget_id");
            if named.is_empty() {
                kept["widget_id"] = json!(format!("c{}", index + 1));
            }
            if kept.get("position").is_none() {
                kept["position"] = json!(index + 1);
            }
            if kept.get("data").is_none() {
                kept["data"] = json!({});
            }
            kept
        })
        .collect();
    let suffix: String = {
        // the shipped service made the last part of the id from a fresh identifier; the caller states it here so
        // the same request answers the same id
        let stated = text(payload, "dashboard_key");
        if stated.is_empty() {
            "custom".to_string()
        } else {
            stated.chars().take(8).collect()
        }
    };
    Ok(json!({
        "dashboard_id": format!("CUST-{company_id}-{suffix}"),
        "company_id": company_id,
        "dashboard_type": dashboard_type,
        "widgets": widgets,
        "last_refreshed": refreshed,
        "period_start": "",
        "period_end": "",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executive_layout_is_the_one_an_unknown_type_gets() {
        let unknown = widgets_for("imaginative");
        let executive = widgets_for("executive");
        assert_eq!(unknown, executive);
        assert_eq!(unknown.len(), 6);
        assert_eq!(unknown[0]["title"], json!("Total Revenue"));
        // the revenue card is the one that trends up, and net income down
        assert_eq!(unknown[0]["data"]["trend"], json!("up"));
        assert_eq!(unknown[1]["data"]["trend"], json!("down"));
    }

    #[test]
    fn every_layout_holds_a_figure_of_nil_and_says_which_kind_it_is() {
        for layout in ["financial", "operations", "compliance"] {
            for widget in widgets_for(layout) {
                let kind = widget["type"].as_str().unwrap();
                assert!(
                    ["kpi_card", "chart", "table", "gauge", "alert"].contains(&kind),
                    "{layout} has a widget of an unknown type: {kind}"
                );
                // a card and a gauge carry a figure, and that figure is nil; a chart, table or alert carries the
                // empty shape it would be drawn from instead
                if kind == "kpi_card" || kind == "gauge" {
                    assert_eq!(widget["data"]["value"], json!(0), "{layout}");
                }
            }
        }
    }

    #[test]
    fn a_loaded_dashboard_echoes_its_period_and_carries_no_reading() {
        let loaded = load_dashboard(&json!({
            "company_id": "co-1",
            "dashboard_type": "operations",
            "refresh_interval": 60,
            "period_start": "2026-03-01",
            "period_end": "2026-03-31",
            "today": "2026-03-31T14:05:00+00:00"
        }))
        .expect("a dashboard");
        assert_eq!(
            loaded["dashboard_id"],
            json!("DASH-co-1-operations-20260331")
        );
        assert_eq!(loaded["period_start"], json!("2026-03-01"));
        assert_eq!(loaded["last_refreshed"], json!("2026-03-31T14:05:00+00:00"));
        assert_eq!(loaded["placeholder"], json!(true));
        assert_eq!(loaded["widgets"].as_array().expect("widgets").len(), 4);
    }

    #[test]
    fn a_custom_dashboard_keeps_the_widgets_it_was_given() {
        let built = create_custom_dashboard(&json!({
            "company_id": "co-1",
            "dashboard_type": "board",
            "dashboard_key": "abc12345",
            "today": "2026-03-31T14:05:00+00:00",
            "widgets": [
                {"type": "kpi_card", "title": "Revenue", "data": {"value": 1200}, "position": 7},
                {"type": "table", "title": "Debtors"}
            ]
        }))
        .expect("a dashboard");
        assert_eq!(built["dashboard_id"], json!("CUST-co-1-abc12345"));
        let widgets = built["widgets"].as_array().expect("widgets");
        // the stated position is kept rather than renumbered
        assert_eq!(widgets[0]["position"], json!(7));
        assert_eq!(widgets[0]["data"]["value"], json!(1200));
        // the widget that stated neither an id nor a position is given both
        assert_eq!(widgets[1]["widget_id"], json!("c2"));
        assert_eq!(widgets[1]["position"], json!(2));
        assert_eq!(widgets[1]["data"], json!({}));
    }

    #[test]
    fn a_dashboard_with_no_company_is_refused() {
        assert!(load_dashboard(&json!({"dashboard_type": "executive"})).is_err());
    }
}
