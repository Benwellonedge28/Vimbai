#!/bin/bash
# Vimbai Neo4j Database Restore Script
set -euo pipefail
BACKUP_FILE="$1"
NEO4J_CONTAINER="${NEO4J_CONTAINER:-neo4j}"
if [ -z "$BACKUP_FILE" ]; then echo "Usage: $0 <backup_file.tar.gz>"; exit 1; fi
if [ ! -f "$BACKUP_FILE" ]; then echo "ERROR: Backup file not found: $BACKUP_FILE"; exit 1; fi
echo "WARNING: This will replace the current Neo4j database!"
read -p "Are you sure? (yes/no): " confirm
if [ "$confirm" != "yes" ]; then echo "Restore cancelled"; exit 0; fi
TEMP_DIR=$(mktemp -d)
echo "Extracting backup to ${TEMP_DIR}..."
tar -xzf "$BACKUP_FILE" -C "$TEMP_DIR"
echo "Stopping Neo4j..."
docker exec "$NEO4J_CONTAINER" neo4j stop 2>/dev/null || true
docker cp "${TEMP_DIR}/." "${NEO4J_CONTAINER}:/tmp/restore/"
echo "Restoring database..."
docker exec "$NEO4J_CONTAINER" neo4j-admin database restore neo4j --from-path=/tmp/restore --overwrite-destination=true 2>&1
docker exec "${NEO4J_CONTAINER}" bash -c "rm -rf /tmp/restore" 2>/dev/null || true
rm -rf "$TEMP_DIR"
echo "Starting Neo4j..."
docker exec "$NEO4J_CONTAINER" neo4j start
echo "Restore completed successfully"
