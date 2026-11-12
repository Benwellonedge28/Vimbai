//! IFRS 16 leases: liability and right-of-use measurement, amortisation,
//! renewal assessment, variable payments and sale-and-leaseback.
use serde::{Deserialize, Serialize};

/// Straight present value of an annuity, plus the discounted residual.
fn pv_of_payments(payment: f64, n: u32, r: f64) -> f64 {
    if r > 0.0 {
        payment * ((1.0 - (1.0 + r).powi(-(n as i32))) / r)
    } else {
        payment * n as f64
    }
}

fn pv_of_residual(residual: f64, n: u32, r: f64) -> f64 {
    if r > 0.0 {
        residual / (1.0 + r).powi(n as i32)
    } else {
        residual
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaseDetails {
    pub lease_id: String,
    pub lessee_name: String,
    /// "operating", "finance", "short_term" or "low_value". Only the last
    /// two change the treatment; operating and finance are both
    /// capitalised identically.
    pub lease_type: String,
    pub asset_class: String,
    pub lease_commencement_date: String,
    pub lease_term_years: i32,
    pub payment_frequency: String,
    pub payment_amount: f64,
    pub number_of_payments: u32,
    pub discount_rate: f64,
    pub initial_direct_costs: f64,
    pub lease_incentive_received: f64,
    pub variable_lease_payments: f64,
    pub renewal_options_probable: bool,
    pub guaranteed_residual_value: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ifrs16Request {
    pub company_id: String,
    pub reporting_date: String,
    pub leases: Vec<LeaseDetails>,
    pub incremental_borrowing_rate: f64,
    pub include_short_term_exemption: bool,
    pub include_low_value_exemption: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaseLiabilityMeasurement {
    pub lease_id: String,
    pub lease_commencement_date: String,
    pub lease_term_years: i32,
    pub payment_amount: f64,
    pub number_of_payments: u32,
    pub discount_rate: f64,
    pub present_value_lease_payments: f64,
    pub guaranteed_residual_value: f64,
    pub initial_measurement: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouAssetMeasurement {
    pub lease_id: String,
    pub initial_lease_liability: f64,
    pub initial_direct_costs: f64,
    pub lease_incentive_received: f64,
    pub prepaid_lease_payments: f64,
    pub initial_recognition: f64,
    pub depreciation_period: i32,
    pub annual_depreciation: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaseExpenseBreakdown {
    pub lease_id: String,
    pub interest_expense: f64,
    pub depreciation_charge: f64,
    pub variable_lease_payment: f64,
    pub short_term_lease_expense: f64,
    pub low_value_lease_expense: f64,
    pub total_lease_expense: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ifrs16Response {
    pub company_id: String,
    pub reporting_date: String,
    pub total_lease_liability: f64,
    pub total_rou_assets: f64,
    pub lease_liabilities: Vec<LeaseLiabilityMeasurement>,
    pub rou_assets: Vec<RouAssetMeasurement>,
    pub lease_expenses: Vec<LeaseExpenseBreakdown>,
    pub interest_payable_current: f64,
    pub interest_payable_non_current: f64,
    pub total_interest_expense: f64,
}

/// `/calculate`.
///
/// PYTHON QUIRKS, all preserved:
/// - an exempt short-term or low-value lease is expensed at
///   `payment_amount * 12` regardless of payment frequency or number of
///   payments, and its variable payments are recorded but left out of the
///   total expense;
/// - the discounted residual is added to the liability, which stands in
///   for the probable residual guarantee;
/// - a probable renewal extends the depreciation period by exactly two
///   years rather than by the renewal term;
/// - year-one interest is the initial liability times the rate, i.e. no
///   repayment has been assumed within the year;
/// - interest payable is split 30% current and 70% non-current by fixed
///   ratio rather than by payment schedule.
pub fn calculate_ifrs16(req: &Ifrs16Request) -> Ifrs16Response {
    let mut lease_liabilities = Vec::new();
    let mut rou_assets = Vec::new();
    let mut lease_expenses = Vec::new();
    let mut total_liability = 0.0;
    let mut total_rou = 0.0;
    let mut total_interest = 0.0;

    for lease in &req.leases {
        if req.include_short_term_exemption && lease.lease_type == "short_term" {
            let annual_expense = lease.payment_amount * 12.0;
            lease_expenses.push(LeaseExpenseBreakdown {
                lease_id: lease.lease_id.clone(),
                interest_expense: 0.0,
                depreciation_charge: 0.0,
                variable_lease_payment: lease.variable_lease_payments,
                short_term_lease_expense: annual_expense,
                low_value_lease_expense: 0.0,
                total_lease_expense: annual_expense,
            });
            continue;
        }
        if req.include_low_value_exemption && lease.lease_type == "low_value" {
            let annual_expense = lease.payment_amount * 12.0;
            lease_expenses.push(LeaseExpenseBreakdown {
                lease_id: lease.lease_id.clone(),
                interest_expense: 0.0,
                depreciation_charge: 0.0,
                variable_lease_payment: lease.variable_lease_payments,
                short_term_lease_expense: 0.0,
                low_value_lease_expense: annual_expense,
                total_lease_expense: annual_expense,
            });
            continue;
        }

        let n = lease.number_of_payments;
        let r = lease.discount_rate;
        let pv_payments = pv_of_payments(lease.payment_amount, n, r);
        let pv_residual = pv_of_residual(lease.guaranteed_residual_value, n, r);
        let pv_total = pv_payments + pv_residual;

        lease_liabilities.push(LeaseLiabilityMeasurement {
            lease_id: lease.lease_id.clone(),
            lease_commencement_date: lease.lease_commencement_date.clone(),
            lease_term_years: lease.lease_term_years,
            payment_amount: lease.payment_amount,
            number_of_payments: n,
            discount_rate: r,
            present_value_lease_payments: pv_payments,
            guaranteed_residual_value: lease.guaranteed_residual_value,
            initial_measurement: pv_total,
        });
        total_liability += pv_total;

        let initial_rou = pv_total + lease.initial_direct_costs - lease.lease_incentive_received;
        let depreciation_period = if lease.renewal_options_probable {
            lease.lease_term_years + 2
        } else {
            lease.lease_term_years
        };
        let annual_dep = initial_rou / depreciation_period as f64;

        rou_assets.push(RouAssetMeasurement {
            lease_id: lease.lease_id.clone(),
            initial_lease_liability: pv_total,
            initial_direct_costs: lease.initial_direct_costs,
            lease_incentive_received: lease.lease_incentive_received,
            prepaid_lease_payments: 0.0,
            initial_recognition: initial_rou,
            depreciation_period,
            annual_depreciation: annual_dep,
        });
        total_rou += initial_rou;

        let year1_interest = pv_total * r;
        total_interest += year1_interest;
        lease_expenses.push(LeaseExpenseBreakdown {
            lease_id: lease.lease_id.clone(),
            interest_expense: year1_interest,
            depreciation_charge: annual_dep,
            variable_lease_payment: lease.variable_lease_payments,
            short_term_lease_expense: 0.0,
            low_value_lease_expense: 0.0,
            total_lease_expense: year1_interest + annual_dep + lease.variable_lease_payments,
        });
    }

    Ifrs16Response {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        total_lease_liability: total_liability,
        total_rou_assets: total_rou,
        lease_liabilities,
        rou_assets,
        lease_expenses,
        interest_payable_current: total_interest * 0.3,
        interest_payable_non_current: total_interest * 0.7,
        total_interest_expense: total_interest,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AmortizationRow {
    pub period: u32,
    pub payment: f64,
    pub interest: f64,
    pub principal: f64,
    pub remaining_balance: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaseLiabilitySchedule {
    pub present_value_lease_payments: f64,
    pub present_value_residual: f64,
    pub total_lease_liability: f64,
    pub amortization_schedule: Vec<AmortizationRow>,
    pub total_interest: f64,
}

/// `/lease-liability`. The schedule carries the FULL payment each period,
/// so once interest exceeds nothing the principal can exceed the balance;
/// the remaining balance is floored at zero rather than going negative,
/// which silently absorbs the excess.
pub fn measure_lease_liability(
    payment_amount: f64,
    number_of_payments: u32,
    discount_rate: f64,
    guaranteed_residual: f64,
) -> LeaseLiabilitySchedule {
    let r = discount_rate;
    let n = number_of_payments;
    let pv_payments = pv_of_payments(payment_amount, n, r);
    let pv_residual = pv_of_residual(guaranteed_residual, n, r);
    let pv_total = pv_payments + pv_residual;

    let mut schedule = Vec::new();
    let mut remaining = pv_total;
    for period in 1..=n {
        let interest = remaining * r;
        let principal = payment_amount - interest;
        remaining = (remaining - principal).max(0.0);
        schedule.push(AmortizationRow {
            period,
            payment: payment_amount,
            interest,
            principal,
            remaining_balance: remaining,
        });
    }
    let total_interest = schedule.iter().map(|s| s.interest).sum();

    LeaseLiabilitySchedule {
        present_value_lease_payments: pv_payments,
        present_value_residual: pv_residual,
        total_lease_liability: pv_total,
        amortization_schedule: schedule,
        total_interest,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenewalAssessment {
    pub renewal_option_years: i32,
    pub market_rent_annual: f64,
    pub current_rent_annual: f64,
    pub economic_incentives: String,
    pub business_use_intent: String,
    pub renewal_probable: bool,
    pub recommended_lease_term_extension: i32,
    pub reasoning: String,
}

/// `/renewal-options`. Probable when incentives are significant and the
/// intent is to continue, or when incentives are moderate and the current
/// rent sits more than 10% below market. A "high" incentive level is not
/// recognised at all, so it excludes the option.
pub fn assess_renewal_options(
    renewal_option_years: i32,
    market_rent_annual: f64,
    current_rent_annual: f64,
    economic_incentives: &str,
    business_use_intent: &str,
) -> RenewalAssessment {
    let market_advantage = if market_rent_annual > 0.0 {
        (market_rent_annual - current_rent_annual) / market_rent_annual
    } else {
        0.0
    };
    let probable = (economic_incentives == "significant" && business_use_intent == "continue")
        || (economic_incentives == "moderate" && market_advantage > 0.1);

    RenewalAssessment {
        renewal_option_years,
        market_rent_annual,
        current_rent_annual,
        economic_incentives: economic_incentives.to_string(),
        business_use_intent: business_use_intent.to_string(),
        renewal_probable: probable,
        recommended_lease_term_extension: if probable { renewal_option_years } else { 0 },
        reasoning: if probable {
            "Renewal options included".to_string()
        } else {
            "Renewal options excluded from lease term".to_string()
        },
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariablePayment {
    pub lease_id: String,
    pub variable_payment_type: String,
    pub base_amount: f64,
    pub variable_rate: f64,
    pub usage_measure: f64,
    pub variable_lease_payment: f64,
    pub recognition: String,
}

/// `/variable-lease-payments`.
///
/// PYTHON QUIRK, preserved as-is but wrong under IFRS 16: the recognition
/// text says a percentage-of-sales payment is "included in lease
/// liability", which inverts the standard. Percentage-of-sales and
/// usage-based payments are both excluded from the liability and expensed
/// as incurred. The arithmetic is also odd: usage-based ignores the rate
/// and multiplies base by the usage measure, and an unknown type returns
/// a payment of zero.
pub fn calculate_variable_payments(
    lease_id: &str,
    variable_payment_type: &str,
    base_amount: f64,
    variable_rate: f64,
    usage_measure: f64,
) -> VariablePayment {
    let variable_payment = match variable_payment_type {
        "percentage_of_sales" => base_amount * (variable_rate / 100.0),
        "usage_based" => base_amount * usage_measure,
        "index_rate" => base_amount * (1.0 + variable_rate),
        _ => 0.0,
    };
    VariablePayment {
        lease_id: lease_id.to_string(),
        variable_payment_type: variable_payment_type.to_string(),
        base_amount,
        variable_rate,
        usage_measure,
        variable_lease_payment: variable_payment,
        recognition: if variable_payment_type != "percentage_of_sales" {
            "expensed as incurred".to_string()
        } else {
            "included in lease liability".to_string()
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaseError {
    /// The sale-and-leaseback PV divides by the monthly rate.
    ZeroDiscountRate,
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaseError::ZeroDiscountRate => {
                write!(
                    f,
                    "discount_rate must be greater than zero for a sale and leaseback"
                )
            }
        }
    }
}

impl std::error::Error for LeaseError {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaleLeaseback {
    pub sale_price: f64,
    pub carrying_amount: f64,
    pub fair_value: f64,
    pub profit_on_sale: f64,
    pub sale_at_fair_value: f64,
    pub sale_below_fair_value: f64,
    pub sale_above_fair_value: f64,
    pub right_of_use_asset_recognized: f64,
    pub gain_deferred: f64,
    pub gain_recognized_immediately: f64,
    pub transfer_qualifies_as_sale: bool,
}

/// `/sale-leaseback`.
///
/// PYTHON QUIRKS, all preserved:
/// - the sale test is `pv_lease >= 90% of fair value`, a leaseback magnitude
///   standing in for the IFRS 15 control test;
/// - the right-of-use asset is the lease liability when the transfer
///   qualifies and the full fair value when it does not;
/// - the only immediate gain is recorded when the sale price equals fair
///   value EXACTLY, so a sale above fair value recognises nothing;
/// - the deferred gain is just the shortfall below fair value, not the
///   unrecognised proportion of the total gain.
///
/// Python raises a division error when `discount_rate` is zero; this
/// returns `LeaseError::ZeroDiscountRate` instead of an infinity.
pub fn calculate_sale_leaseback(
    sale_price: f64,
    carrying_amount: f64,
    fair_value: f64,
    lease_payment: f64,
    lease_term: i32,
    discount_rate: f64,
) -> Result<SaleLeaseback, LeaseError> {
    let profit_on_sale = sale_price - carrying_amount;
    let sale_at_fair_value = if sale_price == fair_value {
        profit_on_sale
    } else {
        0.0
    };
    let sale_below_fair_value = if sale_price < fair_value {
        fair_value - sale_price
    } else {
        0.0
    };
    let sale_above_fair_value = if sale_price > fair_value {
        sale_price - fair_value
    } else {
        0.0
    };

    let n = lease_term * 12;
    let r = discount_rate / 12.0;
    if r == 0.0 {
        return Err(LeaseError::ZeroDiscountRate);
    }
    let pv_lease = lease_payment * ((1.0 - (1.0 + r).powi(-n)) / r);
    let transfer_qualified = pv_lease >= fair_value * 0.9;

    Ok(SaleLeaseback {
        sale_price,
        carrying_amount,
        fair_value,
        profit_on_sale,
        sale_at_fair_value,
        sale_below_fair_value,
        sale_above_fair_value,
        right_of_use_asset_recognized: if transfer_qualified {
            pv_lease
        } else {
            fair_value
        },
        gain_deferred: sale_below_fair_value,
        gain_recognized_immediately: sale_at_fair_value,
        transfer_qualifies_as_sale: transfer_qualified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(id: &str, kind: &str) -> LeaseDetails {
        LeaseDetails {
            lease_id: id.into(),
            lessee_name: "Vimbai Ltd".into(),
            lease_type: kind.into(),
            asset_class: "property".into(),
            lease_commencement_date: "2026-01-01".into(),
            lease_term_years: 5,
            payment_frequency: "monthly".into(),
            payment_amount: 1_000.0,
            number_of_payments: 60,
            discount_rate: 0.01,
            initial_direct_costs: 2_000.0,
            lease_incentive_received: 500.0,
            variable_lease_payments: 200.0,
            renewal_options_probable: false,
            guaranteed_residual_value: 5_000.0,
        }
    }

    fn request(leases: Vec<LeaseDetails>) -> Ifrs16Request {
        Ifrs16Request {
            company_id: "c1".into(),
            reporting_date: "2026-12-31".into(),
            leases,
            incremental_borrowing_rate: 0.012,
            include_short_term_exemption: true,
            include_low_value_exemption: true,
        }
    }

    #[test]
    fn liability_combines_annuity_and_discounted_residual() {
        let r = calculate_ifrs16(&request(vec![lease("L1", "operating")]));
        let liab = &r.lease_liabilities[0];
        // 1,000 monthly for 60 months at 1% = 44,955.03
        assert!((liab.present_value_lease_payments - 44_955.03).abs() < 0.02);
        // 5,000 / 1.01^60 = 2,752.25
        assert!(
            (liab.initial_measurement - liab.present_value_lease_payments - 2_752.25).abs() < 0.02
        );
        assert_eq!(r.total_lease_liability, liab.initial_measurement);
    }

    #[test]
    fn rou_asset_adds_costs_and_removes_incentives() {
        let r = calculate_ifrs16(&request(vec![lease("L1", "operating")]));
        let rou = &r.rou_assets[0];
        assert_eq!(
            rou.initial_recognition,
            rou.initial_lease_liability + 2_000.0 - 500.0
        );
        assert_eq!(rou.depreciation_period, 5);
        assert_eq!(rou.annual_depreciation, rou.initial_recognition / 5.0);
        assert_eq!(rou.prepaid_lease_payments, 0.0);
    }

    #[test]
    fn probable_renewal_adds_exactly_two_years() {
        let mut renewal = lease("L2", "operating");
        renewal.renewal_options_probable = true;
        let r = calculate_ifrs16(&request(vec![renewal]));
        assert_eq!(r.rou_assets[0].depreciation_period, 7);
    }

    #[test]
    fn expenses_and_fixed_interest_split() {
        let r = calculate_ifrs16(&request(vec![lease("L1", "operating")]));
        let exp = &r.lease_expenses[0];
        assert_eq!(exp.interest_expense, r.total_interest_expense);
        assert_eq!(
            exp.total_lease_expense,
            exp.interest_expense + exp.depreciation_charge + 200.0
        );
        assert_eq!(r.interest_payable_current, r.total_interest_expense * 0.3);
        assert_eq!(
            r.interest_payable_non_current,
            r.total_interest_expense * 0.7
        );
    }

    #[test]
    fn exempt_short_term_expense_is_twelve_payments_and_drops_variables() {
        let r = calculate_ifrs16(&request(vec![lease("S1", "short_term")]));
        assert!(r.lease_liabilities.is_empty());
        assert!(r.rou_assets.is_empty());
        let exp = &r.lease_expenses[0];
        assert_eq!(exp.short_term_lease_expense, 12_000.0);
        // the 200 variable payment is recorded but left out of the total
        assert_eq!(exp.variable_lease_payment, 200.0);
        assert_eq!(exp.total_lease_expense, 12_000.0);
        assert_eq!(r.total_lease_liability, 0.0);
    }

    #[test]
    fn low_value_exemption_can_be_switched_off() {
        let mut lv = lease("V1", "low_value");
        lv.payment_amount = 400.0;
        let exempt = calculate_ifrs16(&request(vec![lv.clone()]));
        assert_eq!(exempt.lease_expenses[0].low_value_lease_expense, 4_800.0);
        let capitalised = calculate_ifrs16(&Ifrs16Request {
            include_low_value_exemption: false,
            ..request(vec![lv])
        });
        assert_eq!(capitalised.lease_liabilities.len(), 1);
        assert!(capitalised.total_lease_liability > 0.0);
    }

    #[test]
    fn zero_rate_uses_simple_totals() {
        let mut no_rate = lease("Z1", "operating");
        no_rate.discount_rate = 0.0;
        no_rate.guaranteed_residual_value = 1_000.0;
        let r = calculate_ifrs16(&request(vec![no_rate]));
        // 1,000 * 60 + 1,000 residual, undiscounted
        assert_eq!(r.total_lease_liability, 61_000.0);
        assert_eq!(r.total_interest_expense, 0.0);
    }

    #[test]
    fn schedule_floors_the_balance_at_zero() {
        let s = measure_lease_liability(1_000.0, 3, 0.0, 0.0);
        assert_eq!(s.total_lease_liability, 3_000.0);
        assert_eq!(s.amortization_schedule.len(), 3);
        assert_eq!(s.amortization_schedule[2].remaining_balance, 0.0);
        assert_eq!(s.total_interest, 0.0);
    }

    #[test]
    fn schedule_carries_interest_across_periods() {
        let s = measure_lease_liability(1_000.0, 2, 0.05, 0.0);
        let pv = 1_000.0 * ((1.0 - 1.05f64.powi(-2)) / 0.05);
        assert!((s.total_lease_liability - pv).abs() < 1e-9);
        assert!((s.amortization_schedule[0].interest - pv * 0.05).abs() < 1e-9);
        // second period interest on the reduced balance, then a residue of zero
        assert!(s.amortization_schedule[1].remaining_balance < 0.01);
    }

    #[test]
    fn renewal_scenarios() {
        let significant =
            assess_renewal_options(3, 120_000.0, 100_000.0, "significant", "continue");
        assert!(significant.renewal_probable);
        assert_eq!(significant.recommended_lease_term_extension, 3);
        assert_eq!(significant.reasoning, "Renewal options included");

        // significant incentives but no stated intent to continue
        let leaving = assess_renewal_options(3, 120_000.0, 100_000.0, "significant", "exit");
        assert!(!leaving.renewal_probable);
        assert_eq!(leaving.recommended_lease_term_extension, 0);

        // moderate incentives with rent 20% below market
        let moderate = assess_renewal_options(2, 100_000.0, 80_000.0, "moderate", "continue");
        assert!(moderate.renewal_probable);

        // moderate incentives with only a 5% advantage
        let thin = assess_renewal_options(2, 100_000.0, 95_000.0, "moderate", "continue");
        assert!(!thin.renewal_probable);

        // a "high" incentive level is never recognised
        let high = assess_renewal_options(2, 100_000.0, 50_000.0, "high", "continue");
        assert!(!high.renewal_probable);

        // no market rent means no advantage and no division
        let no_market = assess_renewal_options(2, 0.0, 10_000.0, "moderate", "continue");
        assert!(!no_market.renewal_probable);
    }

    #[test]
    fn variable_payment_types() {
        let sales = calculate_variable_payments("L1", "percentage_of_sales", 200_000.0, 5.0, 0.0);
        assert_eq!(sales.variable_lease_payment, 10_000.0);
        // quirk: percentage of sales claims to sit inside the liability
        assert_eq!(sales.recognition, "included in lease liability");

        let usage = calculate_variable_payments("L1", "usage_based", 50.0, 999.0, 3.0);
        assert_eq!(usage.variable_lease_payment, 150.0);
        assert_eq!(usage.recognition, "expensed as incurred");

        let index = calculate_variable_payments("L1", "index_rate", 1_000.0, 0.08, 0.0);
        assert_eq!(index.variable_lease_payment, 1_080.0);

        let unknown = calculate_variable_payments("L1", "revenue_share", 1_000.0, 0.08, 5.0);
        assert_eq!(unknown.variable_lease_payment, 0.0);
        assert_eq!(unknown.recognition, "expensed as incurred");
    }

    #[test]
    fn sale_at_fair_value_recognises_the_gain() {
        let r = calculate_sale_leaseback(1_000_000.0, 800_000.0, 1_000_000.0, 8_000.0, 5, 0.06)
            .unwrap();
        assert_eq!(r.profit_on_sale, 200_000.0);
        assert_eq!(r.gain_recognized_immediately, 200_000.0);
        assert_eq!(r.gain_deferred, 0.0);
        assert_eq!(r.sale_below_fair_value, 0.0);
    }

    #[test]
    fn sale_below_fair_value_defers_the_shortfall_only() {
        let r =
            calculate_sale_leaseback(900_000.0, 800_000.0, 1_000_000.0, 8_000.0, 5, 0.06).unwrap();
        assert_eq!(r.profit_on_sale, 100_000.0);
        // nothing recognised immediately even though a gain arose
        assert_eq!(r.gain_recognized_immediately, 0.0);
        assert_eq!(r.gain_deferred, 100_000.0);
        assert_eq!(r.sale_below_fair_value, 100_000.0);
    }

    #[test]
    fn sale_above_fair_value_reports_nothing_but_the_excess() {
        let r = calculate_sale_leaseback(1_200_000.0, 800_000.0, 1_000_000.0, 8_000.0, 5, 0.06)
            .unwrap();
        assert_eq!(r.sale_above_fair_value, 200_000.0);
        assert_eq!(r.gain_recognized_immediately, 0.0);
        assert_eq!(r.gain_deferred, 0.0);
    }

    #[test]
    fn sale_transfer_test_and_rou_asset() {
        // large leaseback relative to fair value qualifies as a sale
        let qualifies =
            calculate_sale_leaseback(1_000_000.0, 800_000.0, 1_000_000.0, 20_000.0, 5, 0.06)
                .unwrap();
        assert!(qualifies.transfer_qualifies_as_sale);
        assert_eq!(
            qualifies.right_of_use_asset_recognized,
            qualifies.right_of_use_asset_recognized.max(0.0)
        );
        // a token leaseback does not
        let token =
            calculate_sale_leaseback(1_000_000.0, 800_000.0, 1_000_000.0, 100.0, 5, 0.06).unwrap();
        assert!(!token.transfer_qualifies_as_sale);
        assert_eq!(token.right_of_use_asset_recognized, 1_000_000.0);
    }

    #[test]
    fn zero_rate_is_a_typed_error_not_an_infinity() {
        let err = calculate_sale_leaseback(1_000_000.0, 800_000.0, 1_000_000.0, 8_000.0, 5, 0.0)
            .unwrap_err();
        assert_eq!(err, LeaseError::ZeroDiscountRate);
        assert!(err.to_string().contains("discount_rate"));
    }
}
