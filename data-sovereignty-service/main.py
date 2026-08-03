from fastapi import FastAPI
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Data Sovereignty Service", version="1.0.0")

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "data-sovereignty-service", "version": "1.0.0"}

@app.get("/info")
async def get_info():
    return {"description": "Manages regional data localization"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
