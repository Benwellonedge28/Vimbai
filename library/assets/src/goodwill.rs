//! goodwill-service `/test`: IAS 36 impairment of a CGU, goodwill first.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

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
    pub carrying_value: f64,
    #[serde(default)]
    pub fair_value: f64,
    #[serde(default)]
    pub value_in_use: f64,
    #[serde(default)]
    pub goodwill_allocated: f64,
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
pub fn impairment_test(cgu: &Cgu, method: TestMethod) -> ImpairmentResult {
    let recoverable = match method {
        TestMethod::FairValueOnly => cgu.fair_value,
        TestMethod::ViuOnly => cgu.value_in_use,
        TestMethod::Both => cgu.fair_value.max(cgu.value_in_use),
    };
    let impairment = (cgu.carrying_value - recoverable).max(0.0);
    let goodwill_impaired = impairment.min(cgu.goodwill_allocated);
    let other = impairment - goodwill_impaired;
    ImpairmentResult {
        cgu_name: cgu.name.clone(),
        carrying_value: py_round2(cgu.carrying_value),
        recoverable_amount: py_round2(recoverable),
        impairment_loss: py_round2(impairment),
        goodwill_impaired: py_round2(goodwill_impaired),
        other_assets_impaired: py_round2(other),
        is_impaired: impairment > 0.0,
        post_impairment_carrying: py_round2(cgu.carrying_value - impairment),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cgu() -> Cgu {
        Cgu {
            name: "cgu1".into(),
            carrying_value: 1000.0,
            fair_value: 700.0,
            value_in_use: 750.0,
            goodwill_allocated: 200.0,
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
        c.value_in_use = 1200.0;
        let r = impairment_test(&c, TestMethod::ViuOnly);
        assert!(!r.is_impaired);
        assert_eq!(r.impairment_loss, 0.0);
    }
}
