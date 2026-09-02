"""
Shared test configuration and fixtures for Vimbai
"""

import os

import pytest

# Ensure test environment variables are set before any app import
os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")
os.environ.setdefault("NEO4J_PASSWORD", "test-password")
os.environ.setdefault("NEO4J_URI", "bolt://localhost:7687")
os.environ.setdefault("NEO4J_USER", "neo4j")
os.environ.setdefault("RABBITMQ_HOST", "localhost")
os.environ.setdefault("RABBITMQ_USER", "test_user")
os.environ.setdefault("RABBITMQ_PASS", "test_pass")


@pytest.fixture(scope="session")
def jwt_secret():
    return os.environ["JWT_SECRET"]
