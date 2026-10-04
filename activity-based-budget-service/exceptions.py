"""Custom exceptions for Accounting Standards Service"""


class ActivityBasedBudgetError(Exception):
    """Base exception for Accounting Standards service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(ActivityBasedBudgetError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ValidationError(ActivityBasedBudgetError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 400)
