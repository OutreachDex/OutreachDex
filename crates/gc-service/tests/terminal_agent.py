#!/usr/bin/env python3
"""Interactive PTY fixture; uses the real bundled MCP transport and desktop owner."""
import json
import os
import subprocess
import sys
import re

def tool(name, args):
    print("$ MCP " + name + " " + json.dumps(args, ensure_ascii=False), flush=True)
    process = subprocess.Popen(
        [os.environ["GC_TEST_DESKTOP_BINARY"], "mcp", "--workspace", os.getcwd()],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    frames = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": name, "arguments": args}},
    ]
    out, error = process.communicate("".join(json.dumps(frame) + "\n" for frame in frames), timeout=30)
    if process.returncode:
        raise RuntimeError(error)
    response = json.loads(out.splitlines()[-1])
    if response.get("error") or response.get("result", {}).get("isError"):
        raise RuntimeError(str(response))
    print("MCP OK " + name, flush=True)
    return response["result"].get("structuredContent")

prompt = sys.argv[-1]
match = re.search(r'pinned to contact slug=("[^"\n]+")', prompt)
task_slug = json.loads(match.group(1)) if match else None
if task_slug:
    print("TASK READY " + task_slug + (" reply" if "draft.save" in prompt else " overview"), flush=True)
list_match = re.search(r'outreach list id=("[^"\n]+")', prompt)
list_id = json.loads(list_match.group(1)) if list_match else None
if list_id:
    print("LIST READY " + list_id, flush=True)
if "The user adds" in prompt:
    print("NOTE RECEIVED", flush=True)
source_match = re.search(r'pinned to source user source=("[^"\n]+") external_id=("[^"\n]+")', prompt)
source_person = (json.loads(source_match.group(1)), json.loads(source_match.group(2))) if source_match else None
if source_person:
    print("SOURCE READY " + source_person[1], flush=True)
if "Synchronize all connected" in prompt:
    print("SYNC READY", flush=True)
print("\033[32mOutreachDex terminal fixture READY\033[0m", flush=True)
awaiting_confirmation = False
for line in sys.stdin:
    line = line.strip()
    try:
        if awaiting_confirmation and not line:
            awaiting_confirmation = False
            print("ENTER CONFIRMED", flush=True)
        elif line == "permission":
            awaiting_confirmation = True
            print("Allow synthetic test action? Press Enter to confirm", flush=True)
        elif line == "size":
            print("PTY COLUMNS " + str(os.get_terminal_size().columns), flush=True)
        elif line == "context":
            print("CONTEXT " + json.dumps(tool("context.get", {}), ensure_ascii=False), flush=True)
        elif line == "task" and task_slug:
            tool("memory.read", {"slug": task_slug})
            tool("contact.get", {"slug": task_slug})
            tool("timeline.get", {"slug": task_slug})
            if "draft.save" in prompt:
                saved = tool("draft.save", {"contact_id": task_slug, "channel": "email", "to": "fixture@example.test", "subject": "Fixture reply", "body": "Synthetic task draft. No sending."})
                tool("ui.open", {"kind": "draft", "target": saved["path"]})
                tool("task.report", {"status": "needs_input", "summary": "Draft opened for review"})
            else:
                tool("dossier.write_section", {"slug": task_slug, "section": "analysis", "content": "## Conclusion\nCares about the spreadsheet import format.\n\n| Item | Status |\n|---|---|\n| Import | Format to confirm |\n\n## Evidence\n- Feedback record [source](people/" + task_slug + ".md)\n\n## Open questions\n- Which spreadsheet formats are supported"})
                tool("memory.append", {"slug": task_slug, "kind": "fact", "note": "Terminal test: cares about the spreadsheet import format (source: timeline)"})
                tool("task.report", {"status": "done", "summary": "Understood: cares about the spreadsheet import format"})
            print("TASK DONE " + task_slug, flush=True)
        elif line == "task" and source_person:
            person = tool("source.person", {"source": source_person[0], "external_id": source_person[1]})
            tool("task.report", {"status": "done", "summary": "Reviewed source user: " + str(len(person["events"])) + " events"})
            print("SOURCE DONE " + source_person[1], flush=True)
        elif line == "list" and list_id:
            members = tool("list.get", {"id": list_id})["members"]
            queued = [m for m in members if m["status"] == "queued"]
            for index, member in enumerate(queued):
                tool("list.draft_save", {"list": list_id, "member": member["key"], "subject": "Fixture", "body": "Synthetic list draft. No sending."})
                tool("task.report", {"status": "working", "summary": "Preparing the list", "done": index + 1, "total": len(queued)})
            tool("task.report", {"status": "done", "summary": "List ready: " + str(len(queued)) + " drafts"})
            print("LIST DONE " + str(len(queued)), flush=True)
        elif line == "sync":
            tool("mail.accounts", {})
            tool("source.list", {})
            print("SYNC DONE", flush=True)
        elif line.startswith("touch "):
            tool("contact.get", {"slug": line.split(" ", 1)[1]})
        elif line.startswith("dossier "):
            tool("dossier.write_section", {"slug": line.split(" ", 1)[1], "section": "analysis", "content": "The terminal Agent organized the user's import feedback."})
        elif line == "report":
            tool("document.create", {"path": "notes/feedback.md", "text": "# User feedback report\n\nTo be confirmed."})
        elif line == "revise":
            tool("document.replace", {"path": "notes/feedback.md", "expected": "To be confirmed.", "text": "Confirm the file format, then schedule a follow-up call."})
        elif line == "exit":
            print("SESSION FINISHED", flush=True)
            break
        else:
            print("ECHO: " + line, flush=True)
    except Exception as error:
        print("TOOL ERROR: " + str(error), flush=True)
