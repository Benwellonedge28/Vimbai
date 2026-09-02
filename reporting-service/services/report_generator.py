import json
from datetime import datetime
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession


class ReportGenerator:
    """Generates reports from graph database using Cypher queries"""

    def __init__(self, session: AsyncSession):
        self.session = session

    async def execute_query(self, query: str, parameters: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Execute a Cypher query and return results as list of dicts"""
        result = await self.session.run(query, **parameters)
        records = []
        async for record in result:
            rec = {}
            for key, value in record.items():
                if hasattr(value, "iso_format"):
                    rec[key] = value.iso_format()
                elif isinstance(value, dict):
                    rec[key] = {k: v.iso_format() if hasattr(v, "iso_format") else v for k, v in value.items()}
                else:
                    rec[key] = value
            records.append(rec)
        return records

    async def generate_financial_summary(self, start_date: str, end_date: str) -> Dict[str, Any]:
        """Generate a comprehensive financial summary report"""
        query = """
        MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)
        WHERE je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
        AND je.status = 'posted'
        RETURN a.account_number as account_number, a.name as account_name, a.account_type as account_type,
               SUM(jl.debit) as total_debits, SUM(jl.credit) as total_credits
        ORDER BY a.account_type, a.account_number
        """
        results = await self.execute_query(query, {"start_date": start_date, "end_date": end_date, "user_id": "system"})

        summary = {"revenues": [], "expenses": [], "assets": [], "liabilities": [], "equity": []}
        for row in results:
            if row["account_type"] == "revenue":
                summary["revenues"].append(row)
            elif row["account_type"] == "expense":
                summary["expenses"].append(row)
            elif row["account_type"] == "asset":
                summary["assets"].append(row)
            elif row["account_type"] == "liability":
                summary["liabilities"].append(row)
            elif row["account_type"] == "equity":
                summary["equity"].append(row)

        return summary

    async def generate_account_activity(
        self, account_number: str, start_date: str, end_date: str
    ) -> List[Dict[str, Any]]:
        """Generate detailed account activity report"""
        query = """
        MATCH (u:User)-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {account_number: $account_number})
        WHERE je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
        AND je.status = 'posted'
        RETURN je.id as entry_id, je.entry_date as date, je.description as description,
               jl.debit as debit, jl.credit as credit, je.source_module as source
        ORDER BY je.entry_date
        """
        return await self.execute_query(
            query, {"account_number": account_number, "start_date": start_date, "end_date": end_date}
        )
