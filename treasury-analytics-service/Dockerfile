# bookworm carries a glibc new enough for the manylinux_2_28 wheel
FROM python:3.11-slim-bookworm

WORKDIR /app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

# The calculation wheel, built from library/native. The build context is this
# service directory, so the image cannot compile it: pass the published wheel
# (release asset or registry URL) and the service answers from the Rust
# library. Left empty, the image runs the Python reference engine, which
# tests/test_native_seam.py holds to the same answers.
ARG VIMBAI_NATIVE_WHEEL_URL=""
RUN if [ -n "$VIMBAI_NATIVE_WHEEL_URL" ]; then \
        pip install --no-cache-dir "$VIMBAI_NATIVE_WHEEL_URL"; \
    fi

COPY . .

EXPOSE 8134

RUN groupadd -r appuser && useradd -r -g appuser appuser
USER appuser

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8134"]
