import { describe, expect, test } from "bun:test";
import type { FlowReport } from "../src/lib/api";
import {
  actorColor,
  actorOrder,
  buildBars,
  deltaPercent,
  formatDuration,
  inScope,
  isStale,
  scopeIds,
  share,
  weekStart,
} from "../src/lib/home/flow";

function report(over: Partial<FlowReport> = {}): FlowReport {
  const day = (date: string, done = 0) => ({
    date,
    done,
    cancelled: 0,
    reopened: 0,
    created_by_agents: 1,
    created_by_people: 0,
  });
  return {
    days: 7,
    from: "2026-11-05",
    to: "2026-11-11",
    actors: [
      { id: 1, username: "blake", display_name: "Blake", is_bot: false, owner_id: null },
      { id: 2, username: "claude", display_name: "Claude", is_bot: true, owner_id: 1 },
    ],
    daily: [
      day("2026-11-05"),
      day("2026-11-06"),
      day("2026-11-07"),
      day("2026-11-08"),
      day("2026-11-09", 3),
      day("2026-11-10", 1),
      day("2026-11-11"),
    ],
    closures: [
      { date: "2026-11-09", actor_id: 1, done: 1 },
      { date: "2026-11-09", actor_id: 2, done: 2 },
      { date: "2026-11-10", actor_id: 2, done: 1 },
    ],
    per_actor: [],
    current: { done: 4, cancelled: 0, reopened: 0, created: 7, with_evidence: 0, lead_p50: null, cycle_p50: null },
    previous: { done: 2, cancelled: 0, reopened: 0, created: 3, with_evidence: 0, lead_p50: null, cycle_p50: null },
    stages: [],
    lead: { count: 0, p50: null, p85: null, mean: null },
    cycle: { count: 0, p50: null, p85: null, mean: null },
    timed: 0,
    skipped_active: 0,
    lead_histogram: [],
    slowest: [],
    active: [],
    active_total: 0,
    waiting: [],
    waiting_total: 0,
    waiting_p50: null,
    ...over,
  };
}

describe("home flow", () => {
  test("groups and projects make one scope; nothing picked means everything", () => {
    const groups = [{ id: 1, project_ids: [3, 4] }, { id: 2, project_ids: [9] }];
    expect(scopeIds({ groups: [], projects: [] }, groups)).toBeUndefined();
    expect(scopeIds({ groups: [1], projects: [4, 7] }, groups)).toEqual([3, 4, 7]);
    const items = [{ project_id: 3 }, { project_id: 5 }, { project_id: null }];
    expect(inScope(items, undefined)).toHaveLength(3);
    expect(inScope(items, [3])).toEqual([{ project_id: 3 }]);
  });

  test("weeks start on Monday", () => {
    expect(weekStart("2026-11-11")).toBe("2026-11-09"); // Wednesday
    expect(weekStart("2026-11-09")).toBe("2026-11-09"); // Monday
    expect(weekStart("2026-11-15")).toBe("2026-11-09"); // Sunday
  });

  test("agents stack first, and colors follow the actor, not its rank", () => {
    expect(actorOrder(report())).toEqual([2, 1]);
    expect(actorColor(2)).toBe("var(--series-2)");
    expect(actorColor(10)).toBe("var(--series-2)");
    expect(actorColor(null)).toContain("--text-faint");
  });

  test("day bars carry each actor's closes; week bars add them up", () => {
    const days = buildBars(report(), "day");
    expect(days).toHaveLength(7);
    expect(days[4].segments).toEqual([
      { actor: 2, count: 2 },
      { actor: 1, count: 1 },
    ]);
    const weeks = buildBars(report(), "week");
    expect(weeks.map((w) => [w.date, w.until])).toEqual([
      ["2026-11-02", "2026-11-08"],
      ["2026-11-09", "2026-11-11"],
    ]);
    expect(weeks[1].done).toBe(4);
    expect(weeks[1].created).toBe(3);
    expect(weeks[1].segments).toEqual([
      { actor: 2, count: 3 },
      { actor: 1, count: 1 },
    ]);
  });

  test("numbers: deltas, shares, durations and staleness", () => {
    expect(deltaPercent(4, 2)).toBe(100);
    expect(deltaPercent(4, 0)).toBeNull();
    expect(share(1, 3)).toBe(33);
    expect(share(1, 0)).toBeNull();
    expect(formatDuration(null, "en")).toBe("—");
    expect(formatDuration(45 * 60, "en")).toBe("45m");
    expect(formatDuration(5_400, "en")).toBe("1.5h");
    expect(formatDuration(3 * 86_400, "en")).toBe("3d");
    expect(formatDuration(30 * 86_400, "en")).toBe("4.3w");
    const now = Date.UTC(2026, 10, 11, 12);
    expect(isStale("2026-11-10 11:00:00", now)).toBe(true);
    expect(isStale("2026-11-11 11:00:00", now)).toBe(false);
    expect(isStale(null, now)).toBe(false);
  });
});
