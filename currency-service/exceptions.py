"""Custom exceptions for Currency Service"""


class CurrencyError(Exception):
    """Base exception for Currency service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(CurrencyError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ConflictError(CurrencyError):
    def __init__(self, detail: str, code: str = "CONFLICT", existing_id: str = None):
        super().__init__(detail, code, 409)
        self.existing_id = existing_id


class ValidationError(CurrencyError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)
