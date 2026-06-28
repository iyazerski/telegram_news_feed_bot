from logging.config import fileConfig

from alembic import context
from sqlalchemy import create_engine, pool

from src.config.configs import load_configs
from src.infrastructure.database.models import Base

config = context.config
app_configs = load_configs()

if config.config_file_name is not None:
    fileConfig(config.config_file_name)

target_metadata = Base.metadata


def run_migrations_offline() -> None:
    """
    Run migrations without opening a database connection.
    """
    context.configure(
        url=app_configs.database_url,
        target_metadata=target_metadata,
        literal_binds=True,
        dialect_opts={"paramstyle": "named"},
    )

    with context.begin_transaction():
        context.run_migrations()


def run_migrations_online() -> None:
    """
    Run migrations against the configured database.
    """
    connectable = create_engine(app_configs.database_url, poolclass=pool.NullPool)

    with connectable.connect() as connection:
        context.configure(connection=connection, target_metadata=target_metadata)

        with context.begin_transaction():
            context.run_migrations()


if context.is_offline_mode():
    run_migrations_offline()
else:
    run_migrations_online()
