// Fork: agents (bot accounts owned by people) as addressees of comments.
// Pure, so bun can test it; the server holds the routing rules
// (src/db/queries/agent_inbox.rs) and these only predict them for the UI.

import type { AgentRef, IssueAgents } from "./api";
import { t } from "./i18n";

/** "Claude Code (Anton)", or just the name for an ownerless bot. */
export function agentLabel(a: AgentRef): string {
  const name = a.display_name || a.username;
  return a.owner_name ? t("agents.label", { name, owner: a.owner_name }) : name;
}

const MENTION = /(^|[^A-Za-z0-9_@-])@([A-Za-z0-9_-]+)/g;

/** Agents @mentioned in `text`, in order, each once. */
export function mentionedAgents(text: string, agents: AgentRef[]): AgentRef[] {
  const out: AgentRef[] = [];
  for (const m of text.matchAll(MENTION)) {
    const name = m[2].toLowerCase();
    const a = agents.find((x) => x.username.toLowerCase() === name);
    if (a && !out.includes(a)) out.push(a);
  }
  return out;
}

export type RecipientKind = "picked" | "mention" | "attached" | "issue_route" | "project_route";

export interface Recipient {
  agent: AgentRef;
  kind: RecipientKind;
}

/**
 * Who a comment from the current person will reach, the way the server
 * decides it: explicit picks and @mentions win; otherwise the agents
 * attached to the issue (and those routed to it); otherwise the agents
 * routed to the project. The author's own agents are included: they are
 * not the author.
 */
export function predictRecipients(
  info: IssueAgents | null,
  picks: number[],
  draft: string,
): Recipient[] {
  if (!info) return [];
  const byId = new Map(info.available.map((a) => [a.id, a]));
  const explicit: Recipient[] = [];
  for (const id of picks) {
    const a = byId.get(id);
    if (a) explicit.push({ agent: a, kind: "picked" });
  }
  for (const a of mentionedAgents(draft, info.available)) {
    if (!explicit.some((r) => r.agent.id === a.id)) explicit.push({ agent: a, kind: "mention" });
  }
  if (explicit.length) return explicit;
  const issueLevel: Recipient[] = info.attached.map(({ agent, reasons }) => ({
    agent,
    kind: reasons.length === 1 && reasons[0] === "routed" ? "issue_route" : "attached",
  }));
  if (issueLevel.length) return issueLevel;
  return info.routes
    .filter((r) => r.issue_id === null)
    .map((r) => ({ agent: r.agent, kind: "project_route" as const }));
}

/** The issue-level routes an agent has here. */
export function routedToIssue(info: IssueAgents, agentId: number): boolean {
  return info.routes.some((r) => r.agent.id === agentId && r.issue_id !== null);
}
