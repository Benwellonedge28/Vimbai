"""Add indexes for production service entities."""

def upgrade(session):
    if session is None:
        print("  [dry-run] Would create service entity indexes")
        return
    
    # Treasury entities
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:CashFlow) ON (c.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:CashFlow) ON (c.flow_type)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:CashPosition) ON (c.company_id)")
    
    # Compliance entities
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:ComplianceCheck) ON (c.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:ComplianceCheck) ON (c.status)")
    
    # Tax entities
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:TaxReturn) ON (t.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:TaxReturn) ON (t.tax_year)")
    
    # Revenue recognition
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:Contract) ON (c.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:Contract) ON (c.status)")
    
    # Cost accounting
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:CostCenter) ON (c.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:CostAllocation) ON (c.company_id)")
    
    # Expense tracking
    session.run("CREATE INDEX IF NOT EXISTS FOR (e:Expense) ON (e.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (e:Expense) ON (e.status)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (e:Expense) ON (e.category)")
    
    # Appropriation control
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:Appropriation) ON (a.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:Appropriation) ON (a.fiscal_year)")
    
    # Scenario analysis
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:Scenario) ON (s.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (s:Scenario) ON (s.scenario_type)")
    
    # Zero-based budgeting
    session.run("CREATE INDEX IF NOT EXISTS FOR (z:BudgetPackage) ON (z.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (z:BudgetPackage) ON (z.status)")
    
    # Balance sheet / cash flow
    session.run("CREATE INDEX IF NOT EXISTS FOR (b:BalanceSheet) ON (b.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:CashFlowStatement) ON (c.company_id)")
    
    # Audit engagement entities
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditEngagement) ON (a.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditEngagement) ON (a.status)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditFinding) ON (a.engagement_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditFinding) ON (a.severity)")
    
    # Costing services
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:Job) ON (j.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:Job) ON (j.status)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:ProcessCost) ON (c.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (c:ProductCost) ON (c.company_id)")
    
    print("  Created 25 indexes for production service entities")

def downgrade(session):
    if session is None:
        print("  [dry-run] Would drop service entity indexes")
        return
    print("  Downgrade is a no-op (indexes preserved for data safety)")
