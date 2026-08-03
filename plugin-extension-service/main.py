from fastapi import FastAPI
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Plugin Extension Service", version="1.0.0")

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "plugin-extension-service", "version": "1.0.0"}

@app.get("/info")
async def get_info():
    return {"description": "Plugin registry and lifecycle management"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
