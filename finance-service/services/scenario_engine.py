import json
import random  # Import random for mock data generation
import uuid
from datetime import datetime, timedelta
from typing import Any, Dict, List, Optional

from finance_service import crud, models
from finance_service.exceptions import NotFoundError
from neo4j import AsyncSession


class ScenarioEngine:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session

    async def generate_baseline_forecast(
        self, user_id: str, start_date: datetime, end_date: datetime, interval: str
    ) -> models.ForecastInDB:
        """Generates a baseline forecast based on historical data (mock for now)."""
        print(f"Generating baseline forecast for user {user_id} from {start_date} to {end_date}")
        # In a real implementation, this would pull historical data from Accounting Service
        # and apply forecasting models (e.g., ARIMA, Prophet, ML models).

        # Mock data for demonstration
        forecast_id = str(uuid.uuid4())
        forecast_values: List[models.ForecastValue] = []
        current_date = start_date
        initial_revenue = 10000.0
        initial_expenses = 6000.0

        while current_date <= end_date:
            revenue = initial_revenue * (1 + random.uniform(0.005, 0.015))  # Small growth
            expenses = initial_expenses * (1 + random.uniform(-0.002, 0.008))  # Fluctuating expenses
            profit = revenue - expenses

            forecast_values.append(
                models.ForecastValue(
                    date=current_date.date(),
                    revenue=revenue,
                    expenses=expenses,
                    profit=profit,
                    cash_flow=profit * 0.8,  # Simple assumption
                )
            )

            # Move to next interval
            if interval == "monthly":
                current_date = current_date.replace(day=1) + timedelta(days=32)
                current_date = current_date.replace(day=1)  # Correct month overflow
            elif interval == "quarterly":
                current_date = current_date.replace(day=1) + timedelta(days=92)
                current_date = current_date.replace(day=1)  # Correct month overflow
            elif interval == "yearly":
                current_date = current_date.replace(day=1) + timedelta(days=366)
                current_date = current_date.replace(day=1)
            else:
                raise ValueError("Unsupported interval")

            initial_revenue = revenue
            initial_expenses = expenses

        forecast_data = models.ForecastCreate(
            user_id=user_id,
            name="Baseline Forecast",
            description="Automatically generated baseline forecast based on historical trends.",
            start_date=start_date.date(),
            end_date=end_date.date(),
            interval=interval,
            values=forecast_values,
            is_baseline=True,
        )
        return await crud.create_forecast(self.db_session, forecast_data)

    async def apply_scenario(
        self, baseline_forecast_id: str, scenario_params: models.ScenarioParametersCreate
    ) -> models.ForecastInDB:
        """Applies scenario parameters to a baseline forecast to create a new simulated forecast."""
        baseline_forecast = await crud.get_forecast(self.db_session, baseline_forecast_id)
        if not baseline_forecast:
            raise NotFoundError(f"Baseline Forecast {baseline_forecast_id} not found.")
        if not baseline_forecast.is_baseline:
            raise ValueError("Scenario can only be applied to a baseline forecast.")

        simulated_values: List[models.ForecastValue] = []
        for value in baseline_forecast.values:
            new_revenue = value.revenue
            new_expenses = value.expenses
            new_profit = value.profit
            new_cash_flow = value.cash_flow

            # Apply revenue growth/reduction
            if scenario_params.revenue_growth_rate is not None:
                new_revenue *= 1 + scenario_params.revenue_growth_rate

            # Apply expense changes
            if scenario_params.expense_reduction_rate is not None:
                new_expenses *= 1 - scenario_params.expense_reduction_rate
            if scenario_params.fixed_expense_increase is not None:
                new_expenses += scenario_params.fixed_expense_increase

            # Apply interest rate changes (affects cash flow/profit in a more complex model)
            # For simplicity here, just re-calculate profit and cash flow
            new_profit = new_revenue - new_expenses
            new_cash_flow = new_profit * 0.8  # Re-apply simple cash flow assumption

            simulated_values.append(
                models.ForecastValue(
                    date=value.date,
                    revenue=new_revenue,
                    expenses=new_expenses,
                    profit=new_profit,
                    cash_flow=new_cash_flow,
                )
            )

        # Create the new scenario forecast
        scenario_forecast_data = models.ForecastCreate(
            user_id=baseline_forecast.user_id,
            name=scenario_params.name or f"Scenario based on {baseline_forecast.name}",
            description=scenario_params.description
            or f"Simulated forecast applying: {json.dumps(scenario_params.model_dump(exclude_unset=True))}",
            start_date=baseline_forecast.start_date,
            end_date=baseline_forecast.end_date,
            interval=baseline_forecast.interval,
            values=simulated_values,
            is_baseline=False,
            parent_forecast_id=baseline_forecast.id,
        )
        return await crud.create_forecast(self.db_session, scenario_forecast_data)

    async def compare_scenarios(self, forecast_ids: List[str]) -> Dict[str, Any]:
        """Compares multiple forecasts/scenarios (mock for now)."""
        comparison_results = {}
        for fid in forecast_ids:
            forecast = await crud.get_forecast(self.db_session, fid)
            if forecast:
                total_revenue = sum(v.revenue for v in forecast.values)
                total_expenses = sum(v.expenses for v in forecast.values)
                total_profit = sum(v.profit for v in forecast.values)
                comparison_results[forecast.name] = {
                    "total_revenue": total_revenue,
                    "total_expenses": total_expenses,
                    "total_profit": total_profit,
                    "start_date": forecast.start_date.isoformat(),
                    "end_date": forecast.end_date.isoformat(),
                    "description": forecast.description,
                }
        return comparison_results
