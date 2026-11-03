//! Business combination: what the acquirer paid against what it got.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::money::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentifiableAsset {
    pub name: String,
    pub fair_value: f64,
    /// `tangible`, `intangible`, `liability`.
    pub r#type: String,
}

impl IdentifiableAsset {
    pub fn new(name: &str, fair_value: f64, kind: &str) -> IdentifiableAsset {
        IdentifiableAsset {
            name: name.to_string(),
            fair_value,
            r#type: kind.to_string(),
        }
    }

    pub fn tangible(name: &str, fair_value: f64) -> IdentifiableAsset {
        IdentifiableAsset::new(name, fair_value, "tangible")
    }

    pub fn intangible(name: &str, fair_value: f64) -> IdentifiableAsset {
        IdentifiableAsset::new(name, fair_value, "intangible")
    }

    pub fn liability(name: &str, fair_value: f64) -> IdentifiableAsset {
        IdentifiableAsset::new(name, fair_value, "liability")
    }

    fn is_liability(&self) -> bool {
        self.r#type == "liability"
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcquisitionRequest {
    pub company_id: String,
    pub acquirer: String,
    pub acquiree: String,
    pub purchase_price: f64,
    pub goodwill_recognized: bool,
    pub identifiable_assets: Vec<IdentifiableAsset>,
    pub contingent_consideration: f64,
    pub acquisition_costs: f64,
    pub non_controlling_interest: f64,
}

impl AcquisitionRequest {
    pub fn new(
        company_id: &str,
        acquirer: &str,
        acquiree: &str,
        purchase_price: f64,
    ) -> AcquisitionRequest {
        AcquisitionRequest {
            company_id: company_id.to_string(),
            acquirer: acquirer.to_string(),
            acquiree: acquiree.to_string(),
            purchase_price,
            goodwill_recognized: true,
            identifiable_assets: Vec::new(),
            contingent_consideration: 0.0,
            acquisition_costs: 0.0,
            non_controlling_interest: 0.0,
        }
    }

    pub fn with_assets(mut self, assets: Vec<IdentifiableAsset>) -> AcquisitionRequest {
        self.identifiable_assets = assets;
        self
    }

    pub fn with_consideration(mut self, contingent: f64, nci: f64) -> AcquisitionRequest {
        self.contingent_consideration = contingent;
        self.non_controlling_interest = nci;
        self
    }

    /// Acquisition costs are expensed under IFRS 3, so they are recorded and
    /// deliberately kept out of the consideration.
    pub fn with_costs(mut self, costs: f64) -> AcquisitionRequest {
        self.acquisition_costs = costs;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AcquisitionResult {
    pub company_id: String,
    pub acquirer: String,
    pub acquiree: String,
    pub purchase_price: f64,
    pub total_identifiable_assets: f64,
    pub total_identifiable_liabilities: f64,
    pub net_identifiable_assets: f64,
    pub goodwill: f64,
    pub bargain_purchase: f64,
    pub contingent_consideration: f64,
    pub acquisition_costs: f64,
    pub non_controlling_interest: f64,
    pub goodwill_recognized: bool,
    /// The service's wire name for the same list.
    pub purchase_price_allocation: Vec<Value>,
}

/// Allocate the consideration.
///
/// Preserved quirk: `goodwill` is reported whenever the consideration exceeds
/// net assets, even when the caller said not to recognise it; the flag only
/// controls whether the goodwill line appears in the allocation.
pub fn acquire(request: &AcquisitionRequest) -> AcquisitionResult {
    let total_assets: f64 = request
        .identifiable_assets
        .iter()
        .filter(|a| !a.is_liability())
        .map(|a| a.fair_value)
        .sum();
    let total_liabilities: f64 = request
        .identifiable_assets
        .iter()
        .filter(|a| a.is_liability())
        .map(|a| a.fair_value.abs())
        .sum();
    let net_identifiable = total_assets - total_liabilities;

    let consideration = request.purchase_price
        + request.contingent_consideration
        + request.non_controlling_interest;
    let goodwill = consideration - net_identifiable;
    let bargain = (net_identifiable - consideration).max(0.0);

    let mut allocation: Vec<Value> = request
        .identifiable_assets
        .iter()
        .map(|asset| {
            json!({
                "asset": asset.name,
                "type": asset.r#type,
                "fair_value": money(asset.fair_value),
                // percentages are against total assets, as the service computed them
                "allocation_pct": super::money::round_dp(
                    asset.fair_value.abs() / total_assets.max(1.0) * 100.0,
                    1
                ),
            })
        })
        .collect();

    if goodwill > 0.0 && request.goodwill_recognized {
        allocation.push(json!({
            "asset": "Goodwill",
            "type": "intangible",
            "fair_value": money(goodwill),
            "allocation_pct": 0.0,
        }));
    }
    if bargain > 0.0 {
        allocation.push(json!({
            "asset": "Bargain Purchase Gain",
            "type": "income",
            "fair_value": money(bargain),
            "allocation_pct": 0.0,
        }));
    }

    AcquisitionResult {
        company_id: request.company_id.clone(),
        acquirer: request.acquirer.clone(),
        acquiree: request.acquiree.clone(),
        purchase_price: money(request.purchase_price),
        total_identifiable_assets: money(total_assets),
        total_identifiable_liabilities: money(total_liabilities),
        net_identifiable_assets: money(net_identifiable),
        goodwill: money(goodwill),
        bargain_purchase: money(bargain),
        contingent_consideration: money(request.contingent_consideration),
        acquisition_costs: money(request.acquisition_costs),
        non_controlling_interest: money(request.non_controlling_interest),
        goodwill_recognized: request.goodwill_recognized,
        purchase_price_allocation: allocation,
    }
}
