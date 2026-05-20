FROM python:3.10-slim-buster

WORKDIR /app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

# If using Tesseract locally, you'd need to install it:
# RUN apt-get update && apt-get install -y tesseract-ocr libtesseract-dev
# COPY . .

EXPOSE 8002 # Multimodal service will run on port 8002

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8002"]
