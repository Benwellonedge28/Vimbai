"""Custom exceptions for Sovereign Treasury Service"""


class SovereignTreasuryError(Exception):
    """Base exception for Sovereign Treasury service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(SovereignTreasuryError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ValidationError(SovereignTreasuryError):
    def __init__(self, detail: str, code: str = "VALIDATION_ERROR"):
        super().__init__(detail, code, 422)
