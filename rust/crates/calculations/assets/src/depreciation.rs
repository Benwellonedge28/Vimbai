//! depreciation-service: batch calculation and per-method schedules.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vimbai_common::Money;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepreciationAsset {
    pub asset_id: String,
    pub cost: Money,
    #[serde(default)]
    pub salvage: Money,
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
///
/// Every figure is an amount, so the charge and the register total are decimals:
/// the total is the sum of the whole register, and it is then multiplied by 0.9
/// for the forecast line, which is exactly where a double drifts.
pub fn depreciation_batch(method: &str, assets: &[DepreciationAsset]) -> DepreciationOutput {
    let two = Money::from_i64(2);
    let mut annuals: Vec<Money> = Vec::with_capacity(assets.len());
    let mut rows = Vec::with_capacity(assets.len());
    for a in assets {
        let life = Money::from_i64(if a.useful_life_years > 0 {
            a.useful_life_years
        } else {
            5
        });
        let annual = match method {
            "straight_line" => a.cost.minus(&a.salvage).over(&life),
            "declining_balance" => a.cost.times(&two).over(&life),
            _ => a.cost.over(&life),
        };
        annuals.push(annual.clone());
        rows.push(AssetDepreciationRow {
            asset_id: a.asset_id.clone(),
            annual_depreciation: annual.round(2).to_f64_lossy(),
            accumulated: annual.round(2).to_f64_lossy(),
        });
    }
    let total = Money::sum(&annuals);
    let placeholder = total
        .times(&Money::parse("0.9").expect("0.9 is a decimal"))
        .round(2);
    DepreciationOutput {
        total_depreciation: total.round(2).to_f64_lossy(),
        asset_depreciation: rows,
        schedule: (1..=5)
            .map(|year| DepreciationScheduleRow {
                year,
                depreciation: placeholder.to_f64_lossy(),
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
///
/// The schedule is the accumulation that matters here: each year's book value is
/// last year's carrying amount less the exact charge.
pub fn depreciation_method(
    method: &str,
    cost: &Money,
    salvage: &Money,
    useful_life: i64,
) -> DepreciationMethodOutput {
    let zero = Money::zero();
    let life = Money::from_i64(if useful_life > 0 { useful_life } else { 1 });
    let two = Money::from_i64(2);
    let depreciable = cost.minus(salvage);
    let annual = match method {
        "straight_line" => depreciable.over(&life),
        "double_declining" => cost.times(&two).over(&life),
        _ => depreciable.over(&life),
    };
    let mut remaining = depreciable.clone();
    let mut schedule = Vec::with_capacity(useful_life.max(0) as usize);
    for y in 1..=useful_life {
        remaining = remaining.minus(&annual);
        let book_value = salvage.plus(&remaining);
        schedule.push(MethodScheduleRow {
            year: y,
            depreciation: annual.round(2).to_f64_lossy(),
            accumulated: annual.times(&Money::from_i64(y)).round(2).to_f64_lossy(),
            book_value: if book_value > zero {
                book_value.round(2).to_f64_lossy()
            } else {
                0.0
            },
        });
    }
    DepreciationMethodOutput {
        annual_depreciation: annual.round(2).to_f64_lossy(),
        monthly_depreciation: annual.over(&Money::from_i64(12)).round(2).to_f64_lossy(),
        schedule,
    }
}

/// The service's `/calculate` body. The period is only echoed back.
pub fn depreciation_batch_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Request {
        company_id: String,
        assets: Vec<DepreciationAsset>,
        method: String,
        period_start: String,
        period_end: String,
    }
    let request: Request =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let output = depreciation_batch(&request.method, &request.assets);
    Ok(serde_json::json!({
        "company_id": request.company_id,
        "period": {"start": request.period_start, "end": request.period_end},
        "total_depreciation": output.total_depreciation,
        "asset_depreciation": output.asset_depreciation,
        "schedule": output.schedule,
    }))
}

/// The service's `/method` body. The endpoint has always published the schedule
/// as `depreciation_schedule`, so the name is kept here rather than in clients.
pub fn depreciation_method_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Request {
        cost: Money,
        salvage_value: Money,
        useful_life: i64,
        method: String,
    }
    let request: Request =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let output = depreciation_method(
        &request.method,
        &request.cost,
        &request.salvage_value,
        request.useful_life,
    );
    Ok(serde_json::json!({
        "method": request.method,
        "annual_depreciation": output.annual_depreciation,
        "monthly_depreciation": output.monthly_depreciation,
        "depreciation_schedule": output.schedule,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: &str, cost: i64, salvage: i64, life: i64) -> DepreciationAsset {
        DepreciationAsset {
            asset_id: id.into(),
            cost: Money::from_i64(cost),
            salvage: Money::from_i64(salvage),
            useful_life_years: life,
        }
    }

    #[test]
    fn straight_line_annual() {
        let r = depreciation_batch("straight_line", &[asset("a", 10_000, 1_000, 5)]);
        assert_eq!(r.total_depreciation, 1800.0);
        assert_eq!(r.asset_depreciation[0].annual_depreciation, 1800.0);
    }

    #[test]
    fn declining_balance_ignores_salvage() {
        let r = depreciation_batch("declining_balance", &[asset("a", 10_000, 5_000, 5)]);
        assert_eq!(r.total_depreciation, 4000.0);
    }

    #[test]
    fn method_schedule_accumulates() {
        let r = depreciation_method(
            "straight_line",
            &Money::from_i64(10_000),
            &Money::from_i64(1_000),
            5,
        );
        assert_eq!(r.annual_depreciation, 1800.0);
        assert_eq!(r.schedule[4].accumulated, 9000.0);
        assert_eq!(r.schedule[4].book_value, 1000.0);
    }

    #[test]
    fn double_declining_final_book_value_floored() {
        let r = depreciation_method(
            "double_declining",
            &Money::from_i64(10_000),
            &Money::from_i64(1_000),
            5,
        );
        assert_eq!(r.annual_depreciation, 4000.0);
        // 5 years of 4000 never exhausts: remaining goes very negative -> 0
        assert_eq!(r.schedule[4].book_value, 0.0);
    }
}
