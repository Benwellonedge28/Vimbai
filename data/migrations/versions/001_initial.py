"""Initial database schema migration - core constraints and indexes."""


def upgrade(session):
    if session is None:
        print("  [dry-run] Would create core constraints and indexes")
        return

    # Identity constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (u:User) REQUIRE u.email IS UNIQUE")
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (u:User) REQUIRE u.id IS UNIQUE")
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (o:Organization) REQUIRE o.id IS UNIQUE")
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (r:Role) REQUIRE r.name IS UNIQUE")

    # Accounting constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (a:Account) REQUIRE a.account_number IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:Account) ON (a.account_type)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:Account) ON (a.company_id)")

    # Journal entry constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (j:JournalEntry) REQUIRE j.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:JournalEntry) ON (j.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:JournalEntry) ON (j.entry_date)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (j:JournalEntry) ON (j.status)")

    # Transaction constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (t:Transaction) REQUIRE t.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:Transaction) ON (t.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:Transaction) ON (t.transaction_date)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (t:Transaction) ON (t.category)")

    # Budget constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (b:Budget) REQUIRE b.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (b:Budget) ON (b.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (b:Budget) ON (b.fiscal_year)")

    # Fraud detection constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (f:FraudAlert) REQUIRE f.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (f:FraudAlert) ON (f.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (f:FraudAlert) ON (f.severity)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (f:FraudAlert) ON (f.status)")

    # Risk assessment constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (r:Risk) REQUIRE r.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (r:Risk) ON (r.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (r:Risk) ON (r.category)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (r:Risk) ON (r.level)")

    # Audit constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (a:AuditLog) REQUIRE a.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditLog) ON (a.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditLog) ON (a.timestamp)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (a:AuditLog) ON (a.action)")

    # Webhook constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (w:WebhookEndpoint) REQUIRE w.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (w:WebhookEndpoint) ON (w.company_id)")

    # Policy constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (p:PolicyRule) REQUIRE p.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (p:PolicyRule) ON (p.company_id)")

    # State machine constraints
    session.run("CREATE CONSTRAINT IF NOT EXISTS FOR (d:FinancialDocument) REQUIRE d.id IS UNIQUE")
    session.run("CREATE INDEX IF NOT EXISTS FOR (d:FinancialDocument) ON (d.company_id)")
    session.run("CREATE INDEX IF NOT EXISTS FOR (d:FinancialDocument) ON (d.current_state)")

    print("  Created 30 constraints and indexes for core entities")


def downgrade(session):
    if session is None:
        print("  [dry-run] Would drop core constraints and indexes")
        return
    print("  Downgrade is a no-op (constraints preserved for data safety)")
