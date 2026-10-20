FROM python:3.14-slim AS builder

COPY --from=ghcr.io/astral-sh/uv:0.8.22 /uv /uvx /bin/

ENV UV_COMPILE_BYTECODE=1
ENV UV_LINK_MODE=copy

WORKDIR /app

COPY LICENSE README.md pyproject.toml uv.lock ./
COPY src src

RUN uv sync --locked --no-dev


FROM debian:trixie-slim AS pi-bolt

# Pi-Bolt is Pi compiled to native code, so the image needs neither Node.js nor npm.
ARG PIBOLT_VERSION=0.9.6
# Empty lets the installer pick the build for the build machine's CPU: x64 needs AVX2, so set x64-baseline for older
# x86-64 servers.
ARG PIBOLT_VARIANT=
ARG PI_WEB_ACCESS_VERSION=0.43.0

# OpenSSL lets the installer verify the release signature, and xz picks the smaller archive.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl openssl xz-utils \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL "https://raw.githubusercontent.com/opensec-git/Pi-Bolt/bolt-v${PIBOLT_VERSION}/install.sh" -o /tmp/pi-bolt-install.sh \
    && PIBOLT_VERSION="bolt-v${PIBOLT_VERSION}" PIBOLT_VARIANT="${PIBOLT_VARIANT}" PIBOLT_YES=1 PIBOLT_EXTENSIONS=no \
        PIBOLT_INSTALL=/opt/pi-bolt PIBOLT_BIN_DIR=/opt/pi-bolt/bin sh /tmp/pi-bolt-install.sh \
    && /opt/pi-bolt/bin/pi-bolt --version

# Install web search outside Pi's home, which is a volume at runtime. Pi-Bolt installs npm packages with its built-in
# Bun package manager, and the recapper loads the package by path.
RUN PI_CODING_AGENT_DIR=/opt/pi-packages /opt/pi-bolt/bin/pi-bolt install "npm:pi-web-access@${PI_WEB_ACCESS_VERSION}" \
    && test -f /opt/pi-packages/npm/node_modules/pi-web-access/package.json


FROM python:3.14-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libatomic1 \
    && rm -rf /var/lib/apt/lists/* \
    && adduser --disabled-password --gecos "" --uid 10001 appuser \
    && mkdir -p /var/lib/telegram-news/pi \
    && chown -R appuser:appuser /var/lib/telegram-news

WORKDIR /app

COPY --from=pi-bolt /opt/pi-bolt /opt/pi-bolt
COPY --from=pi-bolt /opt/pi-packages /opt/pi-packages
RUN ln -sf /opt/pi-bolt/bin/pi-bolt /usr/local/bin/pi \
    && pi --version
COPY --from=builder --chown=appuser:appuser /app /app

USER appuser

ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONUNBUFFERED=1
# Cap glibc malloc arenas, which otherwise grow per thread (asyncio.to_thread workers) and inflate idle memory.
ENV MALLOC_ARENA_MAX=2
ENV HOME=/var/lib/telegram-news
ENV TELEGRAM_NEWS_RECAP_PI_HOME=/var/lib/telegram-news/pi

CMD ["python", "-m", "src.entrypoints.recapper.main"]
