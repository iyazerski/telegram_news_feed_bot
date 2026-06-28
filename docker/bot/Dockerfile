FROM python:3.14-slim AS builder

COPY --from=ghcr.io/astral-sh/uv:0.8.22 /uv /uvx /bin/

ENV UV_COMPILE_BYTECODE=1
ENV UV_LINK_MODE=copy

WORKDIR /app

COPY LICENSE README.md pyproject.toml uv.lock ./
COPY src src

RUN uv sync --locked --no-dev


FROM python:3.14-slim AS runtime

RUN adduser --disabled-password --gecos "" --uid 10001 appuser

WORKDIR /app

COPY --from=builder --chown=appuser:appuser /app /app

USER appuser

ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONUNBUFFERED=1

EXPOSE 8000

CMD ["python", "-m", "src.entrypoints.bot.main"]
