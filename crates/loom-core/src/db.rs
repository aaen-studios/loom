//! SQLite storage: sessions and messages. Migrations are keyed off
//! `PRAGMA user_version`, so upgrades are forward-only and idempotent.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{paths, Error, Result};

pub const SCHEMA_VERSION: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    fn from_str(value: &str) -> Role {
        match value {
            "assistant" => Role::Assistant,
            _ => Role::User,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    /// Reasoning variant / effort chosen for this chat.
    pub variant: Option<String>,
    pub persona_id: Option<String>,
    pub system_prompt: Option<String>,
    /// Workspace folder for tools, when the user picked one.
    pub workdir: Option<String>,
    /// Per-chat override for the tool permission mode.
    pub permission_mode: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Partial update for a session row. `None` means "leave alone"; the nested
/// `Option<Option<…>>` fields distinguish "leave alone" from "clear".
#[derive(Debug, Clone, Default)]
pub struct SessionUpdate<'a> {
    pub title: Option<&'a str>,
    pub model: Option<(&'a str, &'a str)>,
    pub variant: Option<Option<&'a str>>,
    pub persona: Option<Option<&'a str>>,
    pub system_prompt: Option<Option<&'a str>>,
    pub workdir: Option<Option<&'a str>>,
    pub permission_mode: Option<Option<&'a str>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub reasoning: Option<String>,
    /// Forward-compatible JSON: tool calls, attachments, usage.
    pub extra: Option<String>,
    pub created_at: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A workspace-index chunk: text plus its embedding.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub id: String,
    pub path: String,
    pub content: String,
    pub embedding: Vec<u8>,
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open_default() -> Result<Self> {
        Self::open(&paths::database_path()?)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let connection = Connection::open(path).map_err(map_sql(path))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(map_sql(path))?;
        let db = Self { connection };
        db.migrate(path)?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|error| Error::other(format!("sqlite (:memory:): {error}")))?;
        let db = Self { connection };
        db.migrate(Path::new(":memory:"))?;
        Ok(db)
    }

    fn migrate(&self, path: &Path) -> Result<()> {
        let current: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(map_sql(path))?;

        if current < 1 {
            self.connection
                .execute_batch(
                    r#"
                    CREATE TABLE sessions (
                        id TEXT PRIMARY KEY,
                        title TEXT NOT NULL DEFAULT '',
                        provider_id TEXT,
                        model_id TEXT,
                        persona_id TEXT,
                        system_prompt TEXT,
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL
                    );

                    CREATE TABLE messages (
                        id TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        role TEXT NOT NULL,
                        content TEXT NOT NULL DEFAULT '',
                        reasoning TEXT,
                        extra TEXT,
                        created_at INTEGER NOT NULL
                    );

                    CREATE INDEX idx_messages_session ON messages(session_id, created_at);
                    CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC);
                    "#,
                )
                .map_err(map_sql(path))?;
        }

        if current < 2 {
            self.connection
                .execute_batch("ALTER TABLE sessions ADD COLUMN variant TEXT;")
                .map_err(map_sql(path))?;
        }

        if current < 3 {
            self.connection
                .execute_batch(
                    "ALTER TABLE sessions ADD COLUMN workdir TEXT;
                     ALTER TABLE sessions ADD COLUMN permission_mode TEXT;",
                )
                .map_err(map_sql(path))?;
        }

        if current < 4 {
            self.connection
                .execute_batch(
                    r#"
                    CREATE TABLE chunks (
                        id TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        path TEXT NOT NULL,
                        content TEXT NOT NULL,
                        embedding BLOB NOT NULL,
                        created_at INTEGER NOT NULL
                    );

                    CREATE INDEX idx_chunks_session ON chunks(session_id);
                    "#,
                )
                .map_err(map_sql(path))?;
        }

        self.connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(map_sql(path))?;
        Ok(())
    }

    pub fn create_session(&self, session: &Session) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO sessions (id, title, provider_id, model_id, variant, persona_id, system_prompt, workdir, permission_mode, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    session.id,
                    session.title,
                    session.provider_id,
                    session.model_id,
                    session.variant,
                    session.persona_id,
                    session.system_prompt,
                    session.workdir,
                    session.permission_mode,
                    session.created_at,
                    session.updated_at
                ],
            )
            .map_err(map_sql("sessions"))
            .map(|_| ())
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, title, provider_id, model_id, variant, persona_id, system_prompt, created_at, updated_at, workdir, permission_mode
                 FROM sessions ORDER BY updated_at DESC",
            )
            .map_err(map_sql("sessions"))?;
        let rows = statement
            .query_map([], row_to_session)
            .map_err(map_sql("sessions"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("sessions"))
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        self.connection
            .query_row(
                "SELECT id, title, provider_id, model_id, variant, persona_id, system_prompt, created_at, updated_at, workdir, permission_mode
                 FROM sessions WHERE id = ?1",
                params![id],
                row_to_session,
            )
            .optional()
            .map_err(map_sql("sessions"))
    }

    pub fn update_session(&self, id: &str, update: SessionUpdate<'_>) -> Result<()> {
        if let Some(title) = update.title {
            self.connection
                .execute(
                    "UPDATE sessions SET title = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, title, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some((provider, model)) = update.model {
            self.connection
                .execute(
                    "UPDATE sessions SET provider_id = ?2, model_id = ?3, updated_at = ?4 WHERE id = ?1",
                    params![id, provider, model, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some(variant) = update.variant {
            self.connection
                .execute(
                    "UPDATE sessions SET variant = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, variant, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some(persona) = update.persona {
            self.connection
                .execute(
                    "UPDATE sessions SET persona_id = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, persona, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some(prompt) = update.system_prompt {
            self.connection
                .execute(
                    "UPDATE sessions SET system_prompt = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, prompt, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some(workdir) = update.workdir {
            self.connection
                .execute(
                    "UPDATE sessions SET workdir = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, workdir, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some(mode) = update.permission_mode {
            self.connection
                .execute(
                    "UPDATE sessions SET permission_mode = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, mode, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .map_err(map_sql("sessions"))
            .map(|_| ())
    }

    pub fn add_message(&self, message: &Message) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO messages (id, session_id, role, content, reasoning, extra, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    message.id,
                    message.session_id,
                    message.role.as_str(),
                    message.content,
                    message.reasoning,
                    message.extra,
                    message.created_at
                ],
            )
            .map_err(map_sql("messages"))?;
        self.connection
            .execute(
                "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
                params![message.session_id, message.created_at],
            )
            .map_err(map_sql("messages"))?;
        Ok(())
    }

    pub fn update_message_extra(&self, id: &str, extra: Option<&str>) -> Result<()> {
        self.connection
            .execute(
                "UPDATE messages SET extra = ?2 WHERE id = ?1",
                params![id, extra],
            )
            .map_err(map_sql("messages"))
            .map(|_| ())
    }

    pub fn update_message(&self, id: &str, content: &str, reasoning: Option<&str>) -> Result<()> {
        self.connection
            .execute(
                "UPDATE messages SET content = ?2, reasoning = ?3 WHERE id = ?1",
                params![id, content, reasoning],
            )
            .map_err(map_sql("messages"))
            .map(|_| ())
    }

    pub fn delete_message(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM messages WHERE id = ?1", params![id])
            .map_err(map_sql("messages"))
            .map(|_| ())
    }

    /// Messages in a session, oldest first.
    pub fn messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, session_id, role, content, reasoning, extra, created_at
                 FROM messages WHERE session_id = ?1 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(map_sql("messages"))?;
        let rows = statement
            .query_map(params![session_id], row_to_message)
            .map_err(map_sql("messages"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("messages"))
    }

    /// The N most recent messages, returned oldest-first (context window).
    pub fn recent_messages(&self, session_id: &str, limit: u32) -> Result<Vec<Message>> {
        let mut all = self.messages(session_id)?;
        if all.len() > limit as usize {
            all.drain(0..all.len() - limit as usize);
        }
        Ok(all)
    }

    // ---------------------------------------------------------------- chunks

    /// Replaces the whole index for a session in one transaction.
    pub fn replace_chunks(&self, session_id: &str, chunks: &[Chunk]) -> Result<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sql("chunks"))?;
        transaction
            .execute("DELETE FROM chunks WHERE session_id = ?1", params![session_id])
            .map_err(map_sql("chunks"))?;

        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO chunks (id, session_id, path, content, embedding, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(map_sql("chunks"))?;
            let now = now_ms();
            for chunk in chunks {
                statement
                    .execute(params![
                        chunk.id,
                        session_id,
                        chunk.path,
                        chunk.content,
                        chunk.embedding,
                        now
                    ])
                    .map_err(map_sql("chunks"))?;
            }
        }

        transaction.commit().map_err(map_sql("chunks"))
    }

    pub fn chunks(&self, session_id: &str) -> Result<Vec<Chunk>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, path, content, embedding FROM chunks WHERE session_id = ?1",
            )
            .map_err(map_sql("chunks"))?;
        let rows = statement
            .query_map(params![session_id], |row| {
                Ok(Chunk {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    content: row.get(2)?,
                    embedding: row.get(3)?,
                })
            })
            .map_err(map_sql("chunks"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("chunks"))
    }

    pub fn chunk_count(&self, session_id: &str) -> Result<usize> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM chunks WHERE session_id = ?1",
                params![session_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
            .map_err(map_sql("chunks"))
    }

    pub fn clear_chunks(&self, session_id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM chunks WHERE session_id = ?1", params![session_id])
            .map_err(map_sql("chunks"))
            .map(|_| ())
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        title: row.get(1)?,
        provider_id: row.get(2)?,
        model_id: row.get(3)?,
        variant: row.get(4)?,
        persona_id: row.get(5)?,
        system_prompt: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        workdir: row.get(9)?,
        permission_mode: row.get(10)?,
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let role: String = row.get(2)?;
    Ok(Message {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: Role::from_str(&role),
        content: row.get(3)?,
        reasoning: row.get(4)?,
        extra: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn map_sql(what: impl Into<PathBuf>) -> impl Fn(rusqlite::Error) -> Error {
    let what = what.into();
    move |error| Error::Other(format!("sqlite ({}): {error}", what.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            title: "Test".into(),
            provider_id: Some("openai".into()),
            model_id: Some("gpt-x".into()),
            variant: None,
            persona_id: None,
            system_prompt: None,
            workdir: None,
            permission_mode: None,
            created_at: now_ms(),
            updated_at: now_ms(),
        }
    }

    fn message(id: &str, session_id: &str, role: Role, content: &str) -> Message {
        Message {
            id: id.to_string(),
            session_id: session_id.to_string(),
            role,
            content: content.to_string(),
            reasoning: None,
            extra: None,
            created_at: now_ms(),
        }
    }

    #[test]
    fn session_and_message_round_trip() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();
        db.add_message(&message("m1", "s1", Role::User, "hello"))
            .unwrap();
        db.add_message(&message("m2", "s1", Role::Assistant, "hi there"))
            .unwrap();

        let sessions = db.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "s1");

        let messages = db.messages("s1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].content, "hi there");
    }

    #[test]
    fn deleting_a_session_cascades_to_messages() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();
        db.add_message(&message("m1", "s1", Role::User, "hello"))
            .unwrap();

        db.delete_session("s1").unwrap();

        assert!(db.get_session("s1").unwrap().is_none());
        assert!(db.messages("s1").unwrap().is_empty());
    }

    #[test]
    fn recent_messages_keeps_the_tail_in_order() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();
        for index in 0..10 {
            db.add_message(&message(
                &format!("m{index}"),
                "s1",
                Role::User,
                &format!("msg {index}"),
            ))
            .unwrap();
        }

        let recent = db.recent_messages("s1", 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "msg 7");
        assert_eq!(recent[2].content, "msg 9");
    }

    #[test]
    fn chunk_index_round_trips_and_cascades() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();

        let chunks = vec![
            Chunk {
                id: "c1".into(),
                path: "src/main.rs".into(),
                content: "fn main() {}".into(),
                embedding: vec![1, 2, 3, 4],
            },
            Chunk {
                id: "c2".into(),
                path: "README.md".into(),
                content: "# hi".into(),
                embedding: vec![5, 6, 7, 8],
            },
        ];
        db.replace_chunks("s1", &chunks).unwrap();
        assert_eq!(db.chunk_count("s1").unwrap(), 2);

        let stored = db.chunks("s1").unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].embedding, vec![1, 2, 3, 4]);

        // Re-indexing replaces rather than appends.
        db.replace_chunks("s1", &chunks[..1]).unwrap();
        assert_eq!(db.chunk_count("s1").unwrap(), 1);

        db.delete_session("s1").unwrap();
        assert_eq!(db.chunk_count("s1").unwrap(), 0);
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("loom.db");
        {
            Database::open(&path).unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert!(db.list_sessions().unwrap().is_empty());
    }

    #[test]
    fn session_variant_and_persona_updates_round_trip() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();

        db.update_session(
            "s1",
            SessionUpdate {
                variant: Some(Some("high")),
                persona: Some(Some("coder")),
                system_prompt: Some(Some("Be terse.")),
                model: Some(("anthropic", "claude-sonnet-4")),
                workdir: Some(Some("C:/work")),
                permission_mode: Some(Some("auto-read-only")),
                ..Default::default()
            },
        )
        .unwrap();

        let stored = db.get_session("s1").unwrap().unwrap();
        assert_eq!(stored.variant.as_deref(), Some("high"));
        assert_eq!(stored.persona_id.as_deref(), Some("coder"));
        assert_eq!(stored.system_prompt.as_deref(), Some("Be terse."));
        assert_eq!(stored.provider_id.as_deref(), Some("anthropic"));
        assert_eq!(stored.workdir.as_deref(), Some("C:/work"));
        assert_eq!(stored.permission_mode.as_deref(), Some("auto-read-only"));

        // Clearing works and leaves other fields alone.
        db.update_session(
            "s1",
            SessionUpdate {
                variant: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
        let stored = db.get_session("s1").unwrap().unwrap();
        assert!(stored.variant.is_none());
        assert_eq!(stored.persona_id.as_deref(), Some("coder"));
    }
}



