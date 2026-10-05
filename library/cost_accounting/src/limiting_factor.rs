#![forbid(unsafe_code)]
//! Limiting-factor (scarce resource) analysis, ported from
//! limiting-factor-service (`/analyze`, `/compare-products`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductWithFactor {
    pub product_id: String,
    pub product_name: String,
    pub demand: f64,
    pub contribution_per_unit: f64,
    pub factor_per_unit: f64,
    pub contribution_per_factor_unit: f64,
    pub ranking: usize,
    pub units_to_produce: f64,
    pub total_contribution: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitingFactorAnalysis {
    pub limiting_factor: String,
    pub factor_type: String,
    pub total_available: f64,
    pub factor_used: f64,
    pub factor_remaining: f64,
    pub total_contribution: f64,
    pub products: Vec<ProductWithFactor>,
    pub optimal_production_plan: std::collections::BTreeMap<String, f64>,
}

/// Optimal production plan under a single limiting factor
/// (limiting-factor-service `/analyze`). Products are ranked by
/// contribution per factor unit (stable sort, highest first) and
/// allocated greedily. A product with `factor_per_unit == 0` takes its
/// full demand and uses no factor (Python parity).
pub fn analyze_limiting_factor(
    limiting_factor: &str,
    factor_type: &str,
    total_available: f64,
    products: &[ProductWithFactor],
) -> LimitingFactorAnalysis {
    let mut ranked: Vec<ProductWithFactor> = products
        .iter()
        .map(|p| {
            let mut q = p.clone();
            if q.factor_per_unit > 0.0 {
                q.contribution_per_factor_unit = q.contribution_per_unit / q.factor_per_unit;
            }
            q
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.contribution_per_factor_unit
            .partial_cmp(&a.contribution_per_factor_unit)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut remaining = total_available;
    let mut factor_used = 0.0;
    let mut total_contribution = 0.0;
    let mut plan = std::collections::BTreeMap::new();
    for (i, p) in ranked.iter_mut().enumerate() {
        p.ranking = i + 1;
        if p.factor_per_unit > 0.0 {
            let max_units = remaining / p.factor_per_unit;
            p.units_to_produce = p.demand.min(max_units);
        } else {
            p.units_to_produce = p.demand;
        }
        let used = p.units_to_produce * p.factor_per_unit;
        remaining -= used;
        factor_used += used;
        p.total_contribution = p.units_to_produce * p.contribution_per_unit;
        total_contribution += p.total_contribution;
        plan.insert(p.product_id.clone(), p.units_to_produce);
    }

    LimitingFactorAnalysis {
        limiting_factor: limiting_factor.to_string(),
        factor_type: factor_type.to_string(),
        total_available,
        factor_used,
        factor_remaining: remaining,
        total_contribution,
        products: ranked,
        optimal_production_plan: plan,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactorComparison {
    pub product_id: String,
    pub product_name: String,
    pub contribution_per_unit: f64,
    pub factor_per_unit: f64,
    pub contribution_per_factor_unit: f64,
    pub ranking: usize,
}

/// Rank products by contribution per factor unit
/// (limiting-factor-service `/compare-products`).
pub fn compare_products_for_factor(products: &[ProductWithFactor]) -> Vec<FactorComparison> {
    let mut comparisons: Vec<FactorComparison> = products
        .iter()
        .map(|p| {
            let cpf = if p.factor_per_unit > 0.0 {
                p.contribution_per_unit / p.factor_per_unit
            } else {
                0.0
            };
            FactorComparison {
                product_id: p.product_id.clone(),
                product_name: p.product_name.clone(),
                contribution_per_unit: p.contribution_per_unit,
                factor_per_unit: p.factor_per_unit,
                contribution_per_factor_unit: cpf,
                ranking: 0,
            }
        })
        .collect();
    comparisons.sort_by(|a, b| {
        b.contribution_per_factor_unit
            .partial_cmp(&a.contribution_per_factor_unit)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, c) in comparisons.iter_mut().enumerate() {
        c.ranking = i + 1;
    }
    comparisons
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, demand: f64, cpu: f64, fpu: f64) -> ProductWithFactor {
        ProductWithFactor {
            product_id: id.into(),
            product_name: format!("Product {id}"),
            demand,
            contribution_per_unit: cpu,
            factor_per_unit: fpu,
            contribution_per_factor_unit: 0.0,
            ranking: 0,
            units_to_produce: 0.0,
            total_contribution: 0.0,
        }
    }

    #[test]
    fn greedy_allocation() {
        // A: 20/hr, B: 15/hr -> B ranks first (15/1 > 20/2 = 10)
        let r = analyze_limiting_factor(
            "labour hours",
            "hours",
            100.0,
            &[p("A", 30.0, 20.0, 2.0), p("B", 50.0, 15.0, 1.0)],
        );
        assert_eq!(r.products[0].product_id, "B");
        assert_eq!(r.products[0].ranking, 1);
        assert_eq!(r.products[0].units_to_produce, 50.0);
        // A gets remaining 50 hours / 2 = 25 units of its 30 demand
        assert_eq!(r.products[1].units_to_produce, 25.0);
        assert_eq!(r.factor_used, 100.0);
        assert_eq!(r.factor_remaining, 0.0);
        assert_eq!(r.total_contribution, 50.0 * 15.0 + 25.0 * 20.0);
        assert_eq!(r.optimal_production_plan["A"], 25.0);
        assert_eq!(r.optimal_production_plan["B"], 50.0);
    }

    #[test]
    fn idle_factor_remaining() {
        let r = analyze_limiting_factor("hours", "hours", 500.0, &[p("A", 10.0, 20.0, 2.0)]);
        assert_eq!(r.factor_remaining, 480.0);
        assert_eq!(r.factor_used, 20.0);
    }

    #[test]
    fn zero_factor_product_takes_full_demand() {
        // Python parity: factor_per_unit == 0 -> produce full demand, use nothing
        let r = analyze_limiting_factor("hours", "hours", 10.0, &[p("A", 40.0, 5.0, 0.0)]);
        assert_eq!(r.products[0].units_to_produce, 40.0);
        assert_eq!(r.factor_used, 0.0);
        assert_eq!(r.total_contribution, 200.0);
    }

    #[test]
    fn compare_ranks_by_cpf() {
        let c = compare_products_for_factor(&[p("A", 0.0, 20.0, 2.0), p("B", 0.0, 15.0, 1.0)]);
        assert_eq!(c[0].product_id, "B");
        assert_eq!(c[0].contribution_per_factor_unit, 15.0);
        assert_eq!(c[0].ranking, 1);
        assert_eq!(c[1].ranking, 2);
        assert_eq!(
            compare_products_for_factor(&[p("Z", 0.0, 5.0, 0.0)])[0].contribution_per_factor_unit,
            0.0
        );
    }
}
