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
            print("Neo4j driver initialized for Banking Integration Service.")
        return cls._driver

    @classmethod
    def close_driver(cls):
        if cls._driver is not None:
            cls._driver.close()
            cls._driver = None
            print("Neo4j driver closed for Banking Integration Service.")


async def init_db_schema():
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (ba:BankAccount) ASSERT ba.account_id IS UNIQUE")
        await session.run("CREATE CONSTRAINT IF NOT EXISTS ON (bt:BankTransaction) ASSERT bt.transaction_id IS UNIQUE")
        print("Neo4j Banking Integration schema constraints ensured.")
