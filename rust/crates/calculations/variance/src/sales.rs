#![forbid(unsafe_code)]
//! Sales variances, ported from sales-price-variance-service,
//! sales-volume-variance-service and variance-service `/sales-variance`.
//!
//! Two conventions live side by side (both preserved):
//!
//! 1. Revenue-based (sales-price service): per item,
//!    price = (AP - BP) x actual_volume, volume = (AVol - BVol) x BP,
//!    mix = (AVol - revised_budget_volume) x BP, qty = (revised - BVol) x BP
//!    where revised = BVol x (total_actual_volume / total_budget_volume).
//! 2. Contribution-margin-based (sales-volume service): volume variance
//!    on average budgeted CM, mix variance per product, yield = volume - mix.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarianceItem {
    #[serde(alias = "product_name")]
    pub product: String,
    pub budgeted_price: f64,
    pub actual_price: f64,
    pub budgeted_volume: f64,
    pub actual_volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemResult {
    pub product: String,
    pub sales_price_variance: f64,
    pub sales_volume_variance: f64,
    pub sales_mix_variance: f64,
    pub sales_quantity_variance: f64,
    pub favorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesAnalysisOutput {
    pub company_id: String,
    pub period: String,
    pub total_sales_price_variance: f64,
    pub total_sales_volume_variance: f64,
    pub total_sales_mix_variance: f64,
    pub total_sales_quantity_variance: f64,
    pub items: Vec<ItemResult>,
}

/// Revenue-based price/volume/mix/quantity analysis (sales-price service).
pub fn sales_price_volume_analysis(
    company_id: &str,
    period: &str,
    items: &[VarianceItem],
) -> SalesAnalysisOutput {
    let total_budget_volume: f64 = items.iter().map(|i| i.budgeted_volume).sum();
    let total_actual_volume: f64 = items.iter().map(|i| i.actual_volume).sum();
    let volume_ratio = if total_budget_volume != 0.0 {
        total_actual_volume / total_budget_volume
    } else {
        0.0
    };

    let mut total_price = 0.0;
    let mut total_volume = 0.0;
    let mut total_mix = 0.0;
    let mut total_qty = 0.0;
    let mut results = Vec::with_capacity(items.len());

    for item in items {
        let price_variance = (item.actual_price - item.budgeted_price) * item.actual_volume;
        let volume_variance = (item.actual_volume - item.budgeted_volume) * item.budgeted_price;
        let revised_budget_volume = item.budgeted_volume * volume_ratio;
        let mix_variance = (item.actual_volume - revised_budget_volume) * item.budgeted_price;
        let quantity_variance =
            (revised_budget_volume - item.budgeted_volume) * item.budgeted_price;

        total_price += price_variance;
        total_volume += volume_variance;
        total_mix += mix_variance;
        total_qty += quantity_variance;

        results.push(ItemResult {
            product: item.product.clone(),
            sales_price_variance: py_round2(price_variance),
            sales_volume_variance: py_round2(volume_variance),
            sales_mix_variance: py_round2(mix_variance),
            sales_quantity_variance: py_round2(quantity_variance),
            favorable: price_variance >= 0.0 && volume_variance >= 0.0,
        });
    }

    SalesAnalysisOutput {
        company_id: company_id.to_string(),
        period: period.to_string(),
        total_sales_price_variance: py_round2(total_price),
        total_sales_volume_variance: py_round2(total_volume),
        total_sales_mix_variance: py_round2(total_mix),
        total_sales_quantity_variance: py_round2(total_qty),
        items: results,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductVolume {
    pub product: String,
    pub budgeted_volume: f64,
    pub actual_volume: f64,
    pub budgeted_price: f64,
    pub budgeted_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeCmOutput {
    pub company_id: String,
    pub period: String,
    pub volume_variance: f64,
    pub mix_variance: f64,
    pub yield_variance: f64,
    pub total_variance: f64,
    pub product_details: Vec<serde_json::Value>,
}

/// CM-based volume/mix/yield analysis (sales-volume service).
pub fn sales_volume_cm_analysis(
    company_id: &str,
    period: &str,
    products: &[ProductVolume],
) -> VolumeCmOutput {
    let total_budget: f64 = products.iter().map(|p| p.budgeted_volume).sum();
    let total_actual: f64 = products.iter().map(|p| p.actual_volume).sum();

    let budgeted_cm_total: f64 = products
        .iter()
        .map(|p| (p.budgeted_price - p.budgeted_cost) * p.budgeted_volume)
        .sum();

    let volume_var = if total_budget != 0.0 {
        (total_actual - total_budget) * (budgeted_cm_total / total_budget)
    } else {
        0.0
    };

    let mut mix_var = 0.0;
    for p in products {
        let budget_cm = p.budgeted_price - p.budgeted_cost;
        let expected_at_actual = if total_budget != 0.0 {
            total_actual * (p.budgeted_volume / total_budget)
        } else {
            0.0
        };
        mix_var += (p.actual_volume - expected_at_actual) * budget_cm;
    }

    let yield_var = volume_var - mix_var;

    let details: Vec<serde_json::Value> = products
        .iter()
        .map(|p| {
            let cm = p.budgeted_price - p.budgeted_cost;
            let vol_diff = p.actual_volume - p.budgeted_volume;
            serde_json::json!({
                "product": p.product,
                "budgeted_volume": p.budgeted_volume,
                "actual_volume": p.actual_volume,
                "volume_diff": vol_diff,
                "contribution_margin": py_round2(cm),
                "product_variance": py_round2(vol_diff * cm),
            })
        })
        .collect();

    VolumeCmOutput {
        company_id: company_id.to_string(),
        period: period.to_string(),
        volume_variance: py_round2(volume_var),
        mix_variance: py_round2(mix_var),
        yield_variance: py_round2(yield_var),
        total_variance: py_round2(volume_var),
        product_details: details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_volume_mix_matches_python_service() {
        let items = vec![VarianceItem {
            product: "widget".into(),
            budgeted_price: 10.0,
            actual_price: 12.0,
            budgeted_volume: 100.0,
            actual_volume: 90.0,
        }];
        let out = sales_price_volume_analysis("c1", "2026-09", &items);
        // price = (12-10)*90 = 180; volume = (90-100)*10 = -100
        // ratio = 0.9; revised = 90; mix = 0; qty = (90-100)*10 = -100
        assert_eq!(out.total_sales_price_variance, 180.0);
        assert_eq!(out.total_sales_volume_variance, -100.0);
        assert_eq!(out.total_sales_mix_variance, 0.0);
        assert_eq!(out.total_sales_quantity_variance, -100.0);
        assert!(!out.items[0].favorable); // volume variance -100 < 0
    }

    #[test]
    fn cm_volume_mix_yield_matches_python_service() {
        let products = vec![
            ProductVolume {
                product: "A".into(),
                budgeted_volume: 100.0,
                actual_volume: 120.0,
                budgeted_price: 10.0,
                budgeted_cost: 6.0,
            },
            ProductVolume {
                product: "B".into(),
                budgeted_volume: 100.0,
                actual_volume: 80.0,
                budgeted_price: 20.0,
                budgeted_cost: 10.0,
            },
        ];
        let out = sales_volume_cm_analysis("c1", "2026-09", &products);
        // budget_cm_total = (4*100)+(10*100) = 1400
        // volume = (200-200)*(1400/200) = 0
        // A: expected_at_actual = 200*(100/200)=100; (120-100)*4 = 80
        // B: expected = 100; (80-100)*10 = -200 -> mix = -120
        // yield = 0 - (-120) = 120
        assert_eq!(out.volume_variance, 0.0);
        assert_eq!(out.mix_variance, -120.0);
        assert_eq!(out.yield_variance, 120.0);
        assert_eq!(out.total_variance, 0.0);
    }
}
