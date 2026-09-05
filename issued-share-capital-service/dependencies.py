"""FastAPI dependencies for Issued Share Capital Service"""

from contextvars import ContextVar
from typing import Optional

from fastapi import Header, HTTPException, status

# Request-scoped Book context (X-Book-ID verified by the API gateway).
# None means personal / unscoped requests.
book_id_var: ContextVar[Optional[str]] = ContextVar("book_id", default=None)


async def get_user_id(x_user_id: str = Header(...)) -> str:
    """Extract user ID from header (injected by the API gateway)."""
    return x_user_id


async def get_db_session():
    from issued_share_capital_service.database import Neo4jConnector

    async with Neo4jConnector.get_driver().session() as session:
        yield session


def require_positive_limit(limit: int) -> int:
    if limit < 1:
        raise HTTPException(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail="limit must be >= 1")
    return limit
