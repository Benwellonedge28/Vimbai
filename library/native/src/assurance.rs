//! The assurance, group, performance and risk libraries behind one JSON door.
//!
//! Each function takes the payload the Python service receives and returns the
//! payload its response model would serialise. No arithmetic lives here: the
//! request is turned into library types, the library does the work, and the
//! result is serialised back. That keeps the numbers identical to the Rust tests.

use std::collections::HashMap;

use serde::Deserialize;

use vimbai_group_accounting::buyout::{self, BuyoutRequest, RemainingPartner};
use vimbai_group_accounting::combination::{self, AcquisitionRequest, IdentifiableAsset};
use vimbai_group_accounting::consolidation::{
    self, ConsolidationRequest, IntercompanyTransaction, ParentFinancials, SubsidiaryFinancials,
};
use vimbai_group_accounting::impairment::{self, Cgu, ImpairmentRequest, TestMethod};
use vimbai_group_accounting::partnership::{self, Partner, PartnershipRequest};
use vimbai_group_accounting::pension::{self, PensionRequest};
use vimbai_market_risk::credit::{self as credit_risk};
use vimbai_market_risk::distress;
use vimbai_market_risk::fraud;
use vimbai_market_risk::options;
use vimbai_market_risk::regulatory;
use vimbai_market_risk::var;
use vimbai_advisory::evaluate_controls;
use vimbai_performance_measures::controls;
use vimbai_performance_measures::fx;
use vimbai_performance_measures::inventory;
use vimbai_performance_measures::profit;
use vimbai_performance_measures::realizable;
use vimbai_performance_measures::scorecard;
use vimbai_tax_reporting::credit::{self, ClaimRequest, Expenditure};
use vimbai_tax_reporting::grants::{self, GrantRecognition, GrantRequest, GrantType};
use vimbai_tax_reporting::group::{self, GroupTaxRequest, Subsidiary};
use vimbai_tax_reporting::provision::{self, ProvisionRequest, TemporaryDifference};
use vimbai_tax_reporting::risk::{self, TaxRiskItem};
use vimbai_tax_reporting::vat::{self, VatReturnRequest, VatTransaction};
use vimbai_tax_reporting::xbrl::{self, FinancialConcept, XbrlRequest};

#[derive(Debug, Clone, Deserialize)]
pub struct ExpenditureDto {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    pub amount: f64,
    #[serde(default)]
    pub qualifies: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditDto {
    pub company_id: String,
    pub fiscal_year: i32,
    #[serde(default)]
    pub expenditures: Vec<ExpenditureDto>,
    /// The service let a caller override the statutory rates, so the seam must too.
    #[serde(default)]
    pub credit_rate: Option<f64>,
    #[serde(default)]
    pub alternative_rate: Option<f64>,
}

pub fn credit_json(payload: &str) -> Result<String, String> {
    let dto: CreditDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let mut request = ClaimRequest::new(
        &dto.company_id,
        dto.fiscal_year,
        dto.expenditures
            .iter()
            .map(|e| Expenditure {
                category: e.category.clone(),
                description: e.description.clone(),
                amount: e.amount,
                qualifies: e.qualifies,
            })
            .collect(),
    );
    if let Some(rate) = dto.credit_rate {
        request.credit_rate = rate;
    }
    if let Some(rate) = dto.alternative_rate {
        request.alternative_rate = rate;
    }
    serde_json::to_string(&credit::calculate(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubsidiaryDto {
    pub entity_id: String,
    #[serde(default)]
    pub entity_name: String,
    #[serde(default)]
    pub jurisdiction: String,
    pub pre_tax_income: f64,
    #[serde(default)]
    pub tax_paid: f64,
    pub tax_rate: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupTaxDto {
    pub group_id: String,
    pub fiscal_year: i32,
    pub group_tax_rate: f64,
    #[serde(default)]
    pub intercompany_eliminations: f64,
    #[serde(default)]
    pub subsidiaries: Vec<SubsidiaryDto>,
}

pub fn group_tax_json(payload: &str) -> Result<String, String> {
    let dto: GroupTaxDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = GroupTaxRequest {
        group_id: dto.group_id,
        fiscal_year: dto.fiscal_year,
        group_tax_rate: dto.group_tax_rate,
        intercompany_eliminations: dto.intercompany_eliminations,
        subsidiaries: dto
            .subsidiaries
            .iter()
            .map(|s| Subsidiary {
                entity_id: s.entity_id.clone(),
                entity_name: s.entity_name.clone(),
                jurisdiction: s.jurisdiction.clone(),
                pre_tax_income: s.pre_tax_income,
                tax_paid: s.tax_paid,
                tax_rate: s.tax_rate,
                loss_carryforward: 0.0,
            })
            .collect(),
    };
    serde_json::to_string(&group::consolidate(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct VatTransactionDto {
    #[serde(default)]
    pub description: String,
    pub amount: f64,
    pub vat_rate: f64,
    #[serde(default)]
    pub transaction_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VatDto {
    pub company_id: String,
    pub tax_period: String,
    #[serde(default)]
    pub output_transactions: Vec<VatTransactionDto>,
    #[serde(default)]
    pub input_transactions: Vec<VatTransactionDto>,
}

fn vat_lines(lines: &[VatTransactionDto]) -> Vec<VatTransaction> {
    lines
        .iter()
        .map(|t| VatTransaction {
            description: t.description.clone(),
            amount: t.amount,
            vat_rate: t.vat_rate,
            transaction_type: t.transaction_type.clone(),
        })
        .collect()
}

pub fn vat_return_json(payload: &str) -> Result<String, String> {
    let dto: VatDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = VatReturnRequest {
        company_id: dto.company_id,
        tax_period: dto.tax_period,
        output_transactions: vat_lines(&dto.output_transactions),
        input_transactions: vat_lines(&dto.input_transactions),
    };
    serde_json::to_string(&vat::prepare(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DifferenceDto {
    #[serde(default)]
    pub description: String,
    pub book_amount: f64,
    pub tax_amount: f64,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvisionDto {
    pub company_id: String,
    pub fiscal_year: i32,
    pub pre_tax_income: f64,
    pub statutory_rate: f64,
    #[serde(default)]
    pub permanent_differences: f64,
    #[serde(default)]
    pub temp_differences: Vec<DifferenceDto>,
    #[serde(default)]
    pub loss_carryforward: f64,
    #[serde(default)]
    pub valuation_allowance: f64,
}

pub fn tax_provision_json(payload: &str) -> Result<String, String> {
    let dto: ProvisionDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = ProvisionRequest {
        company_id: dto.company_id,
        fiscal_year: dto.fiscal_year,
        pre_tax_income: dto.pre_tax_income,
        statutory_rate: dto.statutory_rate,
        permanent_differences: dto.permanent_differences,
        temp_differences: dto
            .temp_differences
            .iter()
            .map(|d| TemporaryDifference {
                description: d.description.clone(),
                book_amount: d.book_amount,
                tax_amount: d.tax_amount,
                r#type: d.kind.clone(),
            })
            .collect(),
        loss_carryforward: dto.loss_carryforward,
        valuation_allowance: dto.valuation_allowance,
    };
    serde_json::to_string(&provision::calculate(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskItemDto {
    pub risk_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub risk_type: String,
    pub potential_exposure: f64,
    pub probability: f64,
    #[serde(default)]
    pub jurisdiction: String,
    #[serde(default)]
    pub mitigation_actions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaxRiskDto {
    pub company_id: String,
    #[serde(default)]
    pub assessment_date: String,
    #[serde(default)]
    pub risks: Vec<RiskItemDto>,
}

pub fn tax_risk_json(payload: &str) -> Result<String, String> {
    let dto: TaxRiskDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let risks: Vec<TaxRiskItem> = dto
        .risks
        .iter()
        .map(|r| TaxRiskItem {
            risk_id: r.risk_id.clone(),
            description: r.description.clone(),
            risk_type: r.risk_type.clone(),
            potential_exposure: r.potential_exposure,
            probability: r.probability,
            jurisdiction: r.jurisdiction.clone(),
            mitigation_actions: r.mitigation_actions.clone(),
        })
        .collect();
    serde_json::to_string(&risk::assess(&dto.company_id, &dto.assessment_date, &risks))
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrantDto {
    pub company_id: String,
    #[serde(default)]
    pub grant_name: String,
    #[serde(default)]
    pub granting_authority: String,
    pub grant_amount: f64,
    #[serde(default)]
    pub grant_type: String,
    #[serde(default)]
    pub recognition_method: String,
    #[serde(default = "default_life")]
    pub asset_useful_life: i32,
    #[serde(default)]
    pub related_asset_cost: f64,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default = "default_compliance")]
    pub compliance_status: String,
}

fn default_life() -> i32 {
    5
}

fn default_compliance() -> String {
    "compliant".to_string()
}

pub fn grant_json(payload: &str) -> Result<String, String> {
    let dto: GrantDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let grant_type = match dto.grant_type.as_str() {
        "income" => GrantType::Income,
        _ => GrantType::Asset,
    };
    let recognition = match dto.recognition_method.as_str() {
        "deduction_from_asset" => GrantRecognition::DeductionFromAsset,
        _ => GrantRecognition::DeferredIncome,
    };
    let request = GrantRequest {
        company_id: dto.company_id,
        grant_name: dto.grant_name,
        granting_authority: dto.granting_authority,
        grant_amount: dto.grant_amount,
        grant_type,
        recognition_method: recognition,
        asset_useful_life: dto.asset_useful_life,
        related_asset_cost: dto.related_asset_cost,
        conditions: dto.conditions,
        compliance_status: dto.compliance_status,
    };
    serde_json::to_string(&grants::recognize(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConceptDto {
    pub concept: String,
    pub value: f64,
    #[serde(default = "default_context")]
    pub context_ref: String,
    #[serde(default = "default_unit")]
    pub unit_ref: String,
    #[serde(default = "default_decimals")]
    pub decimals: i32,
}

fn default_context() -> String {
    "current_year".to_string()
}

fn default_unit() -> String {
    "USD".to_string()
}

fn default_decimals() -> i32 {
    -2
}

#[derive(Debug, Clone, Deserialize)]
pub struct XbrlDto {
    pub company_id: String,
    pub fiscal_year: i32,
    #[serde(default)]
    pub entity_name: String,
    #[serde(default)]
    pub entity_identifier: String,
    #[serde(default = "default_taxonomy")]
    pub taxonomy: String,
    #[serde(default)]
    pub concepts: Vec<ConceptDto>,
}

fn default_taxonomy() -> String {
    "ifrs-full".to_string()
}

pub fn xbrl_json(payload: &str, generated_at: &str) -> Result<String, String> {
    let dto: XbrlDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = XbrlRequest {
        company_id: dto.company_id,
        fiscal_year: dto.fiscal_year,
        entity_name: dto.entity_name,
        entity_identifier: dto.entity_identifier,
        taxonomy: dto.taxonomy,
        concepts: dto
            .concepts
            .iter()
            .map(|c| FinancialConcept {
                concept: c.concept.clone(),
                value: c.value,
                context_ref: c.context_ref.clone(),
                unit_ref: c.unit_ref.clone(),
                decimals: c.decimals,
            })
            .collect(),
    };
    serde_json::to_string(&xbrl::generate(&request, generated_at)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartnerDto {
    pub partner_id: String,
    #[serde(default)]
    pub name: String,
    pub capital_contribution: f64,
    pub profit_share_pct: f64,
    #[serde(default)]
    pub salary: f64,
    #[serde(default = "default_rate")]
    pub interest_on_capital_rate: f64,
    #[serde(default)]
    pub drawings: f64,
}

fn default_rate() -> f64 {
    0.05
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartnershipDto {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    pub net_profit: f64,
    #[serde(default = "default_rate")]
    pub interest_rate_on_capital: f64,
    #[serde(default)]
    pub partners: Vec<PartnerDto>,
}

pub fn partnership_allocation_json(payload: &str) -> Result<String, String> {
    let dto: PartnershipDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = PartnershipRequest {
        company_id: dto.company_id,
        period: dto.period,
        net_profit: dto.net_profit,
        interest_rate_on_capital: dto.interest_rate_on_capital,
        partners: dto
            .partners
            .iter()
            .map(|p| Partner {
                partner_id: p.partner_id.clone(),
                name: p.name.clone(),
                capital_contribution: p.capital_contribution,
                profit_share_pct: p.profit_share_pct,
                salary: p.salary,
                interest_on_capital_rate: p.interest_on_capital_rate,
                drawings: p.drawings,
            })
            .collect(),
    };
    serde_json::to_string(&partnership::allocate(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemainingPartnerDto {
    pub partner_id: String,
    #[serde(default)]
    pub name: String,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuyoutDto {
    pub company_id: String,
    #[serde(default)]
    pub outgoing_partner_id: String,
    #[serde(default)]
    pub outgoing_partner_name: String,
    pub capital_balance: f64,
    #[serde(default)]
    pub share_of_goodwill: f64,
    #[serde(default)]
    pub share_of_reserves: f64,
    #[serde(default)]
    pub loan_to_partner: f64,
    pub agreed_payment: f64,
    #[serde(default)]
    pub remaining_partners: Vec<RemainingPartnerDto>,
}

pub fn partner_buyout_json(payload: &str) -> Result<String, String> {
    let dto: BuyoutDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = BuyoutRequest {
        company_id: dto.company_id,
        outgoing_partner_id: dto.outgoing_partner_id,
        outgoing_partner_name: dto.outgoing_partner_name,
        capital_balance: dto.capital_balance,
        share_of_goodwill: dto.share_of_goodwill,
        share_of_reserves: dto.share_of_reserves,
        loan_to_partner: dto.loan_to_partner,
        agreed_payment: dto.agreed_payment,
        remaining_partners: dto
            .remaining_partners
            .iter()
            .map(|p| RemainingPartner {
                partner_id: p.partner_id.clone(),
                name: p.name.clone(),
                share_pct: p.share_pct,
            })
            .collect(),
    };
    serde_json::to_string(&buyout::settle(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoodwillPartnerDto {
    pub partner_id: String,
    #[serde(default)]
    pub name: String,
    pub capital: f64,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PartnershipGoodwillDto {
    pub company_id: String,
    pub avg_profit: f64,
    #[serde(default = "default_years")]
    pub years: i32,
    #[serde(default)]
    pub rate: f64,
    #[serde(default)]
    pub partners: Vec<GoodwillPartnerDto>,
}

fn default_years() -> i32 {
    3
}

pub fn partnership_goodwill_json(payload: &str) -> Result<String, String> {
    let dto: PartnershipGoodwillDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = buyout::GoodwillRequest {
        company_id: dto.company_id,
        avg_profit: dto.avg_profit,
        years: dto.years,
        rate: dto.rate,
        partners: dto
            .partners
            .iter()
            .map(|p| RemainingPartner {
                partner_id: p.partner_id.clone(),
                name: p.name.clone(),
                share_pct: p.share_pct,
            })
            .collect(),
        capitals: dto.partners.iter().map(|p| p.capital).collect(),
    };
    serde_json::to_string(&buyout::goodwill(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetDto {
    #[serde(default)]
    pub name: String,
    pub fair_value: f64,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AcquisitionDto {
    pub company_id: String,
    #[serde(default)]
    pub acquirer: String,
    #[serde(default)]
    pub acquiree: String,
    pub purchase_price: f64,
    #[serde(default = "default_true")]
    pub goodwill_recognized: bool,
    #[serde(default)]
    pub identifiable_assets: Vec<AssetDto>,
    #[serde(default)]
    pub contingent_consideration: f64,
    #[serde(default)]
    pub acquisition_costs: f64,
    #[serde(default)]
    pub non_controlling_interest: f64,
}

fn default_true() -> bool {
    true
}

pub fn acquisition_json(payload: &str) -> Result<String, String> {
    let dto: AcquisitionDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = AcquisitionRequest {
        company_id: dto.company_id,
        acquirer: dto.acquirer,
        acquiree: dto.acquiree,
        purchase_price: dto.purchase_price,
        goodwill_recognized: dto.goodwill_recognized,
        identifiable_assets: dto
            .identifiable_assets
            .iter()
            .map(|a| IdentifiableAsset {
                name: a.name.clone(),
                fair_value: a.fair_value,
                r#type: a.kind.clone(),
            })
            .collect(),
        contingent_consideration: dto.contingent_consideration,
        acquisition_costs: dto.acquisition_costs,
        non_controlling_interest: dto.non_controlling_interest,
    };
    serde_json::to_string(&combination::acquire(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct CguDto {
    #[serde(default)]
    pub cgu_id: String,
    #[serde(default)]
    pub name: String,
    pub carrying_value: f64,
    #[serde(default)]
    pub goodwill_allocated: f64,
    #[serde(default)]
    pub fair_value: f64,
    #[serde(default)]
    pub value_in_use: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImpairmentDto {
    pub company_id: String,
    pub fiscal_year: i32,
    #[serde(default = "default_test")]
    pub test_method: String,
    pub cgu: CguDto,
}

fn default_test() -> String {
    "higher_of".to_string()
}

pub fn goodwill_impairment_json(payload: &str) -> Result<String, String> {
    let dto: ImpairmentDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = ImpairmentRequest {
        company_id: dto.company_id,
        fiscal_year: dto.fiscal_year,
        test_method: TestMethod::parse(&dto.test_method),
        cgu: Cgu {
            cgu_id: dto.cgu.cgu_id,
            name: dto.cgu.name,
            carrying_value: dto.cgu.carrying_value,
            goodwill_allocated: dto.cgu.goodwill_allocated,
            fair_value: dto.cgu.fair_value,
            value_in_use: dto.cgu.value_in_use,
        },
    };
    serde_json::to_string(&impairment::test(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PensionDto {
    pub company_id: String,
    pub fiscal_year: i32,
    #[serde(default)]
    pub plan_name: String,
    pub obligation_beginning: f64,
    #[serde(default)]
    pub service_cost: f64,
    #[serde(default)]
    pub interest_cost: f64,
    #[serde(default)]
    pub benefits_paid: f64,
    #[serde(default)]
    pub actuarial_gain_loss: f64,
    #[serde(default)]
    pub past_service_cost: f64,
    pub plan_assets_beginning: f64,
    #[serde(default)]
    pub contributions: f64,
    #[serde(default)]
    pub expected_return: f64,
    #[serde(default)]
    pub actual_return: f64,
    #[serde(default)]
    pub benefits_paid_from_assets: f64,
}

pub fn pension_json(payload: &str) -> Result<String, String> {
    let dto: PensionDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = PensionRequest {
        company_id: dto.company_id,
        fiscal_year: dto.fiscal_year,
        plan_name: dto.plan_name,
        obligation_beginning: dto.obligation_beginning,
        service_cost: dto.service_cost,
        interest_cost: dto.interest_cost,
        benefits_paid: dto.benefits_paid,
        actuarial_gain_loss: dto.actuarial_gain_loss,
        past_service_cost: dto.past_service_cost,
        plan_assets_beginning: dto.plan_assets_beginning,
        contributions: dto.contributions,
        expected_return: dto.expected_return,
        actual_return: dto.actual_return,
        benefits_paid_from_assets: dto.benefits_paid_from_assets,
    };
    serde_json::to_string(&pension::calculate(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubsidiaryFinancialsDto {
    pub subsidiary_id: String,
    #[serde(default)]
    pub name: String,
    pub ownership_percentage: f64,
    #[serde(default)]
    pub non_controlling_interests_percentage: Option<f64>,
    pub revenue: f64,
    pub profit: f64,
    pub assets: f64,
    pub liabilities: f64,
    #[serde(default)]
    pub equity: Option<f64>,
    #[serde(default)]
    pub goodwill: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntercompanyDto {
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_type")]
    pub transaction_type: String,
    pub amount: f64,
}

fn default_type() -> String {
    "sale".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsolidationDto {
    pub parent_company_id: String,
    #[serde(default)]
    pub period: String,
    pub parent_revenue: f64,
    pub parent_profit: f64,
    pub parent_assets: f64,
    pub parent_liabilities: f64,
    #[serde(default)]
    pub subsidiaries: Vec<SubsidiaryFinancialsDto>,
    #[serde(default)]
    pub intercompany_transactions: Vec<IntercompanyDto>,
}

pub fn consolidation_json(payload: &str) -> Result<String, String> {
    let dto: ConsolidationDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = ConsolidationRequest {
        parent_company_id: dto.parent_company_id,
        period: dto.period,
        parent: ParentFinancials {
            revenue: dto.parent_revenue,
            profit: dto.parent_profit,
            assets: dto.parent_assets,
            liabilities: dto.parent_liabilities,
        },
        subsidiaries: dto
            .subsidiaries
            .iter()
            .map(|s| SubsidiaryFinancials {
                subsidiary_id: s.subsidiary_id.clone(),
                name: s.name.clone(),
                ownership_percentage: s.ownership_percentage,
                non_controlling_interests_percentage: s
                    .non_controlling_interests_percentage
                    .unwrap_or(100.0 - s.ownership_percentage),
                revenue: s.revenue,
                profit: s.profit,
                assets: s.assets,
                liabilities: s.liabilities,
                equity: s.equity.unwrap_or(s.assets - s.liabilities),
                goodwill: s.goodwill,
            })
            .collect(),
        intercompany_transactions: dto
            .intercompany_transactions
            .iter()
            .map(|t| IntercompanyTransaction {
                description: t.description.clone(),
                transaction_type: t.transaction_type.clone(),
                amount: t.amount,
            })
            .collect(),
    };
    serde_json::to_string(&consolidation::consolidate(&request)).map_err(|e| e.to_string())
}

// ---- performance measures ----

#[derive(Debug, Clone, Deserialize)]
pub struct KpiDto {
    pub name: String,
    #[serde(default = "default_financial")]
    pub perspective: String,
    pub target: f64,
    pub actual: f64,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(default)]
    pub unit: String,
}

fn default_financial() -> String {
    "financial".to_string()
}

fn default_weight() -> f64 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScorecardDto {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub kpis: Vec<KpiDto>,
}

pub fn scorecard_json(payload: &str) -> Result<String, String> {
    let dto: ScorecardDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = scorecard::ScorecardRequest {
        company_id: dto.company_id,
        period: dto.period,
        kpis: dto
            .kpis
            .iter()
            .map(|k| scorecard::Kpi {
                name: k.name.clone(),
                perspective: k.perspective.clone(),
                target: k.target,
                actual: k.actual,
                weight: k.weight,
                unit: k.unit.clone(),
            })
            .collect(),
    };
    serde_json::to_string(&scorecard::score(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControlDto {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub control_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub process: String,
    #[serde(default = "default_sample")]
    pub test_sample_size: i64,
    #[serde(default)]
    pub exceptions_found: i64,
    #[serde(default)]
    pub last_tested: String,
}

fn default_sample() -> i64 {
    25
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControlsDto {
    pub company_id: String,
    pub fiscal_year: i32,
    #[serde(default)]
    pub controls: Vec<ControlDto>,
}

pub fn controls_json(payload: &str) -> Result<String, String> {
    let dto: ControlsDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = controls::TestingRequest {
        company_id: dto.company_id,
        fiscal_year: dto.fiscal_year,
        controls: dto
            .controls
            .iter()
            .map(|c| controls::Control {
                id: c.id.clone(),
                name: c.name.clone(),
                control_type: c.control_type.clone(),
                description: c.description.clone(),
                process: c.process.clone(),
                test_sample_size: c.test_sample_size,
                exceptions_found: c.exceptions_found,
                last_tested: c.last_tested.clone(),
                remediation_needed: c.exceptions_found > 0,
            })
            .collect(),
    };
    serde_json::to_string(&controls::test(&request)).map_err(|e| e.to_string())
}

pub fn profit_json(payload: &str) -> Result<String, String> {
    let dto: ProfitDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    serde_json::to_string(&profit::calculate(dto.revenue, dto.total_costs))
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfitDto {
    pub revenue: f64,
    pub total_costs: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PurchaseDto {
    pub date: String,
    pub quantity: i64,
    pub unit_cost: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaleDto {
    pub date: String,
    pub quantity: i64,
    pub unit_price: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InventoryDto {
    pub company_id: String,
    #[serde(default = "default_fifo")]
    pub method: String,
    #[serde(default)]
    pub purchases: Vec<PurchaseDto>,
    #[serde(default)]
    pub sales: Vec<SaleDto>,
    #[serde(default)]
    pub opening_inventory: f64,
    #[serde(default)]
    pub opening_qty: i64,
}

fn default_fifo() -> String {
    "fifo".to_string()
}

pub fn inventory_json(payload: &str) -> Result<String, String> {
    let dto: InventoryDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = inventory::ValuationRequest {
        company_id: dto.company_id,
        method: inventory::ValuationMethod::parse(&dto.method),
        purchases: dto
            .purchases
            .iter()
            .map(|p| inventory::PurchaseRecord {
                date: p.date.clone(),
                quantity: p.quantity,
                unit_cost: p.unit_cost,
            })
            .collect(),
        sales: dto
            .sales
            .iter()
            .map(|s| inventory::SaleRecord {
                date: s.date.clone(),
                quantity: s.quantity,
                unit_price: s.unit_price,
            })
            .collect(),
        opening_inventory: dto.opening_inventory,
        opening_qty: dto.opening_qty,
    };
    serde_json::to_string(&inventory::calculate(&request)).map_err(|e| e.to_string())
}

pub fn nrv_json(payload: &str) -> Result<String, String> {
    let dto: NrvDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let selling = dto.selling_price.or(dto.selling_price_upper).unwrap_or(0.0);
    let completion = dto
        .completion_cost
        .or(dto.completion_cost_upper)
        .unwrap_or(0.0);
    serde_json::to_string(&realizable::net_realizable_value(
        selling,
        completion,
        dto.selling_cost,
    ))
    .map_err(|e| e.to_string())
}

/// The service accepted the first two fields in either spelling, because the
/// endpoint's parameters were named `selling_Price` and `Completion_Cost`.
#[derive(Debug, Clone, Deserialize)]
pub struct NrvDto {
    #[serde(default)]
    pub selling_price: Option<f64>,
    #[serde(default, rename = "selling_Price")]
    pub selling_price_upper: Option<f64>,
    #[serde(default)]
    pub completion_cost: Option<f64>,
    #[serde(default, rename = "Completion_Cost")]
    pub completion_cost_upper: Option<f64>,
    #[serde(default)]
    pub selling_cost: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScrapDto {
    pub original_cost: f64,
    pub accumulated_depreciation: f64,
    #[serde(default)]
    pub disposal_cost: f64,
    #[serde(default)]
    pub proceeds_from_sale: f64,
}

pub fn scrap_json(payload: &str) -> Result<String, String> {
    let dto: ScrapDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    serde_json::to_string(&realizable::scrap_value(
        dto.original_cost,
        dto.accumulated_depreciation,
        dto.disposal_cost,
        dto.proceeds_from_sale,
    ))
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResidualDto {
    pub original_cost: f64,
    pub depreciation_rate: f64,
    pub useful_life_years: i32,
}

pub fn residual_json(payload: &str) -> Result<String, String> {
    let dto: ResidualDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    serde_json::to_string(&realizable::residual_value(
        dto.original_cost,
        dto.depreciation_rate,
        dto.useful_life_years,
    ))
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversionDto {
    pub from_currency: String,
    pub to_currency: String,
    pub amount: f64,
    #[serde(default)]
    pub rate_date: String,
}

pub fn fx_convert_json(payload: &str, today: &str) -> Result<String, String> {
    let dto: ConversionDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = fx::ConversionRequest {
        from_currency: dto.from_currency,
        to_currency: dto.to_currency,
        amount: dto.amount,
        rate_date: dto.rate_date,
    };
    serde_json::to_string(&fx::convert(&request, today).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct HedgeDto {
    pub company_id: String,
    pub currency: String,
    pub exposure_amount: f64,
    #[serde(default = "default_weight")]
    pub hedge_ratio: f64,
    #[serde(default)]
    pub forward_rate: f64,
}

pub fn fx_hedge_json(payload: &str) -> Result<String, String> {
    let dto: HedgeDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = fx::HedgeRequest {
        company_id: dto.company_id,
        currency: dto.currency,
        exposure_amount: dto.exposure_amount,
        hedge_ratio: dto.hedge_ratio,
        forward_rate: dto.forward_rate,
    };
    serde_json::to_string(&fx::hedge(&request).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

// ---- market and credit risk ----

#[derive(Debug, Clone, Deserialize)]
pub struct OptionDto {
    #[serde(default = "default_call")]
    pub option_type: String,
    pub strike_price: f64,
    pub spot_price: f64,
    pub time_to_expiry: f64,
    pub volatility: f64,
    pub risk_free_rate: f64,
    #[serde(default = "default_weight")]
    pub notional: f64,
}

fn default_call() -> String {
    "call".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PricingDto {
    pub company_id: String,
    #[serde(default)]
    pub options: Vec<OptionDto>,
}

pub fn options_price_json(payload: &str) -> Result<String, String> {
    let dto: PricingDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = options::PricingRequest {
        company_id: dto.company_id,
        options: dto
            .options
            .iter()
            .map(|o| options::Option {
                option_type: o.option_type.clone(),
                strike_price: o.strike_price,
                spot_price: o.spot_price,
                time_to_expiry: o.time_to_expiry,
                volatility: o.volatility,
                risk_free_rate: o.risk_free_rate,
                notional: o.notional,
            })
            .collect(),
    };
    serde_json::to_string(&options::price(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PositionDto {
    #[serde(default)]
    pub instrument: String,
    pub exposure: f64,
    #[serde(default = "default_vol")]
    pub volatility: f64,
    #[serde(default = "default_weight")]
    pub correlation: f64,
}

fn default_vol() -> f64 {
    0.15
}

#[derive(Debug, Clone, Deserialize)]
pub struct VarDto {
    pub company_id: String,
    #[serde(default)]
    pub portfolio_name: String,
    #[serde(default)]
    pub positions: Vec<PositionDto>,
    #[serde(default = "default_confidence")]
    pub confidence_level: f64,
    #[serde(default = "default_days")]
    pub holding_period_days: i32,
}

fn default_confidence() -> f64 {
    0.95
}

fn default_days() -> i32 {
    1
}

pub fn var_json(payload: &str) -> Result<String, String> {
    let dto: VarDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = var::VarRequest {
        company_id: dto.company_id,
        portfolio_name: dto.portfolio_name,
        positions: dto
            .positions
            .iter()
            .map(|p| var::Position {
                instrument: p.instrument.clone(),
                exposure: p.exposure,
                volatility: p.volatility,
                correlation: p.correlation,
            })
            .collect(),
        confidence_level: dto.confidence_level,
        holding_period_days: dto.holding_period_days,
    };
    serde_json::to_string(&var::calculate(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct BorrowerDto {
    pub borrower_id: String,
    #[serde(default)]
    pub borrower_name: String,
    #[serde(default = "default_term")]
    pub facility_type: String,
    pub total_exposure: f64,
    #[serde(default)]
    pub current_exposure: Option<f64>,
    #[serde(default)]
    pub undrawn_committed: f64,
    #[serde(default)]
    pub collateral_value: f64,
    #[serde(default)]
    pub guarantee_value: f64,
}

fn default_term() -> String {
    "term_loan".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct FinancialsDto {
    pub debt_to_equity: f64,
    pub interest_coverage: f64,
    pub current_ratio: f64,
    #[serde(default)]
    pub free_cash_flow: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualitativeDto {
    #[serde(default = "default_neutral")]
    pub industry_outlook: String,
    #[serde(default = "default_fair")]
    pub management_quality: String,
    #[serde(default = "default_average")]
    pub market_position: String,
}

fn default_neutral() -> String {
    "neutral".to_string()
}

fn default_fair() -> String {
    "fair".to_string()
}

fn default_average() -> String {
    "average".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditDto2 {
    pub borrower: BorrowerDto,
    pub financials: FinancialsDto,
    #[serde(default)]
    pub qualitative: Option<QualitativeDto>,
    #[serde(default = "default_year")]
    pub credit_period_days: f64,
}

fn default_year() -> f64 {
    365.0
}

pub fn credit_risk_json(payload: &str) -> Result<String, String> {
    let dto: CreditDto2 = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let borrower = &dto.borrower;
    let qualitative = dto.qualitative.unwrap_or(QualitativeDto {
        industry_outlook: default_neutral(),
        management_quality: default_fair(),
        market_position: default_average(),
    });
    let request = credit_risk::CreditRequest {
        borrower: credit_risk::Borrower {
            borrower_id: borrower.borrower_id.clone(),
            borrower_name: borrower.borrower_name.clone(),
            facility_type: borrower.facility_type.clone(),
            total_exposure: borrower.total_exposure,
            current_exposure: borrower.current_exposure.unwrap_or(borrower.total_exposure),
            undrawn_committed: borrower.undrawn_committed,
            collateral_value: borrower.collateral_value,
            guarantee_value: borrower.guarantee_value,
        },
        financials: credit_risk::Financials {
            debt_to_equity: dto.financials.debt_to_equity,
            interest_coverage: dto.financials.interest_coverage,
            current_ratio: dto.financials.current_ratio,
            free_cash_flow: dto.financials.free_cash_flow,
        },
        qualitative: credit_risk::Qualitative {
            industry_outlook: qualitative.industry_outlook,
            management_quality: qualitative.management_quality,
            market_position: qualitative.market_position,
        },
        credit_period_days: dto.credit_period_days,
    };
    serde_json::to_string(&credit_risk::analyze(&request)).map_err(|e| e.to_string())
}

pub fn pd_historical_json(payload: &str) -> Result<String, String> {
    let dto: PdDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    serde_json::to_string(&credit_risk::historical_pd(
        dto.number_defaults,
        dto.total_observations,
        dto.confidence_level,
    ))
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PdDto {
    pub number_defaults: i64,
    pub total_observations: i64,
    #[serde(default = "default_confidence")]
    pub confidence_level: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FraudDto {
    #[serde(default)]
    pub audit_id: String,
    pub company_id: String,
    #[serde(default)]
    pub industry: String,
    #[serde(default)]
    pub company_size: String,
    #[serde(default)]
    pub financial_indicators: HashMap<String, f64>,
    #[serde(default)]
    pub management_characteristics: Vec<String>,
    #[serde(default)]
    pub internal_control_gaps: Vec<String>,
    #[serde(default)]
    pub prior_fraud_history: bool,
}

pub fn fraud_risk_json(payload: &str) -> Result<String, String> {
    let dto: FraudDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = fraud::FraudAssessmentRequest {
        audit_id: dto.audit_id,
        company_id: dto.company_id,
        industry: dto.industry,
        company_size: dto.company_size,
        financial_indicators: dto.financial_indicators.into_iter().collect(),
        management_characteristics: dto.management_characteristics,
        internal_control_gaps: dto.internal_control_gaps,
        prior_fraud_history: dto.prior_fraud_history,
    };
    serde_json::to_string(&fraud::assess(&request)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyDto {
    pub company_id: String,
    #[serde(default)]
    pub company_name: String,
    #[serde(default)]
    pub industry: String,
    #[serde(default)]
    pub working_capital: f64,
    pub total_assets: f64,
    #[serde(default)]
    pub ebit: f64,
    #[serde(default)]
    pub total_equity: f64,
    #[serde(default)]
    pub total_liabilities: f64,
    #[serde(default)]
    pub revenue: f64,
    #[serde(default)]
    pub retained_earnings: f64,
    #[serde(default)]
    pub market_cap: f64,
    #[serde(default)]
    pub current_assets: f64,
    #[serde(default)]
    pub current_liabilities: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DistressDto {
    pub company: CompanyDto,
    /// Accepted for compatibility with the service payload. Only the Altman
    /// variant was ever implemented, so the value is not used.
    #[serde(default)]
    #[allow(dead_code)]
    pub model: String,
}

pub fn distress_json(payload: &str) -> Result<String, String> {
    let dto: DistressDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let c = &dto.company;
    let company = distress::CompanyData {
        company_id: c.company_id.clone(),
        company_name: c.company_name.clone(),
        industry: c.industry.clone(),
        working_capital: c.working_capital,
        total_assets: c.total_assets,
        ebit: c.ebit,
        total_equity: c.total_equity,
        total_liabilities: c.total_liabilities,
        revenue: c.revenue,
        retained_earnings: c.retained_earnings,
        market_cap: c.market_cap,
        current_assets: c.current_assets,
        current_liabilities: c.current_liabilities,
    };
    serde_json::to_string(&distress::predict(&company)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegulatoryDto {
    pub company_id: String,
    #[serde(default)]
    pub regulator_type: String,
    pub tier1_capital: f64,
    #[serde(default)]
    pub tier2_capital: f64,
    pub risk_weighted_assets: f64,
    pub total_capital: f64,
    pub total_assets: f64,
    pub liquid_assets: f64,
    pub net_loan: f64,
}

pub fn regulatory_ratio_json(payload: &str) -> Result<String, String> {
    let dto: RegulatoryDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = regulatory::RegulatoryRequest {
        company_id: dto.company_id,
        regulator_type: dto.regulator_type,
        tier1_capital: dto.tier1_capital,
        tier2_capital: dto.tier2_capital,
        risk_weighted_assets: dto.risk_weighted_assets,
        total_capital: dto.total_capital,
        total_assets: dto.total_assets,
        liquid_assets: dto.liquid_assets,
        net_loan: dto.net_loan,
    };
    serde_json::to_string(&regulatory::calculate(&request)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn value(out: String) -> Value {
        serde_json::from_str(&out).expect("seam output is JSON")
    }

    #[test]
    fn credit_matches_the_service_figures() {
        let payload = r#"{
            "company_id": "co",
            "fiscal_year": 2026,
            "expenditures": [
                {"category": "wages", "description": "engineers", "amount": 120000, "qualifies": true},
                {"category": "supplies", "description": "lab", "amount": 30000.5, "qualifies": true},
                {"category": "overhead", "description": "rent", "amount": 20000, "qualifies": false}
            ]
        }"#;
        let out = value(credit_json(payload).unwrap());
        assert_eq!(out["credit_regular"], 22_500.08);
        assert_eq!(out["credit_alternative"], 30_000.1);
        assert_eq!(out["recommended_method"], "alternative_simplified");
    }

    #[test]
    fn vat_matches_the_service_figures() {
        let payload = r#"{
            "company_id": "co",
            "tax_period": "2026-Q1",
            "output_transactions": [
                {"description": "sales", "amount": 100000, "vat_rate": 0.15, "transaction_type": "standard"},
                {"description": "health", "amount": 10000, "vat_rate": 0.15, "transaction_type": "exempt"}
            ],
            "input_transactions": [
                {"description": "stock", "amount": 60000, "vat_rate": 0.15, "transaction_type": "standard"}
            ]
        }"#;
        let out = value(vat_return_json(payload).unwrap());
        assert_eq!(out["total_output_vat"], 16_500.0);
        assert_eq!(out["net_vat_payable"], 7_500.0);
        assert_eq!(out["vat_due_to_authority"], 7_500.0);
    }

    #[test]
    fn a_partnership_allocation_round_trips() {
        let payload = r#"{
            "company_id": "co",
            "period": "2026",
            "net_profit": 300000,
            "interest_rate_on_capital": 0.05,
            "partners": [
                {"partner_id": "p1", "name": "A", "capital_contribution": 200000,
                 "profit_share_pct": 60, "salary": 40000, "interest_on_capital_rate": 0.05,
                 "drawings": 30000}
            ]
        }"#;
        let out = value(partnership_allocation_json(payload).unwrap());
        assert_eq!(out["residual_profit"], 250_000.0);
        assert_eq!(out["partner_accounts"][0]["closing_capital"], 370_000.0);
    }

    #[test]
    fn consolidation_no_longer_invents_a_cash_flow() {
        let payload = r#"{
            "parent_company_id": "P",
            "period": "2026",
            "parent_revenue": 10000000,
            "parent_profit": 1500000,
            "parent_assets": 50000000,
            "parent_liabilities": 30000000,
            "subsidiaries": [
                {"subsidiary_id": "s1", "name": "S1", "ownership_percentage": 100,
                 "revenue": 2000000, "profit": 300000, "assets": 5000000, "liabilities": 2500000,
                 "goodwill": 100000}
            ],
            "intercompany_transactions": [
                {"description": "group sales", "transaction_type": "sale", "amount": 200000}
            ]
        }"#;
        let out = value(consolidation_json(payload).unwrap());
        assert_eq!(
            out["consolidated_revenue"]["consolidated_revenue"],
            11_800_000.0
        );
        assert_eq!(
            out["consolidated_profit"]["attributable_to_parent"],
            1_800_000.0
        );
        assert!(out.get("consolidated_cash_flow").is_none());
    }
}

#[cfg(test)]
mod risk_tests {
    use super::*;
    use serde_json::Value;

    fn value(out: String) -> Value {
        serde_json::from_str(&out).expect("seam output is JSON")
    }

    #[test]
    fn scorecard_and_controls_keep_their_scale_quirks() {
        let payload = r#"{"company_id":"co","period":"2026-Q1","kpis":[
            {"name":"Revenue growth","perspective":"financial","target":1000000,"actual":800000,"weight":2},
            {"name":"NPS","perspective":"customer","target":50,"actual":30}]}"#;
        let out = value(scorecard_json(payload).unwrap());
        // one financial KPI at 80% and one customer KPI at 60%, each weighted 1-2:
        // perspective scores are percentages multiplied by 100 again
        assert_eq!(out["financial_score"], 8_000.0);
        assert_eq!(out["customer_score"], 6_000.0);
        assert_eq!(out["overall_score"], 3_500.0);

        let controls = r#"{"company_id":"co","fiscal_year":2026,"controls":[
            {"id":"c1","name":"Bank rec","test_sample_size":40,"exceptions_found":0},
            {"id":"c3","name":"Segregation","test_sample_size":40,"exceptions_found":4}]}"#;
        let out = value(controls_json(controls).unwrap());
        assert_eq!(out["deficient_controls"], 1);
        assert_eq!(
            out["control_details"][1]["deficiency_level"],
            "significant_deficiency"
        );
    }

    #[test]
    fn fx_and_inventory_match_the_service() {
        let payload = r#"{"from_currency":"USD","to_currency":"ZAR","amount":1000}"#;
        let out = value(fx_convert_json(payload, "2026-11-03").unwrap());
        assert_eq!(out["converted_amount"], 18_500.0);
        assert_eq!(out["rate_date"], "2026-11-03");

        let inventory = r#"{"company_id":"co","method":"lifo","opening_inventory":5000,"opening_qty":50,
            "purchases":[{"date":"2026-02-01","quantity":100,"unit_cost":110},
                         {"date":"2026-01-15","quantity":100,"unit_cost":100}],
            "sales":[{"date":"2026-03-01","quantity":120,"unit_price":200}]}"#;
        let out = value(inventory_json(inventory).unwrap());
        assert_eq!(out["cost_of_goods_sold"], 13_000.0);
        assert_eq!(out["ending_inventory"], 13_000.0);
    }

    #[test]
    fn options_and_var_match_the_service() {
        let payload = r#"{"company_id":"co","options":[
            {"option_type":"call","strike_price":100,"spot_price":100,"time_to_expiry":1,
             "volatility":0.2,"risk_free_rate":0.05,"notional":1000},
            {"option_type":"put","strike_price":90,"spot_price":100,"time_to_expiry":0.5,
             "volatility":0.25,"risk_free_rate":0.05,"notional":500}]}"#;
        let out = value(options_price_json(payload).unwrap());
        assert_eq!(out["option_valuations"][0]["premium"], 10.4506);
        assert_eq!(out["greeks_summary"]["delta"], 534.6039);

        let var_payload = r#"{"company_id":"co","portfolio_name":"Book","holding_period_days":10,
            "positions":[{"instrument":"equity","exposure":1000000,"volatility":0.2},
                         {"instrument":"bond","exposure":500000,"volatility":0.08}]}"#;
        let out = value(var_json(var_payload).unwrap());
        assert_eq!(out["var_95"], 1_061_197.14);
        assert_eq!(out["risk_level"], "high");
    }

    #[test]
    fn credit_risk_answers_for_a_borrower_that_used_to_be_a_500() {
        // The Python endpoint raised a validation error on this payload because
        // PDCalculation declared rating_grade as a string and was handed an int.
        let payload = r#"{
            "borrower":{"borrower_id":"b1","borrower_name":"Acme","facility_type":"term_loan",
                "total_exposure":1000000,"current_exposure":1000000,"collateral_value":400000,
                "guarantee_value":100000},
            "financials":{"debt_to_equity":0.8,"interest_coverage":6.0,"current_ratio":2.5,
                "free_cash_flow":300000},
            "qualitative":{"industry_outlook":"positive","management_quality":"good",
                "market_position":"leader"}
        }"#;
        let out = value(credit_risk_json(payload).unwrap());
        // perfect financials plus positive/good/leader qualitative: composite 95.5
        assert_eq!(out["credit_rating"], "AAA");
        assert_eq!(out["rating_grade"], 1);
        assert_eq!(out["pd_calculations"]["rating_grade"], 1);
        assert_eq!(out["lgd_calculations"]["recovery_rate"], 0.5);
    }

    #[test]
    fn distress_and_regulatory_match_the_service() {
        let payload = r#"{"company":{"company_id":"co","company_name":"C","industry":"manufacturing",
            "working_capital":200000,"total_assets":1000000,"ebit":120000,"total_equity":400000,
            "total_liabilities":600000,"revenue":1500000,"retained_earnings":150000}}"#;
        let out = value(distress_json(payload).unwrap());
        assert_eq!(out["z_score"], 2.75);
        assert_eq!(out["zone"], "GREY");

        let regulatory = r#"{"company_id":"co","regulator_type":"bank","tier1_capital":900000,
            "tier2_capital":200000,"risk_weighted_assets":10000000,"total_capital":1100000,
            "total_assets":20000000,"liquid_assets":3000000,"net_loan":2500000}"#;
        let out = value(regulatory_ratio_json(regulatory).unwrap());
        assert_eq!(out["overall_compliance"], "compliant");
        assert_eq!(out["capital_ratios"][0]["actual_value"], 0.09);
    }
}

/// internal-control-evaluation-service `/evaluate`: the whole payload in, the whole
/// answer out. A control is ineffective the moment any deficiency is recorded, and
/// the engagement is effective only with no material weakness and no significant
/// deficiency.
pub fn control_evaluation_json(payload: &str) -> Result<String, String> {
    let request: serde_json::Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let answer = evaluate_controls(&request);
    serde_json::to_string(&answer).map_err(|e| e.to_string())
}

/// payment-optimization-service `/optimize`.
pub fn payment_optimization_json(payload: &str) -> Result<String, String> {
    let request: serde_json::Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let answer = vimbai_treasury_fx::payment_optimization::evaluate(&request)?;
    // the library keeps amounts as text; this service already publishes floats
    let legacy = crate::floats_from_strings(&answer);
    serde_json::to_string(&legacy).map_err(|e| e.to_string())
}

/// substantive-testing-service `/test`.
pub fn substantive_testing_json(payload: &str) -> Result<String, String> {
    let request: serde_json::Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let answer = vimbai_advisory::substantive_testing::evaluate(&request)?;
    let legacy = crate::floats_from_strings(&answer);
    serde_json::to_string(&legacy).map_err(|e| e.to_string())
}

/// divestiture-service `/dispose`.
pub fn divestiture_json(payload: &str) -> Result<String, String> {
    let request: serde_json::Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let answer = vimbai_group_accounting::divestiture::evaluate(&request)?;
    let legacy = crate::floats_from_strings(&answer);
    serde_json::to_string(&legacy).map_err(|e| e.to_string())
}

/// capital-expenditure-budget-service `/prepare`.
pub fn capex_budget_json(payload: &str) -> Result<String, String> {
    let request: serde_json::Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let answer = vimbai_budgeting::capex::prepare_from_json(&request)?;
    // budget_year is numeric-looking text and must stay text
    let legacy = crate::floats_from_strings_except(&answer, &["budget_year"]);
    serde_json::to_string(&legacy).map_err(|e| e.to_string())
}
