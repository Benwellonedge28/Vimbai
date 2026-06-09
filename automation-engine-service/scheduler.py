import asyncio
from datetime import datetime, timedelta, timezone
from typing import List, Dict, Any
from neo4j import AsyncSession
from automation_engine_service import crud, models
from automation_engine_service.dependencies import get_db_session
from automation_engine_service.executor import execute_task_instance # Will be created next
import croniter
import os

# How often the scheduler wakes up to check for due tasks
SCHEDULER_INTERVAL_SECONDS = int(os.getenv("SCHEDULER_INTERVAL_SECONDS", "60"))

async def run_scheduler(db_session: AsyncSession):\n    """Background task to check for and trigger scheduled automation tasks."""\n    print(f"Automation Engine Scheduler started with interval: {SCHEDULER_INTERVAL_SECONDS}s")\n    while True:\n        now = datetime.now(timezone.utc)\n        print(f"Scheduler running at {now.isoformat()}...\n        active_definitions = await crud.get_all_automation_task_definitions(db_session)\n
        for definition in active_definitions:\n            if not definition.is_active:\n                continue\n            
            trigger_now = False\n            if definition.schedule_type == "cron" and definition.cron_schedule:\n                try:\n                    # Calculate next run based on last_executed_at or current time\n                    base_time = definition.last_executed_at or now - timedelta(minutes=1) # Look back a bit if never run\n                    itr = croniter.croniter(definition.cron_schedule, base_time)\n                    next_run = itr.get_next(datetime)\n                    \n                    if next_run <= now and (definition.last_executed_at is None or next_run > definition.last_executed_at):\n                        trigger_now = True\n                        definition.next_execution_at = next_run # Update next_execution_at based on cron\n                except Exception as e:\n                    print(f"ERROR: Invalid cron schedule for definition {definition.id}: {e}")\n                    # Consider marking definition as inactive or logging error status\n
            elif definition.schedule_type == "interval" and definition.interval_seconds is not None:\n                if definition.last_executed_at is None or (now - definition.last_executed_at).total_seconds() >= definition.interval_seconds:\n                    trigger_now = True\n                    definition.next_execution_at = now + timedelta(seconds=definition.interval_seconds) # Update next_execution_at based on interval\n            
            if trigger_now:\n                print(f"Triggering task: {definition.name} ({definition.id})")\n                # Create an instance and execute\n                instance_create = models.AutomationTaskInstanceCreate(\n                    task_definition_id=definition.id!,\n                    triggered_by="schedule",\n                    start_time=now\n                )\n                instance = await crud.create_automation_task_instance(db_session, instance_create)\n                
                # Execute the instance in a separate coroutine\n                asyncio.create_task(execute_task_instance(db_session, instance.id!))\n
                # Update last_executed_at and next_execution_at in the definition\n                await crud.update_automation_task_definition(\n                    db_session, \n                    definition.id!, \n                    models.AutomationTaskDefinitionUpdate(\n                        last_executed_at=now,\n                        next_execution_at=definition.next_execution_at # Use the calculated next_execution_at\n                    )\n                )\n        
        await asyncio.sleep(SCHEDULER_INTERVAL_SECONDS)\n
