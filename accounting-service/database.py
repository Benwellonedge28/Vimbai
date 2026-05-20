import os
from neo4j import GraphDatabase

class Neo4jConnector:
    _driver = None

    @classmethod
    def get_driver(cls):
        if cls._driver is None:
            uri = os.getenv("NEO4J_URI", "bolt://localhost:7687")
            user = os.getenv("NEO4J_USER", "neo4j")
            password = os.getenv("NEO4J_PASSWORD", "neo4j")
            cls._driver = GraphDatabase.driver(uri, auth=(user, password))
            cls._driver.verify_connectivity()
            print("Neo4j driver initialized.")
        return cls._driver

    @classmethod
    def close_driver(cls):
        if cls._driver is not None:
            cls._driver.close()
            cls._driver = None
            print("Neo4j driver closed.")

async def init_db_schema():
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        # Ensure unique constraint for Account number
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (a:Account) ASSERT a.account_number IS UNIQUE")
        # Ensure unique constraint for Account name (within a company/context, but for now just unique)
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (a:Account) ASSERT a.account_name IS UNIQUE")
        print("Neo4j Accounting schema constraints ensured.")
