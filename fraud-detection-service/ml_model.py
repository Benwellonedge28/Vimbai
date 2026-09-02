import random
from datetime import datetime
from typing import Any, Dict, Tuple

from fraud_detection_service.models import FraudDetectionResult, TransactionForFraudCheck

# This is a highly simplified, rule-based "ML model" for demonstration purposes.
# In a real scenario, this would be a trained machine learning model (e.g., RandomForest, XGBoost, Neural Network)
# loaded from a serialized file (.pkl, .joblib, etc.) and much more complex feature engineering.


class FraudDetector:
    def __init__(self):
        self.model_version = "v1.0-rule_based"
        # Simulate loading a model (e.g., from disk)
        print(f"Fraud Detection Model {self.model_version} initialized.")

    def _extract_features(self, transaction: TransactionForFraudCheck) -> Dict[str, Any]:
        """Convert transaction data into features for the model."""
        features = {
            "amount": float(transaction.amount),
            "transaction_type": transaction.transaction_type,
            "previous_transactions_count_24h": transaction.previous_transactions_count_24h,
            "avg_daily_transaction_amount_7d": float(transaction.avg_daily_transaction_amount_7d),
            "is_large_transaction": float(transaction.amount > 5000),  # Rule example
            "is_unusual_time": float(transaction.timestamp.hour < 6 or transaction.timestamp.hour > 22),  # Rule example
            "same_sender_recipient": float(transaction.sender_account_id == transaction.recipient_account_id),
            # Add more features from location_data, device_info, etc.
        }
        return features

    def predict_fraud(self, transaction: TransactionForFraudCheck) -> FraudDetectionResult:
        """
        Predicts the fraudulence of a transaction using a simplified rule-based system.
        """
        features = self._extract_features(transaction)
        fraud_score = 0.0
        reason = []

        # Rule 1: Very large transactions
        if features["amount"] > 10000:
            fraud_score += 0.4
            reason.append("Large transaction amount.")
        # Rule 2: High frequency in last 24h
        if features["previous_transactions_count_24h"] > 10 and features["amount"] > 100:
            fraud_score += 0.3
            reason.append("High transaction frequency in 24 hours.")
        # Rule 3: Unusual time
        if features["is_unusual_time"] > 0.0:
            fraud_score += 0.1
            reason.append("Transaction occurred during unusual hours.")
        # Rule 4: Transaction type often associated with fraud (e.g., transfer to self without clear reason)
        if features["same_sender_recipient"] > 0.0 and transaction.transaction_type == "transfer":
            fraud_score += 0.2
            reason.append("Self-transfer observed.")
        # Rule 5: Deviation from average amount
        if features["avg_daily_transaction_amount_7d"] > 0 and features["amount"] > (
            features["avg_daily_transaction_amount_7d"] * 3
        ):
            fraud_score += 0.2
            reason.append("Transaction amount significantly higher than average.")

        # Add some random noise for realism
        fraud_score = min(1.0, max(0.0, fraud_score + random.uniform(-0.05, 0.05)))

        # Determine fraud flag
        if fraud_score >= 0.8:
            fraud_flag = "high_risk"
        elif fraud_score >= 0.5:
            fraud_flag = "suspicious"
        elif fraud_score >= 0.2:
            fraud_flag = "low_risk"
        else:
            fraud_flag = "safe"
            reason = ["Transaction appears normal."]

        return FraudDetectionResult(
            transaction_id=transaction.transaction_id,
            fraud_score=fraud_score,
            fraud_flag=fraud_flag,
            reason=". ".join(reason) if reason else "No specific issues detected.",
            model_version=self.model_version,
        )
