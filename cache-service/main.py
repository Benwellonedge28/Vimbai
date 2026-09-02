"""
Vimbai Cache Service
In-memory cache with TTL, LRU eviction, and distributed cache coordination.
Port: 8350
"""
import os, time, uuid, json, hashlib
from typing import Dict, Any, Optional, List
from collections import OrderedDict
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "cache-service"
PORT = int(os.getenv("PORT", "8350"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Cache Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

MAX_CACHE_SIZE = int(os.getenv("CACHE_MAX_SIZE", "10000"))
DEFAULT_TTL = int(os.getenv("CACHE_DEFAULT_TTL", "3600"))

_cache: OrderedDict = OrderedDict()
_cache_stats = {"hits": 0, "misses": 0, "evictions": 0, "sets": 0}

class CacheEntry(BaseModel):
    key: str; value: Any; ttl: int = DEFAULT_TTL

class CacheResponse(BaseModel):
    key: str; value: Optional[Any] = None; found: bool; ttl_remaining: int = 0

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0",
            "cache_size": len(_cache), "stats": _cache_stats}

@app.get("/cache/{key}", response_model=CacheResponse)
async def get_cache(key: str):
    if key in _cache:
        entry = _cache[key]
        if entry["expires_at"] > time.time():
            _cache.move_to_end(key)
            _cache_stats["hits"] += 1
            ttl_remaining = int(entry["expires_at"] - time.time())
            return CacheResponse(key=key, value=entry["value"], found=True, ttl_remaining=ttl_remaining)
        else:
            del _cache[key]
    _cache_stats["misses"] += 1
    return CacheResponse(key=key, found=False)

@app.post("/cache", response_model=CacheResponse)
async def set_cache(entry: CacheEntry):
    while len(_cache) >= MAX_CACHE_SIZE:
        _cache.popitem(last=False)
        _cache_stats["evictions"] += 1
    
    _cache[entry.key] = {
        "value": entry.value,
        "expires_at": time.time() + entry.ttl,
        "created_at": time.time()
    }
    _cache_stats["sets"] += 1
    return CacheResponse(key=entry.key, value=entry.value, found=True, ttl_remaining=entry.ttl)

@app.delete("/cache/{key}")
async def delete_cache(key: str):
    if key in _cache:
        del _cache[key]
        return {"key": key, "deleted": True}
    return {"key": key, "deleted": False}

@app.delete("/cache")
async def clear_cache():
    count = len(_cache)
    _cache.clear()
    return {"cleared": count}

@app.get("/stats")
async def get_stats():
    total = _cache_stats["hits"] + _cache_stats["misses"]
    hit_rate = (_cache_stats["hits"] / total * 100) if total > 0 else 0
    return {
        "cache_size": len(_cache), "max_size": MAX_CACHE_SIZE,
        "stats": _cache_stats, "hit_rate_pct": round(hit_rate, 2)
    }

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
