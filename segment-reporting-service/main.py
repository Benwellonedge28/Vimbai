"""
Segment Reporting Service
Port: 8140
Prepares segment reports for business divisions and geographic areas
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Segment Reporting Service", version="1.0.0")

# Pydantic Models
class SegmentDefinition(BaseModel):
    segment_id: str
    segment_name: str
    segment_type: str  # "business", "geographic", "product"
    reportable: bool = True
    aggregation_level: Optional[str] = None

class SegmentRevenue(BaseModel):
    external_revenue: float
    inter_segment_revenue: float
    total_revenue: float

class SegmentProfit(BaseModel):
    operating_profit: float
    interest_revenue: float
    interest_expense: float
    profit_before_tax: float
    income_tax_expense: float
    profit_after_tax: float

class SegmentAssets(BaseModel):
    operating_assets: float
    inter_segment_assets: float
    total_assets: float
    inter_segment_liabilities: float

class SegmentInvestment(BaseModel):
    capital_expenditure: float
    depreciation: float
    amortization: float
    impairment: float

class GeographicSegment(BaseModel):
    region_code: str
    region_name: str
    country: str
    revenue: float
    assets: float
    profit: float

class SegmentReportRequest(BaseModel):
    company_id: str
    period_start: str
    period_end: str
    segments: List[SegmentDefinition]
    include_geographic: bool = False
    include_product: bool = False
    measurement_basis: str = "IFRS"  # IFRS or US GAAP

class SegmentReportResponse(BaseModel):
    period: str
    segments: Dict[str, Dict[str, Any]]
    geographic_segments: Optional[Dict[str, Dict[str, Any]]] = None
    reconciliation_to_group: Dict[str, float]
    total_group_revenue: float
    total_group_profit: float
    total_group_assets: float
    threshold_test_results: Dict[str, bool]

class ThresholdTest(BaseModel):
    segment_id: str
    revenue_test: bool  # >= 10% of total
    profit_test: bool   # >= 10% of total profit or loss
    assets_test: bool   # >= 10% of total assets

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal FinAcc service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "segment-reporting", "version": "1.0.0"}

@app.post("/report", response_model=SegmentReportResponse)
async def prepare_segment_report(request: SegmentReportRequest):
    """Prepare comprehensive segment report."""
    logger.info("Preparing segment report", company=request.company_id, period=request.period_end)

    # Simulate segment data
    segment_data = {}
    total_revenue = 0.0
    total_profit = 0.0
    total_assets = 0.0

    for segment in request.segments:
        seg_revenue = 5000000.0 + hash(segment.segment_id) % 5000000
        seg_profit = seg_revenue * 0.15
        seg_assets = seg_revenue * 3
        seg_capex = seg_revenue * 0.08
        seg_depreciation = seg_revenue * 0.05

        segment_data[segment.segment_id] = {
            "segment_name": segment.segment_name,
            "segment_type": segment.segment_type,
            "revenue": {
                "external": seg_revenue * 0.85,
                "inter_segment": seg_revenue * 0.15,
                "total": seg_revenue
            },
            "profit": {
                "operating_profit": seg_profit,
                "interest_revenue": seg_profit * 0.02,
                "interest_expense": seg_profit * 0.03,
                "profit_before_tax": seg_profit * 0.99,
                "income_tax": seg_profit * 0.25,
                "profit_after_tax": seg_profit * 0.74
            },
            "assets": {
                "operating_assets": seg_assets,
                "inter_segment_assets": seg_assets * 0.1,
                "total": seg_assets * 1.1
            },
            "investments": {
                "capital_expenditure": seg_capex,
                "depreciation": seg_depreciation,
                "amortization": seg_capex * 0.1,
                "impairment": 0.0
            },
            "reportable": segment.reportable
        }

        total_revenue += seg_revenue
        total_profit += seg_profit
        total_assets += seg_assets * 1.1

    # Geographic segments if requested
    geographic_data = None
    if request.include_geographic:
        geographic_data = {
            "north_america": {
                "region_name": "North America",
                "revenue": total_revenue * 0.4,
                "assets": total_assets * 0.35,
                "profit": total_profit * 0.42
            },
            "europe": {
                "region_name": "Europe",
                "revenue": total_revenue * 0.30,
                "assets": total_assets * 0.32,
                "profit": total_profit * 0.28
            },
            "asia_pacific": {
                "region_name": "Asia Pacific",
                "revenue": total_revenue * 0.20,
                "assets": total_assets * 0.25,
                "profit": total_profit * 0.22
            },
            "rest_of_world": {
                "region_name": "Rest of World",
                "revenue": total_revenue * 0.10,
                "assets": total_assets * 0.08,
                "profit": total_profit * 0.08
            }
        }

    # Threshold tests (>= 10% for reportable)
    threshold_results = {}
    for seg_id, seg_data in segment_data.items():
        threshold_results[seg_id] = {
            "revenue_test": seg_data["revenue"]["total"] >= total_revenue * 0.10,
            "profit_test": seg_data["profit"]["profit_after_tax"] >= abs(total_profit * 0.10),
            "assets_test": seg_data["assets"]["total"] >= total_assets * 0.10,
            "passes_any_test": (
                seg_data["revenue"]["total"] >= total_revenue * 0.10 or
                seg_data["profit"]["profit_after_tax"] >= abs(total_profit * 0.10) or
                seg_data["assets"]["total"] >= total_assets * 0.10
            )
        }

    # Reconciliation to group
    reconciliation = {
        "elimination_inter_segment_revenue": total_revenue * 0.15 * 0.1,  # 10% eliminated
        "unallocated_assets": total_assets * 0.05,
        "unallocated_expenses": total_profit * 0.03,
        "consolidation_adjustments": -total_revenue * 0.01
    }

    response = SegmentReportResponse(
        period=f"{request.period_start} to {request.period_end}",
        segments=segment_data,
        geographic_segments=geographic_data,
        reconciliation_to_group=reconciliation,
        total_group_revenue=total_revenue * 0.89,
        total_group_profit=total_profit * 0.97,
        total_group_assets=total_assets,
        threshold_test_results={k: v["passes_any_test"] for k, v in threshold_results.items()}
    )

    logger.info("Segment report prepared", segments=len(segment_data), total_revenue=total_revenue)
    return response

@app.post("/threshold-test")
async def test_reportable_segments(request: SegmentReportRequest):
    """Test which segments meet the threshold criteria for separate reporting."""
    # Calculate totals
    segment_revenues = {s.segment_id: 5000000.0 + hash(s.segment_id) % 5000000 for s in request.segments}
    segment_profits = {s.segment_id: r * 0.15 for s, r in zip(request.segments, segment_revenues.values())}
    segment_assets = {s.segment_id: r * 3 for r in segment_revenues.values()}

    total_revenue = sum(segment_revenues.values())
    total_profit = sum(segment_profits.values())
    total_assets = sum(segment_assets.values())

    threshold = 0.10  # 10% threshold

    results = []
    for segment in request.segments:
        rev_test = segment_revenues[segment.segment_id] >= total_revenue * threshold
        profit_test = segment_profits[segment.segment_id] >= abs(total_profit * threshold)
        assets_test = segment_assets[segment.segment_id] >= total_assets * threshold

        results.append({
            "segment_id": segment.segment_id,
            "segment_name": segment.segment_name,
            "revenue_test_passed": rev_test,
            "profit_test_passed": profit_test,
            "assets_test_passed": assets_test,
            "currently_reportable": segment.reportable,
            "should_be_reportable": rev_test or profit_test or assets_test
        })

    return {
        "thresholds": {
            "revenue_threshold": total_revenue * threshold,
            "profit_threshold": abs(total_profit * threshold),
            "assets_threshold": total_assets * threshold
        },
        "segment_results": results
    }

@app.post("/profit-test")
async def perform_profit_test(request: SegmentReportRequest):
    """Perform the profit or loss test for segment reportability."""
    # 75% test: segments representing 75% of total revenue should be reportable
    segment_revenues = [(s.segment_id, 5000000.0 + hash(s.segment_id) % 5000000) for s in request.segments]
    segment_revenues.sort(key=lambda x: x[1], reverse=True)

    total_revenue = sum(r for _, r in segment_revenues)
    cumulative = 0.0
    reportable_count = 0

    for seg_id, rev in segment_revenues:
        cumulative += rev
        if cumulative >= total_revenue * 0.75:
            break
        reportable_count += 1

    return {
        "total_revenue": total_revenue,
        "revenue_for_75_percent": total_revenue * 0.75,
        "minimum_reportable_segments": reportable_count + 1,
        "recommended_reportable": [s[0] for s in segment_revenues[:reportable_count + 1]]
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8140)
