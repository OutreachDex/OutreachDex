# Analysis and playbooks

All analysis is done by the Agent on the right (the Codex or Claude Code on this computer). The left side only shows the results: profile, summary, next step and **Agent memory**.

## Ask the Agent to understand a person

1. On the person page click "Ask Agent", or click "Understand" in the Agent on the right.
2. A new session pinned to this person opens on the right. The Agent first reads the person's **Agent memory**, dossier, activity and source facts, then summarizes their background, recent situation, needs and next step.
3. To keep asking, type directly in the terminal, for example "Was the export problem she mentioned last time solved?".

The Agent's answer separates facts, inferences and unknowns, and ends with two sections:

- "**Evidence**": each major conclusion with its source (dossier, email, source event or URL);
- "**Open questions**": unsourced claims and questions to ask the person.

When useful, the Agent updates the dossier's analysis section, the background summary and the next step, and may set a follow-up date.

## Draft a reply

Click "Agent drafts reply" (on the person page, next to an incoming email in Activity, or "Draft reply" on the right). The Agent checks the latest incoming mail, judges the person's intent (interested, wants more information, not a good time now…), then saves a **draft for review**. Approving and sending are always up to you.

## Where analysis results are kept

Every "Understand" run saves its conclusions instead of leaving them only in the terminal:

- **Person page Overview → "Agent analysis" card**: the full conclusions (with Evidence and Open questions; tables and source links render properly), when they were written, and how many evidence items and open questions they have. "View session" returns to the terminal record; "Ask again" runs a new analysis. A new analysis replaces the previous one; earlier key points stay in Agent memory.
- **Background summary and next step**: a one- or two-sentence summary and the suggested next step.
- **Today → "Recent Agent results"**: each session's latest outcome (Done, Awaiting your reply, Blocked); click one to open its person and session.

## Agent memory: it gets to know people over time

Every person has an **Agent memory** appended in time order (`memory/<contact>.md` in the workspace), shown in the person page overview:

- **Fact** (with a source), **Insight**, **Preference** (for example how they like to be contacted), **Commitment** (what either side promised), **Question** (still to confirm);
- **Change**: every change the Agent made through OutreachDex (profile updates, saved drafts, scheduled follow-ups…), recorded automatically by the app.

Every new analysis reads this memory first, so understanding of a person accumulates instead of starting from scratch each time. When contacts are merged, their memories are combined. Click "Open full memory" to read all of it.

What the Agent may do:

- **Can**: read data, update the analysis section of the dossier, update the summary and next step, set follow-ups, write Agent memory, **save drafts for review**.
- **Cannot**: approve or send any message.

## Playbooks: custom analysis purposes and business contexts

Analysis purposes and business contexts are Markdown files in the workspace's `playbooks/` folder. You can edit them or add new ones.

```markdown
---
id: feedback
label: Handle feedback
kind: purpose        # purpose = analysis purpose; context = business context
order: 20
creates_drafts: false
requires_prompt: false
---
Understand the person's feedback: their task, symptoms, impact, known evidence, unknowns, and a concrete next step.
```

- At the start of each session the Agent reads every playbook through `workspace.guide`. Files with `kind: context` are business contexts — for example write a `my-product.md` describing your product and its users.
- `creates_drafts: true` means this purpose may draft messages.
- Whatever a playbook says, the app always enforces the safety rules: never send, only write drafts for review, separate facts from inferences.

Workspaces created with older versions may have playbooks and labels written in Chinese; they work the same way, and you can rewrite them in any language.
