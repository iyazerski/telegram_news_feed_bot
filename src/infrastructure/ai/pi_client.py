import asyncio
import json
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any

from src.config.configs import AppConfigs

# Each RPC record is one JSON line, and model catalogs exceed asyncio's default 64 KiB line limit.
STREAM_LINE_LIMIT_BYTES = 32 * 1024 * 1024
RECAP_TOOLS = "web_search,fetch_content"
# Recaps must not pick up the host's skills, context files, MCP servers, or other extensions.
ISOLATION_ARGS = (
    "--no-session",
    "--no-skills",
    "--no-context-files",
    "--no-prompt-templates",
    "--no-themes",
    "--no-extensions",
    "--no-mcp",
    "--no-approve",
)


class PiError(Exception):
    pass


class PiClient:
    def __init__(self, configs: AppConfigs) -> None:
        """
        Create a client that runs isolated Pi-Bolt processes for the configured provider.
        """
        self.configs = configs

    async def run_prompt(self, prompt: str, model: str, thinking: str) -> str:
        """
        Run one web-enabled Pi prompt and return its final reply text.
        """
        with TemporaryDirectory(prefix="news-recap-") as workspace:
            process = await asyncio.create_subprocess_exec(
                self.configs.recap_pi_executable,
                "--print",
                "--model",
                f"{self.configs.recap_provider}/{model}",
                "--thinking",
                thinking,
                *ISOLATION_ARGS,
                "--extension",
                self.configs.recap_pi_web_access_path,
                "--tools",
                RECAP_TOOLS,
                "--",
                prompt,
                cwd=workspace,
                env=self.build_env(),
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            try:
                stdout, stderr = await asyncio.wait_for(process.communicate(), self.configs.recap_timeout_seconds)
            except BaseException:
                # A timed-out or cancelled recap must not leave Pi running in the background.
                await self.kill(process)
                raise

        if process.returncode != 0:
            raise PiError(stderr.decode().strip() or stdout.decode().strip() or "Pi prompt failed")
        return stdout.decode().strip()

    async def list_models(self) -> list[str]:
        """
        Return the model IDs Pi can use with the signed-in provider account.
        """
        with TemporaryDirectory(prefix="news-recap-") as workspace:
            process = await asyncio.create_subprocess_exec(
                self.configs.recap_pi_executable,
                "--mode",
                "rpc",
                "--no-tools",
                *ISOLATION_ARGS,
                cwd=workspace,
                env=self.build_env(),
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.DEVNULL,
                limit=STREAM_LINE_LIMIT_BYTES,
            )
            try:
                response = await asyncio.wait_for(
                    self.request_models(process), self.configs.recap_model_list_timeout_seconds
                )
            finally:
                await self.kill(process)

        if not response.get("success"):
            raise PiError(str(response.get("error") or "Pi model lookup failed"))
        return [
            model["id"] for model in response["data"]["models"] if model.get("provider") == self.configs.recap_provider
        ]

    async def is_authenticated(self) -> bool:
        """
        Return whether Pi holds a usable credential for the configured provider.
        """
        process = await asyncio.create_subprocess_exec(
            self.configs.recap_pi_executable,
            "auth",
            "check",
            "--provider",
            self.configs.recap_provider,
            "--json",
            env=self.build_env(),
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        return await process.wait() == 0

    async def request_models(self, process: asyncio.subprocess.Process) -> dict[str, Any]:
        """
        Ask a Pi RPC process for its model catalog and return the matching response record.
        """
        if process.stdin is None or process.stdout is None:
            raise PiError("Pi RPC pipes are unavailable")

        # Pi exits at end of input before it answers, so stdin stays open until the reply arrives.
        process.stdin.write(json.dumps({"id": "models", "type": "get_available_models"}).encode() + b"\n")
        await process.stdin.drain()
        while line := await process.stdout.readline():
            record = json.loads(line)
            if record.get("type") == "response" and record.get("id") == "models":
                return record
        raise PiError("Pi exited before listing models")

    def build_env(self) -> dict[str, str]:
        """
        Return the process environment with Pi pointed at the configured home directory.
        """
        env = os.environ.copy()
        if self.configs.recap_pi_home:
            env["PI_CODING_AGENT_DIR"] = str(Path(self.configs.recap_pi_home))
        return env

    async def kill(self, process: asyncio.subprocess.Process) -> None:
        """
        Stop a Pi process that is still running and reap it.
        """
        if process.returncode is None:
            process.kill()
            await process.wait()
