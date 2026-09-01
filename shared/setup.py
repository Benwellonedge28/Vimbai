"""
Vimbai Shared Service Setup
One-call setup for any Vimbai service: tracing, middleware, health checks.
"""
import os
import logging
from fastapi import FastAPI

from shared.tracing import setup_tracing
from shared.middleware import setup_middleware
from shared.health import setup_health_endpoint


def setup_service(
    app: FastAPI,
    service_name: str,
    enable_tracing: bool = True,
    enable_middleware: bool = True,
    enable_health: bool = True,
):
    """
    Complete setup for a Vimbai service.
    
    This is the recommended entry point for all services:
    
        from fastapi import FastAPI
        from shared.setup import setup_service
        
        app = FastAPI(title="My Service")
        setup_service(app, service_name="my-service")
    
    Args:
        app: FastAPI app instance
        service_name: Name of the service
        enable_tracing: Enable OpenTelemetry distributed tracing
        enable_middleware: Enable request ID, logging, error handler, CORS
        enable_health: Add /health and /health/ready endpoints
    """
    # Configure logging
    log_level = os.getenv("LOG_LEVEL", "INFO").upper()
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    
    if enable_tracing:
        try:
            setup_tracing(service_name=service_name, instrument_app=app)
        except Exception as e:
            logging.warning(f"Tracing setup failed (non-fatal): {e}")
    
    if enable_middleware:
        try:
            setup_middleware(app, service_name=service_name)
        except Exception as e:
            logging.warning(f"Middleware setup failed (non-fatal): {e}")
    
    if enable_health:
        try:
            setup_health_endpoint(app, service_name=service_name)
        except Exception as e:
            logging.warning(f"Health endpoint setup failed (non-fatal): {e}")
    
    logging.info(f"Service '{service_name}' fully configured")
