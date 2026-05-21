from neo4j import AsyncGraphDatabase, AsyncSession

class Neo4jConnector:
    _driver = None
    _uri = None
    _user = None
    _password = None

    @classmethod
    def configure(cls, uri, user, password):
        cls._uri = uri
        cls._user = user
        cls._password = password

    @classmethod
    def get_driver(cls):
        if cls._driver is None:
            if not cls._uri or not cls._user or not cls._password:
                raise Exception("Neo4j connection not configured. Call configure() first.")
            cls._driver = AsyncGraphDatabase.driver(cls._uri, auth=(cls._user, cls._password))
        return cls._driver

    @classmethod
    async def close_driver(cls):
        if cls._driver:
            await cls._driver.close()
            cls._driver = None

async def init_db_schema():
    driver = Neo4jConnector.get_driver()
    async with driver.session() as session:
        await session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (f:FraudulentTransactionFlag) REQUIRE f.id IS UNIQUE")
        # Add any other constraints or indexes as needed
    print("Neo4j schema initialized for Fraud Detection Service.")
