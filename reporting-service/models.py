from pydantic import BaseModel, Field
from typing import Optional, List, Dict, Any, Literal
from datetime import datetime

# --- Report Template Models ---
class WidgetConfig(BaseModel):
    widget_id: str
    widget_type: Literal["chart", "table", "metric", "gauge", "map"]
    title: str
    query: str = Field(..., description="Cypher query for the widget")
    parameters: Optional[Dict[str, Any]] = {}
    visualization_config: Optional[Dict[str, Any]] = {}
    position: Dict[str, int] = Field(default_factory=lambda: {"x": 0, "y": 0, "w": 4, "h": 3})

class DashboardBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100)
    description: Optional[str] = None
    widgets: List[WidgetConfig] = Field(default_factory=list)
    is_default: bool = Field(False)

class DashboardCreate(DashboardBase):
    pass

class DashboardUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100)
    description: Optional[str] = None
    widgets: Optional[List[WidgetConfig]] = None
    is_default: Optional[bool] = None

class DashboardInDB(DashboardBase):
    id: str
    user_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Report Template Models ---
class ReportTemplateBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100)
    description: Optional[str] = None
    query_definition: str = Field(..., description="Cypher query for the report")
    parameters_schema: Optional[Dict[str, Any]] = {}
    output_format: Literal["json", "csv", "pdf"] = Field("json")
    is_shared: bool = Field(False)

class ReportTemplateCreate(ReportTemplateBase):
    pass

class ReportTemplateUpdate(BaseModel):
    name: Optional[str] = Field(None, min_length=3, max_length=100)
    description: Optional[str] = None
    query_definition: Optional[str] = None
    parameters_schema: Optional[Dict[str, Any]] = None
    output_format: Optional[Literal["json", "csv", "pdf"]] = None
    is_shared: Optional[bool] = None

class ReportTemplateInDB(ReportTemplateBase):
    id: str
    created_by: str
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Generated Report Models ---
class ReportGenerationRequest(BaseModel):
    template_id: Optional[str] = None
    custom_query: Optional[str] = None
    parameters: Dict[str, Any] = Field(default_factory=dict)
    output_format: Literal["json", "csv", "pdf"] = "json"

class ReportDataPoint(BaseModel):
    label: str
    value: float
    category: Optional[str] = None

class ChartData(BaseModel):
    labels: List[str]
    datasets: List[Dict[str, Any]]

class ReportResult(BaseModel):
    report_id: str
    generated_at: datetime
    query_executed: str
    row_count: int
    data: List[Dict[str, Any]]
    summary: Optional[Dict[str, Any]] = None
    chart_data: Optional[ChartData] = None

    class Config:
        from_attributes = True

# --- Scheduled Report Models ---
class ScheduledReportBase(BaseModel):
    name: str = Field(..., min_length=3, max_length=100)
    template_id: str
    parameters: Dict[str, Any] = Field(default_factory=dict)
    schedule: str = Field(..., description="Cron expression for scheduling")
    recipients: List[str] = Field(..., description="Email addresses for distribution")
    is_active: bool = Field(True)

class ScheduledReportCreate(ScheduledReportBase):
    pass

class ScheduledReportInDB(ScheduledReportBase):
    id: str
    created_by: str
    last_run_at: Optional[datetime] = None
    next_run_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)

    class Config:
        from_attributes = True

# --- Filter Models ---
class ReportFilter(BaseModel):
    field: str
    operator: Literal["eq", "neq", "gt", "gte", "lt", "lte", "in", "contains"]
    value: Any

class ReportSort(BaseModel):
    field: str
    order: Literal["asc", "desc"] = "asc"

# --- Export Models ---
class ExportRequest(BaseModel):
    report_id: str
    format: Literal["csv", "json", "pdf"] = "csv"
    include_filters: bool = True

class ExportResult(BaseModel):
    file_path: str
    format: str
    size_bytes: int
    download_url: str
