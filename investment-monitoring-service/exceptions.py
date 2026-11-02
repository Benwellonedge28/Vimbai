"""Custom exceptions for the Investment Monitoring Service"""


class InvestmentMonitoringError(Exception):
    """Base exception for bad debt service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(InvestmentMonitoringError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)


class ConflictError(InvestmentMonitoringError):
    def __init__(self, detail: str, code: str = "CONFLICT"):
        super().__init__(detail, code, 400)
