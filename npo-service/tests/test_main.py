"""
Vimbai NPO Service - Test Suite
Fake Neo4j driver interprets the Cypher patterns used by crud.py;
covers CRUD for the main entity families plus Book (X-Book-ID) isolation.
"""

import re

import pytest
from fastapi.testclient import TestClient
from main import app
from npo_service.database import Neo4jConnector

client = TestClient(app)  # no context manager: startup (real Neo4j) never runs

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}
USER = {"X-User-Id": "user-1"}


class Temporal:
    """Mimics neo4j DateTime/Date temporal values."""

    def __init__(self, iso):
        self._iso = iso

    def iso_format(self):
        return self._iso

    def __str__(self):
        return self._iso


class Counters:
    def __init__(self, nodes_deleted=0):
        self.nodes_deleted = nodes_deleted


class FakeResult:
    def __init__(self, records=None):
        self._records = records or []

    async def single(self):
        return self._records[0] if self._records else None

    def __aiter__(self):
        self._iter = iter(self._records)
        return self

    async def __anext__(self):
        try:
            return next(self._iter)
        except StopIteration:
            raise StopAsyncIteration


PROP_RE = re.compile(r"(\w+): (?:toFloat|date|datetime)?\(?\$(\w+)\)?[,\n]")
CREATE_NODE_RE = re.compile(r"CREATE \((\w+):(\w+) \{")


class FakeSession:
    def __init__(self):
        self.nodes = []  # dicts: {label, var, props}

    # -- helpers -------------------------------------------------------
    def _book_visible(self, node, params):
        return params.get("book_id") is None or node["props"].get("book_id") == params.get("book_id")

    def _extract_props(self, query, params, var, label):
        # take the CREATE block for this node
        m = CREATE_NODE_RE.search(query)
        block = query[m.start() :]
        props = {}
        for pm in re.finditer(r"(\w+): (?:(toFloat|date|datetime)\(\$?(\w+)[^)]*\)?|\$(\w+))", block):
            name, wrapper, pvar, dvar = pm.group(1), pm.group(2), pm.group(3), pm.group(4)
            key = pvar or dvar
            if key not in params:
                continue
            val = params[key]
            if wrapper == "toFloat":
                props[name] = float(val)
            elif wrapper in ("date", "datetime"):
                props[name] = Temporal(val)
            else:
                props[name] = val
        # fund link for transactions
        if "fund_id" in params and "fund_id" not in props:
            props["fund_id"] = params["fund_id"]
        return props

    def _match_nodes(self, query, params):
        """Return nodes matching label + eq-filters + book filter."""
        # skip u:User match, find main node
        m = re.search(
            r"MATCH \(u:User \{\{?id: \$user_id\}\}?\)(?:, |)-\[:\w+\]->\((\w+):(\w+)( \{\{?([^}]*)\}\}?)?", query
        )
        if not m:
            m = re.search(r"MATCH \((\w+):(\w+)(?: \{([^}]*)\})?\)", query)
        if not m:
            raise AssertionError(f"FakeSession: no MATCH pattern in: {query[:100]!r}")
        var, label, match_props = m.group(1), m.group(2), m.group(3) or ""

        eq = {}
        for pm in re.finditer(r"(\w+): \$(\w+)", match_props):
            eq[pm.group(1)] = params.get(pm.group(2))

        # chained node with its own id filter (e.g. ->(f:NPOFund {{id: $fund_id}}))
        for cm2 in re.finditer(r"\)->\[:\w+\]->\((\w+):(\w+) \{\{?\w+: \$(\w+)\}?\}\)", query):
            cvar, clabel, cparam = cm2.groups()
            if cvar != var and cparam in params:
                eq[cparam] = params[cparam]

        found = []
        for node in self.nodes:
            if node["label"] != label:
                continue
            if any(node["props"].get(k) != v for k, v in eq.items()):
                continue
            if not self._book_visible(node, params):
                continue
            if not self._extra_where(query, params, node, var):
                continue
            found.append((var, node))
        return var, label, found

    def _extra_where(self, query, params, node, var):
        """Evaluate WHERE conditions beyond the book filter."""
        m = re.search(r"\bWHERE (.+?)(?:\n\s*(?:RETURN|SET|CREATE)|\"\"\")", query, re.S)
        if not m:
            return True
        cond = m.group(1)
        props = node["props"]
        for part in re.split(r"\bAND\b", cond):
            part = part.strip()
            if not part or part == "true {type_filter}" or "$book_id IS NULL" in part or part == "true":
                continue
            pm = re.match(rf"{var}\.(\w+) (>=|<=|=) date\(\$(\w+)\)", part)
            if pm:
                p, op, k = pm.group(1), pm.group(2), pm.group(3)
                node_val = props.get(p)
                node_iso = node_val.iso_format() if isinstance(node_val, Temporal) else str(node_val)
                want = params[k]
                if op == ">=" and not node_iso >= want:
                    return False
                if op == "<=" and not node_iso <= want:
                    return False
                if op == "=" and node_iso != want:
                    return False
                continue
            pm = re.match(rf"{var}\.(\w+) (>=|<=|=) \$(\w+)", part)
            if pm:
                p, op, k = pm.group(1), pm.group(2), pm.group(3)
                if op == "=" and props.get(p) != params[k]:
                    return False
                continue
            pm = re.match(rf"{var}\.(\w+) = '([^']+)'", part)
            if pm:
                if props.get(pm.group(1)) != pm.group(2):
                    return False
                continue
            pm = re.match(rf"{var}\.(\w+) IN (\[[^\]]*\])", part)
            if pm:
                allowed = re.findall(r"'([^']+)'", pm.group(2))
                if props.get(pm.group(1)) not in allowed:
                    return False
                continue
            # unknown condition: skip
        return True

    # -- main ----------------------------------------------------------
    async def run(self, query, params=None, **kw):
        merged = dict(params or {})
        merged.update(kw)

        # --- SET path (update_fund_balance) ---
        if re.search(r"\bSET f\.current_balance", query):
            var, label, found = self._match_nodes(query, merged)
            if found:
                f = found[0][1]["props"]
                amount = float(merged["amount"])
                if "total_contributions" in query:
                    f["current_balance"] = f.get("current_balance", 0.0) + amount
                    f["total_contributions"] = f.get("total_contributions", 0.0) + amount
                else:
                    f["current_balance"] = f.get("current_balance", 0.0) - amount
                    f["total_disbursements"] = f.get("total_disbursements", 0.0) + amount
            return FakeResult([])

        # --- CREATE path ---
        cm = CREATE_NODE_RE.search(query)
        if cm:
            var, label = cm.group(1), cm.group(2)
            # comma-guard: referenced parent must exist & be book-visible
            gm = re.search(r"MATCH \(u:User \{id: \$user_id\}\), \((\w+):(\w+) \{(\w+): \$(\w+)\}\)", query)
            if gm:
                gvar, glabel, gprop, gparam = gm.groups()
                parent = [
                    n
                    for n in self.nodes
                    if n["label"] == glabel
                    and n["props"].get(gprop) == merged.get(gparam)
                    and self._book_visible(n, merged)
                ]
                if not parent:
                    return FakeResult([])
            props = self._extract_props(query, merged, var, label)
            node = {"label": label, "var": var, "props": props}
            self.nodes.append(node)
            return FakeResult([{var: props}])

        # --- aggregate SUM/COUNT ---
        am = re.search(r"RETURN sum\((\w+)\.(\w+)\) as (\w+)", query)
        if am:
            var, label, found = self._match_nodes(query, merged)
            total = sum(float(n["props"].get(am.group(2)) or 0) for _, n in found)
            rec = {am.group(3): total}
            cm2 = re.search(r"count\((\w+)\) as (\w+)", query)
            if cm2:
                rec[cm2.group(2)] = len(found)
            return FakeResult([rec])

        am = re.search(r"RETURN count\((\w+)\) as (\w+)", query)
        if am:
            var, label, found = self._match_nodes(query, merged)
            return FakeResult([{am.group(2): len(found)}])

        # --- latest-by-order (net assets fallback) ---
        if "ORDER BY" in query:
            var, label, found = self._match_nodes(query, merged)
            om = re.search(r"ORDER BY (\w+)\.(\w+)", query)
            if om:
                found.sort(key=lambda t: str(t[1]["props"].get(om.group(2))), reverse=True)
            limit = 1 if "LIMIT 1" in query else len(found)
            recs = []
            rm2 = re.search(r"RETURN \w+(?:,\s*\w+\.(\w+) as (\w+))?", query)
            for v, n in found[:limit]:
                rec = {v: n["props"]}
                if rm2 and rm2.group(2):
                    rec[rm2.group(2)] = n["props"].get("fund_id")
                recs.append(rec)
            return FakeResult(recs)

        # --- RETURN scalar of node (e.g. f.id as fund_id) ---
        var, label, found = self._match_nodes(query, merged)
        rm = re.search(r"RETURN (\w+)(?:, (\w+)\.(\w+) as (\w+))?", query)
        records = []
        for v, n in found:
            rec = {v: n["props"]}
            if rm.group(2):
                rec[rm.group(4)] = n["props"].get(rm.group(3))
            records.append(rec)
        return FakeResult(records)


class FakeDriver:
    def __init__(self, session):
        self._session = session

    def session(self):
        driver = self

        class _CM:
            async def __aenter__(self_inner):
                return driver._session

            async def __aexit__(self_inner, *a):
                return False

        return _CM()


@pytest.fixture
def session():
    return FakeSession()


@pytest.fixture(autouse=True)
def patch_driver(session):
    Neo4jConnector.get_driver = classmethod(lambda cls: FakeDriver(session))
    yield
    # leave patched; next fixture re-patches


def _fund_payload(code="FUND-001", ftype="general"):
    return {
        "fund_code": code,
        "fund_name": "General Operations Fund",
        "fund_type": ftype,
        "purpose": "Day to day operations",
        "description": "Main fund",
        "initial_balance": "1000.00",
        "currency": "USD",
        "created_date": "2026-01-15",
    }


def _create_fund(extra_headers=None, **overrides):
    overrides = dict(overrides)
    if "code" in overrides:
        overrides["fund_code"] = overrides.pop("code")
    if "ftype" in overrides:
        overrides["fund_type"] = overrides.pop("ftype")
    r = client.post("/funds/", json={**_fund_payload(), **overrides}, headers={**USER, **(extra_headers or {})})
    assert r.status_code == 201, r.text
    return r.json()


class TestFunds:
    def test_create_and_get_fund(self):
        created = _create_fund()
        r = client.get(f"/funds/{created['id']}", headers=USER)
        assert r.status_code == 200
        assert r.json()["fund_code"] == "FUND-001"
        assert float(r.json()["current_balance"]) == 1000.0

    def test_duplicate_code_conflict(self):
        _create_fund()
        r = client.post("/funds/", json=_fund_payload(), headers=USER)
        assert r.status_code == 409

    def test_list_funds_with_type_filter(self):
        _create_fund(code="F-GEN", ftype="general")
        _create_fund(code="F-END", ftype="endowment")
        r = client.get("/funds/", params={"fund_type": "endowment"}, headers=USER)
        assert r.status_code == 200
        assert [f["fund_code"] for f in r.json()] == ["F-END"]

    def test_unknown_fund_404(self):
        r = client.get("/funds/nope-404", headers=USER)
        assert r.status_code == 404


class TestFundTransactions:
    def _tx(self, fund_id, extra_headers=None, ttype="contribution"):
        return client.post(
            f"/funds/{fund_id}/transactions/",
            json={
                "transaction_date": "2026-02-01",
                "transaction_type": ttype,
                "amount": "250.00",
                "description": "Donation deposit",
                "reference_number": "TX-1",
                "category": "donation",
            },
            headers={**USER, **(extra_headers or {})},
        )

    def test_contribution_updates_balance(self):
        fund = _create_fund()
        r = self._tx(fund["id"])
        assert r.status_code == 201, r.text
        assert float(r.json()["balance_after"]) == 1250.0

        r = client.get(f"/funds/{fund['id']}", headers=USER)
        assert float(r.json()["current_balance"]) == 1250.0

    def test_restricted_fund_rejects_disbursement(self):
        fund = _create_fund(code="F-RES", ftype="permanently_restricted")
        r = self._tx(fund["id"], ttype="disbursement")
        assert r.status_code == 400

    def test_transaction_to_unknown_fund_404(self):
        r = self._tx("nope-404")
        assert r.status_code == 404

    def test_transactions_list(self):
        fund = _create_fund()
        self._tx(fund["id"])
        r = client.get(f"/funds/{fund['id']}/transactions/", headers=USER)
        assert r.status_code == 200
        assert len(r.json()) == 1


class TestBookIsolation:
    def test_fund_stamped_with_book_id(self, session):
        _create_fund(BOOK_A)
        assert [n for n in session.nodes if n["label"] == "NPOFund"][0]["props"]["book_id"] == "book-aaa-111"
        _create_fund(code="FUND-PERSONAL")  # personal, no header
        assert [n for n in session.nodes if n["label"] == "NPOFund"][1]["props"]["book_id"] is None

    def test_fund_hidden_from_other_book(self):
        created = _create_fund(BOOK_A)
        r = client.get(f"/funds/{created['id']}", headers={**USER, **BOOK_B})
        assert r.status_code == 404

    def test_fund_list_scoped_to_book(self):
        _create_fund(code="F-A", extra_headers=BOOK_A)
        _create_fund(code="F-B", extra_headers=BOOK_B)
        r_a = client.get("/funds/", headers={**USER, **BOOK_A})
        assert [f["fund_code"] for f in r_a.json()] == ["F-A"]
        r_b = client.get("/funds/", headers={**USER, **BOOK_B})
        assert [f["fund_code"] for f in r_b.json()] == ["F-B"]

    def test_unscoped_sees_all(self):
        _create_fund(code="F-P")
        _create_fund(code="F-BK", extra_headers=BOOK_B)
        r = client.get("/funds/", headers=USER)
        assert len(r.json()) == 2

    def test_transaction_blocked_into_other_books_fund(self):
        fund = _create_fund(BOOK_A)
        r = client.post(
            f"/funds/{fund['id']}/transactions/",
            json={
                "transaction_date": "2026-02-01",
                "transaction_type": "contribution",
                "amount": "10.00",
                "description": "sneaky",
            },
            headers={**USER, **BOOK_B},
        )
        assert r.status_code == 404

    def test_transaction_list_book_scoped(self):
        fund = _create_fund(BOOK_A)
        client.post(
            f"/funds/{fund['id']}/transactions/",
            json={
                "transaction_date": "2026-02-01",
                "transaction_type": "contribution",
                "amount": "5.00",
                "description": "a",
            },
            headers={**USER, **BOOK_A},
        )
        # Book B sees an empty transaction list for its own view of the fund
        r = client.get(f"/funds/{fund['id']}/transactions/", headers={**USER, **BOOK_B})
        assert r.status_code == 200
        assert r.json() == []


class TestGrants:
    def _grant_payload(self, fund_id):
        return {
            "grant_name": "Education Grant 2026",
            "grantor_name": "Future Foundation",
            "grant_type": "foundation",
            "status": "active",
            "amount_awarded": "50000.00",
            "purpose": "Support education programs",
            "fund_id": fund_id,
        }

    def test_create_and_list_grants(self):
        fund = _create_fund(code="F-G")
        r = client.post("/grants/", json=self._grant_payload(fund["id"]), headers=USER)
        assert r.status_code == 201, r.text
        r = client.get("/grants/", headers=USER)
        assert r.status_code == 200
        assert len(r.json()) == 1
        assert r.json()[0]["grant_name"] == "Education Grant 2026"

    def test_grant_requires_visible_fund(self):
        fund = _create_fund(code="F-GA", extra_headers=BOOK_A)
        r = client.post("/grants/", json=self._grant_payload(fund["id"]), headers={**USER, **BOOK_B})
        assert r.status_code == 404

    def test_grant_list_book_scoped(self):
        fund_a = _create_fund(code="F-BA", extra_headers=BOOK_A)
        fund_b = _create_fund(code="F-BB", extra_headers=BOOK_B)
        client.post("/grants/", json=self._grant_payload(fund_a["id"]), headers={**USER, **BOOK_A})
        client.post("/grants/", json=self._grant_payload(fund_b["id"]), headers={**USER, **BOOK_B})
        r_a = client.get("/grants/", headers={**USER, **BOOK_A})
        assert len(r_a.json()) == 1
        r_b = client.get("/grants/", headers={**USER, **BOOK_B})
        assert len(r_b.json()) == 1


class TestDonations:
    def _donation_payload(self):
        return {
            "donation_date": "2026-03-01",
            "amount": "500.00",
            "donor_id": "donor-1",
            "donation_type": "cash",
        }

    def test_create_donation(self):
        r = client.post("/donations/", json=self._donation_payload(), headers=USER)
        assert r.status_code == 201, r.text
        r = client.get("/donations/", headers=USER)
        assert len(r.json()) == 1
        assert float(r.json()[0]["amount"]) == 500.0

    def test_donation_book_scoped(self):
        client.post("/donations/", json=self._donation_payload(), headers={**USER, **BOOK_A})
        client.post("/donations/", json=self._donation_payload(), headers={**USER, **BOOK_B})
        r_a = client.get("/donations/", headers={**USER, **BOOK_A})
        assert len(r_a.json()) == 1
        assert len(client.get("/donations/", headers=USER).json()) == 2


class TestProjectsAndPrograms:
    def test_create_and_list_projects(self):
        payload = {
            "project_name": "Clean Water Project",
            "project_code": "P-CW-1",
            "description": "Wells for rural schools",
            "total_budget": "10000.00",
        }
        r = client.post("/projects/", json=payload, headers=USER)
        assert r.status_code == 201, r.text
        r = client.get("/projects/", headers=USER)
        assert [p["project_code"] for p in r.json()] == ["P-CW-1"]

    def test_projects_book_scoped(self):
        payload = {
            "project_name": "Book A Project",
            "project_code": "P-BA",
            "description": "x",
            "total_budget": "1.00",
        }
        client.post("/projects/", json=payload, headers={**USER, **BOOK_A})
        payload["project_code"] = "P-BB"
        payload["project_name"] = "Book B Project"
        client.post("/projects/", json=payload, headers={**USER, **BOOK_B})
        r_a = client.get("/projects/", headers={**USER, **BOOK_A})
        assert [p["project_code"] for p in r_a.json()] == ["P-BA"]

    def test_create_and_list_programs(self):
        payload = {
            "program_name": "Youth Literacy",
            "program_code": "PR-YL",
            "description": "Reading programs",
            "mission_alignment": "Education focus",
        }
        r = client.post("/programs/", json=payload, headers=USER)
        assert r.status_code == 201, r.text
        r = client.get("/programs/", headers=USER)
        assert [p["program_code"] for p in r.json()] == ["PR-YL"]


class TestDonors:
    def test_create_and_list_donors(self):
        payload = {
            "donor_name": "Grace Moyo",
            "donor_type": "individual",
            "email": "grace@example.org",
        }
        r = client.post("/donors/", json=payload, headers=USER)
        assert r.status_code == 201, r.text
        r = client.get("/donors/", headers=USER)
        assert len(r.json()) == 1

    def test_donors_book_scoped(self):
        payload = {"donor_name": "D A", "donor_type": "corporate"}
        client.post("/donors/", json=payload, headers={**USER, **BOOK_A})
        payload["donor_name"] = "D B"
        client.post("/donors/", json=payload, headers={**USER, **BOOK_B})
        r_b = client.get("/donors/", headers={**USER, **BOOK_B})
        assert [d["donor_name"] for d in r_b.json()] == ["D B"]


class TestBudgets:
    def _budget_payload(self, year="2026"):
        return {
            "budget_name": f"Operating Budget {year}",
            "fiscal_year": year,
            "period_start": f"{year}-01-01",
            "period_end": f"{year}-12-31",
            "total_budget": "25000.00",
        }

    def test_create_and_list_budgets(self):
        r = client.post("/budgets/", json=self._budget_payload(), headers=USER)
        assert r.status_code == 201, r.text
        r = client.get("/budgets/", headers=USER)
        assert len(r.json()) == 1

    def test_budgets_book_scoped(self):
        client.post("/budgets/", json=self._budget_payload("2026"), headers={**USER, **BOOK_A})
        client.post("/budgets/", json=self._budget_payload("2027"), headers={**USER, **BOOK_B})
        r_a = client.get("/budgets/", params={"fiscal_year": "2026"}, headers={**USER, **BOOK_A})
        assert [b["fiscal_year"] for b in r_a.json()] == ["2026"]
        r_b = client.get("/budgets/", headers={**USER, **BOOK_B})
        assert [b["fiscal_year"] for b in r_b.json()] == ["2027"]


class TestCrossBookGuards:
    def test_restriction_blocked_on_hidden_fund(self):
        fund = _create_fund(code="F-RES-BK", ftype="temporarily_restricted", extra_headers=BOOK_A)
        r = client.post(
            f"/funds/{fund['id']}/restrictions/",
            json={
                "restriction_type": "donor_imposed",
                "description": "Scholarships only",
                "is_permanent": False,
            },
            headers={**USER, **BOOK_B},
        )
        assert r.status_code == 404

    def test_budget_line_blocked_on_hidden_budget(self):
        r = client.post(
            "/budgets/",
            json={
                "budget_name": "BK Budget",
                "fiscal_year": "2026",
                "period_start": "2026-01-01",
                "period_end": "2026-12-31",
                "total_budget": "1000.00",
            },
            headers={**USER, **BOOK_A},
        )
        assert r.status_code == 201, r.text
        r2 = client.post(
            f"/budgets/{r.json()['id']}/lines/",
            json={
                "line_description": "Supplies",
                "category": "program",
                "budgeted_amount": "100.00",
            },
            headers={**USER, **BOOK_B},
        )
        assert r2.status_code == 404
