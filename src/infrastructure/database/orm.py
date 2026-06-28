from collections.abc import Generator

from sqlalchemy import create_engine
from sqlalchemy.orm import Session, sessionmaker


class Database:
    def __init__(self, database_url: str) -> None:
        """
        Create the SQLAlchemy engine and request session factory.
        """
        self.engine = create_engine(database_url)
        self._session_factory = sessionmaker(bind=self.engine, expire_on_commit=False)

    def session(self) -> Generator[Session]:
        """
        Yield a database session for one request.
        """
        session = self._session_factory()

        # Close request sessions even when the handler raises.
        try:
            yield session
        finally:
            session.close()

    def create_session(self) -> Session:
        """
        Create an explicitly managed database session for background work.
        """
        return self._session_factory()
