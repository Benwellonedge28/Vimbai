//! payroll-service: `/run` and `/taxes`.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeInput {
    pub employee_id: String,
    #[serde(default = "default_gross")]
    pub gross_pay: f64,
}

fn default_gross() -> f64 {
    5000.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeePay {
    pub employee_id: String,
    pub gross_pay: f64,
    pub deductions: BTreeMap<String, f64>,
    pub taxes: BTreeMap<String, f64>,
    pub net_pay: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayrollRunOutput {
    pub total_gross: f64,
    pub total_deductions: f64,
    pub total_taxes: f64,
    pub total_net: f64,
    pub employee_pays: Vec<EmployeePay>,
    pub status: String,
}

/// Flat US-style rates from the Python service: federal 15%, state 5%,
/// social security 6.2%, medicare 1.45%. Deductions repeat per employee.
pub fn payroll_run(
    employees: &[EmployeeInput],
    deductions: &BTreeMap<String, f64>,
) -> PayrollRunOutput {
    let mut total_gross = 0.0;
    let mut total_deductions = 0.0;
    let mut total_taxes = 0.0;
    let mut pays = Vec::new();

    for emp in employees {
        let gross = emp.gross_pay;
        let mut taxes = BTreeMap::new();
        taxes.insert("federal".to_string(), gross * 0.15);
        taxes.insert("state".to_string(), gross * 0.05);
        taxes.insert("social_security".to_string(), gross * 0.062);
        taxes.insert("medicare".to_string(), gross * 0.0145);
        let tax_sum: f64 = taxes.values().sum();
        let ded_sum: f64 = deductions.values().sum();
        let net = gross - tax_sum - ded_sum;

        pays.push(EmployeePay {
            employee_id: emp.employee_id.clone(),
            gross_pay: gross,
            deductions: deductions.clone(),
            taxes,
            net_pay: py_round2(net),
        });
        total_gross += gross;
        total_taxes += tax_sum;
        total_deductions += ded_sum;
    }

    PayrollRunOutput {
        total_gross: py_round2(total_gross),
        total_deductions: py_round2(total_deductions),
        total_taxes: py_round2(total_taxes),
        total_net: py_round2(total_gross - total_deductions - total_taxes),
        employee_pays: pays,
        status: "processed".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayrollTaxOutput {
    pub federal_withholding: f64,
    pub state_withholding: f64,
    pub social_security: f64,
    pub medicare: f64,
    pub futa: f64,
    pub total_taxes: f64,
}

pub fn payroll_taxes(gross_wages: f64) -> PayrollTaxOutput {
    let federal = gross_wages * 0.15;
    let state = gross_wages * 0.05;
    let ss = (gross_wages * 0.062).min(10_080.60);
    let medicare = gross_wages * 0.0145;
    let futa = (gross_wages * 0.006).min(42.30);

    PayrollTaxOutput {
        federal_withholding: py_round2(federal),
        state_withholding: py_round2(state),
        social_security: py_round2(ss),
        medicare: py_round2(medicare),
        futa: py_round2(futa),
        total_taxes: py_round2(federal + state + ss + medicare + futa),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emp(id: &str, gross: f64) -> EmployeeInput {
        EmployeeInput {
            employee_id: id.to_string(),
            gross_pay: gross,
        }
    }

    #[test]
    fn run_two_employees_with_deductions() {
        let mut deds = BTreeMap::new();
        deds.insert("pension".to_string(), 500.0);
        let r = payroll_run(&[emp("e1", 10_000.0), emp("e2", 5_000.0)], &deds);
        // e1 taxes: 1500+500+620+145 = 2765 -> net 6735
        assert_eq!(r.employee_pays[0].net_pay, 6_735.0);
        assert_eq!(r.total_gross, 15_000.0);
        assert_eq!(r.total_deductions, 1_000.0);
        assert_eq!(r.total_taxes, 4_147.5);
        assert_eq!(r.total_net, 9_852.5);
        assert_eq!(r.status, "processed");
    }

    #[test]
    fn default_gross_applies_when_absent() {
        let r = payroll_run(
            &[EmployeeInput {
                employee_id: "e".into(),
                gross_pay: default_gross(),
            }],
            &BTreeMap::new(),
        );
        assert_eq!(r.total_gross, 5_000.0);
    }

    #[test]
    fn tax_caps_bite_on_large_wages() {
        let t = payroll_taxes(200_000.0);
        assert_eq!(t.social_security, 10_080.6);
        assert_eq!(t.futa, 42.3);
        assert_eq!(
            t.total_taxes,
            py_round2(30_000.0 + 10_000.0 + 10_080.6 + 2_900.0 + 42.3)
        );
    }

    #[test]
    fn tax_caps_do_not_bite_on_small_wages() {
        let t = payroll_taxes(1_000.0);
        assert_eq!(t.social_security, 62.0);
        assert_eq!(t.futa, 6.0);
        assert_eq!(t.total_taxes, 282.5);
    }
}
