import { describe, expect, test } from "bun:test";
import type { FeedItem } from "../src/lib/api";
import {
  buildCards,
  loadFilters,
  selectedProjectIds,
  verbFor,
  EMPTY_FILTERS,
} from "../src/lib/feed";

let nextId = 1000;

function row(over: Partial<FeedItem>): FeedItem {
  return {
    id: nextId--,
    ts: "2026-11-10 22:50:53",
    actor_user_id: 2,
    actor_username: "claude-code-admin",
    actor_display_name: "Claude Code",
    actor_is_bot: true,
    transport: "mcp",
    entity_type: "issue",
    entity_id: 1,
    entity_label: "PRISM-1",
    project_id: 1,
    issue_id: 1,
    page_id: null,
    action: "update",
    field: null,
    old_value: null,
    new_value: null,
    kind: "edit",
    project_identifier: "PRISM",
    project_name: "Prism",
    issue_title: "Title",
    issue_status: "todo",
    page_title: null,
    ...over,
  };
}

function issue(n: number, over: Partial<FeedItem>): FeedItem {
  return row({ entity_id: n, issue_id: n, entity_label: `PRISM-${n}`, ...over });
}

describe("feed cards", () => {
  test("an issue created with labels is one card led by the creation", () => {
    const cards = buildCards([
      issue(19, { action: "attach", field: "label", new_value: "stand", kind: "label" }),
      issue(19, { action: "attach", field: "label", new_value: "perf", kind: "label" }),
      issue(19, { action: "create", new_value: "Capacity", kind: "created" }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].kind).toBe("created");
    expect(cards[0].rows).toHaveLength(3);
    expect(verbFor(cards[0])).toBe("created");
  });

  test("closing with an evidence comment stays one card with the comment", () => {
    const cards = buildCards([
      row({ entity_type: "comment", entity_id: 9, action: "create", new_value: "PR merged", kind: "comment" }),
      row({ action: "update", field: "status", old_value: "active", new_value: "done", kind: "status" }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].kind).toBe("status");
    expect(cards[0].rows.some((r) => r.kind === "comment")).toBe(true);
  });

  test("several issues moved to one status at once become a batch", () => {
    const cards = buildCards([
      issue(22, { field: "status", old_value: "todo", new_value: "done", kind: "status" }),
      issue(21, { field: "status", old_value: "todo", new_value: "done", kind: "status" }),
      issue(20, { field: "status", old_value: "todo", new_value: "done", kind: "status" }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].batch).toBe(true);
    expect(cards[0].subjects.map((s) => s.label)).toEqual(["PRISM-20", "PRISM-21", "PRISM-22"]);
    expect(verbFor(cards[0])).toBe("moved 3 issues to");
  });

  test("different target statuses, actors or distant times do not batch", () => {
    const cards = buildCards([
      issue(3, { field: "status", new_value: "done", kind: "status" }),
      issue(2, { field: "status", new_value: "active", kind: "status" }),
      issue(1, { field: "status", new_value: "active", kind: "status", actor_user_id: 7 }),
      issue(4, { field: "status", new_value: "active", kind: "status", ts: "2026-11-10 20:00:00" }),
    ]);
    expect(cards).toHaveLength(4);
    expect(cards.every((c) => !c.batch)).toBe(true);
  });

  test("a plan created with its steps is led by the plan", () => {
    const cards = buildCards([
      row({ entity_type: "plan_step", entity_id: 85, entity_label: "FORGE-PLAN-1", issue_id: null, action: "create", kind: "plan" }),
      row({ entity_type: "plan_step", entity_id: 84, entity_label: "FORGE-PLAN-1", issue_id: null, action: "create", kind: "plan" }),
      row({ entity_type: "plan", entity_id: 3, entity_label: "FORGE-PLAN-1", issue_id: null, action: "create", new_value: "Roadmap", kind: "plan" }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].lead.entity_type).toBe("plan");
    expect(cards[0].subjects[0].title).toBe("Roadmap");
    expect(cards[0].subjects[0].href).toBe("/PRISM/plans/3");
  });
});

describe("feed filters", () => {
  test("groups and projects add up; nothing selected means every project", () => {
    const groups = [{ id: 1, project_ids: [10, 11] }];
    expect(selectedProjectIds(EMPTY_FILTERS, groups)).toBeUndefined();
    expect(
      selectedProjectIds({ ...EMPTY_FILTERS, groups: [1], projects: [12, 10] }, groups)?.sort(),
    ).toEqual([10, 11, 12]);
  });

  test("stored filters are sanitised", () => {
    const store = new Map<string, string>();
    globalThis.localStorage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    } as unknown as Storage;
    store.set(
      "lific:feed:filters",
      JSON.stringify({ kinds: ["status", "bogus"], actors: [3, "x", "system"], agents: "weird" }),
    );
    const f = loadFilters();
    expect(f.kinds).toEqual(["status"]);
    expect(f.actors).toEqual([3, "system"]);
    expect(f.agents).toBe("all");
  });
});

describe("plain snippets", () => {
  test("drop markdown emphasis but keep identifiers", async () => {
    const { plainSnippet } = await import("../src/lib/feed");
    expect(plainSnippet("**Done**: VOICECORE_SIGNAL=full _ok_ `code`\n# Head")).toBe(
      "Done: VOICECORE_SIGNAL=full ok code Head",
    );
  });
});
