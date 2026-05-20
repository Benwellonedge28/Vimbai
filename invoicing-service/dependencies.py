from neo4j import AsyncSession
from invoicing_service.database import Neo4jConnector
from fastapi import Depends, HTTPException, status, Request
from invoicing_service.utils.auth import get_current_user_claims

async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w") # Default to write access
    try:
        yield session
    finally:
        await session.close()

async def get_user_id(claims: dict = Depends(get_current_user_claims)) -> str:
    return claims["user_id"]

async def get_jwt_token(request: Request, claims: dict = Depends(get_current_user_claims)) -> str:
    auth_header = request.headers.get("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="JWT token missing or invalid format")
    return auth_header.split(" ")[1]
