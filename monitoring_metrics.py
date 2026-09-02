"""
Prometheus Metrics Module for Vimbai Services

This module provides comprehensive metrics collection and exposure
for Prometheus monitoring of Vimbai microservices.

Features:
- Request metrics (count, latency, status codes)
- Database operation metrics (Neo4j query times)
- Business metrics (transactions, errors)
- System metrics (CPU, memory, connections)
- Custom service-specific metrics

Usage:
    from monitoring_metrics import setup_metrics, track_request_duration

    # Initialize metrics on app startup
    setup_metrics(app)

    # Track request duration
    @track_request_duration("accounting_service", "get_accounts")
    async def get_accounts():
        ...
"""

import os
import time
from functools import wraps
from typing import Any, Callable, Dict, Optional

import psutil

# Prometheus metrics library
try:
    from prometheus_client import (
        CONTENT_TYPE_LATEST,
        REGISTRY,
        CollectorRegistry,
        Counter,
        Gauge,
        Histogram,
        Summary,
        generate_latest,
    )
except ImportError:
    print("Install prometheus-client: pip install prometheus-client")
    raise


# =============================================================================
# METRIC REGISTRY
# =============================================================================

# Create a custom registry to avoid conflicts
REGISTRY = CollectorRegistry()

# Custom labels for all metrics
DEFAULT_LABELS = {"service": "vimbai"}


# =============================================================================
# REQUEST METRICS
# =============================================================================

# HTTP Request Counter
http_requests_total = Counter(
    "vimbai_http_requests_total",
    "Total number of HTTP requests",
    ["method", "endpoint", "status_code", "service"],
    registry=REGISTRY,
)

# HTTP Request Duration (Histogram)
http_request_duration_seconds = Histogram(
    "vimbai_http_request_duration_seconds",
    "HTTP request duration in seconds",
    ["method", "endpoint", "service"],
    buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
    registry=REGISTRY,
)

# HTTP Request Size
http_request_size_bytes = Histogram(
    "vimbai_http_request_size_bytes",
    "HTTP request size in bytes",
    ["method", "endpoint", "service"],
    buckets=[100, 500, 1000, 5000, 10000, 50000, 100000],
    registry=REGISTRY,
)

# HTTP Response Size
http_response_size_bytes = Histogram(
    "vimbai_http_response_size_bytes",
    "HTTP response size in bytes",
    ["method", "endpoint", "service"],
    buckets=[100, 500, 1000, 5000, 10000, 50000, 100000, 500000],
    registry=REGISTRY,
)


# =============================================================================
# DATABASE METRICS (Neo4j)
# =============================================================================

# Database Query Counter
db_queries_total = Counter(
    "vimbai_db_queries_total", "Total number of database queries", ["operation", "status", "service"], registry=REGISTRY
)

# Database Query Duration
db_query_duration_seconds = Histogram(
    "vimbai_db_query_duration_seconds",
    "Database query duration in seconds",
    ["operation", "service"],
    buckets=[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
    registry=REGISTRY,
)

# Database Connection Pool
db_connection_pool_size = Gauge(
    "vimbai_db_connection_pool_size", "Database connection pool size", ["service"], registry=REGISTRY
)

db_connection_pool_used = Gauge(
    "vimbai_db_connection_pool_used", "Database connection pool used connections", ["service"], registry=REGISTRY
)


# =============================================================================
# BUSINESS METRICS
# =============================================================================

# Transaction Counter
transactions_total = Counter(
    "vimbai_transactions_total",
    "Total number of financial transactions",
    ["type", "status", "service"],
    registry=REGISTRY,
)

# Journal Entries
journal_entries_total = Counter(
    "vimbai_journal_entries_total", "Total number of journal entries created", ["status", "service"], registry=REGISTRY
)

# Account Operations
account_operations_total = Counter(
    "vimbai_account_operations_total",
    "Total number of account operations",
    ["operation", "status", "service"],
    registry=REGISTRY,
)

# NPO-specific metrics
donations_total = Counter(
    "vimbai_donations_total", "Total number of donations received", ["type", "status"], registry=REGISTRY
)

grants_total = Counter("vimbai_grants_total", "Total number of grants", ["status", "fund_type"], registry=REGISTRY)


# =============================================================================
# ERROR METRICS
# =============================================================================

# Error Counter
errors_total = Counter(
    "vimbai_errors_total", "Total number of errors", ["error_type", "endpoint", "service"], registry=REGISTRY
)

# Validation Errors
validation_errors_total = Counter(
    "vimbai_validation_errors_total", "Total number of validation errors", ["field", "service"], registry=REGISTRY
)

# Database Errors
db_errors_total = Counter(
    "vimbai_db_errors_total", "Total number of database errors", ["operation", "service"], registry=REGISTRY
)


# =============================================================================
# SYSTEM METRICS
# =============================================================================

# CPU Usage
cpu_usage_percent = Gauge("vimbai_cpu_usage_percent", "CPU usage percentage", ["service"], registry=REGISTRY)

# Memory Usage
memory_usage_bytes = Gauge("vimbai_memory_usage_bytes", "Memory usage in bytes", ["service"], registry=REGISTRY)

# Disk Usage
disk_usage_percent = Gauge("vimbai_disk_usage_percent", "Disk usage percentage", ["service"], registry=REGISTRY)

# Active Connections
active_connections = Gauge(
    "vimbai_active_connections", "Number of active connections", ["type", "service"], registry=REGISTRY
)


# =============================================================================
# RATE METRICS
# =============================================================================

# Request Rate (requests per second)
request_rate = Gauge("vimbai_request_rate", "Current request rate per second", ["service"], registry=REGISTRY)

# Error Rate (errors per second)
error_rate = Gauge("vimbai_error_rate", "Current error rate per second", ["service"], registry=REGISTRY)


# =============================================================================
# CUSTOM METRICS REGISTRY
# =============================================================================


class MetricsRegistry:
    """
    Custom metrics registry for Vimbai services.
    Provides methods to create and manage custom metrics.
    """

    _custom_metrics: Dict[str, Any] = {}

    @classmethod
    def register_counter(cls, name: str, description: str, labels: list = None):
        """Register a custom counter metric."""
        if name not in cls._custom_metrics:
            cls._custom_metrics[name] = Counter(name, description, labels or [], registry=REGISTRY)
        return cls._custom_metrics[name]

    @classmethod
    def register_histogram(cls, name: str, description: str, labels: list = None, buckets=None):
        """Register a custom histogram metric."""
        if name not in cls._custom_metrics:
            cls._custom_metrics[name] = Histogram(name, description, labels or [], buckets=buckets, registry=REGISTRY)
        return cls._custom_metrics[name]

    @classmethod
    def register_gauge(cls, name: str, description: str, labels: list = None):
        """Register a custom gauge metric."""
        if name not in cls._custom_metrics:
            cls._custom_metrics[name] = Gauge(name, description, labels or [], registry=REGISTRY)
        return cls._custom_metrics[name]


# =============================================================================
# DECORATORS AND UTILITIES
# =============================================================================


def track_request_duration(service: str):
    """
    Decorator to track request duration.

    Usage:
        @track_request_duration("accounting_service")
        async def my_endpoint():
            ...
    """

    def decorator(func: Callable):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            start_time = time.time()
            try:
                result = await func(*args, **kwargs)
                return result
            except Exception as e:
                errors_total.labels(error_type=type(e).__name__, endpoint=func.__name__, service=service).inc()
                raise
            finally:
                duration = time.time() - start_time
                http_request_duration_seconds.labels(
                    method="POST", endpoint=func.__name__, service=service  # Default, can be updated
                ).observe(duration)

        return wrapper

    return decorator


def track_db_query(operation: str, service: str):
    """
    Decorator to track database query duration.

    Usage:
        @track_db_query("create_account", "accounting_service")
        async def create_account(db, data):
            ...
    """

    def decorator(func: Callable):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            start_time = time.time()
            try:
                result = await func(*args, **kwargs)
                db_queries_total.labels(operation=operation, status="success", service=service).inc()
                return result
            except Exception as e:
                db_queries_total.labels(operation=operation, status="error", service=service).inc()
                db_errors_total.labels(operation=operation, service=service).inc()
                raise
            finally:
                duration = time.time() - start_time
                db_query_duration_seconds.labels(operation=operation, service=service).observe(duration)

        return wrapper

    return decorator


def increment_counter(metric: Counter, labels: Dict[str, str]):
    """
    Increment a counter metric with labels.

    Usage:
        increment_counter(
            transactions_total,
            {"type": "journal_entry", "status": "success", "service": "accounting"}
        )
    """
    metric.labels(**labels).inc()


# =============================================================================
# SYSTEM METRICS COLLECTOR
# =============================================================================


class SystemMetricsCollector:
    """
    Background collector for system metrics.
    Collects CPU, memory, and disk usage periodically.
    """

    def __init__(self, service_name: str, interval: int = 15):
        self.service_name = service_name
        self.interval = interval
        self._running = False

    async def start(self):
        """Start collecting system metrics."""
        self._running = True
        while self._running:
            self._collect_metrics()
            await asyncio.sleep(self.interval)

    def stop(self):
        """Stop collecting system metrics."""
        self._running = False

    def _collect_metrics(self):
        """Collect current system metrics."""
        # CPU usage
        cpu_percent = psutil.cpu_percent(interval=1)
        cpu_usage_percent.labels(service=self.service_name).set(cpu_percent)

        # Memory usage
        memory = psutil.virtual_memory()
        memory_usage_bytes.labels(service=self.service_name).set(memory.used)

        # Disk usage
        disk = psutil.disk_usage("/")
        disk_usage_percent.labels(service=self.service_name).set(disk.percent)


# =============================================================================
# METRICS EXPOSITION
# =============================================================================


def get_metrics() -> bytes:
    """
    Generate metrics output in Prometheus format.

    Returns:
        Metrics in Prometheus text format
    """
    return generate_latest(REGISTRY)


def get_metrics_content_type() -> str:
    """
    Get the content type for metrics response.

    Returns:
        Content type string
    """
    return CONTENT_TYPE_LATEST


# =============================================================================
# FASTAPI INTEGRATION
# =============================================================================


async def setup_metrics(app, service_name: str = "vimbai"):
    """
    Setup Prometheus metrics endpoint for FastAPI application.

    Args:
        app: FastAPI application instance
        service_name: Name of the service for metrics labels
    """
    from fastapi import APIRouter, Response

    @app.get("/metrics")
    async def metrics_endpoint():
        """Prometheus metrics endpoint."""
        return Response(content=get_metrics(), media_type=get_metrics_content_type())

    # Start system metrics collector
    collector = SystemMetricsCollector(service_name)
    # Note: In production, run this as a background task


# =============================================================================
# MIDDLEWARE FOR AUTOMATIC METRICS
# =============================================================================


class MetricsMiddleware:
    """
    FastAPI middleware for automatic request metrics collection.
    """

    def __init__(self, app, service_name: str = "vimbai"):
        self.app = app
        self.service_name = service_name

    async def __call__(self, scope, receive, send):
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        # Extract request details
        method = scope.get("method", "GET")
        path = scope.get("path", "/")

        start_time = time.time()

        # Process request
        status_code = 200
        response_size = 0

        async def send_wrapper(message):
            nonlocal status_code, response_size
            if message["type"] == "http.response.start":
                status_code = message["status"]
            elif message["type"] == "http.response.body":
                response_size += len(message.get("body", b""))
            await send(message)

        try:
            await self.app(scope, receive, send_wrapper)
        except Exception:
            status_code = 500
            raise
        finally:
            duration = time.time() - start_time

            # Record metrics
            http_requests_total.labels(
                method=method, endpoint=path, status_code=status_code, service=self.service_name
            ).inc()

            http_request_duration_seconds.labels(method=method, endpoint=path, service=self.service_name).observe(
                duration
            )

            http_response_size_bytes.labels(method=method, endpoint=path, service=self.service_name).observe(
                response_size
            )


# =============================================================================
# HEALTH CHECK METRICS
# =============================================================================

# Service Health Status
service_health = Gauge(
    "vimbai_service_health", "Service health status (1=healthy, 0=unhealthy)", ["service"], registry=REGISTRY
)

# Database Health
database_health = Gauge(
    "vimbai_database_health", "Database health status (1=healthy, 0=unhealthy)", ["service"], registry=REGISTRY
)


def update_health_status(service: str, healthy: bool, db_healthy: bool = True):
    """
    Update health status metrics.

    Args:
        service: Service name
        healthy: Overall service health
        db_healthy: Database health
    """
    service_health.labels(service=service).set(1 if healthy else 0)
    database_health.labels(service=service).set(1 if db_healthy else 0)


# =============================================================================
# EXPORTS
# =============================================================================

__all__ = [
    # Metrics
    "http_requests_total",
    "http_request_duration_seconds",
    "db_queries_total",
    "db_query_duration_seconds",
    "transactions_total",
    "journal_entries_total",
    "account_operations_total",
    "donations_total",
    "grants_total",
    "errors_total",
    "validation_errors_total",
    "db_errors_total",
    "cpu_usage_percent",
    "memory_usage_bytes",
    "disk_usage_percent",
    "active_connections",
    "service_health",
    "database_health",
    # Utilities
    "track_request_duration",
    "track_db_query",
    "increment_counter",
    "get_metrics",
    "get_metrics_content_type",
    "setup_metrics",
    "MetricsMiddleware",
    "update_health_status",
    "MetricsRegistry",
    "SystemMetricsCollector",
]


# =============================================================================
# ASYNCIO SUPPORT
# =============================================================================

import asyncio
