from sqlalchemy import create_engine
from sqlalchemy.orm import Session, sessionmaker


class Database:
    def __init__(self, database_url: str) -> None:
        """
        Create the SQLAlchemy engine and request session factory.
        """
        # Pre-ping so pooled connections survive Postgres restarts between idle poll cycles.
        self.engine = create_engine(database_url, pool_pre_ping=True)
        self._session_factory = sessionmaker(bind=self.engine, expire_on_commit=False)

    def create_session(self) -> Session:
        """
        Create an explicitly managed database session for background work.
        """
        return self._session_factory()
