//! The workspace holds the same maths in more than one place in several corners.
//!
//! Where the two copies are both reachable, a disagreement is a bug waiting to be found by
//! a caller rather than by us. These tests do not decide which copy is right. They make sure
//! the two answer the same, so a divergence shows up here and not in production.

use capital_valuation::{capital_returns, dcf};
use market_risk::normal as risk_normal;
use market_risk::options as risk_options;
use treasury_fx::options as fx_options;

const TOLERANCE: f64 = 1e-12;

fn rates() -> Vec<f64> {
    vec![0.0, 0.005, 0.01, 0.05, 0.1, 0.25]
}

#[test]
fn the_normal_distribution_helpers_agree() {
    for step in -40..=40 {
        let x = step as f64 / 10.0;
        let cdf = (risk_normal::norm_cdf(x) - fx_options::norm_cdf(x)).abs();
        let pdf = (risk_normal::norm_pdf(x) - fx_options::norm_pdf(x)).abs();
        assert!(cdf < TOLERANCE, "norm_cdf differs at {x}: {cdf}");
        assert!(pdf < TOLERANCE, "norm_pdf differs at {x}: {pdf}");
    }
}

#[test]
fn black_scholes_agrees_between_the_two_options_modules() {
    for spot in [90.0, 100.0, 110.0] {
        for strike in [95.0, 105.0] {
            for sigma in [0.15, 0.3] {
                let call = (risk_options::black_scholes_call(spot, strike, 1.0, 0.05, sigma)
                    - fx_options::black_scholes_call(spot, strike, 1.0, 0.05, sigma))
                .abs();
                let put = (risk_options::black_scholes_put(spot, strike, 1.0, 0.05, sigma)
                    - fx_options::black_scholes_put(spot, strike, 1.0, 0.05, sigma))
                .abs();
                assert!(
                    call < TOLERANCE,
                    "call differs at {spot}/{strike}/{sigma}: {call}"
                );
                assert!(
                    put < TOLERANCE,
                    "put differs at {spot}/{strike}/{sigma}: {put}"
                );
            }
        }
    }
}

#[test]
fn the_discount_factor_lives_twice_in_one_crate_and_agrees() {
    for rate in rates() {
        for years in 0..31 {
            let old = capital_returns::discount_factor(rate, years as i64)
                .expect("years is not negative")
                .discount_factor;
            let new = dcf::discount_factor(rate, years as u32).discount_factor;
            assert!(
                (old - new).abs() < TOLERANCE,
                "discount factor differs at {rate}/{years}: {old} vs {new}"
            );
        }
    }
}

#[test]
fn the_annuity_factor_lives_twice_in_one_crate_and_agrees() {
    for rate in rates() {
        for years in 0..31 {
            let old = capital_returns::annuity_factor(rate, years as i64).annuity_factor;
            let new = dcf::annuity_factor(rate, years as u32);
            assert!(
                (old - new).abs() < TOLERANCE,
                "annuity factor differs at {rate}/{years}: {old} vs {new}"
            );
        }
    }
}

#[test]
fn simple_over_under_absorption_agrees_between_the_two_overhead_modules() {
    for absorbed in [0.0, 40_000.0, 100_000.0] {
        for actual in [0.0, 40_000.0, 65_000.0] {
            let old = cost_accounting::overhead::simple_over_under(absorbed, actual);
            let new = cost_accounting::overhead_variance::simple_over_under(absorbed, actual);
            assert!(
                (old.variance - new.variance).abs() < TOLERANCE,
                "overhead variance differs at {absorbed}/{actual}"
            );
            assert_eq!(
                old.status,
                new.status.as_str(),
                "status differs at {absorbed}/{actual}"
            );
        }
    }
}

#[test]
fn material_cost_variance_agrees_between_the_two_crates() {
    let (standard_price, actual_price, standard_quantity, actual_quantity) =
        (10.0, 11.0, 500.0, 480.0);
    let cost_accounting_side = cost_accounting::material_variance::material_cost_variance(
        &cost_accounting::material_variance::MaterialInput {
            name: "steel".to_string(),
            standard_price,
            actual_price,
            standard_quantity,
            actual_quantity,
        },
    );
    let variance_side = variance::material::material_cost_variance(
        standard_price,
        actual_price,
        standard_quantity,
        actual_quantity,
    );
    assert!(
        (cost_accounting_side.total_material_cost_variance - variance_side.total_material_variance)
            .abs()
            < TOLERANCE,
        "material variance totals differ"
    );
    assert!(
        (cost_accounting_side.price_variance - variance_side.price_variance).abs() < TOLERANCE,
        "price variance differs"
    );
    assert!(
        (cost_accounting_side.usage_variance - variance_side.usage_variance).abs() < TOLERANCE,
        "usage variance differs"
    );
}
