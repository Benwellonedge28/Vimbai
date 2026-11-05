//! Partnership profit allocation: salaries, interest on capital, then the residual.

use serde::{Deserialize, Serialize};

use vimbai_common::numbers::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Partner {
    pub partner_id: String,
    pub name: String,
    pub capital_contribution: f64,
    pub profit_share_pct: f64,
    pub salary: f64,
    pub interest_on_capital_rate: f64,
    pub drawings: f64,
}

impl Partner {
    pub fn new(
        partner_id: &str,
        name: &str,
        capital_contribution: f64,
        profit_share_pct: f64,
    ) -> Partner {
        Partner {
            partner_id: partner_id.to_string(),
            name: name.to_string(),
            capital_contribution,
            profit_share_pct,
            salary: 0.0,
            interest_on_capital_rate: 0.05,
            drawings: 0.0,
        }
    }

    pub fn with_salary(mut self, salary: f64) -> Partner {
        self.salary = salary;
        self
    }

    pub fn with_drawings(mut self, drawings: f64) -> Partner {
        self.drawings = drawings;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartnershipRequest {
    pub company_id: String,
    pub period: String,
    pub partners: Vec<Partner>,
    pub net_profit: f64,
    pub interest_rate_on_capital: f64,
}

impl PartnershipRequest {
    pub fn new(
        company_id: &str,
        period: &str,
        partners: Vec<Partner>,
        net_profit: f64,
    ) -> PartnershipRequest {
        PartnershipRequest {
            company_id: company_id.to_string(),
            period: period.to_string(),
            partners,
            net_profit,
            interest_rate_on_capital: 0.05,
        }
    }

    pub fn at_rate(mut self, rate: f64) -> PartnershipRequest {
        self.interest_rate_on_capital = rate;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartnerAccount {
    pub partner_id: String,
    pub name: String,
    pub opening_capital: f64,
    pub salary: f64,
    pub interest_on_capital: f64,
    pub profit_share: f64,
    pub drawings: f64,
    pub closing_capital: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartnershipResult {
    pub company_id: String,
    pub period: String,
    pub total_salary: f64,
    pub total_interest: f64,
    pub total_profit_share: f64,
    pub residual_profit: f64,
    pub partner_accounts: Vec<PartnerAccount>,
}

/// Allocate a period's profit.
///
/// Preserved quirks: a residual loss is floored at zero, so the partners are
/// never allocated a negative share and the drawings still leave capital above
/// what the accounts would show in reality; and when the shares sum to zero the
/// split is made equal rather than refused. Both are load-bearing for existing
/// partner statements, so they stay, labelled.
pub fn allocate(request: &PartnershipRequest) -> PartnershipResult {
    let total_salary: f64 = request.partners.iter().map(|p| p.salary).sum();
    let total_interest: f64 = request
        .partners
        .iter()
        .map(|p| p.capital_contribution * request.interest_rate_on_capital)
        .sum();
    let residual = (request.net_profit - total_salary - total_interest).max(0.0);

    let mut shares: Vec<f64> = request
        .partners
        .iter()
        .map(|p| p.profit_share_pct)
        .collect();
    if shares.iter().sum::<f64>() == 0.0 && !request.partners.is_empty() {
        let equal = 100.0 / request.partners.len() as f64;
        shares = vec![equal; request.partners.len()];
    }

    let mut accounts = Vec::with_capacity(request.partners.len());
    let mut total_profit_share = 0.0;
    for (partner, share_pct) in request.partners.iter().zip(shares.iter()) {
        let interest = partner.capital_contribution * request.interest_rate_on_capital;
        let profit_share = residual * (share_pct / 100.0);
        total_profit_share += profit_share;
        accounts.push(PartnerAccount {
            partner_id: partner.partner_id.clone(),
            name: partner.name.clone(),
            opening_capital: partner.capital_contribution,
            salary: partner.salary,
            interest_on_capital: money(interest),
            profit_share: money(profit_share),
            drawings: partner.drawings,
            closing_capital: money(
                partner.capital_contribution + partner.salary + interest + profit_share
                    - partner.drawings,
            ),
        });
    }

    PartnershipResult {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        total_salary: money(total_salary),
        total_interest: money(total_interest),
        total_profit_share: money(total_profit_share),
        residual_profit: money(residual),
        partner_accounts: accounts,
    }
}
