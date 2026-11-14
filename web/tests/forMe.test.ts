import { describe, expect, test } from "bun:test";
import { activeFilterCount, DEFAULT_FILTERS, epoch, isNew, loadFilters, plainText } from "../src/lib/forMe";
import { logoRoute } from "../src/lib/logoRoute";

describe("for me", () => {
  test("new means handed over after the last visit, never on a first visit", () => {
    const seen = epoch("2026-11-11 10:00:00");
    expect(isNew({ assigned_at: "2026-11-11 10:00:01" }, seen)).toBe(true);
    expect(isNew({ assigned_at: "2026-11-11 09:59:59" }, seen)).toBe(false);
    expect(isNew({ assigned_at: "2026-11-11 10:00:01" }, 0)).toBe(false);
  });

  test("server timestamps read as UTC", () => {
    expect(epoch("2026-11-11 10:00:00")).toBe(Date.UTC(2026, 10, 11, 10, 0, 0));
    expect(epoch("2026-11-11T10:00:00Z")).toBe(Date.UTC(2026, 10, 11, 10, 0, 0));
  });

  test("sort and mine-first reorder, they do not count as filters", () => {
    expect(activeFilterCount({ ...DEFAULT_FILTERS, sort: "oldest", mineFirst: false })).toBe(0);
    expect(activeFilterCount({ ...DEFAULT_FILTERS, scopes: ["me"], groups: [1], projects: [2] })).toBe(2);
  });

  test("stored filters are sanitised", () => {
    const store = new Map<string, string>();
    globalThis.localStorage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    } as unknown as Storage;
    store.set(
      "lific:forme:filters",
      JSON.stringify({ sort: "sideways", mineFirst: "yes", scopes: ["me", "all"], statuses: ["todo", "open"], requestedBy: "aliens" }),
    );
    const f = loadFilters();
    expect(f.sort).toBe("newest");
    expect(f.mineFirst).toBe(true);
    expect(f.scopes).toEqual(["me"]);
    expect(f.statuses).toEqual(["todo"]);
    expect(f.requestedBy).toBe("all");
  });

  test("previews drop markdown but keep links' text and identifiers", () => {
    expect(plainText("## Plan\n- [docs](http://x) for **VOICE_CORE** `run`")).toBe("Plan docs for VOICE_CORE run");
  });

  test("logo targets map to routes; the repo stays external", () => {
    expect(logoRoute("repo")).toBeNull();
    expect(logoRoute("home")).toBe("/");
    expect(logoRoute("feed")).toBe("/feed");
    expect(logoRoute("for-me")).toBe("/for-me");
  });
});
