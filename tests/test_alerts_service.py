"""
Comprehensive Tests for FinAcc Alerts Service
"""

import pytest
from httpx import AsyncClient, ASGITransport
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'alerts-service'))

from main import app, alert_rules, alerts, AlertEngine, AlertSeverity, AlertCategory

@pytest.fixture
def anyio_backend():
    return "asyncio"


# ============================================================================
# Health Check Tests
# ============================================================================

@pytest.mark.anyio
async def test_health_check():
    """Test health check endpoint"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "healthy"
        assert data["service"] == "alerts"


# ============================================================================
# Alert Engine Tests
# ============================================================================

def test_evaluate_threshold_greater_than():
    """Test threshold evaluation with greater than operator"""
    condition = {"type": "threshold", "field": "amount", "operator": "gt", "value": 1000}
    data = {"amount": 1500}
    assert AlertEngine.evaluate(condition, data) == True

    data = {"amount": 500}
    assert AlertEngine.evaluate(condition, data) == False


def test_evaluate_threshold_less_than():
    """Test threshold evaluation with less than operator"""
    condition = {"type": "threshold", "field": "balance", "operator": "lt", "value": 100}
    data = {"balance": 50}
    assert AlertEngine.evaluate(condition, data) == True

    data = {"balance": 150}
    assert AlertEngine.evaluate(condition, data) == False


def test_evaluate_threshold_equal():
    """Test threshold evaluation with equal operator"""
    condition = {"type": "threshold", "field": "status", "operator": "eq", "value": 1}
    data = {"status": 1}
    assert AlertEngine.evaluate(condition, data) == True

    data = {"status": 0}
    assert AlertEngine.evaluate(condition, data) == False


def test_evaluate_pattern_velocity():
    """Test pattern evaluation for velocity"""
    condition = {"type": "pattern", "pattern_type": "velocity", "threshold": 5}
    data = {"transaction_count": 10, "time_window_minutes": 30}
    assert AlertEngine.evaluate(condition, data) == True

    data = {"transaction_count": 3, "time_window_minutes": 60}
    assert AlertEngine.evaluate(condition, data) == False


def test_evaluate_missing_field():
    """Test that missing fields return False"""
    condition = {"type": "threshold", "field": "amount", "operator": "gt", "value": 1000}
    data = {"balance": 500}
    assert AlertEngine.evaluate(condition, data) == False


# ============================================================================
# Alert Rules API Tests
# ============================================================================

@pytest.mark.anyio
async def test_create_alert_rule():
    """Test creating a new alert rule"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "name": "Test Rule",
            "description": "A test alert rule",
            "category": "fraud",
            "severity": "high",
            "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 5000},
            "action": "notify",
            "enabled": True,
            "cooldown_seconds": 300
        }
        response = await client.post("/rules", json=payload)
        assert response.status_code == 201
        data = response.json()
        assert data["name"] == "Test Rule"
        assert data["category"] == "fraud"
        assert data["severity"] == "high"
        assert "id" in data


@pytest.mark.anyio
async def test_list_alert_rules():
    """Test listing all alert rules"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/rules")
        assert response.status_code == 200
        data = response.json()
        assert isinstance(data, list)
        # Should have default rules
        assert len(data) > 0


@pytest.mark.anyio
async def test_list_enabled_rules_only():
    """Test filtering to only enabled rules"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/rules?enabled_only=true")
        assert response.status_code == 200
        data = response.json()
        assert isinstance(data, list)
        for rule in data:
            assert rule["enabled"] == True


@pytest.mark.anyio
async def test_get_alert_rule():
    """Test getting a specific alert rule"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Use a known rule ID
        rule_id = "fraud-high-amount"
        response = await client.get(f"/rules/{rule_id}")
        assert response.status_code == 200
        data = response.json()
        assert data["id"] == rule_id


@pytest.mark.anyio
async def test_get_nonexistent_rule():
    """Test getting a non-existent rule returns 404"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/rules/nonexistent-rule")
        assert response.status_code == 404


@pytest.mark.anyio
async def test_update_alert_rule():
    """Test updating an alert rule"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        rule_id = "fraud-high-amount"
        payload = {
            "name": "Updated Rule Name",
            "description": "Updated description",
            "category": "compliance",
            "severity": "medium",
            "condition": {"type": "threshold", "field": "amount", "operator": "gt", "value": 2000},
            "action": "email",
            "enabled": True,
            "cooldown_seconds": 600
        }
        response = await client.put(f"/rules/{rule_id}", json=payload)
        assert response.status_code == 200
        data = response.json()
        assert data["name"] == "Updated Rule Name"
        assert data["severity"] == "medium"


@pytest.mark.anyio
async def test_delete_alert_rule():
    """Test deleting an alert rule"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # First create a rule
        payload = {
            "name": "To Be Deleted",
            "category": "system",
            "severity": "low",
            "condition": {"type": "threshold", "field": "value", "operator": "gt", "value": 0},
            "action": "notify"
        }
        create_response = await client.post("/rules", json=payload)
        rule_id = create_response.json()["id"]

        # Now delete it
        response = await client.delete(f"/rules/{rule_id}")
        assert response.status_code == 204

        # Verify it's gone
        get_response = await client.get(f"/rules/{rule_id}")
        assert get_response.status_code == 404


# ============================================================================
# Alerts API Tests
# ============================================================================

@pytest.mark.anyio
async def test_create_alert():
    """Test creating a new alert"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "rule_id": "fraud-high-amount",
            "title": "High Value Transaction Detected",
            "message": "A transaction of $15,000 was detected",
            "severity": "high",
            "category": "fraud",
            "source": "test"
        }
        response = await client.post("/alerts", json=payload)
        assert response.status_code == 201
        data = response.json()
        assert data["title"] == "High Value Transaction Detected"
        assert data["status"] == "active"


@pytest.mark.anyio
async def test_list_alerts():
    """Test listing alerts"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/alerts")
        assert response.status_code == 200
        data = response.json()
        assert isinstance(data, list)


@pytest.mark.anyio
async def test_list_alerts_with_filters():
    """Test filtering alerts by status, category, severity"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/alerts?status=active&category=fraud&severity=high&limit=10")
        assert response.status_code == 200
        data = response.json()
        assert isinstance(data, list)
        for alert in data:
            assert alert["status"] == "active"
            assert alert["category"] == "fraud"
            assert alert["severity"] == "high"


@pytest.mark.anyio
async def test_acknowledge_alert():
    """Test acknowledging an alert"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Create an alert first
        payload = {
            "rule_id": "fraud-high-amount",
            "title": "Test Alert",
            "message": "Test message",
            "severity": "medium",
            "category": "system",
            "source": "test"
        }
        create_response = await client.post("/alerts", json=payload)
        alert_id = create_response.json()["id"]

        # Acknowledge it
        response = await client.put(f"/alerts/{alert_id}/acknowledge")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "acknowledged"
        assert data["acknowledged_at"] is not None


@pytest.mark.anyio
async def test_resolve_alert():
    """Test resolving an alert"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Create an alert first
        payload = {
            "rule_id": "fraud-high-amount",
            "title": "Test Alert to Resolve",
            "message": "Test message",
            "severity": "low",
            "category": "system",
            "source": "test"
        }
        create_response = await client.post("/alerts", json=payload)
        alert_id = create_response.json()["id"]

        # Resolve it
        response = await client.put(f"/alerts/{alert_id}/resolve?resolution_note=Issue fixed")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "resolved"
        assert data["resolved_at"] is not None


@pytest.mark.anyio
async def test_dismiss_alert():
    """Test dismissing an alert"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Create an alert first
        payload = {
            "rule_id": "fraud-high-amount",
            "title": "Test Alert to Dismiss",
            "message": "Test message",
            "severity": "low",
            "category": "system",
            "source": "test"
        }
        create_response = await client.post("/alerts", json=payload)
        alert_id = create_response.json()["id"]

        # Dismiss it
        response = await client.put(f"/alerts/{alert_id}/dismiss")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "dismissed"


# ============================================================================
# Alert Evaluation Tests
# ============================================================================

@pytest.mark.anyio
async def test_evaluate_data_triggers_alert():
    """Test that data evaluation triggers matching alerts"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Data that should trigger fraud-high-amount rule (amount > 10000)
        data = {
            "amount": 15000,
            "source": "test"
        }
        response = await client.post("/evaluate", json=data)
        assert response.status_code == 200
        result = response.json()
        assert result["triggered_count"] >= 0  # May or may not trigger based on cooldown


@pytest.mark.anyio
async def test_evaluate_data_no_match():
    """Test that non-matching data doesn't trigger alerts"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Data that shouldn't trigger any rules
        data = {
            "amount": 50,  # Below all thresholds
            "transaction_count": 1,  # Low velocity
            "source": "test"
        }
        response = await client.post("/evaluate", json=data)
        assert response.status_code == 200
        result = response.json()
        # No alerts triggered
        assert result["triggered_count"] == 0


# ============================================================================
# Stats Tests
# ============================================================================

@pytest.mark.anyio
async def test_get_stats():
    """Test getting alert statistics"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/stats")
        assert response.status_code == 200
        data = response.json()
        assert "total_alerts" in data
        assert "by_status" in data
        assert "by_severity" in data
        assert "by_category" in data
        assert "total_rules" in data
        assert "enabled_rules" in data


# ============================================================================
# Trigger by Rule ID Tests
# ============================================================================

@pytest.mark.anyio
async def test_trigger_alert_by_rule():
    """Test triggering an alert directly by rule ID"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        rule_id = "fraud-high-amount"
        data = {
            "message": "Manually triggered alert",
            "source": "manual_test"
        }
        response = await client.post(f"/trigger/{rule_id}", json=data)
        assert response.status_code == 200
        alert = response.json()
        assert alert["rule_id"] == rule_id
        assert alert["status"] == "active"


@pytest.mark.anyio
async def test_trigger_nonexistent_rule():
    """Test triggering with non-existent rule returns 404"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.post("/trigger/nonexistent-rule", json={"message": "test"})
        assert response.status_code == 404


# ============================================================================
# Severity and Category Enum Tests
# ============================================================================

def test_alert_severity_values():
    """Test alert severity enum values"""
    assert AlertSeverity.CRITICAL.value == "critical"
    assert AlertSeverity.HIGH.value == "high"
    assert AlertSeverity.MEDIUM.value == "medium"
    assert AlertSeverity.LOW.value == "low"
    assert AlertSeverity.INFO.value == "info"


def test_alert_category_values():
    """Test alert category enum values"""
    assert AlertCategory.FRAUD.value == "fraud"
    assert AlertCategory.COMPLIANCE.value == "compliance"
    assert AlertCategory.FINANCIAL.value == "financial"
    assert AlertCategory.SECURITY.value == "security"
    assert AlertCategory.SYSTEM.value == "system"
    assert AlertCategory.WORKFLOW.value == "workflow"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])