//! VAT return: output tax less input tax, with the sales analysis a return needs.

use serde::{Deserialize, Serialize};

use crate::money::money;

/// Standard rate used when a transaction does not state one.
pub const VAT_STANDARD_RATE: f64 = 0.15;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VatTransaction {
    pub description: String,
    pub amount: f64,
    pub vat_rate: f64,
    /// `standard`, `zero_rated`, `exempt`.
    pub transaction_type: String,
}

impl VatTransaction {
    pub fn new(description: &str, amount: f64, transaction_type: &str) -> VatTransaction {
        VatTransaction {
            description: description.to_string(),
            amount,
            vat_rate: VAT_STANDARD_RATE,
            transaction_type: transaction_type.to_string(),
        }
    }

    pub fn with_rate(mut self, rate: f64) -> VatTransaction {
        self.vat_rate = rate;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VatReturnRequest {
    pub company_id: String,
    pub tax_period: String,
    pub input_transactions: Vec<VatTransaction>,
    pub output_transactions: Vec<VatTransaction>,
}

impl VatReturnRequest {
    pub fn new(company_id: &str, tax_period: &str) -> VatReturnRequest {
        VatReturnRequest {
            company_id: company_id.to_string(),
            tax_period: tax_period.to_string(),
            input_transactions: Vec::new(),
            output_transactions: Vec::new(),
        }
    }

    pub fn with_outputs(mut self, transactions: Vec<VatTransaction>) -> VatReturnRequest {
        self.output_transactions = transactions;
        self
    }

    pub fn with_inputs(mut self, transactions: Vec<VatTransaction>) -> VatReturnRequest {
        self.input_transactions = transactions;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VatReturn {
    pub company_id: String,
    pub tax_period: String,
    pub total_output_vat: f64,
    pub total_input_vat: f64,
    pub net_vat_payable: f64,
    pub net_vat_refundable: f64,
    pub standard_rated_sales: f64,
    pub zero_rated_sales: f64,
    pub exempt_sales: f64,
    pub standard_rated_purchases: f64,
    pub vat_due_to_authority: f64,
}

/// Prepare a return.
///
/// Preserved quirk: VAT is charged at the line's rate whatever its type, so a
/// zero-rated line with the default rate still contributes output tax. The
/// service did this and the ZIMRA return it mirrors relies on the caller
/// setting the rate; the classification only drives the sales analysis.
pub fn prepare(request: &VatReturnRequest) -> VatReturn {
    let mut total_output_vat = 0.0;
    let mut total_input_vat = 0.0;
    let mut standard_sales = 0.0;
    let mut zero_sales = 0.0;
    let mut exempt_sales = 0.0;
    let mut standard_purchases = 0.0;

    for transaction in &request.output_transactions {
        total_output_vat += transaction.amount * transaction.vat_rate;
        match transaction.transaction_type.as_str() {
            "standard" => standard_sales += transaction.amount,
            "zero_rated" => zero_sales += transaction.amount,
            _ => exempt_sales += transaction.amount,
        }
    }
    for transaction in &request.input_transactions {
        total_input_vat += transaction.amount * transaction.vat_rate;
        if transaction.transaction_type == "standard" {
            standard_purchases += transaction.amount;
        }
    }

    let net = total_output_vat - total_input_vat;
    let payable = net.max(0.0);
    VatReturn {
        company_id: request.company_id.clone(),
        tax_period: request.tax_period.clone(),
        total_output_vat: money(total_output_vat),
        total_input_vat: money(total_input_vat),
        net_vat_payable: money(payable),
        net_vat_refundable: money((-net).max(0.0)),
        standard_rated_sales: money(standard_sales),
        zero_rated_sales: money(zero_sales),
        exempt_sales: money(exempt_sales),
        standard_rated_purchases: money(standard_purchases),
        vat_due_to_authority: money(payable),
    }
}
