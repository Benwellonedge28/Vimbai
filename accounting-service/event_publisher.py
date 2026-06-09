"""
Message Bus Integration for Accounting Service
=============================================

This module provides event publishing to the Message Bus Service (RabbitMQ).
It publishes events when accounting operations occur.

Usage:
    from accounting_service.event_publisher import publish_event, publish_journal_entry_created
"""

import os
import json
import httpx
from typing import Dict, Any, Optional
from datetime import datetime
import asyncio

# Message Bus Service URL
MESSAGE_BUS_URL = os.getenv("MESSAGE_BUS_URL", "http://message-bus-service:8090")


# =============================================================================
# EVENT TYPES
# =============================================================================

class EventType:
    """Event types for accounting service"""
    JOURNAL_ENTRY_CREATED = "accounting.journal_entry.created"
    JOURNAL_ENTRY_POSTED = "accounting.journal_entry.posted"
    JOURNAL_ENTRY_VOIDED = "accounting.journal_entry.voided"
    ACCOUNT_CREATED = "accounting.account.created"
    ACCOUNT_UPDATED = "accounting.account.updated"
    BUDGET_VARIANCE_ALERT = "finance.budget.variance_alert"
    TRIAL_BALANCE_GENERATED = "accounting.trial_balance.generated"
    FINANCIAL_STATEMENT_GENERATED = "accounting.financial_statement.generated"
    FRAUD_ALERT = "fraud.detection.alert"


# =============================================================================
# EVENT PUBLISHER
# =============================================================================

async def publish_event(
    event_type: str,
    payload: Dict[str, Any],
    user_id: Optional[str] = None,
    idempotency_key: Optional[str] = None
) -> bool:
    """
    Publish an event to the Message Bus Service.

    Args:
        event_type: Type of event (e.g., 'accounting.journal_entry.created')
        payload: Event data payload
        user_id: User who triggered the event
        idempotency_key: Key to prevent duplicate events

    Returns:
        True if event was published successfully, False otherwise
    """
    event_data = {
        "event_type": event_type,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "source_service": "accounting-service",
        "payload": payload,
        "metadata": {
            "user_id": user_id,
            "idempotency_key": idempotency_key,
            "version": "1.0"
        }
    }

    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            response = await client.post(
                f"{MESSAGE_BUS_URL}/events/publish",
                json=event_data
            )
            return response.status_code == 200 or response.status_code == 201
    except Exception as e:
        # Log error but don't fail the main operation
        print(f"Failed to publish event {event_type}: {e}")
        return False


def publish_event_sync(
    event_type: str,
    payload: Dict[str, Any],
    user_id: Optional[str] = None,
    idempotency_key: Optional[str] = None
) -> bool:
    """
    Synchronous wrapper for publish_event (for use in non-async contexts).
    """
    try:
        loop = asyncio.get_event_loop()
        if loop.is_running():
            # If we're in an async context, schedule the coroutine
            asyncio.create_task(publish_event(event_type, payload, user_id, idempotency_key))
            return True
        else:
            return asyncio.run(publish_event(event_type, payload, user_id, idempotency_key))
    except Exception:
        # Fallback: run in a new event loop
        try:
            return asyncio.run(publish_event(event_type, payload, user_id, idempotency_key))
        except Exception as e:
            print(f"Failed to publish event {event_type}: {e}")
            return False


# =============================================================================
# SPECIFIC EVENT PUBLISHERS
# =============================================================================

async def publish_journal_entry_created(
    journal_entry_id: str,
    entry_date: datetime,
    description: str,
    total_amount: float,
    source_module: str,
    user_id: str
) -> bool:
    """Publish event when a journal entry is created"""
    payload = {
        "journal_entry_id": journal_entry_id,
        "entry_date": entry_date.isoformat(),
        "description": description,
        "total_amount": total_amount,
        "source_module": source_module,
        "status": "created"
    }
    return await publish_event(
        EventType.JOURNAL_ENTRY_CREATED,
        payload,
        user_id=user_id,
        idempotency_key=f"je_created_{journal_entry_id}"
    )


async def publish_journal_entry_posted(
    journal_entry_id: str,
    user_id: str
) -> bool:
    """Publish event when a journal entry is posted"""
    payload = {
        "journal_entry_id": journal_entry_id,
        "status": "posted",
        "posted_at": datetime.now(timezone.utc).isoformat()
    }
    return await publish_event(
        EventType.JOURNAL_ENTRY_POSTED,
        payload,
        user_id=user_id,
        idempotency_key=f"je_posted_{journal_entry_id}"
    )


async def publish_account_created(
    account_id: str,
    account_number: str,
    account_name: str,
    account_type: str,
    user_id: str
) -> bool:
    """Publish event when an account is created"""
    payload = {
        "account_id": account_id,
        "account_number": account_number,
        "account_name": account_name,
        "account_type": account_type
    }
    return await publish_event(
        EventType.ACCOUNT_CREATED,
        payload,
        user_id=user_id,
        idempotency_key=f"account_created_{account_id}"
    )


async def publish_trial_balance_generated(
    user_id: str,
    as_of_date: str,
    total_debits: float,
    total_credits: float,
    is_balanced: bool
) -> bool:
    """Publish event when trial balance is generated"""
    payload = {
        "as_of_date": as_of_date,
        "total_debits": total_debits,
        "total_credits": total_credits,
        "is_balanced": is_balanced
    }
    return await publish_event(
        EventType.TRIAL_BALANCE_GENERATED,
        payload,
        user_id=user_id
    )


async def publish_fraud_alert(
    journal_entry_id: str,
    risk_score: float,
    risk_factors: list,
    user_id: str
) -> bool:
    """Publish fraud detection alert"""
    payload = {
        "journal_entry_id": journal_entry_id,
        "risk_score": risk_score,
        "risk_factors": risk_factors,
        "alert_timestamp": datetime.now(timezone.utc).isoformat()
    }
    return await publish_event(
        EventType.FRAUD_ALERT,
        payload,
        user_id=user_id
    )


# =============================================================================
# SUBSCRIPTION HELPERS (for receiving events)
# =============================================================================

async def subscribe_to_events(
    event_types: list,
    callback_url: str
) -> Dict[str, Any]:
    """Subscribe to specific event types"""
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            response = await client.post(
                f"{MESSAGE_BUS_URL}/subscriptions/create",
                json={
                    "subscriber_id": "accounting-service",
                    "callback_url": callback_url,
                    "event_types": event_types
                }
            )
            if response.status_code == 200:
                return response.json()
            return {"success": False, "error": f"Status: {response.status_code}"}
    except Exception as e:
        return {"success": False, "error": str(e)}


# =============================================================================
# HEALTH CHECK
# =============================================================================

async def check_message_bus_health() -> bool:
    """Check if Message Bus Service is healthy"""
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            response = await client.get(f"{MESSAGE_BUS_URL}/health")
            return response.status_code == 200
    except Exception:
        return False