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
            print("Workflow Service: Neo4j driver initialized.")
        return cls._driver

    @classmethod
    def close_driver(cls):
        if cls._driver is not None:
            cls._driver.close()
            cls._driver = None
            print("Workflow Service: Neo4j driver closed.")


async def init_db_schema():
    """Initialize Neo4j constraints and indexes for workflow service"""
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        # Unique constraints
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (wt:WorkflowTemplate) ASSERT wt.id IS UNIQUE")
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (wi:WorkflowInstance) ASSERT wi.id IS UNIQUE")
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (a:ApprovalAction) ASSERT a.id IS UNIQUE")
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (n:Notification) ASSERT n.id IS UNIQUE")

        # Indexes for performance
        await session.run("CREATE INDEX IF NOT EXISTS FOR (wi:WorkflowInstance) ON (wi.status)")
        await session.run("CREATE INDEX IF NOT EXISTS FOR (wi:WorkflowInstance) ON (wi.entity_id, wi.entity_type)")
        await session.run("CREATE INDEX IF NOT EXISTS FOR (n:Notification) ON (n.recipient_id, n.read)")

        print("Workflow Service: Neo4j schema constraints ensured.")
