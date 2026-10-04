#!/usr/bin/env python3
"""Vimbai end-to-end usability smoke test.

Walks the full first-run chain a real client needs, against a running stack
(default: local docker-compose gateway at http://localhost:8081):

  1. register a fresh user at /identity (open route by design)
  2. log in and obtain a JWT
  3. create a Book (book-sync) and get an ACTIVE membership
  4. make Book-scoped calls (create + list departments, run a report)
  5. prove isolation: a second user must NOT see the first user's data
  6. prove auth is enforced: a call without a token is rejected 401

Exit code 0 = the stack is usable end to end.

Usage:
    python3 scripts/smoke_e2e_flow.py [--base-url http://localhost:8081]
"""

import argparse
import json
import sys
import urllib.error
import urllib.request
import uuid


def request(method, url, payload=None, headers=None, form=None, timeout=15.0):
    data = None
    hdrs = {"Content-Type": "application/json"}
    if headers:
        hdrs.update(headers)
    if form is not None:
        data = form.encode()
        hdrs["Content-Type"] = "application/x-www-form-urlencoded"
    elif payload is not None:
        data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode()
            return resp.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as exc:
        return exc.code, json.loads(exc.read().decode(errors="replace") or "{}")


class Step:
    def __init__(self, name):
        self.name = name

    def __enter__(self):
        print(f"[ .. ] {self.name}")
        return self

    def __exit__(self, exc_type, exc, tb):
        if exc is None:
            print(f"[ ok ] {self.name}")
        else:
            print(f"[FAIL] {self.name}: {exc}")
        return False  # propagate


def make_user(base, label):
    username = f"smoke_{label}_{uuid.uuid4().hex[:8]}"
    password = "Smoke!2026"
    status, body = request(
        "POST",
        f"{base}/identity/users/register",
        {
            "email": f"{username}@example.com",
            "username": username,
            "password": password,
            "first_name": "Smoke",
            "last_name": label,
        },
    )
    if status not in (200, 201):
        raise RuntimeError(f"register returned {status}: {body}")
    import urllib.parse

    form = urllib.parse.urlencode({"username": username, "password": password})
    status, body = request("POST", f"{base}/identity/users/login", form=form)
    if status != 200 or not body.get("access_token"):
        raise RuntimeError(f"login returned {status}: {body}")
    return {"username": username, "token": body["access_token"]}


def authed(user, method, path, payload=None, base=None, book_id=None):
    headers = {"Authorization": f"Bearer {user['token']}"}
    if book_id:
        headers["X-Book-ID"] = book_id
    return request(method, f"{base}{path}", payload, headers)


def main():
    parser = argparse.ArgumentParser(description="Vimbai E2E usability smoke")
    parser.add_argument("--base-url", default="http://localhost:8081")
    args = parser.parse_args()
    base = args.base_url

    with Step("gateway is up and auth is enforced"):
        # An unauthenticated call to a protected route must 401 - this also
        # proves the gateway itself is responding.
        status, _ = request("GET", f"{base}/departmental-accounting/departments")
        if status != 401:
            raise RuntimeError(f"expected 401 without token, got {status}")

    with Step("register + login (user A)"):
        user_a = make_user(base, "a")

    with Step("register + login (user B)"):
        user_b = make_user(base, "b")

    with Step("user A creates a business Book"):
        status, book = authed(user_a, "POST", "/book-sync/books", {"name": "Smoke Book", "tier": "business"}, base)
        if status not in (200, 201):
            raise RuntimeError(f"book create returned {status}: {book}")
        book_id = (book.get("book") or {}).get("id") or book.get("id") or book.get("book_id")
        if not book_id:
            raise RuntimeError(f"no book id: {book}")

    with Step("Book-scoped write: create department"):
        status, dept = authed(
            user_a,
            "POST",
            "/departmental-accounting/departments",
            {
                "id": str(uuid.uuid4()),
                "department_code": "SMK-001",
                "department_name": "Smoke Department",
                "department_type": "cost",
                "manager_id": user_a["username"],
                "manager_name": user_a["username"],
                "status": "active",
            },
            base,
            book_id,
        )
        if status not in (200, 201):
            raise RuntimeError(f"department create returned {status}: {dept}")
        dept_id = dept.get("id")

    with Step("Book-scoped read: list shows the department"):
        status, listing = authed(user_a, "GET", "/departmental-accounting/departments", base=base, book_id=book_id)
        if status != 200:
            raise RuntimeError(f"department list returned {status}: {listing}")
        items = listing if isinstance(listing, list) else listing.get("departments", [])
        ids = [d.get("id") for d in items]
        if dept_id not in ids:
            raise RuntimeError(f"created department {dept_id} not visible: {ids}")

    with Step("Book-scoped report runs (POST body contract)"):
        status, report = authed(
            user_a,
            "POST",
            "/departmental-accounting/reports/department-comparison",
            {
                "department_ids": [dept_id],
                "period_start": "2026-01-01T00:00:00",
                "period_end": "2026-12-31T23:59:59",
            },
            base,
            book_id,
        )
        if status != 200:
            raise RuntimeError(f"comparison report returned {status}: {report}")

    with Step("isolation: user B sees none of user A's data"):
        status, listing = authed(user_b, "GET", "/departmental-accounting/departments", base=base)
        if status != 200:
            raise RuntimeError(f"user B list returned {status}: {listing}")
        b_items = listing if isinstance(listing, list) else listing.get("departments", [])
        b_ids = [d.get("id") for d in b_items]
        if dept_id in b_ids:
            raise RuntimeError("CROSS-TENANT LEAK: user B sees user A's department")

    with Step("isolation: user B cannot use user A's Book"):
        status, _ = authed(user_b, "GET", "/departmental-accounting/departments", base=base, book_id=book_id)
        if status == 200:
            # gateway returns 403 for non-members; a 200 with foreign Book
            # context would be a leak.
            raise RuntimeError("user B used user A's Book context without membership")

    print("\nAll smoke checks passed - the stack is usable end to end.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
