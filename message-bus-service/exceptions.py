"""Custom exceptions for Message Bus Service"""


class MessageBusServiceError(Exception):
    """Base exception for Message Bus service errors"""

    def __init__(self, detail: str, code: str = None, status_code: int = 500):
        self.detail = detail
        self.code = code
        self.status_code = status_code
        super().__init__(detail)


class NotFoundError(MessageBusServiceError):
    def __init__(self, detail: str, code: str = "NOT_FOUND"):
        super().__init__(detail, code, 404)
