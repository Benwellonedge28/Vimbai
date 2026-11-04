//! goodwill-service `/test`: IAS 36 impairment of a CGU, goodwill first.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vimbai_common::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestMethod {
    FairValueOnly,
    ViuOnly,
    /// anything else in Python falls through to max(fv, viu)
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cgu {
    #[serde(default)]
    pub name: String,
    pub carrying_value: Money,
    #[serde(default)]
    pub fair_value: Money,
    #[serde(default)]
    pub value_in_use: Money,
    #[serde(default)]
    pub goodwill_allocated: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpairmentResult {
    pub cgu_name: String,
    pub carrying_value: f64,
    pub recoverable_amount: f64,
    pub impairment_loss: f64,
    pub goodwill_impaired: f64,
    pub other_assets_impaired: f64,
    pub is_impaired: bool,
    pub post_impairment_carrying: f64,
}

/// Recoverable amount by method; impairment capped by goodwill first,
/// remainder allocated to other assets.
///
/// Carry value, recoverable amount and goodwill are amounts, so an impairment
/// loss is a decimal difference. That matters when a loss lands exactly on the
/// goodwill balance: the cap is a comparison, not a rounded guess.
pub fn impairment_test(cgu: &Cgu, method: TestMethod) -> ImpairmentResult {
    let zero = Money::zero();
    let recoverable = match method {
        TestMethod::FairValueOnly => cgu.fair_value.clone(),
        TestMethod::ViuOnly => cgu.value_in_use.clone(),
        TestMethod::Both => {
            if cgu.fair_value > cgu.value_in_use {
                cgu.fair_value.clone()
            } else {
                cgu.value_in_use.clone()
            }
        }
    };
    let shortfall = cgu.carrying_value.minus(&recoverable);
    let impairment = if shortfall > zero {
        shortfall
    } else {
        zero.clone()
    };
    let goodwill_impaired = if impairment > cgu.goodwill_allocated {
        cgu.goodwill_allocated.clone()
    } else {
        impairment.clone()
    };
    let other = impairment.minus(&goodwill_impaired);
    ImpairmentResult {
        cgu_name: cgu.name.clone(),
        carrying_value: cgu.carrying_value.round(2).to_f64_lossy(),
        recoverable_amount: recoverable.round(2).to_f64_lossy(),
        impairment_loss: impairment.round(2).to_f64_lossy(),
        goodwill_impaired: goodwill_impaired.round(2).to_f64_lossy(),
        other_assets_impaired: other.round(2).to_f64_lossy(),
        is_impaired: impairment > zero,
        post_impairment_carrying: cgu
            .carrying_value
            .minus(&impairment)
            .round(2)
            .to_f64_lossy(),
    }
}

/// The service's `/test` body, answered in the shape it published. The `id` the
/// service stamped is left to the caller: this is a pure function of the payload.
pub fn impairment_test_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Request {
        company_id: String,
        fiscal_year: i64,
        cgu: Cgu,
        #[serde(default)]
        test_method: String,
    }
    let request: Request =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let method = match request.test_method.as_str() {
        "fair_value_only" => TestMethod::FairValueOnly,
        "viu_only" => TestMethod::ViuOnly,
        _ => TestMethod::Both,
    };
    let result = impairment_test(&request.cgu, method);
    Ok(serde_json::json!({
        "company_id": request.company_id,
        "fiscal_year": request.fiscal_year,
        "cgu_name": result.cgu_name,
        "carrying_value": result.carrying_value,
        "recoverable_amount": result.recoverable_amount,
        "impairment_loss": result.impairment_loss,
        "goodwill_impaired": result.goodwill_impaired,
        "other_assets_impaired": result.other_assets_impaired,
        "test_method": if request.test_method.is_empty() { "higher_of".to_string() } else { request.test_method },
        "is_impaired": result.is_impaired,
        "post_impairment_carrying": result.post_impairment_carrying,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cgu() -> Cgu {
        Cgu {
            name: "cgu1".into(),
            carrying_value: Money::from_i64(1000),
            fair_value: Money::from_i64(700),
            value_in_use: Money::from_i64(750),
            goodwill_allocated: Money::from_i64(200),
        }
    }

    #[test]
    fn higher_of_fv_and_viu() {
        let r = impairment_test(&cgu(), TestMethod::Both);
        assert_eq!(r.recoverable_amount, 750.0);
        assert_eq!(r.impairment_loss, 250.0);
        assert_eq!(r.goodwill_impaired, 200.0);
        assert_eq!(r.other_assets_impaired, 50.0);
        assert!(r.is_impaired);
    }

    #[test]
    fn fair_value_only_ignores_viu() {
        let r = impairment_test(&cgu(), TestMethod::FairValueOnly);
        assert_eq!(r.impairment_loss, 300.0);
        assert_eq!(r.post_impairment_carrying, 700.0);
    }

    #[test]
    fn no_impairment_when_recoverable_above_carrying() {
        let mut c = cgu();
        c.value_in_use = Money::from_i64(1200);
        let r = impairment_test(&c, TestMethod::ViuOnly);
        assert!(!r.is_impaired);
        assert_eq!(r.impairment_loss, 0.0);
    }
}
