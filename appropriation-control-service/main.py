from fastapi import FastAPI
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Appropriation Control Service", version="1.0.0")

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "appropriation-control-service", "version": "1.0.0"}

@app.get("/info")
async def get_info():
    return {"description": "Controls government expenditure against appropriations"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
