"""In-process gateway contract for the Vimbai local runtime.

Replicates the production API gateway behaviour for a single-process
deployment: JWT verification, identity-header injection, Book-context
membership gating, and open auth routes for /identity (register/login).
"""

import os
import sqlite3

import jwt
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse


class LocalGatewayMiddleware(BaseHTTPMiddleware):
    """Production-parity auth for local mode.

    - /identity/* is open (register + login), matching the gateway route.
    - Every other mounted service requires a Bearer token issued by
      identity-service; the caller's user id is injected as X-User-ID
      (never trusted from the client).
    - X-Book-ID is validated against the caller's ACTIVE membership in the
      local book-sync database, else 403 - same as BookContextMiddleware.
    """

    OPEN_PREFIXES = (
        "/identity",  # register + login (gateway route is AuthRequired:false)
        "/health",  # local runtime meta
        "/docs",
        "/redoc",
        "/openapi.json",
    )
    OPEN_EXACT = ("", "/")

    def __init__(self, app, jwt_secret, book_sync_db, extra_open=()):
        super().__init__(app)
        self.jwt_secret = jwt_secret
        self.book_sync_db = book_sync_db
        self.open = tuple(self.OPEN_PREFIXES) + tuple(extra_open)

    def _is_member(self, user_id, book_id):
        conn = sqlite3.connect(self.book_sync_db)
        try:
            row = conn.execute(
                "SELECT 1 FROM memberships WHERE book_id=? AND user_id=? AND status='active'",
                (book_id, user_id),
            ).fetchone()
            return row is not None
        finally:
            conn.close()

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
        if book_id and not self._is_member(user_id, book_id):
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
