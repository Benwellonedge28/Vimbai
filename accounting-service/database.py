"""
Neo4j Database Connection with Connection Pooling
Implements connection pool management, health checks, and graceful shutdown
"""
import os
import logging
from neo4j import AsyncGraphDatabase
from neo4j.exceptions import ServiceUnavailable, AuthError

logger = logging.getLogger(__name__)

class Neo4jConnector:
    """Singleton Neo4j connector with connection pooling."""
    _driver = None
    _config = None

    @classmethod
    def _get_config(cls):
        if cls._config is None:
            cls._config = {
                "uri": os.getenv("NEO4J_URI", "bolt://localhost:7687"),
                "user": os.getenv("NEO4J_USER", "neo4j"),
                "password": os.environ["NEO4J_PASSWORD"],  # Fail-fast if not set
                "max_connection_pool_size": int(os.getenv("NEO4J_MAX_POOL_SIZE", "100")),
                "max_connection_lifetime": int(os.getenv("NEO4J_CONN_LIFETIME", "3600")),
                "connection_timeout": int(os.getenv("NEO4J_CONN_TIMEOUT", "30")),
                "max_transaction_retry_time": int(os.getenv("NEO4J_RETRY_TIME", "30")),
            }
        return cls._config

    @classmethod
    def get_driver(cls):
        if cls._driver is None:
            config = cls._get_config()
            cls._driver = AsyncGraphDatabase.driver(
                config["uri"],
                auth=(config["user"], config["password"]),
                max_connection_pool_size=config["max_connection_pool_size"],
                max_connection_lifetime=config["max_connection_lifetime"],
                connection_timeout=config["connection_timeout"],
                max_transaction_retry_time=config["max_transaction_retry_time"],
            )
            logger.info(f"Neo4j driver created with pool size {config['max_connection_pool_size']}")
        return cls._driver

    @classmethod
    async def verify_connectivity(cls):
        """Health check - verify database connectivity."""
        try:
            await cls.get_driver().verify_connectivity()
            return True
        except (ServiceUnavailable, AuthError) as e:
            logger.error(f"Neo4j connectivity check failed: {e}")
            return False

    @classmethod
    async def close_driver(cls):
        """Graceful shutdown - close the driver and release all connections."""
        if cls._driver is not None:
            await cls._driver.close()
            cls._driver = None
            logger.info("Neo4j driver closed")

    @classmethod
    async def initialize_schema(cls):
        """Run initial schema creation if needed."""
        from migrations import run_migrations
        async with cls.get_driver().session() as session:
            await run_migrations(session)
