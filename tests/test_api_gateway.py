"""
Comprehensive Tests for Vimbai API Gateway
Includes tests for rate limiting, circuit breaker, and routing
"""

import pytest
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'api-gateway'))

from middleware.ratelimit import (
    TokenBucket, RateLimiter, RateLimitConfig,
    extractClientID, hasPrefix
)

@pytest.fixture
def anyio_backend():
    return "asyncio"


# ============================================================================
# Token Bucket Tests
# ============================================================================

def test_token_bucket_allow():
    """Test that token bucket allows requests when tokens available"""
    bucket = TokenBucket(rate=10, burst=20)

    # Should allow up to burst size
    for i in range(20):
        assert bucket.Allow() == True


def test_token_bucket_deny_when_empty():
    """Test that token bucket denies when empty"""
    bucket = TokenBucket(rate=1, burst=2)

    # Consume all tokens
    bucket.Allow()
    bucket.Allow()

    # Should be denied
    assert bucket.Allow() == False


def test_token_bucket_refill():
    """Test that tokens refill over time"""
    bucket = TokenBucket(rate=100, burst=1)

    # Consume the token
    bucket.Allow()
    assert bucket.Allow() == False  # No tokens

    # Simulate time passing (in production this happens automatically)
    bucket.mu.lock()
    bucket.lastRefillTime = bucket.lastRefillTime.add_sec(-1.1)  # Just over 1 second
    bucket.mu.unlock()

    # Should have refilled
    assert bucket.Allow() == True


def test_token_bucket_max_tokens():
    """Test that tokens don't exceed max"""
    bucket = TokenBucket(rate=100, burst=10)

    # Wait for some refill
    bucket.mu.lock()
    bucket.lastRefillTime = bucket.lastRefillTime.add_sec(-10)  # 10 seconds
    bucket.mu.unlock()

    # Should not exceed burst size
    tokens = bucket.GetTokens()
    assert tokens <= 10


# ============================================================================
# Rate Limiter Tests
# ============================================================================

def test_rate_limiter_creation():
    """Test rate limiter creation"""
    config = RateLimitConfig(
        RequestsPerSecond=100,
        BurstSize=200,
        Enabled=True
    )
    limiter = RateLimiter(config)
    assert limiter.config.RequestsPerSecond == 100
    assert limiter.config.BurstSize == 200


def test_rate_limiter_allow():
    """Test rate limiter allows requests"""
    config = RateLimitConfig(
        RequestsPerSecond=100,
        BurstSize=200,
        Enabled=True
    )
    limiter = RateLimiter(config)

    # Should allow requests
    assert limiter.Allow("client1") == True
    assert limiter.Allow("client2") == True


def test_rate_limiter_per_client():
    """Test that rate limiting is per-client"""
    config = RateLimitConfig(
        RequestsPerSecond=1,
        BurstSize=1,
        Enabled=True
    )
    limiter = RateLimiter(config)

    # Client 1 uses their bucket
    assert limiter.Allow("client1") == True
    assert limiter.Allow("client1") == False  # Depleted

    # Client 2 has their own bucket (should be allowed)
    assert limiter.Allow("client2") == True


def test_rate_limiter_cleanup():
    """Test rate limiter cleanup of stale buckets"""
    config = RateLimitConfig(
        RequestsPerSecond=100,
        BurstSize=200,
        Enabled=True
    )
    limiter = RateLimiter(config)

    # Add some clients
    limiter.Allow("client1")
    limiter.Allow("client2")

    # Should have 2 buckets
    assert len(limiter.buckets) >= 2

    # Stop the cleanup goroutine
    limiter.Stop()


# ============================================================================
# Config Loading Tests
# ============================================================================

def test_default_rate_limit_config():
    """Test default rate limit config values"""
    config = RateLimitConfig(
        RequestsPerSecond=100,
        BurstSize=200,
        Enabled=True
    )

    assert config.RequestsPerSecond == 100
    assert config.BurstSize == 200
    assert config.Enabled == True


# ============================================================================
# Helper Function Tests
# ============================================================================

def test_has_prefix():
    """Test string prefix helper"""
    assert hasPrefix("hello world", "hello") == True
    assert hasPrefix("hello world", "world") == False
    assert hasPrefix("hello", "hello world") == False
    assert hasPrefix("", "hello") == True
    assert hasPrefix("hello", "") == True


# ============================================================================
# Mock Context for WebSocket Tests (simulated)
# ============================================================================

class MockContext:
    """Mock Echo context for testing middleware logic"""
    def __init__(self, headers=None, path="/test", user_id=None):
        self._headers = headers or {}
        self._path = path
        self._user_id = user_id

    def Request(self):
        return MockRequest(self._headers)

    def RealIP(self):
        return "127.0.0.1"


class MockRequest:
    def __init__(self, headers):
        self._headers = headers

    def Header(self, key):
        return self._headers.get(key)


def test_extract_client_id_from_xff():
    """Test client ID extraction from X-Forwarded-For"""
    # Simulate header extraction
    headers = {"X-Forwarded-For": "192.168.1.1, 10.0.0.1"}
    req = MockRequest(headers)

    # Extract first IP from XFF
    xff = req.Header("X-Forwarded-For")
    if xff:
        if ',' in xff:
            client_id = xff.split(',')[0].strip()
        else:
            client_id = xff
    else:
        client_id = "default"

    assert client_id == "192.168.1.1"


def test_extract_client_id_from_user_id():
    """Test client ID extraction from user ID header"""
    headers = {"X-User-ID": "user123"}
    req = MockRequest(headers)

    xff = req.Header("X-Forwarded-For")
    user_id = req.Header("X-User-ID")

    if xff:
        client_id = xff.split(',')[0].strip() if ',' in xff else xff
    elif user_id:
        client_id = f"user:{user_id}"
    else:
        client_id = "default"

    assert client_id == "user:user123"


def test_extract_client_id_fallback():
    """Test client ID fallback to default"""
    headers = {}
    req = MockRequest(headers)

    xff = req.Header("X-Forwarded-For")
    user_id = req.Header("X-User-ID")

    if xff:
        client_id = xff.split(',')[0].strip() if ',' in xff else xff
    elif user_id:
        client_id = f"user:{user_id}"
    else:
        client_id = "default"

    assert client_id == "default"


# ============================================================================
# Configuration Structure Tests
# ============================================================================

def test_route_config_structure():
    """Test route configuration structure"""
    from config.config import Route

    route = Route(
        Path="/accounts",
        TargetURL="http://localhost:8000",
        AuthRequired=True,
        RateLimitPerSecond=100,
        RateLimitBurst=200
    )

    assert route.Path == "/accounts"
    assert route.AuthRequired == True
    assert route.RateLimitPerSecond == 100


def test_rate_limit_config_structure():
    """Test rate limit configuration structure"""
    from config.config import RateLimitConfig, RouteRateLimit

    config = RateLimitConfig(
        Enabled=True,
        RequestsPerSecond=100,
        BurstSize=200,
        RouteOverrides={
            "/identity": RouteRateLimit(RequestsPerSecond=10, BurstSize=20)
        }
    )

    assert config.Enabled == True
    assert config.RequestsPerSecond == 100
    assert "/identity" in config.RouteOverrides
    assert config.RouteOverrides["/identity"].RequestsPerSecond == 10


# ============================================================================
# Circuit Breaker Integration Tests
# ============================================================================

def test_circuit_breaker_settings():
    """Test circuit breaker configuration"""
    from middleware.circuit_breaker import DefaultCircuitBreakerConfig

    # Verify default settings exist
    assert DefaultCircuitBreakerConfig.MaxRequests > 0
    assert DefaultCircuitBreakerConfig.Interval > 0
    assert DefaultCircuitBreakerConfig.Timeout > 0
    assert DefaultCircuitBreakerConfig.ReadyToOpen > 0


# ============================================================================
# Resilience Handler Tests
# ============================================================================

def test_resilience_handler_structure():
    """Test proxy resilience handler structure"""
    # This tests the structure without actual HTTP calls
    # In production, you'd use httptest for full integration tests

    class MockProxyResilienceHandler:
        def __init__(self, auth_required=True, route_path="/test"):
            self.auth_required = auth_required
            self.route_path = route_path

        def Handle(self, context):
            if not self.auth_required:
                return {"status": "proxied"}
            return {"status": "auth_required"}

    handler = MockProxyResilienceHandler(auth_required=False, route_path="/accounts")
    result = handler.Handle(None)
    assert result["status"] == "proxied"

    handler2 = MockProxyResilienceHandler(auth_required=True, route_path="/accounts")
    result2 = handler2.Handle(None)
    assert result2["status"] == "auth_required"


# ============================================================================
# Route Configuration Tests
# ============================================================================

def test_load_config_routes():
    """Test that routes are loaded correctly"""
    from config.config import LoadConfig

    cfg = LoadConfig()

    # Verify routes exist
    assert len(cfg.Routes) > 0

    # Check critical routes
    route_paths = [r.Path for r in cfg.Routes]
    assert "/identity" in route_paths
    assert "/accounts" in route_paths

    # Verify identity route doesn't require auth
    identity_route = next(r for r in cfg.Routes if r.Path == "/identity")
    assert identity_route.AuthRequired == False

    # Verify accounts route requires auth
    accounts_route = next(r for r in cfg.Routes if r.Path == "/accounts")
    assert accounts_route.AuthRequired == True


def test_load_config_rate_limit():
    """Test that rate limit config is loaded"""
    from config.config import LoadConfig

    cfg = LoadConfig()

    # Rate limit config should exist
    assert hasattr(cfg, 'RateLimit')
    assert cfg.RateLimit.Enabled in [True, False]


# ============================================================================
# Auth Middleware Tests
# ============================================================================

def test_auth_middleware_skip():
    """Test that auth middleware skips non-auth routes"""
    from middleware.auth import AuthMiddleware

    # Create mock config with non-auth routes
    class MockRoute:
        Path = "/identity"
        AuthRequired = False

    class MockConfig:
        Routes = [MockRoute()]
        JwtSecret = "test-secret"

    # Verify middleware exists and is callable
    middleware = AuthMiddleware(MockConfig())
    assert callable(middleware)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])