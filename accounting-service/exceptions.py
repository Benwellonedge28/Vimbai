from fastapi import HTTPException, status

class FinAccException(HTTPException):
    def __init__(self, status_code: int, detail: str, code: str = "GENERIC_ERROR"):
        super().__init__(status_code=status_code, detail={"detail": detail, "code": code})
        self.code = code

class NotFoundError(FinAccException):
    def __init__(self, detail: str = "Resource not found", code: str = "NOT_FOUND"):
        super().__init__(status_code=status.HTTP_404_NOT_FOUND, detail=detail, code=code)

class ConflictError(FinAccException):
    def __init__(self, detail: str = "Resource already exists or conflicts with existing data", code: str = "CONFLICT_ERROR"):
        super().__init__(status_code=status.HTTP_409_CONFLICT, detail=detail, code=code)

class ValidationError(FinAccException):
    def __init__(self, detail: str = "Invalid input data", code: str = "VALIDATION_ERROR"):
        super().__init__(status_code=status.HTTP_422_UNPROCESSABLE_ENTITY, detail=detail, code=code)

class UnauthorizedError(FinAccException):
    def __init__(self, detail: str = "Authentication required or invalid credentials", code: str = "UNAUTHORIZED"):
        super().__init__(status_code=status.HTTP_401_UNAUTHORIZED, detail=detail, code=code)

class ForbiddenError(FinAccException):
    def __init__(self, detail: str = "Not enough permissions", code: str = "FORBIDDEN"):
        super().__init__(status_code=status.HTTP_403_FORBIDDEN, detail=detail, code=code)
