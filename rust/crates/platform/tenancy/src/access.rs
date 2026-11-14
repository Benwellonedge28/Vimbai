//! The one rule that decides whether a caller may open a Book.
//!
//! Two questions, in order:
//!
//! 1. Does the Book belong to an organisation? Then the caller has to be an active member of that organisation. The
//!    organisation is the boundary, so twenty Books in one organisation are twenty Books a member may open without
//!    twenty membership rows.
//! 2. No organisation? Then the caller has to be an active member of the Book itself, which is what every Book created
//!    before organisations existed relies on.
//!
//! There is no third case and no caller-visible difference between them: a refusal is a refusal. Keeping the rule here
//! rather than in each server is the point of the crate - the cluster's gateway, the engine and the on-device runtime
//! all ask this function, so they cannot drift into disagreeing about who may see what.
#![forbid(unsafe_code)]

use serde_json::Value;

use vimbai_service_offline_sync::sync;
use vimbai_store::read::text;
use vimbai_store::{Graph, StoreError};

use crate::organisation;

/// How the caller got in. Useful in a log line, and the only difference between the two routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// Through the organisation that owns the Book.
    Organisation,
    /// Through a membership of the Book itself, for a Book no organisation owns.
    Book,
}

/// What the caller may do, once the rule has answered yes.
#[derive(Debug, Clone)]
pub struct Access {
    pub book_id: String,
    /// The organisation the Book belongs to, when one does.
    pub organisation_id: Option<String>,
    /// The caller's role, in the organisation or in the Book.
    pub role: String,
    pub via: Via,
}

impl Access {
    /// The role is one of the ones that may change the Book.
    pub fn may_configure(&self) -> bool {
        match self.via {
            Via::Organisation => {
                organisation::ADMINISTRATORS.contains(&self.role.as_str())
                    || sync::ADMIN_ROLES.contains(&self.role.as_str())
            }
            Via::Book => sync::ADMIN_ROLES.contains(&self.role.as_str()),
        }
    }

    /// The role is one of the ones that may write records in the Book.
    pub fn may_write(&self) -> bool {
        match self.via {
            Via::Organisation => organisation::ROLES.contains(&self.role.as_str()),
            Via::Book => sync::WRITER_ROLES.contains(&self.role.as_str()),
        }
    }
}

/// The organisation a Book belongs to, when it belongs to one.
pub fn organisation_of(
    graph: &(impl Graph + ?Sized),
    book_id: &str,
) -> Result<Option<String>, StoreError> {
    let book = sync::book_of(graph, book_id)?;
    match text(&book, "organisation_id") {
        empty if empty.trim().is_empty() => Ok(None),
        named => Ok(Some(named)),
    }
}

/// May this person open this Book, and as what?
///
/// The Book has to exist first: a caller who names a Book that is not there is answered with the Book being missing, not
/// with a refusal, because "may you see this" is not a question about a Book nobody has.
pub fn allows(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    book_id: &str,
) -> Result<Access, StoreError> {
    let book = sync::book_of(graph, book_id)?;
    let organisation_id = match text(&book, "organisation_id") {
        empty if empty.trim().is_empty() => None,
        named => Some(named),
    };

    if let Some(ref organisation_id) = organisation_id {
        match organisation::membership_for(graph, organisation_id, user_id)? {
            Some(membership) if text(&membership, "status") == organisation::ACTIVE => {
                return Ok(Access {
                    book_id: book_id.to_string(),
                    organisation_id: Some(organisation_id.clone()),
                    role: text(&membership, "role"),
                    via: Via::Organisation,
                });
            }
            _ => {}
        }
    }

    // the Book's own membership: the rule for a Book no organisation owns, and the fallback a member of the
    // organisation also reaches, since a Book keeps the memberships it was opened with
    if let Some(membership) = sync::membership_for(graph, book_id, user_id)? {
        if text(&membership, "status") == "active" {
            return Ok(Access {
                book_id: book_id.to_string(),
                organisation_id,
                role: text(&membership, "role"),
                via: Via::Book,
            });
        }
    }

    Err(StoreError::Forbidden(format!(
        "No active membership in book {book_id}"
    )))
}

/// The same rule, for a caller who has nowhere to put the answer: ok, or the refusal a route answers with.
pub fn require(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    book_id: &str,
) -> Result<(), StoreError> {
    allows(graph, user_id, book_id).map(|_| ())
}

/// The answer the gateway asks for, kept here so the cluster, the engine and the machine in the corner share it.
pub fn membership_check(
    graph: &(impl Graph + ?Sized),
    user_id: &str,
    book_id: &str,
) -> Result<Value, StoreError> {
    let access = allows(graph, user_id, book_id)?;
    let book = sync::book_of(graph, book_id)?;
    Ok(serde_json::json!({
        "service": "book-sync-service",
        "book_id": book_id,
        "user_id": user_id,
        "role": access.role,
        "via": match access.via {
            Via::Organisation => "organisation",
            Via::Book => "book",
        },
        "organisation_id": access.organisation_id,
        "tier": match book.get("tier") {
            Some(Value::String(tier)) => serde_json::json!(tier),
            _ => Value::Null,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};
    use vimbai_store::fake::MemoryGraph;

    /// A Book the caller owns, as `POST /books` would leave it: opened by user-1, with no organisation yet.
    fn book(graph: &MemoryGraph, user: &str, organisation_id: Option<&str>) -> String {
        let id = sync::default_book(graph, &vimbai_store::BookScope::new(user, ""), &json!({}))
            .expect("a book");
        let book_id = text(&id["book"], "id");
        if let Some(organisation_id) = organisation_id {
            // the Book moves into the organisation through the same door an operator would use
            organisation::attach_book(graph, user, organisation_id, &book_id).expect("moved in");
        }
        book_id
    }

    fn organisation_for(graph: &MemoryGraph, user: &str, name: &str) -> String {
        let mut sent = Map::new();
        sent.insert("name".to_string(), json!(name));
        let opened = organisation::create(graph, user, &sent).expect("an organisation");
        text(&opened["organisation"], "id")
    }

    #[test]
    fn a_book_no_organisation_owns_is_opened_by_its_members() {
        let graph = MemoryGraph::new();
        let book_id = book(&graph, "user-1", None);
        let access = allows(&graph, "user-1", &book_id).expect("the owner");
        assert_eq!(access.via, Via::Book);
        assert_eq!(access.organisation_id, None);
        assert!(access.may_configure());

        let refusal = allows(&graph, "user-2", &book_id);
        assert!(
            matches!(refusal, Err(StoreError::Forbidden(_))),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_book_an_organisation_owns_is_opened_by_the_organisations_members() {
        let graph = MemoryGraph::new();
        let organisation_id = organisation_for(&graph, "user-1", "Holdings");
        let book_id = book(&graph, "user-1", Some(&organisation_id));

        // a member of the organisation, added by name, with no membership row of the Book at all
        let mut sent = Map::new();
        sent.insert("user_id".to_string(), json!("user-2"));
        organisation::add_member(&graph, "user-1", &organisation_id, &sent).expect("a member");

        let access = allows(&graph, "user-2", &book_id).expect("a member of the owner");
        assert_eq!(access.via, Via::Organisation);
        assert_eq!(
            access.organisation_id.as_deref(),
            Some(organisation_id.as_str())
        );
        assert_eq!(access.role, organisation::MEMBER);
        assert!(access.may_write());
        assert!(!access.may_configure(), "a member is not an administrator");

        // and a stranger is refused by the same rule, not by an empty list
        let refusal = allows(&graph, "user-9", &book_id);
        assert!(
            matches!(refusal, Err(StoreError::Forbidden(_))),
            "{refusal:?}"
        );
    }

    #[test]
    fn a_book_that_is_not_there_says_so_rather_than_refusing() {
        let graph = MemoryGraph::new();
        let answer = allows(&graph, "user-1", "book-that-never-was");
        assert!(matches!(answer, Err(StoreError::NotFound)), "{answer:?}");
    }

    #[test]
    fn the_gateway_answer_names_the_route_the_caller_came_in_by() {
        let graph = MemoryGraph::new();
        let organisation_id = organisation_for(&graph, "user-1", "Holdings");
        let book_id = book(&graph, "user-1", Some(&organisation_id));
        let answer = membership_check(&graph, "user-1", &book_id).expect("a check");
        assert_eq!(answer["via"], json!("organisation"));
        assert_eq!(answer["role"], json!("owner"));
        assert_eq!(answer["organisation_id"], json!(organisation_id));

        let lonely = book(&graph, "user-3", None);
        let plain = membership_check(&graph, "user-3", &lonely).expect("a check");
        assert_eq!(plain["via"], json!("book"));
        assert_eq!(plain["organisation_id"], Value::Null);
    }
}
