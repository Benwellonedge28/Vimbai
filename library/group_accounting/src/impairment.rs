//! Goodwill impairment: the higher of fair value and value in use, applied to the
//! goodwill first.

use serde::{Deserialize, Serialize};

use crate::money::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cgu {
    pub cgu_id: String,
    pub name: String,
    pub carrying_value: f64,
    pub goodwill_allocated: f64,
    pub fair_value: f64,
    pub value_in_use: f64,
}

impl Cgu {
    pub fn new(cgu_id: &str, name: &str, carrying_value: f64) -> Cgu {
        Cgu {
            cgu_id: cgu_id.to_string(),
            name: name.to_string(),
            carrying_value,
            goodwill_allocated: 0.0,
            fair_value: 0.0,
            value_in_use: 0.0,
        }
    }

    pub fn with_goodwill(mut self, goodwill_allocated: f64) -> Cgu {
        self.goodwill_allocated = goodwill_allocated;
        self
    }

    pub fn with_recoverable(mut self, fair_value: f64, value_in_use: f64) -> Cgu {
        self.fair_value = fair_value;
        self.value_in_use = value_in_use;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestMethod {
    HigherOf,
    FairValueOnly,
    ViuOnly,
}

impl TestMethod {
    pub fn parse(text: &str) -> TestMethod {
        match text {
            "fair_value_only" => TestMethod::FairValueOnly,
            "viu_only" => TestMethod::ViuOnly,
            _ => TestMethod::HigherOf,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            TestMethod::HigherOf => "higher_of",
            TestMethod::FairValueOnly => "fair_value_only",
            TestMethod::ViuOnly => "viu_only",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImpairmentRequest {
    pub company_id: String,
    pub fiscal_year: i32,
    pub cgu: Cgu,
    pub test_method: TestMethod,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImpairmentResult {
    pub company_id: String,
    pub fiscal_year: i32,
    pub cgu_name: String,
    pub carrying_value: f64,
    pub recoverable_amount: f64,
    pub impairment_loss: f64,
    pub goodwill_impaired: f64,
    pub other_assets_impaired: f64,
    pub test_method: String,
    pub is_impaired: bool,
    pub post_impairment_carrying: f64,
}

/// Test a cash-generating unit. Goodwill absorbs the loss before other assets,
/// which is the IAS 36 order.
pub fn test(request: &ImpairmentRequest) -> ImpairmentResult {
    let recoverable = match request.test_method {
        TestMethod::FairValueOnly => request.cgu.fair_value,
        TestMethod::ViuOnly => request.cgu.value_in_use,
        TestMethod::HigherOf => request.cgu.fair_value.max(request.cgu.value_in_use),
    };

    let impairment = (request.cgu.carrying_value - recoverable).max(0.0);
    let goodwill_impaired = impairment.min(request.cgu.goodwill_allocated);
    let other_impaired = impairment - goodwill_impaired;

    ImpairmentResult {
        company_id: request.company_id.clone(),
        fiscal_year: request.fiscal_year,
        cgu_name: request.cgu.name.clone(),
        carrying_value: money(request.cgu.carrying_value),
        recoverable_amount: money(recoverable),
        impairment_loss: money(impairment),
        goodwill_impaired: money(goodwill_impaired),
        other_assets_impaired: money(other_impaired),
        test_method: request.test_method.value().to_string(),
        is_impaired: impairment > 0.0,
        post_impairment_carrying: money(request.cgu.carrying_value - impairment),
    }
}
