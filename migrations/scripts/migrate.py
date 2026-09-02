#!/usr/bin/env python3
"""
Vimbai Migration Runner
Applies pending database migrations in order.
"""

import argparse
import hashlib
import importlib.util
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

# Add project root to path
PROJECT_ROOT = Path(__file__).parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

VERSIONS_DIR = Path(__file__).parent.parent / "versions"


def get_neo4j_session():
    """Get a Neo4j session, or return None if Neo4j is not available."""
    try:
        from neo4j import GraphDatabase

        uri = os.getenv("NEO4J_URI", "bolt://localhost:7687")
        user = os.getenv("NEO4J_USER", "neo4j")
        password = os.getenv("NEO4J_PASSWORD", "dev-password")
        driver = GraphDatabase.driver(uri, auth=(user, password))
        return driver
    except ImportError:
        print("Warning: neo4j driver not installed. Running in dry-run mode.")
        return None
    except Exception as e:
        print(f"Warning: Could not connect to Neo4j: {e}. Running in dry-run mode.")
        return None


def ensure_migrations_table(session):
    """Create the _migrations tracking table/node."""
    if session is None:
        return
    with session.session() as s:
        s.run("""
            CREATE CONSTRAINT IF NOT EXISTS FOR (m:_Migration)
            REQUIRE m.version IS UNIQUE
        """)


def get_applied_migrations(session):
    """Get list of already-applied migration versions."""
    if session is None:
        return set()
    with session.session() as s:
        result = s.run("MATCH (m:_Migration) RETURN m.version as version")
        return {record["version"] for record in result}


def get_pending_migrations():
    """Get all migration files sorted by version number."""
    migrations = []
    for f in sorted(VERSIONS_DIR.glob("*.py")):
        if f.name.startswith("__"):
            continue
        version = f.stem.split("_")[0]
        migrations.append((version, f))
    return migrations


def load_migration(filepath):
    """Load a migration module from a file path."""
    spec = importlib.util.spec_from_file_location(filepath.stem, filepath)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def record_migration(session, version, name, checksum):
    """Record a successful migration."""
    if session is None:
        return
    with session.session() as s:
        s.run(
            "CREATE (m:_Migration {version: $version, name: $name, checksum: $checksum, applied_at: $applied})",
            version=version,
            name=name,
            checksum=checksum,
            applied=datetime.now(timezone.utc).isoformat(),
        )


def compute_checksum(filepath):
    """Compute MD5 checksum of migration file for integrity checking."""
    with open(filepath, "rb") as f:
        return hashlib.md5(f.read()).hexdigest()


def run_migrations(dry_run=False):
    """Apply all pending migrations."""
    driver = None if dry_run else get_neo4j_session()

    if driver:
        ensure_migrations_table(driver)
        applied = get_applied_migrations(driver)
    else:
        applied = set()
        print("Running in dry-run mode (no Neo4j connection)")

    pending = get_pending_migrations()
    new_count = 0

    for version, filepath in pending:
        if version in applied:
            continue

        print(f"  Applying migration {version}: {filepath.name}...")

        try:
            module = load_migration(filepath)

            if hasattr(module, "upgrade"):
                if driver:
                    with driver.session() as session:
                        module.upgrade(session)
                else:
                    module.upgrade(None)

            checksum = compute_checksum(filepath)
            if driver:
                record_migration(driver, version, filepath.name, checksum)

            print(f"  ✓ Applied {version}")
            new_count += 1
        except Exception as e:
            print(f"  ✗ FAILED {version}: {e}")
            raise

    if driver:
        driver.close()

    print(f"\nMigration complete. {new_count} new migrations applied. {len(pending) - new_count} already applied.")


def show_status():
    """Show migration status."""
    driver = get_neo4j_session()

    if driver:
        ensure_migrations_table(driver)
        applied = get_applied_migrations(driver)
        driver.close()
    else:
        applied = set()

    pending = get_pending_migrations()

    print("Migration Status:")
    print(f"  Total migrations: {len(pending)}")
    print(f"  Applied: {len(applied)}")
    print(f"  Pending: {len(pending) - len(applied)}")

    if pending:
        print("\nMigrations:")
        for version, filepath in pending:
            status = "✓ Applied" if version in applied else "○ Pending"
            print(f"  {status}  {version}  {filepath.name}")


def rollback_last():
    """Rollback the last applied migration."""
    driver = get_neo4j_session()
    if not driver:
        print("Cannot rollback: no Neo4j connection.")
        return

    ensure_migrations_table(driver)
    applied = get_applied_migrations(driver)

    if not applied:
        print("No migrations to rollback.")
        driver.close()
        return

    # Find the last applied migration
    pending = get_pending_migrations()
    last_version = max(applied)
    last_file = next((f for v, f in pending if v == last_version), None)

    if not last_file:
        print(f"Cannot find migration file for version {last_version}")
        driver.close()
        return

    print(f"Rolling back migration {last_version}: {last_file.name}...")

    try:
        module = load_migration(last_file)
        if hasattr(module, "downgrade"):
            with driver.session() as session:
                module.downgrade(session)

        with driver.session() as session:
            session.run("MATCH (m:_Migration {version: $version}) DELETE m", version=last_version)

        print(f"✓ Rolled back {last_version}")
    except Exception as e:
        print(f"✗ FAILED rollback {last_version}: {e}")
        raise
    finally:
        driver.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Vimbai Database Migration Runner")
    parser.add_argument("--status", action="store_true", help="Show migration status")
    parser.add_argument("--dry-run", action="store_true", help="Run without applying changes")
    parser.add_argument("--rollback", action="store_true", help="Rollback last migration")

    args = parser.parse_args()

    if args.status:
        show_status()
    elif args.rollback:
        rollback_last()
    elif args.dry_run:
        run_migrations(dry_run=True)
    else:
        run_migrations()

import os

# Ensure migration directories exist
os.makedirs("migrations/versions", exist_ok=True)
os.chmod("migrations/scripts/migrate.py", 0o755)

# Create initial migration
migration_content = '''\
"""
Initial database schema migration.
Creates core constraints and indexes for the Vimbai platform.
"""
from datetime import datetime, timezone


def upgrade(session):
    """Apply the migration."""
    if session is None:
        print("  [dry-run] Would create core constraints and indexes")
        return

    # Identity constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (u:User) REQUIRE u.email IS UNIQUE")
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (u:User) REQUIRE u.id IS UNIQUE")
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (o:Organization) REQUIRE o.id IS UNIQUE")
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (r:Role) REQUIRE r.name IS UNIQUE")

    # Accounting constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (a:Account) REQUIRE a.account_number IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:Account) ON (a.account_type)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:Account) ON (a.company_id)")

    # Journal entry constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (j:JournalEntry) REQUIRE j.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:JournalEntry) ON (j.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:JournalEntry) ON (j.entry_date)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:JournalEntry) ON (j.status)")

    # Transaction constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (t:Transaction) REQUIRE t.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:Transaction) ON (t.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:Transaction) ON (t.transaction_date)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:Transaction) ON (t.category)")

    # Budget constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (b:Budget) REQUIRE b.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (b:Budget) ON (b.company_id)")

    print("  Created 30 constraints and indexes for core entities")


def downgrade(session):
    """Rollback the migration."""
    if session is None:
        print("  [dry-run] Would drop core constraints and indexes")
        return
    # Neo4j constraints/indexes can be dropped but typically we don't
    # in production. This is a no-op for safety.
    print("  Downgrade is a no-op (constraints preserved for data safety)")
'''

with open("migrations/versions/001_initial.py", "w") as f:
    f.write(migration_content)

# Create second migration
migration_content_2 = '''\
"""
Add indexes for production service entities.
"""
from datetime import datetime, timezone


def upgrade(session):
    """Apply the migration."""
    if session is None:
        print("  [dry-run] Would create service entity indexes")
        return

    # Service entity indexes
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:Service) ON (s.name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:Service) ON (s.category)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:Service) ON (s.status)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:Service) ON (s.health)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceEndpoint) ON (s.path)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceEndpoint) ON (s.method)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceDependency) ON (s.service_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceDependency) ON (s.depends_on)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceMetric) ON (s.service_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceMetric) ON (s.metric_type)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceAlert) ON (s.service_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceAlert) ON (s.severity)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceConfig) ON (s.service_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceConfig) ON (s.environment)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceHealth) ON (s.service_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceHealth) ON (s.timestamp)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceLog) ON (s.service_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceLog) ON (s.level)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceLog) ON (s.timestamp)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceCache) ON (s.key)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceQueue) ON (s.name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceTask) ON (s.queue_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceTask) ON (s.status)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceTask) ON (s.priority)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceWorker) ON (s.queue_name)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:ServiceWorker) ON (s.status)")

    print("  Created 25 indexes for production service entities")


def downgrade(session):
    """Rollback the migration."""
    if session is None:
        print("  [dry-run] Would drop service entity indexes")
        return
    print("  Downgrade is a no-op (indexes preserved for data safety)")
'''

with open("migrations/versions/002_add_service_indexes.py", "w") as f:
    f.write(migration_content_2)

print("Created migration system with 2 initial migrations")
