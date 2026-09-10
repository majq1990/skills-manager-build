//! Agent artifact persistence: the `agents`, `agent_targets`, and
//! `discovered_agents` tables introduced by migration v7.
//!
//! Implemented as an inherent `impl SkillStore` extension so the agent
//! family shares the skills family's single connection, lifecycle, and
//! state management (design spec §4 — independent tables, shared store).

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::skill_store::SkillStore;

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn new_row_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Human-facing display name (e.g. a WorkBuddy expert's localized
    /// `displayName`). Identity remains `name`; this is purely cosmetic.
    pub display_name: Option<String>,
    pub source_type: String,
    pub source_ref: Option<String>,
    pub central_path: String,
    pub content_hash: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentTargetRecord {
    pub id: String,
    pub agent_id: String,
    pub tool: String,
    pub target_path: String,
    pub mode: String,
    pub status: String,
    pub synced_at: Option<i64>,
    pub last_error: Option<String>,
    pub source_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredAgentRecord {
    pub id: String,
    pub tool: String,
    pub found_path: String,
    pub name_guess: Option<String>,
    pub fingerprint: Option<String>,
    pub found_at: i64,
    pub imported_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioAgentToolToggleRecord {
    pub scenario_id: String,
    pub agent_id: String,
    pub tool: String,
    pub enabled: bool,
    pub updated_at: i64,
}

fn row_to_agent(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    Ok(AgentRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        source_type: row.get(3)?,
        source_ref: row.get(4)?,
        central_path: row.get(5)?,
        content_hash: row.get(6)?,
        enabled: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        status: row.get(10)?,
        display_name: row.get(11)?,
    })
}

const AGENT_COLS: &str = "id, name, description, source_type, source_ref, central_path, \
     content_hash, enabled, created_at, updated_at, status, display_name";

impl SkillStore {
    // ── agents ──

    pub fn insert_agent(&self, agent: &AgentRecord) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            &format!(
                "INSERT INTO agents ({AGENT_COLS}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
            ),
            params![
                agent.id,
                agent.name,
                agent.description,
                agent.source_type,
                agent.source_ref,
                agent.central_path,
                agent.content_hash,
                agent.enabled as i64,
                agent.created_at,
                agent.updated_at,
                agent.status,
                agent.display_name,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_agent(&self, agent: &AgentRecord) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            &format!(
                "INSERT INTO agents ({AGENT_COLS}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
                 ON CONFLICT(id) DO UPDATE SET \
                   name=excluded.name, description=excluded.description, \
                   display_name=COALESCE(excluded.display_name, agents.display_name), \
                   source_type=excluded.source_type, source_ref=excluded.source_ref, \
                   central_path=excluded.central_path, content_hash=excluded.content_hash, \
                   enabled=excluded.enabled, updated_at=excluded.updated_at, \
                   status=excluded.status"
            ),
            params![
                agent.id,
                agent.name,
                agent.description,
                agent.source_type,
                agent.source_ref,
                agent.central_path,
                agent.content_hash,
                agent.enabled as i64,
                agent.created_at,
                agent.updated_at,
                agent.status,
                agent.display_name,
            ],
        )?;
        Ok(())
    }

    pub fn get_all_agents(&self) -> Result<Vec<AgentRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {AGENT_COLS} FROM agents ORDER BY name COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([], row_to_agent)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_agent_by_id(&self, id: &str) -> Result<Option<AgentRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {AGENT_COLS} FROM agents WHERE id = ?1"
        ))?;
        Ok(stmt
            .query_row(params![id], row_to_agent)
            .optional()?)
    }

    pub fn get_agent_by_name(&self, name: &str) -> Result<Option<AgentRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {AGENT_COLS} FROM agents WHERE name = ?1"
        ))?;
        Ok(stmt
            .query_row(params![name], row_to_agent)
            .optional()?)
    }

    pub fn update_agent_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE agents SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, enabled as i64, now_ts()],
        )?;
        Ok(())
    }

    pub fn update_agent_hash(&self, id: &str, content_hash: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE agents SET content_hash = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, content_hash, now_ts()],
        )?;
        Ok(())
    }

    pub fn delete_agent(&self, id: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        // agent_targets / discovered_agents references cascade (FK ON DELETE CASCADE).
        conn.execute("DELETE FROM agents WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── agent_targets ──

    pub fn insert_agent_target(&self, target: &AgentTargetRecord) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "INSERT INTO agent_targets \
               (id, agent_id, tool, target_path, mode, status, synced_at, last_error, source_hash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(agent_id, tool) DO UPDATE SET \
               target_path=excluded.target_path, mode=excluded.mode, \
               status=excluded.status, synced_at=excluded.synced_at, \
               last_error=excluded.last_error, source_hash=excluded.source_hash",
            params![
                target.id,
                target.agent_id,
                target.tool,
                target.target_path,
                target.mode,
                target.status,
                target.synced_at,
                target.last_error,
                target.source_hash,
            ],
        )?;
        Ok(())
    }

    pub fn get_targets_for_agent(&self, agent_id: &str) -> Result<Vec<AgentTargetRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, tool, target_path, mode, status, synced_at, last_error, \
                    source_hash \
             FROM agent_targets WHERE agent_id = ?1 ORDER BY tool",
        )?;
        let rows = stmt.query_map(params![agent_id], row_to_agent_target)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_all_agent_targets(&self) -> Result<Vec<AgentTargetRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, tool, target_path, mode, status, synced_at, last_error, \
                    source_hash \
             FROM agent_targets ORDER BY tool",
        )?;
        let rows = stmt.query_map([], row_to_agent_target)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_agent_target(&self, agent_id: &str, tool: &str) -> Result<Option<AgentTargetRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, tool, target_path, mode, status, synced_at, last_error, \
                    source_hash \
             FROM agent_targets WHERE agent_id = ?1 AND tool = ?2",
        )?;
        Ok(stmt
            .query_row(params![agent_id, tool], row_to_agent_target)
            .optional()?)
    }

    pub fn delete_agent_target(&self, agent_id: &str, tool: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "DELETE FROM agent_targets WHERE agent_id = ?1 AND tool = ?2",
            params![agent_id, tool],
        )?;
        Ok(())
    }

    // ── discovered_agents ──

    pub fn clear_discovered_agents(&self) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute("DELETE FROM discovered_agents", [])?;
        Ok(())
    }

    pub fn insert_discovered_agent(&self, rec: &DiscoveredAgentRecord) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "INSERT INTO discovered_agents \
               (id, tool, found_path, name_guess, fingerprint, found_at, imported_agent_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rec.id,
                rec.tool,
                rec.found_path,
                rec.name_guess,
                rec.fingerprint,
                rec.found_at,
                rec.imported_agent_id,
            ],
        )?;
        Ok(())
    }

    pub fn get_all_discovered_agents(&self) -> Result<Vec<DiscoveredAgentRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tool, found_path, name_guess, fingerprint, found_at, imported_agent_id \
             FROM discovered_agents ORDER BY tool, found_path",
        )?;
        let rows = stmt.query_map([], row_to_discovered_agent)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn set_discovered_agent_imported(
        &self,
        discovered_id: &str,
        imported_agent_id: &str,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE discovered_agents SET imported_agent_id = ?2 WHERE id = ?1",
            params![discovered_id, imported_agent_id],
        )?;
        Ok(())
    }

    // ── scenario membership (Preset 混装, v8) ──

    pub fn add_agent_to_scenario(&self, scenario_id: &str, agent_id: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR IGNORE INTO scenario_agents (scenario_id, agent_id, added_at) VALUES (?1, ?2, ?3)",
            params![scenario_id, agent_id, now],
        )?;
        Ok(())
    }

    pub fn remove_agent_from_scenario(&self, scenario_id: &str, agent_id: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "DELETE FROM scenario_agents WHERE scenario_id = ?1 AND agent_id = ?2",
            params![scenario_id, agent_id],
        )?;
        Ok(())
    }

    pub fn get_agent_ids_for_scenario(&self, scenario_id: &str) -> Result<Vec<String>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT agent_id FROM scenario_agents WHERE scenario_id = ?1 ORDER BY sort_order, added_at",
        )?;
        let rows = stmt.query_map(params![scenario_id], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_agents_for_scenario(&self, scenario_id: &str) -> Result<Vec<AgentRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {AGENT_COLS} FROM agents a
             INNER JOIN scenario_agents sa ON a.id = sa.agent_id
             WHERE sa.scenario_id = ?1
             ORDER BY sa.sort_order, a.name"
        ))?;
        let rows = stmt.query_map(params![scenario_id], row_to_agent)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn count_agents_for_scenario(&self, scenario_id: &str) -> Result<i64> {
        let conn = self.conn().lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scenario_agents WHERE scenario_id = ?1",
            params![scenario_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_scenarios_for_agent(&self, agent_id: &str) -> Result<Vec<String>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT scenario_id FROM scenario_agents WHERE agent_id = ?1")?;
        let rows = stmt.query_map(params![agent_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn ensure_scenario_agent_tool_defaults(
        &self,
        scenario_id: &str,
        agent_id: &str,
        tools: &[String],
    ) -> Result<()> {
        if tools.is_empty() {
            return Ok(());
        }

        let conn = self.conn().lock().unwrap();
        let mut existing_stmt = conn.prepare(
            "SELECT tool
             FROM scenario_agent_tools
             WHERE scenario_id = ?1 AND agent_id = ?2",
        )?;
        let existing_rows = existing_stmt.query_map(params![scenario_id, agent_id], |row| {
            row.get::<_, String>(0)
        })?;
        let existing_tools: std::collections::HashSet<String> = existing_rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .collect();

        let missing_tools: Vec<&String> = tools
            .iter()
            .filter(|tool| !existing_tools.contains(*tool))
            .collect();
        if missing_tools.is_empty() {
            return Ok(());
        }

        let tx = conn.unchecked_transaction()?;
        let now = chrono::Utc::now().timestamp_millis();

        for tool in missing_tools {
            tx.execute(
                "INSERT OR IGNORE INTO scenario_agent_tools (scenario_id, agent_id, tool, enabled, updated_at)
                 VALUES (?1, ?2, ?3, 1, ?4)",
                params![scenario_id, agent_id, tool, now],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn set_scenario_agent_tool_enabled(
        &self,
        scenario_id: &str,
        agent_id: &str,
        tool: &str,
        enabled: bool,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO scenario_agent_tools (scenario_id, agent_id, tool, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(scenario_id, agent_id, tool)
             DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
            params![scenario_id, agent_id, tool, enabled, now],
        )?;
        Ok(())
    }

    pub fn get_scenario_agent_tool_toggles(
        &self,
        scenario_id: &str,
        agent_id: &str,
    ) -> Result<Vec<ScenarioAgentToolToggleRecord>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT scenario_id, agent_id, tool, enabled, updated_at
             FROM scenario_agent_tools
             WHERE scenario_id = ?1 AND agent_id = ?2
             ORDER BY tool",
        )?;
        let rows = stmt.query_map(params![scenario_id, agent_id], |row| {
            Ok(ScenarioAgentToolToggleRecord {
                scenario_id: row.get(0)?,
                agent_id: row.get(1)?,
                tool: row.get(2)?,
                enabled: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_enabled_tools_for_scenario_agent(
        &self,
        scenario_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT tool
             FROM scenario_agent_tools
             WHERE scenario_id = ?1 AND agent_id = ?2 AND enabled = 1",
        )?;
        let rows = stmt.query_map(params![scenario_id, agent_id], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Convenience constructor for new agent rows.
    pub fn new_agent_record(
        name: String,
        description: Option<String>,
        source_type: &str,
        central_path: String,
        content_hash: Option<String>,
    ) -> AgentRecord {
        let ts = now_ts();
        AgentRecord {
            id: new_row_id(),
            name,
            description,
            display_name: None,
            source_type: source_type.to_string(),
            source_ref: None,
            central_path,
            content_hash,
            enabled: true,
            created_at: ts,
            updated_at: ts,
            status: "ok".into(),
        }
    }

    /// Convenience constructor for new target rows.
    pub fn new_agent_target_record(
        agent_id: String,
        tool: String,
        target_path: String,
        mode: String,
        source_hash: Option<String>,
    ) -> AgentTargetRecord {
        AgentTargetRecord {
            id: new_row_id(),
            agent_id,
            tool,
            target_path,
            mode,
            status: "ok".into(),
            synced_at: Some(now_ts()),
            last_error: None,
            source_hash,
        }
    }

    /// Convenience constructor for discovery rows.
    pub fn new_discovered_agent_record(
        tool: String,
        found_path: String,
        name_guess: Option<String>,
        fingerprint: Option<String>,
    ) -> DiscoveredAgentRecord {
        DiscoveredAgentRecord {
            id: new_row_id(),
            tool,
            found_path,
            name_guess,
            fingerprint,
            found_at: now_ts(),
            imported_agent_id: None,
        }
    }
}

fn row_to_agent_target(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTargetRecord> {
    Ok(AgentTargetRecord {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        tool: row.get(2)?,
        target_path: row.get(3)?,
        mode: row.get(4)?,
        status: row.get(5)?,
        synced_at: row.get(6)?,
        last_error: row.get(7)?,
        source_hash: row.get(8)?,
    })
}

fn row_to_discovered_agent(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<DiscoveredAgentRecord> {
    Ok(DiscoveredAgentRecord {
        id: row.get(0)?,
        tool: row.get(1)?,
        found_path: row.get(2)?,
        name_guess: row.get(3)?,
        fingerprint: row.get(4)?,
        found_at: row.get(5)?,
        imported_agent_id: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> SkillStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        super::super::migrations::run_migrations(&conn).unwrap();
        // Reuse the public constructor path via an in-memory db file is not
        // possible; tests open the store through its normal `new`.
        SkillStore::from_connection(conn).expect("test store")
    }

    use rusqlite::Connection;

    #[test]
    fn agent_crud_roundtrip() {
        let store = test_store();
        let agent = SkillStore::new_agent_record(
            "implementer".into(),
            Some("does things".into()),
            "local-imported",
            "/central/agents/implementer".into(),
            Some("abc".into()),
        );
        store.insert_agent(&agent).unwrap();

        let all = store.get_all_agents().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "implementer");

        let by_name = store.get_agent_by_name("implementer").unwrap().unwrap();
        assert_eq!(by_name.id, agent.id);

        store.update_agent_enabled(&agent.id, false).unwrap();
        assert!(!store.get_agent_by_id(&agent.id).unwrap().unwrap().enabled);

        store.delete_agent(&agent.id).unwrap();
        assert!(store.get_agent_by_id(&agent.id).unwrap().is_none());
    }

    #[test]
    fn agent_target_upsert_and_cascade_delete() {
        let store = test_store();
        let agent = SkillStore::new_agent_record(
            "reviewer".into(),
            None,
            "local-imported",
            "/central/agents/reviewer".into(),
            None,
        );
        store.insert_agent(&agent).unwrap();

        let t1 = SkillStore::new_agent_target_record(
            agent.id.clone(),
            "opencode".into(),
            "/home/.config/opencode/agents/reviewer.md".into(),
            "symlink".into(),
            Some("h1".into()),
        );
        store.insert_agent_target(&t1).unwrap();

        // Upsert same (agent, tool) replaces the row.
        let t2 = SkillStore::new_agent_target_record(
            agent.id.clone(),
            "opencode".into(),
            "/home/.config/opencode/agents/reviewer.md".into(),
            "copy".into(),
            Some("h2".into()),
        );
        store.insert_agent_target(&t2).unwrap();
        assert_eq!(store.get_targets_for_agent(&agent.id).unwrap().len(), 1);
        assert_eq!(
            store.get_agent_target(&agent.id, "opencode").unwrap().unwrap().mode,
            "copy"
        );

        // Deleting the agent cascades to targets.
        store.delete_agent(&agent.id).unwrap();
        assert!(store.get_all_agent_targets().unwrap().is_empty());
    }

    #[test]
    fn discovered_agents_roundtrip() {
        let store = test_store();
        let rec = SkillStore::new_discovered_agent_record(
            "opencode".into(),
            "/home/.config/opencode/agents/web-crawler.md".into(),
            Some("web-crawler".into()),
            Some("fp".into()),
        );
        store.insert_discovered_agent(&rec).unwrap();

        let agent = SkillStore::new_agent_record(
            "web-crawler".into(),
            None,
            "local-imported",
            "/central/agents/web-crawler".into(),
            None,
        );
        store.insert_agent(&agent).unwrap();
        store
            .set_discovered_agent_imported(&rec.id, &agent.id)
            .unwrap();

        let all = store.get_all_discovered_agents().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].imported_agent_id.as_deref(), Some(agent.id.as_str()));

        store.clear_discovered_agents().unwrap();
        assert!(store.get_all_discovered_agents().unwrap().is_empty());
    }
}
