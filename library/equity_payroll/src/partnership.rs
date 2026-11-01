//! partnership-sale-service: partner buyout on retirement and
//! super-profit goodwill.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemainingPartner {
    pub partner_id: String,
    pub name: String,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyoutRequest {
    pub company_id: String,
    pub outgoing_partner_name: String,
    pub capital_balance: f64,
    pub share_of_goodwill: f64,
    pub share_of_reserves: f64,
    pub loan_to_partner: f64,
    pub agreed_payment: f64,
    #[serde(default)]
    pub remaining_partners: Vec<RemainingPartner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapitalAdjustment {
    pub partner_id: String,
    pub name: String,
    pub capital_adjustment: f64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyoutResult {
    pub company_id: String,
    pub outgoing_partner: String,
    pub capital_due: f64,
    pub goodwill_due: f64,
    pub reserves_due: f64,
    pub loan_due: f64,
    pub total_payable: f64,
    pub agreed_payment: f64,
    pub difference: f64,
    pub gain_or_loss_on_retirement: f64,
    pub remaining_partner_adjustments: Vec<CapitalAdjustment>,
}

pub fn partner_buyout(req: &BuyoutRequest) -> BuyoutResult {
    let total_payable =
        req.capital_balance + req.share_of_goodwill + req.share_of_reserves + req.loan_to_partner;
    let difference = total_payable - req.agreed_payment;
    let gain_or_loss = req.agreed_payment - total_payable;

    let total_share: f64 = req.remaining_partners.iter().map(|p| p.share_pct).sum();
    let adjustments = req
        .remaining_partners
        .iter()
        .map(|p| {
            let share = if total_share != 0.0 {
                p.share_pct / total_share
            } else {
                0.0
            };
            CapitalAdjustment {
                partner_id: p.partner_id.clone(),
                name: p.name.clone(),
                capital_adjustment: py_round2(difference * share),
                share_pct: p.share_pct,
            }
        })
        .collect();

    BuyoutResult {
        company_id: req.company_id.clone(),
        outgoing_partner: req.outgoing_partner_name.clone(),
        capital_due: py_round2(req.capital_balance),
        goodwill_due: py_round2(req.share_of_goodwill),
        reserves_due: py_round2(req.share_of_reserves),
        loan_due: py_round2(req.loan_to_partner),
        total_payable: py_round2(total_payable),
        agreed_payment: py_round2(req.agreed_payment),
        difference: py_round2(difference),
        gain_or_loss_on_retirement: py_round2(gain_or_loss),
        remaining_partner_adjustments: adjustments,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartnerCapital {
    pub partner_id: String,
    pub capital: f64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodwillRequest {
    pub company_id: String,
    pub avg_profit: f64,
    pub rate: f64,
    pub years: f64,
    #[serde(default)]
    pub partners: Vec<PartnerCapital>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodwillResult {
    pub company_id: String,
    pub total_goodwill: f64,
    pub super_profit: f64,
    pub normal_return_rate: f64,
    pub partner_goodwill_shares: Vec<(String, f64)>,
}

pub fn partnership_goodwill(req: &GoodwillRequest) -> GoodwillResult {
    let total_capital: f64 = req.partners.iter().map(|p| p.capital).sum();
    let super_profit = req.avg_profit - (total_capital * req.rate);
    let goodwill = super_profit * req.years;
    let shares = req
        .partners
        .iter()
        .map(|p| {
            (
                p.partner_id.clone(),
                py_round2(goodwill * (p.share_pct / 100.0)),
            )
        })
        .collect();

    GoodwillResult {
        company_id: req.company_id.clone(),
        total_goodwill: py_round2(goodwill),
        super_profit: py_round2(super_profit),
        normal_return_rate: req.rate,
        partner_goodwill_shares: shares,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> BuyoutRequest {
        BuyoutRequest {
            company_id: "c1".into(),
            outgoing_partner_name: "Tendai".into(),
            capital_balance: 50000.0,
            share_of_goodwill: 10000.0,
            share_of_reserves: 5000.0,
            loan_to_partner: 2000.0,
            agreed_payment: 60000.0,
            remaining_partners: vec![
                RemainingPartner {
                    partner_id: "p2".into(),
                    name: "Rudo".into(),
                    share_pct: 60.0,
                },
                RemainingPartner {
                    partner_id: "p3".into(),
                    name: "Kuda".into(),
                    share_pct: 40.0,
                },
            ],
        }
    }

    #[test]
    fn buyout_totals_and_gain() {
        let r = partner_buyout(&req());
        // payable = 50k + 10k + 5k + 2k = 67k; paid 60k -> gain 7k to partner
        assert_eq!(r.total_payable, 67000.0);
        assert_eq!(r.difference, 7000.0);
        assert_eq!(r.gain_or_loss_on_retirement, -7000.0);
    }

    #[test]
    fn adjustments_use_relative_shares() {
        let r = partner_buyout(&req());
        // 7000 * 0.6 = 4200 (partner must absorb), 7000 * 0.4 = 2800
        assert_eq!(
            r.remaining_partner_adjustments[0].capital_adjustment,
            4200.0
        );
        assert_eq!(
            r.remaining_partner_adjustments[1].capital_adjustment,
            2800.0
        );
    }

    #[test]
    fn zero_share_pool_does_not_divide_by_zero() {
        let mut q = req();
        q.remaining_partners = vec![];
        let r = partner_buyout(&q);
        assert!(r.remaining_partner_adjustments.is_empty());
    }

    #[test]
    fn goodwill_super_profit() {
        let g = partnership_goodwill(&GoodwillRequest {
            company_id: "c1".into(),
            avg_profit: 40000.0,
            rate: 0.10,
            years: 3.0,
            partners: vec![
                PartnerCapital {
                    partner_id: "p1".into(),
                    capital: 100000.0,
                    share_pct: 50.0,
                },
                PartnerCapital {
                    partner_id: "p2".into(),
                    capital: 100000.0,
                    share_pct: 50.0,
                },
            ],
        });
        // super profit = 40000 - 200000*0.1 = 20000; goodwill = 60000
        assert_eq!(g.super_profit, 20000.0);
        assert_eq!(g.total_goodwill, 60000.0);
        assert_eq!(g.partner_goodwill_shares[0], ("p1".to_string(), 30000.0));
    }
}
