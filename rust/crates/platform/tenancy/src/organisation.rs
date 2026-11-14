//! Organisations and the people in them.
//!
//! An organisation is a tenant: it owns Books, and a Book belongs to exactly one. It is a PLATFORM record rather than a
//! Book record, and for a reason the store already knows: it has to be found before a Book is known, and it is the same
//! organisation in every Book it owns. Scoping it to a Book would mean an organisation per Book.
//!
//! Membership is a record of its own rather than a field on the account, because the same person can belong to several
//! organisations with a different role in each, and because a field on the account has nowhere to say what happens when
//! somebody is removed.
#![forbid(unsafe_code)]

use serde_json::{json, Map, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{refused, text};
use vimbai_store::shared::SharedRecords;
use vimbai_store::{Graph, PlatformRecords, RecordKind, StoreError};

/// The organisation record. Kept at the published label, which the identity surface already wrote.
pub const ORGANIZATION: RecordKind = RecordKind::new("Organization", "OWNS_ORGANIZATION");
/// One person's place in an organisation.
pub const ORGANIZATION_MEMBERSHIP: RecordKind =
    RecordKind::new("OrganizationMembership", "OWNS_ORGANIZATION_MEMBERSHIP");

/// The most privileged role: may add members and move Books.
pub const OWNER: &str = "owner";
/// May do what an owner may, except remove the organisation.
pub const ADMIN: &str = "admin";
/// May read and write inside the organisation's Books.
pub const MEMBER: &str = "member";

/// Every role an organisation recognises. A role outside this list is refused rather than stored and ignored.
pub const ROLES: [&str; 3] = [OWNER, ADMIN, MEMBER];

/// The roles that may change the organisation itself.
pub const ADMINISTRATORS: [&str; 2] = [OWNER, ADMIN];

/// An active membership.
pub const ACTIVE: &str = "active";

fn held(
    graph: &(impl Graph + ?Sized),
    kind: RecordKind,
) -> PlatformRecords<'_, impl Graph + ?Sized> {
    PlatformRecords::new(graph, kind)
}

fn books(graph: &(impl Graph + ?Sized)) -> SharedRecords<'_, impl Graph + ?Sized> {
    SharedRecords::new(graph, vimbai_service_offline_sync::sync::BOOK)
}

/// The key a membership is stored under.
///
/// A membership is looked up by one person in one organisation on almost every request, so the pair is the key rather
/// than two properties a query has to intersect. It is also what makes "add this person twice" an update instead of a
/// second row.
fn key(organisation_id: &str, user_id: &str) -> String {
    format!("{organisation_id}:{user_id}")
}

fn stamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// A resource segment from a name: lower case, digits and single dashes.
pub fn slug_of(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "organisation".to_string()
    } else {
        trimmed
    }
}

/// The first slug not already taken, so two organisations may share a name.
fn free_slug(graph: &(impl Graph + ?Sized), name: &str) -> Result<String, StoreError> {
    let base = slug_of(name);
    let existing = held(graph, ORGANIZATION).list(2_000)?;
    let taken: Vec<String> = existing.iter().map(|row| text(row, "slug")).collect();
    if !taken.iter().any(|held| held == &base) {
        return Ok(base);
    }
    for suffix in 2..500 {
        let candidate = format!("{base}-{suffix}");
        if !taken.iter().any(|held| held == &candidate) {
            return Ok(candidate);
        }
    }
    Err(refused("too many organisations share this name"))
}

/// Record a person's place in an organisation, or update the role of a place they already hold.
pub fn grant(
    graph: &(impl Graph + ?Sized),
    organisation_id: &str,
    user_id: &str,
    role: &str,
) -> Result<Value, StoreError> {
    if !ROLES.contains(&role) {
        return Err(refused(&format!(
            "role must be one of {}, not {role}",
            ROLES.join(", ")
        )));
    }
    let id = key(organisation_id, user_id);
    let fields = json!({
        "id": id,
        "organisation_id": organisation_id,
        "member_user_id": user_id,
        "role": role,
        "status": ACTIVE,
        "updated_at": stamp(),
    });
    match held(graph, ORGANIZATION_MEMBERSHIP).get(&id)? {
        Some(_) => held(graph, ORGANIZATION_MEMBERSHIP)
            .update(
                &id,
                &json!({ "role": role, "status": ACTIVE, "updated_at": stamp() }),
            )?
            .ok_or(StoreError::NotFound),
        None => {
            let mut created = fields;
            if let Some(object) = created.as_object_mut() {
                object.insert("created_at".to_string(), json!(stamp()));
            }
            held(graph, ORGANIZATION_MEMBERSHIP).create(&created)
        }
    }
}

/// The caller's membership of an organisation, whatever its state.
pub fn membership_for(
    graph: &(impl Graph + ?Sized),
    organisation_id: &str,
    user_id: &str,
) -> Result<Option<Value>, StoreError> {
    held(graph, ORGANIZATION_MEMBERSHIP).get(&key(organisation_id, user_id))
}

/// The caller's ACTIVE membership, or the refusal that says which of the two things went wrong.
///
/// A non-member is told the organisation is not there. Telling a stranger that an organisation exists and is not theirs
/// is a directory of other people's tenants, and it is the same answer the identity surface already gives.
pub fn active_membership(
    graph: &(impl Graph + ?Sized),
    organisation_id: &str,
    user_id: &str,
) -> Result<Value, StoreError> {
    match membership_for(graph, organisation_id, user_id)? {
        Some(membership) if text(&membership, "status") == ACTIVE => Ok(membership),
        _ => Err(StoreError::NotFound),
    }
}

/// The caller's active membership, when the role has to permit the action as well.
pub fn require_administrator(
    graph: &(impl Graph + ?Sized),
    organisation_id: &str,
    user_id: &str,
) -> Result<Value, StoreError> {
    let membership = active_membership(graph, organisation_id, user_id)?;
    if !ADMINISTRATORS.contains(&text(&membership, "role").as_str()) {
        return Err(StoreError::Forbidden(
            "Only an owner or an admin may change this organisation".to_string(),
        ));
    }
    Ok(membership)
}

/// Open an organisation, with the person who opened it as its owner.
///
/// This is the first thing a new account does and it happens with no prior context: no Book, no organisation, nothing to
/// belong to yet. So it takes a person and a name, and creates the membership that makes every later question
/// answerable.
pub fn create(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    sent: &Map<String, Value>,
) -> Result<Value, StoreError> {
    let name = text(&Value::Object(sent.clone()), "name")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err(refused("an organisation needs a name"));
    }
    let id = match text(&Value::Object(sent.clone()), "id") {
        given if given.trim().is_empty() => format!("org-{}", service_id()),
        given => given,
    };
    let slug = free_slug(graph, &name)?;
    let record = json!({
        "id": id,
        "name": name,
        "slug": slug,
        "description": text(&Value::Object(sent.clone()), "description"),
        "settings": sent.get("settings").cloned().unwrap_or_else(|| json!({})),
        "status": ACTIVE,
        "created_by": user_id,
        "created_at": stamp(),
        "updated_at": stamp(),
    });
    held(graph, ORGANIZATION).create(&record)?;
    let membership = grant(graph, &id, user_id, OWNER)?;
    Ok(json!({
        "organisation": record,
        "membership": membership,
    }))
}

/// The organisation, if the caller may see it.
pub fn get(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    organisation_id: &str,
) -> Result<Value, StoreError> {
    let membership = active_membership(graph, organisation_id, user_id)?;
    let record = held(graph, ORGANIZATION)
        .get(organisation_id)?
        .ok_or(StoreError::NotFound)?;
    let mut answer = record;
    answer["role"] = json!(text(&membership, "role"));
    Ok(answer)
}

/// Every organisation the caller belongs to, newest first.
pub fn mine(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    limit: usize,
) -> Result<Vec<Value>, StoreError> {
    let memberships =
        held(graph, ORGANIZATION_MEMBERSHIP).matching("member_user_id", user_id, 2_000)?;
    let mut found = Vec::new();
    for membership in memberships {
        if text(&membership, "status") != ACTIVE {
            continue;
        }
        let organisation_id = text(&membership, "organisation_id");
        let Ok(Some(record)) = held(graph, ORGANIZATION).get(&organisation_id) else {
            continue;
        };
        let mut answer = record;
        answer["role"] = json!(text(&membership, "role"));
        found.push(answer);
        if found.len() >= limit {
            break;
        }
    }
    Ok(found)
}

/// The people in an organisation.
pub fn members(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    organisation_id: &str,
) -> Result<Vec<Value>, StoreError> {
    active_membership(graph, organisation_id, user_id)?;
    let memberships =
        held(graph, ORGANIZATION_MEMBERSHIP).matching("organisation_id", organisation_id, 2_000)?;
    Ok(memberships
        .into_iter()
        .map(|membership| {
            json!({
                "user_id": text(&membership, "member_user_id"),
                "role": text(&membership, "role"),
                "status": text(&membership, "status"),
                "joined_at": membership.get("created_at").cloned().unwrap_or(Value::Null),
            })
        })
        .collect())
}

/// Put somebody in the organisation, or change the role of somebody already in it.
pub fn add_member(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    organisation_id: &str,
    sent: &Map<String, Value>,
) -> Result<Value, StoreError> {
    require_administrator(graph, organisation_id, user_id)?;
    let payload = Value::Object(sent.clone());
    let member = text(&payload, "user_id").trim().to_string();
    if member.is_empty() {
        return Err(refused("a membership names a user_id"));
    }
    let role = match text(&payload, "role") {
        stated if stated.trim().is_empty() => MEMBER.to_string(),
        stated => stated,
    };
    // the last owner cannot be removed or demoted, or the organisation becomes nobody's
    if role != OWNER {
        if let Ok(Some(existing)) = membership_for(graph, organisation_id, &member) {
            if text(&existing, "role") == OWNER {
                let owners = held(graph, ORGANIZATION_MEMBERSHIP)
                    .matching("organisation_id", organisation_id, 2_000)?
                    .into_iter()
                    .filter(|row| text(row, "role") == OWNER && text(row, "status") == ACTIVE)
                    .count();
                if owners <= 1 {
                    return Err(StoreError::Conflict(
                        "an organisation keeps at least one owner".to_string(),
                    ));
                }
            }
        }
    }
    grant(graph, organisation_id, &member, &role)
}

/// The Books the organisation owns.
pub fn books_of(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    organisation_id: &str,
) -> Result<Vec<Value>, StoreError> {
    active_membership(graph, organisation_id, user_id)?;
    let mut found: Vec<Value> = books(graph)
        .everywhere("organisation_id", &json!(organisation_id))?
        .into_iter()
        .filter(|book| text(book, "organisation_id") == organisation_id)
        .collect();
    found.sort_by_key(|book| {
        std::cmp::Reverse(
            book.get("created_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )
    });
    Ok(found)
}

/// Move a Book into an organisation.
///
/// The caller has to be able to do both halves: administer the organisation, and hold the Book. Otherwise a member of
/// one organisation could pull somebody else's Book into it.
pub fn attach_book(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    organisation_id: &str,
    book_id: &str,
) -> Result<Value, StoreError> {
    require_administrator(graph, organisation_id, user_id)?;
    vimbai_service_offline_sync::sync::member_of(graph, book_id, user_id, Some(&[OWNER]))?
        .get("role")
        .cloned()
        .ok_or(StoreError::NotFound)?;
    let moved = books(graph).update(
        book_id,
        book_id,
        &json!({ "organisation_id": organisation_id, "updated_at": stamp() }),
    )?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn created(graph: &MemoryGraph, user: &str, name: &str) -> Value {
        let mut sent = Map::new();
        sent.insert("name".to_string(), json!(name));
        create(graph, user, &sent).expect("an organisation")
    }

    #[test]
    fn the_first_organisation_needs_no_prior_context() {
        let graph = MemoryGraph::new();
        let opened = created(&graph, "user-1", "Mukandara Holdings");
        let organisation = &opened["organisation"];
        assert_eq!(organisation["name"], json!("Mukandara Holdings"));
        assert_eq!(organisation["slug"], json!("mukandara-holdings"));
        assert_eq!(organisation["created_by"], json!("user-1"));
        // whoever opens it owns it, and that membership is what every later question reads
        assert_eq!(opened["membership"]["role"], json!("owner"));
        assert_eq!(opened["membership"]["status"], json!("active"));

        let theirs = mine(&graph, "user-1", 50).expect("their organisations");
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0]["role"], json!("owner"));
        // and it is not somebody else's
        assert!(mine(&graph, "user-2", 50).expect("a listing").is_empty());
    }

    #[test]
    fn two_organisations_may_share_a_name_and_keep_separate_slugs() {
        let graph = MemoryGraph::new();
        let first = created(&graph, "user-1", "Kufunda School");
        let second = created(&graph, "user-2", "Kufunda School");
        assert_eq!(first["organisation"]["slug"], json!("kufunda-school"));
        assert_eq!(second["organisation"]["slug"], json!("kufunda-school-2"));
    }

    #[test]
    fn a_person_may_belong_to_two_organisations_with_a_different_role_in_each() {
        let graph = MemoryGraph::new();
        // somebody else's organisation, where user-1 is only a member
        let first = created(&graph, "user-2", "One");
        // and user-1's own, where they are the owner
        created(&graph, "user-1", "Two");
        let first_id = text(&first["organisation"], "id");
        add_member(
            &graph,
            "user-2",
            &first_id,
            &Map::from_iter([("user_id".to_string(), json!("user-1"))]),
        )
        .expect("added as a member");

        let mine = mine(&graph, "user-1", 10).expect("their organisations");
        assert_eq!(mine.len(), 2);
        let roles: Vec<String> = mine.iter().map(|row| text(row, "role")).collect();
        assert!(roles.contains(&"owner".to_string()), "{roles:?}");
        assert!(roles.contains(&"member".to_string()), "{roles:?}");
    }

    #[test]
    fn a_member_may_not_add_members_and_a_stranger_is_told_nothing() {
        let graph = MemoryGraph::new();
        let opened = created(&graph, "user-1", "One");
        let id = text(&opened["organisation"], "id");
        add_member(
            &graph,
            "user-1",
            &id,
            &Map::from_iter([("user_id".to_string(), json!("user-2"))]),
        )
        .expect("a member");

        let refusal = add_member(
            &graph,
            "user-2",
            &id,
            &Map::from_iter([("user_id".to_string(), json!("user-3"))]),
        );
        assert!(
            matches!(refusal, Err(StoreError::Forbidden(_))),
            "{refusal:?}"
        );

        // a non-member learns the organisation is not there, rather than that it is not theirs
        assert!(matches!(
            get(&graph, "user-9", &id),
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            members(&graph, "user-9", &id),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn the_last_owner_cannot_be_demoted() {
        let graph = MemoryGraph::new();
        let opened = created(&graph, "user-1", "One");
        let id = text(&opened["organisation"], "id");
        let refusal = add_member(
            &graph,
            "user-1",
            &id,
            &Map::from_iter([
                ("user_id".to_string(), json!("user-1")),
                ("role".to_string(), json!("member")),
            ]),
        );
        assert!(
            matches!(refusal, Err(StoreError::Conflict(_))),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_role_outside_the_list_is_refused_rather_than_stored() {
        let graph = MemoryGraph::new();
        let opened = created(&graph, "user-1", "One");
        let id = text(&opened["organisation"], "id");
        let refusal = add_member(
            &graph,
            "user-1",
            &id,
            &Map::from_iter([
                ("user_id".to_string(), json!("user-2")),
                ("role".to_string(), json!("superuser")),
            ]),
        );
        assert!(
            matches!(refusal, Err(StoreError::Invalid(_))),
            "{refusal:?}"
        );
    }
}
