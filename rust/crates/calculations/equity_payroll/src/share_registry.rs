//! authorized-share-capital-service + issued-share-capital-service.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareClassState {
    pub name: String,
    pub authorized_shares: i64,
    pub issued_shares: i64,
    pub par_value: f64,
    pub voting_rights: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIssuanceResult {
    pub share_class_id: String,
    pub number_of_shares: i64,
    pub issue_price: f64,
    pub total_proceeds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareBuybackResult {
    pub share_class_id: String,
    pub number_of_shares: i64,
    pub buyback_price: f64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassSummary {
    pub name: String,
    pub authorized: i64,
    pub issued: i64,
    pub available: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapitalSummary {
    pub total_classes: usize,
    pub total_authorized: i64,
    pub total_issued: i64,
    pub total_proceeds: f64,
    pub total_buyback_cost: f64,
    pub by_class: Vec<ClassSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLine {
    pub account_code: String,
    pub description: String,
    pub debit: f64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub description: String,
    pub entries: Vec<JournalLine>,
    pub reference: String,
}

pub fn new_share_class(
    name: &str,
    authorized_shares: i64,
    par_value: f64,
    voting_rights: &str,
) -> Result<ShareClassState, String> {
    if !matches!(voting_rights, "ordinary" | "preferential" | "none") {
        return Err(format!("Invalid voting rights: {voting_rights}"));
    }
    Ok(ShareClassState {
        name: name.to_string(),
        authorized_shares,
        issued_shares: 0,
        par_value,
        voting_rights: voting_rights.to_string(),
    })
}

pub fn issue_shares(
    class: &mut ShareClassState,
    number_of_shares: i64,
    issue_price: f64,
) -> Result<ShareIssuanceResult, String> {
    if class.issued_shares + number_of_shares > class.authorized_shares {
        return Err("Issuance exceeds authorized shares".to_string());
    }
    let total_proceeds = number_of_shares as f64 * issue_price;
    class.issued_shares += number_of_shares;
    Ok(ShareIssuanceResult {
        share_class_id: class.name.clone(),
        number_of_shares,
        issue_price,
        total_proceeds,
    })
}

pub fn buyback_shares(
    class: &mut ShareClassState,
    number_of_shares: i64,
    buyback_price: f64,
) -> Result<ShareBuybackResult, String> {
    if number_of_shares > class.issued_shares {
        return Err("Buyback exceeds issued shares".to_string());
    }
    class.issued_shares -= number_of_shares;
    Ok(ShareBuybackResult {
        share_class_id: class.name.clone(),
        number_of_shares,
        buyback_price,
        total_cost: number_of_shares as f64 * buyback_price,
    })
}

pub fn capital_summary(
    classes: &[ShareClassState],
    issuances: &[ShareIssuanceResult],
    buybacks: &[ShareBuybackResult],
) -> CapitalSummary {
    CapitalSummary {
        total_classes: classes.len(),
        total_authorized: classes.iter().map(|c| c.authorized_shares).sum(),
        total_issued: classes.iter().map(|c| c.issued_shares).sum(),
        total_proceeds: issuances.iter().map(|i| i.total_proceeds).sum(),
        total_buyback_cost: buybacks.iter().map(|b| b.total_cost).sum(),
        by_class: classes
            .iter()
            .map(|c| ClassSummary {
                name: c.name.clone(),
                authorized: c.authorized_shares,
                issued: c.issued_shares,
                available: c.authorized_shares - c.issued_shares,
            })
            .collect(),
    }
}

/// issued-share-capital-service `/issue`. PYTHON QUIRK: nominal value is
/// hard-coded at 1.00 per share, so premium absorbs the rest of the price.
pub fn issue_with_journal(
    company_id: &str,
    share_class: &str,
    shares_issued: i64,
    issue_price: f64,
) -> (ShareIssuanceResult, Journal) {
    let total_proceeds = shares_issued as f64 * issue_price;
    let nominal = shares_issued as f64;
    let journal = Journal {
        description: format!("Issue of {shares_issued} {share_class} shares at {issue_price}"),
        entries: vec![
            JournalLine {
                account_code: "1000".into(),
                description: "Bank".into(),
                debit: total_proceeds,
                credit: 0.0,
            },
            JournalLine {
                account_code: "3200".into(),
                description: "Share Capital".into(),
                debit: 0.0,
                credit: nominal,
            },
            JournalLine {
                account_code: "3210".into(),
                description: "Share Premium".into(),
                debit: 0.0,
                credit: total_proceeds - nominal,
            },
        ],
        reference: format!("ISSUE-{company_id}-{share_class}"),
    };
    (
        ShareIssuanceResult {
            share_class_id: share_class.to_string(),
            number_of_shares: shares_issued,
            issue_price,
            total_proceeds,
        },
        journal,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_voting_rights() {
        assert!(new_share_class("B", 1000, 1.0, "deferred").is_err());
        assert!(new_share_class("B", 1000, 1.0, "none").is_ok());
    }

    #[test]
    fn issuance_capped_by_authorized() {
        let mut c = new_share_class("Ordinary", 1000, 1.0, "ordinary").unwrap();
        assert!(issue_shares(&mut c, 1000, 2.5).is_ok());
        assert!(issue_shares(&mut c, 1, 2.5).is_err());
        assert_eq!(c.issued_shares, 1000);
    }

    #[test]
    fn buyback_reduces_issued_and_costs() {
        let mut c = new_share_class("Ordinary", 1000, 1.0, "ordinary").unwrap();
        issue_shares(&mut c, 400, 3.0).unwrap();
        let b = buyback_shares(&mut c, 150, 4.0).unwrap();
        assert_eq!(b.total_cost, 600.0);
        assert_eq!(c.issued_shares, 250);
        assert!(buyback_shares(&mut c, 251, 4.0).is_err());
    }

    #[test]
    fn summary_totals_and_available() {
        let mut a = new_share_class("A", 1000, 1.0, "ordinary").unwrap();
        let mut b = new_share_class("B", 500, 2.0, "preferential").unwrap();
        let i1 = issue_shares(&mut a, 600, 2.0).unwrap();
        let i2 = issue_shares(&mut b, 100, 3.0).unwrap();
        let s = capital_summary(&[a, b], &[i1, i2], &[]);
        assert_eq!(s.total_classes, 2);
        assert_eq!(s.total_authorized, 1500);
        assert_eq!(s.total_issued, 700);
        assert_eq!(s.total_proceeds, 1500.0);
        assert_eq!(s.by_class[0].available, 400);
    }

    #[test]
    fn journal_hardcodes_one_unit_nominal() {
        let (r, j) = issue_with_journal("c1", "ordinary", 100, 2.5);
        assert_eq!(r.total_proceeds, 250.0);
        assert_eq!(j.entries[1].credit, 100.0);
        assert_eq!(j.entries[2].credit, 150.0);
        let d: f64 = j.entries.iter().map(|e| e.debit).sum();
        let c: f64 = j.entries.iter().map(|e| e.credit).sum();
        assert_eq!(d, c);
    }
}
