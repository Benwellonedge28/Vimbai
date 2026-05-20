# ... (existing imports and endpoints) ...

# --- Financial Analysis Endpoints (NEW) ---

@app.get("/budgets/{budget_id}/variance-report", response_model=models.BudgetVarianceReport,
             dependencies=[Depends(check_permission("finance.read.variance_reports"))])
async def get_budget_variance_report(budget_id: str, db_session: AsyncSession = Depends(get_db_session)):
    report = await crud.generate_budget_variance_report(db_session, budget_id)
    if report is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Budget not found")
    return report

# --- Root endpoint for health check (unchanged) ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Finance Service is running!"}
