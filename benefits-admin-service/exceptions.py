"""Custom exceptions for Benefits Administration Service"""


class BenefitsError(Exception):
    """Base exception for Benefits Administration service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(BenefitsError):
    """Resource not found"""

    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ConflictError(BenefitsError):
    """Resource conflict"""

    def __init__(self, detail: str, code: str = "CONFLICT"):
        super().__init__(detail, code, 409)


class ValidationError(BenefitsError):
    """Validation error"""

    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)


class UnauthorizedError(BenefitsError):
    """Unauthorized access"""

    def __init__(self, detail: str, code: str = "UNAUTHORIZED"):
        super().__init__(detail, code, 401)
