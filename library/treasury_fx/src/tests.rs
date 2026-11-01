//! Parity tests. Expected figures were produced by running the original
//! Python service logic against the same inputs.
use crate::fx_hedging::*;
use crate::fx_rates::*;
use crate::fx_translation::*;
use crate::multi_currency::*;

fn currencies() -> Vec<Currency> {
    vec![
        Currency {
            code: "USD".into(),
            name: "US Dollar".into(),
            symbol: "$".into(),
            decimal_places: 2,
            is_active: true,
        },
        Currency {
            code: "EUR".into(),
            name: "Euro".into(),
            symbol: "€".into(),
            decimal_places: 2,
            is_active: true,
        },
        Currency {
            code: "ZWL".into(),
            name: "Zimbabwe Dollar".into(),
            symbol: "Z$".into(),
            decimal_places: 2,
            is_active: true,
        },
        Currency {
            code: "JPY".into(),
            name: "Yen".into(),
            symbol: "¥".into(),
            decimal_places: 0,
            is_active: true,
        },
    ]
}

fn book() -> RateBook {
    RateBook::new(
        vec![
            ExchangeRate {
                from_currency: "EUR".into(),
                to_currency: "USD".into(),
                rate: 1.08,
                effective_date: "2026-01-01".into(),
            },
            ExchangeRate {
                from_currency: "USD".into(),
                to_currency: "ZWL".into(),
                rate: 15.5,
                effective_date: "2026-01-01".into(),
            },
            ExchangeRate {
                from_currency: "GBP".into(),
                to_currency: "USD".into(),
                rate: 1.27,
                effective_date: "2026-01-01".into(),
            },
        ],
        currencies(),
    )
}

#[test]
fn direct_inverse_and_cross_rates() {
    let b = book();
    assert_eq!(b.get_rate("EUR", "USD"), Some(1.08));
    // inverse of the stored pair
    assert!((b.get_rate("USD", "EUR").unwrap() - 1.0 / 1.08).abs() < 1e-12);
    // cross through USD: 15.5 / (1/1.08)
    assert!((b.get_rate("EUR", "ZWL").unwrap() - 16.74).abs() < 1e-9);
    assert!((b.get_rate("GBP", "ZWL").unwrap() - 19.685).abs() < 1e-9);
    assert_eq!(b.get_rate("JPY", "EUR"), None);
}

#[test]
fn conversion_rounds_to_target_places() {
    let b = book();
    let eur_usd = b.convert("EUR", "USD", 1_000.0, "2026-02-01").unwrap();
    assert_eq!(eur_usd.converted_amount, 1_080.0);
    assert_eq!(eur_usd.rate_used, 1.08);
    assert_eq!(eur_usd.rate_date, "2026-02-01");
    assert_eq!(eur_usd.rounding_mode, "HALF_UP");

    let usd_eur = b.convert("USD", "EUR", 1_000.0, "").unwrap();
    assert_eq!(usd_eur.converted_amount, 925.93);

    let eur_zwl = b.convert("EUR", "ZWL", 1_000.0, "").unwrap();
    assert_eq!(eur_zwl.converted_amount, 16_740.0);

    let gbp_zwl = b.convert("GBP", "ZWL", 2_500.555, "").unwrap();
    assert_eq!(gbp_zwl.converted_amount, 49_223.43);

    let missing = b.convert("JPY", "EUR", 100.0, "").unwrap_err();
    assert_eq!(
        missing,
        CurrencyError::NoRate {
            from: "JPY".into(),
            to: "EUR".into()
        }
    );
    assert!(missing.to_string().contains("No exchange rate found"));
}

#[test]
fn zero_decimal_currency_rounds_to_whole_units() {
    let b = RateBook::new(
        vec![ExchangeRate {
            from_currency: "USD".into(),
            to_currency: "JPY".into(),
            rate: 149.55,
            effective_date: "2026-01-01".into(),
        }],
        currencies(),
    );
    let c = b.convert("USD", "JPY", 10.0, "").unwrap();
    assert_eq!(c.converted_amount, 1_496.0);
}

#[test]
fn rounding_helpers_differ_on_ties() {
    // Python's Decimal ROUND_HALF_UP moves 2.675 up, built-in round() does not
    assert_eq!(round_half_up(2.675, 2), 2.68);
    assert_eq!(round_half_even(2.675, 2), 2.67);
    assert_eq!(round_half_up(1.005, 2), 1.01);
    assert_eq!(round_half_up(-2.675, 2), -2.68);
    assert_eq!(round_half_even(0.125, 2), 0.12);
}

#[test]
fn latest_rate_wins_whatever_the_date() {
    let mut rates = book().rates;
    rates.push(ExchangeRate {
        from_currency: "EUR".into(),
        to_currency: "USD".into(),
        rate: 1.15,
        effective_date: "2027-06-01".into(),
    });
    let b = RateBook::new(rates, currencies());
    // the historical date is ignored, so the newest rate applies
    assert_eq!(b.get_rate("EUR", "USD"), Some(1.15));
    assert_eq!(
        b.convert("EUR", "USD", 100.0, "2020-01-01")
            .unwrap()
            .converted_amount,
        115.0
    );
}

#[test]
fn triangulation_reports_direct_and_cross_legs() {
    let mut rates = book().rates;
    rates.push(ExchangeRate {
        from_currency: "EUR".into(),
        to_currency: "ZWL".into(),
        rate: 17.0,
        effective_date: "2026-01-01".into(),
    });
    let b = RateBook::new(rates, currencies());
    let t = triangulate(&b, "EUR", "ZWL", 1_000.0).unwrap();
    assert_eq!(
        t.direct_conversion.as_ref().unwrap().converted_amount,
        17_000.0
    );
    // the cross leg starts from the ROUNDED intermediate USD amount
    assert_eq!(
        t.cross_conversion.as_ref().unwrap().converted_amount,
        16_740.0
    );
    assert_eq!(t.difference, Some(260.0));
    let savings = t.potential_savings_percent.unwrap();
    assert!((savings - 1.5294117647058822).abs() < 1e-9);
    assert_eq!(t.recommendation, "Use cross-rate");

    // a USD leg needs no triangulation
    let usd = triangulate(&b, "EUR", "USD", 1_000.0).unwrap();
    assert!(usd.cross_conversion.is_none());
    assert_eq!(
        usd.recommendation,
        "No triangulation needed (same base currency)"
    );
}

#[test]
fn formatting_groups_and_uses_the_symbol() {
    let b = book();
    assert_eq!(b.format_amount("USD", 1_234_567.891), "$1,234,567.89");
    assert_eq!(b.format_amount("ZWL", -4_500.0), "Z$-4,500.00");
    assert_eq!(b.format_amount("JPY", 1_234.6), "¥1,235");
    assert_eq!(b.format_amount("EUR", 12.0), "€12.00");
}

fn line(currency: &str, amount: f64) -> TransactionLine {
    TransactionLine {
        currency: currency.into(),
        amount,
        account: Some("4000".into()),
        description: None,
    }
}

#[test]
fn transaction_lines_convert_into_the_base_currency() {
    let b = book();
    let t = validate_multi_currency_transaction(
        &b,
        "usd",
        &[
            line("EUR", 1_000.0),
            line("ZWL", 15_500.0),
            line("USD", 250.0),
        ],
    )
    .unwrap();
    assert!(t.valid);
    assert_eq!(t.base_currency, "USD");
    assert_eq!(t.line_count, 3);
    // 1,080 + 1,000 + 250
    assert_eq!(t.total_in_base, 2_330.0);
    assert_eq!(t.lines[1].original_currency, "ZWL");
    assert_eq!(t.lines[1].converted_to, "USD");
    assert_eq!(t.lines[2].converted_amount, 250.0);
}

#[test]
fn unknown_base_currency_is_rejected() {
    let b = book();
    let err = validate_multi_currency_transaction(&b, "CHF", &[line("USD", 10.0)]).unwrap_err();
    assert_eq!(err, CurrencyError::UnsupportedBase { code: "CHF".into() });
    assert_eq!(err.to_string(), "Base currency CHF not supported");
}

fn translation_request() -> TranslationRequest {
    TranslationRequest {
        company_id: "c1".into(),
        subsidiary: "Harare Ops".into(),
        functional_currency: "USD".into(),
        presentation_currency: "ZWL".into(),
        closing_rate: 16.2,
        avg_rate: 15.8,
        historical_rate: 14.0,
        net_assets: vec![
            BalanceSheetItem {
                account: "Cash".into(),
                amount: 100_000.0,
                is_monetary: true,
            },
            BalanceSheetItem {
                account: "Inventory".into(),
                amount: 50_000.0,
                is_monetary: false,
            },
            BalanceSheetItem {
                account: "Receivable".into(),
                amount: 25_000.0,
                is_monetary: true,
            },
        ],
        income_statement: vec![
            BalanceSheetItem {
                account: "Revenue".into(),
                amount: 400_000.0,
                is_monetary: true,
            },
            BalanceSheetItem {
                account: "Costs".into(),
                amount: 250_000.0,
                is_monetary: true,
            },
        ],
        goodwill: 80_000.0,
    }
}

#[test]
fn translation_uses_rate_per_item_type() {
    let r = translate_financials(&translation_request());
    // monetary at closing, non-monetary at historical
    assert_eq!(r.translated_net_assets, 2_725_000.0);
    assert_eq!(r.translated_income, 10_270_000.0);
    assert_eq!(r.goodwill_translated, 1_296_000.0);

    let cash = &r.details[0];
    assert_eq!(cash.rate_used, 16.2);
    assert_eq!(cash.rate_type, "closing");
    assert_eq!(cash.translated, 1_620_000.0);

    let inventory = &r.details[1];
    assert_eq!(inventory.rate_used, 14.0);
    assert_eq!(inventory.rate_type, "historical");
    assert_eq!(inventory.translated, 700_000.0);

    let revenue = &r.details[3];
    assert_eq!(revenue.rate_type, "average");
    assert_eq!(revenue.rate_used, 15.8);
}

#[test]
fn translation_adjustment_only_sees_monetary_items() {
    let r = translate_financials(&translation_request());
    // 125,000 monetary * (16.2 - 15.8) = 50,000; inventory never enters
    assert_eq!(r.cumulative_translation_adjustment, 50_000.0);
    // and the two reported figures are always the same number
    assert_eq!(r.exchange_differences, r.cumulative_translation_adjustment);
    // which is why it can never reconcile to the net asset movement
    assert_ne!(r.cumulative_translation_adjustment, r.translated_net_assets);
}

#[test]
fn fx_hedge_percentage_quirk() {
    let r = calculate_fx_hedge(&FxExposureRequest {
        company_id: "c1".into(),
        currency: "ZWL".into(),
        exposure_amount: 1_000_000.0,
        hedge_ratio: 0.7,
        forward_rate: 16.0,
    })
    .unwrap();
    assert_eq!(r.hedged_amount, 700_000.0);
    assert_eq!(r.unhedged_amount, 300_000.0);
    // reported as a PERCENTAGE, not an amount: 300,000 * 0.5 / 15.5 * 100
    assert_eq!(r.potential_loss_unhedged, 967_741.94);
    assert_eq!(r.hedge_cost_estimate, 1_400.0);
    assert_eq!(r.forward_rate, 16.0);
}

#[test]
fn fx_hedge_defaults_forward_to_spot() {
    let r = calculate_fx_hedge(&FxExposureRequest {
        company_id: "c1".into(),
        currency: "ZWL".into(),
        exposure_amount: 100_000.0,
        hedge_ratio: 1.0,
        forward_rate: 0.0,
    })
    .unwrap();
    assert_eq!(r.forward_rate, 15.5);
    // an unhedged-forward hedge reports no loss at all
    assert_eq!(r.potential_loss_unhedged, 0.0);
    assert_eq!(r.unhedged_amount, 0.0);
}

#[test]
fn unsupported_currency_is_a_typed_error() {
    let err = calculate_fx_hedge(&FxExposureRequest {
        company_id: "c1".into(),
        currency: "CHF".into(),
        exposure_amount: 1.0,
        hedge_ratio: 1.0,
        forward_rate: 0.0,
    })
    .unwrap_err();
    assert_eq!(
        err,
        FxHedgeError::UnsupportedCurrency { code: "CHF".into() }
    );
}

fn hedging_request() -> CurrencyHedgingRequest {
    CurrencyHedgingRequest {
        company_id: "c1".into(),
        exposures: vec![
            FxExposure {
                currency_pair: "EURUSD".into(),
                exposure_amount: 500_000.0,
                spot_rate: 0.92,
                expected_rate: 0.94,
                volatility: 0.08,
            },
            FxExposure {
                currency_pair: "GBPUSD".into(),
                exposure_amount: 300_000.0,
                spot_rate: 0.79,
                expected_rate: 0.81,
                volatility: 0.11,
            },
        ],
        current_hedge_ratio: 0.6,
        target_hedge_ratio: 0.85,
        risk_aversion: 2.0,
    }
}

#[test]
fn exposure_analysis_figures() {
    let r = analyze_currency_hedge(&hedging_request());
    let eur = &r.exposure_analysis[0];
    assert_eq!(eur.exposure_usd, 543_478.26);
    assert_eq!(eur.volatility, 8.0);
    assert_eq!(eur.hedged_amount, 326_086.96);
    assert_eq!(eur.hedge_cost, 2_608.7);
    assert_eq!(eur.var_95, 71_739.13);
    assert_eq!(eur.hedged_var, 28_695.65);
    assert_eq!(eur.var_reduction, 60.0);

    assert_eq!(r.portfolio_metrics.total_exposure_usd, 923_225.1);
    assert_eq!(r.portfolio_metrics.hedged_value_usd, 553_935.06);
    assert_eq!(r.portfolio_metrics.current_hedge_ratio, 60.0);
    assert_eq!(r.portfolio_metrics.target_hedge_ratio, 85.0);
    // summed from the already rounded per-exposure figures
    assert_eq!(r.portfolio_metrics.var_portfolio_95, 140_663.18);
}

#[test]
fn hedge_recommendations_only_top_up() {
    let under = analyze_currency_hedge(&hedging_request());
    assert_eq!(under.hedge_recommendations.len(), 2);
    let eur = &under.hedge_recommendations[0];
    assert_eq!(eur.action, "BUY");
    // (0.85 - 0.6) * 543,478.26 = 135,869.57
    assert_eq!(eur.amount, 135_869.57);
    assert_eq!(eur.instrument, "Forward");
    assert!(under
        .recommendations
        .contains(&"Under-hedged - increase hedge ratio to target level".to_string()));

    // an over-hedged book gets no sell instruction at all
    let over = analyze_currency_hedge(&CurrencyHedgingRequest {
        current_hedge_ratio: 1.0,
        target_hedge_ratio: 0.85,
        ..hedging_request()
    });
    assert!(over.hedge_recommendations.is_empty());
    assert!(over.recommendations.is_empty());
}

#[test]
fn var_comfort_rule_uses_the_hedged_figure() {
    let r = analyze_currency_hedge(&CurrencyHedgingRequest {
        exposures: vec![FxExposure {
            currency_pair: "USDZWL".into(),
            exposure_amount: 1_000_000.0,
            spot_rate: 1.0,
            expected_rate: 1.0,
            volatility: 0.5,
        }],
        current_hedge_ratio: 0.5,
        target_hedge_ratio: 0.95,
        ..hedging_request()
    });
    // hedged VaR 412,500 exceeds 15% of 1,000,000
    assert_eq!(r.portfolio_metrics.hedged_var_95, 412_500.0);
    assert!(r
        .recommendations
        .contains(&"VaR exceeds comfort level - consider increasing hedges".to_string()));
}

#[test]
fn fixed_table_conversion_subtracts_the_spread() {
    let r = convert_fixed(
        &CurrencyConversionRequest {
            company_id: "c1".into(),
            amount: 1_000.0,
            from_currency: "EUR".into(),
            to_currency: "USD".into(),
            conversion_date: None,
        },
        "2026-11-01T00:00:00Z",
    );
    assert_eq!(r.exchange_rate, 1.10);
    assert_eq!(r.spread, 0.0022);
    // 1,000 * (1.10 - 0.0022) = 1,097.8, unrounded
    assert!((r.converted_amount - 1_097.8).abs() < 1e-9);
    assert_eq!(r.converted_at, "2026-11-01T00:00:00Z");

    // USD to EUR is in the table in its own right, at 0.91 rather than the
    // 0.909090... implied by inverting EUR to USD
    let reverse = convert_fixed(
        &CurrencyConversionRequest {
            amount: 1.0,
            from_currency: "USD".into(),
            to_currency: "EUR".into(),
            ..Default::default()
        },
        "",
    );
    assert_eq!(reverse.exchange_rate, 0.91);

    // a pair absent in both directions is inverted from the stored one
    let inverted = convert_fixed(
        &CurrencyConversionRequest {
            amount: 1.0,
            from_currency: "JPY".into(),
            to_currency: "USD".into(),
            ..Default::default()
        },
        "",
    );
    assert!((inverted.exchange_rate - 1.0 / 149.5).abs() < 1e-12);

    // an unknown pair converts at parity less the spread
    let unknown = convert_fixed(
        &CurrencyConversionRequest {
            amount: 100.0,
            from_currency: "ZWL".into(),
            to_currency: "KES".into(),
            ..Default::default()
        },
        "",
    );
    assert_eq!(unknown.exchange_rate, 1.0);
    assert!((unknown.converted_amount - 99.8).abs() < 1e-9);
}

#[test]
fn transaction_books_everything_in_usd() {
    let r = process_transaction(&TransactionRequest {
        company_id: "c1".into(),
        transaction_id: "TX1".into(),
        amount: 2_500.0,
        currency: "GBP".into(),
        transaction_type: "invoice".into(),
        counterparty_currency: Some("EUR".into()),
    });
    assert_eq!(r.amount_in_functional, 2_500.0);
    assert_eq!(r.functional_currency, "USD");
    assert_eq!(r.exchange_rate_used, 1.0);
    assert_eq!(r.fx_gain_loss, 25.0);
    assert_eq!(r.booking_status, "posted");

    let domestic = process_transaction(&TransactionRequest {
        amount: 2_500.0,
        counterparty_currency: Some("USD".into()),
        ..Default::default()
    });
    assert_eq!(domestic.fx_gain_loss, 0.0);

    let none = process_transaction(&TransactionRequest {
        amount: 2_500.0,
        ..Default::default()
    });
    assert_eq!(none.fx_gain_loss, 0.0);
}

#[test]
fn hedge_terms_are_placeholders() {
    let r = create_hedge(
        &HedgeRequest {
            company_id: "c1".into(),
            exposure_amount: 200_000.0,
            currency_pair: "EURUSD".into(),
            hedge_type: "forward".into(),
            tenor: 90,
            notional: 150_000.0,
        },
        "HEDGE-202611010000",
    );
    assert_eq!(r.instrument_type, "forward");
    assert_eq!(r.strike_rate, 1.10);
    assert_eq!(r.premium, 3_000.0);
    assert_eq!(r.effectiveness_ratio, 0.95);
    assert_eq!(r.mark_to_market, 3_000.0);
    assert_eq!(r.hedge_id, "HEDGE-202611010000");
}

// ------------------------------------------------------- derivatives

use crate::derivatives::*;
use crate::hedging_analytics::*;

fn xccs(tenor: f64, with_ois: bool) -> CrossCurrencySwapRequest {
    CrossCurrencySwapRequest {
        company_id: "c1".into(),
        swaps: vec![CrossCurrencySwap {
            swap_id: "CCS1".into(),
            notional_1: 1_000_000.0,
            currency_1: "USD".into(),
            rate_1: 0.045,
            notional_2: 920_000.0,
            currency_2: "EUR".into(),
            rate_2: 0.035,
            tenor_years: tenor,
            exchange_at_inception: true,
        }],
        fx_spot: vec![("USD/EUR".to_string(), 0.92)],
        ois_rates: if with_ois {
            vec![("USD".to_string(), 0.04), ("EUR".to_string(), 0.03)]
        } else {
            vec![]
        },
    }
}

#[test]
fn cross_currency_swap_short_tenor() {
    let r = value_cross_currency_swaps(&xccs(3.0, true));
    let v = &r.swap_valuations[0];
    // discounting at OIS, coupon at the contractual rate
    assert_eq!(v.pv_leg_1, 1_011_607.62);
    assert_eq!(v.pv_leg_2, 857_272.96);
    assert_eq!(v.mtm, 154_334.65);
    assert_eq!(v.mtm_currency1, v.mtm);
    // contractual rate difference in basis points
    assert_eq!(v.cross_currency_basis, -1.0);
    assert_eq!(r.currency_exposure.total_mtm, 154_334.65);
    assert!(r.recommendations.is_empty());
}

#[test]
fn cross_currency_swap_long_tenor_drops_the_coupon() {
    let short = value_cross_currency_swaps(&xccs(7.0, true));
    let v = &short.swap_valuations[0];
    // past five years the coupon leg is notional * (1 - exp(-r t)), so the
    // contractual rate no longer matters at all
    assert_eq!(v.pv_leg_1, 1_030_527.03);
    assert_eq!(v.pv_leg_2, 873_120.25);
    assert_eq!(v.mtm, 157_406.78);

    // the coupon leg is notional * rate/OIS * (1 - exp(-OIS t)), so with a
    // supplied OIS rate the contractual rate is AMPLIFIED rather than
    // cancelled: doubling it adds another full coupon annuity
    let mut altered = xccs(7.0, true);
    altered.swaps[0].rate_1 = 0.09;
    let again = value_cross_currency_swaps(&altered);
    assert_eq!(again.swap_valuations[0].pv_leg_1, 1_305_270.32);
}

#[test]
fn long_tenor_coupon_cancels_when_no_ois_is_supplied() {
    // with no OIS entry the discount rate IS the contract rate, so the two
    // cancel and the long leg collapses to the notional less its discount
    let r = value_cross_currency_swaps(&xccs(7.0, false));
    assert_eq!(r.swap_valuations[0].pv_leg_1, 1_000_000.0);

    let mut doubled = xccs(7.0, false);
    doubled.swaps[0].rate_1 = 0.09;
    let again = value_cross_currency_swaps(&doubled);
    assert_eq!(again.swap_valuations[0].pv_leg_1, 1_000_000.0);
}

#[test]
fn cross_currency_swap_without_ois_uses_contract_rates() {
    let mut req = xccs(2.5, false);
    req.fx_spot = vec![];
    req.swaps[0].notional_1 = 500_000.0;
    req.swaps[0].currency_1 = "USD".into();
    req.swaps[0].rate_1 = 0.05;
    req.swaps[0].notional_2 = 400_000.0;
    req.swaps[0].currency_2 = "GBP".into();
    req.swaps[0].rate_2 = 0.03;
    let r = value_cross_currency_swaps(&req);
    let v = &r.swap_valuations[0];
    // fractional tenor keeps only the whole years it reaches
    assert_eq!(v.pv_leg_1, 487_650.12);
    // no spot for the pair means a rate of 1
    assert_eq!(v.pv_leg_2, 394_043.92);
    assert_eq!(v.mtm, 93_606.21);
    assert_eq!(v.cross_currency_basis, -2.0);
}

#[test]
fn cross_currency_exposure_is_signed_and_flagged() {
    let mut req = xccs(3.0, true);
    let mut extra = req.swaps[0].clone();
    extra.swap_id = "CCS2".into();
    extra.currency_1 = "GBP".into();
    extra.currency_2 = "JPY".into();
    req.swaps.push(extra);
    let r = value_cross_currency_swaps(&req);
    let map = &r.currency_exposure.by_currency;
    let usd = map.iter().find(|(k, _)| k == "USD").unwrap();
    let eur = map.iter().find(|(k, _)| k == "EUR").unwrap();
    assert_eq!(usd.1, 1_000_000.0);
    assert_eq!(eur.1, -920_000.0);
    assert_eq!(map.len(), 4);
    assert!(r
        .recommendations
        .contains(&"Complex multi-currency exposure - simplify where possible".to_string()));
}

fn swap(notional: f64, fixed: f64, floating: f64, tenor: u32, freq: &str) -> Swap {
    Swap {
        swap_id: "S1".into(),
        swap_type: "pay_fixed".into(),
        notional,
        fixed_rate: fixed,
        floating_rate: floating,
        tenor_years: tenor,
        payment_frequency: freq.into(),
    }
}

fn swap_request(swaps: Vec<Swap>, ois: f64) -> SwapValuationRequest {
    SwapValuationRequest {
        company_id: "c1".into(),
        swaps,
        current_yield_curve: vec![("3y".to_string(), 0.032)],
        ois_rate: ois,
    }
}

#[test]
fn swap_legs_depend_only_on_the_rate_difference() {
    let r = value_swaps(&swap_request(
        vec![swap(5_000_000.0, 0.05, 0.04, 3, "semi-annual")],
        0.03,
    ));
    let v = &r.swap_valuations[0];
    assert_eq!(v.fixed_leg_pv, 712_417.29);
    assert_eq!(v.floating_leg_pv, 569_933.83);
    assert_eq!(v.market_value, 142_483.46);
    assert_eq!(v.market_value_pct, 2.8497);
    // the sign alone decides this, not the contract's swap_type
    assert!(v.receive_fixed);
    assert_eq!(v.swap_type, "pay_fixed");
}

#[test]
fn unknown_frequency_is_treated_as_annual() {
    let r = value_swaps(&swap_request(
        vec![swap(1_000_000.0, 0.07, 0.05, 4, "monthly")],
        0.02,
    ));
    let v = &r.swap_valuations[0];
    // four annual periods, not forty-eight monthly ones
    assert_eq!(v.fixed_leg_pv, 266_541.01);
    assert_eq!(v.floating_leg_pv, 190_386.43);
    assert_eq!(v.market_value_pct, 7.6155);

    let quarterly = value_swaps(&swap_request(
        vec![swap(2_000_000.0, 0.06, 0.045, 2, "quarterly")],
        0.025,
    ));
    assert_eq!(quarterly.swap_valuations[0].fixed_leg_pv, 233_448.1);
}

#[test]
fn swap_portfolio_split_and_concentration() {
    let r = value_swaps(&swap_request(
        vec![
            swap(5_000_000.0, 0.05, 0.04, 3, "semi-annual"),
            swap(4_000_000.0, 0.03, 0.045, 3, "semi-annual"),
        ],
        0.03,
    ));
    let negative = r.swap_valuations[1].market_value;
    assert!(negative < 0.0);
    assert!(!r.swap_valuations[1].receive_fixed);
    // pay_fixed_pv is a NEGATIVE figure that is subtracted in the summary
    assert_eq!(r.portfolio_summary.pay_fixed_pv, negative);
    let legs = r.swap_valuations[0].market_value + negative;
    assert!((r.portfolio_summary.total_mtm - legs).abs() < 1e-6);
    assert_eq!(r.portfolio_summary.swap_count, 2);
    // net exposure floors at zero, gross exposure adds the absolute value
    let expected_net = round_half_even(
        (r.portfolio_summary.receive_fixed_pv + negative).max(0.0),
        2,
    );
    let expected_gross = round_half_even(r.portfolio_summary.receive_fixed_pv + negative.abs(), 2);
    assert!((r.counterparty_exposure.net_exposure - expected_net).abs() < 1e-6);
    assert!((r.counterparty_exposure.gross_exposure - expected_gross).abs() < 1e-6);
    assert!(r.counterparty_exposure.largest_exposure > 0.0);
}

fn futures_request() -> FuturesHedgingRequest {
    FuturesHedgingRequest {
        company_id: "c1".into(),
        positions: vec![
            FuturePosition {
                future_id: "ZF1".into(),
                contract_month: "2027-03".into(),
                position_size: 100.0,
                entry_price: 450.0,
                current_price: 462.0,
                contract_size: 1_000.0,
            },
            FuturePosition {
                future_id: "ZF2".into(),
                contract_month: "2027-06".into(),
                position_size: -50.0,
                entry_price: 455.0,
                current_price: 462.0,
                contract_size: 1_000.0,
            },
        ],
        spot_exposure: 200_000.0,
        target_hedge_ratio: 0.8,
        basis_risk: 0.015,
    }
}

#[test]
fn futures_positions_and_ratio() {
    let r = analyze_futures_hedge(&futures_request());
    let first = &r.position_analysis[0];
    assert_eq!(first.contract_value, 100_000.0);
    assert_eq!(first.unrealized_pnl, 1_200_000.0);
    assert_eq!(first.pnl_percentage, 1_200.0);
    assert_eq!(first.daily_var, 1_500.0);

    let second = &r.position_analysis[1];
    assert_eq!(second.contract_value, -50_000.0);
    // a negative contract value flips the sign of the percentage
    assert_eq!(second.unrealized_pnl, -350_000.0);
    assert_eq!(second.pnl_percentage, 700.0);
    assert_eq!(second.daily_var, -750.0);

    assert_eq!(r.hedge_effectiveness.current_ratio, 0.25);
    assert_eq!(r.hedge_effectiveness.effectiveness, 98.5);
    assert_eq!(r.hedge_effectiveness.basis_risk, 1.5);
    assert_eq!(r.optimal_hedge_ratio, 0.8);
}

#[test]
fn futures_recommendations() {
    let r = analyze_futures_hedge(&futures_request());
    assert!(r
        .recommendations
        .contains(&"Hedge ratio deviates from target by 55.00%".to_string()));
    assert!(r
        .recommendations
        .contains(&"Add 110000 notional to reach optimal hedge ratio".to_string()));
    assert!(!r
        .recommendations
        .contains(&"Basis risk is elevated - monitor spread closely".to_string()));
}

#[test]
fn futures_over_hedge_is_never_trimmed() {
    let mut req = futures_request();
    req.spot_exposure = 50_000.0;
    req.target_hedge_ratio = 0.5;
    let r = analyze_futures_hedge(&req);
    // ratio 1.0 against a 0.5 target, so the shortfall is negative
    assert_eq!(r.hedge_effectiveness.current_ratio, 1.0);
    assert!(!r.recommendations.iter().any(|m| m.starts_with("Add ")));
    // the deviation message still appears
    assert!(r
        .recommendations
        .iter()
        .any(|m| m.starts_with("Hedge ratio deviates")));
    // and effectiveness is capped by the 95% optimal, not the target
    assert_eq!(r.optimal_hedge_ratio, 0.5);
}

#[test]
fn futures_optimal_ratio_capped_at_95_percent() {
    let mut req = futures_request();
    req.target_hedge_ratio = 1.0;
    let r = analyze_futures_hedge(&req);
    assert_eq!(r.optimal_hedge_ratio, 0.95);
}

fn commodity_request() -> CommodityHedgingRequest {
    CommodityHedgingRequest {
        company_id: "c1".into(),
        exposures: vec![
            CommodityExposure {
                commodity: "Copper".into(),
                exposure_type: "buy".into(),
                volume: 1_000.0,
                unit: "t".into(),
                current_price: 8_500.0,
                budget_price: 8_000.0,
                volatility: 0.28,
            },
            CommodityExposure {
                commodity: "Wheat".into(),
                exposure_type: "buy".into(),
                volume: 5_000.0,
                unit: "t".into(),
                current_price: 240.0,
                budget_price: 260.0,
                volatility: 0.18,
            },
            CommodityExposure {
                commodity: "Fuel".into(),
                exposure_type: "buy".into(),
                volume: 200_000.0,
                unit: "L".into(),
                current_price: 1.35,
                budget_price: 1.20,
                volatility: 0.35,
            },
        ],
        hedge_ratio: 0.6,
        budget_rate: vec![("Copper".to_string(), 8_800.0)],
    }
}

#[test]
fn commodity_exposure_rows() {
    let r = analyze_commodity_hedge(&commodity_request());
    let copper = &r.exposure_analysis[0];
    assert_eq!(copper.exposure_value, 8_500_000.0);
    // the budget_rate entry wins over the exposure's own budget price
    assert_eq!(copper.budget_value, 8_800_000.0);
    assert_eq!(copper.budget_price, 8_000.0);
    // but the risk figures still use the exposure's own price
    assert_eq!(copper.upside_risk, 500_000.0);
    assert_eq!(copper.downside_risk, 0.0);
    assert_eq!(copper.volatility, 28.0);

    let wheat = &r.exposure_analysis[1];
    assert_eq!(wheat.upside_risk, 0.0);
    assert_eq!(wheat.downside_risk, 100_000.0);
}

#[test]
fn commodity_hedge_plan_and_budget() {
    let r = analyze_commodity_hedge(&commodity_request());
    assert_eq!(r.hedge_plan[0].hedge_type, "Futures");
    assert_eq!(r.hedge_plan[0].hedge_volume, 600.0);
    assert_eq!(r.hedge_plan[0].estimated_cost, 214_200.0);
    assert_eq!(r.hedge_plan[1].hedge_type, "Options");
    assert_eq!(r.hedge_plan[2].protection_level, 0.88);
    assert_eq!(r.hedge_plan[0].hedge_percentage, 60.0);

    assert_eq!(r.budget_impact.total_exposure, 9_970_000.0);
    assert_eq!(r.budget_impact.budget_value, 10_340_000.0);
    assert_eq!(r.budget_impact.variance, -370_000.0);
    assert_eq!(r.budget_impact.variance_pct, -3.58);
    assert_eq!(r.budget_impact.status, "Under Budget");
    assert!(r
        .recommendations
        .contains(&"High volatility commodities need options-based hedging".to_string()));
    assert!(!r
        .recommendations
        .contains(&"Low hedge ratio - consider increasing coverage".to_string()));
}

#[test]
fn commodity_zero_variance_reads_as_under_budget() {
    let r = analyze_commodity_hedge(&CommodityHedgingRequest {
        company_id: "c1".into(),
        exposures: vec![CommodityExposure {
            commodity: "Salt".into(),
            volume: 10.0,
            current_price: 5.0,
            budget_price: 5.0,
            volatility: 0.1,
            ..Default::default()
        }],
        hedge_ratio: 0.3,
        budget_rate: vec![],
    });
    assert_eq!(r.budget_impact.variance, 0.0);
    assert_eq!(r.budget_impact.status, "Under Budget");
    assert!(r
        .recommendations
        .contains(&"Low hedge ratio - consider increasing coverage".to_string()));
}

fn ir_request() -> InterestRateHedgingRequest {
    InterestRateHedgingRequest {
        company_id: "c1".into(),
        exposures: vec![
            DebtExposure {
                debt_id: "D1".into(),
                principal: 4_000_000.0,
                current_rate: 0.062,
                rate_type: "variable".into(),
                maturity_years: 1.5,
                reference_rate: "SOFR".into(),
            },
            DebtExposure {
                debt_id: "D2".into(),
                principal: 2_000_000.0,
                current_rate: 0.048,
                rate_type: "fixed".into(),
                maturity_years: 4.0,
                reference_rate: "fixed".into(),
            },
        ],
        total_debt: 6_000_000.0,
        variable_debt: 4_200_000.0,
        rate_shock_scenarios: vec![0.0025, 0.01, 0.02],
        yield_curve_slope: 0.015,
    }
}

#[test]
fn interest_rate_sensitivity_uses_a_fixed_baseline() {
    let r = analyze_interest_rate_hedge(&ir_request());
    assert_eq!(r.exposure_summary.variable_debt_pct, 70.0);
    assert_eq!(r.exposure_summary.fixed_debt_pct, 30.0);
    assert_eq!(r.exposure_summary.fixed_debt, 1_800_000.0);

    let bp25 = &r.sensitivity_analysis[0];
    assert_eq!(bp25.rate_shock_bp, 25.0);
    assert_eq!(bp25.additional_annual_cost, 10_500.0);
    assert_eq!(bp25.monthly_impact, 875.0);
    // divided by total debt, not by EBITDA
    assert_eq!(bp25.ebitda_impact_pct, 0.18);

    let bp200 = &r.sensitivity_analysis[2];
    assert_eq!(bp200.additional_annual_cost, 84_000.0);
    assert_eq!(bp200.ebitda_impact_pct, 1.4);
    // the loans' own 6.2% and 4.8% rates never enter the calculation
    assert_eq!(bp200.additional_annual_cost, 4_200_000.0 * 0.02);
}

#[test]
fn interest_rate_hedge_actions() {
    let r = analyze_interest_rate_hedge(&ir_request());
    let fix = &r.hedge_recommendations[0];
    assert_eq!(fix.action, "FIX");
    assert_eq!(fix.amount, 1_260_000.0);
    assert_eq!(fix.instrument, "Interest Rate Swap");
    // 5% plus TWICE the slope
    assert_eq!(fix.target_rate, 0.08);
    assert_eq!(fix.tenor, "3 years");
    // weighted maturity 2.3333 years is long enough, so no extension
    assert_eq!(r.hedge_recommendations.len(), 1);
    // exactly 70% is NOT above the threshold, so the message does not fire
    // even though a swap is still recommended
    assert!(!r
        .recommendations
        .contains(&"High variable rate exposure - consider swapping to fixed".to_string()));
    assert!(!r
        .recommendations
        .contains(&"Short debt maturity - extend to reduce refinancing risk".to_string()));
}

#[test]
fn interest_rate_short_maturity_extension() {
    let r = analyze_interest_rate_hedge(&InterestRateHedgingRequest {
        company_id: "c1".into(),
        exposures: vec![DebtExposure {
            debt_id: "D1".into(),
            principal: 1_000_000.0,
            current_rate: 0.07,
            rate_type: "fixed".into(),
            maturity_years: 1.0,
            reference_rate: "fixed".into(),
        }],
        total_debt: 1_000_000.0,
        variable_debt: 200_000.0,
        rate_shock_scenarios: vec![0.01],
        yield_curve_slope: 0.015,
    });
    assert_eq!(r.exposure_summary.variable_debt_pct, 20.0);
    // too little variable debt to swap, but the maturity is short
    assert_eq!(r.hedge_recommendations.len(), 1);
    let extend = &r.hedge_recommendations[0];
    assert_eq!(extend.action, "EXTEND");
    assert_eq!(extend.amount, 200_000.0);
    assert_eq!(extend.instrument, "Fixed Rate Bond");
    // one slope, not two
    assert_eq!(extend.target_rate, 0.065);
    assert!(r
        .recommendations
        .contains(&"Short debt maturity - extend to reduce refinancing risk".to_string()));
}

// --------------------------------------------------------- options

use crate::liquidity::*;
use crate::options::*;

#[test]
fn normal_distribution_helpers() {
    assert!((norm_cdf(0.0) - 0.5).abs() < 1e-15);
    assert!((norm_cdf(1.959963984540054) - 0.975).abs() < 1e-9);
    assert!((norm_cdf(-2.5) - 0.006209665325776132).abs() < 1e-12);
    assert!((norm_pdf(0.0) - 0.3989422804014327).abs() < 1e-12);
    // saturates cleanly beyond the series' useful range
    assert_eq!(erf(6.0), 1.0);
    assert_eq!(erf(-6.0), -1.0);
}

fn price_request() -> OptionsPricingRequest {
    OptionsPricingRequest {
        company_id: "c1".into(),
        options: vec![
            Option {
                option_type: "call".into(),
                strike_price: 100.0,
                spot_price: 100.0,
                time_to_expiry: 0.5,
                volatility: 0.20,
                risk_free_rate: 0.05,
                notional: 1_000.0,
            },
            Option {
                option_type: "put".into(),
                strike_price: 105.0,
                spot_price: 100.0,
                time_to_expiry: 0.25,
                volatility: 0.25,
                risk_free_rate: 0.05,
                notional: 500.0,
            },
            Option {
                option_type: "call".into(),
                strike_price: 70.0,
                spot_price: 50.0,
                time_to_expiry: 0.1,
                volatility: 0.40,
                risk_free_rate: 0.05,
                notional: 200.0,
            },
        ],
        current_spot: 999.0,
    }
}

#[test]
fn black_scholes_matches_reference_premiums() {
    assert_eq!(
        round_half_even(black_scholes_call(100.0, 100.0, 0.5, 0.05, 0.20), 4),
        6.8887
    );
    assert_eq!(
        round_half_even(black_scholes_put(100.0, 105.0, 0.25, 0.05, 0.25), 4),
        7.1356
    );
    assert_eq!(
        round_half_even(black_scholes_call(50.0, 70.0, 0.1, 0.05, 0.40), 4),
        0.0102
    );
}

#[test]
fn option_greeks_and_moneyness() {
    let r = price_options(&price_request());

    let atm = &r.option_valuations[0];
    assert_eq!(atm.premium, 6.8887);
    assert_eq!(atm.premium_pct, 6.8887);
    assert_eq!(atm.moneyness, "ATM");
    assert_eq!(atm.delta, 0.5977);
    assert_eq!(atm.gamma, 0.027359);
    assert_eq!(atm.theta, -0.023);
    assert_eq!(atm.vega, 0.2736);
    assert_eq!(atm.intrinsic_value, 0.0);
    // time value is the raw premium less intrinsic, never rounded
    assert_eq!(
        atm.time_value,
        atm.premium - atm.intrinsic_value + (6.888728577680624 - 6.8887)
    );

    let put = &r.option_valuations[1];
    assert_eq!(put.moneyness, "ITM");
    assert_eq!(put.delta, -0.5901);
    // intrinsic 105 - 100 = 5, time value the rest
    assert_eq!(put.intrinsic_value, 5.0);
    assert!((put.time_value - 2.1356).abs() < 1e-3);

    let otm = &r.option_valuations[2];
    assert_eq!(otm.moneyness, "OTM");
    assert_eq!(otm.delta, 0.0053);
}

#[test]
fn put_theta_uses_the_wrong_cdf_argument() {
    // the source discounts the strike against cdf(-d1); the textbook put
    // theta uses cdf(-d2), which here is a materially different figure
    let s = 100.0_f64;
    let k = 105.0_f64;
    let t = 0.25_f64;
    let rf = 0.05_f64;
    let sigma = 0.25_f64;
    let d1 = ((s / k).ln() + (rf + sigma * sigma / 2.0) * t) / (sigma * t.sqrt());
    let d2 = d1 - sigma * t.sqrt();
    let quirk = (-s * norm_pdf(d1) * sigma / (2.0 * t.sqrt())
        - rf * k * (-rf * t).exp() * norm_cdf(-d1))
        / 365.0;
    let textbook = (-s * norm_pdf(d1) * sigma / (2.0 * t.sqrt())
        - rf * k * (-rf * t).exp() * norm_cdf(-d2))
        / 365.0;
    let r = price_options(&OptionsPricingRequest {
        company_id: "c1".into(),
        options: vec![Option {
            option_type: "put".into(),
            strike_price: k,
            spot_price: s,
            time_to_expiry: t,
            volatility: sigma,
            risk_free_rate: rf,
            notional: 1.0,
        }],
        current_spot: 0.0,
    });
    assert_eq!(r.option_valuations[0].theta, round_half_even(quirk, 4));
    assert_ne!(r.option_valuations[0].theta, round_half_even(textbook, 4));
}

#[test]
fn portfolio_greeks_are_scaled_by_notional() {
    let r = price_options(&price_request());
    assert_eq!(r.portfolio_summary.option_count, 3);
    assert_eq!(r.portfolio_summary.total_premium, 10_458.55);
    // against spot notional, not premium paid
    assert_eq!(r.portfolio_summary.avg_premium_pct, 6.5366);
    assert_eq!(r.greeks_summary.delta, 303.7357);
    assert_eq!(r.greeks_summary.gamma, 43.387117);
    assert_eq!(r.greeks_summary.theta, -40.7535);
    assert_eq!(r.greeks_summary.vega, 371.2467);
    assert!(r
        .recommendations
        .contains(&"Gamma risk elevated - monitor position closely".to_string()));
}

#[test]
fn request_spot_is_ignored() {
    let mut expensive = price_request();
    expensive.current_spot = 1_000_000.0;
    let a = price_options(&price_request());
    let b = price_options(&expensive);
    assert_eq!(
        a.option_valuations[0].premium,
        b.option_valuations[0].premium
    );
}

// ------------------------------------------------------- liquidity

fn forecast_request() -> LiquidityForecastRequest {
    LiquidityForecastRequest {
        company_id: "c1".into(),
        starting_cash: 100_000.0,
        cash_flows: vec![
            CashFlow {
                date: "2026-11-02".into(),
                inflow: 50_000.0,
                outflow: 20_000.0,
                net: 30_000.0,
            },
            CashFlow {
                date: "2026-11-03".into(),
                inflow: 10_000.0,
                outflow: 80_000.0,
                net: -70_000.0,
            },
            CashFlow {
                date: "2026-11-04".into(),
                inflow: 30_000.0,
                outflow: 25_000.0,
                net: 5_000.0,
            },
            CashFlow {
                date: "2026-11-05".into(),
                inflow: 5_000.0,
                outflow: 60_000.0,
                net: -55_000.0,
            },
        ],
        credit_lines_available: 200_000.0,
        minimum_cash_requirement: 60_000.0,
    }
}

#[test]
fn forecast_runs_the_balance_from_the_supplied_net() {
    let r = forecast_liquidity(&forecast_request());
    assert_eq!(r.daily_forecast[0].balance, 130_000.0);
    assert_eq!(r.daily_forecast[1].balance, 60_000.0);
    assert_eq!(r.daily_forecast[3].balance, 10_000.0);
    assert_eq!(r.forecast_summary.ending_cash, 10_000.0);
    assert_eq!(r.forecast_summary.total_inflows, 95_000.0);
    assert_eq!(r.forecast_summary.total_outflows, 185_000.0);
    assert_eq!(r.forecast_summary.net_change, -90_000.0);
    assert_eq!(r.forecast_summary.min_balance, 10_000.0);
    assert_eq!(r.forecast_summary.credit_available, 200_000.0);
}

#[test]
fn forecast_trusts_a_mis_stated_net() {
    // inflow less outflow says -10,000 but the supplied net says +40,000,
    // and the balance follows the net
    let mut req = forecast_request();
    req.cash_flows = vec![CashFlow {
        date: "2026-11-02".into(),
        inflow: 10_000.0,
        outflow: 20_000.0,
        net: 40_000.0,
    }];
    let r = forecast_liquidity(&req);
    assert_eq!(r.daily_forecast[0].balance, 140_000.0);
    assert_eq!(r.forecast_summary.net_change, 40_000.0);
}

#[test]
fn gaps_use_the_rounded_balance() {
    let r = forecast_liquidity(&forecast_request());
    // 60,000 exactly equals the floor, so only the 10,000 day is a gap
    assert_eq!(r.liquidity_gaps.len(), 1);
    assert_eq!(r.liquidity_gaps[0].date, "2026-11-05");
    assert_eq!(r.liquidity_gaps[0].shortfall, 50_000.0);
    assert_eq!(r.liquidity_gaps[0].required_credit, 50_000.0);
    // available credit is reported but never netted off the shortfall
    assert_eq!(r.forecast_summary.credit_available, 200_000.0);
    assert!(r
        .recommendations
        .contains(&"Liquidity gap identified - draw on credit facilities".to_string()));
    assert!(r
        .recommendations
        .contains(&"Significant cash depletion - monitor closely".to_string()));
}

fn liquidity_request() -> LiquidityRequest {
    LiquidityRequest {
        company_id: "c1".into(),
        cash: 250_000.0,
        marketable_securities: 400_000.0,
        credit_facilities_available: 300_000.0,
        credit_facilities_total: 1_000_000.0,
        current_assets: 1_800_000.0,
        current_liabilities: 1_200_000.0,
        monthly_fixed_costs: 350_000.0,
        monthly_variable_costs: 250_000.0,
        monthly_min_revenue: 900_000.0,
    }
}

#[test]
fn liquidity_coverage_includes_undrawn_credit() {
    let r = analyze_liquidity(&liquidity_request());
    assert_eq!(r.position.total_liquid_assets, 950_000.0);
    assert_eq!(r.liquidity_ratios.current_ratio, 1.5);
    assert_eq!(r.liquidity_ratios.quick_ratio, 0.5417);
    assert_eq!(r.liquidity_ratios.cash_ratio, 0.2083);
    assert_eq!(r.liquidity_ratios.credit_utilization, 0.7);
    // 950,000 of "liquid assets" including 300,000 of undrawn facility
    assert_eq!(r.coverage_days, 47.5);
}

#[test]
fn stress_scenarios_mix_their_bases() {
    let r = analyze_liquidity(&liquidity_request());
    // a revenue drop charged against COSTS
    assert_eq!(r.stress_test.revenue_drop_30.monthly_impact, 180_000.0);
    assert_eq!(r.stress_test.revenue_drop_50.monthly_impact, 300_000.0);
    // a payment delay charged against MINIMUM REVENUE
    assert_eq!(r.stress_test.payment_delay_60.monthly_impact, 540_000.0);
    assert_eq!(r.stress_test.revenue_drop_30.runway_days, 47.5 / 0.3);
    assert_eq!(
        r.stress_test.payment_delay_60.runway_days,
        950_000.0 / 540_000.0 * 30.0
    );
}

#[test]
fn coverage_warnings_fire_together() {
    let r = analyze_liquidity(&liquidity_request());
    // 47.5 days is both under 30... no: under 60 only
    assert!(!r.recommendations.iter().any(|m| m.starts_with("CRITICAL")));
    assert!(r.recommendations.iter().any(|m| m.starts_with("WARNING")));
    assert!(r
        .recommendations
        .contains(&"Quick ratio below 1.0 - focus on converting receivables to cash".to_string()));

    let starved = analyze_liquidity(&LiquidityRequest {
        credit_facilities_available: 0.0,
        cash: 100_000.0,
        marketable_securities: 0.0,
        ..liquidity_request()
    });
    assert!(starved
        .recommendations
        .iter()
        .any(|m| m.starts_with("CRITICAL")));
    assert!(starved
        .recommendations
        .iter()
        .any(|m| m.starts_with("WARNING")));
}

fn risk_request() -> LiquidityAnalysisRequest {
    LiquidityAnalysisRequest {
        company_id: "c1".into(),
        reporting_date: "2026-10-31".into(),
        liquid_assets: vec![
            LiquidityAsset {
                asset_id: "A1".into(),
                asset_type: "cash".into(),
                value: 500_000.0,
                haircut: 0.02,
                maturity_bucket: "0-30d".into(),
            },
            LiquidityAsset {
                asset_id: "A2".into(),
                asset_type: "corporate_bond".into(),
                value: 300_000.0,
                haircut: 0.15,
                maturity_bucket: "1y+".into(),
            },
            LiquidityAsset {
                asset_id: "HQLA".into(),
                asset_type: "gilts".into(),
                value: 700_000.0,
                haircut: 0.0,
                maturity_bucket: "0-30d".into(),
            },
        ],
        liabilities: vec![
            Liability {
                liability_id: "L1".into(),
                liability_type: "deposit".into(),
                value: 400_000.0,
                outflow_rate: 0.10,
                maturity_bucket: "0-30d".into(),
            },
            Liability {
                liability_id: "L2".into(),
                liability_type: "wholesale".into(),
                value: 600_000.0,
                outflow_rate: 0.05,
                maturity_bucket: "90d".into(),
            },
            Liability {
                liability_id: "L3".into(),
                liability_type: "contingent".into(),
                value: 250_000.0,
                outflow_rate: 0.25,
                maturity_bucket: "1y+".into(),
            },
        ],
    }
}

#[test]
fn lcr_is_capped_and_nsfr_ignores_haircuts() {
    let r = analyze_liquidity_risk(&risk_request());
    assert_eq!(r.liquid_asset_buffer, 1_445_000.0);
    // the raw ratio is 3,729% and the cap holds it at 500
    assert_eq!(r.lcr_ratio, 500.0);
    assert_eq!(r.liquidity_risk_rating, "LOW");
    // 95% of liabilities over 50% of GROSS assets
    // the source never rounds the NSFR
    assert!((r.nsfr_ratio - 158.33333333333331).abs() < 1e-9);
    assert_eq!(r.net_cash_position, 1_312_500.0);
    assert_eq!(r.survival_horizon_days, 327);
}

#[test]
fn maturity_mismatch_is_a_fixed_constant() {
    let a = analyze_liquidity_risk(&risk_request());
    let mut b = risk_request();
    b.liabilities.clear();
    b.liquid_assets.clear();
    let other = analyze_liquidity_risk(&b);
    // identical for every company, whatever the balance sheet
    assert_eq!(a.maturity_mismatch.day_30, -50_000.0);
    assert_eq!(a.maturity_mismatch.day_90, -200_000.0);
    assert_eq!(a.maturity_mismatch.year_1, -100_000.0);
    assert_eq!(other.maturity_mismatch.day_30, a.maturity_mismatch.day_30);
}

#[test]
fn low_outflows_read_as_an_infinite_lcr() {
    let r = analyze_liquidity_risk(&LiquidityAnalysisRequest {
        company_id: "c1".into(),
        liquid_assets: vec![LiquidityAsset {
            value: 100_000.0,
            ..Default::default()
        }],
        liabilities: vec![Liability {
            value: 1_000_000.0,
            outflow_rate: 0.001,
            ..Default::default()
        }],
        ..Default::default()
    });
    // outflows 1,000 do not exceed the 100,000 "net inflow", so the source
    // divides by the infinite case and the cap reports 500
    assert_eq!(r.lcr_ratio, 500.0);
    assert_eq!(r.liquidity_risk_rating, "LOW");
    // survival truncates rather than rounds
    assert_eq!(r.survival_horizon_days, 3_000);
}

#[test]
fn medium_and_high_ratings() {
    let medium = analyze_liquidity_risk(&LiquidityAnalysisRequest {
        company_id: "c1".into(),
        liquid_assets: vec![LiquidityAsset {
            value: 90_000.0,
            ..Default::default()
        }],
        liabilities: vec![Liability {
            value: 1_000_000.0,
            outflow_rate: 0.15,
            ..Default::default()
        }],
        ..Default::default()
    });
    // 90,000 / (150,000 - 75,000) = 120%, inside the medium band
    assert_eq!(medium.lcr_ratio, 120.0);
    assert_eq!(medium.liquidity_risk_rating, "MEDIUM");

    let high = analyze_liquidity_risk(&LiquidityAnalysisRequest {
        company_id: "c1".into(),
        liquid_assets: vec![LiquidityAsset {
            value: 20_000.0,
            ..Default::default()
        }],
        liabilities: vec![Liability {
            value: 1_000_000.0,
            outflow_rate: 0.15,
            ..Default::default()
        }],
        ..Default::default()
    });
    // 20,000 / 75,000 = 26.7%
    assert_eq!(high.liquidity_risk_rating, "HIGH");
}

// ------------------------------------------------------- cash management

use crate::cash::*;
use vimbai_common::Money;

fn amount(text: &str) -> Money {
    Money::parse(text).expect("valid amount")
}

fn optimization_request() -> CashOptimizationRequest {
    CashOptimizationRequest {
        company_id: "c1".into(),
        current_cash: amount("750000"),
        target_cash_balance: amount("500000"),
        investment_options: vec![
            InvestmentOption {
                name: "T-Bill".into(),
                min_investment: Some(amount("10000")),
                yield_rate: amount("0.052"),
                maturity_days: 90,
            },
            InvestmentOption {
                name: "Corporate paper".into(),
                min_investment: Some(amount("100000")),
                yield_rate: amount("0.061"),
                maturity_days: 30,
            },
            InvestmentOption {
                name: String::new(),
                min_investment: None,
                yield_rate: amount("0.048"),
                maturity_days: 180,
            },
        ],
        risk_tolerance: "moderate".into(),
    }
}

#[test]
fn optimization_treats_the_minimum_as_a_cap() {
    let r = optimize_cash_flow(&optimization_request(), "2026-11-01");
    assert_eq!(r.excess_cash.to_fixed(2), "250000.00");
    // 30% of 250,000 is 75,000, and each instrument is measured against it:
    // a 10,000 "minimum" caps the bill at 10,000, a 100,000 minimum cannot
    // reach the 75,000 slice, and no minimum takes the whole slice
    assert_eq!(r.recommended_investments[0].amount.to_fixed(2), "10000.00");
    assert_eq!(r.recommended_investments[1].amount.to_fixed(2), "75000.00");
    // the third carries no cap at all, and the source defaults a missing
    // minimum to 10,000
    assert_eq!(r.recommended_investments[2].amount.to_fixed(2), "10000.00");
    // the unnamed instrument falls back to T-Bill
    // an absent cap defaults to 10,000 in the source, which would have capped
    // this instrument too; it is modelled as an explicit None so the default
    // applies only when the key really is missing
    assert_eq!(r.recommended_investments.len(), 3);
    assert_eq!(r.recommended_investments[2].instrument, "T-Bill");
    // 95,000 of recommendations against 250,000 of excess: each option gets
    // its own slice rather than sharing one pot
    assert_eq!(
        Money::sum(r.recommended_investments.iter().map(|i| &i.amount)).to_fixed(2),
        "95000.00"
    );
}

#[test]
fn an_explicit_zero_cap_invests_nothing() {
    let mut req = optimization_request();
    req.investment_options = vec![InvestmentOption {
        name: "Zero cap".into(),
        min_investment: Some(Money::zero()),
        yield_rate: amount("0.05"),
        maturity_days: 30,
    }];
    let r = optimize_cash_flow(&req, "2026-11-01");
    assert!(r.recommended_investments.is_empty());
}

#[test]
fn an_absent_cap_defaults_to_ten_thousand() {
    let mut req = optimization_request();
    req.investment_options = vec![InvestmentOption {
        name: "No cap given".into(),
        min_investment: None,
        yield_rate: amount("0.05"),
        maturity_days: 30,
    }];
    let r = optimize_cash_flow(&req, "2026-11-01");
    assert_eq!(r.recommended_investments[0].amount.to_fixed(2), "10000.00");
}

#[test]
fn optimization_yield_is_a_flat_constant() {
    let r = optimize_cash_flow(&optimization_request(), "2026-11-01");
    assert_eq!(r.expected_yield.to_fixed(3), "0.045");
    // every instrument quotes 5.2%, 6.1% and 4.8% respectively and none of it
    // moves the plan yield
    assert_eq!(r.recommended_investments[1].yield_rate.to_fixed(3), "0.061");
}

#[test]
fn optimization_maturity_schedule_crosses_the_year() {
    let r = optimize_cash_flow(&optimization_request(), "2026-11-01");
    assert_eq!(r.maturity_schedule[0].date, "2027-01-30");
    assert_eq!(r.maturity_schedule[1].date, "2026-12-01");
    assert_eq!(r.maturity_schedule[2].date, "2027-04-30");
    assert_eq!(r.maturity_schedule[0].amount.to_fixed(2), "10000.00");
}

#[test]
fn optimization_handles_an_astronomical_balance() {
    // 30 digits before the point: beyond any float's ability to hold the
    // cents, and the 30% slice is still exact
    let mut req = optimization_request();
    req.current_cash = amount("123456789012345678901234567890.55");
    req.target_cash_balance = amount("0");
    req.investment_options = vec![InvestmentOption {
        name: "Repo".into(),
        min_investment: Some(amount("1000000000000000000000000000000000")),
        yield_rate: amount("0.03"),
        maturity_days: 7,
    }];
    let r = optimize_cash_flow(&req, "2026-11-01");
    assert_eq!(
        r.excess_cash.to_fixed(2),
        "123456789012345678901234567890.55"
    );
    assert_eq!(
        r.recommended_investments[0].amount.to_fixed(2),
        "37037036703703703670370370367.16"
    );
}

#[test]
fn optimization_with_no_excess_still_offers_nothing() {
    let mut req = optimization_request();
    req.current_cash = amount("400000");
    let r = optimize_cash_flow(&req, "2026-11-01");
    assert_eq!(r.excess_cash.to_fixed(2), "0.00");
    assert!(r.recommended_investments.is_empty());
    assert!(r.maturity_schedule.is_empty());
}

fn reconciliation_request() -> BankReconciliationRequest {
    BankReconciliationRequest {
        company_id: "c1".into(),
        bank_account_id: "BA-1".into(),
        statement_date: "2026-10-31".into(),
        book_balance: amount("250000"),
    }
}

#[test]
fn reconciliation_uses_fixed_outstanding_items() {
    let r = reconcile_bank(&reconciliation_request());
    assert_eq!(r.book_balance.to_fixed(2), "250000.00");
    // the bank balance is the book balance plus a flat 500
    assert_eq!(r.bank_balance.to_fixed(2), "250500.00");
    assert_eq!(r.adjusted_book_balance.to_fixed(2), "241500.00");
    // built from book plus deposits, not from the bank balance
    assert_eq!(r.adjusted_bank_balance.to_fixed(2), "262000.00");
    assert_eq!(r.reconciliation_status, "review_needed");
    assert_eq!(r.outstanding_checks.len(), 2);
    assert_eq!(r.deposits_in_transit[0].amount.to_fixed(2), "12000.00");
    // the statement carries no differences because the list is never filled
    assert!(r.differences.is_empty());
}

#[test]
fn reconciliation_ignores_the_statement_date_and_scales() {
    // 40-digit book balance: the fixed offsets still apply exactly
    let mut req = reconciliation_request();
    req.book_balance = amount(&"9".repeat(40));
    let r = reconcile_bank(&req);
    assert_eq!(
        r.adjusted_book_balance.to_fixed(2),
        req.book_balance.minus(&amount("8500")).to_fixed(2)
    );
    assert_eq!(
        r.adjusted_bank_balance.to_fixed(2),
        req.book_balance.plus(&amount("12000")).to_fixed(2)
    );
    // still 40 significant digits, none of them floating
    assert_eq!(r.adjusted_book_balance.to_fixed(0).len(), 40);
    assert_eq!(r.reconciliation_status, "review_needed");
}

fn ratio_request() -> LiquidityRatioRequest {
    LiquidityRatioRequest {
        company_id: "c1".into(),
        current_assets: amount("1800000"),
        current_liabilities: amount("1200000"),
        inventory: amount("300000"),
        cash_and_equivalents: amount("250000"),
        marketable_securities: amount("150000"),
    }
}

#[test]
fn liquidity_ratios_and_the_constant_burn() {
    let r = calculate_liquidity_ratios(&ratio_request());
    assert_eq!(r.current_ratio.to_fixed(4), "1.5000");
    assert_eq!(r.quick_ratio.to_fixed(4), "1.2500");
    assert_eq!(r.cash_ratio.to_fixed(4), "0.3333");
    // 400,000 of cash and securities over a hard-coded 50,000 daily burn
    assert_eq!(r.defensive_interval.to_fixed(2), "8.00");
    // 1.5*0.3 + 1.25*0.4 + 0.3333*0.3, i.e. ratios added together
    assert_eq!(r.liquidity_index.to_fixed(4), "1.0500");
    // 1.5 is not above 1.5, so an otherwise healthy book is "weak"
    assert_eq!(r.assessment, "weak");
}

#[test]
fn liquidity_assessment_only_reads_the_current_ratio() {
    let r = calculate_liquidity_ratios(&LiquidityRatioRequest {
        company_id: "c1".into(),
        current_assets: amount("3000000"),
        current_liabilities: amount("1000000"),
        inventory: amount("50000"),
        cash_and_equivalents: amount("10000"),
        marketable_securities: amount("0"),
    });
    assert_eq!(r.current_ratio.to_fixed(4), "3.0000");
    assert_eq!(r.cash_ratio.to_fixed(4), "0.0100");
    assert_eq!(r.assessment, "strong");
}

#[test]
fn ratios_survive_a_hyperinflated_balance_sheet() {
    let r = calculate_liquidity_ratios(&LiquidityRatioRequest {
        company_id: "c1".into(),
        current_assets: amount("999999999999999999999999999999.00"),
        current_liabilities: amount("333333333333333333333333333333.00"),
        inventory: amount("1"),
        cash_and_equivalents: amount("1e30"),
        marketable_securities: amount("0"),
    });
    assert_eq!(r.current_ratio.to_fixed(4), "3.0000");
    assert_eq!(r.cash_ratio.to_fixed(4), "3.0000");
    // 1e30 of cash over a 50,000 daily burn
    assert_eq!(
        r.defensive_interval.to_fixed(2),
        "20000000000000000000000000.00"
    );
}

#[test]
fn zero_denominators_yield_zero() {
    let r = calculate_liquidity_ratios(&LiquidityRatioRequest {
        company_id: "c1".into(),
        current_assets: amount("500"),
        current_liabilities: Money::zero(),
        ..Default::default()
    });
    assert_eq!(r.current_ratio.to_fixed(4), "0.0000");
    assert_eq!(r.quick_ratio.to_fixed(4), "0.0000");
    assert_eq!(r.cash_ratio.to_fixed(4), "0.0000");
    assert_eq!(r.assessment, "weak");
}

#[test]
fn calendar_helpers_handle_month_and_year_boundaries() {
    assert_eq!(add_days("2026-11-01", 90), "2027-01-30");
    assert_eq!(add_days("2026-12-31", 1), "2027-01-01");
    assert_eq!(add_days("2024-02-28", 1), "2024-02-29");
    assert_eq!(add_days("2025-02-28", 1), "2025-03-01");
    assert_eq!(add_days("2026-11-01", 0), "2026-11-01");
    assert_eq!(add_days("2026-11-01", -1), "2026-10-31");
    // an unparseable date is passed straight through
    assert_eq!(add_days("not-a-date", 5), "not-a-date");
}

// ------------------------------------------------ treasury operations

use crate::operations::*;
use std::collections::BTreeMap;

fn txn(id: &str, kind: &str, amount: &str, status: &str) -> TreasuryTransaction {
    TreasuryTransaction {
        transaction_id: id.into(),
        transaction_type: kind.into(),
        amount: amount_of(amount),
        currency: "USD".into(),
        counterparty: "CP".into(),
        status: status.into(),
    }
}

fn amount_of(text: &str) -> Money {
    Money::parse(text).expect("valid amount")
}

fn operations_request() -> TreasuryOperationsRequest {
    let mut cash = BTreeMap::new();
    cash.insert("ACC-A".to_string(), amount_of("750000.75"));
    cash.insert("ACC-B".to_string(), amount_of("400000.50"));
    let mut limits = BTreeMap::new();
    limits.insert("max_single".to_string(), amount_of("500000"));
    limits.insert("min_cash".to_string(), amount_of("1000000"));
    TreasuryOperationsRequest {
        company_id: "c1".into(),
        transactions: vec![
            txn("T1", "inflow", "500000.55", "completed"),
            txn("T2", "outflow", "250000.25", "pending"),
            txn("T3", "outflow", "12000.10", "failed"),
            txn("T4", "inflow", "3000.00", "failed"),
            txn("T5", "outflow", "1000.00", "completed"),
        ],
        bank_balances: cash,
        treasury_policy_limits: limits,
    }
}

#[test]
fn operations_match_the_python_figures() {
    let r = analyze_treasury_operations(&operations_request());
    assert_eq!(r.total_cash.to_fixed(2), "1150001.25");
    let s = &r.transactions_summary;
    assert_eq!(s.total_transactions, 5);
    assert_eq!(s.total_inflows.to_fixed(2), "503000.55");
    assert_eq!(s.total_outflows.to_fixed(2), "263000.35");
    assert_eq!(s.net_flow.to_fixed(2), "240000.20");
    assert_eq!(s.pending, 1);
    assert_eq!(s.failed, 2);
    assert_eq!(r.policy_compliance.limits_defined, 2);
    assert_eq!(r.policy_compliance.compliance_rate.to_fixed(2), "0.95");
    assert_eq!(r.bank_balance_summary.bank_count, 2);
    assert_eq!(
        r.bank_balance_summary.largest_balance.to_fixed(2),
        "750000.75"
    );
    assert_eq!(r.alerts, vec!["High failure rate: 2 failed transactions"]);
    assert_eq!(
        r.recommendations,
        vec!["Review failed transactions and resolve issues"]
    );
}

#[test]
fn failed_transactions_still_move_the_net_flow() {
    // status never filters a total: both failed transactions are counted
    let mut req = operations_request();
    req.transactions = vec![
        txn("F1", "inflow", "100", "failed"),
        txn("F2", "outflow", "40", "failed"),
    ];
    let r = analyze_treasury_operations(&req);
    assert_eq!(r.transactions_summary.net_flow.to_fixed(2), "60.00");
    assert_eq!(r.transactions_summary.failed, 2);
}

#[test]
fn failure_alert_needs_more_than_five_percent() {
    // 1 failure in 20 is not above 5%, so it is silent
    let mut req = operations_request();
    req.transactions = (0..20)
        .map(|i| {
            txn(
                &format!("T{i}"),
                "inflow",
                "100",
                if i == 0 { "failed" } else { "completed" },
            )
        })
        .collect();
    assert!(analyze_treasury_operations(&req).alerts.is_empty());

    // one failure in 19 is above 5% and trips it
    req.transactions.truncate(19);
    let alerts = analyze_treasury_operations(&req).alerts;
    assert_eq!(alerts, vec!["High failure rate: 1 failed transactions"]);
}

#[test]
fn pending_alert_needs_more_than_twenty() {
    let mut req = operations_request();
    req.transactions = (0..20)
        .map(|i| txn(&format!("P{i}"), "inflow", "1", "pending"))
        .collect();
    assert!(analyze_treasury_operations(&req)
        .alerts
        .iter()
        .all(|a| !a.starts_with("Many pending")));
    req.transactions.push(txn("P20", "inflow", "1", "pending"));
    let alerts = analyze_treasury_operations(&req).alerts;
    assert!(alerts.iter().any(|a| a == "Many pending transactions: 21"));
}

#[test]
fn low_cash_recommendation_is_strict() {
    let mut req = operations_request();
    req.bank_balances = BTreeMap::new();
    req.bank_balances
        .insert("only".into(), amount_of("1000000"));
    // exactly the threshold, so no recommendation
    assert!(analyze_treasury_operations(&req)
        .recommendations
        .iter()
        .all(|r| !r.contains("low")));

    req.bank_balances
        .insert("only".into(), amount_of("999999.99"));
    assert!(analyze_treasury_operations(&req)
        .recommendations
        .iter()
        .any(|r| r.contains("Cash position is low")));
}

#[test]
fn unknown_transaction_types_are_ignored() {
    let mut req = operations_request();
    req.transactions = vec![
        txn("U1", "transfer", "500", "completed"),
        txn("U2", "Inflow", "700", "completed"),
    ];
    let r = analyze_treasury_operations(&req);
    // neither "transfer" nor a capitalised "Inflow" counts
    assert_eq!(r.transactions_summary.total_inflows.to_fixed(2), "0.00");
    assert_eq!(r.transactions_summary.total_outflows.to_fixed(2), "0.00");
    assert_eq!(r.transactions_summary.net_flow.to_fixed(2), "0.00");
}

#[test]
fn operations_survive_a_hyperinflated_book() {
    let mut req = operations_request();
    req.bank_balances = BTreeMap::new();
    req.bank_balances
        .insert("ZWG".into(), amount_of("123456789012345678901234567890.55"));
    req.bank_balances.insert("USD".into(), amount_of("0.45"));
    req.transactions = vec![
        txn("H1", "inflow", "1e30", "completed"),
        txn("H2", "outflow", "1", "completed"),
    ];
    let r = analyze_treasury_operations(&req);
    // the cents of a 30-digit balance survive the summation: .55 plus .45
    // lands on a whole unit rather than on a binary approximation of it
    assert_eq!(
        r.total_cash.to_fixed(2),
        "123456789012345678901234567891.00"
    );
    assert_eq!(
        r.bank_balance_summary.largest_balance.to_fixed(2),
        "123456789012345678901234567890.55"
    );
    assert_eq!(
        r.transactions_summary.total_inflows.to_fixed(2),
        "1000000000000000000000000000000.00"
    );
    assert_eq!(
        r.transactions_summary.net_flow.to_fixed(2),
        "999999999999999999999999999999.00"
    );
}
