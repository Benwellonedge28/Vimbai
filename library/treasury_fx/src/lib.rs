#![forbid(unsafe_code)]
//! Treasury, FX and derivatives math: currency conversion and
//! triangulation, IAS 21 translation, FX exposure hedging, and the
//! multi-currency service's fixed tables.

pub mod analytics;
pub mod cash;
pub mod derivatives;
pub mod fx_hedging;
pub mod fx_rates;
pub mod fx_translation;
pub mod hedging_analytics;
pub mod liquidity;
pub mod multi_currency;
pub mod operations;
pub mod options;

pub use cash::{
    add_days, calculate_liquidity_ratios, optimize_cash_flow, reconcile_bank,
    BankReconciliationRequest, BankReconciliationResult, CashOptimizationRequest,
    CashOptimizationResult, InvestmentOption, LiquidityRatioRequest, LiquidityRatioResult,
    MaturityEntry, OutstandingItem, RecommendedInvestment, ReconciliationDifference,
};
pub use derivatives::{
    value_cross_currency_swaps, value_swaps, CounterpartyExposure, CrossCurrencySwap,
    CrossCurrencySwapRequest, CrossCurrencySwapResult, CrossCurrencySwapValuation,
    CurrencyExposure, PortfolioSummary, Swap, SwapValuation, SwapValuationRequest,
    SwapValuationResult,
};
pub use fx_hedging::{
    analyze_currency_hedge, calculate_fx_hedge, spot_to_usd, CurrencyHedgingRequest,
    CurrencyHedgingResult, ExposureAnalysis, FxExposure, FxExposureRequest, FxHedgeError,
    FxHedgeResult, HedgeRecommendation, PortfolioMetrics,
};
pub use fx_rates::{
    round_half_even, round_half_up, triangulate, validate_multi_currency_transaction, Conversion,
    ConvertedLine, Currency, CurrencyError, ExchangeRate, RateBook, TransactionLine, Triangulation,
    ValidatedTransaction,
};
pub use fx_translation::{
    translate_financials, BalanceSheetItem, TranslationDetail, TranslationRequest,
    TranslationResult,
};
pub use hedging_analytics::{
    analyze_commodity_hedge, analyze_futures_hedge, analyze_interest_rate_hedge, BudgetImpact,
    CommodityExposure, CommodityExposureAnalysis, CommodityHedgePlan, CommodityHedgingRequest,
    CommodityHedgingResult, DebtExposure, ExposureSummary, FuturePosition, FuturesHedgingRequest,
    FuturesHedgingResult, HedgeAction, HedgeEffectiveness, InterestRateHedgingRequest,
    InterestRateHedgingResult, PositionAnalysis, SensitivityRow,
};
pub use liquidity::{
    analyze_liquidity, analyze_liquidity_risk, forecast_liquidity, CashFlow, DailyForecast,
    ForecastSummary, Liability, LiquidityAnalysisRequest, LiquidityAnalysisResult, LiquidityAsset,
    LiquidityForecastRequest, LiquidityForecastResult, LiquidityGap, LiquidityPosition,
    LiquidityRatios, LiquidityRequest, LiquidityResult, MaturityMismatch, StressScenario,
    StressTest,
};
pub use multi_currency::{
    convert_fixed, create_hedge, fixed_pair_rate, process_transaction, CurrencyConversionRequest,
    CurrencyConversionResponse, HedgeRequest, HedgeResponse, TransactionRequest,
    TransactionResponse,
};
pub use operations::{
    analyze_treasury_operations, BankBalanceSummary, PolicyCompliance, TransactionsSummary,
    TreasuryOperationsRequest, TreasuryOperationsResult, TreasuryTransaction,
};
pub use options::{
    black_scholes_call, black_scholes_put, erf, norm_cdf, norm_pdf, price_options, GreeksSummary,
    Option, OptionValuation, OptionsPortfolioSummary, OptionsPricingRequest, OptionsPricingResult,
};

#[cfg(test)]
mod tests;

/// Re-exported so callers of the cash module do not need two crates.
pub use vimbai_common::Money;
