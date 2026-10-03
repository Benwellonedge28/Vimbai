"""Custom exceptions for SOX Compliance Service"""


class SoxComplianceServiceError(Exception):
    """Base exception for SOX Compliance service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(SoxComplianceServiceError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)
