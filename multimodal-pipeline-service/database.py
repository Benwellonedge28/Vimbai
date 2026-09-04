"""Neo4j connection management for the Multimodal Pipeline Service."""

import os

from neo4j import AsyncGraphDatabase


class Neo4jConnector:
    _driver = None
    _config = {}

    @classmethod
    def configure(cls, uri: str = None, user: str = None, password: str = None):
        """Store connection settings (used at service startup)."""
        cls._config = {
            "uri": uri or os.getenv("NEO4J_URI", "bolt://localhost:7687"),
            "user": user or os.getenv("NEO4J_USER", "neo4j"),
            "password": password or os.getenv("NEO4J_PASSWORD", "neo4j"),
        }

    @classmethod
    def get_driver(cls):
        if cls._driver is None:
            config = cls._config or {
                "uri": os.getenv("NEO4J_URI", "bolt://localhost:7687"),
                "user": os.getenv("NEO4J_USER", "neo4j"),
                "password": os.getenv("NEO4J_PASSWORD", "neo4j"),
            }
            cls._driver = AsyncGraphDatabase.driver(config["uri"], auth=(config["user"], config["password"]))
        return cls._driver

    @classmethod
    async def close_driver(cls):
        if cls._driver is not None:
            await cls._driver.close()
            cls._driver = None


async def init_db_schema():
    """Create constraints for the MultimodalProcessingTask label."""
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        await session.run(
            "CREATE CONSTRAINT multimodal_task_id IF NOT EXISTS "
            "FOR (mpt:MultimodalProcessingTask) REQUIRE mpt.id IS UNIQUE"
        )
