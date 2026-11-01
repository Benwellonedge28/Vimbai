//! basis-apportionment-service: apportion an indirect cost across cost
//! centres on a chosen basis (floor area, volume, asset cost, book value,
//! replacement value, headcount, requisitions).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApportionmentBasis {
    FloorArea,
    Volume,
    Cost,
    NetBookValue,
    ReplacementValue,
    NumberOfRequisitions,
    ValueOfRequisitions,
    NumberOfPersonnel,
    MachineHours,
    LaborHours,
    Units,
}

impl ApportionmentBasis {
    pub fn value(&self) -> &'static str {
        match self {
            ApportionmentBasis::FloorArea => "floor_area",
            ApportionmentBasis::Volume => "volume",
            ApportionmentBasis::Cost => "cost",
            ApportionmentBasis::NetBookValue => "net_book_value",
            ApportionmentBasis::ReplacementValue => "replacement_value",
            ApportionmentBasis::NumberOfRequisitions => "number_of_requisitions",
            ApportionmentBasis::ValueOfRequisitions => "value_of_requisitions",
            ApportionmentBasis::NumberOfPersonnel => "number_of_personnel",
            ApportionmentBasis::MachineHours => "machine_hours",
            ApportionmentBasis::LaborHours => "labor_hours",
            ApportionmentBasis::Units => "units",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostCentreBasis {
    pub cost_centre_id: String,
    pub cost_centre_name: String,
    /// The basis value: floor area in m2, volume in m3, asset cost, net
    /// book value, headcount, requisition count, hours or units.
    pub basis_value: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApportionedCentre {
    pub cost_centre_id: String,
    /// PYTHON QUIRK: the headcount endpoint omits this name, so it is
    /// `None` there and `Some` on every other basis.
    pub cost_centre_name: Option<String>,
    pub basis_value: f64,
    /// Also omitted on the headcount basis, alongside the apportioned cost.
    pub proportion: Option<f64>,
    pub apportioned_cost: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApportionmentResult {
    pub cost_type: String,
    pub total_cost: f64,
    pub basis: String,
    pub cost_centres: Vec<ApportionedCentre>,
    pub total_apportioned: f64,
    pub unapportioned: f64,
}

/// Apportion `total_cost` across the centres on `basis`.
///
/// PYTHON QUIRKS, both preserved:
/// - when the basis totals zero nothing is apportioned, so the whole cost
///   lands in `unapportioned`;
/// - the headcount basis still accumulates `total_apportioned` but returns
///   centres carrying only the id and headcount, with no name, proportion
///   or apportioned cost. Summing the returned centres therefore yields 0
///   even though the result reports the cost as apportioned.
pub fn apportion(
    cost_type: &str,
    total_cost: f64,
    basis: ApportionmentBasis,
    centres: &[CostCentreBasis],
) -> ApportionmentResult {
    let total_basis: f64 = centres.iter().map(|c| c.basis_value).sum();
    let mut result = ApportionmentResult {
        cost_type: cost_type.to_string(),
        total_cost,
        basis: basis.value().to_string(),
        ..Default::default()
    };

    if total_basis > 0.0 {
        for centre in centres {
            let proportion = centre.basis_value / total_basis;
            let apportioned_cost = total_cost * proportion;
            result.total_apportioned += apportioned_cost;
            if basis == ApportionmentBasis::NumberOfPersonnel {
                result.cost_centres.push(ApportionedCentre {
                    cost_centre_id: centre.cost_centre_id.clone(),
                    cost_centre_name: None,
                    basis_value: centre.basis_value,
                    proportion: None,
                    apportioned_cost: None,
                });
            } else {
                result.cost_centres.push(ApportionedCentre {
                    cost_centre_id: centre.cost_centre_id.clone(),
                    cost_centre_name: Some(centre.cost_centre_name.clone()),
                    basis_value: centre.basis_value,
                    proportion: Some(proportion),
                    apportioned_cost: Some(apportioned_cost),
                });
            }
        }
    }

    result.unapportioned = total_cost - result.total_apportioned;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centres() -> Vec<CostCentreBasis> {
        vec![
            CostCentreBasis {
                cost_centre_id: "cc1".into(),
                cost_centre_name: "Assembly".into(),
                basis_value: 600.0,
            },
            CostCentreBasis {
                cost_centre_id: "cc2".into(),
                cost_centre_name: "Finishing".into(),
                basis_value: 400.0,
            },
        ]
    }

    #[test]
    fn floor_area_splits_in_proportion() {
        let r = apportion("rent", 100_000.0, ApportionmentBasis::FloorArea, &centres());
        assert_eq!(r.basis, "floor_area");
        assert_eq!(r.cost_centres.len(), 2);
        assert_eq!(r.cost_centres[0].proportion, Some(0.6));
        assert_eq!(r.cost_centres[0].apportioned_cost, Some(60_000.0));
        assert_eq!(
            r.cost_centres[1].cost_centre_name.as_deref(),
            Some("Finishing")
        );
        assert_eq!(r.total_apportioned, 100_000.0);
        assert_eq!(r.unapportioned, 0.0);
    }

    #[test]
    fn zero_basis_leaves_everything_unapportioned() {
        let zeros = vec![CostCentreBasis {
            cost_centre_id: "cc1".into(),
            cost_centre_name: "Idle".into(),
            basis_value: 0.0,
        }];
        let r = apportion("insurance", 5_000.0, ApportionmentBasis::Cost, &zeros);
        assert!(r.cost_centres.is_empty());
        assert_eq!(r.total_apportioned, 0.0);
        assert_eq!(r.unapportioned, 5_000.0);
    }

    #[test]
    fn headcount_basis_reports_totals_but_omits_centre_amounts() {
        let r = apportion(
            "canteen",
            8_000.0,
            ApportionmentBasis::NumberOfPersonnel,
            &centres(),
        );
        assert_eq!(r.basis, "number_of_personnel");
        // the total claims the cost is apportioned...
        assert_eq!(r.total_apportioned, 8_000.0);
        assert_eq!(r.unapportioned, 0.0);
        // ...but each centre carries only an id and headcount
        assert_eq!(r.cost_centres[0].basis_value, 600.0);
        assert!(r.cost_centres[0].cost_centre_name.is_none());
        assert!(r.cost_centres[0].proportion.is_none());
        assert!(r.cost_centres[0].apportioned_cost.is_none());
        let summed: f64 = r
            .cost_centres
            .iter()
            .filter_map(|c| c.apportioned_cost)
            .sum();
        assert_eq!(summed, 0.0);
    }

    #[test]
    fn every_basis_shares_one_wire_name() {
        assert_eq!(ApportionmentBasis::NetBookValue.value(), "net_book_value");
        assert_eq!(
            ApportionmentBasis::ReplacementValue.value(),
            "replacement_value"
        );
        assert_eq!(
            ApportionmentBasis::NumberOfRequisitions.value(),
            "number_of_requisitions"
        );
        assert_eq!(
            ApportionmentBasis::ValueOfRequisitions.value(),
            "value_of_requisitions"
        );
        assert_eq!(ApportionmentBasis::MachineHours.value(), "machine_hours");
        assert_eq!(ApportionmentBasis::LaborHours.value(), "labor_hours");
        assert_eq!(ApportionmentBasis::Units.value(), "units");
        assert_eq!(ApportionmentBasis::Volume.value(), "volume");
    }

    #[test]
    fn rounding_residue_shows_as_unapportioned() {
        let thirds = vec![
            CostCentreBasis {
                cost_centre_id: "a".into(),
                cost_centre_name: "A".into(),
                basis_value: 1.0,
            },
            CostCentreBasis {
                cost_centre_id: "b".into(),
                cost_centre_name: "B".into(),
                basis_value: 1.0,
            },
            CostCentreBasis {
                cost_centre_id: "c".into(),
                cost_centre_name: "C".into(),
                basis_value: 1.0,
            },
        ];
        let r = apportion("rates", 100.0, ApportionmentBasis::Units, &thirds);
        assert!((r.total_apportioned - 100.0).abs() < 1e-9);
        assert!(r.unapportioned.abs() < 1e-9);
    }
}
