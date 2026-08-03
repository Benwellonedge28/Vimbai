from fastapi import FastAPI
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Realtime Calculation Engine", version="1.0.0")

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "realtime-calculation-engine", "version": "1.0.0"}

@app.get("/info")
async def get_info():
    return {"description": "Triggers recalculation on transaction events"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
