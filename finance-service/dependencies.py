from fastapi import Depends, HTTPException, Request, status
from finance_service.database import Neo4jConnector
from finance_service.utils.auth import get_current_user_claims
from neo4j import AsyncSession


async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w")  # Default to write access
    try:
        yield session
    finally:
        await session.close()


# This dependency extracts the JWT token from the request for internal service calls
async def get_jwt_token(request: Request, claims: dict = Depends(get_current_user_claims)) -> str:
    auth_header = request.headers.get("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="JWT token missing or invalid format")
    return auth_header.split(" ")[1]
