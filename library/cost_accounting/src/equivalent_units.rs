#![forbid(unsafe_code)]
//! Equivalent units (process costing), ported from
//! equivalent-units-service with exact weighted-average and FIFO parity.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquivalentUnitsMethod {
    WeightedAverage,
    Fifo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalentUnitsRequest {
    #[serde(default)]
    pub company_id: String,
    #[serde(default)]
    pub department: String,
    #[serde(default)]
    pub period: String,
    pub method: EquivalentUnitsMethod,
    pub units_completed: f64,
    pub beginning_wip_units: f64,
    pub beginning_wip_completion_materials: f64,
    pub beginning_wip_completion_conversion: f64,
    pub ending_wip_units: f64,
    pub ending_wip_completion_materials: f64,
    pub ending_wip_completion_conversion: f64,
    pub materials_cost_beginning: f64,
    pub materials_cost_added: f64,
    pub conversion_cost_beginning: f64,
    pub conversion_cost_added: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalentUnitsOutput {
    pub company_id: String,
    pub department: String,
    pub period: String,
    pub method: EquivalentUnitsMethod,
    pub equivalent_units_materials: f64,
    pub equivalent_units_conversion: f64,
    pub cost_per_unit_materials: f64,
    pub cost_per_unit_conversion: f64,
    pub cost_per_unit_total: f64,
    pub cost_of_completed: f64,
    pub cost_of_ending_wip: f64,
}

/// Equivalent units and unit costs
/// (equivalent-units-service `/calculate`).
pub fn equivalent_units(req: &EquivalentUnitsRequest) -> EquivalentUnitsOutput {
    let (eu_materials, eu_conversion, cpu_materials, cpu_conversion) = match req.method {
        EquivalentUnitsMethod::WeightedAverage => {
            let eu_m =
                req.units_completed + req.ending_wip_units * req.ending_wip_completion_materials;
            let eu_c =
                req.units_completed + req.ending_wip_units * req.ending_wip_completion_conversion;
            let total_m = req.materials_cost_beginning + req.materials_cost_added;
            let total_c = req.conversion_cost_beginning + req.conversion_cost_added;
            let cpu_m = if eu_m != 0.0 { total_m / eu_m } else { 0.0 };
            let cpu_c = if eu_c != 0.0 { total_c / eu_c } else { 0.0 };
            (eu_m, eu_c, cpu_m, cpu_c)
        }
        EquivalentUnitsMethod::Fifo => {
            let eu_m = (req.units_completed
                - req.beginning_wip_units * req.beginning_wip_completion_materials)
                + req.ending_wip_units * req.ending_wip_completion_materials;
            let eu_c = (req.units_completed
                - req.beginning_wip_units * req.beginning_wip_completion_conversion)
                + req.ending_wip_units * req.ending_wip_completion_conversion;
            let cpu_m = if eu_m != 0.0 {
                req.materials_cost_added / eu_m
            } else {
                0.0
            };
            let cpu_c = if eu_c != 0.0 {
                req.conversion_cost_added / eu_c
            } else {
                0.0
            };
            (eu_m, eu_c, cpu_m, cpu_c)
        }
    };

    let cpu_total = cpu_materials + cpu_conversion;
    let cost_completed = req.units_completed * cpu_total;
    let cost_ending_wip =
        req.ending_wip_units * req.ending_wip_completion_materials * cpu_materials
            + req.ending_wip_units * req.ending_wip_completion_conversion * cpu_conversion;

    EquivalentUnitsOutput {
        company_id: req.company_id.clone(),
        department: req.department.clone(),
        period: req.period.clone(),
        method: req.method,
        equivalent_units_materials: py_round(eu_materials, 1),
        equivalent_units_conversion: py_round(eu_conversion, 1),
        cost_per_unit_materials: py_round(cpu_materials, 4),
        cost_per_unit_conversion: py_round(cpu_conversion, 4),
        cost_per_unit_total: py_round(cpu_total, 4),
        cost_of_completed: py_round2(cost_completed),
        cost_of_ending_wip: py_round2(cost_ending_wip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_req(method: EquivalentUnitsMethod) -> EquivalentUnitsRequest {
        EquivalentUnitsRequest {
            company_id: "c1".into(),
            department: "mixing".into(),
            period: "2026-09".into(),
            method,
            units_completed: 9000.0,
            beginning_wip_units: 1000.0,
            beginning_wip_completion_materials: 1.0,
            beginning_wip_completion_conversion: 0.4,
            ending_wip_units: 2000.0,
            ending_wip_completion_materials: 1.0,
            ending_wip_completion_conversion: 0.5,
            materials_cost_beginning: 5000.0,
            materials_cost_added: 45000.0,
            conversion_cost_beginning: 2000.0,
            conversion_cost_added: 40000.0,
        }
    }

    #[test]
    fn weighted_average_parity() {
        let out = equivalent_units(&base_req(EquivalentUnitsMethod::WeightedAverage));
        // eu_m = 9000 + 2000*1 = 11000; eu_c = 9000 + 2000*0.5 = 10000
        // cpu_m = 50000/11000 = 4.5455; cpu_c = 42000/10000 = 4.2
        assert_eq!(out.equivalent_units_materials, 11000.0);
        assert_eq!(out.equivalent_units_conversion, 10000.0);
        assert_eq!(out.cost_per_unit_materials, py_round(50000.0 / 11000.0, 4));
        assert_eq!(out.cost_per_unit_conversion, 4.2);
        // cpu_total = 8.7455; completed = 9000 * 8.7455 = 78709.09
        let cpu_total = 50000.0 / 11000.0 + 4.2;
        assert_eq!(out.cost_of_completed, py_round2(9000.0 * cpu_total));
        // ending wip = 2000*1*4.5455 + 2000*0.5*4.2 = 9090.91 + 4200 = 13290.91
        assert_eq!(
            out.cost_of_ending_wip,
            py_round2(2000.0 * (50000.0 / 11000.0) + 1000.0 * 4.2)
        );
    }

    #[test]
    fn fifo_parity() {
        let out = equivalent_units(&base_req(EquivalentUnitsMethod::Fifo));
        // eu_m = (9000 - 1000*1) + 2000*1 = 10000; eu_c = (9000-400) + 1000 = 9600
        // cpu_m = 45000/10000 = 4.5; cpu_c = 40000/9600 = 4.1667
        assert_eq!(out.equivalent_units_materials, 10000.0);
        assert_eq!(out.equivalent_units_conversion, 9600.0);
        assert_eq!(out.cost_per_unit_materials, 4.5);
        assert_eq!(out.cost_per_unit_conversion, py_round(40000.0 / 9600.0, 4));
        let cpu_total = 4.5 + 40000.0 / 9600.0;
        assert_eq!(out.cost_of_completed, py_round2(9000.0 * cpu_total));
        assert_eq!(
            out.cost_of_ending_wip,
            py_round2(2000.0 * 4.5 + 1000.0 * (40000.0 / 9600.0))
        );
    }

    #[test]
    fn zero_units_guard() {
        let mut req = base_req(EquivalentUnitsMethod::WeightedAverage);
        req.units_completed = 0.0;
        req.ending_wip_units = 0.0;
        let out = equivalent_units(&req);
        assert_eq!(out.cost_per_unit_materials, 0.0);
        assert_eq!(out.cost_per_unit_conversion, 0.0);
        assert_eq!(out.cost_of_completed, 0.0);
    }
}
