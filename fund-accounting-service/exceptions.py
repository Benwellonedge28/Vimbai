"""Custom exceptions for Fund Accounting Service"""


class FundAccountingError(Exception):
    """Base exception for Fund Accounting service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(FundAccountingError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ValidationError(FundAccountingError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)
