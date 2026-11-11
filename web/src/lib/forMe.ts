// For me page (fork): filter state and the "new since you last looked"
// marker. Pure, so it can be tested without a browser.

import type { ForMeItem, ForMeRequestedBy, ForMeScope, ForMeSort } from "./api";

export const SORT_OPTIONS: { value: ForMeSort; label: string }[] = [
  { value: "newest", label: "Newest first" },
  { value: "oldest", label: "Oldest first" },
  { value: "updated", label: "Recently updated" },
  { value: "priority", label: "Priority" },
];

export const SCOPE_OPTIONS: { value: ForMeScope; label: string; hint: string }[] = [
  { value: "me", label: "Me", hint: "Named for you, or waiting on you" },
  { value: "anyone", label: "Any person", hint: "Marked for a person, nobody named" },
  { value: "others", label: "Others", hint: "Named for someone else" },
];

export const STATUS_OPTIONS = ["backlog", "todo", "active", "done", "cancelled"] as const;
export const OPEN_STATUSES = ["backlog", "todo", "active"];
export const PRIORITY_OPTIONS = ["urgent", "high", "medium", "low", "none"] as const;

export interface ForMeFilters {
  sort: ForMeSort;
  mineFirst: boolean;
  scopes: ForMeScope[];
  statuses: string[];
  priorities: string[];
  requestedBy: ForMeRequestedBy;
  groups: number[];
  projects: number[];
}

export const DEFAULT_FILTERS: ForMeFilters = {
  sort: "newest",
  mineFirst: true,
  scopes: [],
  statuses: [],
  priorities: [],
  requestedBy: "all",
  groups: [],
  projects: [],
};

const FILTERS_KEY = "lific:forme:filters";
const SEEN_KEY = "lific:forme:seen";

function nums(v: unknown): number[] {
  return Array.isArray(v) ? v.filter((x): x is number => typeof x === "number") : [];
}

function pick<T extends string>(v: unknown, allowed: readonly T[]): T[] {
  return Array.isArray(v) ? v.filter((x): x is T => allowed.includes(x as T)) : [];
}

export function loadFilters(): ForMeFilters {
  try {
    const raw = localStorage.getItem(FILTERS_KEY);
    if (raw) {
      const v = JSON.parse(raw) as Partial<ForMeFilters>;
      return {
        sort: SORT_OPTIONS.some((o) => o.value === v.sort) ? (v.sort as ForMeSort) : DEFAULT_FILTERS.sort,
        mineFirst: typeof v.mineFirst === "boolean" ? v.mineFirst : DEFAULT_FILTERS.mineFirst,
        scopes: pick(v.scopes, SCOPE_OPTIONS.map((o) => o.value)),
        statuses: pick(v.statuses, STATUS_OPTIONS),
        priorities: pick(v.priorities, PRIORITY_OPTIONS),
        requestedBy: v.requestedBy === "agents" || v.requestedBy === "people" ? v.requestedBy : "all",
        groups: nums(v.groups),
        projects: nums(v.projects),
      };
    }
  } catch {
    // private mode or a hand-edited value: start clean
  }
  return { ...DEFAULT_FILTERS };
}

export function saveFilters(f: ForMeFilters): void {
  try {
    localStorage.setItem(FILTERS_KEY, JSON.stringify(f));
  } catch {
    // ignore
  }
}

/** Filters that narrow the list (sort and mine-first only reorder it). */
export function activeFilterCount(f: ForMeFilters): number {
  return (
    (f.scopes.length ? 1 : 0) +
    (f.statuses.length ? 1 : 0) +
    (f.priorities.length ? 1 : 0) +
    (f.requestedBy !== "all" ? 1 : 0) +
    (f.groups.length + f.projects.length ? 1 : 0)
  );
}

// ── New since last visit ─────────────────────────────────

/** When the reader last had this page open, as epoch ms; 0 on a first visit. */
export function loadSeen(): number {
  try {
    const v = Number(localStorage.getItem(SEEN_KEY));
    return Number.isFinite(v) ? v : 0;
  } catch {
    return 0;
  }
}

export function saveSeen(at: number): void {
  try {
    localStorage.setItem(SEEN_KEY, String(at));
  } catch {
    // ignore
  }
}

/** Server timestamps are UTC without a zone ("2026-11-11 10:00:00"). */
export function epoch(ts: string): number {
  return Date.parse(ts.replace(" ", "T") + (/[zZ]|[+-]\d\d:?\d\d$/.test(ts) ? "" : "Z"));
}

/** Handed to people after the reader last looked. A first visit marks
 *  nothing, or every card would shout at once. */
export function isNew(item: Pick<ForMeItem, "assigned_at">, seenAt: number): boolean {
  return seenAt > 0 && epoch(item.assigned_at) > seenAt;
}

/** Plain text for a card preview: markdown markers out, identifiers kept. */
export function plainText(v: string | null | undefined, max = 280): string {
  if (!v) return "";
  const flat = v
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/[*`]+|^[#>\s-]+|(?<![\w])_+|_+(?![\w])/gm, "")
    .replace(/\s+/g, " ")
    .trim();
  return flat.length > max ? flat.slice(0, max - 1) + "…" : flat;
}
