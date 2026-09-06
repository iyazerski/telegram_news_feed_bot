from pathlib import Path

import pytest

from src.config.configs import AppConfigs
from src.infrastructure.database.models import Base
from src.infrastructure.database.orm import Database
from src.use_cases.manage_settings import SettingsService


@pytest.fixture
def app_configs() -> AppConfigs:
    """
    Create default application configuration for unit tests.
    """
    return AppConfigs()


@pytest.fixture
def database(tmp_path: Path) -> Database:
    """
    Create an isolated temporary database with the application schema.
    """
    db = Database(f"sqlite:///{tmp_path / 'test.db'}")
    Base.metadata.create_all(db.engine)
    return db


@pytest.fixture
def settings_service() -> SettingsService:
    """
    Create the application settings service for unit tests.
    """
    return SettingsService()
