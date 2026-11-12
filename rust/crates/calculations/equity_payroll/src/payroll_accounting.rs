//! payroll-accounting-service `/process`: Zimbabwe-style PAYE/NSSA
//! payroll with employer contributions and journal entries.
//!
//! Every figure is a `Money`. A salary, a deduction and a statutory payment are amounts, so they
//! are exact to whatever scale the caller sent and are rounded once, on the way out, exactly where
//! the service rounded them before. The rates stay ratios: a rate is a proportion, and the only
//! arithmetic that needs it multiplies it by an amount.
use serde::{Deserialize, Serialize};
use vimbai_common::Money;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeSalary {
    pub gross_salary: Money,
    pub paye_rate: f64,
    pub nassa_rate: f64,
    pub pension_rate: f64,
    #[serde(default)]
    pub medical_aid: Money,
    #[serde(default)]
    pub other_deductions: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLine {
    pub account: String,
    pub debit: Money,
    pub credit: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayrollAccountingOutput {
    pub total_gross: Money,
    pub total_net: Money,
    pub total_paye: Money,
    pub total_nassa: Money,
    pub total_pension: Money,
    pub total_medical: Money,
    pub total_other: Money,
    pub employer_pension: Money,
    pub employer_nassa: Money,
    pub total_cost: Money,
    pub journal_entries: Vec<JournalLine>,
}

/// A rate applied to an amount, exactly. `0.045` arrives as a decimal string and multiplies as one,
/// so forty employees at 6,000.00 contribute 10,800.00 of NSSA rather than a float's approximation
/// of it.
fn at_rate(amount: &Money, rate: f64) -> Money {
    match Money::from_f64_shortest(rate) {
        Ok(ratio) => amount.times(&ratio),
        Err(_) => Money::zero(),
    }
}

pub fn process_payroll(employees: &[EmployeeSalary]) -> PayrollAccountingOutput {
    let mut total_gross = Money::zero();
    let mut total_net = Money::zero();
    let mut total_paye = Money::zero();
    let mut total_nassa = Money::zero();
    let mut total_pension = Money::zero();
    let mut total_medical = Money::zero();
    let mut total_other = Money::zero();

    for emp in employees {
        let paye = at_rate(&emp.gross_salary, emp.paye_rate);
        let nassa = at_rate(&emp.gross_salary, emp.nassa_rate);
        let pension = at_rate(&emp.gross_salary, emp.pension_rate);
        let deductions = paye
            .plus(&nassa)
            .plus(&pension)
            .plus(&emp.medical_aid)
            .plus(&emp.other_deductions);
        let net = emp.gross_salary.minus(&deductions);

        total_gross = total_gross.plus(&emp.gross_salary);
        total_net = total_net.plus(&net);
        total_paye = total_paye.plus(&paye);
        total_nassa = total_nassa.plus(&nassa);
        total_pension = total_pension.plus(&pension);
        total_medical = total_medical.plus(&emp.medical_aid);
        total_other = total_other.plus(&emp.other_deductions);
    }

    // Employer contributions are flat 7% pension / 3% NSSA on total gross.
    let employer_pension = at_rate(&total_gross, 0.07);
    let employer_nassa = at_rate(&total_gross, 0.03);
    let total_cost = total_gross.plus(&employer_pension).plus(&employer_nassa);

    let line = |account: &str, debit: Money, credit: Money| JournalLine {
        account: account.to_string(),
        debit: debit.round(2),
        credit: credit.round(2),
    };
    let zero = Money::zero();

    let journal_entries = vec![
        line("Salaries Expense", total_gross.clone(), zero.clone()),
        line("Pension Expense", employer_pension.clone(), zero.clone()),
        line("NSSA Expense", employer_nassa.clone(), zero.clone()),
        line("PAYE Payable", zero.clone(), total_paye.clone()),
        line(
            "NSSA Payable",
            zero.clone(),
            total_nassa.plus(&employer_nassa),
        ),
        line(
            "Pension Payable",
            zero.clone(),
            total_pension.plus(&employer_pension),
        ),
        line("Medical Aid Payable", zero.clone(), total_medical.clone()),
        line("Salaries Payable", zero.clone(), total_net.clone()),
    ];

    PayrollAccountingOutput {
        total_gross: total_gross.round(2),
        total_net: total_net.round(2),
        total_paye: total_paye.round(2),
        total_nassa: total_nassa.round(2),
        total_pension: total_pension.round(2),
        total_medical: total_medical.round(2),
        total_other: total_other.round(2),
        employer_pension: employer_pension.round(2),
        employer_nassa: employer_nassa.round(2),
        total_cost: total_cost.round(2),
        journal_entries,
    }
}

/// The service's `/process` body.
///
/// The response keeps the field names the service published, including `total_other_deductions`
/// and `total_cost_to_company`, which differ from the internal struct. The service also minted an
/// `id`; a calculator has no clock and no identity, so the door does not invent one.
pub fn process_payroll_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Employee {
        gross_salary: Money,
        #[serde(default = "default_paye")]
        paye_rate: f64,
        #[serde(default = "default_nassa")]
        nassa_rate: f64,
        #[serde(default = "default_pension")]
        pension_rate: f64,
        #[serde(default)]
        medical_aid: Money,
        #[serde(default)]
        other_deductions: Money,
    }
    fn default_paye() -> f64 {
        0.25
    }
    fn default_nassa() -> f64 {
        0.03
    }
    fn default_pension() -> f64 {
        0.05
    }

    #[derive(Deserialize)]
    struct Request {
        company_id: String,
        period: String,
        employees: Vec<Employee>,
    }

    let request: Request = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    let salaries: Vec<EmployeeSalary> = request
        .employees
        .iter()
        .map(|employee| EmployeeSalary {
            gross_salary: employee.gross_salary.clone(),
            paye_rate: employee.paye_rate,
            nassa_rate: employee.nassa_rate,
            pension_rate: employee.pension_rate,
            medical_aid: employee.medical_aid.clone(),
            other_deductions: employee.other_deductions.clone(),
        })
        .collect();
    let output = process_payroll(&salaries);

    Ok(serde_json::json!({
        "company_id": request.company_id,
        "period": request.period,
        "employee_count": request.employees.len(),
        "total_gross": output.total_gross,
        "total_net": output.total_net,
        "total_paye": output.total_paye,
        "total_nassa": output.total_nassa,
        "total_pension": output.total_pension,
        "total_medical": output.total_medical,
        "total_other_deductions": output.total_other,
        "employer_pension": output.employer_pension,
        "employer_nassa": output.employer_nassa,
        "total_cost_to_company": output.total_cost,
        "journal_entries": output.journal_entries,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn employee(gross: &str, medical: &str, other: &str) -> EmployeeSalary {
        EmployeeSalary {
            gross_salary: money(gross),
            paye_rate: 0.25,
            nassa_rate: 0.045,
            pension_rate: 0.05,
            medical_aid: money(medical),
            other_deductions: money(other),
        }
    }

    #[test]
    fn single_employee_journals_balance() {
        let r = process_payroll(&[employee("10000", "300", "200")]);
        assert_eq!(r.total_paye.to_fixed(2), "2500.00");
        assert_eq!(r.total_nassa.to_fixed(2), "450.00");
        assert_eq!(r.total_pension.to_fixed(2), "500.00");
        assert_eq!(r.total_net.to_fixed(2), "6050.00");
        assert_eq!(r.employer_pension.to_fixed(2), "700.00");
        assert_eq!(r.employer_nassa.to_fixed(2), "300.00");
        assert_eq!(r.total_cost.to_fixed(2), "11000.00");

        let debits = Money::sum(r.journal_entries.iter().map(|j| &j.debit));
        let credits = Money::sum(r.journal_entries.iter().map(|j| &j.credit));
        // PYTHON QUIRK KEPT: "other deductions" reduce net pay but have no
        // payable credit line, so the journal sits 200 out of balance.
        assert_eq!(debits.to_fixed(2), "11000.00");
        assert_eq!(credits.to_fixed(2), "10800.00");
        assert_eq!(debits.minus(&credits).to_fixed(2), "200.00");
    }

    #[test]
    fn two_employees_aggregate() {
        let r = process_payroll(&[employee("6000", "0", "0"), employee("6000", "0", "0")]);
        assert_eq!(r.total_gross.to_fixed(2), "12000.00");
        assert_eq!(r.total_net.to_fixed(2), "7860.00");
        assert_eq!(r.journal_entries.len(), 8);
    }

    #[test]
    fn a_salary_beyond_double_precision_keeps_every_digit() {
        let mut emp = employee("123456789012345678901.55", "0", "0");
        emp.paye_rate = 0.1;
        emp.nassa_rate = 0.0;
        emp.pension_rate = 0.0;
        let r = process_payroll(&[emp]);
        assert_eq!(r.total_paye.to_fixed(2), "12345678901234567890.16");
        assert_eq!(r.total_net.to_fixed(2), "111111110111111111011.40");
    }

    #[test]
    fn the_door_keeps_the_published_field_names() {
        let payload = serde_json::json!({
            "company_id": "C1",
            "period": "2026-11",
            "employees": [{
                "employee_id": "E1",
                "employee_name": "Sam",
                "gross_salary": "10000.00",
                "medical_aid": "300.00",
                "other_deductions": "200.00"
            }]
        });
        let out = process_payroll_from_json(&payload).expect("door");
        assert_eq!(out["total_other_deductions"], "200.00");
        assert_eq!(out["total_cost_to_company"], "11000.00");
        assert_eq!(out["employee_count"], 1);
        // defaults are the service's: 25% PAYE, 3% NSSA, 5% pension
        assert_eq!(out["total_paye"], "2500.00");
        assert_eq!(out["total_nassa"], "300.00");
        assert_eq!(out["total_pension"], "500.00");
        assert!(out.get("id").is_none(), "a calculator does not mint ids");
    }

    #[test]
    fn a_zero_salary_still_produces_a_balanced_shell() {
        let mut emp = employee("0", "0", "0");
        emp.paye_rate = 0.25;
        let r = process_payroll(&[emp]);
        assert_eq!(r.total_net.to_fixed(2), "0.00");
        assert_eq!(r.journal_entries.len(), 8);
    }
}
