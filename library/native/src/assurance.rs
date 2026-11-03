//! The assurance, group, performance and risk libraries behind one JSON door.
//!
//! Each function takes the payload the Python service receives and returns the
//! payload its response model would serialise. No arithmetic lives here: the
//! request is turned into library types, the library does the work, and the
//! result is serialised back. That keeps the numbers identical to the Rust tests.

use serde::Deserialize;

use vimbai_group_accounting::buyout::{self, BuyoutRequest, RemainingPartner};
use vimbai_group_accounting::combination::{self, AcquisitionRequest, IdentifiableAsset};
use vimbai_group_accounting::consolidation::{
    self, ConsolidationRequest, IntercompanyTransaction, ParentFinancials, SubsidiaryFinancials,
};
use vimbai_group_accounting::impairment::{self, Cgu, ImpairmentRequest, TestMethod};
use vimbai_group_accounting::partnership::{self, Partner, PartnershipRequest};
use vimbai_group_accounting::pension::{self, PensionRequest};
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
}

pub fn credit_json(payload: &str) -> Result<String, String> {
    let dto: CreditDto = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    let request = ClaimRequest::new(
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
