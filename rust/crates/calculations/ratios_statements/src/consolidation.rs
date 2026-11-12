//! consolidation-service: group consolidation, intercompany eliminations,
//! NCI and goodwill on acquisition.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

/// Raw subsidiary financial data. Missing keys fall back to the service's
/// own defaults rather than zero, matching `.get(key, default)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubFinancials {
    pub revenue: Option<f64>,
    pub profit: Option<f64>,
    pub assets: Option<f64>,
    pub liabilities: Option<f64>,
    pub equity: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Subsidiary {
    pub subsidiary_id: String,
    pub subsidiary_name: String,
    pub ownership_percentage: f64,
    pub non_controlling_interests_percentage: f64,
    pub goodwill: f64,
    pub financial_data: SubFinancials,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntercompanyTransaction {
    pub transaction_type: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidationInput {
    pub parent_company_id: String,
    pub period: String,
    pub subsidiaries: Vec<Subsidiary>,
    pub intercompany_transactions: Vec<IntercompanyTransaction>,
    pub minority_interest: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidatedRevenue {
    pub parent_revenue: f64,
    pub subsidiary_revenues: Vec<(String, f64)>,
    pub intercompany_sales_elimination: f64,
    pub consolidated_revenue: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidatedProfit {
    pub parent_profit: f64,
    pub subsidiary_profits: Vec<(String, f64)>,
    pub goodwill_amortization: f64,
    pub nci_share: f64,
    pub attributable_to_parent: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidatedBalanceSheet {
    pub total_assets: f64,
    pub non_current_assets: f64,
    pub current_assets: f64,
    pub total_liabilities: f64,
    pub non_current_liabilities: f64,
    pub current_liabilities: f64,
    pub equity_attributable_to_parent: f64,
    pub non_controlling_interests: f64,
    pub total_equity: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidatedCashFlow {
    pub operating_activities: f64,
    pub investing_activities: f64,
    pub financing_activities: f64,
    pub effect_of_exchange_rate: f64,
    pub net_increase_in_cash: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidatedStatement {
    pub parent_company_id: String,
    pub period: String,
    pub consolidated_revenue: ConsolidatedRevenue,
    pub consolidated_profit: ConsolidatedProfit,
    pub consolidated_balance_sheet: ConsolidatedBalanceSheet,
    pub consolidated_cash_flow: ConsolidatedCashFlow,
    pub goodwill_recognized: f64,
    pub nci_at_fair_value: f64,
    pub minority_interest: f64,
    pub elimination_count: usize,
}

/// PYTHON QUIRKS, all preserved:
/// - only revenue and profit are scaled by ownership; assets, liabilities
///   and equity are added at 100%;
/// - parent figures are hardcoded simulations, not inputs;
/// - the NCI profit share is applied to the already ownership-scaled
///   profit, so it is deducted from a minority stake of a minority stake;
/// - goodwill amortization appears in the statement but is never deducted;
/// - the balance sheet splits are fixed 60/40 and 40/60 ratios, and
///   goodwill_recognized is the only unrounded figure.
pub fn consolidate(req: &ConsolidationInput) -> ConsolidatedStatement {
    let mut total_sub_revenue = 0.0;
    let mut total_sub_profit = 0.0;
    let mut total_sub_assets = 0.0;
    let mut total_sub_liabilities = 0.0;
    let mut total_sub_equity = 0.0;
    let mut subsidiary_revenues = Vec::new();
    let mut subsidiary_profits = Vec::new();

    for sub in &req.subsidiaries {
        let ownership = sub.ownership_percentage / 100.0;
        let sub_revenue = sub.financial_data.revenue.unwrap_or(2_000_000.0) * ownership;
        let sub_profit = sub.financial_data.profit.unwrap_or(300_000.0) * ownership;
        let sub_assets = sub.financial_data.assets.unwrap_or(5_000_000.0);
        let sub_liabilities = sub.financial_data.liabilities.unwrap_or(2_500_000.0);
        let sub_equity = sub
            .financial_data
            .equity
            .unwrap_or(sub_assets - sub_liabilities);
        subsidiary_revenues.push((sub.subsidiary_id.clone(), py_round2(sub_revenue)));
        subsidiary_profits.push((sub.subsidiary_id.clone(), py_round2(sub_profit)));
        total_sub_revenue += sub_revenue;
        total_sub_profit += sub_profit;
        total_sub_assets += sub_assets;
        total_sub_liabilities += sub_liabilities;
        total_sub_equity += sub_equity;
    }

    let parent_revenue = 10_000_000.0;
    let parent_profit = 1_500_000.0;
    let parent_assets = 50_000_000.0;
    let parent_liabilities = 30_000_000.0;

    let intercompany_sales: f64 = req
        .intercompany_transactions
        .iter()
        .filter(|t| t.transaction_type == "sale")
        .map(|t| t.amount)
        .sum();

    let nci_pct_avg = if req.subsidiaries.is_empty() {
        0.0
    } else {
        req.subsidiaries
            .iter()
            .map(|s| s.non_controlling_interests_percentage)
            .sum::<f64>()
            / req.subsidiaries.len() as f64
    };
    let nci_profit_share = total_sub_profit * (nci_pct_avg / 100.0);
    let nci_equity_share = total_sub_equity * (nci_pct_avg / 100.0);
    let minority = if req.minority_interest {
        (total_sub_equity + parent_assets - parent_liabilities) * 0.1
    } else {
        0.0
    };

    let consolidated_revenue = parent_revenue + total_sub_revenue - intercompany_sales;
    let consolidated_profit_attributable = parent_profit + total_sub_profit - nci_profit_share;
    let total_assets = parent_assets + total_sub_assets;
    let total_liabilities = parent_liabilities + total_sub_liabilities;
    let total_equity = total_assets - total_liabilities;
    let parent_equity = total_equity - nci_equity_share;

    ConsolidatedStatement {
        parent_company_id: req.parent_company_id.clone(),
        period: req.period.clone(),
        consolidated_revenue: ConsolidatedRevenue {
            parent_revenue,
            subsidiary_revenues,
            intercompany_sales_elimination: py_round2(intercompany_sales),
            consolidated_revenue: py_round2(consolidated_revenue),
        },
        consolidated_profit: ConsolidatedProfit {
            parent_profit,
            subsidiary_profits,
            goodwill_amortization: 50_000.0,
            nci_share: py_round2(nci_profit_share),
            attributable_to_parent: py_round2(consolidated_profit_attributable),
        },
        consolidated_balance_sheet: ConsolidatedBalanceSheet {
            total_assets: py_round2(total_assets),
            non_current_assets: py_round2(total_assets * 0.6),
            current_assets: py_round2(total_assets * 0.4),
            total_liabilities: py_round2(total_liabilities),
            non_current_liabilities: py_round2(total_liabilities * 0.4),
            current_liabilities: py_round2(total_liabilities * 0.6),
            equity_attributable_to_parent: py_round2(parent_equity),
            non_controlling_interests: py_round2(nci_equity_share),
            total_equity: py_round2(total_equity),
        },
        consolidated_cash_flow: ConsolidatedCashFlow {
            operating_activities: 2_000_000.0,
            investing_activities: -800_000.0,
            financing_activities: -500_000.0,
            effect_of_exchange_rate: 10_000.0,
            net_increase_in_cash: 710_000.0,
        },
        goodwill_recognized: req.subsidiaries.iter().map(|s| s.goodwill).sum(),
        nci_at_fair_value: py_round2(nci_equity_share * 1.2),
        minority_interest: py_round2(minority),
        elimination_count: req.intercompany_transactions.len(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Eliminations {
    pub parent_company_id: String,
    pub period: String,
    pub total_eliminations: f64,
    pub asset_eliminations: f64,
    pub liability_eliminations: f64,
    pub revenue_eliminations: f64,
    pub expense_eliminations: f64,
    pub intercompany_dividends: f64,
    pub intercompany_loans: f64,
    pub intercompany_management_fees: f64,
    pub unmatched_transactions: Vec<IntercompanyTransaction>,
}

/// `/eliminations`. Sales and purchases are categorised, NOT netted here;
/// any unrecognised type lands in `unmatched_transactions` while still
/// counting towards the total.
pub fn process_eliminations(req: &ConsolidationInput) -> Eliminations {
    let mut total_eliminations = 0.0;
    let mut asset_elim = 0.0;
    let mut liability_elim = 0.0;
    let mut revenue_elim = 0.0;
    let mut expense_elim = 0.0;
    let mut dividend_elim = 0.0;
    let mut loan_elim = 0.0;
    let mut mgmt_fee_elim = 0.0;
    let mut unmatched = Vec::new();

    for txn in &req.intercompany_transactions {
        let amt = txn.amount;
        total_eliminations += amt;
        match txn.transaction_type.as_str() {
            "receivable" => asset_elim += amt,
            "payable" => liability_elim += amt,
            "sale" => revenue_elim += amt,
            "purchase" => expense_elim += amt,
            "dividend" => dividend_elim += amt,
            "loan" => loan_elim += amt,
            "management_fee" => mgmt_fee_elim += amt,
            _ => unmatched.push(txn.clone()),
        }
    }

    Eliminations {
        parent_company_id: req.parent_company_id.clone(),
        period: req.period.clone(),
        total_eliminations: py_round2(total_eliminations),
        asset_eliminations: py_round2(asset_elim),
        liability_eliminations: py_round2(liability_elim),
        revenue_eliminations: py_round2(revenue_elim),
        expense_eliminations: py_round2(expense_elim),
        intercompany_dividends: py_round2(dividend_elim),
        intercompany_loans: py_round2(loan_elim),
        intercompany_management_fees: py_round2(mgmt_fee_elim),
        unmatched_transactions: unmatched,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NciDetail {
    pub subsidiary_name: String,
    pub nci_percentage: f64,
    pub nci_profit_share: f64,
    pub nci_equity_share: f64,
    pub nci_fair_value_adjustment: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NciCalculation {
    pub parent_company_id: String,
    pub period: String,
    pub subsidiary_nci_details: Vec<(String, NciDetail)>,
    pub total_nci_profit: f64,
    pub total_nci_equity: f64,
}

/// `/nci-calculation`. This endpoint uses different defaults from
/// `/consolidate`: profit 500,000 and equity 2,500,000, and no ownership
/// scaling at all.
pub fn calculate_nci(req: &ConsolidationInput) -> NciCalculation {
    let mut details = Vec::new();
    for sub in &req.subsidiaries {
        let sub_profit = sub.financial_data.profit.unwrap_or(500_000.0);
        let sub_equity = sub.financial_data.equity.unwrap_or(2_500_000.0);
        let nci_pct = sub.non_controlling_interests_percentage;
        details.push((
            sub.subsidiary_id.clone(),
            NciDetail {
                subsidiary_name: sub.subsidiary_name.clone(),
                nci_percentage: nci_pct,
                nci_profit_share: py_round2(sub_profit * (nci_pct / 100.0)),
                nci_equity_share: py_round2(sub_equity * (nci_pct / 100.0)),
                nci_fair_value_adjustment: py_round2(sub_equity * (nci_pct / 100.0) * 0.2),
            },
        ));
    }
    let total_nci_profit = details.iter().map(|(_, d)| d.nci_profit_share).sum();
    let total_nci_equity = details.iter().map(|(_, d)| d.nci_equity_share).sum();
    NciCalculation {
        parent_company_id: req.parent_company_id.clone(),
        period: req.period.clone(),
        subsidiary_nci_details: details,
        total_nci_profit: py_round2(total_nci_profit),
        total_nci_equity: py_round2(total_nci_equity),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcquisitionGoodwill {
    pub subsidiary_id: String,
    pub subsidiary_name: String,
    pub consideration_transferred: f64,
    pub fair_value_nci: f64,
    pub net_identifiable_assets: f64,
    pub goodwill: f64,
    pub bargain_purchase: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoodwillCalculation {
    pub parent_company_id: String,
    pub acquisitions: Vec<AcquisitionGoodwill>,
    pub total_goodwill: f64,
    pub impairment_test_required: bool,
}

/// `/goodwill-calculation`. Consideration 6m and NCI 1.5m are fixed
/// simulations; net identifiable assets are a flat 4.5m for every
/// acquisition, and a shortfall is flagged as a bargain purchase rather
/// than a bargain gain.
pub fn calculate_goodwill(req: &ConsolidationInput) -> GoodwillCalculation {
    let mut acquisitions = Vec::new();
    let mut total_goodwill = 0.0;
    for sub in &req.subsidiaries {
        let consideration_paid = 6_000_000.0 * (sub.ownership_percentage / 100.0);
        let fair_value_nci = 1_500_000.0 * (sub.non_controlling_interests_percentage / 100.0);
        let net_identifiable_assets = 4_500_000.0;
        let goodwill = consideration_paid + fair_value_nci - net_identifiable_assets;
        let goodwill_recognised = goodwill.max(0.0);
        acquisitions.push(AcquisitionGoodwill {
            subsidiary_id: sub.subsidiary_id.clone(),
            subsidiary_name: sub.subsidiary_name.clone(),
            consideration_transferred: py_round2(consideration_paid),
            fair_value_nci: py_round2(fair_value_nci),
            net_identifiable_assets: py_round2(net_identifiable_assets),
            goodwill: py_round2(goodwill_recognised),
            bargain_purchase: goodwill < 0.0,
        });
        total_goodwill += goodwill_recognised;
    }
    GoodwillCalculation {
        parent_company_id: req.parent_company_id.clone(),
        acquisitions,
        total_goodwill: py_round2(total_goodwill),
        impairment_test_required: total_goodwill > 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(id: &str, ownership: f64, nci: f64) -> Subsidiary {
        Subsidiary {
            subsidiary_id: id.into(),
            subsidiary_name: format!("Sub {}", id),
            ownership_percentage: ownership,
            non_controlling_interests_percentage: nci,
            goodwill: 250_000.0,
            financial_data: SubFinancials {
                revenue: Some(2_000_000.0),
                profit: Some(300_000.0),
                assets: Some(5_000_000.0),
                liabilities: Some(2_500_000.0),
                equity: None,
            },
        }
    }

    fn input() -> ConsolidationInput {
        ConsolidationInput {
            parent_company_id: "parent".into(),
            period: "2026".into(),
            subsidiaries: vec![sub("s1", 80.0, 20.0), sub("s2", 60.0, 40.0)],
            intercompany_transactions: vec![
                IntercompanyTransaction {
                    transaction_type: "sale".into(),
                    amount: 400_000.0,
                },
                IntercompanyTransaction {
                    transaction_type: "dividend".into(),
                    amount: 100_000.0,
                },
                IntercompanyTransaction {
                    transaction_type: "royalty".into(),
                    amount: 50_000.0,
                },
            ],
            minority_interest: false,
        }
    }

    #[test]
    fn revenue_scaled_but_assets_added_in_full() {
        let r = consolidate(&input());
        assert_eq!(r.consolidated_revenue.subsidiary_revenues[0].1, 1_600_000.0);
        assert_eq!(r.consolidated_revenue.subsidiary_revenues[1].1, 1_200_000.0);
        // parent 10m + 2.8m scaled revenue - 0.4m intercompany sales
        assert_eq!(r.consolidated_revenue.consolidated_revenue, 12_400_000.0);
        // both subsidiaries' full 5m assets count
        assert_eq!(r.consolidated_balance_sheet.total_assets, 60_000_000.0);
        assert_eq!(r.consolidated_balance_sheet.total_liabilities, 35_000_000.0);
        assert_eq!(r.consolidated_balance_sheet.total_equity, 25_000_000.0);
    }

    #[test]
    fn nci_share_applies_to_already_scaled_profit() {
        let r = consolidate(&input());
        // scaled profits 240k + 180k = 420k; average NCI 30% => 126k
        assert_eq!(r.consolidated_profit.subsidiary_profits[0].1, 240_000.0);
        assert_eq!(r.consolidated_profit.nci_share, 126_000.0);
        assert_eq!(r.consolidated_profit.attributable_to_parent, 1_794_000.0);
        // goodwill amortization is reported but never deducted
        assert_eq!(r.consolidated_profit.goodwill_amortization, 50_000.0);
    }

    #[test]
    fn balance_sheet_splits_and_goodwill_are_fixed_shapes() {
        let r = consolidate(&input());
        let bs = &r.consolidated_balance_sheet;
        assert_eq!(bs.non_current_assets, 36_000_000.0);
        assert_eq!(bs.current_assets, 24_000_000.0);
        assert_eq!(bs.non_current_liabilities, 14_000_000.0);
        assert_eq!(bs.current_liabilities, 21_000_000.0);
        assert_eq!(r.consolidated_cash_flow.net_increase_in_cash, 710_000.0);
        // both subsidiaries' full equity is 5m, so the 30% NCI share is 1.5m
        assert_eq!(bs.non_controlling_interests, 1_500_000.0);
        assert_eq!(bs.equity_attributable_to_parent, 23_500_000.0);
        assert_eq!(r.nci_at_fair_value, 1_800_000.0);
        // goodwill is the only unrounded figure
        assert_eq!(r.goodwill_recognized, 500_000.0);
        // elimination count includes the unmatched royalty
        assert_eq!(r.elimination_count, 3);
        assert_eq!(r.minority_interest, 0.0);
    }

    #[test]
    fn minority_interest_legacy_flag() {
        let r = consolidate(&ConsolidationInput {
            minority_interest: true,
            ..input()
        });
        // (sub equity 5m + parent assets 50m - parent liabilities 30m) * 10%
        assert_eq!(r.minority_interest, 2_500_000.0);
    }

    #[test]
    fn subsidiaries_missing_financials_use_service_defaults() {
        let bare = Subsidiary {
            financial_data: SubFinancials::default(),
            ..sub("s3", 100.0, 0.0)
        };
        let r = consolidate(&ConsolidationInput {
            subsidiaries: vec![bare],
            intercompany_transactions: vec![],
            ..input()
        });
        assert_eq!(r.consolidated_revenue.subsidiary_revenues[0].1, 2_000_000.0);
        assert_eq!(r.consolidated_revenue.consolidated_revenue, 12_000_000.0);
        // equity defaults to assets - liabilities, so 50m + 5m less 30m + 2.5m
        assert_eq!(r.consolidated_balance_sheet.total_equity, 22_500_000.0);
    }

    #[test]
    fn no_subsidiaries_leaves_nci_at_zero() {
        let r = consolidate(&ConsolidationInput {
            subsidiaries: vec![],
            intercompany_transactions: vec![],
            ..input()
        });
        assert_eq!(r.consolidated_profit.nci_share, 0.0);
        assert_eq!(r.consolidated_revenue.consolidated_revenue, 10_000_000.0);
        assert_eq!(r.consolidated_balance_sheet.total_assets, 50_000_000.0);
    }

    #[test]
    fn eliminations_categorise_and_flag_unmatched() {
        let r = process_eliminations(&input());
        assert_eq!(r.total_eliminations, 550_000.0);
        assert_eq!(r.revenue_eliminations, 400_000.0);
        assert_eq!(r.intercompany_dividends, 100_000.0);
        assert_eq!(r.asset_eliminations, 0.0);
        assert_eq!(r.unmatched_transactions.len(), 1);
        assert_eq!(r.unmatched_transactions[0].transaction_type, "royalty");
    }

    #[test]
    fn nci_endpoint_does_not_scale_for_ownership() {
        let r = calculate_nci(&input());
        // full profit 300k at 20% NCI, not the 80%-scaled 240k
        assert_eq!(r.subsidiary_nci_details[0].1.nci_profit_share, 60_000.0);
        assert_eq!(r.subsidiary_nci_details[0].1.nci_equity_share, 500_000.0);
        assert_eq!(
            r.subsidiary_nci_details[0].1.nci_fair_value_adjustment,
            100_000.0
        );
        assert_eq!(r.total_nci_profit, 180_000.0);
        assert_eq!(r.total_nci_equity, 1_500_000.0);
    }

    #[test]
    fn goodwill_uses_fixed_consideration_and_flags_bargains() {
        let r = calculate_goodwill(&input());
        assert_eq!(r.acquisitions[0].consideration_transferred, 4_800_000.0);
        assert_eq!(r.acquisitions[0].fair_value_nci, 300_000.0);
        // 4.8m + 0.3m - 4.5m
        assert_eq!(r.acquisitions[0].goodwill, 600_000.0);
        assert!(!r.acquisitions[0].bargain_purchase);
        // the second acquisition is a bargain, so it adds nothing to the total
        assert_eq!(r.total_goodwill, 600_000.0);
        assert!(r.impairment_test_required);

        let bargain = calculate_goodwill(&ConsolidationInput {
            subsidiaries: vec![sub("s9", 50.0, 10.0)],
            ..input()
        });
        // 3m + 150k - 4.5m is negative: recognised at 0, flagged as a bargain
        assert_eq!(bargain.acquisitions[0].goodwill, 0.0);
        assert!(bargain.acquisitions[0].bargain_purchase);
        assert_eq!(bargain.total_goodwill, 0.0);
        assert!(!bargain.impairment_test_required);
    }
}

// ------------------------------------------------- currency translation

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyTranslationInput {
    pub parent_id: String,
    pub period: String,
    pub local_currency: String,
    pub reporting_currency: String,
    /// Account name to balance, in the caller's order.
    pub balances: Vec<(String, f64)>,
    /// Currency to rate. Only the local currency's entry is ever used.
    pub exchange_rates: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrencyTranslation {
    pub parent_id: String,
    pub local_currency: String,
    pub reporting_currency: String,
    pub translated_balances: Vec<(String, f64)>,
    pub translation_adjustment: f64,
    pub cumulative_translation_adjustment: f64,
}

/// `/currency-translation`.
///
/// PYTHON QUIRKS, all preserved:
/// - one rate is looked up by the *local* currency name and applied to every
///   account, so per-account or historical rates in `exchange_rates` are
///   ignored;
/// - a missing local rate silently translates at 1.0, leaving balances
///   untouched rather than raising;
/// - the adjustment only picks up accounts whose name contains "equity"
///   (case-insensitive) and is computed on the untranslated balance;
/// - the cumulative adjustment is a flat 1.5 times the period adjustment.
pub fn translate_currency(req: &CurrencyTranslationInput) -> CurrencyTranslation {
    let rate = req
        .exchange_rates
        .iter()
        .find(|(c, _)| *c == req.local_currency)
        .map(|(_, r)| *r)
        .unwrap_or(1.0);

    let mut translated = Vec::new();
    let mut translation_adj = 0.0;
    for (account, balance) in &req.balances {
        translated.push((account.clone(), vimbai_common::py_round2(balance * rate)));
        if account.to_lowercase().contains("equity") {
            translation_adj += balance * (rate - 1.0);
        }
    }

    CurrencyTranslation {
        parent_id: req.parent_id.clone(),
        local_currency: req.local_currency.clone(),
        reporting_currency: req.reporting_currency.clone(),
        translated_balances: translated,
        translation_adjustment: vimbai_common::py_round2(translation_adj),
        cumulative_translation_adjustment: vimbai_common::py_round2(translation_adj * 1.5),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationRuleResult {
    pub rule: String,
    pub status: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidationValidation {
    pub parent_id: String,
    pub period: String,
    pub is_valid: bool,
    pub validation_results: Vec<ValidationRuleResult>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// `/validate`.
///
/// PYTHON QUIRK: this endpoint cannot fail. Every caller rule is echoed as
/// "passed", the balance-sheet and intercompany checks are appended as
/// passed without being evaluated, and `errors` is always empty, so
/// `is_valid` is unconditionally true. Kept as-is for parity; callers must
/// not treat a pass here as evidence that anything balanced.
pub fn validate_consolidation(
    parent_id: &str,
    period: &str,
    rules: &[String],
) -> ConsolidationValidation {
    let mut results: Vec<ValidationRuleResult> = rules
        .iter()
        .map(|r| ValidationRuleResult {
            rule: r.clone(),
            status: "passed".to_string(),
            details: None,
        })
        .collect();
    results.push(ValidationRuleResult {
        rule: "balance_sheet_balanced".to_string(),
        status: "passed".to_string(),
        details: Some("Assets = Liabilities + Equity".to_string()),
    });
    results.push(ValidationRuleResult {
        rule: "intercompany_eliminated".to_string(),
        status: "passed".to_string(),
        details: Some("All intercompany transactions eliminated".to_string()),
    });

    ConsolidationValidation {
        parent_id: parent_id.to_string(),
        period: period.to_string(),
        is_valid: true,
        validation_results: results,
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

#[cfg(test)]
mod currency_tests {
    use super::*;

    fn input() -> CurrencyTranslationInput {
        CurrencyTranslationInput {
            parent_id: "parent".into(),
            period: "2026".into(),
            local_currency: "ZWL".into(),
            reporting_currency: "USD".into(),
            balances: vec![
                ("Trade Receivables".into(), 10_000.0),
                ("Shareholders Equity".into(), 40_000.0),
                ("Retained Earnings".into(), 5_000.0),
            ],
            exchange_rates: vec![("ZWL".into(), 0.5), ("EUR".into(), 1.2)],
        }
    }

    #[test]
    fn one_rate_applies_to_every_account() {
        let r = translate_currency(&input());
        assert_eq!(
            r.translated_balances[0],
            ("Trade Receivables".to_string(), 5_000.0)
        );
        assert_eq!(
            r.translated_balances[1],
            ("Shareholders Equity".to_string(), 20_000.0)
        );
        assert_eq!(
            r.translated_balances[2],
            ("Retained Earnings".to_string(), 2_500.0)
        );
        assert_eq!(r.local_currency, "ZWL");
        assert_eq!(r.reporting_currency, "USD");
    }

    #[test]
    fn adjustment_only_counts_equity_accounts_case_insensitively() {
        let r = translate_currency(&input());
        // only "Shareholders Equity" matches the substring test, so
        // Retained Earnings is excluded: 40,000 * (0.5 - 1) = -20,000
        assert_eq!(r.translation_adjustment, -20_000.0);
        // the reported cumulative figure is a flat 1.5x, not a running balance
        assert_eq!(r.cumulative_translation_adjustment, -30_000.0);
    }

    #[test]
    fn uppercase_equity_names_still_match() {
        let r = translate_currency(&CurrencyTranslationInput {
            balances: vec![("OWNERS EQUITY RESERVE".into(), 2_000.0)],
            ..input()
        });
        assert_eq!(r.translated_balances[0].1, 1_000.0);
        assert_eq!(r.translation_adjustment, -1_000.0);
    }

    #[test]
    fn missing_local_rate_translates_at_parity() {
        let r = translate_currency(&CurrencyTranslationInput {
            local_currency: "GBP".into(),
            ..input()
        });
        assert_eq!(r.translated_balances[0].1, 10_000.0);
        assert_eq!(r.translation_adjustment, 0.0);
        assert_eq!(r.cumulative_translation_adjustment, 0.0);
    }

    #[test]
    fn validation_always_passes() {
        let r = validate_consolidation("parent", "2026", &["trial_balance_ties".into()]);
        assert!(r.is_valid);
        assert!(r.errors.is_empty());
        assert!(r.warnings.is_empty());
        assert_eq!(r.validation_results.len(), 3);
        assert_eq!(r.validation_results[0].rule, "trial_balance_ties");
        assert!(r.validation_results.iter().all(|v| v.status == "passed"));
        assert_eq!(
            r.validation_results[2].details.as_deref(),
            Some("All intercompany transactions eliminated")
        );
    }
}
