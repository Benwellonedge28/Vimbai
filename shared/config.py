"""
Shared configuration for Vimbai services.
Provides connection pooling, retry policies, and common settings.
"""
import os
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class Neo4jConfig:
    """Neo4j connection pool configuration."""
    uri: str = field(default_factory=lambda: os.getenv("NEO4J_URI", "bolt://localhost:7687"))
    user: str = field(default_factory=lambda: os.getenv("NEO4J_USER", "neo4j"))
    password: str = field(default_factory=lambda: os.getenv("NEO4J_PASSWORD", "dev-password"))
    max_connection_pool_size: int = field(default_factory=lambda: int(os.getenv("NEO4J_MAX_POOL_SIZE", "50")))
    connection_timeout: int = field(default_factory=lambda: int(os.getenv("NEO4J_CONNECTION_TIMEOUT", "30")))
    max_connection_lifetime: int = field(default_factory=lambda: int(os.getenv("NEO4J_MAX_CONNECTION_LIFETIME", "3600")))


@dataclass
class RedisConfig:
    """Redis connection pool configuration."""
    url: str = field(default_factory=lambda: os.getenv("REDIS_URL", "redis://localhost:6379"))
    max_connections: int = field(default_factory=lambda: int(os.getenv("REDIS_MAX_CONNECTIONS", "20")))
    socket_timeout: int = field(default_factory=lambda: int(os.getenv("REDIS_SOCKET_TIMEOUT", "5")))
    socket_connect_timeout: int = field(default_factory=lambda: int(os.getenv("REDIS_SOCKET_CONNECT_TIMEOUT", "5")))
    retry_on_timeout: bool = True


@dataclass
class RetryConfig:
    """Retry policy for external service calls."""
    max_retries: int = 3
    initial_delay: float = 0.5
    max_delay: float = 10.0
    backoff_factor: float = 2.0
    retryable_status_codes: tuple = (502, 503, 504)


@dataclass
class ServiceConfig:
    """Base configuration for all Vimbai services."""
    service_name: str = ""
    host: str = field(default_factory=lambda: os.getenv("HOST", "0.0.0.0"))
    port: int = field(default_factory=lambda: int(os.getenv("PORT", "8000")))
    log_level: str = field(default_factory=lambda: os.getenv("LOG_LEVEL", "info"))
    jwt_secret: str = field(default_factory=lambda: os.getenv("JWT_SECRET", "dev-secret"))
    environment: str = field(default_factory=lambda: os.getenv("ENVIRONMENT", "development"))
    neo4j: Neo4jConfig = field(default_factory=Neo4jConfig)
    redis: RedisConfig = field(default_factory=RedisConfig)
    retry: RetryConfig = field(default_factory=RetryConfig)


def get_neo4j_driver(config: Optional[Neo4jConfig] = None):
    """Get a Neo4j driver with proper connection pooling."""
    cfg = config or Neo4jConfig()
    try:
        from neo4j import GraphDatabase
        return GraphDatabase.driver(
            cfg.uri,
            auth=(cfg.user, cfg.password),
            max_connection_pool_size=cfg.max_connection_pool_size,
            connection_timeout=cfg.connection_timeout,
            max_connection_lifetime=cfg.max_connection_lifetime,
        )
    except ImportError:
        return None
    except Exception as e:
        print(f"Warning: Could not create Neo4j driver: {e}")
        return None


def get_redis_client(config: Optional[RedisConfig] = None):
    """Get a Redis client with proper connection pooling."""
    cfg = config or RedisConfig()
    try:
        import redis
        pool = redis.ConnectionPool(
            host=cfg.url.split("://")[1].split(":")[0] if "://" in cfg.url else "localhost",
            port=int(cfg.url.split(":")[-1]) if ":" in cfg.url else 6379,
            max_connections=cfg.max_connections,
            socket_timeout=cfg.socket_timeout,
            socket_connect_timeout=cfg.socket_connect_timeout,
            retry_on_timeout=cfg.retry_on_timeout,
        )
        return redis.Redis(connection_pool=pool)
    except ImportError:
        return None
    except Exception as e:
        print(f"Warning: Could not create Redis client: {e}")
        return None
