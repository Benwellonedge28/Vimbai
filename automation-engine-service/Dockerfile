# Vimbai Automation Engine Service Dockerfile
# Manages automated tasks and scheduling across Vimbai microservices

FROM python:3.11-slim

# Set working directory
WORKDIR /app

# Set environment variables
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

# Copy requirements first for better caching
COPY requirements.txt /app/requirements.txt

# Install Python dependencies
RUN pip install --no-cache-dir -r requirements.txt

# Copy the application code
COPY . /app/

# Create log directory
RUN mkdir -p /app/logs

# Expose port
EXPOSE 8006

# Health check (python:slim has no curl)
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD python -c "import urllib.request,sys; sys.exit(0 if urllib.request.urlopen('http://localhost:8006/').status==200 else 1)" || exit 1

# Run the application
RUN groupadd -r appuser && useradd -r -g appuser appuser
USER appuser

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8006"]
