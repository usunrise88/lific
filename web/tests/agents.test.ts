import { describe, expect, test } from "bun:test";
import type { AgentRef, IssueAgents } from "../src/lib/api";
import { mentionedAgents, predictRecipients, routedToIssue } from "../src/lib/agents";

const claude: AgentRef = { id: 2, username: "claude-anton", display_name: "Claude Code", owner_id: 1, owner_name: "Anton" };
const desktop: AgentRef = { id: 3, username: "desktop-anton", display_name: "Claude Desktop", owner_id: 1, owner_name: "Anton" };
const yana: AgentRef = { id: 7, username: "claude-yana", display_name: "Claude Code", owner_id: 5, owner_name: "Yana" };

function info(over: Partial<IssueAgents> = {}): IssueAgents {
  return { available: [claude, desktop, yana], attached: [], routes: [], deliveries: {}, mine: [2, 3], ...over };
}

describe("agent recipients", () => {
  test("mentions resolve by username, once each, not inside words or emails", () => {
    expect(mentionedAgents("@claude-anton and @Claude-Anton, hi@claude-yana", [claude, yana])).toEqual([claude]);
    expect(mentionedAgents("(@claude-yana)", [claude, yana])).toEqual([yana]);
  });

  test("picks and mentions replace automatic routing", () => {
    const i = info({ attached: [{ agent: claude, reasons: ["created"] }] });
    expect(predictRecipients(i, [], "thanks").map((r) => [r.agent.id, r.kind])).toEqual([[2, "attached"]]);
    expect(predictRecipients(i, [3], "ping @claude-yana").map((r) => [r.agent.id, r.kind])).toEqual([
      [3, "picked"],
      [7, "mention"],
    ]);
  });

  test("the project route is the fallback; a lone issue route says so", () => {
    const projectRoute = { id: 1, agent: yana, project_id: 9, issue_id: null };
    expect(predictRecipients(info({ routes: [projectRoute] }), [], "").map((r) => r.kind)).toEqual(["project_route"]);
    const issueRoute = { id: 2, agent: desktop, project_id: 9, issue_id: 4 };
    const i = info({ routes: [projectRoute, issueRoute], attached: [{ agent: desktop, reasons: ["routed"] }] });
    expect(predictRecipients(i, [], "").map((r) => [r.agent.id, r.kind])).toEqual([[3, "issue_route"]]);
    expect(routedToIssue(i, 3)).toBe(true);
    expect(routedToIssue(i, 7)).toBe(false);
  });

  test("nothing known, nobody predicted", () => {
    expect(predictRecipients(null, [2], "@claude-anton")).toEqual([]);
    expect(predictRecipients(info(), [], "hello")).toEqual([]);
  });
});
