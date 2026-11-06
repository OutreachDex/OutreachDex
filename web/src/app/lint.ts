import { t } from "./i18n";
// Local copy checks shown beside a draft. Hints only: nothing is rewritten automatically.
export type Hint = { level: "block" | "warn"; text: string };

const openers = [/^(希望|祝)?你(一切|最近)?(都)?好/, /冒昧(打扰|来信)/, /^hope (this|you)/i, /^i hope this (email|message) finds you/i, /^i('m| am) reaching out/i, /^just following up/i];
const buzz = ["赋能", "抓手", "闭环", "颗粒度", "打通", "痛点", "无缝", "深度赋能", "leverage", "synergy", "delve", "game-changer", "cutting-edge", "seamless", "unlock", "revolutionize"];
const placeholder = /\{\{[^}]*\}\}|\[(姓名|名字|公司|name|first ?name|company)\]|<(name|company)>/i;

/** `language`: the contact's Language field ("Chinese" / "English"; older workspaces store "中文"), when known. */
export function lintDraft(body: string, channel: string, language?: string): Hint[] {
  const hints: Hint[] = [];
  const text = body.trim();
  if (!text) return hints;
  if (placeholder.test(text)) hints.push({ level: "block", text: t("There are unreplaced placeholders (such as {{name}} or [name]); fix them before sending.") });
  const firstLine = text.split("\n").find((l) => l.trim() && !/^(hi|hello|dear|你好|您好)[\s,，!！]*\S{0,12}[,，!！]?$/i.test(l.trim()))?.trim() || "";
  if (openers.some((r) => r.test(firstLine))) hints.push({ level: "warn", text: t("The opening is boilerplate; say directly why you are reaching out.") });
  const words = buzz.filter((w) => text.toLowerCase().includes(w.toLowerCase()));
  if (words.length) hints.push({ level: "warn", text: t("AI-sounding words or jargon: {words}", { words: words.join(t(", ")) }) });
  const cjk = (text.match(/[\u4e00-\u9fff]/g) || []).length;
  const latinWords = text.replace(/[\u4e00-\u9fff]/g, " ").split(/\s+/).filter(Boolean).length;
  const short = channel !== "email";
  // Written in the other language than the contact uses (e.g. Chinese to an English speaker).
  if (language === "English" && cjk >= 10 && cjk > latinWords)
    hints.push({ level: "warn", text: t("This contact's language is English, but the draft is mostly Chinese.") });
  if ((language === "Chinese" || language === "中文") && cjk < 5 && latinWords >= 20)
    hints.push({ level: "warn", text: t("This contact's language is Chinese, but the draft is in English.") });
  if (cjk > (short ? 150 : 300) || (cjk < 20 && latinWords > (short ? 80 : 180)))
    hints.push({ level: "warn", text: short ? t("This message is long; keep it to a few sentences.") : t("This email is long; a first contact should be shorter and cover one thing.") });
  const asks = (text.match(/[?？]/g) || []).length;
  if (asks > 2) hints.push({ level: "warn", text: t("{n} questions; one clear request per message works best.", { n: asks }) });
  const links = (text.match(/https?:\/\//g) || []).length;
  if (links > 2) hints.push({ level: "warn", text: t("{n} links; keep a first contact to 1–2.", { n: links }) });
  if ((text.match(/[!！]/g) || []).length > 2) hints.push({ level: "warn", text: t("Too many exclamation marks; it reads like marketing.") });
  if ((text.match(/——|—/g) || []).length > 2) hints.push({ level: "warn", text: t("Too many dashes; it reads like AI-generated text.") });
  return hints;
}
