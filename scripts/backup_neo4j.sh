#!/bin/bash
# Vimbai Neo4j Database Backup Script
set -euo pipefail
BACKUP_DIR="${1:-./backups}"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BACKUP_PATH="${BACKUP_DIR}/neo4j_backup_${TIMESTAMP}"
RETENTION_DAYS="${NEO4J_BACKUP_RETENTION:-30}"
NEO4J_CONTAINER="${NEO4J_CONTAINER:-neo4j}"

echo "Starting Neo4j backup to ${BACKUP_PATH}..."
mkdir -p "${BACKUP_PATH}"
docker exec "${NEO4J_CONTAINER}" neo4j-admin database backup neo4j --to-path=/tmp/backup 2>&1 || {
    echo "ERROR: neo4j-admin backup failed."
    exit 1
}
docker cp "${NEO4J_CONTAINER}:/tmp/backup/." "${BACKUP_PATH}/" 2>&1
docker exec "${NEO4J_CONTAINER}" bash -c "rm -rf /tmp/backup" 2>&1 || true
tar -czf "${BACKUP_PATH}.tar.gz" -C "${BACKUP_DIR}" "neo4j_backup_${TIMESTAMP}"
rm -rf "${BACKUP_PATH}"
echo "Backup completed: ${BACKUP_PATH}.tar.gz"
find "${BACKUP_DIR}" -name "neo4j_backup_*.tar.gz" -mtime +${RETENTION_DAYS} -delete 2>/dev/null || true
echo "Cleaned up backups older than ${RETENTION_DAYS} days"
