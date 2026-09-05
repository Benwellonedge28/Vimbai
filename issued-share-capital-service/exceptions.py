"""Custom exceptions for Tax Accounting Service"""


class IssuedShareCapitalError(Exception):
    """Base exception for Cashbook service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(IssuedShareCapitalError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ConflictError(IssuedShareCapitalError):
    def __init__(self, detail: str, code: str = "CONFLICT"):
        super().__init__(detail, code, 409)


class ValidationError(IssuedShareCapitalError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)
