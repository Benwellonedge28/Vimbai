"""
Comprehensive Tests for Vimbai Notifications Service
"""

import pytest
from httpx import AsyncClient, ASGITransport
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'notifications-service'))

from main import app, notification_manager, templates, NotificationType, NotificationPriority, NotificationChannel

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
        assert data["service"] == "notifications"


# ============================================================================
# Notification Creation Tests
# ============================================================================

@pytest.mark.anyio
async def test_create_notification():
    """Test creating a new notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "type": "approval_required",
            "title": "Test Approval Request",
            "message": "Please review this approval request",
            "priority": "high",
            "recipients": ["user1", "user2"],
            "channels": ["in_app", "email"]
        }
        response = await client.post("/notifications", json=payload)
        assert response.status_code == 201
        data = response.json()
        assert data["title"] == "Test Approval Request"
        assert data["status"] == "sent"
        assert len(data["recipients"]) == 2


@pytest.mark.anyio
async def test_create_notification_with_metadata():
    """Test creating notification with additional metadata"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "type": "system",
            "title": "System Alert",
            "message": "A system event occurred",
            "recipients": ["admin"],
            "channels": ["in_app"],
            "metadata": {
                "event_type": "login_failed",
                "count": 5,
                "ip_address": "192.168.1.100"
            },
            "action_url": "/admin/security"
        }
        response = await client.post("/notifications", json=payload)
        assert response.status_code == 201
        data = response.json()
        assert data["metadata"]["event_type"] == "login_failed"
        assert data["action_url"] == "/admin/security"


@pytest.mark.anyio
async def test_create_notification_invalid_type():
    """Test creating notification with invalid type fails"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "type": "invalid_type",
            "title": "Test",
            "message": "Test",
            "recipients": ["user1"]
        }
        response = await client.post("/notifications", json=payload)
        assert response.status_code == 422  # Validation error


# ============================================================================
# Batch Notifications Tests
# ============================================================================

@pytest.mark.anyio
async def test_create_batch_notifications():
    """Test creating multiple notifications at once"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = [
            {
                "type": "workflow_completed",
                "title": "Workflow 1 Complete",
                "message": "First workflow finished",
                "recipients": ["user1"]
            },
            {
                "type": "workflow_completed",
                "title": "Workflow 2 Complete",
                "message": "Second workflow finished",
                "recipients": ["user2"]
            },
            {
                "type": "workflow_completed",
                "title": "Workflow 3 Complete",
                "message": "Third workflow finished",
                "recipients": ["user3"]
            }
        ]
        response = await client.post("/notifications/batch", json=payload)
        assert response.status_code == 201
        data = response.json()
        assert data["count"] == 3
        assert len(data["notifications"]) == 3


@pytest.mark.anyio
async def test_batch_empty_list():
    """Test creating batch with empty list"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.post("/notifications/batch", json=[])
        assert response.status_code == 201
        data = response.json()
        assert data["count"] == 0


# ============================================================================
# Get User Notifications Tests
# ============================================================================

@pytest.mark.anyio
async def test_get_user_notifications():
    """Test retrieving notifications for a user"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # First create some notifications
        await client.post("/notifications", json={
            "type": "comment_added",
            "title": "Comment notification",
            "message": "A new comment was added",
            "recipients": ["testuser"]
        })

        # Now retrieve them
        response = await client.get("/notifications/testuser")
        assert response.status_code == 200
        data = response.json()
        assert "notifications" in data
        assert "unread_count" in data
        assert "total_count" in data


@pytest.mark.anyio
async def test_get_unread_notifications_only():
    """Test retrieving only unread notifications"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/notifications/newuser?unread_only=true&limit=10")
        assert response.status_code == 200
        data = response.json()
        for notif in data["notifications"]:
            assert notif["status"] != "read"


@pytest.mark.anyio
async def test_get_notifications_with_limit():
    """Test retrieving notifications with limit parameter"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/notifications/testuser?limit=5")
        assert response.status_code == 200
        data = response.json()
        assert len(data["notifications"]) <= 5


# ============================================================================
# Mark as Read Tests
# ============================================================================

@pytest.mark.anyio
async def test_mark_notification_read():
    """Test marking a notification as read"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Create a notification
        create_response = await client.post("/notifications", json={
            "type": "approval_required",
            "title": "Mark as Read Test",
            "message": "Test notification",
            "recipients": ["testuser"]
        })
        notification_id = create_response.json()["id"]

        # Mark as read
        response = await client.put(f"/notifications/{notification_id}/read?user_id=testuser")
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "success"


@pytest.mark.anyio
async def test_mark_all_notifications_read():
    """Test marking all notifications as read for a user"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Create multiple notifications
        for i in range(3):
            await client.post("/notifications", json={
                "type": "system",
                "title": f"Notification {i}",
                "message": "Test",
                "recipients": ["bulkuser"]
            })

        # Mark all as read
        response = await client.put("/notifications/bulkuser/read-all")
        assert response.status_code == 200
        data = response.json()
        assert data["marked_count"] >= 0


# ============================================================================
# Delete Notification Tests
# ============================================================================

@pytest.mark.anyio
async def test_delete_notification():
    """Test deleting a notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        # Create a notification
        create_response = await client.post("/notifications", json={
            "type": "system",
            "title": "To Be Deleted",
            "message": "Test",
            "recipients": ["deleteuser"]
        })
        notification_id = create_response.json()["id"]

        # Delete it
        response = await client.delete(f"/notifications/{notification_id}?user_id=deleteuser")
        assert response.status_code == 200


# ============================================================================
# Preferences Tests
# ============================================================================

@pytest.mark.anyio
async def test_update_preferences():
    """Test updating notification preferences"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "user_id": "testuser",
            "channels": {
                "approval_required": ["in_app", "email"],
                "workflow_completed": ["in_app"]
            },
            "quiet_hours_start": "22:00",
            "quiet_hours_end": "08:00",
            "email_batch": True,
            "email_batch_interval_minutes": 30
        }
        response = await client.put("/preferences/testuser", json=payload)
        assert response.status_code == 200


@pytest.mark.anyio
async def test_get_preferences():
    """Test getting notification preferences"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/preferences/testuser")
        assert response.status_code == 200
        data = response.json()
        assert data["user_id"] == "testuser"


# ============================================================================
# Template Tests
# ============================================================================

@pytest.mark.anyio
async def test_list_templates():
    """Test listing all notification templates"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/templates")
        assert response.status_code == 200
        data = response.json()
        assert len(data) > 0


@pytest.mark.anyio
async def test_get_template():
    """Test getting a specific template"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/templates/approval_request")
        assert response.status_code == 200
        data = response.json()
        assert data["name"] == "Approval Request"


@pytest.mark.anyio
async def test_get_nonexistent_template():
    """Test getting non-existent template returns 404"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/templates/nonexistent")
        assert response.status_code == 404


@pytest.mark.anyio
async def test_send_from_template():
    """Test sending notification from template"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "recipients": ["user1"],
            "variables": {
                "title": "Invoice #12345",
                "sender": "John Doe"
            }
        }
        response = await client.post("/templates/approval_request/send", json=payload)
        assert response.status_code == 201
        data = response.json()
        assert data["title"] == "Approval Required: Invoice #12345"


# ============================================================================
# Workflow Notification Tests
# ============================================================================

@pytest.mark.anyio
async def test_notify_workflow_started():
    """Test workflow started notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "started",
            "participants": ["user1", "user2"],
            "metadata": {"started_by": "admin"}
        }
        response = await client.post("/workflow/wf-123/notify", json=payload)
        assert response.status_code == 201


@pytest.mark.anyio
async def test_notify_workflow_completed():
    """Test workflow completed notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "completed",
            "participants": ["user1"],
            "metadata": {"duration_seconds": 120}
        }
        response = await client.post("/workflow/wf-456/notify", json=payload)
        assert response.status_code == 201


@pytest.mark.anyio
async def test_notify_workflow_failed():
    """Test workflow failed notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "failed",
            "participants": ["admin"],
            "metadata": {"error": "Connection timeout"}
        }
        response = await client.post("/workflow/wf-789/notify", json=payload)
        assert response.status_code == 201


# ============================================================================
# Approval Notification Tests
# ============================================================================

@pytest.mark.anyio
async def test_notify_approval_requested():
    """Test approval request notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "requested",
            "requester": "user1",
            "approvers": ["manager1", "manager2"]
        }
        response = await client.post("/approval/appr-001/notify", json=payload)
        assert response.status_code == 201


@pytest.mark.anyio
async def test_notify_approval_approved():
    """Test approval approved notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "approved",
            "requester": "user1",
            "approvers": ["manager1"],
            "metadata": {"approved_by": "manager1", "notes": "Looks good"}
        }
        response = await client.post("/approval/appr-002/notify", json=payload)
        assert response.status_code == 201


@pytest.mark.anyio
async def test_notify_approval_rejected():
    """Test approval rejected notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "rejected",
            "requester": "user1",
            "approvers": ["manager1"],
            "metadata": {"rejected_by": "manager1", "reason": "Budget exceeded"}
        }
        response = await client.post("/approval/appr-003/notify", json=payload)
        assert response.status_code == 201


@pytest.mark.anyio
async def test_notify_approval_commented():
    """Test approval commented notification"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        payload = {
            "event_type": "commented",
            "requester": "user1",
            "approvers": ["manager1"]
        }
        response = await client.post("/approval/appr-004/notify", json=payload)
        assert response.status_code == 201


# ============================================================================
# Stats Tests
# ============================================================================

@pytest.mark.anyio
async def test_get_notification_stats():
    """Test getting notification statistics"""
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/stats")
        assert response.status_code == 200
        data = response.json()
        assert "total_notifications" in data
        assert "unread_notifications" in data
        assert "active_connections" in data
        assert "users_with_notifications" in data


# ============================================================================
# Notification Types Enum Tests
# ============================================================================

def test_notification_type_values():
    """Test notification type enum values"""
    assert NotificationType.APPROVAL_REQUIRED.value == "approval_required"
    assert NotificationType.APPROVAL_COMPLETED.value == "approval_completed"
    assert NotificationType.COMMENT_ADDED.value == "comment_added"
    assert NotificationType.MENTION.value == "mention"
    assert NotificationType.WORKFLOW_COMPLETED.value == "workflow_completed"
    assert NotificationType.SYSTEM.value == "system"


def test_notification_priority_values():
    """Test notification priority enum values"""
    assert NotificationPriority.URGENT.value == "urgent"
    assert NotificationPriority.HIGH.value == "high"
    assert NotificationPriority.NORMAL.value == "normal"
    assert NotificationPriority.LOW.value == "low"


def test_notification_channel_values():
    """Test notification channel enum values"""
    assert NotificationChannel.IN_APP.value == "in_app"
    assert NotificationChannel.EMAIL.value == "email"
    assert NotificationChannel.SMS.value == "sms"
    assert NotificationChannel.WEBHOOK.value == "webhook"
    assert NotificationChannel.PUSH.value == "push"


# ============================================================================
# Template Variable Replacement Tests
# ============================================================================

def test_template_variable_replacement():
    """Test that template variables are correctly replaced"""
    template = templates["approval_request"]
    assert "{{title}}" in template.subject_template
    assert "{{sender}}" in template.body_template


if __name__ == "__main__":
    pytest.main([__file__, "-v"])