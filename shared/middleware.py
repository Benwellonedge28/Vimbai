"""
Vimbai Shared Middleware
Provides common middleware for all services:
- Request ID correlation
- Structured logging
- Error handling
- CORS configuration
"""
import os
import uuid
import time
import logging
from typing import Optional, Callable
from contextvars import ContextVar

from fastapi import FastAPI, Request, Response
from fastapi.middleware.cors import CORSMiddleware
from starlette.middleware.base import BaseHTTPMiddleware

# Context variables for request-scoped data
request_id_ctx: ContextVar[str] = ContextVar("request_id", default="")
company_id_ctx: ContextVar[str] = ContextVar("company_id", default="")
service_name_ctx: ContextVar[str] = ContextVar("service_name", default="")


class RequestIDMiddleware(BaseHTTPMiddleware):
    """Injects a request ID into every request for correlation across services."""
    
    async def dispatch(self, request: Request, call_next: Callable):
        request_id = request.headers.get("X-Request-ID", str(uuid.uuid4()))
        request_id_ctx.set(request_id)
        
        # Extract company_id from headers or path params
        company_id = request.headers.get("X-Company-ID", "")
        if company_id:
            company_id_ctx.set(company_id)
        
        response = await call_next(request)
        response.headers["X-Request-ID"] = request_id
        return response


class RequestLoggingMiddleware(BaseHTTPMiddleware):
    """Logs every request with timing, method, path, and status code."""
    
    async def dispatch(self, request: Request, call_next: Callable):
        start_time = time.time()
        service = service_name_ctx.get() or os.getenv("SERVICE_NAME", "unknown")
        
        # Skip health checks to reduce noise
        if request.url.path in ("/health", "/", "/metrics"):
            return await call_next(request)
        
        try:
            response = await call_next(request)
            duration_ms = (time.time() - start_time) * 1000
            
            logging.info(
                f"request_completed service={service} method={request.method} "
                f"path={request.url.path} status={response.status_code} "
                f"duration_ms={duration_ms:.2f} "
                f"request_id={request_id_ctx.get()}"
            )
            return response
        except Exception as e:
            duration_ms = (time.time() - start_time) * 1000
            logging.error(
                f"request_failed service={service} method={request.method} "
                f"path={request.url.path} error={str(e)} "
                f"duration_ms={duration_ms:.2f} "
                f"request_id={request_id_ctx.get()}"
            )
            raise


class ErrorHandlerMiddleware(BaseHTTPMiddleware):
    """Catches unhandled exceptions and returns a standardized error response."""
    
    async def dispatch(self, request: Request, call_next: Callable):
        try:
            return await call_next(request)
        except Exception as e:
            request_id = request_id_ctx.get()
            logging.error(
                f"unhandled_error service={service_name_ctx.get()} "
                f"path={request.url.path} error={type(e).__name__} "
                f"message={str(e)} request_id={request_id}"
            )
            return Response(
                content=f'{{"detail": "Internal server error", "code": "INTERNAL_ERROR", "request_id": "{request_id}"}}',
                status_code=500,
                media_type="application/json"
            )


def setup_middleware(
    app: FastAPI,
    service_name: str = "",
    cors_origins: Optional[list] = None,
    enable_logging: bool = True,
    enable_error_handler: bool = True,
):
    """
    Configure all standard middleware for a Vimbai service.
    
    Args:
        app: FastAPI app instance
        service_name: Name of the service for logging
        cors_origins: List of allowed CORS origins (default: ["*"] for dev)
        enable_logging: Whether to enable request logging middleware
        enable_error_handler: Whether to enable error handler middleware
    """
    service_name_ctx.set(service_name)
    os.environ["SERVICE_NAME"] = service_name
    
    # CORS
    origins = cors_origins or os.getenv("CORS_ORIGINS", "*").split(",")
    app.add_middleware(
        CORSMiddleware,
        allow_origins=origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
        expose_headers=["X-Request-ID"],
    )
    
    # Request ID (always first, so other middleware can access it)
    app.add_middleware(RequestIDMiddleware)
    
    # Error handler (catches exceptions, returns clean JSON)
    if enable_error_handler:
        app.add_middleware(ErrorHandlerMiddleware)
    
    # Request logging (outermost, so it can time the full request)
    if enable_logging:
        app.add_middleware(RequestLoggingMiddleware)
    
    logging.info(f"Middleware configured for {service_name}")


def get_request_id() -> str:
    """Get the current request ID (for use within request handlers)."""
    return request_id_ctx.get()


def get_company_id() -> str:
    """Get the current company ID from request context."""
    return company_id_ctx.get()
