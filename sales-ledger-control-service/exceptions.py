"""Custom exceptions for Sales Ledger Control Service"""


class SalesLedgerControlError(Exception):
    """Base exception for Sales Ledger Control service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(SalesLedgerControlError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)
