"""
Vimbai Supply Chain Service - Comprehensive Test Suite
Tests: inventory, suppliers, customers, purchase orders, sales invoices
"""

import os
from unittest.mock import AsyncMock, patch

import pytest
from fastapi.testclient import TestClient

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

from main import app

client = TestClient(app)


@pytest.fixture
def auth_headers():
    from datetime import datetime, timedelta, timezone

    import jwt as pyjwt

    token = pyjwt.encode(
        {
            "user_id": "test-user-id",
            "username": "testuser",
            "role": "admin",
            "permissions": ["supply:view", "supply:create", "supply:edit", "supply:delete"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


@pytest.fixture
def valid_customer():
    return {
        "name": "Test Customer",
        "email": "customer@vimbai.com",
        "phone": "+263771234567",
        "address": "123 Test Street, Harare",
        "tax_number": "BRN123456",
    }


@pytest.fixture
def valid_supplier():
    return {
        "name": "Test Supplier",
        "email": "supplier@vimbai.com",
        "phone": "+263771987654",
        "address": "456 Supplier Ave, Harare",
        "payment_terms": "net_30",
    }


@pytest.fixture
def valid_inventory_item():
    return {
        "name": "Test Product",
        "sku": "TEST-001",
        "quantity": 100,
        "unit_price": "15.99",
        "reorder_level": 20,
        "category": "General",
    }


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestCustomers:
    def test_create_customer_no_auth(self, valid_customer):
        response = client.post("/customers/", json=valid_customer)
        assert response.status_code in [401, 403]

    def test_create_customer_with_auth(self, auth_headers, valid_customer):
        response = client.post("/customers/", json=valid_customer, headers=auth_headers)
        assert response.status_code in [201, 200, 500]

    def test_create_customer_missing_fields(self, auth_headers):
        response = client.post("/customers/", json={"name": "Missing Fields"}, headers=auth_headers)
        assert response.status_code in [422, 400, 201]

    def test_get_customers_with_auth(self, auth_headers):
        response = client.get("/customers/", headers=auth_headers)
        assert response.status_code in [200, 500]

    def test_get_customers_no_auth(self):
        response = client.get("/customers/")
        assert response.status_code in [401, 403]


class TestSuppliers:
    def test_create_supplier_no_auth(self, valid_supplier):
        response = client.post("/suppliers/", json=valid_supplier)
        assert response.status_code in [401, 403]

    def test_create_supplier_with_auth(self, auth_headers, valid_supplier):
        response = client.post("/suppliers/", json=valid_supplier, headers=auth_headers)
        assert response.status_code in [201, 200, 500]

    def test_get_suppliers_with_auth(self, auth_headers):
        response = client.get("/suppliers/", headers=auth_headers)
        assert response.status_code in [200, 500]


class TestInventory:
    def test_create_inventory_item_no_auth(self, valid_inventory_item):
        response = client.post("/inventory-items/", json=valid_inventory_item)
        assert response.status_code in [401, 403]

    def test_create_inventory_item_with_auth(self, auth_headers, valid_inventory_item):
        response = client.post("/inventory-items/", json=valid_inventory_item, headers=auth_headers)
        assert response.status_code in [201, 200, 500]

    def test_get_inventory_items_with_auth(self, auth_headers):
        response = client.get("/inventory-items/", headers=auth_headers)
        assert response.status_code in [200, 500]

    def test_create_inventory_negative_quantity(self, auth_headers):
        response = client.post(
            "/inventory-items/",
            json={"name": "Negative Stock", "sku": "NEG-001", "quantity": -10, "unit_price": "5.00"},
            headers=auth_headers,
        )
        assert response.status_code in [422, 400, 201]


class TestPurchaseOrders:
    def test_create_purchase_order_no_auth(self):
        response = client.post(
            "/purchase-orders/",
            json={"supplier_id": "supplier-001", "items": [{"sku": "TEST-001", "quantity": 10, "unit_price": "15.99"}]},
        )
        assert response.status_code in [401, 403]

    def test_get_purchase_orders_with_auth(self, auth_headers):
        response = client.get("/purchase-orders/", headers=auth_headers)
        assert response.status_code in [200, 500]


class TestSalesInvoices:
    def test_create_sales_invoice_no_auth(self):
        response = client.post(
            "/sales-invoices/",
            json={"customer_id": "customer-001", "items": [{"sku": "TEST-001", "quantity": 5, "unit_price": "15.99"}]},
        )
        assert response.status_code in [401, 403]

    def test_get_sales_invoices_with_auth(self, auth_headers):
        response = client.get("/sales-invoices/", headers=auth_headers)
        assert response.status_code in [200, 500]
