//! The part of what customers owe that the business does not expect to see.
//!
//! Writing a debt off is a decision about one customer, made when the business stops expecting anything from them. A
//! provision is the same worry said in advance and about all of them at once: some of what is owed will not come, the
//! business does not yet know which, and a year that reports the whole of what is owed as though it were coming reports a
//! profit it may not have made.
//!
//! So a provision is an estimate that stands until it is changed. It is not a write-off: no customer's debt leaves the
//! books, nothing is said about any one of them, and the money is still owed. It is an allowance carried against what is
//! owed, and what moves is only the difference between the estimate the business wants to stand at and the one already
//! standing:
//!
//! * raising it - Dr Doubtful Debts, Cr Allowance for Doubtful Debts: the month carries the extra doubt, and the allowance
//!   against receivables grows;
//! * lowering it - Dr Allowance, Cr Doubtful Debts: the business now expects more of what it is owed, so the charge it took
//!   earlier comes back to the same account it was taken to, and the account says what doubt has cost the business over
//!   time rather than hiding a release in a corner of sales.
//!
//! What this keeps, and why:
//!
//! * the person states the provision they want to stand at, in money, because the estimate is theirs: how much of what is
//!   owed will not come is a judgement about customers, and an engine that guessed a percentage would be inventing the
//!   judgement and putting a figure in the books nobody can defend;
//! * only the movement is posted. A provision is not re-posted each time it is looked at, so the accounts show one running
//!   allowance and one running charge rather than a pile of figures that have to be added up to mean anything;
//! * a provision already standing at the figure asked for is refused: an adjustment that changes nothing is a mistake, not
//!   a posting;
//! * one adjustment per day, so a second press cannot charge the month twice;
//! * a closed month is not written into: its profit has been reported;
//! * the accounts are found by what the chart calls them and by their type, never by number;
//! * nothing moves money. A provision is a statement of doubt, and no customer is chased or forgiven by it.
//!
//! The receivables domain already keeps a register of doubtful debtors and works out what a provision should be from it,
//! recording the answer as a provision with what it opened at, what was required and what the adjustment was. What it never
//! did was reach the books: a calculation is not a posting. This journal is that posting, and it is named apart from the
//! estimate rather than standing in for it. Having the register's own figure arrive here in one act, instead of a person
//! carrying it across, is the obvious next step and is not done here.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, DOUBTFUL_DEBT_PROVISION_JOURNAL};

use crate::ledger_book;
use crate::periods;
use crate::subsidiary_journals::{account_named, must_be_open, positive, post};

/// The day the estimate was made, and the month it belongs to.
fn day_of(payload: &Value) -> Result<String, StoreError> {
    let stated = text(payload, "entry_date").trim().to_string();
    if stated.is_empty() {
        return Err(refused(
            "Say the day the provision was decided, as 2026-11-20. The month follows from it.",
        ));
    }
    if periods::month_of(&stated).is_none() {
        return Err(refused(&format!(
            "{stated} does not read as a day. Say it as 2026-11-20."
        )));
    }
    Ok(stated)
}

/// What the allowance stands at now, as the ledger carries it.
fn standing(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    allowance: &str,
) -> Result<Money, StoreError> {
    let chart = ledger_book::accounts(graph, scope)?;
    let account = chart
        .iter()
        .find(|row| field(row, "account_number") == allowance)
        .ok_or_else(|| refused("The allowance account is not in this Book's chart."))?;
    ledger_book::account_balance(graph, scope, account, None)
}

/// `POST /doubtful-debts`: state what the provision should stand at, and post only the movement.
pub fn provide(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = day_of(payload)?;
    must_be_open(graph, scope, &entry_date)?;
    if text(payload, "provision").trim().is_empty() {
        return Err(refused(
            "Say what the provision should stand at in money, as 450.00: what you expect not to be paid, not what is owed.",
        ));
    }
    let wanted = positive(payload, "provision", "The provision")?;
    let allowance = account_named(
        graph,
        scope,
        payload,
        "allowance_account",
        "allowance for doubtful",
        "asset",
    )?;
    let charge = account_named(
        graph,
        scope,
        payload,
        "provision_account",
        "doubtful debts",
        "expense",
    )?;
    if allowance == charge {
        return Err(refused(
            "The allowance and the charge cannot be the same account: one stands against what customers owe and the other is what the doubt has cost.",
        ));
    }
    let already = standing(graph, scope, &allowance)?;
    let movement = wanted.minus(&already);
    if movement.is_zero() {
        return Err(refused(&format!(
            "The provision already stands at {already}, which is the figure asked for, so there is nothing to adjust. An \
             adjustment has to change the estimate."
        )));
    }
    // raising the estimate charges the month and grows the allowance; lowering it takes the earlier charge back
    let raising = !movement.is_negative();
    let figure = if raising {
        movement.clone()
    } else {
        Money::zero().minus(&movement)
    };
    let (debit, credit) = if raising {
        (&charge, &allowance)
    } else {
        (&allowance, &charge)
    };
    let description = if raising {
        format!("Provision for doubtful debts raised to {wanted}")
    } else {
        format!("Provision for doubtful debts reduced to {wanted}")
    };
    let note = match text(payload, "reason").trim() {
        "" => format!("Provision now {wanted}, was {already}"),
        why => format!("Provision now {wanted}, was {already}: {why}"),
    };
    // one adjustment per day: the ledger refuses a reference it has already posted, so a second press cannot charge the
    // month twice
    let reference = format!("PROV-{entry_date}");
    let dated = json!({"entry_date": entry_date, "created_at": text(payload, "created_at")});
    let entry_id = post(
        graph,
        scope,
        &dated,
        description,
        &reference,
        "DoubtfulDebts",
        debit,
        credit,
        &figure,
        &note,
    )?;
    remember(
        graph,
        scope,
        DOUBTFUL_DEBT_PROVISION_JOURNAL,
        json!({
            "entry_date": entry_date,
            "provision": wire(&wanted),
            "previous": wire(&already),
            "movement": wire(&movement),
            "direction": if raising { "raised" } else { "reduced" },
            "allowance_account": allowance,
            "provision_account": charge,
            "reason": text(payload, "reason"),
            "journal_entry_id": entry_id,
            "notes": text(payload, "notes"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /doubtful-debts`: the ladder of estimates this Book has made, newest first, with what stands today.
pub fn list_provisions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let period = text(query, "period");
    let allowance = account_named(
        graph,
        scope,
        query,
        "allowance_account",
        "allowance for doubtful",
        "asset",
    )?;
    let standing_now = standing(graph, scope, &allowance)?;
    let mut rows: Vec<Value> = Records::new(graph, DOUBTFUL_DEBT_PROVISION_JOURNAL)
        .list(scope, 5000)?
        .into_iter()
        .filter(|row| {
            period.trim().is_empty()
                || periods::month_of(field(row, "entry_date")).as_deref() == Some(period.trim())
        })
        .collect();
    rows.sort_by(|a, b| {
        field(b, "entry_date")
            .cmp(field(a, "entry_date"))
            .then_with(|| field(b, "created_at").cmp(field(a, "created_at")))
    });
    // what the allowance stands at today is the answer a reader wants first, so it leads rather than being left to be
    // worked out from the ladder
    Ok(json!({
        "allowance_account": allowance,
        "standing": wire(&standing_now),
        "adjustments": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::MemoryGraph;
    use vimbai_store::read::money;

    fn business() -> (MemoryGraph, BookScope) {
        let graph = MemoryGraph::new();
        let scope = BookScope::new("user-1", "book-1");
        crate::chart::seed_opening_chart(&graph, &scope, "business").expect("a chart");
        (graph, scope)
    }

    fn amount(text: &str) -> Money {
        Money::parse(text).expect("an amount")
    }

    fn provision(day: &str, figure: &str) -> Value {
        json!({"entry_date": day, "provision": figure, "reason": "Two customers have gone quiet"})
    }

    fn line_on<'a>(lines: &'a [Value], account: &str) -> &'a Value {
        lines
            .iter()
            .find(|one| field(one, "account_number") == account)
            .unwrap_or_else(|| panic!("no line on {account}"))
    }

    #[test]
    fn a_first_provision_charges_the_month_and_stands_against_what_is_owed() {
        let (graph, scope) = business();
        let written =
            provide(&graph, &scope, &provision("2026-11-20", "450.00")).expect("a provision");

        assert_eq!(field(&written, "provision"), "450.000000");
        assert_eq!(field(&written, "previous"), "0.000000");
        assert_eq!(field(&written, "movement"), "450.000000");
        assert_eq!(field(&written, "direction"), "raised");

        let entry =
            crate::ledger_book::get_entry(&graph, &scope, field(&written, "journal_entry_id"))
                .expect("the entry");
        let lines = entry["lines"].as_array().expect("lines");
        assert_eq!(
            money(line_on(lines, "6510"), "debit").expect("a debit"),
            amount("450")
        );
        assert_eq!(
            money(line_on(lines, "1120"), "credit").expect("a credit"),
            amount("450")
        );
        assert_eq!(field(&entry, "source_module"), "DoubtfulDebts");
    }

    #[test]
    fn raising_the_estimate_posts_only_the_difference() {
        let (graph, scope) = business();
        provide(&graph, &scope, &provision("2026-11-20", "450.00")).expect("a provision");
        let raised =
            provide(&graph, &scope, &provision("2026-12-18", "700.00")).expect("raised again");

        // the movement is the difference, not the new figure: a provision is not re-posted every time it is looked at
        assert_eq!(field(&raised, "previous"), "450.000000");
        assert_eq!(field(&raised, "movement"), "250.000000");
        assert_eq!(field(&raised, "direction"), "raised");
    }

    #[test]
    fn lowering_the_estimate_takes_the_earlier_charge_back_to_the_same_account() {
        let (graph, scope) = business();
        provide(&graph, &scope, &provision("2026-11-20", "450.00")).expect("a provision");
        let lowered = provide(&graph, &scope, &provision("2026-12-18", "200.00")).expect("lowered");

        assert_eq!(field(&lowered, "movement"), "-250.000000");
        assert_eq!(field(&lowered, "direction"), "reduced");

        let entry =
            crate::ledger_book::get_entry(&graph, &scope, field(&lowered, "journal_entry_id"))
                .expect("the entry");
        let lines = entry["lines"].as_array().expect("lines");
        // the allowance comes down by a debit and the cost comes back by a credit, to the account it was taken to
        assert_eq!(
            money(line_on(lines, "1120"), "debit").expect("a debit"),
            amount("250")
        );
        assert_eq!(
            money(line_on(lines, "6510"), "credit").expect("a credit"),
            amount("250")
        );
    }

    #[test]
    fn a_figure_already_standing_is_refused_because_an_adjustment_has_to_change_something() {
        let (graph, scope) = business();
        provide(&graph, &scope, &provision("2026-11-20", "450.00")).expect("a provision");

        let said = provide(&graph, &scope, &provision("2026-12-18", "450.00"))
            .expect_err("nothing to adjust")
            .to_string();
        assert!(said.contains("nothing to adjust"), "{said}");
    }

    #[test]
    fn the_month_cannot_be_charged_twice_on_the_same_day() {
        let (graph, scope) = business();
        provide(&graph, &scope, &provision("2026-11-20", "450.00")).expect("a provision");

        let again = provide(&graph, &scope, &provision("2026-11-20", "500.00"))
            .expect_err("a second adjustment on the day")
            .to_string();
        assert!(again.to_lowercase().contains("already"), "{again}");
        assert_eq!(
            Records::new(&graph, DOUBTFUL_DEBT_PROVISION_JOURNAL)
                .list(&scope, 10)
                .expect("the records")
                .len(),
            1
        );
    }

    #[test]
    fn a_provision_has_to_say_what_it_should_stand_at_and_when() {
        let (graph, scope) = business();
        let no_figure = json!({"entry_date": "2026-11-20"});
        let said = provide(&graph, &scope, &no_figure)
            .expect_err("no figure")
            .to_string();
        assert!(
            said.contains("what the provision should stand at"),
            "{said}"
        );

        let no_day = json!({"provision": "450.00"});
        let said = provide(&graph, &scope, &no_day)
            .expect_err("no day")
            .to_string();
        assert!(said.contains("the day the provision was decided"), "{said}");

        let nothing = json!({"entry_date": "2026-11-20", "provision": "0"});
        let said = provide(&graph, &scope, &nothing)
            .expect_err("nothing")
            .to_string();
        assert!(said.contains("more than nothing"), "{said}");
    }

    #[test]
    fn a_closed_month_is_not_written_into() {
        let (graph, scope) = business();
        crate::periods::close_period(
            &graph,
            &scope,
            &json!({"period": "2026-11", "closes_on": "2026-11-30"}),
        )
        .expect("closed");

        let said = provide(&graph, &scope, &provision("2026-11-20", "450.00"))
            .expect_err("closed month")
            .to_string();
        assert!(said.to_lowercase().contains("closed"), "{said}");
    }

    #[test]
    fn the_ladder_of_estimates_is_listed_with_what_stands_today_leading() {
        let (graph, scope) = business();
        provide(&graph, &scope, &provision("2026-11-20", "450.00")).expect("a provision");
        provide(&graph, &scope, &provision("2026-12-18", "700.00")).expect("raised again");

        let listed = list_provisions(&graph, &scope, &json!({})).expect("a list");
        assert_eq!(field(&listed, "allowance_account"), "1120");
        // what stands today is the answer a reader wants first, rather than something to work out from the ladder
        assert_eq!(field(&listed, "standing"), "700.000000");
        let rows = listed["adjustments"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(field(&rows[0], "entry_date"), "2026-12-18");
        assert_eq!(field(&rows[0], "movement"), "250.000000");
        assert_eq!(field(&rows[1], "entry_date"), "2026-11-20");

        let december =
            list_provisions(&graph, &scope, &json!({"period": "2026-12"})).expect("a list");
        assert_eq!(december["adjustments"].as_array().expect("rows").len(), 1);
    }
}
