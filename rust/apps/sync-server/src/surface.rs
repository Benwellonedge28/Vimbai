#![forbid(unsafe_code)]
//! What a sync server is allowed to answer.
//!
//! The engine answers fourteen hundred addresses; a sync server exists to answer the handful a device needs while it is
//! catching up, so that sync can be scaled, restarted and rate-limited without also scaling the journal. It is the same
//! handlers - there is no second implementation of pushing an entry - with a surface in front of them.
//!
//! Everything outside that surface answers 404 with a reason rather than being quietly forwarded, because a sync pod that
//! will post a journal entry is a sync pod that can be used to post one.

/// The prefixes a device needs: somewhere to sign in, the Book it syncs, and the health door a deployment probes.
pub const SURFACE: [&str; 3] = ["/identity", "/books", "/health"];

/// Whether a path belongs on a sync server.
pub fn serves(path: &str) -> bool {
    if path == "/" {
        return false;
    }
    SURFACE.iter().any(|prefix| {
        path == *prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The addresses this server refuses, said in the answer so a misplaced client is not left guessing.
pub fn outside(path: &str) -> String {
    format!(
        "this is a sync server: it serves {}, not {path}",
        SURFACE.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_book_surface_is_served_and_the_ledger_is_not() {
        for path in [
            "/health",
            "/identity/users/login",
            "/books",
            "/books/book-1/entries/push",
            "/books/book-1/entries",
            "/books/book-1/devices",
        ] {
            assert!(serves(path), "{path} is what a device needs");
        }
        for path in [
            "/",
            "/journals",
            "/accounts/1000/postings",
            "/accounting/journal-entries",
            "/book-sync/books",
            "/booksomething",
            "/identityx",
        ] {
            assert!(!serves(path), "{path} does not belong on a sync server");
        }
    }

    #[test]
    fn a_refusal_names_what_this_server_does_serve() {
        let said = outside("/journals");
        assert!(said.contains("/books"), "{said}");
        assert!(said.contains("/journals"), "and what was asked for: {said}");
    }
}
