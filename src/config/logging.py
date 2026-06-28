import logging
import sys

import loguru


class _InterceptHandler(logging.Handler):
    def emit(self, record: logging.LogRecord) -> None:
        """
        Forward standard-library log records to Loguru.
        """
        try:
            level: str | int = loguru.logger.level(record.levelname).name
        except ValueError:
            level = record.levelno

        frame = logging.currentframe()
        depth = 2

        # Skip logging internals so Loguru reports the original caller.
        while frame is not None and frame.f_code.co_filename == logging.__file__:
            frame = frame.f_back
            depth += 1

        loguru.logger.opt(depth=depth, exception=record.exc_info).log(level, record.getMessage())


def configure_logging(log_level: str) -> None:
    """
    Configure Loguru output and route standard-library loggers through it.
    """
    loguru.logger.remove()
    loguru.logger.add(sys.stderr, level=log_level)
    _configure_standard_logging()


def _configure_standard_logging() -> None:
    """
    Route standard-library loggers to the configured Loguru sinks.
    """
    intercept_handler = _InterceptHandler()
    logging.basicConfig(handlers=[intercept_handler], level=0, force=True)

    # Remove library-specific handlers so every record flows through the root interceptor once.
    for logger_name in tuple(logging.root.manager.loggerDict):
        stdlib_logger = logging.getLogger(logger_name)
        stdlib_logger.handlers = []
        stdlib_logger.propagate = True

    # Avoid logging Telegram Bot API URLs because they contain the bot token.
    logging.getLogger("httpx").setLevel(logging.WARNING)
    logging.getLogger("httpcore").setLevel(logging.WARNING)
