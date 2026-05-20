# ... (existing imports and models) ...

# --- Error Response Model (NEW) ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
