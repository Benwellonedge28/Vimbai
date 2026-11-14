"""In-process gateway contract for the Vimbai local runtime.

Replicates the production API gateway behaviour for a single-process
deployment: JWT verification, identity-header injection, Book-context
membership gating, and open auth routes for /identity (register/login).
"""

import os

import httpx
import jwt
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse


class LocalGatewayMiddleware(BaseHTTPMiddleware):
    """Production-parity auth for local mode.

    - /identity/* is open (register + login), matching the gateway route.
    - Every other mounted service requires a Bearer token issued by
      identity-service; the caller's user id is injected as X-User-ID
      (never trusted from the client).
    - X-Book-ID is validated against the caller's ACTIVE membership, else 403
      - same as BookContextMiddleware. The memberships used to live in the
      book-sync service's sqlite table; that service is in the engine now, so
      the check asks the engine, which is also what holds the Book.
    """

    OPEN_PREFIXES = (
        "/identity",  # register + login (gateway route is AuthRequired:false)
        "/health",  # local runtime meta
        "/docs",
        "/redoc",
        "/openapi.json",
    )
    OPEN_EXACT = ("", "/")

    def __init__(self, app, jwt_secret, membership_url=None, extra_open=()):
        super().__init__(app)
        self.jwt_secret = jwt_secret
        # Where the Book lives. None means no engine was built on this machine, and a book-scoped call is then
        # refused rather than let through unverified.
        self.membership_url = membership_url.rstrip("/") if membership_url else None
        self.open = tuple(self.OPEN_PREFIXES) + tuple(extra_open)

    def _is_member(self, user_id, book_id):
        """True when the engine says this user is an active member of this Book."""
        answer = httpx.get(
            f"{self.membership_url}/books/{book_id}/membership",
            headers={"x-user-id": user_id, "x-book-id": book_id},
            timeout=5,
        )
        # The engine answers 404 when there is no active membership, and 403 when the caller may not look.
        return answer.status_code == 200

    async def dispatch(self, request, call_next):
        path = request.url.path
        if path in self.OPEN_EXACT or path.startswith(self.open):
            return await call_next(request)

        auth = request.headers.get("authorization", "")
        if not auth.startswith("Bearer "):
            return JSONResponse({"detail": "Not authenticated"}, status_code=401)
        try:
            payload = jwt.decode(auth[7:], self.jwt_secret, algorithms=["HS256"])
        except Exception:
            return JSONResponse({"detail": "Invalid or expired token"}, status_code=401)
        user_id = payload.get("sub")
        if not user_id:
            return JSONResponse({"detail": "Invalid token payload"}, status_code=401)

        book_id = request.headers.get("x-book-id")
        if book_id:
            if self.membership_url is None:
                return JSONResponse(
                    {"detail": "No engine on this machine, so Book membership cannot be checked"},
                    status_code=503,
                )
            if not self._is_member(user_id, book_id):
                return JSONResponse({"detail": f"No active membership in book {book_id}"}, status_code=403)

        # Inject the verified identity; drop any client-supplied header.
        headers = [(k, v) for k, v in request.scope["headers"] if k.decode().lower() != "x-user-id"]
        headers.append((b"x-user-id", user_id.encode()))
        request.scope["headers"] = headers
        return await call_next(request)


def load_or_create_secret(data_dir):
    """A stable local JWT secret, created on first boot."""
    path = os.path.join(data_dir, "jwt_secret")
    if os.path.isfile(path):
        with open(path, encoding="utf-8") as fh:
            return fh.read().strip()
    import secrets

    secret = secrets.token_urlsafe(48)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(secret)
    try:
        os.chmod(path, 0o600)
    except OSError:
        pass  # Windows: ACLs apply instead
    return secret
