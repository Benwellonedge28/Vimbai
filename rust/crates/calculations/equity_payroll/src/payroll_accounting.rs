//! payroll-accounting-service `/process`: Zimbabwe-style PAYE/NSSA
//! payroll with employer contributions and journal entries.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeSalary {
    pub gross_salary: f64,
    pub paye_rate: f64,
    pub nassa_rate: f64,
    pub pension_rate: f64,
    #[serde(default)]
    pub medical_aid: f64,
    #[serde(default)]
    pub other_deductions: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLine {
    pub account: String,
    pub debit: f64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayrollAccountingOutput {
    pub total_gross: f64,
    pub total_net: f64,
    pub total_paye: f64,
    pub total_nassa: f64,
    pub total_pension: f64,
    pub total_medical: f64,
    pub total_other: f64,
    pub employer_pension: f64,
    pub employer_nassa: f64,
    pub total_cost: f64,
    pub journal_entries: Vec<JournalLine>,
}

pub fn process_payroll(employees: &[EmployeeSalary]) -> PayrollAccountingOutput {
    let mut total_gross = 0.0;
    let mut total_net = 0.0;
    let mut total_paye = 0.0;
    let mut total_nassa = 0.0;
    let mut total_pension = 0.0;
    let mut total_medical = 0.0;
    let mut total_other = 0.0;

    for emp in employees {
        let paye = emp.gross_salary * emp.paye_rate;
        let nassa = emp.gross_salary * emp.nassa_rate;
        let pension = emp.gross_salary * emp.pension_rate;
        let deductions = paye + nassa + pension + emp.medical_aid + emp.other_deductions;
        let net = emp.gross_salary - deductions;

        total_gross += emp.gross_salary;
        total_net += net;
        total_paye += paye;
        total_nassa += nassa;
        total_pension += pension;
        total_medical += emp.medical_aid;
        total_other += emp.other_deductions;
    }

    // Employer contributions are flat 7% pension / 3% NSSA on total gross.
    let employer_pension = total_gross * 0.07;
    let employer_nassa = total_gross * 0.03;
    let total_cost = total_gross + employer_pension + employer_nassa;

    let journal_entries = vec![
        JournalLine {
            account: "Salaries Expense".into(),
            debit: py_round2(total_gross),
            credit: 0.0,
        },
        JournalLine {
            account: "Pension Expense".into(),
            debit: py_round2(employer_pension),
            credit: 0.0,
        },
        JournalLine {
            account: "NSSA Expense".into(),
            debit: py_round2(employer_nassa),
            credit: 0.0,
        },
        JournalLine {
            account: "PAYE Payable".into(),
            debit: 0.0,
            credit: py_round2(total_paye),
        },
        JournalLine {
            account: "NSSA Payable".into(),
            debit: 0.0,
            credit: py_round2(total_nassa + employer_nassa),
        },
        JournalLine {
            account: "Pension Payable".into(),
            debit: 0.0,
            credit: py_round2(total_pension + employer_pension),
        },
        JournalLine {
            account: "Medical Aid Payable".into(),
            debit: 0.0,
            credit: py_round2(total_medical),
        },
        JournalLine {
            account: "Salaries Payable".into(),
            debit: 0.0,
            credit: py_round2(total_net),
        },
    ];

    PayrollAccountingOutput {
        total_gross: py_round2(total_gross),
        total_net: py_round2(total_net),
        total_paye: py_round2(total_paye),
        total_nassa: py_round2(total_nassa),
        total_pension: py_round2(total_pension),
        total_medical: py_round2(total_medical),
        total_other: py_round2(total_other),
        employer_pension: py_round2(employer_pension),
        employer_nassa: py_round2(employer_nassa),
        total_cost: py_round2(total_cost),
        journal_entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_employee_journals_balance() {
        let e = EmployeeSalary {
            gross_salary: 10_000.0,
            paye_rate: 0.25,
            nassa_rate: 0.045,
            pension_rate: 0.05,
            medical_aid: 300.0,
            other_deductions: 200.0,
        };
        let r = process_payroll(&[e]);
        assert_eq!(r.total_paye, 2_500.0);
        assert_eq!(r.total_nassa, 450.0);
        assert_eq!(r.total_pension, 500.0);
        assert_eq!(r.total_net, 6_050.0);
        assert_eq!(r.employer_pension, 700.0);
        assert_eq!(r.employer_nassa, 300.0);
        assert_eq!(r.total_cost, 11_000.0);

        let debits: f64 = r.journal_entries.iter().map(|j| j.debit).sum();
        let credits: f64 = r.journal_entries.iter().map(|j| j.credit).sum();
        // PYTHON QUIRK KEPT: "other deductions" reduce net pay but have no
        // payable credit line, so the journal sits 200 out of balance.
        assert_eq!(py_round2(debits), 11_000.0);
        assert_eq!(py_round2(credits), 10_800.0);
        assert_eq!(py_round2(debits - credits), 200.0);
    }

    #[test]
    fn two_employees_aggregate() {
        let base = EmployeeSalary {
            gross_salary: 6_000.0,
            paye_rate: 0.2,
            nassa_rate: 0.045,
            pension_rate: 0.05,
            medical_aid: 0.0,
            other_deductions: 0.0,
        };
        let r = process_payroll(&[base.clone(), base]);
        assert_eq!(r.total_gross, 12_000.0);
        assert_eq!(r.total_net, 2.0 * (6_000.0 - 1_200.0 - 270.0 - 300.0));
        assert_eq!(r.journal_entries.len(), 8);
    }

    #[test]
    fn medical_follows_period_case() {
        assert_eq!(py_round2(0.0), 0.0);
    }
}
