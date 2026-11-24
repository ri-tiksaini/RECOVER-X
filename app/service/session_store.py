import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional


DEFAULT_DB_PATH = Path.home() / ".local" / "share" / "recoverx" / "sessions.db"


class SessionStore:
    '''SQLite persistence of scan sessions including candidate extents for zero-rescan export [LAW-4].'''

    def __init__(self, db_path: Path = DEFAULT_DB_PATH) -> None:
        self.db_path = db_path
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_schema()

    def _connect(self) -> sqlite3.Connection:
        return sqlite3.connect(str(self.db_path))

    def _init_schema(self) -> None:
        with self._connect() as conn:
            conn.execute(
                '''
                CREATE TABLE IF NOT EXISTS sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_utc TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    fs_kind TEXT NOT NULL,
                    mode TEXT NOT NULL,
                    candidate_count INTEGER NOT NULL,
                    candidates_json TEXT NOT NULL
                )
                '''
            )

    def save_session(
        self,
        source_path: str,
        fs_kind: str,
        mode: str,
        candidates: list[dict[str, Any]],
    ) -> int:
        now = datetime.now(timezone.utc).isoformat()
        payload = json.dumps(candidates)
        with self._connect() as conn:
            cur = conn.execute(
                '''
                INSERT INTO sessions (created_utc, source_path, fs_kind, mode, candidate_count, candidates_json)
                VALUES (?, ?, ?, ?, ?, ?)
                ''',
                (now, source_path, fs_kind, mode, len(candidates), payload),
            )
            return int(cur.lastrowid)

    def list_sessions(self, limit: int = 20) -> list[dict[str, Any]]:
        with self._connect() as conn:
            rows = conn.execute(
                '''
                SELECT id, created_utc, source_path, fs_kind, mode, candidate_count
                FROM sessions
                ORDER BY id DESC
                LIMIT ?
                ''',
                (limit,),
            ).fetchall()
        return [
            {
                "id": r[0],
                "created_utc": r[1],
                "source_path": r[2],
                "fs_kind": r[3],
                "mode": r[4],
                "candidate_count": r[5],
            }
            for r in rows
        ]

    def load_session(self, session_id: int) -> Optional[dict[str, Any]]:
        with self._connect() as conn:
            row = conn.execute(
                '''
                SELECT id, created_utc, source_path, fs_kind, mode, candidates_json
                FROM sessions WHERE id = ?
                ''',
                (session_id,),
            ).fetchone()
        if not row:
            return None
        return {
            "id": row[0],
            "created_utc": row[1],
            "source_path": row[2],
            "fs_kind": row[3],
            "mode": row[4],
            "candidates": json.loads(row[5]),
        }
