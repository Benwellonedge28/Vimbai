# Use the official Python image
FROM python:3.10-slim-buster

# Set the working directory in the container
WORKDIR /app

# Copy the requirements file and install dependencies
COPY fraud-detection-service/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy the rest of the application code
COPY fraud-detection-service/ ./fraud_detection_service/

# Expose the port the app runs on
EXPOSE 8005

# Run the Uvicorn server
CMD ["uvicorn", "fraud_detection_service.main:app", "--host", "0.0.0.0", "--port", "8005"]

RUN groupadd -r appuser && useradd -r -g appuser appuser
USER appuser
