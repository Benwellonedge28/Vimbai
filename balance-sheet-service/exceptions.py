"""Custom exceptions for Balance Sheet Service"""


class BalanceSheetError(Exception):
    """Base exception for Balance Sheet service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(BalanceSheetError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ValidationError(BalanceSheetError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)
