import httpx
from typing import Dict, Any
from datetime import datetime
from neo4j import AsyncSession
from automation_engine_service import crud, models
import os
import json

async def execute_task_instance(db_session: AsyncSession, instance_id: str):
    """
    Executes a single automation task instance.
    Fetches the task definition, makes the HTTP call, and updates the instance status.
    """
    instance = await crud.get_automation_task_instance(db_session, instance_id)
    if not instance:
        print(f"Executor: Automation Task Instance {instance_id} not found.")
        return

    definition = await crud.get_automation_task_definition(db_session, instance.task_definition_id)
    if not definition:
        print(f"Executor: Automation Task Definition {instance.task_definition_id} not found for instance {instance_id}.")
        await crud.update_automation_task_instance(db_session, instance_id, models.AutomationTaskInstanceUpdate(
            status="failed",
            end_time=datetime.utcnow(),
            error_message="Task Definition not found."
        ))
        return

    # Mark instance as running
    await crud.update_automation_task_instance(db_session, instance_id, models.AutomationTaskInstanceUpdate(
        status="running",
        start_time=datetime.utcnow() # Ensure start time is recorded at actual execution start
    ))
    await crud.create_automation_log(db_session, models.AutomationLogCreate(
        instance_id=instance_id,
        log_level="INFO",
        message=f"Starting execution of task '{definition.name}' to {definition.service_target}{definition.endpoint_path}"
    ))

    try:
        # Resolve target service URL
        service_url_env_var = f"{definition.service_target.upper().replace('-', '_')}_SERVICE_URL"
        service_base_url = os.getenv(service_url_env_var)
        if not service_base_url:
            raise ValueError(f"Environment variable {service_url_env_var} not set for target service {definition.service_target}.")

        full_url = f"{service_base_url}{definition.endpoint_path}"
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer internal_service_token_for_{definition.service_target}", # Use internal service token
        }

        async with httpx.AsyncClient() as client:
            response: httpx.Response = None
            if definition.http_method == "GET":
                response = await client.get(full_url, headers=headers, params=definition.payload_template)
            elif definition.http_method == "POST":
                response = await client.post(full_url, headers=headers, json=definition.payload_template)
            elif definition.http_method == "PUT":
                response = await client.put(full_url, headers=headers, json=definition.payload_template)
            elif definition.http_method == "DELETE":
                response = await client.delete(full_url, headers=headers, json=definition.payload_template)
            else:
                raise ValueError(f"Unsupported HTTP method: {definition.http_method}")

            response.raise_for_status() # Raise an exception for bad status codes (4xx or 5xx)
            response_output = response.json()
            
            # Log successful execution
            await crud.create_automation_log(db_session, models.AutomationLogCreate(
                instance_id=instance_id,
                log_level="INFO",
                message=f"Task '{definition.name}' completed successfully. Status: {response.status_code}",
                details={"response": response_output}
            ))
            await crud.update_automation_task_instance(db_session, instance_id, models.AutomationTaskInstanceUpdate(
                status="completed",
                end_time=datetime.utcnow(),
                output=response_output
            ))

    except httpx.HTTPStatusError as http_error:
        error_message = f"HTTP Error during task execution: {http_error.response.status_code} - {http_error.response.text}"
        print(f"Executor Error: {error_message}")
        await crud.create_automation_log(db_session, models.AutomationLogCreate(
            instance_id=instance_id,
            log_level="ERROR",
            message=error_message,
            details={"status_code": http_error.response.status_code, "response_text": http_error.response.text}
        ))
        await crud.update_automation_task_instance(db_session, instance_id, models.AutomationTaskInstanceUpdate(
            status="failed",
            end_time=datetime.utcnow(),
            error_message=error_message
        ))
    except Exception as e:
        error_message = f"Unhandled exception during task execution: {e}"
        print(f"Executor Error: {error_message}")
        await crud.create_automation_log(db_session, models.AutomationLogCreate(
            instance_id=instance_id,
            log_level="ERROR",
            message=error_message,
            details={"exception": str(e)}
        ))
        await crud.update_automation_task_instance(db_session, instance_id, models.AutomationTaskInstanceUpdate(
            status="failed",
            end_time=datetime.utcnow(),
            error_message=error_message
        ))
