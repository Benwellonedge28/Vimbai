import pytest
from unittest.mock import Mock, patch
import numpy as np

def test_fraud_score_calculation():
    """Test fraud score calculation logic"""
    from fraud_detection_service.ml_model import calculate_risk_score
    
    transaction = {
        "amount": 5000,
        "previous_transactions_count_24h": 10,
        "avg_daily_transaction_amount_7d": 500
    }
    
    score = calculate_risk_score(transaction)
    assert 0 <= score <= 1

def test_anamoly_detection():
    """Test anomaly detection with mock data"""
    from fraud_detection_service.ml_model import detect_anomalies
    
    # Generate normal transactions
    normal_amounts = np.random.normal(500, 100, 100)
    
    # Add outliers
    test_amounts = np.concatenate([normal_amounts, [10000, 15000]])
    
    anomalies = detect_anomalies(test_amounts, threshold=3.0)
    assert len(anomalies) >= 2  # Should detect outliers
