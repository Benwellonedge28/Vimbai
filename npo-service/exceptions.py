"""Custom exceptions for NPO Service"""


class NPOError(Exception):
    """Base exception for NPO service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(NPOError):
    """Resource not found"""

    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ConflictError(NPOError):
    """Resource conflict"""

    def __init__(self, detail: str, code: str = "CONFLICT"):
        super().__init__(detail, code, 409)


class ValidationError(NPOError):
    """Validation error"""

    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)


class UnauthorizedError(NPOError):
    """Unauthorized access"""

    def __init__(self, detail: str, code: str = "UNAUTHORIZED"):
        super().__init__(detail, code, 401)


class ForbiddenError(NPOError):
    """Forbidden access"""

    def __init__(self, detail: str, code: str = "FORBIDDEN"):
        super().__init__(detail, code, 403)


class InsufficientFundsError(NPOError):
    """Insufficient funds in fund"""

    def __init__(self, detail: str, code: str = "INSUFFICIENT_FUNDS"):
        super().__init__(detail, code, 400)


class RestrictionViolationError(NPOError):
    """Fund restriction violation"""

    def __init__(self, detail: str, code: str = "RESTRICTION_VIOLATION"):
        super().__init__(detail, code, 400)
