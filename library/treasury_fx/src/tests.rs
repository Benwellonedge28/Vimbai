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
