//! A partner leaving: what is owed, and how the difference is shared out.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use vimbai_common::numbers::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemainingPartner {
    pub partner_id: String,
    pub name: String,
    pub share_pct: f64,
}

impl RemainingPartner {
    pub fn new(partner_id: &str, name: &str, share_pct: f64) -> RemainingPartner {
        RemainingPartner {
            partner_id: partner_id.to_string(),
            name: name.to_string(),
            share_pct,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuyoutRequest {
    pub company_id: String,
    pub outgoing_partner_id: String,
    pub outgoing_partner_name: String,
    pub capital_balance: f64,
    pub share_of_goodwill: f64,
    pub share_of_reserves: f64,
    pub loan_to_partner: f64,
    pub agreed_payment: f64,
    pub remaining_partners: Vec<RemainingPartner>,
}

impl BuyoutRequest {
    pub fn new(
        company_id: &str,
        partner_id: &str,
        partner_name: &str,
        capital_balance: f64,
        agreed_payment: f64,
    ) -> BuyoutRequest {
        BuyoutRequest {
            company_id: company_id.to_string(),
            outgoing_partner_id: partner_id.to_string(),
            outgoing_partner_name: partner_name.to_string(),
            capital_balance,
            share_of_goodwill: 0.0,
            share_of_reserves: 0.0,
            loan_to_partner: 0.0,
            agreed_payment,
            remaining_partners: Vec::new(),
        }
    }

    pub fn owing(mut self, goodwill: f64, reserves: f64, loan: f64) -> BuyoutRequest {
        self.share_of_goodwill = goodwill;
        self.share_of_reserves = reserves;
        self.loan_to_partner = loan;
        self
    }

    pub fn with_remaining(mut self, partners: Vec<RemainingPartner>) -> BuyoutRequest {
        self.remaining_partners = partners;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pub remaining_partner_adjustments: Vec<Value>,
}

/// Settle a retiring partner.
///
/// Preserved quirk: `difference` is payable-minus-agreed and `gain_or_loss` is
/// its negation, so a payment *above* the dues reports a positive gain to the
/// outgoing partner. Both fields are kept with their original signs because
/// posted journals already reference them; read `difference` as the amount the
/// remaining partners absorb.
pub fn settle(request: &BuyoutRequest) -> BuyoutResult {
    let total_payable = request.capital_balance
        + request.share_of_goodwill
        + request.share_of_reserves
        + request.loan_to_partner;
    let difference = total_payable - request.agreed_payment;
    let gain_or_loss = request.agreed_payment - total_payable;

    let total_share: f64 = request.remaining_partners.iter().map(|p| p.share_pct).sum();
    let adjustments = request
        .remaining_partners
        .iter()
        .map(|partner| {
            let share = if total_share != 0.0 {
                partner.share_pct / total_share
            } else {
                0.0
            };
            json!({
                "partner_id": partner.partner_id,
                "name": partner.name,
                "capital_adjustment": money(difference * share),
                "share_pct": partner.share_pct,
            })
        })
        .collect();

    BuyoutResult {
        company_id: request.company_id.clone(),
        outgoing_partner: request.outgoing_partner_name.clone(),
        capital_due: money(request.capital_balance),
        goodwill_due: money(request.share_of_goodwill),
        reserves_due: money(request.share_of_reserves),
        loan_due: money(request.loan_to_partner),
        total_payable: money(total_payable),
        agreed_payment: money(request.agreed_payment),
        difference: money(difference),
        gain_or_loss_on_retirement: money(gain_or_loss),
        remaining_partner_adjustments: adjustments,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoodwillRequest {
    pub company_id: String,
    pub partners: Vec<RemainingPartner>,
    pub capitals: Vec<f64>,
    pub avg_profit: f64,
    pub years: i32,
    pub rate: f64,
}

impl GoodwillRequest {
    pub fn new(company_id: &str, avg_profit: f64) -> GoodwillRequest {
        GoodwillRequest {
            company_id: company_id.to_string(),
            partners: Vec::new(),
            capitals: Vec::new(),
            avg_profit,
            years: 3,
            rate: 0.15,
        }
    }

    pub fn with_capital(
        mut self,
        partner_id: &str,
        name: &str,
        share_pct: f64,
        capital: f64,
    ) -> GoodwillRequest {
        self.partners
            .push(RemainingPartner::new(partner_id, name, share_pct));
        self.capitals.push(capital);
        self
    }

    pub fn over_years(mut self, years: i32) -> GoodwillRequest {
        self.years = years;
        self
    }

    pub fn at_rate(mut self, rate: f64) -> GoodwillRequest {
        self.rate = rate;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoodwillResult {
    pub company_id: String,
    pub total_goodwill: f64,
    pub super_profit: f64,
    pub normal_return_rate: f64,
    pub partner_goodwill_shares: IndexMap<String, f64>,
}

/// Super-profit goodwill: profit above a normal return on capital, capitalised.
pub fn goodwill(request: &GoodwillRequest) -> GoodwillResult {
    let total_capital: f64 = request.capitals.iter().sum();
    let super_profit = request.avg_profit - (total_capital * request.rate);
    let total = super_profit * request.years as f64;

    let shares: IndexMap<String, f64> = request
        .partners
        .iter()
        .map(|partner| {
            (
                partner.partner_id.clone(),
                money(total * (partner.share_pct / 100.0)),
            )
        })
        .collect();

    GoodwillResult {
        company_id: request.company_id.clone(),
        total_goodwill: money(total),
        super_profit: money(super_profit),
        normal_return_rate: request.rate,
        partner_goodwill_shares: shares,
    }
}
