from fastapi import Depends, Request, status
from invoicing_service.database import Neo4jConnector
from invoicing_service.exceptions import UnauthorizedError  # NEW
from invoicing_service.utils.auth import get_current_user_claims
from neo4j import AsyncSession


async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w")
    try:
        yield session
    finally:
        await session.close()


async def get_user_id(claims: dict = Depends(get_current_user_claims)) -> str:
    return claims["user_id"]


async def get_jwt_token(request: Request, claims: dict = Depends(get_current_user_claims)) -> str:
    auth_header = request.headers.get("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise UnauthorizedError(
            detail="JWT token missing or invalid format.", code="MISSING_OR_INVALID_TOKEN_FORMAT"
        )  # MODIFIED
    return auth_header.split(" ")[1]
