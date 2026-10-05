#![forbid(unsafe_code)]
//! Overhead apportionment, ported from overhead-apportionment-service
//! (`/apportion`, `/batch-apportion`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverheadItem {
    pub overhead_id: String,
    pub overhead_name: String,
    pub total_amount: f64,
    /// Basis field name on the centre data (e.g. "floor_area",
    /// "machine_hours", "personnel").
    pub basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCentreDatum {
    pub cost_centre_id: String,
    pub cost_centre_name: String,
    /// Basis values keyed by field name; missing keys read as 0
    /// (Python `dict.get(basis, 0)` parity).
    #[serde(flatten)]
    pub basis_values: std::collections::BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apportionment {
    pub cost_centre_id: String,
    pub cost_centre_name: String,
    pub basis_value: f64,
    pub basis: String,
    pub proportion: f64,
    pub apportioned_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApportionedOverhead {
    pub overhead_id: String,
    pub period: String,
    pub total_overhead: f64,
    pub cost_centre_apportionments: Vec<Apportionment>,
}

/// Apportion one overhead by its basis
/// (overhead-apportionment-service `/apportion`). Centres with basis 0
/// are skipped entirely when the total basis is positive (Python parity:
/// the entry is only appended inside the `if total_basis > 0` branch).
pub fn apportion_overhead(
    overhead: &OverheadItem,
    period: &str,
    cost_centre_data: &[CostCentreDatum],
) -> ApportionedOverhead {
    let get = |c: &CostCentreDatum, field: &str| c.basis_values.get(field).copied().unwrap_or(0.0);
    let total_basis: f64 = cost_centre_data
        .iter()
        .map(|c| get(c, &overhead.basis))
        .sum();

    let mut result = ApportionedOverhead {
        overhead_id: overhead.overhead_id.clone(),
        period: period.to_string(),
        total_overhead: overhead.total_amount,
        cost_centre_apportionments: Vec::new(),
    };
    for centre in cost_centre_data {
        let basis_value = get(centre, &overhead.basis);
        if total_basis > 0.0 {
            let proportion = basis_value / total_basis;
            let apportioned = overhead.total_amount * proportion;
            result.cost_centre_apportionments.push(Apportionment {
                cost_centre_id: centre.cost_centre_id.clone(),
                cost_centre_name: centre.cost_centre_name.clone(),
                basis_value,
                basis: overhead.basis.clone(),
                proportion,
                apportioned_amount: apportioned,
            });
        }
    }
    result
}

/// Apportion every overhead for a period
/// (overhead-apportionment-service `/batch-apportion`).
pub fn batch_apportion_overheads(
    overheads: &[OverheadItem],
    period: &str,
    cost_centre_data: &[CostCentreDatum],
) -> Vec<ApportionedOverhead> {
    overheads
        .iter()
        .map(|o| apportion_overhead(o, period, cost_centre_data))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centre(id: &str, name: &str, floor: f64, hours: f64) -> CostCentreDatum {
        let mut m = std::collections::BTreeMap::new();
        m.insert("floor_area".to_string(), floor);
        m.insert("machine_hours".to_string(), hours);
        CostCentreDatum {
            cost_centre_id: id.to_string(),
            cost_centre_name: name.to_string(),
            basis_values: m,
        }
    }

    #[test]
    fn apportion_by_floor_area() {
        let oh = OverheadItem {
            overhead_id: "o1".into(),
            overhead_name: "rent".into(),
            total_amount: 10000.0,
            basis: "floor_area".into(),
        };
        let data = vec![
            centre("c1", "Alpha", 300.0, 5.0),
            centre("c2", "Beta", 700.0, 1.0),
        ];
        let r = apportion_overhead(&oh, "2026-09", &data);
        assert_eq!(r.total_overhead, 10000.0);
        assert_eq!(r.cost_centre_apportionments[0].basis_value, 300.0);
        assert_eq!(r.cost_centre_apportionments[0].proportion, 0.3);
        assert_eq!(r.cost_centre_apportionments[0].apportioned_amount, 3000.0);
        assert_eq!(r.cost_centre_apportionments[1].apportioned_amount, 7000.0);
        let total: f64 = r
            .cost_centre_apportionments
            .iter()
            .map(|a| a.apportioned_amount)
            .sum();
        assert_eq!(total, 10000.0);
    }

    #[test]
    fn zero_total_basis_yields_no_apportionments() {
        let oh = OverheadItem {
            overhead_id: "o1".into(),
            overhead_name: "x".into(),
            total_amount: 500.0,
            basis: "machine_hours".into(),
        };
        let data = vec![centre("c1", "Alpha", 100.0, 0.0)];
        let r = apportion_overhead(&oh, "p", &data);
        assert!(r.cost_centre_apportionments.is_empty());
    }

    #[test]
    fn missing_basis_key_reads_zero() {
        let oh = OverheadItem {
            overhead_id: "o1".into(),
            overhead_name: "x".into(),
            total_amount: 500.0,
            basis: "personnel".into(),
        };
        let data = vec![centre("c1", "Alpha", 100.0, 0.0)];
        // personnel key absent everywhere -> total basis 0 -> no rows
        assert!(apportion_overhead(&oh, "p", &data)
            .cost_centre_apportionments
            .is_empty());
    }

    #[test]
    fn batch_apportions_all() {
        let ohs = vec![
            OverheadItem {
                overhead_id: "o1".into(),
                overhead_name: "rent".into(),
                total_amount: 10000.0,
                basis: "floor_area".into(),
            },
            OverheadItem {
                overhead_id: "o2".into(),
                overhead_name: "power".into(),
                total_amount: 2000.0,
                basis: "machine_hours".into(),
            },
        ];
        let data = vec![
            centre("c1", "Alpha", 300.0, 60.0),
            centre("c2", "Beta", 100.0, 20.0),
        ];
        let rs = batch_apportion_overheads(&ohs, "2026-09", &data);
        assert_eq!(rs.len(), 2);
        assert_eq!(
            rs[0].cost_centre_apportionments[0].apportioned_amount,
            7500.0
        );
        // power by hours: 60/80 * 2000 = 1500
        assert_eq!(
            rs[1].cost_centre_apportionments[0].apportioned_amount,
            1500.0
        );
    }
}
