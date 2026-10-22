"""remove recap model settings

Revision ID: 68bd9bdb5e84
Revises: 1e1c66832a1a
Create Date: 2026-10-22 18:11:22.847246

"""

from collections.abc import Sequence

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision: str = "68bd9bdb5e84"
down_revision: str | Sequence[str] | None = "1e1c66832a1a"
branch_labels: str | Sequence[str] | None = None
depends_on: str | Sequence[str] | None = None

# The recapper uses one fixed model now, so the Mini App no longer stores a model choice or lists models.
REMOVED_SETTING_KEYS = ("recap_model", "recap_thinking", "recap_available_models")


def upgrade() -> None:
    """
    Delete the obsolete recap model settings.
    """
    op.execute(
        sa.text("DELETE FROM app_setting WHERE key IN :keys").bindparams(
            sa.bindparam("keys", value=REMOVED_SETTING_KEYS, expanding=True)
        )
    )


def downgrade() -> None:
    """
    Keep the deleted settings deleted, since nothing reads them.
    """
