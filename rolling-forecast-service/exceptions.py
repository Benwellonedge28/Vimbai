"""Custom exceptions for Rolling Forecast Service"""


class RollingForecastError(Exception):
    """Base exception for the service"""

    def __init__(self, message: str = "Rolling forecast error"):
        self.message = message
        super().__init__(self.message)


class NotFoundError(RollingForecastError):
    """Raised when a forecast cannot be found in the caller's scope"""


class ValidationError(RollingForecastError):
    """Raised when input fails validation"""
