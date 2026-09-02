"""
Database Migration System for Vimbai
Runs migrations in order, tracks applied migrations in Neo4j
"""

import importlib
import logging
import os
from typing import List

logger = logging.getLogger(__name__)

MIGRATIONS_DIR = os.path.dirname(__file__)


async def run_migrations(session):
    """Run all pending migrations in order."""
    # Ensure Migration node constraint exists
    await session.run("CREATE CONSTRAINT migration_id_unique IF NOT EXISTS FOR (m:Migration) REQUIRE m.id IS UNIQUE")

    # Get applied migrations
    result = await session.run("MATCH (m:Migration) RETURN m.id AS id")
    applied = set()
    records = await result.values("id")
    for record in records:
        applied.add(record)

    # Find and sort migration files
    migration_files = []
    for filename in sorted(os.listdir(MIGRATIONS_DIR)):
        if filename.startswith("__") or not filename.endswith(".py"):
            continue
        if filename == "__init__.py":
            continue
        migration_files.append(filename)

    pending_count = 0
    for filename in migration_files:
        module_name = filename[:-3]  # Remove .py
        migration_id = module_name.split("_")[0]

        if migration_id in applied:
            logger.debug(f"Migration {migration_id} already applied, skipping")
            continue

        try:
            module = importlib.import_module(f"migrations.{module_name}")
            logger.info(f"Running migration {migration_id}: {getattr(module, 'MIGRATION_NAME', 'unknown')}")
            await module.up(session)
            pending_count += 1
        except Exception as e:
            logger.error(f"Migration {migration_id} failed: {e}")
            raise

    if pending_count == 0:
        logger.info("All migrations already applied")
    else:
        logger.info(f"Applied {pending_count} migrations")


async def rollback_migration(session, migration_id):
    """Rollback a specific migration."""
    module_name = None
    for filename in sorted(os.listdir(MIGRATIONS_DIR)):
        if filename.startswith(str(migration_id).zfill(3)) and filename.endswith(".py"):
            module_name = filename[:-3]
            break

    if not module_name:
        raise ValueError(f"Migration {migration_id} not found")

    module = importlib.import_module(f"migrations.{module_name}")
    await module.down(session)
    logger.info(f"Rolled back migration {migration_id}")
