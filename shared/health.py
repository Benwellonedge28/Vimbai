"""
Vimbai Shared Health Check Module
Provides standardized health check endpoints for all services.
"""
import os
import time
from typing import Optional, Dict, Any
from datetime import datetime, timezone
from fastapi import FastAPI, APIRouter
from pydantic import BaseModel


class HealthStatus(BaseModel):
    status: str  # "healthy", "degraded", "unhealthy"
    service: str
    version: str = "1.0.0"
    timestamp: str = ""
    checks: Dict[str, Any] = {}
    uptime_seconds: float = 0.0


_start_time = time.time()
_health_checks: Dict[str, Any] = {}


def register_health_check(name: str, check_fn):
    """Register a health check function. Should return (bool, str) for (is_healthy, message)."""
    _health_checks[name] = check_fn


def create_health_router(service_name: str = "") -> APIRouter:
    """Create a health check router with /health and /health/ready endpoints."""
    router = APIRouter(tags=["health"])
    svc_name = service_name or os.getenv("SERVICE_NAME", "unknown")
    
    @router.get("/health", response_model=HealthStatus)
    async def health():
        """Liveness probe - is the service running?"""
        return HealthStatus(
            status="healthy",
            service=svc_name,
            timestamp=datetime.now(timezone.utc).isoformat(),
            uptime_seconds=time.time() - _start_time,
        )
    
    @router.get("/health/ready", response_model=HealthStatus)
    async def readiness():
        """Readiness probe - is the service ready to accept traffic?"""
        checks = {}
        all_healthy = True
        
        for name, check_fn in _health_checks.items():
            try:
                result = check_fn()
                if isinstance(result, tuple):
                    is_healthy, message = result
                else:
                    is_healthy = result
                    message = "OK" if is_healthy else "Failed"
                
                checks[name] = {"status": "healthy" if is_healthy else "unhealthy", "message": message}
                if not is_healthy:
                    all_healthy = False
            except Exception as e:
                checks[name] = {"status": "unhealthy", "message": str(e)}
                all_healthy = False
        
        return HealthStatus(
            status="healthy" if all_healthy else "degraded",
            service=svc_name,
            timestamp=datetime.now(timezone.utc).isoformat(),
            uptime_seconds=time.time() - _start_time,
            checks=checks,
        )
    
    return router


def setup_health_endpoint(app: FastAPI, service_name: str = ""):
    """Add health check endpoints to a FastAPI app."""
    app.include_router(create_health_router(service_name))
