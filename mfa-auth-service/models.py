"""Pydantic models for the MFA Auth Service."""

from typing import List, Optional

from pydantic import BaseModel


class MFASetupRequest(BaseModel):
    user_id: str
    method: str = "totp"  # totp, sms


class MFASetupResponse(BaseModel):
    user_id: str
    secret: str
    qr_uri: str
    backup_codes: List[str]


class MFAVerifyRequest(BaseModel):
    user_id: str
    code: str


class MFAVerifyResponse(BaseModel):
    verified: bool
    user_id: str
    message: str
    access_token: Optional[str] = None
