"""
Vimbai Exotic Derivatives Service
Pricing and risk analysis for exotic options (barrier, Asian, lookback, binary).
Port: 8348
"""
import os, uuid, math
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "exotic-derivatives-service"
PORT = int(os.getenv("PORT", "8348"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Exotic Derivatives Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class DerivativeRequest(BaseModel):
    option_type: str  # barrier, asian, lookback, binary, chooser
    underlying: str; spot_price: float; strike_price: float
    volatility: float; risk_free_rate: float; time_to_expiry: float  # years
    barrier_price: Optional[float] = None; barrier_type: Optional[str] = None  # knock_in, knock_out
    is_call: bool = True; observations: int = 252

class DerivativeResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    option_type: str; underlying: str
    estimated_price: float; delta: float; gamma: float; vega: float; theta: float; rho: float
    model_used: str; notes: str = ""

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

def black_scholes(S, K, T, r, sigma, is_call=True):
    if T <= 0 or sigma <= 0: return max(S - K, 0) if is_call else max(K - S, 0)
    d1 = (math.log(S / K) + (r + 0.5 * sigma**2) * T) / (sigma * math.sqrt(T))
    d2 = d1 - sigma * math.sqrt(T)
    if is_call:
        price = S * 0.5 * (1 + math.erf(d1 / math.sqrt(2))) - K * math.exp(-r * T) * 0.5 * (1 + math.erf(d2 / math.sqrt(2)))
        delta = 0.5 * (1 + math.erf(d1 / math.sqrt(2)))
    else:
        price = K * math.exp(-r * T) * 0.5 * (1 - math.erf(d2 / math.sqrt(2))) - S * 0.5 * (1 - math.erf(d1 / math.sqrt(2)))
        delta = -0.5 * (1 - math.erf(d1 / math.sqrt(2)))
    return price, d1, d2, delta

@app.post("/price", response_model=DerivativeResult)
async def price_derivative(req: DerivativeRequest):
    S, K, T, r, sigma = req.spot_price, req.strike_price, req.time_to_expiry, req.risk_free_rate, req.volatility
    bs_price, d1, d2, delta = black_scholes(S, K, T, r, sigma, req.is_call)
    
    n_prime_d1 = math.exp(-0.5 * d1**2) / math.sqrt(2 * math.pi)
    gamma = n_prime_d1 / (S * sigma * math.sqrt(T))
    vega = S * math.sqrt(T) * n_prime_d1
    theta = (-S * n_prime_d1 * sigma / (2 * math.sqrt(T)) - r * K * math.exp(-r * T) * (0.5*(1+math.erf(d2/math.sqrt(2))) if req.is_call else 0.5*(1-math.erf(d2/math.sqrt(2))))) / 365
    rho = K * T * math.exp(-r * T) * (0.5*(1+math.erf(d2/math.sqrt(2))) if req.is_call else -0.5*(1-math.erf(d2/math.sqrt(2)))) / 100
    
    if req.option_type == "asian":
        adjusted_sigma = sigma * math.sqrt((2 * req.observations + 1) / (6 * (req.observations + 1)))
        adjusted_T = T * (req.observations + 1) / (2 * (req.observations + 1))
        price, d1, d2, delta = black_scholes(S, K, adjusted_T, r, adjusted_sigma, req.is_call)
        model = "Asian (arithmetic average) - adjusted Black-Scholes"
    elif req.option_type == "lookback":
        price = bs_price * 1.5  # Approximate lookback premium
        model = "Lookback - approximate premium over vanilla"
    elif req.option_type == "binary":
        price = K * math.exp(-r * T) * (0.5*(1+math.erf(d2/math.sqrt(2))) if req.is_call else 0.5*(1-math.erf(d2/math.sqrt(2))))
        model = "Binary/Digital cash-or-nothing"
    elif req.option_type == "barrier":
        if req.barrier_price:
            barrier_prob = 0.5 * (1 + math.erf((math.log(req.barrier_price/S) + (r - 0.5*sigma**2)*T) / (sigma*math.sqrt(T)*math.sqrt(2))))
            if req.barrier_type == "knock_out":
                price = bs_price * (1 - barrier_prob)
                model = f"Barrier knock-out at {req.barrier_price}"
            else:
                price = bs_price * barrier_prob
                model = f"Barrier knock-in at {req.barrier_price}"
        else:
            price = bs_price
            model = "Barrier (no barrier set - using vanilla)"
    else:
        price = bs_price
        model = "Vanilla Black-Scholes"
    
    return DerivativeResult(
        option_type=req.option_type, underlying=req.underlying,
        estimated_price=round(price, 4), delta=round(delta, 4),
        gamma=round(gamma, 6), vega=round(vega, 4),
        theta=round(theta, 4), rho=round(rho, 4),
        model_used=model,
        notes=f"Parameters: S={S}, K={K}, T={T}y, sigma={sigma}, r={r}"
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
