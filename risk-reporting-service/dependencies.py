"""FastAPI dependencies for Risk Reporting Service"""

from contextvars import ContextVar
from typing import Optional

from fastapi import Header

# Request-scoped Book context (X-Book-ID verified by the API gateway).
# None means personal / unscoped requests.
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)


async def get_user_id(x_user_id: str = Header(...)) -> str:
    """Extract user ID from header (injected by the API gateway)."""
    return x_user_id


async def get_db_session():
    from risk_reporting_service.database import Neo4jConnector

    async with Neo4jConnector.get_driver().session() as session:
        yield session
