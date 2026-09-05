"""Database configuration and Neo4j connection for Ordinary Shares Service"""

import os
from typing import Optional

from neo4j import AsyncDriver, AsyncGraphDatabase


class Neo4jConnector:
    """Neo4j database connector singleton"""

    _driver: Optional[AsyncDriver] = None
    _uri = os.getenv("NEO4J_URI", "bolt://localhost:7687")
    _user = os.getenv("NEO4J_USER", "neo4j")
    _password = os.getenv("NEO4J_PASSWORD", "password")

    @classmethod
    def configure(cls, uri: str, user: str, password: str):
        cls._uri = uri
        cls._user = user
        cls._password = password

    @classmethod
    def get_driver(cls) -> AsyncDriver:
        if cls._driver is None:
            cls._driver = AsyncGraphDatabase.driver(cls._uri, auth=(cls._user, cls._password))
        return cls._driver

    @classmethod
    async def close_driver(cls):
        if cls._driver:
            await cls._driver.close()
            cls._driver = None
