"""Database configuration and Neo4j connection for Benefits Admin Service"""

import os
from typing import Optional

from neo4j import AsyncDriver, AsyncGraphDatabase


class Neo4jConnector:
    """Neo4j database connector singleton"""

    _driver: Optional[AsyncDriver] = None

    @classmethod
    def configure(cls, uri: str, user: str, password: str):
        """Configure Neo4j connection"""
        cls._uri = uri
        cls._user = user
        cls._password = password

    @classmethod
    def get_driver(cls) -> AsyncDriver:
        """Get Neo4j driver instance"""
        if cls._driver is None:
            cls._driver = AsyncGraphDatabase.driver(cls._uri, auth=(cls._user, cls._password))
        return cls._driver

    @classmethod
    async def close_driver(cls):
        """Close Neo4j driver"""
        if cls._driver:
            await cls._driver.close()
            cls._driver = None

    @classmethod
    async def get_session(cls):
        """Get a new Neo4j session"""
        driver = cls.get_driver()
        return driver.session()


async def get_db_session():
    """Dependency for getting database session"""
    async with Neo4jConnector.get_driver().session() as session:
        yield session


async def init_db_schema():
    """Initialize Neo4j schema constraints and indexes"""
    async with Neo4jConnector.get_driver().session() as session:
        # Create constraints
        constraints = [
            "CREATE CONSTRAINT npo_fund IF NOT EXISTS FOR (f:NPOFund) REQUIRE f.id IS UNIQUE",
            "CREATE CONSTRAINT npo_fund_code IF NOT EXISTS FOR (f:NPOFund) REQUIRE f.fund_code IS UNIQUE",
            "CREATE CONSTRAINT npo_grant IF NOT EXISTS FOR (g:Grant) REQUIRE g.id IS UNIQUE",
            "CREATE CONSTRAINT npo_donor IF NOT EXISTS FOR (d:Donor) REQUIRE d.id IS UNIQUE",
            "CREATE CONSTRAINT npo_budget IF NOT EXISTS FOR (b:Budget) REQUIRE b.id IS UNIQUE",
            "CREATE CONSTRAINT npo_project IF NOT EXISTS FOR (p:Project) REQUIRE p.id IS UNIQUE",
            "CREATE CONSTRAINT npo_program IF NOT EXISTS FOR (p:Program) REQUIRE p.id IS UNIQUE",
        ]

        # Create indexes
        indexes = [
            "CREATE INDEX npo_fund_type IF NOT EXISTS FOR (f:NPOFund) ON (f.fund_type)",
            "CREATE INDEX npo_fund_status IF NOT EXISTS FOR (f:NPOFund) ON (f.status)",
            "CREATE INDEX npo_grant_status IF NOT EXISTS FOR (g:Grant) ON (g.status)",
            "CREATE INDEX npo_budget_period IF NOT EXISTS FOR (b:Budget) ON (b.fiscal_year)",
            "CREATE INDEX npo_project_status IF NOT EXISTS FOR (p:Project) ON (p.status)",
        ]

        for constraint in constraints:
            try:
                await session.run(constraint)
            except Exception:
                pass  # Ignore if constraint already exists

        for index in indexes:
            try:
                await session.run(index)
            except Exception:
                pass  # Ignore if index already exists
