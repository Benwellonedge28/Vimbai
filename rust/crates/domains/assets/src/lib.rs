#![forbid(unsafe_code)]
//! Asset math: depreciation, amortization, inventory valuation, net
//! realizable value, revaluation and goodwill impairment.
//!
//! Ported from: depreciation-service, amortization-service,
//! inventory-valuation-service, net-realizable-value-service,
//! revaluation-model-service and goodwill-service.
//!
//! Investment property (IAS 40), biological assets (IAS 41) and fixed asset
//! disposals live here too, so every asset question the platform answers is in
//! one crate.
//!
//! Remaining services queued for this crate: fixed-assets-register,
//! fixed-assets-schedule, intangible-assets, revaluation-reserve.

pub mod amortization;
pub mod asset_schedule;
pub mod biological_assets;
pub mod depreciation;
pub mod disposal_account;
pub mod fixed_assets;
pub mod fixed_assets_register;
pub mod goodwill;
pub mod intangible_assets;
pub mod inventory;
pub mod investment_property;
pub mod revaluation;
pub mod revaluation_reserve;

pub use amortization::{
    amortization, amortization_from_json, AmortizationAsset, AmortizationOutput, AmortizationRow,
};
pub use asset_schedule::{
    generate_schedule, list_with_count as list_register_with_count, summary as schedule_summary,
    update_asset as update_registered_asset,
};
pub use biological_assets::{
    biological_classify_from_json, classify_biological_asset,
    disclosure_from_json as biological_disclosure_from_json, level1_from_json, level2_from_json,
    level3_from_json, measure_biological_assets, measure_from_json,
    prepare_disclosure as prepare_biological_disclosure, BiologicalAsset, BiologicalAssetsRequest,
    BiologicalAssetsResponse, ClassificationRequest, ClassificationResult, DisclosureRequest,
    DisclosureResult, GainLossBreakdown, HarvestEntry, Level1Request, Level2Request, Level3Request,
    TransformationEntry,
};
pub use depreciation::{
    depreciation_batch, depreciation_batch_from_json, depreciation_method,
    depreciation_method_from_json, AssetDepreciationRow, DepreciationAsset,
    DepreciationMethodOutput, DepreciationOutput, DepreciationScheduleRow, MethodScheduleRow,
};
pub use disposal_account::{
    calculate_asset_disposal, calculate_from_json as disposal_calculate_from_json, post_disposal,
    post_from_json as disposal_post_from_json, DisposalEntry, DisposalJournal, DisposalRequest,
    JournalLine,
};
pub use fixed_assets::{
    analyze_fixed_assets, analyze_fixed_assets_from_json, AssetSummary, CapitalExpenditurePlan,
    CategoryAnalysis, FixedAsset, FixedAssetsAnalysis, FixedAssetsError, FixedAssetsRequest,
    ScheduleRow,
};
pub use fixed_assets_register::{
    create_asset, depreciate, dispose as dispose_registered_asset, get_asset, list_assets,
    list_entries, summary as asset_register_summary, CATEGORIES,
};
pub use goodwill::{impairment_test, impairment_test_from_json, Cgu, ImpairmentResult, TestMethod};
pub use intangible_assets::{
    amortize, create_asset as create_intangible, get_asset as get_intangible,
    list_assets as list_intangibles, list_entries as list_amortization_entries,
    TYPES as INTANGIBLE_TYPES,
};
pub use inventory::{inventory_valuation, InventoryOutput, Purchase, Sale, ValuationMethod};
pub use investment_property::{
    apply_cost_model, apply_fair_value_model, calculate_property_yields, cost_model_from_json,
    disclosure_from_json, fair_value_model_from_json, prepare_fair_value_disclosure,
    process_transfer, transfer_from_json, yields_from_json, CostModelDepreciation,
    CostModelRequest, FairValueMeasurement, FairValueModelRequest, InvestmentProperty,
    InvestmentPropertyResponse,
};
pub use revaluation::{revalue, AssetBook, RevaluationAsset, RevaluationOutput, RevaluationRow};
pub use revaluation_reserve::{
    cumulative as cumulative_revaluation, list_revaluations, record_revaluation,
    record_utilization as record_revaluation_utilization, summary as revaluation_summary,
};
