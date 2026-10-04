#!/usr/bin/env python3
"""Vimbai load/stability test.

Spawns concurrent virtual users spread across N Books against a running
stack (by default the local docker-compose gateway), drives a realistic
Book-scoped read/write mix, and reports latency percentiles, throughput,
and error rates. Exits non-zero if any 5xx occurred or the error rate
exceeded the configured threshold.

Usage:
    python3 scripts/load_test.py [--base-url http://localhost:8081] \
        [--users 50] [--books 10] [--duration 60] [--error-threshold 0.01]

Authentication: the script registers fresh users via /identity
(unauthenticated by design), logs in, and drives every other call with the
issued Bearer token plus the user's Book context via X-Book-ID.
"""

import argparse
import json
import random
import statistics
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid

DEFAULT_BASE_URL = "http://localhost:8081"


class HttpError(Exception):
    def __init__(self, status, body):
        super().__init__(f"HTTP {status}: {body[:200]}")
        self.status = status


def _request(method, url, payload=None, headers=None, timeout=15.0):
    data = None
    hdrs = {"Content-Type": "application/json"}
    if headers:
        hdrs.update(headers)
    if payload is not None:
        data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode()
            return json.loads(body) if body else {}
    except urllib.error.HTTPError as exc:
        raise HttpError(exc.code, exc.read().decode(errors="replace")) from exc


class User:
    """A virtual user with its own identity token and Book."""

    def __init__(self, base_url, index):
        self.base_url = base_url
        self.username = f"loadtest_{index}_{uuid.uuid4().hex[:8]}"
        self.password = "LoadTest!2026"
        self.token = None
        self.book_id = None

    def register_and_login(self):
        _request(
            "POST",
            f"{self.base_url}/identity/users/register",
            {
                "email": f"{self.username}@example.com",
                "username": self.username,
                "password": self.password,
                "first_name": "Load",
                "last_name": "Test",
            },
        )
        # OAuth2 password grant form is used by identity-service login.
        import urllib.parse

        form = urllib.parse.urlencode({"username": self.username, "password": self.password}).encode()
        req = urllib.request.Request(f"{self.base_url}/identity/users/login", data=form, method="POST")
        req.add_header("Content-Type", "application/x-www-form-urlencoded")
        with urllib.request.urlopen(req, timeout=15.0) as resp:
            data = json.loads(resp.read().decode())
        self.token = data.get("access_token")
        if not self.token:
            raise RuntimeError(f"no access_token in login response: {list(data)}")

    def my_user_id(self):
        me = self._authed("GET", "/identity/users/me", with_book=False)
        return me.get("id") or me.get("user_id")

    def create_book(self, tier="business"):
        book = self._authed(
            "POST",
            "/book-sync/books",
            {"name": f"LoadTest Book {self.username}", "tier": tier},
        )
        self.book_id = (book.get("book") or {}).get("id") or book.get("id") or book.get("book_id")
        if not self.book_id:
            raise RuntimeError(f"no book id in create response: {book}")

    def invite(self, owner, role="admin"):
        """Have `owner` invite this user into their Book, then accept."""
        owner._authed(
            "POST",
            f"/book-sync/books/{owner.book_id}/members",
            {"user_id": self.my_user_id(), "role": role, "wrapped_book_key": "loadtest-wrapped-key"},
        )
        # Accept without X-Book-ID: membership is still 'invited' here, so the
        # gateway's Book-context membership check would 403 the call.
        self._authed("POST", f"/book-sync/books/{owner.book_id}/members/accept", with_book=False)

    def _authed(self, method, path, payload=None, with_book=True):
        headers = {"Authorization": f"Bearer {self.token}"}
        if with_book and self.book_id:
            headers["X-Book-ID"] = self.book_id
        return _request(method, f"{self.base_url}{path}", payload, headers)

    # --- scenario actions -------------------------------------------------

    def action_create_department(self):
        code = f"LTD-{random.randint(1000, 9999)}"
        return self._authed(
            "POST",
            "/departmental-accounting/departments",
            {
                "id": str(uuid.uuid4()),
                "department_code": code,
                "department_name": f"Load Dept {code}",
                "department_type": random.choice(["cost", "revenue", "support"]),
                "manager_id": self.username,
                "manager_name": self.username,
                "status": "active",
            },
        )

    def action_list_departments(self):
        return self._authed("GET", "/departmental-accounting/departments")

    def action_report(self):
        depts = self._authed("GET", "/departmental-accounting/departments")
        items = depts if isinstance(depts, list) else depts.get("departments", [])
        ids = [d.get("id") for d in items if d.get("id")]
        if not ids:
            return depts
        return self._authed(
            "POST",
            "/departmental-accounting/reports/department-comparison",
            {
                "department_ids": random.sample(ids, min(len(ids), 3)),
                "period_start": "2026-01-01T00:00:00",
                "period_end": "2026-12-31T23:59:59",
            },
        )

    ACTIONS = (action_list_departments, action_create_department, action_report)


class Stats:
    def __init__(self):
        self.lock = threading.Lock()
        self.latencies = []
        self.errors = 0
        self.client_errors = 0
        self.server_errors = 0
        self.counts = {}

    def record(self, elapsed, ok, status=None):
        with self.lock:
            self.latencies.append(elapsed)
            self.counts[status] = self.counts.get(status, 0) + 1
            if not ok:
                self.errors += 1
                if status is not None and status >= 500:
                    self.server_errors += 1
                else:
                    self.client_errors += 1


def worker(user, stats, deadline):
    while time.monotonic() < deadline:
        action = random.choice(User.ACTIONS)
        start = time.monotonic()
        try:
            action(user)
            stats.record(time.monotonic() - start, True, 200)
        except HttpError as exc:
            stats.record(time.monotonic() - start, False, exc.status)
        except Exception:
            stats.record(time.monotonic() - start, False, None)
        time.sleep(random.uniform(0.05, 0.25))


def pct(values, p):
    if not values:
        return 0.0
    values = sorted(values)
    idx = min(len(values) - 1, int(round((p / 100.0) * (len(values) - 1))))
    return values[idx]


def main():
    parser = argparse.ArgumentParser(description="Vimbai load test")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--users", type=int, default=50)
    parser.add_argument("--books", type=int, default=10, help="distinct Books to spread users across")
    parser.add_argument("--duration", type=int, default=60, help="seconds")
    parser.add_argument("--error-threshold", type=float, default=0.01)
    parser.add_argument("--skip-setup", action="store_true", help="assume users/books already exist in a local file")
    args = parser.parse_args()

    print(f"Load test against {args.base_url}: {args.users} users, {args.books} books, {args.duration}s")
    stats = Stats()

    users = []
    owners = []
    setup_errors = 0
    for i in range(args.users):
        user = User(args.base_url, i)
        try:
            user.register_and_login()
            if len(owners) < args.books:
                user.create_book()
                owners.append(user)
            else:
                # Join an existing Book: invite + accept, so Book-context
                # membership checks (gateway 403) are satisfied for real.
                user.invite(owners[i % max(len(owners), 1)], role="admin")
                user.book_id = owners[i % max(len(owners), 1)].book_id
            users.append(user)
        except Exception as exc:  # noqa: BLE001 - setup resilience
            setup_errors += 1
            print(f"  setup failed for user {i}: {exc}")
    if not users:
        print("ERROR: no users set up successfully - is the stack running?")
        return 2
    print(f"  set up {len(users)}/{args.users} users ({setup_errors} failed)")

    deadline = time.monotonic() + args.duration
    threads = [threading.Thread(target=worker, args=(u, stats, deadline)) for u in users]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    total = len(stats.latencies)
    error_rate = (stats.errors / total) if total else 1.0
    wall = max(args.duration, 1)
    print("\n===== Load test results =====")
    print(f"  requests:        {total}")
    print(f"  throughput:      {total / wall:.1f} req/s")
    print(f"  error rate:      {error_rate:.2%}  (client {stats.client_errors}, server {stats.server_errors})")
    if stats.latencies:
        print(f"  latency mean:    {statistics.mean(stats.latencies) * 1000:.0f} ms")
        print(f"  latency p50:     {pct(stats.latencies, 50) * 1000:.0f} ms")
        print(f"  latency p95:     {pct(stats.latencies, 95) * 1000:.0f} ms")
        print(f"  latency p99:     {pct(stats.latencies, 99) * 1000:.0f} ms")

    failed = False
    if stats.server_errors > 0:
        print("  FAIL: server errors (5xx) occurred")
        failed = True
    if error_rate > args.error_threshold:
        print(f"  FAIL: error rate above {args.error_threshold:.2%}")
        failed = True
    print("===== PASS =====" if not failed else "===== FAIL =====")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
