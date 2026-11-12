//! Internal controls testing: exception rates banded into deficiency levels.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use vimbai_common::numbers::round_dp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeficiencyLevel {
    None,
    Deficiency,
    Significant,
    Material,
}

impl DeficiencyLevel {
    pub fn value(self) -> &'static str {
        match self {
            DeficiencyLevel::None => "none",
            DeficiencyLevel::Deficiency => "deficiency",
            DeficiencyLevel::Significant => "significant_deficiency",
            DeficiencyLevel::Material => "material_weakness",
        }
    }

    /// Bands: none at zero, deficiency to 5%, significant to 10%, material above.
    pub fn from_exception_rate(rate: f64) -> DeficiencyLevel {
        if rate == 0.0 {
            DeficiencyLevel::None
        } else if rate <= 0.05 {
            DeficiencyLevel::Deficiency
        } else if rate <= 0.10 {
            DeficiencyLevel::Significant
        } else {
            DeficiencyLevel::Material
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Control {
    pub id: String,
    pub name: String,
    pub control_type: String,
    pub description: String,
    pub process: String,
    pub test_sample_size: i64,
    pub exceptions_found: i64,
    pub last_tested: String,
    pub remediation_needed: bool,
}

impl Control {
    pub fn new(id: &str, name: &str, control_type: &str, process: &str) -> Control {
        Control {
            id: id.to_string(),
            name: name.to_string(),
            control_type: control_type.to_string(),
            description: String::new(),
            process: process.to_string(),
            test_sample_size: 25,
            exceptions_found: 0,
            last_tested: String::new(),
            remediation_needed: false,
        }
    }

    pub fn tested(mut self, sample_size: i64, exceptions_found: i64) -> Control {
        self.test_sample_size = sample_size;
        self.exceptions_found = exceptions_found;
        self
    }

    fn exception_rate(&self) -> f64 {
        if self.test_sample_size != 0 {
            self.exceptions_found as f64 / self.test_sample_size as f64
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestingRequest {
    pub company_id: String,
    pub fiscal_year: i32,
    pub controls: Vec<Control>,
}

impl TestingRequest {
    pub fn new(company_id: &str, fiscal_year: i32, controls: Vec<Control>) -> TestingRequest {
        TestingRequest {
            company_id: company_id.to_string(),
            fiscal_year,
            controls,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestingResult {
    pub company_id: String,
    pub fiscal_year: i32,
    pub total_controls: usize,
    pub controls_tested: usize,
    pub effective_controls: usize,
    pub deficient_controls: usize,
    pub material_weaknesses: usize,
    pub significant_deficiencies: usize,
    pub overall_assessment: String,
    pub control_details: Vec<Value>,
    pub remediation_plan: Vec<Value>,
}

/// Test a control set.
///
/// Preserved: a significant deficiency counts as both significant *and*
/// deficient, so the two columns overlap; `controls_tested` equals the total
/// because untested controls were never modelled.
pub fn test(request: &TestingRequest) -> TestingResult {
    let mut effective = 0usize;
    let mut deficient = 0usize;
    let mut material = 0usize;
    let mut significant = 0usize;
    let mut details = Vec::new();
    let mut remediation = Vec::new();

    for control in &request.controls {
        let rate = control.exception_rate();
        let level = DeficiencyLevel::from_exception_rate(rate);
        match level {
            DeficiencyLevel::None => effective += 1,
            DeficiencyLevel::Deficiency => deficient += 1,
            DeficiencyLevel::Significant => {
                significant += 1;
                deficient += 1;
            }
            DeficiencyLevel::Material => {
                material += 1;
                deficient += 1;
            }
        }
        if level != DeficiencyLevel::None {
            remediation.push(json!({
                "control_id": control.id,
                "control_name": control.name,
                "deficiency_level": level.value(),
                "exception_rate": round_dp(rate * 100.0, 1),
                "recommended_action": format!(
                    "Redesign {} - increase sample testing frequency",
                    control.name
                ),
            }));
        }
        details.push(json!({
            "id": control.id,
            "name": control.name,
            "type": control.control_type,
            "process": control.process,
            "sample_size": control.test_sample_size,
            // "exceptions" and "effective" are the names the service published;
            // the diagnostic pair stays alongside them rather than replacing them.
            "exceptions": control.exceptions_found,
            "exception_rate": round_dp(rate * 100.0, 1),
            "deficiency_level": level.value(),
            "effective": level == DeficiencyLevel::None,
            "remediation_needed": level != DeficiencyLevel::None,
            "last_tested": control.last_tested,
        }));
    }

    let assessment = if material > 0 {
        "Adverse opinion - material weaknesses identified"
    } else if significant > 0 {
        "Qualified opinion - significant deficiencies identified"
    } else {
        "Unqualified opinion - controls operating effectively"
    };

    TestingResult {
        company_id: request.company_id.clone(),
        fiscal_year: request.fiscal_year,
        total_controls: request.controls.len(),
        controls_tested: request.controls.len(),
        effective_controls: effective,
        deficient_controls: deficient,
        material_weaknesses: material,
        significant_deficiencies: significant,
        overall_assessment: assessment.to_string(),
        control_details: details,
        remediation_plan: remediation,
    }
}
