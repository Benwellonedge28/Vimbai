"""
Vimbai Shared OpenTelemetry Tracing Module
Import this in any service's main.py to enable distributed tracing.

Usage:
    from shared.tracing import setup_tracing
    setup_tracing(service_name="accounting-service")

    # For manual spans:
    from shared.tracing import get_tracer
    tracer = get_tracer()
    with tracer.start_as_current_span("operation_name") as span:
        span.set_attribute("key", "value")
        ...
"""

import logging
import os
from typing import Optional

from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPGrcpSpanExporter
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.httpx import HTTPXClientInstrumentor
from opentelemetry.instrumentation.logging import LoggingInstrumentor
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.trace import get_tracer_provider

logger = logging.getLogger(__name__)

_TRACER: Optional[trace.Tracer] = None
_INITIALIZED = False


def setup_tracing(
    service_name: str,
    service_version: str = "1.0.0",
    otlp_endpoint: Optional[str] = None,
    instrument_app=None,
):
    """
    Initialize OpenTelemetry tracing for a Vimbai service.

    Args:
        service_name: Name of the service (e.g., "accounting-service")
        service_version: Version string
        otlp_endpoint: OTLP collector endpoint (default: env OTEL_EXPORTER_OTLP_ENDPOINT)
        instrument_app: FastAPI app instance to auto-instrument
    """
    global _TRACER, _INITIALIZED

    if _INITIALIZED:
        logger.warning(f"Tracing already initialized for {_TRACER}")
        return _TRACER

    endpoint = otlp_endpoint or os.environ.get("OTEL_EXPORTER_OTLP_ENDPOINT", "http://otel-collector:4317")

    resource = Resource.create(
        {
            "service.name": service_name,
            "service.version": service_version,
            "service.namespace": "vimbai",
            "deployment.environment": os.environ.get("DEPLOYMENT_ENV", "production"),
        }
    )

    provider = TracerProvider(resource=resource)
    exporter = OTLPGrcpSpanExporter(endpoint=endpoint, insecure=True)
    provider.add_span_processor(BatchSpanProcessor(exporter))
    trace.set_tracer_provider(provider)

    # Auto-instrument HTTP client calls
    HTTPXClientInstrumentor().instrument()

    # Auto-instrument logging (adds trace_id to log records)
    LoggingInstrumentor().instrument(set_logging_format=True)

    # Auto-instrument FastAPI if app provided
    if instrument_app is not None:
        FastAPIInstrumentor.instrument_app(instrument_app)

    _TRACER = trace.get_tracer(service_name)
    _INITIALIZED = True

    logger.info(f"OpenTelemetry tracing initialized for {service_name} -> {endpoint}")
    return _TRACER


def get_tracer() -> trace.Tracer:
    """Get the configured tracer. Must call setup_tracing first."""
    if _TRACER is None:
        # Return a no-op tracer if not initialized
        return trace.get_tracer(__name__)
    return _TRACER


def instrument_app(app, service_name: str):
    """Convenience: instrument a FastAPI app with tracing."""
    setup_tracing(service_name=service_name, instrument_app=app)
