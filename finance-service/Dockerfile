FROM python:3.10-slim-buster

WORKDIR /app

RUN groupadd -r appuser && useradd -r -g appuser appuser

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

USER appuser

EXPOSE 8001

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8001"]
