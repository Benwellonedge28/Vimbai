//! partnership-dissolution-service and partnership-revaluation-service:
//! realisation accounting, creditor and partner settlements, and
//! revaluation journals with goodwill treatment.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRealization {
    pub asset_id: String,
    pub asset_name: String,
    pub book_value: f64,
    pub sale_proceeds: f64,
    pub profit: f64,
    pub loss: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditorSettlement {
    pub creditor_id: String,
    pub creditor_name: String,
    pub amount_owed: f64,
    pub amount_paid: f64,
    pub discount_received: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartnerSettlement {
    pub partner_id: String,
    pub partner_name: String,
    pub capital_balance: f64,
    pub current_account_balance: f64,
    pub share_of_profit_loss: f64,
    pub total_due: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DissolutionReport {
    pub partnership_id: String,
    pub assets: Vec<AssetRealization>,
    pub creditors: Vec<CreditorSettlement>,
    pub partners: Vec<PartnerSettlement>,
    pub realization_profit: f64,
    pub realization_loss: f64,
    pub total_assets_realized: f64,
    pub total_creditors: f64,
    pub total_partners_capitals: f64,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetInput {
    pub asset_id: String,
    pub asset_name: String,
    pub book_value: f64,
    pub sale_proceeds: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreditorInput {
    pub id: String,
    pub name: String,
    pub amount: f64,
    /// Defaults to `amount` when absent, mirroring `creditor.get("paid", amount)`.
    pub paid: Option<f64>,
    #[serde(default)]
    pub discount: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartnerInput {
    pub id: String,
    pub name: String,
    pub capital: f64,
    #[serde(default)]
    pub current: f64,
    #[serde(default)]
    pub share: f64,
}

/// Partnership dissolution: realise assets, settle creditors (honouring
/// any agreed discount and partial payment), then compute each partner's
/// total due. No cash moves; the report is a record only.
pub fn dissolve_partnership(
    partnership_id: &str,
    assets: &[AssetInput],
    creditors: &[CreditorInput],
    partners: &[PartnerInput],
) -> DissolutionReport {
    let mut report = DissolutionReport {
        partnership_id: partnership_id.to_string(),
        status: "completed".to_string(),
        ..Default::default()
    };

    for asset in assets {
        let mut realization = AssetRealization {
            asset_id: asset.asset_id.clone(),
            asset_name: asset.asset_name.clone(),
            book_value: asset.book_value,
            sale_proceeds: asset.sale_proceeds,
            profit: 0.0,
            loss: 0.0,
        };
        if realization.sale_proceeds > realization.book_value {
            realization.profit = realization.sale_proceeds - realization.book_value;
            report.realization_profit += realization.profit;
        } else {
            realization.loss = realization.book_value - realization.sale_proceeds;
            report.realization_loss += realization.loss;
        }
        report.total_assets_realized += realization.sale_proceeds;
        report.assets.push(realization);
    }

    for creditor in creditors {
        let settlement = CreditorSettlement {
            creditor_id: creditor.id.clone(),
            creditor_name: creditor.name.clone(),
            amount_owed: creditor.amount,
            amount_paid: creditor.paid.unwrap_or(creditor.amount),
            discount_received: creditor.discount,
        };
        report.total_creditors += settlement.amount_paid;
        report.creditors.push(settlement);
    }

    for partner in partners {
        let total_due = partner.capital + partner.current + partner.share;
        report.total_partners_capitals += total_due;
        report.partners.push(PartnerSettlement {
            partner_id: partner.id.clone(),
            partner_name: partner.name.clone(),
            capital_balance: partner.capital,
            current_account_balance: partner.current,
            share_of_profit_loss: partner.share,
            total_due,
        });
    }

    report
}

// ---------------------------------------------------------------------
// Revaluation
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodwillTreatment {
    RaiseAndRaise,
    RaiseAndWriteOff,
    WriteOffOnly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevaluationInput {
    pub asset_id: String,
    pub asset_name: String,
    pub asset_code: String,
    pub old_value: f64,
    pub new_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLineOut {
    pub account_code: String,
    pub description: String,
    pub debit: f64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalOut {
    pub description: String,
    pub entries: Vec<JournalLineOut>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevaluationReport {
    pub total_increase: f64,
    pub total_decrease: f64,
    pub journal_entries: Vec<JournalOut>,
}

fn line(code: &str, desc: &str, debit: f64, credit: f64) -> JournalLineOut {
    JournalLineOut {
        account_code: code.to_string(),
        description: desc.to_string(),
        debit: py_round2(debit),
        credit: py_round2(credit),
    }
}

/// Partnership revaluation: gains to the revaluation reserve (3100),
/// losses to 6200, goodwill raised against the reserve only for the
/// raise-and-raise treatment.
pub fn revalue_partnership(
    revaluations: &[RevaluationInput],
    goodwill_treatment: GoodwillTreatment,
    goodwill_amount: f64,
) -> RevaluationReport {
    let mut report = RevaluationReport::default();

    for rev in revaluations {
        if rev.new_value > rev.old_value {
            let increase = rev.new_value - rev.old_value;
            report.total_increase += increase;
            report.journal_entries.push(JournalOut {
                description: format!("Revaluation gain - {}", rev.asset_name),
                entries: vec![
                    line(&rev.asset_code, &rev.asset_name, increase, 0.0),
                    line("3100", "Revaluation Reserve", 0.0, increase),
                ],
            });
        } else {
            let decrease = rev.old_value - rev.new_value;
            report.total_decrease += decrease;
            report.journal_entries.push(JournalOut {
                description: format!("Revaluation loss - {}", rev.asset_name),
                entries: vec![
                    line("6200", "Revaluation Loss", decrease, 0.0),
                    line(&rev.asset_code, &rev.asset_name, 0.0, decrease),
                ],
            });
        }
    }

    if goodwill_amount > 0.0 && goodwill_treatment == GoodwillTreatment::RaiseAndRaise {
        report.journal_entries.push(JournalOut {
            description: "Goodwill arising on revaluation".to_string(),
            entries: vec![
                line("1500", "Goodwill", goodwill_amount, 0.0),
                line("3100", "Revaluation Reserve", 0.0, goodwill_amount),
            ],
        });
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: &str, book: f64, proceeds: f64) -> AssetInput {
        AssetInput {
            asset_id: id.into(),
            asset_name: "Asset".into(),
            book_value: book,
            sale_proceeds: proceeds,
        }
    }

    #[test]
    fn dissolution_splits_gains_and_losses() {
        let r = dissolve_partnership(
            "p1",
            &[asset("a1", 1000.0, 1200.0), asset("a2", 500.0, 400.0)],
            &[],
            &[],
        );
        assert_eq!(r.realization_profit, 200.0);
        assert_eq!(r.realization_loss, 100.0);
        assert_eq!(r.total_assets_realized, 1600.0);
        assert_eq!(r.status, "completed");
    }

    #[test]
    fn equal_proceeds_is_a_loss_of_zero_not_a_profit() {
        let r = dissolve_partnership("p1", &[asset("a1", 500.0, 500.0)], &[], &[]);
        assert_eq!(r.realization_profit, 0.0);
        assert_eq!(r.realization_loss, 0.0);
    }

    #[test]
    fn creditor_defaults_paid_to_amount_owed() {
        let r = dissolve_partnership(
            "p1",
            &[],
            &[
                CreditorInput {
                    id: "c1".into(),
                    name: "Bank".into(),
                    amount: 500.0,
                    paid: None,
                    discount: 0.0,
                },
                CreditorInput {
                    id: "c2".into(),
                    name: "Supplier".into(),
                    amount: 300.0,
                    paid: Some(250.0),
                    discount: 50.0,
                },
            ],
            &[],
        );
        assert_eq!(r.creditors[0].amount_paid, 500.0);
        assert_eq!(r.creditors[1].amount_paid, 250.0);
        assert_eq!(r.total_creditors, 750.0);
    }

    #[test]
    fn partner_total_due_sums_three_balances() {
        let r = dissolve_partnership(
            "p1",
            &[],
            &[],
            &[PartnerInput {
                id: "u1".into(),
                name: "Ann".into(),
                capital: 10_000.0,
                current: 500.0,
                share: -200.0,
            }],
        );
        assert_eq!(r.partners[0].total_due, 10_300.0);
        assert_eq!(r.total_partners_capitals, 10_300.0);
    }

    fn rev(name: &str, old: f64, new: f64) -> RevaluationInput {
        RevaluationInput {
            asset_id: "a1".into(),
            asset_name: name.into(),
            asset_code: "1200".into(),
            old_value: old,
            new_value: new,
        }
    }

    #[test]
    fn revaluation_gain_and_loss_journals() {
        let r = revalue_partnership(
            &[
                rev("Building", 100_000.0, 120_000.0),
                rev("Plant", 50_000.0, 45_000.0),
            ],
            GoodwillTreatment::WriteOffOnly,
            0.0,
        );
        assert_eq!(r.total_increase, 20_000.0);
        assert_eq!(r.total_decrease, 5_000.0);
        assert_eq!(r.journal_entries.len(), 2);
        assert_eq!(r.journal_entries[0].entries[1].account_code, "3100");
        assert_eq!(r.journal_entries[1].entries[0].account_code, "6200");
    }

    #[test]
    fn goodwill_only_raised_for_raise_and_raise() {
        let none = revalue_partnership(&[], GoodwillTreatment::WriteOffOnly, 15_000.0);
        assert!(none.journal_entries.is_empty());

        let raised = revalue_partnership(&[], GoodwillTreatment::RaiseAndRaise, 15_000.0);
        assert_eq!(raised.journal_entries.len(), 1);
        assert_eq!(raised.journal_entries[0].entries[0].account_code, "1500");
        assert_eq!(raised.journal_entries[0].entries[1].credit, 15_000.0);
    }
}
