use crate::{app::App, contacts::TABLE};
use serde_json::{Value, json};
fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}
fn string() -> Value {
    json!({"type":"string"})
}
fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max { format!("{}…", flat.chars().take(max).collect::<String>()) } else { flat }
}
/// The memory line for a write tool, with the contact it concerns.
fn change_entry(name: &str, args: &Value, result: &Value) -> Option<(String, String)> {
    let slug = |k: &str| args[k].as_str().map(str::to_owned);
    match name {
        "dossier.write_section" => Some((slug("slug")?, format!("Rewrote dossier section “{}”: {}", args["section"].as_str().unwrap_or(""), clip(args["content"].as_str().unwrap_or(""), 200)))),
        "contact.update" => Some((
            slug("slug")?,
            format!(
                "Updated profile: {}",
                args["values"].as_object()?.iter().map(|(k, v)| format!("{k}={}", clip(v.as_str().unwrap_or(&v.to_string()), 120))).collect::<Vec<_>>().join("; ")
            ),
        )),
        "channel.add" => Some((slug("slug")?, format!("Added unverified channel {}: {} (source {})", args["kind"].as_str().unwrap_or(""), args["value"].as_str().unwrap_or(""), args["source"].as_str().unwrap_or("")))),
        "draft.save" => Some((slug("contact_id")?, format!("Saved draft for review {} “{}”", result["path"].as_str().unwrap_or(""), clip(args["subject"].as_str().unwrap_or(""), 80)))),
        "list.draft_save" => Some((result["contact"].as_str()?.to_owned(), format!("Saved draft for review for list {}: {}", args["list"].as_str().unwrap_or(""), result["path"].as_str().unwrap_or("")))),
        "contact.follow_up" => Some((slug("slug")?, match result["follow_up"].as_str() { Some(d) => format!("Set follow-up date {d}"), None => "Clear follow-up date".into() })),
        _ => None,
    }
}
impl App {
    pub fn table_catalog(&self) -> Result<Value, String> {
        let bytes = std::fs::read(self.resources.join("kits/table/commands.json"))
            .map_err(|e| format!("Table command catalog missing: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
    pub async fn mcp(&self, input: Value) -> Result<Value, String> {
        let root = input["workspace"].as_str().ok_or("workspace required")?;
        let service = self.service(root)?;
        let catalog = self.table_catalog()?;
        if input["method"] == "tools/list" {
            let mut tools = vec![
                tool(
                    "context.get",
                    "Read the current UI contact and document for this workspace. For a NEW request about 'this person', resolve once and pin the slug for that task; do not retarget work already in progress.",
                    json!({}),
                    &[],
                ),
                tool(
                    "mail.accounts",
                    "List connected incoming mailboxes and sync status; no credentials. Sync applies to a whole configured mailbox, not one person.",
                    json!({}),
                    &[],
                ),
                tool(
                    "mail.sync",
                    "Sync a connected incoming mailbox when the user requests it, then match messages to contacts. Never sends mail. Use mail.accounts for the id; clarify which account if ambiguous.",
                    json!({"id":string()}),
                    &["id"],
                ),
                tool(
                    "document.list",
                    "List workspace Markdown documents and evidence paths",
                    json!({}),
                    &[],
                ),
                tool(
                    "document.read",
                    "Read a workspace Markdown document, including live editor contents",
                    json!({"path":string()}),
                    &["path"],
                ),
                tool(
                    "document.create",
                    "Create a new Markdown report in notes/ or campaign brief in campaigns/",
                    json!({"path":string(),"text":string()}),
                    &["path", "text"],
                ),
                tool(
                    "document.replace",
                    "Replace unique exact text in notes/ or campaigns/ through the live editor. Read first; rejects stale or ambiguous text. For dossiers use dossier.write_section, for drafts use draft.save.",
                    json!({"path":string(),"expected":string(),"text":string()}),
                    &["path", "expected", "text"],
                ),
                tool(
                    "contact.get",
                    "Contact, channels, dossier and drafts",
                    json!({"slug":string()}),
                    &["slug"],
                ),
                tool(
                    "contact.search",
                    "Search names, affiliations, campaigns and stages",
                    json!({"query":string()}),
                    &[],
                ),
                tool(
                    "source.facts",
                    "Facts about a contact from connected data sources (GCSP): plan, usage and other declared attributes, with source name and sync time. These are sourced facts; source events appear in timeline.get. Pending identity suggestions are marked match=pending and are NOT confirmed.",
                    json!({"slug":string()}),
                    &["slug"],
                ),
                tool(
                    "source.people",
                    "Search everyone from connected data sources, including people who are not contacts yet: name, emails, match state (auto/confirmed = linked to a contact, pending, unlinked), linked contact slug and declared attributes (e.g. plan, credits). query matches names, emails and attribute values; match can be linked, pending or unlinked. Use it to find users worth understanding or contacting.",
                    json!({"source":string(),"query":string(),"match":{"enum":["linked","pending","unlinked"]},"limit":{"type":"integer","minimum":1,"maximum":200}}),
                    &[],
                ),
                tool(
                    "source.person",
                    "One person from a data source (linked or not): labeled facts, how attributes changed over time, and their events (signup, purchases, failures, feedback…) newest first with evidence paths readable by document.read. Facts come from the source and outrank inference.",
                    json!({"source":string(),"external_id":string()}),
                    &["source", "external_id"],
                ),
                tool(
                    "source.list",
                    "List connected data sources with sync status and counts of linked, pending and unlinked people. No credentials.",
                    json!({}),
                    &[],
                ),
                tool(
                    "source.sync",
                    "Pull new people and events from a connected data source when the user asks. Read-only toward the source; never links identities on its own beyond verified-email matches.",
                    json!({"id":string()}),
                    &["id"],
                ),
                tool(
                    "timeline.get",
                    "Recent interactions including source document bodies",
                    json!({"slug":string(),"limit":{"type":"integer","minimum":1,"maximum":100}}),
                    &["slug"],
                ),
                tool(
                    "campaign.get",
                    "Read a campaign brief",
                    json!({"name":string()}),
                    &["name"],
                ),
                tool(
                    "dossier.write_section",
                    "Replace only an agent-owned dossier section",
                    json!({"slug":string(),"section":{"enum":["analysis","channels"]},"content":string()}),
                    &["slug", "section", "content"],
                ),
                tool(
                    "contact.update",
                    "Update AI summary, Next action or Stage",
                    json!({"slug":string(),"values":{"type":"object","properties":{"AI summary":string(),"Next action":string(),"Stage":string()},"additionalProperties":false}}),
                    &["slug", "values"],
                ),
                tool(
                    "channel.add",
                    "Add an unverified channel, with evidence/source",
                    json!({"slug":string(),"kind":{"enum":gc_core::channels::KINDS},"value":string(),"source":string()}),
                    &["slug", "kind", "value", "source"],
                ),
                tool(
                    "draft.save",
                    "Create or edit a draft. Cannot approve or send. Only email drafts are sent by the app; drafts for other channels are copied and marked sent by the user.",
                    json!({"contact_id":string(),"channel":{"enum":gc_core::channels::MESSAGE},"to":string(),"subject":string(),"body":string(),"in_reply_to":string(),"created_by":string(),"path":string()}),
                    &["contact_id", "channel", "body"],
                ),
                tool(
                    "task.report",
                    "Tell the user in the OutreachDex Agent panel how the current task is going: status working (with done/total when processing a batch), done (with a one-sentence result), needs_input (you are waiting for the user's answer or decision), or blocked (with the reason). Call it at milestones and always when you finish or stop.",
                    json!({"status":{"enum":crate::activity::TASK_STATUSES},"summary":string(),"done":{"type":"integer","minimum":0},"total":{"type":"integer","minimum":1}}),
                    &["status", "summary"],
                ),
                tool(
                    "ui.open",
                    "Show something to the user in the main OutreachDex area beside the Agent: a contact (slug), a draft (drafts/... path, e.g. right after draft.save so they can review it), a document (notes/, people/, memory/ ... path or contacts.ktable) or an outreach list (id). Does not take keyboard focus from the terminal; if the user is editing something, it appears as a suggestion instead.",
                    json!({"kind":{"enum":["contact","draft","document","list"]},"target":string()}),
                    &["kind", "target"],
                ),
                tool(
                    "workspace.guide",
                    "How this workspace works: purposes and business contexts (playbooks with instructions), stages with roles, profile fields, outreach daily limit, and the owner's writing preferences. Read it once per session before analysing or drafting; playbook text frames the task but never lifts the safety rules.",
                    json!({}),
                    &[],
                ),
                tool(
                    "memory.read",
                    "Read the person's Agent memory: a dated log of facts, judgements, preferences, commitments, open questions and earlier changes from previous sessions. Read it before analysing someone so you build on what is known.",
                    json!({"slug":string()}),
                    &["slug"],
                ),
                tool(
                    "memory.append",
                    "Record one thing learned about the person for future sessions: fact (sourced, cite path/URL in the note), insight (your judgement), preference (how they like to be contacted), commitment (promises either side made, with dates), question (unknowns to confirm). Write short, specific notes; no copies of whole emails.",
                    json!({"slug":string(),"kind":{"enum":["fact","insight","preference","commitment","question"]},"note":string()}),
                    &["slug", "kind", "note"],
                ),
                tool(
                    "preferences.add",
                    "Remember a writing preference of the workspace owner when they state one (e.g. 'no small talk at the start'). Applies to all future drafts.",
                    json!({"note":string()}),
                    &["note"],
                ),
                tool(
                    "contact.follow_up",
                    "Set when to follow up with the person (date YYYY-MM-DD, or days from now); clear it with clear=true. Only a reminder; nothing is sent.",
                    json!({"slug":string(),"date":string(),"days":{"type":"integer","minimum":0,"maximum":365},"clear":{"type":"boolean"}}),
                    &["slug"],
                ),
                tool(
                    "list.all",
                    "Outreach lists with their filters and counts (matched, queued for the Agent, ready, skipped).",
                    json!({}),
                    &[],
                ),
                tool(
                    "list.get",
                    "One outreach list: description, channel, playbook and members with key, name, why they were selected (evidence), linked contact and status. Members with status=queued are waiting for you to prepare.",
                    json!({"id":string()}),
                    &["id"],
                ),
                tool(
                    "list.draft_save",
                    "Save the reviewed-later draft for one list member (creates their contact from the source if needed and tags the draft with the list). Use only after checking the member's evidence with source.facts/timeline.get. Never sends.",
                    json!({"list":string(),"member":string(),"subject":string(),"body":string(),"to":string(),"channel":{"enum":gc_core::channels::MESSAGE}}),
                    &["list", "member", "body"],
                ),
                tool(
                    "list.decline",
                    "Mark a list member as not worth contacting now, with the reason shown to the user.",
                    json!({"list":string(),"member":string(),"reason":string()}),
                    &["list", "member", "reason"],
                ),
                tool(
                    "table_model_describe",
                    "Describe the supplier model schema",
                    json!({}),
                    &[],
                ),
                tool(
                    "table_document_check",
                    "Check the table document",
                    json!({}),
                    &[],
                ),
            ];
            for command in catalog["commands"]
                .as_array()
                .ok_or("invalid commands catalog")?
            {
                tools.push(json!({"name":command["tool"],"description":command["title"],"inputSchema":command["inputSchema"],"outputSchema":command["outputSchema"]}));
            }
            return Ok(json!({"tools":tools}));
        }
        if input["method"] != "tools/call" {
            return Err("Unsupported method".into());
        }
        let name = input["params"]["name"]
            .as_str()
            .ok_or("tool name required")?;
        let args = &input["params"]["arguments"];
        let session = input["session"].as_str().unwrap_or("");
        let outcome = self.mcp_tool(root, session, name, args, &service, &catalog).await;
        // Every call, including refusals, appears in the Agent panel's activity for its session.
        self.record_activity(root, session, name, args, outcome.as_ref().map_err(String::as_str));
        let result = outcome?;
        Ok(
            json!({"content":[{"type":"text","text":result.to_string()}],"structuredContent":if result.is_object(){result.clone()}else{json!({"result":result})}}),
        )
    }

    async fn mcp_tool(
        &self,
        root: &str,
        session: &str,
        name: &str,
        args: &Value,
        service: &crate::Service,
        catalog: &Value,
    ) -> Result<Value, String> {
        let text = |key: &str| args[key].as_str().ok_or_else(|| format!("{key} required"));
        let result = match name {
            "context.get" => self.current_context(root).await?,
            "mail.accounts" => {
                json!({"imap":self.mail(root)?.list(crate::app::user_id(root)).await.map_err(|e| e.message())?})
            }
            "mail.sync" => self.call(root, "mail.sync", args.clone()).await?,
            "task.report" => {
                let status = text("status")?;
                if !crate::activity::TASK_STATUSES.contains(&status) {
                    return Err("status must be working, done, needs_input or blocked".into());
                }
                if text("summary")?.trim().is_empty() {
                    return Err("summary required".into());
                }
                json!({"ok":true})
            }
            "ui.open" => {
                let (kind, target) = (text("kind")?, text("target")?);
                match kind {
                    "contact" => {
                        service.contact(target).await?;
                    }
                    "draft" => {
                        if !target.starts_with("drafts/") || service.docs.workspace.stamp(target).map_err(|e| e.to_string())?.is_none() {
                            return Err("draft not found; use the path returned by draft.save".into());
                        }
                    }
                    "document" => {
                        if target != "contacts.ktable" {
                            crate::Service::document_path(target, false)?;
                            if service.docs.workspace.stamp(target).map_err(|e| e.to_string())?.is_none() {
                                return Err("document not found".into());
                            }
                        }
                    }
                    "list" => {
                        self.list_get(root, target).await?;
                    }
                    _ => return Err("kind must be contact, draft, document or list".into()),
                }
                (self.notify)("agent-open", json!({"root":root,"session":session,"kind":kind,"target":target}));
                json!({"ok":true,"note":"Shown beside the terminal, or offered as a suggestion if the user is busy editing."})
            }
            "workspace.guide" => {
                let config = service.workspace_config()?;
                let playbooks = gc_core::config::playbooks(&service.docs.workspace).map_err(|e| e.to_string())?;
                json!({
                    "playbooks": playbooks.iter().map(|p| json!({"id":p.id,"label":p.label,"kind":p.kind,"creates_drafts":p.creates_drafts,"instructions":p.instructions})).collect::<Vec<_>>(),
                    "stages": serde_json::to_value(&config.stages).map_err(|e| e.to_string())?,
                    "outreach_daily_limit": config.outreach.daily_limit,
                    "fields": service.contact_fields().await?,
                    "writing_preferences": service.preferences(),
                })
            }
            "memory.read" => service.memory_read(text("slug")?).await?,
            "memory.append" => service.memory_append(text("slug")?, text("kind")?, text("note")?, "Agent").await?,
            "preferences.add" => {
                service.preferences_add(text("note")?)?;
                json!({"ok":true})
            }
            "contact.follow_up" => {
                let at = if args["clear"] == true {
                    None
                } else if let Some(date) = args["date"].as_str() {
                    Some(
                        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                            .map_err(|_| "date must be YYYY-MM-DD")?
                            .and_hms_opt(9, 0, 0)
                            .ok_or("invalid date")?
                            .and_utc(),
                    )
                } else {
                    let days = args["days"].as_i64().ok_or("date, days or clear required")?;
                    Some(chrono::Utc::now() + chrono::Duration::days(days))
                };
                service.set_follow_up(text("slug")?, at).await?;
                json!({"ok":true,"follow_up":at.map(|t| t.format("%Y-%m-%d").to_string())})
            }
            "list.all" => self.lists_all(root).await?,
            "list.get" => self.list_get(root, text("id")?).await?,
            "list.draft_save" => self.list_draft_save(root, args).await?,
            "list.decline" => self.list_decline(root, text("list")?, text("member")?, text("reason")?).await?,
            "source.people" => self.source_search(root, args).await?,
            "source.person" => self.source_identity(root, text("source")?, text("external_id")?).await?,
            "source.facts" => self.source_person(root, text("slug")?).await?,
            "source.list" => {
                // Agents see names, status and counts, never the endpoint credentials.
                let mut list = self.source_list(root).await?;
                for item in list.as_array_mut().into_iter().flatten() {
                    if let Some(map) = item.as_object_mut() {
                        map.remove("header");
                    }
                }
                list
            }
            "source.sync" => self.call(root, "source.sync", args.clone()).await?,
            "document.list" => json!({"paths":service.documents()?}),
            "document.read" => {
                crate::Service::document_path(text("path")?, false)?;
                json!({"path":text("path")?,"text":service.read_note(text("path")?).await?})
            }
            "document.create" => {
                let path = text("path")?;
                crate::Service::document_path(path, true)?;
                service
                    .docs
                    .workspace
                    .put_new(path, text("text")?.as_bytes())
                    .map_err(|e| e.to_string())?;
                json!({"path":path})
            }
            "document.replace" => {
                service
                    .document_replace(text("path")?, text("expected")?, text("text")?)
                    .await?;
                json!({"path":text("path")?,"ok":true})
            }
            "contact.get" => service.detail(text("slug")?).await?,
            "contact.search" => {
                let query = args["query"].as_str().unwrap_or("").to_lowercase();
                json!(
                    service
                        .rows("contacts")
                        .await?
                        .into_iter()
                        .filter(|r| ["Name", "Affiliation", "Campaign", "Stage"]
                            .iter()
                            .any(|k| r.text(k).to_lowercase().contains(&query)))
                        .collect::<Vec<_>>()
                )
            }
            "timeline.get" => {
                let detail = service.detail(text("slug")?).await?;
                let mut rows = detail["timeline"].as_array().cloned().unwrap_or_default();
                rows.truncate(args["limit"].as_u64().unwrap_or(30).clamp(1, 100) as usize);
                for row in &mut rows {
                    if let Some(path) = row["values"]["File"]
                        .as_str()
                        .filter(|s| s.ends_with(".md"))
                    {
                        row["body"] = json!(service.read_note(path).await?);
                    }
                }
                json!(rows)
            }
            "campaign.get" => {
                let name = text("name")?;
                if name.contains(['/', '\\', '.']) {
                    return Err("Invalid campaign name".into());
                }
                json!({"text":service.read_note(&format!("campaigns/{name}.md")).await?})
            }
            "dossier.write_section" => {
                service
                    .write_section(text("slug")?, text("section")?, text("content")?)
                    .await?;
                json!({"ok":true})
            }
            "contact.update" => {
                service
                    .update_contact(
                        text("slug")?,
                        args["values"]
                            .as_object()
                            .cloned()
                            .ok_or("values required")?,
                        true,
                    )
                    .await?;
                json!({"ok":true})
            }
            "channel.add" => {
                json!({"created":service.add_channel(text("slug")?,text("kind")?,text("value")?,text("source")?,false).await?})
            }
            "draft.save" => service.draft_save(args, true).await?,
            "table_model_describe" | "table_document_check" => {
                service
                    .docs
                    .headless_method(
                        TABLE,
                        if name == "table_model_describe" {
                            "model.describe"
                        } else {
                            "document.check"
                        },
                        self.resources.join("kits/kit-table").as_path(),
                    )
                    .await?
            }
            _ => {
                let command = catalog["commands"]
                    .as_array()
                    .and_then(|a| a.iter().find(|c| c["tool"] == name))
                    .ok_or("Unknown tool; sending and approval are not exposed to agents.")?;
                service
                    .docs
                    .headless(
                        TABLE,
                        command["command"].as_str().ok_or("command missing")?,
                        args.clone(),
                        &self.resources.join("kits/kit-table"),
                    )
                    .await?
            }
        };
        // The person's memory gets a line for every change made for them, so later sessions
        // (and the user) can see what the Agent already did.
        if let Some((slug, summary)) = change_entry(name, args, &result) {
            service.memory_append(&slug, "change", &summary, "Agent").await?;
        }
        if matches!(
            name,
            "document.create"
                | "document.replace"
                | "dossier.write_section"
                | "contact.update"
                | "channel.add"
                | "draft.save"
                | "memory.append"
                | "preferences.add"
                | "contact.follow_up"
                | "list.draft_save"
                | "list.decline"
        ) || catalog["commands"]
            .as_array()
            .is_some_and(|commands| commands.iter().any(|c| c["tool"] == name))
        {
            (self.notify)("workspace-changed", json!({"root":root}));
        }
        Ok(result)
    }
}
