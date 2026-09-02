import os

from neo4j import GraphDatabase


class Neo4jConnector:
    _driver = None

    @classmethod
    def configure(cls, uri: str, user: str, password: str):
        cls._config = {"uri": uri, "user": user, "password": password}

    @classmethod
    def get_driver(cls):
        if cls._driver is None:
            config = getattr(cls, "_config", {})
            uri = config.get("uri", os.getenv("NEO4J_URI", "bolt://localhost:7687"))
            user = config.get("user", os.getenv("NEO4J_USER", "neo4j"))
            password = config.get("password", os.getenv("NEO4J_PASSWORD", "neo4j"))
            cls._driver = GraphDatabase.driver(uri, auth=(user, password))
            cls._driver.verify_connectivity()
            print("Reporting Service: Neo4j driver initialized.")
        return cls._driver

    @classmethod
    def close_driver(cls):
        if cls._driver is not None:
            cls._driver.close()
            cls._driver = None
            print("Reporting Service: Neo4j driver closed.")


async def init_db_schema():
    """Initialize Neo4j constraints and indexes for reporting service"""
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (d:Dashboard) ASSERT d.id IS UNIQUE")
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (rt:ReportTemplate) ASSERT rt.id IS UNIQUE")
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (r:Report) ASSERT r.id IS UNIQUE")
        await session.run("CREATE INDEX IF NOT EXISTS FOR (d:Dashboard) ON (d.user_id)")
        print("Reporting Service: Neo4j schema constraints ensured.")
