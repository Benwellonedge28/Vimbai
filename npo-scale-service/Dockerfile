FROM python:3.10-slim-buster

WORKDIR /app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

EXPOSE 9021

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "9021"]

RUN groupadd -r appuser && useradd -r -g appuser appuser
USER appuser
