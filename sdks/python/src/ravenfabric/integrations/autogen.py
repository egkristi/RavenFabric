"""Microsoft AutoGen integration for RavenFabric.

Provides a RavenFabricExecutor compatible with AutoGen multi-agent orchestration.
"""

from __future__ import annotations

from typing import Any

from ravenfabric.client import McpClient
from ravenfabric.transport import StdioTransport


class AutoGenExecutor:
    """An AutoGen-compatible executor that runs commands via RavenFabric.

    Compatible with AutoGen's code execution interface. Commands are
    subject to RavenFabric's deny-by-default security policy.

    Usage with AutoGen:
        from ravenfabric.integrations.autogen import AutoGenExecutor

        executor = AutoGenExecutor(transport)
        result = executor.execute_code_blocks([("bash", "ls -la")])
    """

    def __init__(
        self,
        transport: StdioTransport,
        timeout: float = 30.0,
    ) -> None:
        self._transport = transport
        self._client = McpClient(transport, timeout=timeout)
        self._initialized = False

    def _ensure_initialized(self) -> None:
        if not self._initialized:
            self._client.initialize_sync()
            self._initialized = True

    def execute_code_blocks(
        self, code_blocks: list[tuple[str, str]]
    ) -> tuple[int, str, str | None]:
        """Execute code blocks in AutoGen format.

        Args:
            code_blocks: List of (language, code) tuples.

        Returns:
            Tuple of (exit_code, output, image_path).
            image_path is always None (no image support).
        """
        self._ensure_initialized()

        outputs: list[str] = []
        last_exit_code = 0

        for language, code in code_blocks:
            if language not in ("bash", "sh", "shell", "zsh"):
                outputs.append(f"[Unsupported language: {language}]")
                last_exit_code = 1
                continue

            result = self._client.exec_sync(code)
            outputs.append(result.output)
            last_exit_code = result.exit_code

            if result.exit_code != 0:
                break

        return (last_exit_code, "\n".join(outputs), None)

    @property
    def code_execution_config(self) -> dict[str, Any]:
        """AutoGen code execution configuration."""
        return {
            "executor": "ravenfabric",
            "work_dir": "/tmp",
            "use_docker": False,
            "timeout": self._client._timeout,
        }
