#![forbid(unsafe_code)]
//! Treasury, FX and derivatives math: currency conversion and
//! triangulation, IAS 21 translation, FX exposure hedging, and the
//! multi-currency service's fixed tables.

pub mod fx_hedging;
pub mod fx_rates;
pub mod fx_translation;
pub mod multi_currency;

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
pub use multi_currency::{
    convert_fixed, create_hedge, fixed_pair_rate, process_transaction, CurrencyConversionRequest,
    CurrencyConversionResponse, HedgeRequest, HedgeResponse, TransactionRequest,
    TransactionResponse,
};

#[cfg(test)]
mod tests;
