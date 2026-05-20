from neo4j import AsyncSession
from accounting_service.database import Neo4jConnector

async def get_db_session() -> AsyncSession:
    driver = Neo4jConnector.get_driver()
    session = driver.session(database="neo4j", default_access_mode="w") # Default to write access
    try:
        yield session
    finally:
        await session.close()
