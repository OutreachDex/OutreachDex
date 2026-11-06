import { t, locale } from "./i18n";
// Contact pipeline helpers. Stage names, roles and analysis playbooks come from the workspace's
// globalcontact.toml and playbooks/ (see docs/plan-productization.md, phase 1).
export type V = Record<string, any>;
export type Row = { id: number; values: V };
export type Role = "open" | "waiting" | "active" | "closed" | "suppressed";
export type Stage = { label: string; role: Role };
export type Playbook = {
  id: string;
  label: string;
  kind: "purpose" | "context";
  creates_drafts: boolean;
  requires_prompt: boolean;
  path: string;
};
export type WorkspaceInfo = {
  config: { template: string; stages: Stage[]; contacts: { campaigns: string[] }; fields: { title: string }[] };
  playbooks: Playbook[];
};

let workspace: WorkspaceInfo | null = null;
/** Called by the app whenever the workspace configuration is (re)loaded. */
export function setWorkspace(info: WorkspaceInfo | null) {
  workspace = info;
}

export const stageList = (): Stage[] => workspace?.config.stages || [];
export const stages = () => stageList().map((s) => s.label);
export const roleOf = (label: string): Role | undefined => stageList().find((s) => s.label === label)?.role;
const roleTones: Record<Role, string> = { open: "gray", waiting: "amber", active: "green", closed: "gray", suppressed: "red" };
export function stageTone(label: string) {
  const role = roleOf(label);
  if (!role) return "gray";
  // Later active stages ("in progress") read differently from the first ("replied").
  if (role === "active" && stageList().find((s) => s.role === "active")?.label !== label) return "violet";
  return roleTones[role];
}
const inactive = (label: string) => ["closed", "suppressed"].includes(roleOf(label) || "");
export const purposes = () => (workspace?.playbooks || []).filter((p) => p.kind === "purpose");
export const contexts = () => (workspace?.playbooks || []).filter((p) => p.kind === "context");
const time = (value: unknown) => Date.parse(String(value || "")) || 0;

/** Our last message is older than the follow-up window and has no reply. */
export function isStale(row: Row, followupDays = 7) {
  const sent = time(row.values["Last contacted"]);
  const reply = time(row.values["Last reply"]);
  return (
    !inactive(row.values.Stage) &&
    sent > 0 &&
    sent > reply &&
    Date.now() - sent > followupDays * 86400000
  );
}

/** A follow-up date was scheduled and has come. */
export function isDue(row: Row) {
  const due = String(row.values["Follow up"] || "").slice(0, 10);
  return !!due && !inactive(row.values.Stage) && due <= new Date().toISOString().slice(0, 10);
}

/** A keep-in-touch cadence is set, nothing is scheduled, and the last touch is older than it. */
export function cadenceDue(row: Row) {
  const days = Number(row.values["Keep in touch"]) || 0;
  if (!days || row.values["Follow up"] || inactive(row.values.Stage)) return false;
  const last = lastTouch(row);
  return !last || Date.now() - last > days * 86400000;
}

/** Needs a nudge: a scheduled follow-up or keep-in-touch cadence has come, or our message went
 *  unanswered too long. */
export function needsFollowUp(row: Row, followupDays = 7) {
  return isDue(row) || cadenceDue(row) || isStale(row, followupDays);
}

/** They wrote after our last message. */
export function awaitsReply(row: Row) {
  const sent = time(row.values["Last contacted"]);
  const reply = time(row.values["Last reply"]);
  return !inactive(row.values.Stage) && reply > 0 && reply > sent;
}

/** The person wrote after this draft was created, so it may no longer fit. */
export function draftStale(draft: V, lastReply: unknown) {
  const created = time(draft?.metadata?.created_at);
  return !!created && time(lastReply) > created && !["sent", "discarded"].includes(draft?.metadata?.status);
}

export function lastTouch(row: Row) {
  return Math.max(time(row.values["Last contacted"]), time(row.values["Last reply"]));
}

export function relative(at: number | string | undefined) {
  const value = typeof at === "number" ? at : time(at);
  if (!value) return "";
  const diff = Date.now() - value;
  const minute = 60000,
    hour = 60 * minute,
    day = 24 * hour;
  if (diff < minute) return t("just now");
  if (diff < hour) return t("{n} min ago", { n: Math.floor(diff / minute) });
  if (diff < day) return t("{n} h ago", { n: Math.floor(diff / hour) });
  if (diff < 7 * day) return t("{n} d ago", { n: Math.floor(diff / day) });
  return new Date(value).toLocaleDateString(locale, { month: "numeric", day: "numeric" });
}

export const matches = (row: Row, query: string) =>
  !query ||
  Object.values(row.values).join(" ").toLowerCase().includes(query.trim().toLowerCase());
