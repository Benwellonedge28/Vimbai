from banking_integration_service.database import Neo4jConnector
from banking_integration_service.utils.auth import get_current_user_claims
from fastapi import Depends, HTTPException, Request, status
from neo4j import AsyncSession


async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w")  # Default to write access
    try:
        yield session
    finally:
        await session.close()


async def get_user_id(claims: dict = Depends(get_current_user_claims)) -> str:
    return claims["user_id"]
