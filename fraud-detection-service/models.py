from pydantic import BaseModel, Field, condecimal, validator
from typing import Optional, List, Literal, Dict, Any
from datetime import datetime
from decimal import Decimal

# --- Transaction Models for Fraud Check ---
class TransactionForFraudCheck(BaseModel):
    transaction_id: str = Field(..., description="Unique ID of the transaction.")
    amount: condecimal(decimal_places=2, gt=Decimal('0.00')) = Field(..., description="Amount of the transaction.")
    currency: str = Field("USD", max_length=3, description="Currency of the transaction (ISO 4217).")
    sender_account_id: str = Field(..., description="ID of the sender's account.")
    recipient_account_id: str = Field(..., description="ID of the recipient's account.")
    transaction_type: Literal["debit", "credit", "transfer", "payment", "purchase"] = Field(..., description="Type of transaction.")
    timestamp: datetime = Field(default_factory=datetime.utcnow, description="Timestamp of the transaction.")
    # Additional fields that might be useful for fraud detection
    location_data: Optional[Dict[str, Any]] = Field(None, description="Geographic location data of the transaction.")
    device_info: Optional[Dict[str, Any]] = Field(None, description="Device information (e.g., IP address, OS, browser).")
    previous_transactions_count_24h: int = Field(0, ge=0, description="Number of transactions by sender in last 24h.")
    avg_daily_transaction_amount_7d: condecimal(decimal_places=2, ge=Decimal('0.00')) = Field(Decimal('0.00'), description="Average daily transaction amount by sender over 7 days.")

    @validator('amount', pre=True)
    def convert_to_decimal(cls, v):
        if isinstance(v, float):
            return Decimal(str(v))
        return v

# --- Fraud Detection Results ---
class FraudDetectionResult(BaseModel):
    transaction_id: str = Field(..., description="ID of the transaction that was analyzed.")
    fraud_score: float = Field(..., ge=0.0, le=1.0, description="Probability or score indicating likelihood of fraud (0-1).")
    fraud_flag: Literal["safe", "low_risk", "suspicious", "high_risk"] = Field(..., description="Categorical flag for fraud risk.")
    reason: Optional[str] = Field(None, description="Reason or rules triggered for the flag.")
    model_version: str = Field(..., description="Version of the ML model used for detection.")

# --- Stored Fraudulent Transaction Flag ---
class FraudulentTransactionFlagCreate(BaseModel):
    transaction_id: str = Field(..., description="ID of the transaction flagged as fraudulent.")
    fraud_score: float = Field(..., ge=0.0, le=1.0, description="Fraud score at the time of flagging.")
    fraud_flag: Literal["safe", "low_risk", "suspicious", "high_risk"] = Field(..., description="Categorical flag for fraud risk.")
    reason: str = Field(..., description="Reason or rules triggered for the flag.")
    model_version: str = Field(..., description="Version of the ML model used for detection.")
    flagged_by_user_id: Optional[str] = Field(None, description="User who manually flagged this (if any).")
    status: Literal["open", "investigating", "false_positive", "confirmed_fraud"] = Field("open", description="Current status of the fraud flag.")

class FraudulentTransactionFlagInDB(FraudulentTransactionFlagCreate):
    id: str = Field(..., example="uuid-string-for-node")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Standard Error Response ---
class ErrorResponse(BaseModel):
    detail: str
    code: Optional[str] = None
    status_code: int = 500
