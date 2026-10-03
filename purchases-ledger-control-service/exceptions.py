"""Custom exceptions for Purchases Ledger Control Service"""


class PurchasesLedgerControlError(Exception):
    """Base exception for Purchases Ledger Control service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(PurchasesLedgerControlError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)
