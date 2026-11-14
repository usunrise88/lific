<script lang="ts">
  // LIF-157 — audit-log timeline for issue/page detail.
  //
  // A quiet, text-first history: each entry is one line of "who did what
  // via which door", newest first, with a hairline gutter rail tying the
  // entries together. Field changes render old → new; status/priority
  // reuse the shared icon vocabulary; long values (descriptions, page
  // content) collapse behind a details-style expander instead of flooding
  // the page. Collapsed to the latest few entries by default — history
  // should be available, not loud.

  import type { Activity } from "./api";
  import { formatDate } from "./format";
  import TimeAgo from "./TimeAgo.svelte";
  import StatusIcon from "./StatusIcon.svelte";
  import PriorityIcon from "./PriorityIcon.svelte";
  import { History, ChevronDown } from "lucide-svelte";
  import { assignActivity } from "./issues/assignees";
  import { relationLabel } from "./feed";
  import { t } from "./i18n";

  /** The entity's name in the active language; an unknown type stays as is. */
  function entityName(type: string): string {
    const key = `projectActivity.entity.${type}`;
    const name = t(key);
    return name === key ? type : name;
  }

  let {
    items,
    /** How many entries show before the "Show all" expander. */
    initialCount = 6,
  }: {
    items: Activity[];
    initialCount?: number;
  } = $props();

  let expanded = $state(false);
  let visible = $derived(expanded ? items : items.slice(0, initialCount));

  /** Per-entry expansion for long old→new values (description/content). */
  let openValues = $state<Set<number>>(new Set());

  function toggleValue(id: number) {
    const next = new Set(openValues);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    openValues = next;
  }

  function actorName(a: Activity): string {
    return a.actor_display_name || a.actor_username || t("activity.system");
  }

  /** Long-form fields get the expandable old/new treatment. */
  function isLongField(a: Activity): boolean {
    return a.field === "description" || a.field === "content";
  }

  function shortValue(v: string | null, max = 80): string {
    if (!v) return t("activity.none");
    const flat = v.replace(/\n+/g, " ").trim();
    if (!flat) return t("activity.none");
    return flat.length > max ? flat.slice(0, max) + "…" : flat;
  }

  /** Human verb for the entry, excluding the value rendering. */
  function verb(a: Activity): string {
    // ── Plans: phrase step/plan mutations as readable diffs, not raw
    //    "created this plan_step" noise (LIF-177 follow-up). ──
    if (a.entity_type === "plan_step") {
      switch (a.action) {
        case "create":
          return t("activity.step.added");
        case "delete":
          return t("activity.step.removed");
        case "auto-complete":
          return t("activity.step.autoCompleted");
        case "auto-reopen":
          return t("activity.step.autoReopened");
        case "update":
          if (a.field === "done")
            return a.new_value === "1" ? t("activity.step.completed") : t("activity.step.reopened");
          if (a.field === "title") return t("activity.step.renamed");
          if (a.field === "description") return t("activity.step.editedDescription");
          if (a.field === "issue")
            return a.new_value ? t("activity.step.linked") : t("activity.step.unlinked");
          return t("activity.step.changedField", { field: a.field ?? "" });
      }
    }
    if (a.entity_type === "plan") {
      switch (a.action) {
        case "create":
          return t("activity.plan.created");
        case "delete":
          return t("activity.plan.deleted");
        case "auto-archive":
          return t("activity.plan.autoArchived");
        case "update":
          if (a.field === "status") return t("activity.plan.setStatus");
          if (a.field === "title") return t("activity.plan.renamed");
          if (a.field === "anchor_issue")
            return a.new_value ? t("activity.plan.setAnchor") : t("activity.plan.clearedAnchor");
          return t("activity.changedField", { field: a.field ?? "" });
      }
    }

    switch (a.action) {
      case "create":
        if (a.entity_type === "comment") return t("activity.commented");
        if (a.entity_type === "issue") return t("activity.created.issue");
        if (a.entity_type === "page") return t("activity.created.page");
        return t("activity.created.other", { entity: entityName(a.entity_type) });
      case "delete":
        if (a.entity_type === "comment") return t("activity.deletedComment");
        if (a.entity_type === "issue") return t("activity.deleted.issue");
        if (a.entity_type === "page") return t("activity.deleted.page");
        return t("activity.deleted.other", { entity: entityName(a.entity_type) });
      case "update":
        return a.entity_type === "comment"
          ? t("activity.editedComment")
          : t("activity.changedField", { field: a.field ?? "" });
      case "attach":
        return t("activity.labelAdded");
      case "detach":
        return t("activity.labelRemoved");
      case "link":
        return t("activity.linked", { relation: relationLabel(a.field) });
      case "unlink":
        return t("activity.unlinked", { relation: relationLabel(a.field) });
      // LIF-484: user and date blockers. The value is the rendered wait.
      case "wait":
        return t("activity.waitStarted");
      case "unwait":
        return t("activity.waitStopped");
      // Fork: parked issues. The value is the reason.
      case "park":
        return t("parked.activity.parked");
      case "unpark":
        return t("parked.activity.unparked");
      // LIF-147: the value is "@username" or "human".
      case "assign":
      case "unassign":
        return assignActivity(a.action, a.action === "assign" ? a.new_value : a.old_value).verb;
      default:
        return a.action;
    }
  }

  /** Steps/plans carry a title in new_value (create) / old_value (delete);
   *  show it in quotes so "added step" reads as "added step “schema”". */
  const STATUSES = new Set(["backlog", "todo", "active", "done", "cancelled"]);
  const PRIORITIES = new Set(["urgent", "high", "medium", "low", "none"]);
  function statusLabel(v: string | null): string {
    return v && STATUSES.has(v) ? t(`common.status.${v}`) : (v ?? "");
  }
  function priorityLabel(v: string | null): string {
    return v && PRIORITIES.has(v) ? t(`common.priority.${v}`) : (v ?? "");
  }

  function titleValue(a: Activity): string | null {
    if (a.entity_type !== "plan_step" && a.entity_type !== "plan") return null;
    if (a.action === "create") return a.new_value;
    if (a.action === "delete") return a.old_value;
    return null;
  }
</script>

{#if items.length > 0}
  <section class="mt-10">
    <!-- Header: same uppercase-tracking vocabulary as the sidebar field
         labels and list group headers. -->
    <div class="flex items-center gap-2 mb-4 pb-2 border-b border-[var(--border)]">
      <History size={13} class="text-[var(--text-faint)]" />
      <h2
        class="text-micro font-semibold uppercase tracking-widest
               text-[var(--text-muted)]"
      >
        {t("activity.title")}
      </h2>
      <span class="text-micro text-[var(--text-faint)] tabular-nums">
        {items.length}
      </span>
    </div>

    <ol class="m-0 p-0 list-none relative">
      <!-- Gutter rail: ties entries into one history, mirroring the
           comment thread's connector vocabulary. -->
      <div
        class="absolute left-[3px] top-1.5 bottom-1.5 w-px bg-[var(--border)]"
        aria-hidden="true"
      ></div>

      {#each visible as a (a.id)}
        <li class="relative pl-5 pb-3 last:pb-0">
          <!-- Timeline dot -->
          <span
            class="absolute left-0 top-[0.4375rem] size-[7px] rounded-full
                   border border-[var(--border)] bg-[var(--surface)]"
            aria-hidden="true"
          ></span>

          <div class="text-body-sm leading-relaxed text-[var(--text-muted)]">
            <!-- Actor -->
            <span class="font-medium text-[var(--text)]">{actorName(a)}</span>
            {#if a.actor_is_bot}
              <span
                class="inline-block align-middle text-micro font-semibold
                       uppercase tracking-wider px-1 py-px rounded
                       bg-[var(--accent-subtle)] text-[var(--accent)] mx-0.5"
              >
                {t("activity.agent")}
              </span>
            {/if}

            <!-- Verb + values -->
            {verb(a)}
            {#if titleValue(a)}
              <span class="text-[var(--text-faint)] italic">“{shortValue(titleValue(a), 60)}”</span>
            {:else if a.action === "update" && a.field === "done"}
              <!-- verb already reads "completed/reopened a step" -->
            {:else if a.action === "update" && (a.field === "issue" || a.field === "anchor_issue")}
              <span class="font-mono text-caption text-[var(--accent)]">{a.new_value ?? a.old_value}</span>
            {:else if a.action === "update" && a.field === "status" && a.entity_type === "plan"}
              <span class="capitalize text-[var(--text)] mx-0.5">{a.new_value}</span>
            {:else if a.action === "update" && a.field === "status"}
              <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                <StatusIcon status={a.old_value ?? ""} size={12} />
                <span>{statusLabel(a.old_value)}</span>
              </span>
              <span class="text-[var(--text-faint)]">→</span>
              <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                <StatusIcon status={a.new_value ?? ""} size={12} />
                <span class="text-[var(--text)]">{statusLabel(a.new_value)}</span>
              </span>
            {:else if a.action === "update" && a.field === "priority"}
              <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                <PriorityIcon priority={a.old_value ?? "none"} size={12} />
                <span>{priorityLabel(a.old_value)}</span>
              </span>
              <span class="text-[var(--text-faint)]">→</span>
              <span class="inline-flex items-center gap-1 align-middle mx-0.5">
                <PriorityIcon priority={a.new_value ?? "none"} size={12} />
                <span class="text-[var(--text)]">{priorityLabel(a.new_value)}</span>
              </span>
            {:else if a.action === "update" && isLongField(a)}
              <button
                class="text-caption text-[var(--accent)] hover:underline
                       inline-flex items-center gap-0.5 align-baseline"
                onclick={() => toggleValue(a.id)}
              >
                {openValues.has(a.id) ? t("activity.hideChange") : t("activity.showChange")}
                <ChevronDown
                  size={11}
                  class="transition-transform {openValues.has(a.id) ? 'rotate-180' : ''}"
                />
              </button>
            {:else if a.action === "update"}
              <span class="text-[var(--text-faint)]">{shortValue(a.old_value, 40)}</span>
              <span class="text-[var(--text-faint)]">→</span>
              <span class="text-[var(--text)]">{shortValue(a.new_value, 40)}</span>
            {:else if a.action === "attach" || a.action === "detach"}
              <span
                class="text-micro font-medium px-1.5 py-0.5 rounded-full
                       border border-[var(--border)] align-middle"
              >
                {a.action === "attach" ? a.new_value : a.old_value}
              </span>
            {:else if a.action === "link" || a.action === "unlink"}
              <span class="font-mono text-caption text-[var(--accent)]">
                {a.action === "link" ? a.new_value : a.old_value}
              </span>
            {:else if a.action === "wait" || a.action === "unwait"}
              <span class="text-[var(--text)]">
                {shortValue(a.action === "wait" ? a.new_value : a.old_value, 60)}
              </span>
            {:else if a.action === "park" && a.new_value}
              <span class="text-[var(--text)]">{shortValue(a.new_value, 60)}</span>
            {:else if a.action === "assign" || a.action === "unassign"}
              {@const subject = assignActivity(a.action, a.action === "assign" ? a.new_value : a.old_value).subject}
              {#if subject}
                <span class="text-[var(--text)]">{subject}</span>
              {/if}
            {:else if a.action === "create" && a.entity_type === "comment"}
              <span class="text-[var(--text-faint)] italic">
                “{shortValue(a.new_value, 60)}”
              </span>
            {/if}

            <!-- Time + transport, quiet, at the end of the line -->
            <span
              class="text-caption text-[var(--text-faint)] whitespace-nowrap"
              title={t("activity.viaTitle", { date: formatDate(a.ts), transport: a.transport })}
            >
              · <TimeAgo date={a.ts} /> {t("activity.via", { transport: a.transport })}
            </span>
          </div>

          <!-- Expanded old/new blocks for description/content edits -->
          {#if a.action === "update" && isLongField(a) && openValues.has(a.id)}
            <div class="mt-2 mb-1 flex flex-col gap-1.5 max-w-[640px]">
              <div
                class="text-caption leading-relaxed px-3 py-2 rounded-md
                       border border-[var(--border)] bg-[var(--error-bg)]
                       text-[var(--text-muted)] whitespace-pre-wrap break-words
                       max-h-[200px] overflow-y-auto"
              >{a.old_value || t("activity.empty")}</div>
              <div
                class="text-caption leading-relaxed px-3 py-2 rounded-md
                       border border-[var(--border)] bg-[var(--success-bg)]
                       text-[var(--text)] whitespace-pre-wrap break-words
                       max-h-[200px] overflow-y-auto"
              >{a.new_value || t("activity.empty")}</div>
            </div>
          {/if}
        </li>
      {/each}
    </ol>

    {#if items.length > initialCount}
      <button
        class="mt-2 ml-5 text-caption text-[var(--text-muted)]
               hover:text-[var(--text)] inline-flex items-center gap-1
               transition-colors"
        onclick={() => { expanded = !expanded; }}
      >
        <ChevronDown
          size={12}
          class="transition-transform {expanded ? 'rotate-180' : ''}"
        />
        {expanded ? t("activity.showRecent") : t("activity.showAll", { count: items.length })}
      </button>
    {/if}
  </section>
{/if}
