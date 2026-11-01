//! depreciation-service: batch calculation and per-method schedules.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepreciationAsset {
    pub asset_id: String,
    pub cost: f64,
    #[serde(default)]
    pub salvage: f64,
    #[serde(default = "default_life")]
    pub useful_life_years: i64,
}
fn default_life() -> i64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDepreciationRow {
    pub asset_id: String,
    pub annual_depreciation: f64,
    pub accumulated: f64,
}

/// The service publishes a five-year schedule on `/calculate` that is not a
/// real schedule: every year repeats `total * 0.9`. Kept as-is because the
/// endpoint's consumers read it as a forecast placeholder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepreciationScheduleRow {
    pub year: i64,
    pub depreciation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepreciationOutput {
    pub total_depreciation: f64,
    pub asset_depreciation: Vec<AssetDepreciationRow>,
    pub schedule: Vec<DepreciationScheduleRow>,
}

/// Batch `/calculate`. Python quirks kept: declining balance ignores
/// salvage and uses 2/life on full cost; the returned schedule is a flat
/// placeholder (total*0.9 for years 1..5), not a real schedule.
pub fn depreciation_batch(method: &str, assets: &[DepreciationAsset]) -> DepreciationOutput {
    let mut total = 0.0;
    let mut rows = Vec::with_capacity(assets.len());
    for a in assets {
        let life = if a.useful_life_years > 0 {
            a.useful_life_years as f64
        } else {
            5.0
        };
        let annual = match method {
            "straight_line" => (a.cost - a.salvage) / life,
            "declining_balance" => a.cost * (2.0 / life),
            _ => a.cost / life,
        };
        total += annual;
        rows.push(AssetDepreciationRow {
            asset_id: a.asset_id.clone(),
            annual_depreciation: py_round2(annual),
            accumulated: py_round2(annual),
        });
    }
    let placeholder = py_round2(total * 0.9);
    DepreciationOutput {
        total_depreciation: py_round2(total),
        asset_depreciation: rows,
        schedule: (1..=5)
            .map(|year| DepreciationScheduleRow {
                year,
                depreciation: placeholder,
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodScheduleRow {
    pub year: i64,
    pub depreciation: f64,
    pub accumulated: f64,
    pub book_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepreciationMethodOutput {
    pub annual_depreciation: f64,
    pub monthly_depreciation: f64,
    pub schedule: Vec<MethodScheduleRow>,
}

/// `/method`: straight line on depreciable amount, double declining on
/// full cost. Python subtracts the annual amount from `remaining` WITHOUT
/// flooring, so the final book value can dip below salvage by one
/// year's worth only after max(0, ...) on the book value itself.
pub fn depreciation_method(
    method: &str,
    cost: f64,
    salvage: f64,
    useful_life: i64,
) -> DepreciationMethodOutput {
    let life = if useful_life > 0 {
        useful_life as f64
    } else {
        1.0
    };
    let depreciable = cost - salvage;
    let annual = match method {
        "straight_line" => depreciable / life,
        "double_declining" => cost * (2.0 / life),
        _ => depreciable / life,
    };
    let mut remaining = depreciable;
    let mut schedule = Vec::with_capacity(useful_life.max(0) as usize);
    for y in 1..=useful_life {
        remaining -= annual;
        schedule.push(MethodScheduleRow {
            year: y,
            depreciation: py_round2(annual),
            accumulated: py_round2(annual * (y as f64)),
            book_value: py_round2((salvage + remaining).max(0.0)),
        });
    }
    DepreciationMethodOutput {
        annual_depreciation: py_round2(annual),
        monthly_depreciation: py_round2(annual / 12.0),
        schedule,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: &str, cost: f64, salvage: f64, life: i64) -> DepreciationAsset {
        DepreciationAsset {
            asset_id: id.into(),
            cost,
            salvage,
            useful_life_years: life,
        }
    }

    #[test]
    fn straight_line_annual() {
        let r = depreciation_batch("straight_line", &[asset("a", 10000.0, 1000.0, 5)]);
        assert_eq!(r.total_depreciation, 1800.0);
        assert_eq!(r.asset_depreciation[0].annual_depreciation, 1800.0);
    }

    #[test]
    fn declining_balance_ignores_salvage() {
        let r = depreciation_batch("declining_balance", &[asset("a", 10000.0, 5000.0, 5)]);
        assert_eq!(r.total_depreciation, 4000.0);
    }

    #[test]
    fn method_schedule_accumulates() {
        let r = depreciation_method("straight_line", 10000.0, 1000.0, 5);
        assert_eq!(r.annual_depreciation, 1800.0);
        assert_eq!(r.schedule[4].accumulated, 9000.0);
        assert_eq!(r.schedule[4].book_value, 1000.0);
    }

    #[test]
    fn double_declining_final_book_value_floored() {
        let r = depreciation_method("double_declining", 10000.0, 1000.0, 5);
        assert_eq!(r.annual_depreciation, 4000.0);
        // 5 years of 4000 never exhausts: remaining goes very negative -> 0
        assert_eq!(r.schedule[4].book_value, 0.0);
    }
}
