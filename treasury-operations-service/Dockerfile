FROM python:3.10-slim-buster

WORKDIR /app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

EXPOSE 8257

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8257"]

RUN groupadd -r appuser && useradd -r -g appuser appuser
USER appuser
