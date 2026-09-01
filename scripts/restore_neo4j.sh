#!/bin/bash
# Vimbai Neo4j Restore Script
# Usage: ./scripts/restore_neo4j.sh <backup-file.tar.gz>
set -euo pipefail

BACKUP_FILE="${1:?Usage: $0 <backup-file.tar.gz>}"

if [ ! -f "${BACKUP_FILE}" ]; then
    echo "Error: Backup file not found: ${BACKUP_FILE}"
    exit 1
fi

TEMP_DIR=$(mktemp -d)
echo "Extracting backup to ${TEMP_DIR}"
tar -xzf "${BACKUP_FILE}" -C "${TEMP_DIR}"

DUMP_DIR=$(find "${TEMP_DIR}" -type d | head -2 | tail -1)

if command -v neo4j-admin &>/dev/null; then
    echo "Using neo4j-admin to restore"
    neo4j-admin database load vimbai --from-path="${DUMP_DIR}"
    echo "Restore complete"
elif command -v cypher-shell &>/dev/null; then
    CYPHER_FILE=$(find "${TEMP_DIR}" -name "*.cypher" | head -1)
    if [ -n "${CYPHER_FILE}" ]; then
        echo "Using cypher-shell to restore"
        cypher-shell -a "${NEO4J_URI:-bolt://localhost:7687}" \
            -u "${NEO4J_USER:-neo4j}" -p "${NEO4J_PASSWORD:-dev-password}" \
            --file "${CYPHER_FILE}"
        echo "Restore complete"
    else
        echo "Error: No .cypher file found in backup"
        exit 1
    fi
fi

echo "Restore complete from ${BACKUP_FILE}"
