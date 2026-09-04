"""Custom exceptions for Expense Tracking Service"""


class ExpenseError(Exception):
    """Base exception for Expense Tracking service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(ExpenseError):
    """Resource not found"""

    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ValidationError(ExpenseError):
    """Validation error"""

    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)


class UnauthorizedError(ExpenseError):
    """Unauthorized access"""

    def __init__(self, detail: str, code: str = "UNAUTHORIZED"):
        super().__init__(detail, code, 401)
