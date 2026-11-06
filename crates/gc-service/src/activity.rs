//! What the Agent does through MCP, per terminal session: every tool call is recorded and pushed
//! to the UI as `agent-activity`, so the Agent panel can show readable progress next to the raw
//! terminal, and pages can show that the Agent is working on a person. `task.report` entries carry
//! the Agent's own status (working / done / needs_input / blocked).
use crate::app::App;
use rusqlite::params;
use serde_json::{Value, json};

pub const TASK_STATUSES: &[&str] = &["working", "done", "needs_input", "blocked"];

fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max { format!("{}…", flat.chars().take(max).collect::<String>()) } else { flat }
}

/// The person, the object (path / list / target) and a short detail for one call.
fn fields(tool: &str, args: &Value, result: Result<&Value, &str>) -> (String, String, String, String) {
    let ok = result.ok();
    let s = |v: &Value| v.as_str().unwrap_or("").to_owned();
    let contact = [&args["slug"], &args["contact_id"], &args["contact"]]
        .into_iter()
        .map(s)
        .find(|c| !c.is_empty())
        .or_else(|| ok.map(|r| s(&r["contact"])).filter(|c| !c.is_empty()))
        .or_else(|| (tool == "ui.open" && args["kind"] == "contact").then(|| s(&args["target"])))
        .unwrap_or_default();
    let target = if tool == "task.report" {
        // Batch progress, e.g. "3/12", so it survives a reload.
        args["total"].as_u64().map(|total| format!("{}/{total}", args["done"].as_u64().unwrap_or(0))).unwrap_or_default()
    } else {
        ok
        .map(|r| s(&r["path"]))
        .filter(|p| !p.is_empty())
            .or_else(|| [&args["path"], &args["target"], &args["list"], &args["id"], &args["name"]].into_iter().map(s).find(|p| !p.is_empty()))
            .unwrap_or_default()
    };
    let detail = match result {
        Err(error) => clip(error, 200),
        Ok(_) => match tool {
            "task.report" => clip(args["summary"].as_str().unwrap_or(""), 300),
            "memory.append" | "preferences.add" => clip(args["note"].as_str().unwrap_or(""), 160),
            "contact.update" => args["values"].as_object().map(|v| v.keys().cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default(),
            "contact.search" => clip(args["query"].as_str().unwrap_or(""), 80),
            "list.decline" => clip(args["reason"].as_str().unwrap_or(""), 160),
            "draft.save" | "list.draft_save" => clip(args["subject"].as_str().unwrap_or(""), 80),
            _ => String::new(),
        },
    };
    let status = if tool == "task.report" { s(&args["status"]) } else { String::new() };
    (contact, target, detail, status)
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, i64>(0)?,
        "session": r.get::<_, String>(1)?,
        "tool": r.get::<_, String>(2)?,
        "contact": r.get::<_, String>(3)?,
        "target": r.get::<_, String>(4)?,
        "detail": r.get::<_, String>(5)?,
        "status": r.get::<_, String>(6)?,
        "ok": r.get::<_, bool>(7)?,
        "at": r.get::<_, i64>(8)?,
    }))
}
const COLUMNS: &str = "id,session,tool,contact,target,detail,status,ok,at";

impl App {
    pub fn record_activity(&self, root: &str, session: &str, tool: &str, args: &Value, result: Result<&Value, &str>) {
        let (contact, target, detail, status) = fields(tool, args, result);
        let progress = if tool == "task.report" && args["total"].as_u64().is_some() {
            json!({"done": args["done"].as_u64().unwrap_or(0), "total": args["total"]})
        } else {
            Value::Null
        };
        let at = gc_core::now_millis();
        let Ok(conn) = self.db.conn() else { return };
        if conn
            .execute(
                "INSERT INTO agent_activity(workspace,session,tool,contact,target,detail,status,ok,at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![root, session, tool, contact, target, detail, status, result.is_ok(), at],
            )
            .is_err()
        {
            return;
        }
        let id = conn.last_insert_rowid();
        (self.notify)(
            "agent-activity",
            json!({"root":root,"id":id,"session":session,"tool":tool,"contact":contact,"target":target,"detail":detail,"status":status,"progress":progress,"ok":result.is_ok(),"at":at}),
        );
    }

    /// Recent calls, oldest first; one session or the whole workspace.
    pub fn activity_list(&self, root: &str, session: Option<&str>, limit: i64) -> Result<Value, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM (SELECT * FROM agent_activity WHERE workspace=?1 AND (?2 IS NULL OR session=?2) ORDER BY id DESC LIMIT ?3) ORDER BY id"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![root, session, limit.clamp(1, 500)], row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(json!(rows))
    }

    /// The latest task.report per session, for the session list.
    pub fn task_statuses(&self, root: &str) -> Result<std::collections::HashMap<String, Value>, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM agent_activity WHERE id IN (SELECT MAX(id) FROM agent_activity WHERE workspace=?1 AND tool='task.report' AND ok=1 GROUP BY session)"
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![root], row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|r| (r["session"].as_str().unwrap_or("").to_owned(), r)).collect())
    }
}

/// The text between `<!-- agent:<section> -->` markers in a dossier, if any.
pub fn dossier_section(text: &str, section: &str) -> String {
    let (start, end) = (format!("<!-- agent:{section} -->"), format!("<!-- /agent:{section} -->"));
    text.find(&start)
        .and_then(|a| {
            let from = a + start.len();
            text[from..].find(&end).map(|b| text[from..from + b].trim().to_owned())
        })
        .unwrap_or_default()
}

impl App {
    /// The person's latest Agent analysis (dossier section) and when and by which session it was written.
    pub async fn agent_analysis(&self, root: &str, slug: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let path = format!("people/{slug}.md");
        let text = if service.docs.workspace.stamp(&path).map_err(|e| e.to_string())?.is_some() {
            dossier_section(&service.read_note(&path).await?, "analysis")
        } else {
            String::new()
        };
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let written: Option<(i64, String)> = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT at,session FROM agent_activity WHERE workspace=?1 AND tool='dossier.write_section' AND contact=?2 AND ok=1 ORDER BY id DESC LIMIT 1",
            params![root, slug],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))
        .map_err(|e| e.to_string())?;
        Ok(json!({"path":path,"text":text,"at":written.as_ref().map(|w| w.0),"session":written.map(|w| w.1)}))
    }

    /// Recent task outcomes (done / needs_input / blocked) across sessions, newest first, with the
    /// person each session last worked on.
    pub fn agent_recent(&self, root: &str, limit: i64) -> Result<Value, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT a.id,a.session,a.status,a.detail,a.at,
                    (SELECT contact FROM agent_activity b WHERE b.workspace=a.workspace AND b.session=a.session AND b.contact!='' AND b.id<=a.id ORDER BY b.id DESC LIMIT 1)
                 FROM agent_activity a
                 WHERE a.id IN (SELECT MAX(id) FROM agent_activity WHERE workspace=?1 AND tool='task.report' AND ok=1 GROUP BY session)
                   AND a.status IN ('done','needs_input','blocked') AND a.at>=?2
                 ORDER BY a.id DESC LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;
        let week = gc_core::now_millis() - 7 * 86_400_000;
        let rows = stmt
            .query_map(params![root, week, limit.clamp(1, 50)], |r| {
                Ok(json!({"id":r.get::<_,i64>(0)?,"session":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"summary":r.get::<_,String>(3)?,"at":r.get::<_,i64>(4)?,"contact":r.get::<_,Option<String>>(5)?}))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(json!(rows))
    }
}
