#![forbid(unsafe_code)]
//! family-community-group-service: savings groups, their members, contributions and the pool.
//!
//! A group carries the frequency it meets at, the amount a member puts in, the cycle it is on and the pool it has
//! collected. Members hang off the group; contributions hang off the group and name the member who paid; payouts are
//! read as they were scheduled. Every record goes through the store, so it is scoped to the caller and their Book.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a new group starts at cycle 1, a pool of nothing, no members, and active - whatever the caller sent;
//! * the frequency is one of weekly, biweekly, monthly and anything else is refused by name;
//! * a member added without an amount of their own takes the group's, and the group's member count is *recounted*
//!   from the members it holds rather than incremented, so a repeated add cannot inflate it;
//! * a contribution is only accepted for a member the group actually holds, and it carries the cycle the group is
//!   on at the time, not one the caller claims;
//! * the pool rises by exactly the amount contributed;
//! * advancing a cycle moves it on by one and answers the cycle the group is now on;
//! * a group the caller does not hold is a miss, and so is a member from another group.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, maybe_flag, money_or_nothing, or_default, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, COMMUNITY_GROUP, GROUP_CONTRIBUTION, GROUP_MEMBER,
    GROUP_PAYOUT,
};

use crate::figures::group_pool_after;

/// The frequencies a group may meet at, in the order the service listed them.
const FREQUENCIES: [&str; 3] = ["weekly", "biweekly", "monthly"];

fn group_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, COMMUNITY_GROUP).get(scope, group_id)
}

/// `POST /groups`.
pub fn create_group(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let frequency = or_default(payload, "contribution_frequency", "monthly");
    if !FREQUENCIES.contains(&frequency.as_str()) {
        return Err(refused(&format!(
            "Invalid frequency. Must be one of {FREQUENCIES:?}"
        )));
    }
    let amount = money_or_nothing(payload, "contribution_amount")?;

    let recorded = json!({
        "name": name,
        "description": text(payload, "description"),
        "contribution_frequency": frequency,
        "contribution_amount": wire(&amount),
        // the four the register owns, not the caller
        "member_count": 0,
        "current_cycle": 1,
        "total_pool": wire(&Money::zero()),
        "status": "active",
    });
    remember(graph, scope, COMMUNITY_GROUP, recorded)
}

/// `GET /groups`.
pub fn list_groups(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = maybe_flag(payload, "status")
        .is_none()
        .then(|| text(payload, "status"));
    let rows: Vec<Value> = Records::new(graph, COMMUNITY_GROUP)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| match &wanted {
            Some(status) if !status.is_empty() => {
                row.get("status").and_then(Value::as_str) == Some(status.as_str())
            }
            _ => true,
        })
        .collect();
    Ok(json!({ "groups": rows, "count": rows.len() }))
}

/// `GET /groups/{group_id}`.
pub fn get_group(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    group_of(graph, scope, group_id)
}

/// `POST /groups/{group_id}/members`.
pub fn add_member(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let group = group_of(graph, scope, group_id)?;
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }

    // a member who brings no amount of their own is on the group's
    let named = money_or_nothing(payload, "contribution_amount")?;
    let amount = if named.is_zero() {
        Money::parse(&decimal(&group, "contribution_amount", "0"))
            .map_err(|_| refused("the group's contribution amount is not a number"))?
    } else {
        named
    };

    let member = remember(
        graph,
        scope,
        GROUP_MEMBER,
        json!({
            "group_id": group_id,
            "name": name,
            "email": text(payload, "email"),
            "phone": text(payload, "phone"),
            "contribution_amount": wire(&amount),
        }),
    )?;

    recount_members(graph, scope, group_id)?;
    Ok(member)
}

/// The group's member count, taken from the members it holds rather than added to.
fn recount_members(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<u64, StoreError> {
    let held = members_of(graph, scope, group_id)?.len() as u64;
    Records::new(graph, COMMUNITY_GROUP).update(
        scope,
        group_id,
        &json!({ "member_count": held }),
    )?;
    Ok(held)
}

fn members_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(Records::new(graph, GROUP_MEMBER)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| row.get("group_id").and_then(Value::as_str) == Some(group_id))
        .collect())
}

/// `GET /groups/{group_id}/members`.
pub fn list_members(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    let members = members_of(graph, scope, group_id)?;
    Ok(json!({ "members": members, "count": members.len() }))
}

/// `POST /groups/{group_id}/contribute`.
pub fn contribute(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let group = group_of(graph, scope, group_id)?;
    let member_id = text(payload, "member_id");
    let held = members_of(graph, scope, group_id)?;
    let member = held
        .iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(member_id.as_str()))
        .cloned()
        .ok_or_else(|| refused("Member not found in this group"))?;
    let _ = &member;

    let amount = money_or_nothing(payload, "amount")?;
    let cycle = group
        .get("current_cycle")
        .and_then(Value::as_i64)
        .unwrap_or(1);

    let contribution = remember(
        graph,
        scope,
        GROUP_CONTRIBUTION,
        json!({
            "group_id": group_id,
            "member_id": member_id,
            "amount": wire(&amount),
            "cycle_number": cycle,
            "notes": text(payload, "notes"),
        }),
    )?;

    // the pool is the pool plus this amount, worked out from the record rather than from the caller
    let pool = group_pool_after(&group, &amount)?;
    Records::new(graph, COMMUNITY_GROUP).update(
        scope,
        group_id,
        &json!({ "total_pool": wire(&pool) }),
    )?;
    Ok(contribution)
}

/// `GET /groups/{group_id}/contributions`, narrowed to a cycle when one is asked for.
pub fn list_contributions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = payload.get("cycle").and_then(Value::as_i64);
    let rows: Vec<Value> = Records::new(graph, GROUP_CONTRIBUTION)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| row.get("group_id").and_then(Value::as_str) == Some(group_id))
        .filter(|row| match wanted {
            Some(cycle) => row.get("cycle_number").and_then(Value::as_i64) == Some(cycle),
            None => true,
        })
        .collect();
    Ok(json!({ "contributions": rows, "count": rows.len() }))
}

/// `POST /groups/{group_id}/advance-cycle`.
pub fn advance_cycle(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    let group = group_of(graph, scope, group_id)?;
    let current = group
        .get("current_cycle")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let next = current + 1;
    Records::new(graph, COMMUNITY_GROUP).update(
        scope,
        group_id,
        &json!({ "current_cycle": next }),
    )?;
    Ok(json!({ "group_id": group_id, "current_cycle": next }))
}

/// `GET /groups/{group_id}/payouts`.
pub fn list_payouts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    group_id: &str,
) -> Result<Value, StoreError> {
    let rows: Vec<Value> = Records::new(graph, GROUP_PAYOUT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| row.get("group_id").and_then(Value::as_str) == Some(group_id))
        .collect();
    Ok(json!({ "payouts": rows, "count": rows.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// The fields the store was handed, for the statement the test cares about: a create carries
    /// them as props, an update as the patch it merges in.
    fn props(graph: &FakeGraph, index: usize) -> Value {
        let params = graph.statement(index).1;
        if params.get("props").is_some() {
            params["props"].clone()
        } else {
            params["patch"].clone()
        }
    }

    #[test]
    fn a_new_group_starts_empty_and_on_its_first_cycle() {
        let graph = FakeGraph::returning(vec![json!({"id": "g-1"})]);
        let answer = create_group(
            &graph,
            &scope(),
            &json!({
                "name": "Chikafu family",
                "contribution_frequency": "weekly",
                "contribution_amount": 25,
                "member_count": 9,
                "current_cycle": 4,
                "total_pool": 700,
            }),
        )
        .expect("created");

        // the register's own four, whatever the caller claimed
        let written = props(&graph, 0);
        assert_eq!(written["member_count"], json!(0));
        assert_eq!(written["current_cycle"], json!(1));
        assert_eq!(written["total_pool"], json!("0.000000"));
        assert_eq!(written["status"], json!("active"));
        assert_eq!(written["contribution_amount"], json!("25.000000"));
        assert_eq!(answer["id"], json!("g-1"));

        let (cypher, params) = graph.statement(0);
        assert!(cypher.contains("CommunityGroup"), "{cypher}");
        assert!(cypher.contains("OWNS_GROUP"), "{cypher}");
        assert_eq!(params["book_id"], json!("book-1"));
    }

    #[test]
    fn a_frequency_the_group_does_not_meet_at_is_refused_before_anything_is_written() {
        let graph = FakeGraph::returning(vec![]);
        let refused = create_group(
            &graph,
            &scope(),
            &json!({"name": "Weekly", "contribution_frequency": "daily"}),
        );
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("Invalid frequency"));
        assert_eq!(graph.statements(), 0);
    }

    #[test]
    fn a_member_without_an_amount_takes_the_groups_and_the_count_is_recounted() {
        // a group on 40 a head, then: the member is written, the members are listed, the count is set
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "g-1", "contribution_amount": "40.000000"})],
            vec![json!({"id": "m-1"})],
            vec![
                json!({"id": "m-1", "group_id": "g-1", "contribution_amount": "40.000000"}),
                json!({"id": "m-2", "group_id": "g-1", "contribution_amount": "15.000000"}),
            ],
            vec![json!({"id": "g-1", "member_count": 2})],
        ]);

        let member =
            add_member(&graph, &scope(), "g-1", &json!({"name": "Tendai"})).expect("added");
        assert_eq!(props(&graph, 1)["contribution_amount"], json!("40.000000"));
        assert_eq!(member["id"], json!("m-1"));

        // the count comes from the two members the group holds, not from the one just added
        assert_eq!(props(&graph, 3)["member_count"], json!(2));
        assert_eq!(graph.statements(), 4);
    }

    #[test]
    fn a_contribution_raises_the_pool_and_carries_the_cycle_the_group_is_on() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "g-1", "current_cycle": 2, "total_pool": "100.000000"})],
            vec![json!({"id": "m-1", "group_id": "g-1"})],
            vec![json!({"id": "c-1", "amount": "12.500000", "cycle_number": 2})],
            vec![json!({"id": "g-1", "total_pool": "112.500000"})],
        ]);

        // the caller claiming cycle 99 does not get it: the group's cycle is the one that is recorded
        let paid = contribute(
            &graph,
            &scope(),
            "g-1",
            &json!({"member_id": "m-1", "amount": 12.5, "cycle_number": 99}),
        )
        .expect("contributed");

        assert_eq!(props(&graph, 2)["cycle_number"], json!(2));
        assert_eq!(paid["amount"], json!("12.500000"));
        // 100 held plus 12.50 paid in
        assert_eq!(props(&graph, 3)["total_pool"], json!("112.500000"));
    }

    #[test]
    fn a_contribution_from_a_member_the_group_does_not_hold_is_refused() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "g-1", "current_cycle": 1})],
            vec![json!({"id": "m-1", "group_id": "g-1"})],
        ]);
        let refused = contribute(
            &graph,
            &scope(),
            "g-1",
            &json!({"member_id": "ghost", "amount": 5}),
        );
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("Member not found in this group"));
        // nothing was written
        assert_eq!(graph.statements(), 2);
    }

    #[test]
    fn advancing_a_cycle_moves_it_on_by_one_and_answers_where_it_landed() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "g-1", "current_cycle": 3})],
            vec![json!({"id": "g-1", "current_cycle": 4})],
        ]);
        let answer = advance_cycle(&graph, &scope(), "g-1").expect("advanced");
        assert_eq!(props(&graph, 1)["current_cycle"], json!(4));
        assert_eq!(answer, json!({"group_id": "g-1", "current_cycle": 4}));
    }

    #[test]
    fn a_group_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(get_group(&graph, &scope(), "nope").is_err());
    }

    #[test]
    fn a_listing_narrows_to_the_status_asked_for() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "g-1", "status": "active"}),
            json!({"id": "g-2", "status": "closed"}),
        ]);
        let answer = list_groups(&graph, &scope(), &json!({"status": "closed"})).expect("listed");
        assert_eq!(answer["count"], json!(1));
        assert_eq!(answer["groups"][0]["id"], json!("g-2"));
    }

    #[test]
    fn contributions_narrow_to_a_cycle_when_one_is_asked_for() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "c-1", "group_id": "g-1", "cycle_number": 1}),
            json!({"id": "c-2", "group_id": "g-1", "cycle_number": 2}),
            json!({"id": "c-3", "group_id": "g-2", "cycle_number": 1}),
        ]);
        let answer =
            list_contributions(&graph, &scope(), "g-1", &json!({"cycle": 2})).expect("listed");
        assert_eq!(answer["count"], json!(1));
        assert_eq!(answer["contributions"][0]["id"], json!("c-2"));
    }

    #[test]
    fn the_pool_is_the_pool_plus_what_was_paid_in() {
        // the shared figure, on its own: an exact sum, not a float one
        let held = json!({"total_pool": "0.10"});
        let paid = Money::parse("0.20").expect("an amount");
        assert_eq!(
            group_pool_after(&held, &paid).expect("summed").to_fixed(6),
            "0.300000"
        );
    }
}
