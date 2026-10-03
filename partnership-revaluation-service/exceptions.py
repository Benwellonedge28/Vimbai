"""Custom exceptions for Partnership Revaluation Service"""


class PartnershipRevaluationServiceError(Exception):
    """Base exception for Partnership Revaluation service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(PartnershipRevaluationServiceError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)
