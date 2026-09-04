"""FastAPI dependencies for Expense Tracking Service"""

from contextvars import ContextVar
from typing import Optional

from benefits_admin_service.database import Neo4jConnector
from fastapi import Header
from neo4j import AsyncSession

# Request-scoped Book context (X-Book-ID verified by the API gateway).
# None means personal / unscoped requests.
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)


async def get_user_id(x_user_id: str = Header(...)) -> str:
    """Extract user ID from the X-User-Id header injected by the API gateway."""
    return x_user_id


async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w")
    try:
        yield session
    finally:
        await session.close()
