# Vimbai Database Migrations

## Overview

Migrations are managed using a simple versioned SQL/Python script approach.
Each migration is numbered and applied in order. The system tracks applied
migrations in a `_migrations` table.

## Structure

```
migrations/
  versions/          # Versioned migration scripts
    001_initial.py
    002_add_neo4j_indexes.py
    ...
  scripts/
    migrate.py      # Migration runner
    rollback.py     # Rollback runner
```

## Usage

```bash
# Apply all pending migrations
python migrations/scripts/migrate.py

# Rollback last migration
python migrations/scripts/rollback.py

# Check migration status
python migrations/scripts/migrate.py --status
```

## Writing a Migration

1. Create a file in `versions/` with format: `NNN_description.py`
2. The file must have `upgrade()` and `downgrade()` functions
3. Use the `neo4j` driver or raw Cypher queries

Example:
```python
def upgrade(neo4j_session):
    neo4j_session.run("CREATE INDEX IF NOT EXISTS FOR (n:Account) ON (n.account_number)")

def downgrade(neo4j_session):
    pass  # Neo4j indexes can be dropped
```
