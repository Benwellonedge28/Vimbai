#!/bin/bash
# Vimbai Neo4j Backup Script
# Creates a full backup of the Neo4j database
# Usage: ./scripts/backup_neo4j.sh [backup-dir]
set -euo pipefail

BACKUP_DIR="${1:-/tmp/vimbai-backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_PATH="${BACKUP_DIR}/${TIMESTAMP}"
NEO4J_URI="${NEO4J_URI:-bolt://localhost:7687}"
NEO4J_USER="${NEO4J_USER:-neo4j}"
NEO4J_PASSWORD="${NEO4J_PASSWORD:-dev-password}"

echo "Starting Neo4j backup to ${BACKUP_PATH}"
mkdir -p "${BACKUP_PATH}"

if command -v neo4j-admin &>/dev/null; then
    echo "Using neo4j-admin for offline backup"
    neo4j-admin database dump vimbai --to-path="${BACKUP_PATH}"
elif command -v cypher-shell &>/dev/null; then
    echo "Using cypher-shell for online export"
    cypher-shell -a "${NEO4J_URI}" -u "${NEO4J_USER}" -p "${NEO4J_PASSWORD}" \
        "CALL apoc.export.cypher.all('${BACKUP_PATH}/full-dump.cypher', {format: 'cypher'});"
else
    echo "Warning: Neither neo4j-admin nor cypher-shell found."
    echo "For Docker: docker exec vimbai-neo4j neo4j-admin database dump vimbai --to-path=/backups"
    exit 1
fi

tar -czf "${BACKUP_PATH}.tar.gz" -C "${BACKUP_DIR}" "${TIMESTAMP}"
echo "Compressed backup: ${BACKUP_PATH}.tar.gz"
echo "Backup complete"
