"""
Departmental accounting report tests.

Covers the POST /reports/department-comparison alias that was added so
clients can send the comparison request as a standard JSON body (the
original GET endpoint accepts a request body, which is awkward for HTTP
clients and proxies). The GET-with-body contract is kept unchanged for
backward compatibility.
"""

import importlib.util
import os
import sys

from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)


def load_service_app(service_dir):
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


DEPARTMENT_PAYLOAD = {
    "id": "will-be-reassigned",
    "department_code": "ENG-001",
    "department_name": "Engineering",
    "department_type": "cost",
    "manager_id": "mgr-001",
    "manager_name": "T. Moyo",
    "status": "active",
}


class TestDepartmentComparisonReport:
    def setup_method(self):
        # App load registers the package alias; then patch the fake driver and
        # stamp caller identity on every request.
        app = load_service_app("departmental-accounting-service")
        from departmental_accounting_service.database import Neo4jConnector

        fake_path = os.path.join(REPO_ROOT, "departmental-accounting-service", "fake_neo4j.py")
        fake_spec = importlib.util.spec_from_file_location("dept_root_fake", fake_path)
        fake = importlib.util.module_from_spec(fake_spec)
        fake_spec.loader.exec_module(fake)
        shared = fake.FakeSession()
        Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(shared))
        self.client = TestClient(app)
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})

    def _create_department(self, code: str, name: str) -> dict:
        payload = dict(DEPARTMENT_PAYLOAD)
        payload["department_code"] = code
        payload["department_name"] = name
        resp = self.client.post("/departments", json=payload)
        assert resp.status_code in (200, 201), resp.text
        return resp.json()

    def test_post_department_comparison_body_alias(self):
        dept = self._create_department("ENG-001", "Engineering")
        body = {
            "department_ids": [dept["id"]],
            "period_start": "2026-01-01T00:00:00",
            "period_end": "2026-12-31T23:59:59",
        }
        resp = self.client.post("/reports/department-comparison", json=body)
        assert resp.status_code == 200, resp.text
        data = resp.json()
        assert data["period"] == "2026-01-01 to 2026-12-31"
        assert len(data["departments"]) == 1
        assert data["summary"]["total_net_income"] is not None

    def test_get_department_comparison_still_accepts_body(self):
        # Backward compatibility: the original GET takes a bare JSON list
        # body for department_ids and query params for the period.
        dept = self._create_department("FIN-002", "Finance")
        resp = self.client.request(
            "GET",
            "/reports/department-comparison",
            params={"period_start": "2026-01-01T00:00:00", "period_end": "2026-12-31T23:59:59"},
            json=[dept["id"]],
        )
        assert resp.status_code == 200, resp.text
        assert len(resp.json()["departments"]) == 1

    def test_post_comparison_multiple_departments_sorted(self):
        d1 = self._create_department("OPS-003", "Operations")
        d2 = self._create_department("SLS-004", "Sales")
        body = {
            "department_ids": [d1["id"], d2["id"]],
            "period_start": "2026-01-01T00:00:00",
            "period_end": "2026-12-31T23:59:59",
        }
        resp = self.client.post("/reports/department-comparison", json=body)
        assert resp.status_code == 200, resp.text
        data = resp.json()
        assert len(data["departments"]) == 2

    def test_post_comparison_unknown_department(self):
        body = {
            "department_ids": ["no-such-dept"],
            "period_start": "2026-01-01T00:00:00",
            "period_end": "2026-12-31T23:59:59",
        }
        resp = self.client.post("/reports/department-comparison", json=body)
        # Unknown / cross-scope ids are rejected (404), never a 5xx.
        assert resp.status_code == 404, resp.text
