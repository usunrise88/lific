// Fork: Home filters and the flow report's view logic (closures per day or
// week and per actor, durations, period deltas). Pure, so bun can test it.

import type { FlowActor, FlowReport } from "../api";
import { locale } from "../i18n";

// ── Filters ──────────────────────────────────────────────

export const PERIODS = [7, 30, 90] as const;
export type Period = (typeof PERIODS)[number];

export interface HomeFilters {
  groups: number[];
  projects: number[];
  days: Period;
}

export const DEFAULT_HOME_FILTERS: HomeFilters = { groups: [], projects: [], days: 30 };

const FILTERS_KEY = "lific:home:filters";

function nums(v: unknown): number[] {
  return Array.isArray(v) ? v.filter((x): x is number => typeof x === "number") : [];
}

export function loadHomeFilters(): HomeFilters {
  try {
    const raw = localStorage.getItem(FILTERS_KEY);
    if (raw) {
      const v = JSON.parse(raw) as Partial<HomeFilters>;
      return {
        groups: nums(v.groups),
        projects: nums(v.projects),
        days: PERIODS.includes(v.days as Period) ? (v.days as Period) : DEFAULT_HOME_FILTERS.days,
      };
    }
  } catch {
    // private mode or a hand-edited value: start clean
  }
  return { ...DEFAULT_HOME_FILTERS };
}

export function saveHomeFilters(f: HomeFilters): void {
  try {
    localStorage.setItem(FILTERS_KEY, JSON.stringify(f));
  } catch {
    // ignore
  }
}

/** Groups and projects as one id list; undefined when nothing is picked. */
export function scopeIds(
  f: Pick<HomeFilters, "groups" | "projects">,
  groups: { id: number; project_ids: number[] }[],
): number[] | undefined {
  if (f.groups.length === 0 && f.projects.length === 0) return undefined;
  const ids = new Set(f.projects);
  for (const g of groups) if (f.groups.includes(g.id)) for (const id of g.project_ids) ids.add(id);
  return [...ids].sort((a, b) => a - b);
}

/** Keep only items in scope; everything passes when no scope is set. */
export function inScope<T extends { project_id: number | null }>(items: T[], scope: number[] | undefined): T[] {
  if (!scope) return items;
  const set = new Set(scope);
  return items.filter((i) => i.project_id !== null && set.has(i.project_id));
}

// ── Actors ───────────────────────────────────────────────

export const SERIES = 8;

/** A color per actor that follows the actor, not its rank, so filtering
 *  never repaints the survivors. System writes are neutral. */
export function actorColor(id: number | null): string {
  if (id === null) return "color-mix(in oklab, var(--text-faint) 55%, transparent)";
  return `var(--series-${(((id - 1) % SERIES) + SERIES) % SERIES + 1})`;
}

export function actorName(actors: FlowActor[], id: number | null, system: string): string {
  if (id === null) return system;
  const a = actors.find((x) => x.id === id);
  return a ? a.display_name || a.username : `#${id}`;
}

export function isAgent(actors: FlowActor[], id: number | null): boolean {
  return id !== null && actors.some((a) => a.id === id && a.is_bot);
}

// ── Closure bars ─────────────────────────────────────────

export type Grain = "day" | "week";

export interface Segment {
  actor: number | null;
  count: number;
}

export interface Bar {
  /** First day of the bar, `YYYY-MM-DD`. */
  date: string;
  /** Last day, same as `date` for a day bar. */
  until: string;
  done: number;
  cancelled: number;
  reopened: number;
  created: number;
  /** Closes by actor, in the report's actor order. */
  segments: Segment[];
}

function parseDay(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}

function dayText(d: Date): string {
  return d.toISOString().slice(0, 10);
}

/** The Monday starting `day`'s week. */
export function weekStart(day: string): string {
  const d = parseDay(day);
  const back = (d.getUTCDay() + 6) % 7;
  d.setUTCDate(d.getUTCDate() - back);
  return dayText(d);
}

/** Actors in a stable stacking order: agents first, then by id. */
export function actorOrder(report: FlowReport): (number | null)[] {
  const ids = new Set<number | null>(report.closures.map((c) => c.actor_id));
  return [...ids].sort((a, b) => {
    const ab = isAgent(report.actors, a) ? 0 : 1;
    const bb = isAgent(report.actors, b) ? 0 : 1;
    if (ab !== bb) return ab - bb;
    return (a ?? Number.MAX_SAFE_INTEGER) - (b ?? Number.MAX_SAFE_INTEGER);
  });
}

export function buildBars(report: FlowReport, grain: Grain): Bar[] {
  const order = actorOrder(report);
  const bars = new Map<string, Bar & { byActor: Map<number | null, number> }>();
  const keyOf = (day: string) => (grain === "week" ? weekStart(day) : day);
  for (const d of report.daily) {
    const key = keyOf(d.date);
    let bar = bars.get(key);
    if (!bar) {
      bar = { date: key, until: d.date, done: 0, cancelled: 0, reopened: 0, created: 0, segments: [], byActor: new Map() };
      bars.set(key, bar);
    }
    bar.until = d.date;
    bar.done += d.done;
    bar.cancelled += d.cancelled;
    bar.reopened += d.reopened;
    bar.created += d.created_by_agents + d.created_by_people;
  }
  for (const c of report.closures) {
    const bar = bars.get(keyOf(c.date));
    if (bar) bar.byActor.set(c.actor_id, (bar.byActor.get(c.actor_id) ?? 0) + c.done);
  }
  return [...bars.values()].map(({ byActor, ...bar }) => ({
    ...bar,
    segments: order.filter((a) => byActor.has(a)).map((actor) => ({ actor, count: byActor.get(actor)! })),
  }));
}

// ── Numbers ──────────────────────────────────────────────

/** Change from `prev` to `cur` as a whole percent; null when there is
 *  nothing to compare against. */
export function deltaPercent(cur: number | null, prev: number | null): number | null {
  if (cur === null || prev === null || prev === 0) return null;
  return Math.round(((cur - prev) / prev) * 100);
}

export function share(part: number, whole: number): number | null {
  return whole > 0 ? Math.round((part / whole) * 100) : null;
}

/** A duration in its most readable unit, localized: 40 min, 3.5 h, 2 d. */
export function formatDuration(seconds: number | null | undefined, lang = locale()): string {
  if (seconds === null || seconds === undefined) return "—";
  const fmt = (value: number, unit: string) =>
    new Intl.NumberFormat(lang, {
      style: "unit",
      unit,
      unitDisplay: "narrow",
      maximumFractionDigits: value < 10 ? 1 : 0,
    }).format(value);
  if (seconds < 60) return fmt(Math.max(0, Math.round(seconds)), "second");
  if (seconds < 3_600) return fmt(Math.round(seconds / 60), "minute");
  if (seconds < 86_400) return fmt(Math.round((seconds / 3_600) * 10) / 10, "hour");
  if (seconds < 14 * 86_400) return fmt(Math.round((seconds / 86_400) * 10) / 10, "day");
  return fmt(Math.round((seconds / (7 * 86_400)) * 10) / 10, "week");
}

/** Server UTC timestamp ("2026-11-11 10:00:00") to epoch ms. */
export function epochOf(ts: string): number {
  return Date.parse(ts.replace(" ", "T") + (/[zZ]|[+-]\d\d:?\d\d$/.test(ts) ? "" : "Z"));
}

/** Quiet this long, an active issue looks abandoned. */
export const STALE_SECONDS = 24 * 3_600;

export function isStale(lastActivity: string | null, now = Date.now()): boolean {
  return lastActivity !== null && now - epochOf(lastActivity) > STALE_SECONDS * 1000;
}

/** Short date for an axis label in the reader's language. */
export function shortDate(day: string, lang = locale()): string {
  return parseDay(day).toLocaleDateString(lang, { day: "numeric", month: "short", timeZone: "UTC" });
}
