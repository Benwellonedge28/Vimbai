import py_compile
import sys

files_to_check = [
    "automation-engine-service/main.py",
    "banking-integration-service/main.py",
    "business-documents-service/main.py",
    "cache-service/main.py",
    "exotic-derivatives-service/main.py",
    "government-grants-service/main.py",
    "labour-cost-variance-service/main.py",
    "labour-efficiency-variance-service/main.py",
    "make-or-buy-decision-service/main.py",
    "partnership-accounting-service/main.py",
    "partnership-sale-service/main.py",
    "payroll-accounting-service/main.py",
    "profit-loss-account-service/main.py",
    "sales-price-variance-service/main.py",
    "sales-volume-variance-service/main.py",
    "supply-chain-service/main.py",
    "suspense-error-service/main.py",
    "tax-calculation-service/main.py",
    "trading-account-service/main.py",
]

for file_path in files_to_check:
    try:
        py_compile.compile(file_path, doraise=True)
    except py_compile.PyCompileError as e:
        print(f"--- Syntax error in {file_path} ---")
        print(e.exc_value)
        print()
