use crate::{Service, contacts::now};
use serde_json::{Value, json};
use std::path::Path;

pub fn frontmatter(meta: &Value, body: &str) -> Result<String, String> {
    let mut output = String::from("---\n");
    for (key, value) in meta.as_object().ok_or("metadata must be an object")? {
        output.push_str(&format!(
            "{key}: {}\n",
            serde_json::to_string(value).map_err(|e| e.to_string())?
        ));
    }
    Ok(format!("{output}---\n\n{body}"))
}
pub fn metadata(text: &str) -> Value {
    let mut result = serde_json::Map::new();
    if let Some(rest) = text.strip_prefix("---\n")
        && let Some((head, _)) = rest.split_once("\n---")
    {
        for line in head.lines() {
            if let Some((key, value)) = line.split_once(':') {
                result.insert(
                    key.trim().into(),
                    serde_json::from_str(value.trim()).unwrap_or(json!(value.trim())),
                );
            }
        }
    }
    Value::Object(result)
}
pub fn body(text: &str) -> &str {
    text.strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .map(|(_, s)| s.trim_start_matches('\n'))
        .unwrap_or(text)
}
impl Service {
    pub fn document_path(path: &str, writable: bool) -> Result<(), String> {
        let folders = if writable {
            &["notes", "campaigns"][..]
        } else {
            &["notes", "campaigns", "people", "memory", "intake", "mail", "drafts", "sources"][..]
        };
        if !path.ends_with(".md")
            || path.contains('\\')
            || !folders.contains(&path.split('/').next().unwrap_or(""))
            || path.split('/').any(|p| p.is_empty() || p.starts_with('.'))
        {
            return Err(if writable { "General editing only supports Markdown under notes/ and campaigns/. Use the dedicated tools for contact dossiers and drafts." } else { "Choose a Markdown document in the workspace." }.into());
        }
        Ok(())
    }
    pub fn documents(&self) -> Result<Vec<String>, String> {
        let mut result = Vec::new();
        for folder in ["notes", "campaigns", "people", "memory", "intake", "mail", "drafts"] {
            let root = self
                .docs
                .workspace
                .file(folder)
                .map_err(|e| e.to_string())?;
            let mut files = Vec::new();
            collect_markdown(&root, &mut files)?;
            for file in files {
                let relative = file
                    .strip_prefix(self.docs.workspace.root())
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned();
                if Self::document_path(&relative, false).is_ok() {
                    result.push(relative);
                }
            }
        }
        result.sort();
        Ok(result)
    }
    pub async fn document_replace(
        &self,
        path: &str,
        expected: &str,
        replacement: &str,
    ) -> Result<(), String> {
        Self::document_path(path, true)?;
        if expected.is_empty() {
            return Err("Provide the original text to replace, so others' changes aren't overwritten.".into());
        }
        let _guard = self.mutations.lock().await;
        let text = self.read_note(path).await?;
        let mut occurrences = text.match_indices(expected);
        let (start, _) = occurrences.next().ok_or("The original text has changed. Read the document again.")?;
        if occurrences.next().is_some() {
            return Err("The original text appears more than once. Provide more context.".into());
        }
        self.docs.execute(path,"text.replace",json!({"from":text[..start].chars().count(),"to":text[..start+expected.len()].chars().count(),"unit":"scalar","expected":expected,"text":replacement}),"system:agent").await?;
        Ok(())
    }
    pub async fn read_note(&self, path: &str) -> Result<String, String> {
        let mut result = self
            .docs
            .execute(path, "text.read", json!({}), "system:notes")
            .await;
        // A read may race the UI's saved-document unmount. Retry only this
        // read-only operation; never replay a possibly applied mutation.
        if result
            .as_ref()
            .err()
            .is_some_and(|e| e == "The editor was closed. Try again.")
        {
            result = self
                .docs
                .execute(path, "text.read", json!({}), "system:notes")
                .await;
        }
        let answer = result?;
        answer["text"]
            .as_str()
            .or_else(|| answer["result"]["text"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| format!("invalid note text response: {answer}"))
    }
    pub async fn ensure_dossier(
        &self,
        slug: &str,
        name: &str,
        background: &str,
        hook: &str,
        campaign: &str,
    ) -> Result<(), String> {
        let path = format!("people/{slug}.md");
        if self
            .docs
            .workspace
            .stamp(&path)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Ok(());
        }
        let text = frontmatter(
            &json!({"contact_id":slug,"campaign":campaign}),
            &format!(
                "# {name}\n\n## Background\n{background}\n\n## Hook\n{hook}\n\n## AI analysis\n<!-- agent:analysis -->\n\n<!-- /agent:analysis -->\n\n## Channel suggestions\n<!-- agent:channels -->\n\n<!-- /agent:channels -->\n\n## My notes\n"
            ),
        )?;
        self.docs
            .workspace
            .put_new(&path, text.as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn write_section(
        &self,
        slug: &str,
        section: &str,
        content: &str,
    ) -> Result<(), String> {
        if !["analysis", "channels"].contains(&section) {
            return Err("The Agent can only change the AI analysis and channel suggestions.".into());
        }
        self.contact(slug).await?;
        let path = format!("people/{slug}.md");
        let text = self.read_note(&path).await?;
        let start_marker = format!("<!-- agent:{section} -->");
        let end_marker = format!("<!-- /agent:{section} -->");
        let start = text.find(&start_marker).ok_or("The dossier is missing the Agent section marker.")? + start_marker.len();
        let end = start
            + text[start..]
                .find(&end_marker)
                .ok_or("The dossier is missing the Agent section end marker.")?;
        if content.contains("<!-- agent:") || content.contains("<!-- /agent:") {
            return Err("Section content can't contain Agent boundary markers.".into());
        }
        self.docs.execute(&path,"text.replace",json!({"from":text[..start].chars().count(),"to":text[..end].chars().count(),"unit":"scalar","expected":&text[start..end],"text":format!("\n{}\n",content.trim())}),"system:agent").await?;
        Ok(())
    }
    pub async fn draft_save(&self, args: &Value, agent: bool) -> Result<Value, String> {
        let slug = args["contact_id"].as_str().ok_or("contact_id required")?;
        let contact = self.contact(slug).await?;
        let channel = args["channel"].as_str().unwrap_or("email");
        if !gc_core::channels::is_message(channel) {
            return Err("Invalid channel.".into());
        }
        let existing = args["path"].as_str();
        let path = existing.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "drafts/{slug}/{}-{channel}-{}.md",
                chrono::Utc::now().format("%Y%m%d-%H%M%S"),
                &uuid::Uuid::new_v4().to_string()[..8]
            )
        });
        if !path.starts_with(&format!("drafts/{slug}/")) || !path.ends_with(".md") {
            return Err("The draft path doesn't match the contact.".into());
        }
        let content = args["body"].as_str().ok_or("body required")?;
        let mut meta = json!({"contact_id":slug,"channel":channel,"to":args["to"].as_str().unwrap_or(contact.text("Email")),"subject":args["subject"].as_str().unwrap_or(""),"status":"draft","created_by":args["created_by"].as_str().unwrap_or(if agent{"agent"}else{"me"}),"created_at":now(),"in_reply_to":args["in_reply_to"].as_str().unwrap_or("")});
        if self
            .docs
            .workspace
            .stamp(&path)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            let old = self.read_note(&path).await?;
            let old_meta = metadata(&old);
            if old_meta["status"] == "sent"
                || old_meta["status"] == "sending"
                || old_meta["status"] == "uncertain"
            {
                return Err("Drafts that were sent, or whose send status is uncertain, can't be rewritten.".into());
            }
            if agent && old_meta["status"] != "draft" {
                return Err("The Agent can't edit an approved draft.".into());
            }
            meta["created_at"] = old_meta["created_at"].clone();
            // The agent's first wording stays available for comparison after a person edits it.
            if old_meta["ai_original"].is_string() {
                meta["ai_original"] = old_meta["ai_original"].clone();
            } else if agent {
                meta["ai_original"] = json!(content);
            }
            // The outreach list is host-managed: editing a draft keeps it (and its daily limit).
            if old_meta["list"].as_str().is_some_and(|l| !l.is_empty()) {
                meta["list"] = old_meta["list"].clone();
            }
            let updated = frontmatter(&meta, content)?;
            self.docs
                .execute(
                    &path,
                    "text.replace",
                    json!({"from":0,"to":old.chars().count(),"expected":old,"text":updated}),
                    if agent {
                        "system:agent"
                    } else {
                        "system:drafts"
                    },
                )
                .await?;
        } else {
            if agent {
                meta["ai_original"] = json!(content);
            }
            self.docs
                .workspace
                .put_new(&path, frontmatter(&meta, content)?.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        Ok(json!({"path":path,"metadata":meta,"body":content}))
    }
    pub async fn drafts(&self, slug: Option<&str>) -> Result<Vec<Value>, String> {
        let root = self
            .docs
            .workspace
            .file("drafts")
            .map_err(|e| e.to_string())?;
        let mut paths = Vec::new();
        collect_markdown(&root, &mut paths)?;
        let mut result = Vec::new();
        for path in paths {
            let relative = path
                .strip_prefix(self.docs.workspace.root())
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned();
            let text = self.read_note(&relative).await?;
            let meta = metadata(&text);
            if slug.is_none_or(|s| meta["contact_id"] == s) {
                result.push(json!({"path":relative,"metadata":meta,"body":body(&text)}));
            }
        }
        result.sort_by(|a, b| b["path"].as_str().cmp(&a["path"].as_str()));
        Ok(result)
    }
    pub async fn draft_status(&self, path: &str, status: &str) -> Result<(), String> {
        if !path.starts_with("drafts/")
            || !path.ends_with(".md")
            || !["approved", "discarded", "draft"].contains(&status)
        {
            return Err("Invalid draft action.".into());
        }
        let text = self.read_note(path).await?;
        let meta = metadata(&text);
        if ["sent", "sending", "uncertain"].contains(&meta["status"].as_str().unwrap_or("")) {
            return Err("This draft was sent or its send status is uncertain; it can't be changed.".into());
        }
        self.docs
            .execute(
                path,
                "property.set",
                json!({"key":"status","value":status}),
                "system:drafts",
            )
            .await?;
        Ok(())
    }
    pub async fn intake(
        &self,
        slug: &str,
        channel: &str,
        title: &str,
        content: &str,
    ) -> Result<String, String> {
        self.contact(slug).await?;
        if content.trim().is_empty() {
            return Err("Paste a link or text.".into());
        }
        let path = format!(
            "intake/{slug}/{}-{}.md",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            uuid::Uuid::new_v4()
        );
        self.docs
            .workspace
            .put_new(
                &path,
                frontmatter(
                    &json!({"contact_id":slug,"channel":channel,"created_at":now()}),
                    &format!("# {title}\n\n{content}\n"),
                )?
                .as_bytes(),
            )
            .map_err(|e| e.to_string())?;
        self.interaction(slug, channel, "intake", "note", title, &path, "", &now())
            .await?;
        Ok(path)
    }
}
fn collect_markdown(root: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_markdown(&entry.path(), out)?;
        } else if entry.path().extension().is_some_and(|e| e == "md") {
            out.push(entry.path());
        }
    }
    Ok(())
}
