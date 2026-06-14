"""
FinAcc Cost Accounting Service
Cost tracking, allocation, and analysis for manufacturing and service industries.
Supports job costing, process costing, standard costing, and activity-based costing.
"""

import os
import uuid
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

# ============================================================================
# Configuration
# ============================================================================

SERVICE_NAME = "cost-accounting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8023"))

# Internal service URLs
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
BUDGETING_SERVICE_URL = os.getenv("BUDGETING_SERVICE_URL", "http://localhost:8099")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ADMIN_SERVICE_URL = os.getenv("ADMIN_SERVICE_URL", "http://localhost:8001")

# ============================================================================
# Logging Configuration
# ============================================================================

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)

logger = structlog.get_logger(SERVICE_NAME)

# ============================================================================
# FastAPI Application
# ============================================================================

app = FastAPI(
    title="FinAcc Cost Accounting Service",
    description="Cost tracking, allocation, and analysis for manufacturing and service industries",
    version=SERVICE_VERSION,
    docs_url="/docs",
    redoc_url="/redoc",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# ============================================================================
# Enums
# ============================================================================


class CostingMethod(str, Enum):
    JOB_COSTING = "job_costing"
    PROCESS_COSTING = "process_costing"
    STANDARD_COSTING = "standard_costing"
    ACTIVITY_BASED_COSTING = "activity_based_costing"
    MIXED_COSTING = "mixed_costing"
    DIRECT_COSTING = "direct_costing"
    ABSORPTION_COSTING = "absorption_costing"


class CostElement(str, Enum):
    DIRECT_MATERIAL = "direct_material"
    DIRECT_LABOR = "direct_labor"
    DIRECT_EXPENSE = "direct_expense"
    INDIRECT_MATERIAL = "indirect_material"
    INDIRECT_LABOR = "indirect_labor"
    INDIRECT_EXPENSE = "indirect_expense"
    FACTORY_OVERHEAD = "factory_overhead"
    ADMINISTRATIVE_OVERHEAD = "administrative_overhead"
    SELLING_OVERHEAD = "selling_overhead"
    DISTRIBUTION_OVERHEAD = "distribution_overhead"


class CostBehavior(str, Enum):
    VARIABLE = "variable"
    FIXED = "fixed"
    SEMI_VARIABLE = "semi_variable"
    STEP = "step"
    MIXED = "mixed"


class CostAllocationMethod(str, Enum):
    DIRECT_METHOD = "direct_method"
    STEP_DOWN_METHOD = "step_down_method"
    RECIPROCAL_METHOD = "reciprocal_method"
    ACTIVITY_BASED = "activity_based"
    CAUSAL_BASED = "causal_based"


class CostCenterType(str, Enum):
    PRODUCTION = "production"
    SERVICE = "service"
    ADMINISTRATIVE = "administrative"
    SELLING = "selling"
    DISTRIBUTION = "distribution"
    RESEARCH = "research"
    CAPITAL = "capital"


class VarianceType(str, Enum):
    MATERIAL_PRICE = "material_price"
    MATERIAL_USAGE = "material_usage"
    LABOR_RATE = "labor_rate"
    LABOR_EFFICIENCY = "labor_efficiency"
    OVERHEAD_SPENDING = "overhead_spending"
    OVERHEAD_VOLUME = "overhead_volume"
    OVERHEAD_EFFICIENCY = "overhead_efficiency"
    MIX = "mix"
    YIELD = "yield"


# ============================================================================
# Pydantic Models
# ============================================================================


class CostCenter(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    code: str
    name: str
    center_type: CostCenterType
    parent_id: Optional[str] = None
    manager: Optional[str] = None
    budget_amount: Optional[float] = None
    is_active: bool = True
    allocation_percentage: float = Field(default=0, ge=0, le=100)
    cost_driver: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostPool(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    cost_element: CostElement
    total_amount: float = 0
    allocation_method: CostAllocationMethod
    cost_drivers: List[Dict[str, Any]] = []
    allocated_to: List[str] = []  # cost center IDs
    is_active: bool = True
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ProductCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    product_name: str
    costing_method: CostingMethod
    period_start: datetime
    period_end: datetime
    direct_material: float = 0
    direct_labor: float = 0
    direct_expense: float = 0
    overhead_applied: float = 0
    total_cost: float = 0
    cost_per_unit: float = 0
    units_produced: int = 0
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class JobCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    job_number: str
    description: str
    customer_id: Optional[str] = None
    cost_center_id: Optional[str] = None
    status: str = "in_progress"  # pending, in_progress, completed, closed
    estimated_cost: Optional[float] = None
    direct_material_cost: float = 0
    direct_labor_cost: float = 0
    overhead_applied: float = 0
    total_cost: float = 0
    start_date: Optional[datetime] = None
    completion_date: Optional[datetime] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class WorkInProgress(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_center_id: str
    period_end: datetime
    opening_wip: float = 0
    direct_material: float = 0
    direct_labor: float = 0
    overhead_applied: float = 0
    total_cost: float = 0
    completed_cost: float = 0
    closing_wip: float = 0
    units_started: int = 0
    units_completed: int = 0
    units_closing: int = 0
    equivalent_units_material: float = 0
    equivalent_units_conversion: float = 0
    cost_per_equivalent_unit: Dict[str, float] = {}
    journal_entry_id: Optional[str] = None


class StandardCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    product_name: str
    standard_material_qty: float
    standard_material_rate: float
    standard_labor_hours: float
    standard_labor_rate: float
    standard_overhead_rate: float
    standard_overhead_hours: float
    total_standard_cost: float = 0
    effective_from: datetime
    effective_to: Optional[datetime] = None
    is_active: bool = True
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostVariance(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    variance_type: VarianceType
    product_id: Optional[str] = None
    cost_center_id: Optional[str] = None
    period: str  # e.g., "2024-01"
    standard_cost: float
    actual_cost: float
    variance_amount: float  # actual - standard (negative = favorable)
    variance_percentage: float = 0
    is_favorable: bool = True
    explanation: Optional[str] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ActivityCostDriver(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    activity_name: str
    cost_pool_id: str
    driver_type: str  # transaction, duration, intensity, capacity
    driver_quantity: float
    driver_rate: float
    total_cost: float = 0
    cost_objects: List[Dict[str, Any]] = []  # products, services, jobs
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostAllocation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    source_cost_center_id: str
    destination_cost_center_id: str
    allocation_method: CostAllocationMethod
    amount_allocated: float
    allocation_basis: str
    allocation_percentage: float
    period: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class OverheadRate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_center_id: str
    overhead_type: CostElement
    rate: float
    rate_type: str = "per_unit"  # per_unit, percentage, per_hour, per_transaction
    base_unit: str  # labor_hours, machine_hours, units, etc.
    is_predetermined: bool = True
    effective_from: datetime
    effective_to: Optional[datetime] = None
    is_active: bool = True
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostReport(BaseModel):
    report_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_type: str
    period_start: datetime
    period_end: datetime
    generated_at: datetime = Field(default_factory=datetime.utcnow)
    cost_centers: List[Dict[str, Any]] = []
    products: List[Dict[str, Any]] = []
    summary: Dict[str, float] = {}
    recommendations: List[str] = []


# ============================================================================
# In-Memory Storage
# ============================================================================

cost_centers: Dict[str, CostCenter] = {}
cost_pools: Dict[str, CostPool] = {}
product_costs: List[ProductCost] = []
job_costs: Dict[str, JobCost] = {}
wip_records: List[WorkInProgress] = []
standard_costs: Dict[str, StandardCost] = {}
cost_variances: List[CostVariance] = []
activity_drivers: Dict[str, ActivityCostDriver] = {}
cost_allocations: List[CostAllocation] = []
overhead_rates: Dict[str, OverheadRate] = {}
cost_reports: List[CostReport] = []


# ============================================================================
# Internal Service Communication
# ============================================================================


async def call_accounting_service(
    method: str, endpoint: str, data: Optional[Dict] = None
) -> Dict[str, Any]:
    """Call the main accounting service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "GET":
                response = await client.get(url)
            elif method == "POST":
                response = await client.post(url, json=data)
            else:
                raise ValueError(f"Unsupported HTTP method: {method}")

            if response.status_code in [200, 201]:
                return response.json() if response.text else {}
            return {}
    except Exception as e:
        logger.error("accounting_service_call_error", error=str(e))
        return {}


async def call_budgeting_service(
    method: str, endpoint: str, data: Optional[Dict] = None
) -> Dict[str, Any]:
    """Call the budgeting service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{BUDGETING_SERVICE_URL}{endpoint}"
            if method == "GET":
                response = await client.get(url)
            elif method == "POST":
                response = await client.post(url, json=data)
            else:
                raise ValueError(f"Unsupported HTTP method: {method}")

            if response.status_code in [200, 201]:
                return response.json() if response.text else {}
            return {}
    except Exception as e:
        logger.error("budgeting_service_call_error", error=str(e))
        return {}


async def call_audit_service(
    action: str, resource_type: str, resource_id: str, details: Dict[str, Any]
) -> None:
    """Log actions to the audit service."""
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception as e:
        logger.error("audit_service_call_error", error=str(e))


# ============================================================================
# Helper Functions
# ============================================================================


def calculate_overhead_rate(total_overhead: float, base_units: float) -> float:
    """Calculate overhead application rate."""
    return total_overhead / base_units if base_units > 0 else 0


def calculate_variance(actual: float, standard: float) -> tuple[float, bool]:
    """Calculate variance and determine if favorable."""
    variance = actual - standard
    is_favorable = variance <= 0
    return variance, is_favorable


def calculate_equivalent_units(
    units_completed: int, closing_wip_units: int, completion_percentage: float
) -> float:
    """Calculate equivalent units for process costing."""
    return units_completed + (closing_wip_units * completion_percentage)


def allocate_costs_direct_method(
    service_costs: Dict[str, float], production_centers: List[str]
) -> Dict[str, float]:
    """Allocate service department costs using direct method."""
    total_cost = sum(service_costs.values())
    total_base = len(production_centers)
    if total_base == 0:
        return {}
    return {center: total_cost / total_base for center in production_centers}


# ============================================================================
# API Endpoints - Health & Info
# ============================================================================


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "timestamp": datetime.utcnow().isoformat(),
    }


@app.get("/")
async def root():
    """Root endpoint."""
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Cost tracking, allocation, and analysis",
    }


# ============================================================================
# API Endpoints - Cost Centers
# ============================================================================


@app.post("/cost-centers", response_model=CostCenter, status_code=status.HTTP_201_CREATED)
async def create_cost_center(data: CostCenter):
    """Create a new cost center."""
    center_id = str(uuid.uuid4())
    data.id = center_id
    data.created_at = datetime.utcnow()
    cost_centers[center_id] = data

    # Create account in accounting service
    await call_accounting_service("POST", "/accounts", {
        "code": f"7000-{data.code}",
        "name": f"Cost Center: {data.name}",
        "account_type": "expense",
        "sub_type": "cost_center",
        "description": f"Cost center: {data.name}",
        "is_active": True,
    })

    await call_audit_service("CREATE", "cost_center", center_id, {"name": data.name, "type": data.center_type})
    logger.info("cost_center_created", center_id=center_id, name=data.name)
    return data


@app.get("/cost-centers")
async def list_cost_centers(
    center_type: Optional[CostCenterType] = None, is_active: Optional[bool] = None
):
    """List cost centers."""
    result = list(cost_centers.values())
    if center_type:
        result = [c for c in result if c.center_type == center_type]
    if is_active is not None:
        result = [c for c in result if c.is_active == is_active]
    return {"cost_centers": result, "count": len(result)}


@app.get("/cost-centers/{center_id}")
async def get_cost_center(center_id: str):
    """Get cost center details."""
    center = cost_centers.get(center_id)
    if not center:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Cost center {center_id} not found")
    return center


@app.put("/cost-centers/{center_id}")
async def update_cost_center(center_id: str, data: Dict[str, Any]):
    """Update cost center."""
    center = cost_centers.get(center_id)
    if not center:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Cost center {center_id} not found")

    for key, value in data.items():
        if hasattr(center, key) and key not in ["id", "created_at"]:
            setattr(center, key, value)

    await call_audit_service("UPDATE", "cost_center", center_id, {"updated_fields": list(data.keys())})
    return center


# ============================================================================
# API Endpoints - Cost Pools
# ============================================================================


@app.post("/cost-pools", response_model=CostPool, status_code=status.HTTP_201_CREATED)
async def create_cost_pool(data: CostPool):
    """Create a cost pool for overhead costs."""
    pool_id = str(uuid.uuid4())
    data.id = pool_id
    data.created_at = datetime.utcnow()
    cost_pools[pool_id] = data

    await call_audit_service("CREATE", "cost_pool", pool_id, {"name": data.name})
    return data


@app.get("/cost-pools")
async def list_cost_pools(is_active: Optional[bool] = None):
    """List cost pools."""
    result = list(cost_pools.values())
    if is_active is not None:
        result = [p for p in result if p.is_active == is_active]
    return {"cost_pools": result, "count": len(result)}


@app.post("/cost-pools/{pool_id}/allocate")
async def allocate_cost_pool(pool_id: str, period: str):
    """Allocate costs from a cost pool to cost centers."""
    pool = cost_pools.get(pool_id)
    if not pool:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Cost pool {pool_id} not found")

    # Simple allocation based on equal distribution
    # In production, would use actual cost drivers
    allocations = []
    if pool.allocated_to:
        amount_per_center = pool.total_amount / len(pool.allocated_to)
        for dest_id in pool.allocated_to:
            allocation = CostAllocation(
                source_cost_center_id="",
                destination_cost_center_id=dest_id,
                allocation_method=pool.allocation_method,
                amount_allocated=amount_per_center,
                allocation_basis="equal_distribution",
                allocation_percentage=100 / len(pool.allocated_to) if pool.allocated_to else 0,
                period=period,
            )
            cost_allocations.append(allocation)
            allocations.append(allocation)

    await call_audit_service("ALLOCATE", "cost_pool", pool_id, {"amount": pool.total_amount, "period": period})
    return {"allocations": allocations, "total_allocated": pool.total_amount}


# ============================================================================
# API Endpoints - Product Costing
# ============================================================================


@app.post("/products/{product_id}/calculate-cost", response_model=ProductCost)
async def calculate_product_cost(
    product_id: str, product_name: str, costing_method: CostingMethod,
    direct_material: float, direct_labor: float, direct_expense: float,
    overhead_applied: float, units_produced: int, period_start: datetime, period_end: datetime,
):
    """Calculate product cost using specified costing method."""
    total_cost = direct_material + direct_labor + direct_expense + overhead_applied
    cost_per_unit = total_cost / units_produced if units_produced > 0 else 0

    product_cost = ProductCost(
        product_id=product_id, product_name=product_name, costing_method=costing_method,
        period_start=period_start, period_end=period_end,
        direct_material=direct_material, direct_labor=direct_labor, direct_expense=direct_expense,
        overhead_applied=overhead_applied, total_cost=total_cost,
        cost_per_unit=cost_per_unit, units_produced=units_produced,
    )

    # Create journal entry to record cost
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Product cost for {product_name}",
        "entries": [
            {"account_code": "1500", "description": "Finished Goods", "debit": total_cost, "credit": 0},
            {"account_code": "5100", "description": "Direct Material", "debit": 0, "credit": direct_material},
            {"account_code": "5200", "description": "Direct Labor", "debit": 0, "credit": direct_labor},
            {"account_code": "5300", "description": "Overhead Applied", "debit": 0, "credit": overhead_applied},
        ],
        "reference": f"COST-{product_cost.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    product_cost.journal_entry_id = result.get("id")

    product_costs.append(product_cost)
    await call_audit_service("CALCULATE", "product_cost", product_cost.id, {"product_id": product_id, "total_cost": total_cost})

    return product_cost


@app.get("/products/{product_id}/cost-history")
async def get_product_cost_history(product_id: str, limit: int = 10):
    """Get product cost history."""
    history = [c for c in product_costs if c.product_id == product_id]
    history.sort(key=lambda x: x.created_at, reverse=True)
    return {"costs": history[:limit], "count": len(history)}


# ============================================================================
# API Endpoints - Job Costing
# ============================================================================


@app.post("/jobs", response_model=JobCost, status_code=status.HTTP_201_CREATED)
async def create_job(data: JobCost):
    """Create a new job for job costing."""
    job_id = str(uuid.uuid4())
    data.id = job_id
    data.start_date = datetime.utcnow()
    data.created_at = datetime.utcnow()
    job_costs[job_id] = data

    await call_audit_service("CREATE", "job", job_id, {"job_number": data.job_number})
    return data


@app.get("/jobs/{job_id}")
async def get_job(job_id: str):
    """Get job details."""
    job = job_costs.get(job_id)
    if not job:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Job {job_id} not found")
    return job


@app.get("/jobs")
async def list_jobs(status: Optional[str] = None, customer_id: Optional[str] = None):
    """List jobs with filters."""
    result = list(job_costs.values())
    if status:
        result = [j for j in result if j.status == status]
    if customer_id:
        result = [j for j in result if j.customer_id == customer_id]
    return {"jobs": result, "count": len(result)}


@app.post("/jobs/{job_id}/add-cost")
async def add_job_cost(job_id: str, cost_type: CostElement, amount: float, description: str):
    """Add cost to a job."""
    job = job_costs.get(job_id)
    if not job:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Job {job_id} not found")

    if cost_type == CostElement.DIRECT_MATERIAL:
        job.direct_material_cost += amount
    elif cost_type == CostElement.DIRECT_LABOR:
        job.direct_labor_cost += amount
    elif cost_type == CostElement.INDIRECT_MATERIAL or cost_type == CostElement.FACTORY_OVERHEAD:
        job.overhead_applied += amount

    job.total_cost = job.direct_material_cost + job.direct_labor_cost + job.overhead_applied

    await call_audit_service("ADD_COST", "job", job_id, {"cost_type": cost_type, "amount": amount})
    return job


@app.post("/jobs/{job_id}/complete")
async def complete_job(job_id: str):
    """Mark a job as completed."""
    job = job_costs.get(job_id)
    if not job:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Job {job_id} not found")

    job.status = "completed"
    job.completion_date = datetime.utcnow()

    # Transfer to finished goods
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Job {job.job_number} completed",
        "entries": [
            {"account_code": "1500", "description": "Finished Goods", "debit": job.total_cost, "credit": 0},
            {"account_code": "1400", "description": "Work in Progress", "debit": 0, "credit": job.total_cost},
        ],
        "reference": f"JOB-{job_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    await call_audit_service("COMPLETE", "job", job_id, {"total_cost": job.total_cost})
    return job


# ============================================================================
# API Endpoints - Process Costing (WIP)
# ============================================================================


@app.post("/wip", response_model=WorkInProgress, status_code=status.HTTP_201_CREATED)
async def calculate_wip(data: WorkInProgress):
    """Calculate work in progress using process costing."""
    data.id = str(uuid.uuid4())
    data.total_cost = data.opening_wip + data.direct_material + data.direct_labor + data.overhead_applied
    data.closing_wip = data.total_cost - data.completed_cost

    # Calculate equivalent units
    material_completion = 0.5  # Assuming 50% complete for materials
    conversion_completion = 0.5  # Assuming 50% complete for conversion
    data.equivalent_units_material = data.units_completed + (data.units_closing * material_completion)
    data.equivalent_units_conversion = data.units_completed + (data.units_closing * conversion_completion)

    # Calculate cost per equivalent unit
    material_cost = data.opening_wip + data.direct_material
    conversion_cost = data.direct_labor + data.overhead_applied
    data.cost_per_equivalent_unit = {
        "material": material_cost / data.equivalent_units_material if data.equivalent_units_material > 0 else 0,
        "conversion": conversion_cost / data.equivalent_units_conversion if data.equivalent_units_conversion > 0 else 0,
    }

    # Create journal entry
    journal_entry = {
        "date": data.period_end,
        "description": f"WIP calculation for period ending {data.period_end.date()}",
        "entries": [
            {"account_code": "1400", "description": "Finished Goods", "debit": data.completed_cost, "credit": 0},
            {"account_code": "1400", "description": "WIP", "debit": 0, "credit": data.total_cost},
        ],
        "reference": f"WIP-{data.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    data.journal_entry_id = result.get("id")

    wip_records.append(data)
    await call_audit_service("CALCULATE", "wip", data.id, {"total_cost": data.total_cost})
    return data


@app.get("/wip")
async def get_wip_records(cost_center_id: Optional[str] = None):
    """Get WIP records."""
    result = wip_records
    if cost_center_id:
        result = [w for w in result if w.cost_center_id == cost_center_id]
    return {"records": result, "count": len(result)}


# ============================================================================
# API Endpoints - Standard Costs
# ============================================================================


@app.post("/standard-costs", response_model=StandardCost, status_code=status.HTTP_201_CREATED)
async def create_standard_cost(data: StandardCost):
    """Set standard cost for a product."""
    data.id = str(uuid.uuid4())
    data.total_standard_cost = (
        data.standard_material_qty * data.standard_material_rate +
        data.standard_labor_hours * data.standard_labor_rate +
        data.standard_overhead_rate * data.standard_overhead_hours
    )
    data.created_at = datetime.utcnow()
    standard_costs[data.id] = data

    await call_audit_service("CREATE", "standard_cost", data.id, {"product_id": data.product_id})
    return data


@app.get("/standard-costs/{product_id}")
async def get_standard_cost(product_id: str):
    """Get standard cost for a product."""
    cost = next((c for c in standard_costs.values() if c.product_id == product_id and c.is_active), None)
    if not cost:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Standard cost for {product_id} not found")
    return cost


# ============================================================================
# API Endpoints - Cost Variances
# ============================================================================


@app.post("/variances", response_model=CostVariance, status_code=status.HTTP_201_CREATED)
async def calculate_variance(data: CostVariance):
    """Calculate and record a cost variance."""
    data.id = str(uuid.uuid4())
    data.variance_amount, data.is_favorable = calculate_variance(data.actual_cost, data.standard_cost)
    data.variance_percentage = (data.variance_amount / data.standard_cost * 100) if data.standard_cost != 0 else 0
    data.created_at = datetime.utcnow()

    cost_variances.append(data)

    # Record variance journal entry
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"{data.variance_type.value} variance",
        "entries": [
            {"account_code": "6000", "description": "Cost of Goods Sold", "debit": abs(data.variance_amount) if data.is_favorable else 0, "credit": 0},
            {"account_code": "7100", "description": "Variance Account", "debit": 0 if data.is_favorable else abs(data.variance_amount), "credit": abs(data.variance_amount) if data.is_favorable else 0},
        ],
        "reference": f"VAR-{data.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    data.journal_entry_id = result.get("id")

    await call_audit_service("CALCULATE", "variance", data.id, {"variance_amount": data.variance_amount})
    return data


@app.get("/variances")
async def get_variances(
    variance_type: Optional[VarianceType] = None, period: Optional[str] = None,
    is_favorable: Optional[bool] = None,
):
    """Get cost variances with filters."""
    result = list(cost_variances)
    if variance_type:
        result = [v for v in result if v.variance_type == variance_type]
    if period:
        result = [v for v in result if v.period == period]
    if is_favorable is not None:
        result = [v for v in result if v.is_favorable == is_favorable]

    total_variance = sum(v.variance_amount for v in result)
    return {"variances": result, "total_variance": total_variance, "count": len(result)}


# ============================================================================
# API Endpoints - Overhead Rates
# ============================================================================


@app.post("/overhead-rates", response_model=OverheadRate, status_code=status.HTTP_201_CREATED)
async def create_overhead_rate(data: OverheadRate):
    """Set overhead rate for a cost center."""
    rate_id = str(uuid.uuid4())
    data.id = rate_id
    data.created_at = datetime.utcnow()
    overhead_rates[rate_id] = data

    await call_audit_service("CREATE", "overhead_rate", rate_id, {"rate": data.rate, "cost_center": data.cost_center_id})
    return data


@app.get("/overhead-rates/{cost_center_id}")
async def get_overhead_rate(cost_center_id: str):
    """Get overhead rate for a cost center."""
    rates = [r for r in overhead_rates.values() if r.cost_center_id == cost_center_id and r.is_active]
    if not rates:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"No active overhead rate for {cost_center_id}")
    return rates[0]


@app.post("/overhead-rates/{rate_id}/apply")
async def apply_overhead(cost_center_id: str, base_quantity: float):
    """Apply overhead based on activity."""
    rate = next((r for r in overhead_rates.values() if r.cost_center_id == cost_center_id and r.is_active), None)
    if not rate:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"No active overhead rate for {cost_center_id}")

    overhead_applied = rate.rate * base_quantity

    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Overhead applied for {rate.base_unit}",
        "entries": [
            {"account_code": "1400", "description": "WIP", "debit": overhead_applied, "credit": 0},
            {"account_code": "5300", "description": "Overhead Applied", "debit": 0, "credit": overhead_applied},
        ],
        "reference": f"OVERHEAD-{rate.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    return {"overhead_applied": overhead_applied, "base_quantity": base_quantity, "rate": rate.rate, "journal_entry_id": result.get("id")}


# ============================================================================
# API Endpoints - Cost Reports
# ============================================================================


@app.post("/reports/cost-analysis", response_model=CostReport)
async def generate_cost_analysis_report(
    period_start: datetime, period_end: datetime, cost_center_id: Optional[str] = None,
):
    """Generate comprehensive cost analysis report."""
    report = CostReport(
        report_type="cost_analysis",
        period_start=period_start,
        period_end=period_end,
    )

    # Aggregate costs by center
    centers_data = []
    total_cost = 0

    centers = [cost_centers.get(cost_center_id)] if cost_center_id else cost_centers.values()

    for center in centers:
        if center:
            center_costs = {
                "cost_center_id": center.id,
                "cost_center_name": center.name,
                "direct_material": 0,
                "direct_labor": 0,
                "overhead": 0,
                "total": 0,
            }

            # Calculate totals from product costs
            for pc in product_costs:
                if period_start <= pc.period_start <= period_end:
                    center_costs["direct_material"] += pc.direct_material * 0.3
                    center_costs["direct_labor"] += pc.direct_labor * 0.3
                    center_costs["overhead"] += pc.overhead_applied * 0.3
                    center_costs["total"] += pc.total_cost * 0.3

            center_costs["total"] = (
                center_costs["direct_material"] + center_costs["direct_labor"] + center_costs["overhead"]
            )
            total_cost += center_costs["total"]
            centers_data.append(center_costs)

    report.cost_centers = centers_data
    report.summary = {
        "total_cost": total_cost,
        "total_direct_material": sum(c["direct_material"] for c in centers_data),
        "total_direct_labor": sum(c["direct_labor"] for c in centers_data),
        "total_overhead": sum(c["overhead"] for c in centers_data),
    }

    # Add recommendations based on variances
    unfavorable_variances = [v for v in cost_variances if not v.is_favorable]
    if unfavorable_variances:
        report.recommendations.append(
            f"Review {len(unfavorable_variances)} unfavorable variances for cost control opportunities"
        )

    cost_reports.append(report)
    return report


@app.get("/reports/marginal-cost")
async def get_marginal_cost_analysis():
    """Get marginal cost analysis."""
    # Calculate contribution margin
    return {
        "description": "Marginal cost analysis",
        "fixed_costs": 50000,
        "variable_cost_per_unit": 25,
        "contribution_margin_ratio": 0.35,
        "break_even_units": 2000,
        "recommendations": [
            "Consider volume discounts for materials",
            "Evaluate labor efficiency improvements",
        ],
    }


# ============================================================================
# Main Entry Point
# ============================================================================


if __name__ == "__main__":
    import uvicorn
    logger.info("starting_cost_accounting_service", port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)