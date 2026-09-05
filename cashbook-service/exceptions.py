"""Custom exceptions for Cashbook Service"""


class CashBookError(Exception):
    """Base exception for Cashbook service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(CashBookError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ConflictError(CashBookError):
    def __init__(self, detail: str, code: str = "CONFLICT"):
        super().__init__(detail, code, 409)


class ValidationError(CashBookError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)
