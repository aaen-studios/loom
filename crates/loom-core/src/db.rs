//! SQLite storage: sessions and messages. Migrations are keyed off
//! `PRAGMA user_version`, so upgrades are forward-only and idempotent.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{paths, Error, Result};

pub const SCHEMA_VERSION: i64 = 9;

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
    /// Per-chat override for the agent mode (plan or build).
    pub agent_mode: Option<String>,
    /// Computer use is armed for this chat with the composer's Computer chip.
    pub computer_access: bool,
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
    pub agent_mode: Option<Option<&'a str>>,
    pub computer_access: Option<bool>,
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
    /// Which persona (or cast member) produced an assistant reply. `None` for
    /// user messages and single-persona chats recorded before this existed.
    pub persona_id: Option<String>,
    pub created_at: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One persistent persona memory entry, shared across every chat that uses the
/// persona.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntry {
    pub id: String,
    pub persona_id: String,
    /// Short stable label; upserting the same key replaces the value.
    pub key: String,
    pub value: String,
    /// `user` or `model`, so the UI can show provenance.
    pub source: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A workspace-index chunk: text plus its embedding.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub id: String,
    pub path: String,
    pub content: String,
    pub embedding: Vec<u8>,
}

/// One item on a chat's live task list, maintained by `todo_write` and the
/// GoalPanel above the composer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    pub content: String,
    /// `pending` | `in_progress` | `completed`
    pub status: String,
    #[serde(default)]
    pub position: i64,
}

/// A long-term memory: one durable fact, scoped `global` or to a workspace
/// folder path. The embedding is never serialized to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub id: String,
    /// `global`, or the workspace folder path it belongs to.
    pub scope: String,
    pub content: String,
    #[serde(skip)]
    pub embedding: Option<Vec<u8>>,
    pub pinned: bool,
    pub source_session: Option<String>,
    pub source_message: Option<String>,
    /// `user`, `model`, or `auto` (the extraction pass).
    pub source: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A detached agent run: background subagent, or one firing of a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    /// The hidden session the run writes its transcript to.
    pub session_id: String,
    /// The chat that spawned it, when a chat did.
    pub origin_session: Option<String>,
    pub job_id: Option<String>,
    pub title: String,
    pub prompt: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    /// `queued` | `running` | `done` | `failed` | `interrupted` | `cancelled` | `skipped`
    pub status: String,
    /// Why it ended, or what it is waiting on.
    pub detail: Option<String>,
    /// The run's final reply (or the failure), for the list view.
    pub result: Option<String>,
    pub notify: bool,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// A scheduled job: a prompt plus a cron expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub name: String,
    /// Standard five-field cron: minute hour day-of-month month day-of-week.
    pub cron: String,
    pub enabled: bool,
    pub prompt: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub persona_id: Option<String>,
    pub workdir: Option<String>,
    pub permission_mode: Option<String>,
    pub notify_on_success: bool,
    /// Catch-up window in minutes for firings missed while the app was closed.
    pub catch_up_minutes: i64,
    pub last_run_at: Option<i64>,
    pub last_status: Option<String>,
    pub next_run_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
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

        if current < 5 {
            self.connection
                .execute_batch("ALTER TABLE sessions ADD COLUMN agent_mode TEXT;")
                .map_err(map_sql(path))?;
        }

        if current < 6 {
            self.connection
                .execute_batch(
                    "ALTER TABLE sessions ADD COLUMN computer_access INTEGER NOT NULL DEFAULT 0;",
                )
                .map_err(map_sql(path))?;
        }

        if current < 7 {
            self.connection
                .execute_batch(
                    r#"
                    ALTER TABLE messages ADD COLUMN persona_id TEXT;

                    CREATE TABLE persona_memory (
                        id TEXT PRIMARY KEY,
                        persona_id TEXT NOT NULL,
                        key TEXT NOT NULL,
                        value TEXT NOT NULL,
                        source TEXT NOT NULL DEFAULT 'user',
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL
                    );

                    CREATE UNIQUE INDEX idx_persona_memory_key
                        ON persona_memory(persona_id, key);

                    CREATE TABLE session_personas (
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        persona_id TEXT NOT NULL,
                        position INTEGER NOT NULL,
                        PRIMARY KEY (session_id, persona_id)
                    );
                    "#,
                )
                .map_err(map_sql(path))?;
        }

        if current < 8 {
            self.connection
                .execute_batch(
                    r#"
                    ALTER TABLE sessions ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat';

                    CREATE TABLE memories (
                        id TEXT PRIMARY KEY,
                        scope TEXT NOT NULL,
                        content TEXT NOT NULL,
                        embedding BLOB,
                        pinned INTEGER NOT NULL DEFAULT 0,
                        source_session TEXT,
                        source_message TEXT,
                        source TEXT NOT NULL DEFAULT 'model',
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL
                    );

                    CREATE INDEX idx_memories_scope ON memories(scope, updated_at DESC);

                    CREATE TABLE tasks (
                        id TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL,
                        origin_session TEXT,
                        job_id TEXT,
                        title TEXT NOT NULL DEFAULT '',
                        prompt TEXT NOT NULL,
                        provider_id TEXT,
                        model_id TEXT,
                        status TEXT NOT NULL DEFAULT 'queued',
                        detail TEXT,
                        result TEXT,
                        notify INTEGER NOT NULL DEFAULT 1,
                        created_at INTEGER NOT NULL,
                        started_at INTEGER,
                        finished_at INTEGER
                    );

                    CREATE INDEX idx_tasks_created ON tasks(created_at DESC);
                    CREATE INDEX idx_tasks_status ON tasks(status, created_at DESC);

                    CREATE TABLE jobs (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        cron TEXT NOT NULL,
                        enabled INTEGER NOT NULL DEFAULT 1,
                        prompt TEXT NOT NULL,
                        provider_id TEXT,
                        model_id TEXT,
                        persona_id TEXT,
                        workdir TEXT,
                        permission_mode TEXT,
                        notify_on_success INTEGER NOT NULL DEFAULT 0,
                        catch_up_minutes INTEGER NOT NULL DEFAULT 720,
                        last_run_at INTEGER,
                        last_status TEXT,
                        next_run_at INTEGER,
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL
                    );
                    "#,
                )
                .map_err(map_sql(path))?;
        }

        if current < 9 {
            self.connection
                .execute_batch(
                    r#"
                    CREATE TABLE session_goals (
                        session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                        goal TEXT NOT NULL,
                        updated_at INTEGER NOT NULL
                    );

                    CREATE TABLE todos (
                        id TEXT PRIMARY KEY,
                        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                        content TEXT NOT NULL,
                        status TEXT NOT NULL DEFAULT 'pending',
                        position INTEGER NOT NULL,
                        created_at INTEGER NOT NULL,
                        updated_at INTEGER NOT NULL
                    );

                    CREATE INDEX idx_todos_session ON todos(session_id, position);
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
                "INSERT INTO sessions (id, title, provider_id, model_id, variant, persona_id, system_prompt, workdir, permission_mode, agent_mode, computer_access, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                    session.agent_mode,
                    session.computer_access,
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
                "SELECT id, title, provider_id, model_id, variant, persona_id, system_prompt, created_at, updated_at, workdir, permission_mode, agent_mode, computer_access
                 FROM sessions WHERE kind = 'chat' ORDER BY updated_at DESC",
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
                "SELECT id, title, provider_id, model_id, variant, persona_id, system_prompt, created_at, updated_at, workdir, permission_mode, agent_mode, computer_access
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
        if let Some(mode) = update.agent_mode {
            self.connection
                .execute(
                    "UPDATE sessions SET agent_mode = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, mode, now_ms()],
                )
                .map_err(map_sql("sessions"))?;
        }
        if let Some(enabled) = update.computer_access {
            self.connection
                .execute(
                    "UPDATE sessions SET computer_access = ?2, updated_at = ?3 WHERE id = ?1",
                    params![id, enabled, now_ms()],
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

    /// Deletes chats that were never used: no messages and no title of their
    /// own. `keep` protects the chat the user is currently looking at, so an
    /// empty chat that is still open is not pulled out from under them.
    pub fn prune_empty_sessions(&self, keep: Option<&str>) -> Result<usize> {
        self.connection
            .execute(
                "DELETE FROM sessions
                 WHERE (?1 IS NULL OR id != ?1)
                   AND kind = 'chat'
                   AND title = ''
                   AND NOT EXISTS (
                     SELECT 1 FROM messages WHERE messages.session_id = sessions.id
                   )",
                params![keep],
            )
            .map_err(map_sql("sessions"))
    }

    pub fn add_message(&self, message: &Message) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO messages (id, session_id, role, content, reasoning, extra, persona_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    message.id,
                    message.session_id,
                    message.role.as_str(),
                    message.content,
                    message.reasoning,
                    message.extra,
                    message.persona_id,
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
                "SELECT id, session_id, role, content, reasoning, extra, created_at, persona_id
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

    /// `extra` JSON for every assistant message that has one. Usage totals want
    /// only this column, so this avoids loading (and copying) message bodies
    /// for the whole history.
    pub fn assistant_extras(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT extra FROM messages
                 WHERE role = 'assistant' AND extra IS NOT NULL",
            )
            .map_err(map_sql("messages"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(map_sql("messages"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("messages"))
    }

    // ---------------------------------------------------------------- chunks

    /// Replaces the whole index for a session in one transaction.
    pub fn replace_chunks(&self, session_id: &str, chunks: &[Chunk]) -> Result<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sql("chunks"))?;
        transaction
            .execute(
                "DELETE FROM chunks WHERE session_id = ?1",
                params![session_id],
            )
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
            .prepare("SELECT id, path, content, embedding FROM chunks WHERE session_id = ?1")
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
            .execute(
                "DELETE FROM chunks WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(map_sql("chunks"))
            .map(|_| ())
    }

    // ----------------------------------------------------------- goals/todos

    /// Sets or clears the chat's standing goal (`/goal <text>`).
    pub fn set_session_goal(&self, session_id: &str, goal: Option<&str>) -> Result<()> {
        let trimmed = goal.map(str::trim).filter(|goal| !goal.is_empty());
        match trimmed {
            Some(goal) => {
                self.connection
                    .execute(
                        "INSERT INTO session_goals (session_id, goal, updated_at)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(session_id) DO UPDATE
                         SET goal = excluded.goal, updated_at = excluded.updated_at",
                        params![session_id, goal, now_ms()],
                    )
                    .map_err(map_sql("session_goals"))
                    .map(|_| ())
            }
            None => {
                self.connection
                    .execute(
                        "DELETE FROM session_goals WHERE session_id = ?1",
                        params![session_id],
                    )
                    .map_err(map_sql("session_goals"))
                    .map(|_| ())
            }
        }
    }

    pub fn session_goal(&self, session_id: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT goal FROM session_goals WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sql("session_goals"))
    }

    /// Replaces the whole task list for a session in one transaction, the same
    /// contract `todo_write` gives the model.
    pub fn replace_todos(&self, session_id: &str, todos: &[Todo]) -> Result<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sql("todos"))?;
        transaction
            .execute("DELETE FROM todos WHERE session_id = ?1", params![session_id])
            .map_err(map_sql("todos"))?;

        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO todos (id, session_id, content, status, position, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                )
                .map_err(map_sql("todos"))?;
            let now = now_ms();
            for (index, todo) in todos.iter().enumerate() {
                statement
                    .execute(params![
                        todo.id,
                        session_id,
                        todo.content,
                        todo.status,
                        index as i64,
                        now
                    ])
                    .map_err(map_sql("todos"))?;
            }
        }

        transaction.commit().map_err(map_sql("todos"))
    }

    pub fn todos(&self, session_id: &str) -> Result<Vec<Todo>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, content, status, position FROM todos
                 WHERE session_id = ?1 ORDER BY position ASC, rowid ASC",
            )
            .map_err(map_sql("todos"))?;
        let rows = statement
            .query_map(params![session_id], |row| {
                Ok(Todo {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    status: row.get(2)?,
                    position: row.get(3)?,
                })
            })
            .map_err(map_sql("todos"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("todos"))
    }

    // -------------------------------------------------------- persona memory

    /// Every memory entry for a persona, oldest key first.
    pub fn persona_memory(&self, persona_id: &str) -> Result<Vec<MemoryEntry>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, persona_id, key, value, source, created_at, updated_at
                 FROM persona_memory WHERE persona_id = ?1 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(map_sql("persona_memory"))?;
        let rows = statement
            .query_map(params![persona_id], |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    persona_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    source: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(map_sql("persona_memory"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("persona_memory"))
    }

    /// Inserts or replaces the entry with the same `(persona_id, key)`.
    pub fn upsert_persona_memory(
        &self,
        persona_id: &str,
        key: &str,
        value: &str,
        source: &str,
    ) -> Result<MemoryEntry> {
        if let Some(mut existing) = self
            .persona_memory(persona_id)?
            .into_iter()
            .find(|entry| entry.key == key)
        {
            existing.value = value.to_string();
            existing.source = source.to_string();
            existing.updated_at = now_ms();
            self.connection
                .execute(
                    "UPDATE persona_memory SET value = ?2, source = ?3, updated_at = ?4 WHERE id = ?1",
                    params![existing.id, existing.value, existing.source, existing.updated_at],
                )
                .map_err(map_sql("persona_memory"))?;
            return Ok(existing);
        }

        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            persona_id: persona_id.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            source: source.to_string(),
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        self.connection
            .execute(
                "INSERT INTO persona_memory (id, persona_id, key, value, source, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    entry.id,
                    entry.persona_id,
                    entry.key,
                    entry.value,
                    entry.source,
                    entry.created_at,
                    entry.updated_at
                ],
            )
            .map_err(map_sql("persona_memory"))?;
        Ok(entry)
    }

    pub fn delete_persona_memory(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM persona_memory WHERE id = ?1", params![id])
            .map_err(map_sql("persona_memory"))
            .map(|_| ())
    }

    pub fn clear_persona_memory(&self, persona_id: &str) -> Result<usize> {
        self.connection
            .execute(
                "DELETE FROM persona_memory WHERE persona_id = ?1",
                params![persona_id],
            )
            .map_err(map_sql("persona_memory"))
    }

    // --------------------------------------------------------- session cast

    /// Replaces the cast for a session. An empty cast means "one persona":
    /// the session's own `persona_id`.
    pub fn set_session_cast(&self, session_id: &str, persona_ids: &[String]) -> Result<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sql("session_personas"))?;
        transaction
            .execute(
                "DELETE FROM session_personas WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(map_sql("session_personas"))?;
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO session_personas (session_id, persona_id, position)
                     VALUES (?1, ?2, ?3)",
                )
                .map_err(map_sql("session_personas"))?;
            for (position, persona_id) in persona_ids.iter().enumerate() {
                statement
                    .execute(params![session_id, persona_id, position as i64])
                    .map_err(map_sql("session_personas"))?;
            }
        }
        transaction.commit().map_err(map_sql("session_personas"))
    }

    /// The cast persona ids for a session, in order.
    pub fn session_cast(&self, session_id: &str) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT persona_id FROM session_personas
                 WHERE session_id = ?1 ORDER BY position ASC",
            )
            .map_err(map_sql("session_personas"))?;
        let rows = statement
            .query_map(params![session_id], |row| row.get::<_, String>(0))
            .map_err(map_sql("session_personas"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("session_personas"))
    }
}

/// Tasks, jobs and long-term memory. Kept in their own impl block so the
/// original session/message surface stays easy to read.
impl Database {
    /// Inserts a session that belongs to a task: hidden from the chats popup,
    /// but otherwise a normal session (messages, tools, cancelling).
    pub fn create_task_session(&self, session: &Session) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO sessions (id, title, provider_id, model_id, variant, persona_id, system_prompt, workdir, permission_mode, agent_mode, computer_access, kind, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'task', ?12, ?13)",
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
                    session.agent_mode,
                    session.computer_access,
                    session.created_at,
                    session.updated_at
                ],
            )
            .map_err(map_sql("sessions"))
            .map(|_| ())
    }

    // ------------------------------------------------------------- tasks

    pub fn create_task(&self, task: &Task) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO tasks (id, session_id, origin_session, job_id, title, prompt, provider_id, model_id, status, detail, result, notify, created_at, started_at, finished_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    task.id,
                    task.session_id,
                    task.origin_session,
                    task.job_id,
                    task.title,
                    task.prompt,
                    task.provider_id,
                    task.model_id,
                    task.status,
                    task.detail,
                    task.result,
                    task.notify,
                    task.created_at,
                    task.started_at,
                    task.finished_at
                ],
            )
            .map_err(map_sql("tasks"))
            .map(|_| ())
    }

    pub fn task(&self, id: &str) -> Result<Option<Task>> {
        self.connection
            .query_row(
                "SELECT id, session_id, origin_session, job_id, title, prompt, provider_id, model_id, status, COALESCE(detail, ''), result, notify, created_at, started_at, finished_at
                 FROM tasks WHERE id = ?1",
                params![id],
                row_to_task,
            )
            .optional()
            .map_err(map_sql("tasks"))
    }

    pub fn task_for_session(&self, session_id: &str) -> Result<Option<Task>> {
        self.connection
            .query_row(
                "SELECT id, session_id, origin_session, job_id, title, prompt, provider_id, model_id, status, COALESCE(detail, ''), result, notify, created_at, started_at, finished_at
                 FROM tasks WHERE session_id = ?1",
                params![session_id],
                row_to_task,
            )
            .optional()
            .map_err(map_sql("tasks"))
    }

    /// Newest first. Filter by job when a job's run list is opened.
    pub fn list_tasks(&self, job_id: Option<&str>) -> Result<Vec<Task>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, session_id, origin_session, job_id, title, prompt, provider_id, model_id, status, COALESCE(detail, ''), result, notify, created_at, started_at, finished_at
                 FROM tasks WHERE (?1 IS NULL OR job_id = ?1)
                 ORDER BY created_at DESC LIMIT 200",
            )
            .map_err(map_sql("tasks"))?;
        let rows = statement
            .query_map(params![job_id], row_to_task)
            .map_err(map_sql("tasks"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("tasks"))
    }

    /// Moves a task to a new status, stamping start/finish times as it goes.
    pub fn set_task_status(
        &self,
        id: &str,
        status: &str,
        detail: Option<&str>,
        result: Option<&str>,
    ) -> Result<()> {
        self.connection
            .execute(
                "UPDATE tasks SET
                   status = ?2,
                   detail = ?3,
                   result = COALESCE(?4, result),
                   started_at = CASE WHEN ?2 = 'running' AND started_at IS NULL THEN ?5 ELSE started_at END,
                   finished_at = CASE WHEN ?2 IN ('done', 'failed', 'interrupted', 'cancelled', 'skipped') THEN ?5 ELSE finished_at END
                 WHERE id = ?1",
                params![id, status, detail, result, now_ms()],
            )
            .map_err(map_sql("tasks"))
            .map(|_| ())
    }

    /// Every task that never reached a terminal status, e.g. after a crash.
    pub fn unfinished_tasks(&self) -> Result<Vec<Task>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, session_id, origin_session, job_id, title, prompt, provider_id, model_id, status, COALESCE(detail, ''), result, notify, created_at, started_at, finished_at
                 FROM tasks WHERE status IN ('queued', 'running') ORDER BY created_at ASC",
            )
            .map_err(map_sql("tasks"))?;
        let rows = statement.query_map([], row_to_task).map_err(map_sql("tasks"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("tasks"))
    }

    /// Deletes a task and the hidden session it ran in.
    pub fn delete_task(&self, id: &str) -> Result<()> {
        let session_id: Option<String> = self
            .connection
            .query_row(
                "SELECT session_id FROM tasks WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sql("tasks"))?;

        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_sql("tasks"))?;
        transaction
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(map_sql("tasks"))?;
        if let Some(session_id) = session_id {
            transaction
                .execute("DELETE FROM sessions WHERE id = ?1", params![session_id])
                .map_err(map_sql("tasks"))?;
        }
        transaction.commit().map_err(map_sql("tasks"))
    }

    // -------------------------------------------------------------- jobs

    pub fn upsert_job(&self, job: &Job) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO jobs (id, name, cron, enabled, prompt, provider_id, model_id, persona_id, workdir, permission_mode, notify_on_success, catch_up_minutes, last_run_at, last_status, next_run_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   cron = excluded.cron,
                   enabled = excluded.enabled,
                   prompt = excluded.prompt,
                   provider_id = excluded.provider_id,
                   model_id = excluded.model_id,
                   persona_id = excluded.persona_id,
                   workdir = excluded.workdir,
                   permission_mode = excluded.permission_mode,
                   notify_on_success = excluded.notify_on_success,
                   catch_up_minutes = excluded.catch_up_minutes,
                   next_run_at = excluded.next_run_at,
                   updated_at = excluded.updated_at",
                params![
                    job.id,
                    job.name,
                    job.cron,
                    job.enabled,
                    job.prompt,
                    job.provider_id,
                    job.model_id,
                    job.persona_id,
                    job.workdir,
                    job.permission_mode,
                    job.notify_on_success,
                    job.catch_up_minutes,
                    job.last_run_at,
                    job.last_status,
                    job.next_run_at,
                    job.created_at,
                    job.updated_at
                ],
            )
            .map_err(map_sql("jobs"))
            .map(|_| ())
    }

    pub fn jobs(&self) -> Result<Vec<Job>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, cron, enabled, prompt, provider_id, model_id, persona_id, workdir, permission_mode, notify_on_success, catch_up_minutes, last_run_at, last_status, next_run_at, created_at, updated_at
                 FROM jobs ORDER BY created_at ASC",
            )
            .map_err(map_sql("jobs"))?;
        let rows = statement.query_map([], row_to_job).map_err(map_sql("jobs"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("jobs"))
    }

    pub fn job(&self, id: &str) -> Result<Option<Job>> {
        self.connection
            .query_row(
                "SELECT id, name, cron, enabled, prompt, provider_id, model_id, persona_id, workdir, permission_mode, notify_on_success, catch_up_minutes, last_run_at, last_status, next_run_at, created_at, updated_at
                 FROM jobs WHERE id = ?1",
                params![id],
                row_to_job,
            )
            .optional()
            .map_err(map_sql("jobs"))
    }

    pub fn delete_job(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM jobs WHERE id = ?1", params![id])
            .map_err(map_sql("jobs"))
            .map(|_| ())
    }

    pub fn set_job_schedule(
        &self,
        id: &str,
        next_run_at: Option<i64>,
        last_run_at: Option<i64>,
        last_status: Option<&str>,
    ) -> Result<()> {
        self.connection
            .execute(
                "UPDATE jobs SET
                   next_run_at = ?2,
                   last_run_at = COALESCE(?3, last_run_at),
                   last_status = COALESCE(?4, last_status),
                   updated_at = ?5
                 WHERE id = ?1",
                params![id, next_run_at, last_run_at, last_status, now_ms()],
            )
            .map_err(map_sql("jobs"))
            .map(|_| ())
    }

    // ----------------------------------------------------------- memories

    pub fn insert_memory(&self, memory: &Memory) -> Result<()> {
        self.connection
            .execute(
                "INSERT INTO memories (id, scope, content, embedding, pinned, source_session, source_message, source, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    memory.id,
                    memory.scope,
                    memory.content,
                    memory.embedding,
                    memory.pinned,
                    memory.source_session,
                    memory.source_message,
                    memory.source,
                    memory.created_at,
                    memory.updated_at
                ],
            )
            .map_err(map_sql("memories"))
            .map(|_| ())
    }

    pub fn update_memory(
        &self,
        id: &str,
        content: &str,
        pinned: bool,
        embedding: Option<&[u8]>,
    ) -> Result<()> {
        self.connection
            .execute(
                "UPDATE memories SET content = ?2, pinned = ?3, embedding = COALESCE(?4, embedding), updated_at = ?5 WHERE id = ?1",
                params![id, content, pinned, embedding, now_ms()],
            )
            .map_err(map_sql("memories"))
            .map(|_| ())
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map_err(map_sql("memories"))
            .map(|_| ())
    }

    /// Newest first. `scope` filters to one scope (`global` or a workspace
    /// path); `None` returns everything.
    pub fn memories(&self, scope: Option<&str>) -> Result<Vec<Memory>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, scope, content, embedding, pinned, source_session, source_message, source, created_at, updated_at
                 FROM memories WHERE (?1 IS NULL OR scope = ?1)
                 ORDER BY pinned DESC, updated_at DESC",
            )
            .map_err(map_sql("memories"))?;
        let rows = statement
            .query_map(params![scope], row_to_memory)
            .map_err(map_sql("memories"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("memories"))
    }

    /// Memories in any of `scopes` that carry an embedding, for retrieval.
    pub fn memories_in_scopes(&self, scopes: &[String]) -> Result<Vec<Memory>> {
        if scopes.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; scopes.len()].join(", ");
        let sql = format!(
            "SELECT id, scope, content, embedding, pinned, source_session, source_message, source, created_at, updated_at
             FROM memories WHERE scope IN ({placeholders}) AND embedding IS NOT NULL"
        );
        let mut statement = self.connection.prepare(&sql).map_err(map_sql("memories"))?;
        let values = rusqlite::params_from_iter(scopes.iter());
        let rows = statement
            .query_map(values, row_to_memory)
            .map_err(map_sql("memories"))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_sql("memories"))
    }

    pub fn clear_memories(&self, scope: &str) -> Result<usize> {
        self.connection
            .execute("DELETE FROM memories WHERE scope = ?1", params![scope])
            .map_err(map_sql("memories"))
    }

    pub fn memory_count(&self, scope: Option<&str>) -> Result<usize> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE (?1 IS NULL OR scope = ?1)",
                params![scope],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
            .map_err(map_sql("memories"))
    }
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        session_id: row.get(1)?,
        origin_session: row.get(2)?,
        job_id: row.get(3)?,
        title: row.get(4)?,
        prompt: row.get(5)?,
        provider_id: row.get(6)?,
        model_id: row.get(7)?,
        status: row.get(8)?,
        detail: {
            let detail: String = row.get(9)?;
            if detail.is_empty() {
                None
            } else {
                Some(detail)
            }
        },
        result: row.get(10)?,
        notify: row.get(11)?,
        created_at: row.get(12)?,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
    })
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get(0)?,
        name: row.get(1)?,
        cron: row.get(2)?,
        enabled: row.get(3)?,
        prompt: row.get(4)?,
        provider_id: row.get(5)?,
        model_id: row.get(6)?,
        persona_id: row.get(7)?,
        workdir: row.get(8)?,
        permission_mode: row.get(9)?,
        notify_on_success: row.get(10)?,
        catch_up_minutes: row.get(11)?,
        last_run_at: row.get(12)?,
        last_status: row.get(13)?,
        next_run_at: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: row.get(0)?,
        scope: row.get(1)?,
        content: row.get(2)?,
        embedding: row.get(3)?,
        pinned: row.get(4)?,
        source_session: row.get(5)?,
        source_message: row.get(6)?,
        source: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
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
        agent_mode: row.get(11)?,
        computer_access: row.get(12)?,
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
        persona_id: row.get(7)?,
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
            agent_mode: None,
            computer_access: false,
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
            persona_id: None,
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
    fn assistant_extras_skip_users_and_empty_extras() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();

        let mut assistant = message("m1", "s1", Role::Assistant, "hi");
        assistant.extra = Some(r#"{"usage":{"inputTokens":1}}"#.into());
        db.add_message(&assistant).unwrap();

        let mut user = message("m2", "s1", Role::User, "hello");
        user.extra = Some(r#"{"attachments":[]}"#.into());
        db.add_message(&user).unwrap();

        db.add_message(&message("m3", "s1", Role::Assistant, "plain"))
            .unwrap();

        let extras = db.assistant_extras().unwrap();
        assert_eq!(extras.len(), 1);
        assert!(extras[0].contains("usage"));
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

    #[test]
    fn pruning_removes_unused_chats_but_keeps_the_open_one() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("empty")).unwrap();
        db.create_session(&session("kept")).unwrap();
        db.create_session(&session("used")).unwrap();
        db.create_session(&session("renamed")).unwrap();
        db.add_message(&message("m1", "used", Role::User, "hello"))
            .unwrap();
        db.update_session(
            "renamed",
            SessionUpdate {
                title: Some("Named by hand"),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(db.prune_empty_sessions(Some("kept")).unwrap(), 1);
        assert!(db.get_session("empty").unwrap().is_none());
        assert!(db.get_session("kept").unwrap().is_some());
        assert!(db.get_session("used").unwrap().is_some());
        assert!(db.get_session("renamed").unwrap().is_some());

        // With nothing open, the protected chat goes too.
        assert_eq!(db.prune_empty_sessions(None).unwrap(), 1);
        assert!(db.get_session("kept").unwrap().is_none());
        assert!(db.get_session("used").unwrap().is_some());
    }

    #[test]
    fn agent_mode_round_trips_and_clears() {
        let db = Database::open_in_memory().unwrap();
        db.create_session(&session("s1")).unwrap();
        assert!(db.get_session("s1").unwrap().unwrap().agent_mode.is_none());

        db.update_session(
            "s1",
            SessionUpdate {
                agent_mode: Some(Some("plan")),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            db.get_session("s1").unwrap().unwrap().agent_mode.as_deref(),
            Some("plan")
        );

        // Clearing means "fall back to the global default", not "build".
        db.update_session(
            "s1",
            SessionUpdate {
                agent_mode: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(db.get_session("s1").unwrap().unwrap().agent_mode.is_none());
    }
}
