// Feed page (fork): turns the flat audit rows from /api/feed into cards.
//
// One user action writes several audit rows at the same second: creating an
// issue also attaches its labels, assigns it and links it; closing work over
// MCP sets a status and leaves an evidence comment. Showing each row on its
// own buries the action, so rows are grouped twice:
//
//   1. by subject: same actor, same issue/page/plan, within a few seconds;
//   2. into batches: consecutive subjects the actor created or moved to the
//      same status in one go ("moved 3 issues to Done").
//
// Everything here is pure so it can be tested without a browser.

import type { FeedItem, FeedKind } from "./api";

/** Rows further apart than this never share a card. */
const SAME_ACTION_SECONDS = 10;

/** Which row leads a card, and so decides its wording and icon. */
const KIND_RANK: FeedKind[] = [
  "created",
  "status",
  "comment",
  "edit",
  "assignee",
  "link",
  "label",
  "page",
  "plan",
  "structure",
];

export const KIND_OPTIONS: { kind: FeedKind; label: string }[] = [
  { kind: "created", label: "Created" },
  { kind: "status", label: "Status" },
  { kind: "comment", label: "Comments" },
  { kind: "edit", label: "Edits" },
  { kind: "assignee", label: "Assignees" },
  { kind: "label", label: "Labels" },
  { kind: "link", label: "Relations" },
  { kind: "page", label: "Pages" },
  { kind: "plan", label: "Plans" },
  { kind: "structure", label: "Structure" },
];

export interface FeedSubject {
  key: string;
  type: "issue" | "page" | "plan" | "other";
  label: string | null;
  title: string | null;
  status: string | null;
  issueId: number | null;
  pageId: number | null;
  /** In-app route, or null when there is nothing to open. */
  href: string | null;
}

export interface FeedCard {
  key: string;
  kind: FeedKind;
  /** Newest timestamp in the card. */
  ts: string;
  lead: FeedItem;
  rows: FeedItem[];
  subjects: FeedSubject[];
  batch: boolean;
  actorId: number | null;
  actorName: string;
  actorUsername: string | null;
  isBot: boolean;
  transport: string;
  project: string | null;
  projectName: string | null;
}

export function actorName(a: FeedItem): string {
  return a.actor_display_name || a.actor_username || "System";
}

function seconds(ts: string): number {
  return Date.parse(ts.replace(" ", "T") + "Z") / 1000;
}

function subjectOf(a: FeedItem): FeedSubject {
  const project = a.project_identifier;
  if (a.issue_id !== null) {
    // A comment row carries the issue's identifier as its label too.
    const label = a.entity_label;
    return {
      key: `issue:${a.issue_id}`,
      type: "issue",
      label,
      title: a.issue_title ?? (a.entity_type === "issue" && a.action === "create" ? a.new_value : null),
      status: a.issue_status,
      issueId: a.issue_id,
      pageId: null,
      href: project && label ? `/${project}/issues/${label}` : null,
    };
  }
  if (a.page_id !== null) {
    return {
      key: `page:${a.page_id}`,
      type: "page",
      label: a.entity_type === "page" ? a.entity_label : null,
      title: a.page_title ?? (a.entity_type === "page" && a.action === "create" ? a.new_value : null),
      status: null,
      issueId: null,
      pageId: a.page_id,
      href: project ? `/${project}/pages/${a.page_id}` : null,
    };
  }
  if (a.entity_type === "plan" || a.entity_type === "plan_step") {
    return {
      key: `plan:${a.entity_label ?? a.entity_id}`,
      type: "plan",
      label: a.entity_label,
      title: a.entity_type === "plan" && a.action === "create" ? a.new_value : null,
      status: null,
      issueId: null,
      pageId: null,
      href: project && a.entity_type === "plan" ? `/${project}/plans/${a.entity_id}` : null,
    };
  }
  return {
    key: `${a.entity_type}:${a.entity_id}`,
    type: "other",
    label: a.entity_label,
    title: null,
    status: null,
    issueId: null,
    pageId: null,
    href: a.entity_type === "module" && project ? `/${project}/modules/${a.entity_id}` : null,
  };
}

function leadOf(rows: FeedItem[]): FeedItem {
  let best = rows[0];
  for (const r of rows) {
    if (KIND_RANK.indexOf(r.kind) < KIND_RANK.indexOf(best.kind)) best = r;
    // A plan's own creation outranks the steps created with it.
    else if (r.kind === best.kind && r.entity_type === "plan" && best.entity_type === "plan_step") best = r;
  }
  return best;
}

function mergeSubject(into: FeedSubject, from: FeedSubject) {
  into.title ??= from.title;
  into.status ??= from.status;
  into.href ??= from.href;
  into.label ??= from.label;
}

function cardFrom(rows: FeedItem[], subject: FeedSubject): FeedCard {
  const lead = leadOf(rows);
  const first = rows[0];
  return {
    key: `c${rows[rows.length - 1].id}`,
    kind: lead.kind,
    ts: first.ts,
    lead,
    rows,
    subjects: [subject],
    batch: false,
    actorId: first.actor_user_id,
    actorName: actorName(first),
    actorUsername: first.actor_username,
    isBot: first.actor_is_bot,
    transport: first.transport,
    project: first.project_identifier,
    projectName: first.project_name,
  };
}

/** A card that can absorb its neighbour into a batch: one subject that was
 *  only created, or only moved to a status. */
function batchSignature(c: FeedCard): string | null {
  if (c.subjects.length !== 1 || c.subjects[0].type !== "issue") return null;
  if (c.kind === "created") return "created";
  if (c.kind === "status" && c.rows.every((r) => r.kind === "status" || r.kind === "comment")) {
    return c.rows.some((r) => r.kind === "comment") ? null : `status:${c.lead.new_value}`;
  }
  return null;
}

/** Group feed rows (newest first, as the API returns them) into cards. */
export function buildCards(items: FeedItem[]): FeedCard[] {
  // Pass 1: rows → per-subject groups. Look back over a few open groups so
  // rows of one action that interleave with another subject still meet.
  type Group = { rows: FeedItem[]; subject: FeedSubject; last: number };
  const groups: Group[] = [];
  for (const item of items) {
    const subject = subjectOf(item);
    const t = seconds(item.ts);
    let target: Group | undefined;
    for (let i = groups.length - 1; i >= Math.max(0, groups.length - 8); i--) {
      const g = groups[i];
      const head = g.rows[0];
      if (Math.abs(g.last - t) > SAME_ACTION_SECONDS) break;
      if (
        g.subject.key === subject.key &&
        head.actor_user_id === item.actor_user_id &&
        head.transport === item.transport
      ) {
        target = g;
        break;
      }
    }
    if (target) {
      target.rows.push(item);
      target.last = t;
      mergeSubject(target.subject, subject);
    } else {
      groups.push({ rows: [item], subject, last: t });
    }
  }

  // Pass 2: consecutive same-shaped cards → one batch card.
  const cards: FeedCard[] = [];
  let prevSig: string | null = null;
  for (const g of groups) {
    const card = cardFrom(g.rows, g.subject);
    const sig = batchSignature(card);
    const prev = cards[cards.length - 1];
    if (
      prev &&
      sig !== null &&
      sig === prevSig &&
      prev.actorId === card.actorId &&
      prev.project === card.project &&
      Math.abs(seconds(prev.rows[prev.rows.length - 1].ts) - seconds(card.ts)) <= SAME_ACTION_SECONDS
    ) {
      prev.batch = true;
      prev.rows.push(...card.rows);
      prev.subjects.push(...card.subjects);
      prev.key = card.key;
      continue;
    }
    cards.push(card);
    prevSig = sig;
  }
  // Batches list their issues oldest first, the order they were made in.
  for (const c of cards) if (c.batch) c.subjects.reverse();
  return cards;
}

// ── Wording ──────────────────────────────────────────────

export const STATUS_LABELS: Record<string, string> = {
  backlog: "Backlog",
  todo: "Todo",
  active: "In progress",
  done: "Done",
  cancelled: "Cancelled",
};

export function statusLabel(s: string | null): string {
  return (s && STATUS_LABELS[s]) || s || "none";
}

export function humanField(field: string | null): string {
  switch (field) {
    case "target_date":
      return "due date";
    case "start_date":
      return "start date";
    case null:
      return "details";
    default:
      return field.replace(/_/g, " ");
  }
}

/** "blocks", "relates to", "waits on" from an audit field. */
export function relationLabel(field: string | null): string {
  return (field ?? "relates_to").replace(/_/g, " ");
}

/** Assignment values are "@username" or "human". */
export function assigneeLabel(v: string | null): string {
  if (!v) return "nobody";
  if (v === "human") return "a person";
  return v;
}

/** The short verb phrase leading a card, without its subject. */
export function verbFor(card: FeedCard): string {
  const a = card.lead;
  const n = card.subjects.length;
  switch (card.kind) {
    case "created":
      return card.batch ? `created ${n} issues` : "created";
    case "status":
      return card.batch ? `moved ${n} issues to` : "moved";
    case "comment":
      if (a.action === "update") return "edited a comment on";
      if (a.action === "delete") return "deleted a comment on";
      return "commented on";
    case "edit": {
      const fields = [...new Set(card.rows.filter((r) => r.kind === "edit").map((r) => humanField(r.field)))];
      return fields.length > 2 ? "edited" : `changed ${fields.join(" and ")} of`;
    }
    case "assignee":
      return a.action === "unassign" ? "unassigned" : "assigned";
    case "link":
      return a.action === "unlink" ? "unlinked" : "linked";
    case "label":
      return a.action === "detach" ? "removed a label from" : "labeled";
    case "page":
      if (a.action === "create") return "created page";
      if (a.action === "delete") return "deleted page";
      return "edited page";
    case "plan": {
      if (a.entity_type === "plan") {
        if (a.action === "create") return "created plan";
        return a.action === "delete" ? "deleted plan" : "updated plan";
      }
      const steps = card.rows.filter((r) => r.entity_type === "plan_step");
      if (steps.every((r) => r.field === "done")) return "completed a step in";
      if (steps.every((r) => r.action === "create")) {
        return steps.length === 1 ? "added a step to" : `added ${steps.length} steps to`;
      }
      return "updated steps in";
    }
    default: {
      const what = a.entity_type === "member" ? "member" : a.entity_type;
      if (a.action === "create") return `created ${what}`;
      if (a.action === "delete") return `deleted ${what}`;
      return `updated ${what}`;
    }
  }
}

/** One-line text without markdown, for previews. */
export function plainSnippet(v: string | null, max = 140): string {
  if (!v) return "";
  const flat = v
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/[*_`#>]+/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return flat.length > max ? flat.slice(0, max - 1) + "…" : flat;
}

export function dayLabel(ts: string, now = new Date()): string {
  const d = new Date(ts.replace(" ", "T") + "Z");
  if (d.toDateString() === now.toDateString()) return "Today";
  const yesterday = new Date(now.getTime() - 86_400_000);
  if (d.toDateString() === yesterday.toDateString()) return "Yesterday";
  return d.toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    ...(d.getFullYear() !== now.getFullYear() ? { year: "numeric" } : {}),
  });
}

export function initials(name: string): string {
  return (
    name
      .split(/[\s_-]+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? "")
      .join("") || "?"
  );
}

// ── Filters ──────────────────────────────────────────────

export type AgentMode = "all" | "only" | "exclude";

export interface FeedFilters {
  groups: number[];
  projects: number[];
  kinds: FeedKind[];
  actors: (number | "system")[];
  agents: AgentMode;
}

export const EMPTY_FILTERS: FeedFilters = {
  groups: [],
  projects: [],
  kinds: [],
  actors: [],
  agents: "all",
};

const STORAGE_KEY = "lific:feed:filters";

export function loadFilters(): FeedFilters {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const v = JSON.parse(raw) as Partial<FeedFilters>;
      const kinds = KIND_OPTIONS.map((o) => o.kind);
      return {
        groups: Array.isArray(v.groups) ? v.groups.filter((x) => typeof x === "number") : [],
        projects: Array.isArray(v.projects) ? v.projects.filter((x) => typeof x === "number") : [],
        kinds: Array.isArray(v.kinds) ? v.kinds.filter((k) => kinds.includes(k)) : [],
        actors: Array.isArray(v.actors)
          ? v.actors.filter((x) => typeof x === "number" || x === "system")
          : [],
        agents: v.agents === "only" || v.agents === "exclude" ? v.agents : "all",
      };
    }
  } catch {
    // private mode or a hand-edited value: start clean
  }
  return { ...EMPTY_FILTERS };
}

export function saveFilters(f: FeedFilters): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(f));
  } catch {
    // ignore
  }
}

export function activeFilterCount(f: FeedFilters): number {
  return (
    (f.groups.length + f.projects.length > 0 ? 1 : 0) +
    (f.kinds.length > 0 ? 1 : 0) +
    (f.actors.length > 0 ? 1 : 0) +
    (f.agents !== "all" ? 1 : 0)
  );
}

/** Project ids the filters select, or undefined for "every project".
 *  Groups and single projects add up. */
export function selectedProjectIds(
  f: FeedFilters,
  groups: { id: number; project_ids: number[] }[],
): number[] | undefined {
  if (f.groups.length === 0 && f.projects.length === 0) return undefined;
  const ids = new Set(f.projects);
  for (const g of groups) if (f.groups.includes(g.id)) for (const id of g.project_ids) ids.add(id);
  return [...ids];
}

export function toggle<T>(list: T[], value: T): T[] {
  return list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
}
