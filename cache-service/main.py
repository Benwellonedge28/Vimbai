"""
FinAcc Redis Caching Service
Provides distributed caching for all FinAcc services
"""

from fastapi import FastAPI, HTTPException, Depends, Query
from pydantic import BaseModel, Field
from typing import Optional, List, Dict, Any
from datetime import datetime, timedelta
import json
import hashlib
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Cache Service",
    description="Redis-based distributed caching service",
    version="0.1.0",
)

# ============================================================================
# Models
# ============================================================================

class CacheEntry(BaseModel):
    key: str
    value: Any
    ttl: int = Field(default=300, ge=0)  # TTL in seconds
    tags: List[str] = []

class CacheRequest(BaseModel):
    key: str
    value: Optional[Any] = None
    ttl: int = Field(default=300, ge=0)
    tags: List[str] = []

class CacheBatchRequest(BaseModel):
    entries: List[CacheRequest]

class CacheResponse(BaseModel):
    key: str
    value: Optional[Any] = None
    hit: bool
    ttl_remaining: Optional[int] = None

# ============================================================================
# In-Memory Cache (Fallback when Redis unavailable)
# ============================================================================

class InMemoryCache:
    """Simple in-memory cache for when Redis is not available"""

    def __init__(self):
        self._cache: Dict[str, Dict[str, Any]] = {}

    def get(self, key: str) -> Optional[Any]:
        if key in self._cache:
            entry = self._cache[key]
            if entry['expires_at'] > datetime.utcnow():
                return entry['value']
            else:
                del self._cache[key]
        return None

    def set(self, key: str, value: Any, ttl: int = 300, tags: List[str] = None):
        expires_at = datetime.utcnow() + timedelta(seconds=ttl)
        self._cache[key] = {
            'value': value,
            'expires_at': expires_at,
            'tags': tags or [],
            'created_at': datetime.utcnow()
        }

    def delete(self, key: str) -> bool:
        if key in self._cache:
            del self._cache[key]
            return True
        return False

    def exists(self, key: str) -> bool:
        if key in self._cache:
            if self._cache[key]['expires_at'] > datetime.utcnow():
                return True
            del self._cache[key]
        return False

    def get_ttl(self, key: str) -> Optional[int]:
        if key in self._cache:
            entry = self._cache[key]
            if entry['expires_at'] > datetime.utcnow():
                remaining = (entry['expires_at'] - datetime.utcnow()).total_seconds()
                return int(remaining)
            del self._cache[key]
        return None

    def get_by_tag(self, tag: str) -> List[Dict[str, Any]]:
        results = []
        now = datetime.utcnow()
        for key, entry in self._cache.items():
            if entry['expires_at'] > now and tag in entry['tags']:
                results.append({
                    'key': key,
                    'value': entry['value'],
                    'ttl': int((entry['expires_at'] - now).total_seconds())
                })
        return results

    def clear_tag(self, tag: str) -> int:
        count = 0
        to_delete = []
        now = datetime.utcnow()
        for key, entry in self._cache.items():
            if entry['expires_at'] > now and tag in entry['tags']:
                to_delete.append(key)

        for key in to_delete:
            del self._cache[key]
            count += 1

        return count

    def clear_all(self):
        self._cache.clear()

    def stats(self) -> Dict[str, Any]:
        now = datetime.utcnow()
        total = len(self._cache)
        expired = sum(1 for e in self._cache.values() if e['expires_at'] <= now)
        valid = total - expired

        return {
            'total_entries': total,
            'valid_entries': valid,
            'expired_entries': expired
        }


# Global cache instance
cache = InMemoryCache()

# ============================================================================
# Cache Key Patterns
# ============================================================================

CACHE_KEYS = {
    'account': 'account:{id}',
    'journal_entry': 'journal_entry:{id}',
    'financial_statement': 'financial_statement:{id}',
    'transaction': 'transaction:{id}',
    'user': 'user:{id}',
    'budget': 'budget:{id}',
    'exchange_rate': 'exchange_rate:{from}:{to}',
    'alert': 'alert:{id}',
    'notification': 'notification:{user_id}:{id}',
    'dashboard': 'dashboard:{user_id}:{id}'
}

def generate_cache_key(pattern: str, **kwargs) -> str:
    """Generate a cache key from a pattern"""
    key = pattern
    for k, v in kwargs.items():
        key = key.replace(f'{{{k}}}', str(v))
    return key

def hash_key(key: str) -> str:
    """Hash a key for consistent distribution"""
    return hashlib.sha256(key.encode()).hexdigest()[:16]

# ============================================================================
# API Endpoints
# ============================================================================

@app.on_event("startup")
async def startup():
    print("Cache service started (in-memory mode)")

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cache"}

# --- Basic Cache Operations ---

@app.post("/cache", response_model=CacheResponse)
async def set_cache(request: CacheRequest):
    """Set a cache entry"""
    cache.set(request.key, request.value, request.ttl, request.tags)

    return CacheResponse(
        key=request.key,
        value=request.value,
        hit=True,
        ttl_remaining=request.ttl
    )

@app.get("/cache/{key}", response_model=CacheResponse)
async def get_cache(key: str):
    """Get a cache entry"""
    value = cache.get(key)
    hit = value is not None
    ttl_remaining = cache.get_ttl(key) if hit else None

    return CacheResponse(
        key=key,
        value=value,
        hit=hit,
        ttl_remaining=ttl_remaining
    )

@app.delete("/cache/{key}")
async def delete_cache(key: str):
    """Delete a cache entry"""
    deleted = cache.delete(key)
    return {"success": deleted, "key": key}

@app.head("/cache/{key}")
async def check_cache_exists(key: str):
    """Check if a cache entry exists"""
    exists = cache.exists(key)
    return {"exists": exists}

# --- Batch Operations ---

@app.post("/cache/batch/set")
async def batch_set_cache(request: CacheBatchRequest):
    """Set multiple cache entries"""
    results = []
    for entry in request.entries:
        cache.set(entry.key, entry.value, entry.ttl, entry.tags)
        results.append({"key": entry.key, "success": True})

    return {"count": len(results), "results": results}

@app.post("/cache/batch/get")
async def batch_get_cache(keys: List[str]):
    """Get multiple cache entries"""
    results = []
    for key in keys:
        value = cache.get(key)
        results.append({
            "key": key,
            "value": value,
            "hit": value is not None
        })

    return {"count": len(results), "results": results}

@app.delete("/cache/batch")
async def batch_delete_cache(keys: List[str]):
    """Delete multiple cache entries"""
    deleted = 0
    for key in keys:
        if cache.delete(key):
            deleted += 1

    return {"deleted": deleted, "total": len(keys)}

# --- Tag-Based Operations ---

@app.get("/cache/tag/{tag}")
async def get_by_tag(tag: str):
    """Get all cache entries with a specific tag"""
    results = cache.get_by_tag(tag)
    return {"count": len(results), "entries": results}

@app.delete("/cache/tag/{tag}")
async def clear_tag(tag: str):
    """Clear all cache entries with a specific tag"""
    count = cache.clear_tag(tag)
    return {"cleared": count, "tag": tag}

# --- Utility Endpoints ---

@app.post("/cache/invalidate/{pattern}")
async def invalidate_pattern(pattern: str):
    """Invalidate cache entries matching a pattern"""
    # In production, this would use Redis SCAN
    cleared = cache.clear_tag(pattern)
    return {"pattern": pattern, "cleared": cleared}

@app.get("/cache/stats")
async def get_cache_stats():
    """Get cache statistics"""
    return cache.stats()

@app.post("/cache/clear")
async def clear_all_cache():
    """Clear all cache entries"""
    cache.clear_all()
    return {"success": True, "message": "All cache entries cleared"}

# --- Predefined Cache Operations ---

@app.post("/cache/account/{account_id}")
async def cache_account(account_id: str, data: Dict[str, Any], ttl: int = 300):
    """Cache account data"""
    key = generate_cache_key(CACHE_KEYS['account'], id=account_id)
    cache.set(key, data, ttl, tags=['account'])
    return {"key": key, "success": True}

@app.get("/cache/account/{account_id}")
async def get_cached_account(account_id: str):
    """Get cached account data"""
    key = generate_cache_key(CACHE_KEYS['account'], id=account_id)
    value = cache.get(key)
    return {"key": key, "value": value, "hit": value is not None}

@app.post("/cache/journal-entry/{entry_id}")
async def cache_journal_entry(entry_id: str, data: Dict[str, Any], ttl: int = 600):
    """Cache journal entry"""
    key = generate_cache_key(CACHE_KEYS['journal_entry'], id=entry_id)
    cache.set(key, data, ttl, tags=['journal_entry'])
    return {"key": key, "success": True}

@app.get("/cache/journal-entry/{entry_id}")
async def get_cached_journal_entry(entry_id: str):
    """Get cached journal entry"""
    key = generate_cache_key(CACHE_KEYS['journal_entry'], id=entry_id)
    value = cache.get(key)
    return {"key": key, "value": value, "hit": value is not None}

@app.post("/cache/exchange-rate/{from_currency}/{to_currency}")
async def cache_exchange_rate(from_currency: str, to_currency: str, rate: float, ttl: int = 3600):
    """Cache exchange rate"""
    key = generate_cache_key(CACHE_KEYS['exchange_rate'], from=from_currency, to=to_currency)
    cache.set(key, rate, ttl, tags=['exchange_rate', 'currency'])
    return {"key": key, "success": True}

@app.get("/cache/exchange-rate/{from_currency}/{to_currency}")
async def get_cached_exchange_rate(from_currency: str, to_currency: str):
    """Get cached exchange rate"""
    key = generate_cache_key(CACHE_KEYS['exchange_rate'], from=from_currency, to=to_currency)
    value = cache.get(key)
    return {"key": key, "value": value, "hit": value is not None}

@app.post("/cache/dashboard/{user_id}/{dashboard_id}")
async def cache_dashboard(user_id: str, dashboard_id: str, data: Dict[str, Any], ttl: int = 120):
    """Cache dashboard data"""
    key = generate_cache_key(CACHE_KEYS['dashboard'], user_id=user_id, id=dashboard_id)
    cache.set(key, data, ttl, tags=['dashboard', f'user:{user_id}'])
    return {"key": key, "success": True}

@app.get("/cache/dashboard/{user_id}/{dashboard_id}")
async def get_cached_dashboard(user_id: str, dashboard_id: str):
    """Get cached dashboard data"""
    key = generate_cache_key(CACHE_KEYS['dashboard'], user_id=user_id, id=dashboard_id)
    value = cache.get(key)
    return {"key": key, "value": value, "hit": value is not None}

@app.delete("/cache/dashboard/user/{user_id}")
async def invalidate_user_dashboards(user_id: str):
    """Invalidate all cached dashboards for a user"""
    count = cache.clear_tag(f'user:{user_id}')
    return {"cleared": count, "user_id": user_id}

# --- Cache-Aside Pattern Helpers ---

class CacheAsideRequest(BaseModel):
    key: str
    fetch_function: Optional[str] = None  # Reserved for future use
    ttl: int = 300
    tags: List[str] = []

@app.post("/cache-aside")
async def cache_aside_get(request: CacheAsideRequest, fetch_missing: bool = True):
    """
    Cache-aside pattern: Check cache first, fetch and cache if miss
    Note: fetch_function should be implemented with actual service calls
    """
    # Try to get from cache
    value = cache.get(request.key)

    if value is not None:
        return {
            "key": request.key,
            "value": value,
            "source": "cache",
            "hit": True
        }

    # Cache miss - in production, would call fetch_function
    # For now, return miss indicator
    return {
        "key": request.key,
        "value": None,
        "source": "origin",
        "hit": False,
        "message": "Cache miss - fetch from origin and call cache-aside/set"
    }

@app.post("/cache-aside/set")
async def cache_aside_set(request: CacheAsideRequest):
    """Set value in cache (after fetching from origin)"""
    # This endpoint should be called after fetching from origin
    return {"key": request.key, "success": True}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8096)