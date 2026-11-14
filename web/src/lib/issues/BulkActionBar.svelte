<script module lang="ts">
  // Exported from the module context so the parent can import the type
  // (instance-script exports aren't visible as module exports in Svelte 5).
  export type BulkMenu =
    | "status"
    | "priority"
    | "module"
    | "label"
    | "assign"
    | "delete"
    | "park"
    | null;
</script>

<script lang="ts">
  // LIF-149 floating bulk-action bar. Extracted from IssueList.svelte
  // (LIF-99). Appears while issues are selected; menus open upward.
  //
  // `bulkMenu` is bindable because the parent also mutates it (Escape key,
  // outside-click on window), so the open menu must stay in sync both ways.
  // All the actual mutations live in the parent and arrive as callbacks.
  import { t } from "../i18n";
  import type { Module, Label } from "../api";
  import { safeLabelColor } from "../labelColors";
  import { fly } from "svelte/transition";
  import { Trash2, X, Hourglass, Undo2 } from "lucide-svelte";
  import StatusIcon from "../StatusIcon.svelte";
  import PriorityIcon from "../PriorityIcon.svelte";
  import { STATUSES, PRIORITIES } from "./grouping";
  import AssigneePicker from "./AssigneePicker.svelte";
  import type { Person } from "./assignees";

  let {
    selectedCount,
    bulkBusy,
    bulkMenu = $bindable(),
    modules,
    labels,
    projectId = null,
    selectedAssignment = null,
    onAssign,
    onUpdate,
    onAddLabel,
    onDelete,
    parkedMode = false,
    onPark,
    onUnpark,
    onClear,
    onExport,
    error = "",
  }: {
    selectedCount: number;
    bulkBusy: boolean;
    bulkMenu: BulkMenu;
    modules: Module[];
    labels: Label[];
    /** LIF-147: the project the selection belongs to; null hides Assign. */
    projectId?: number | null;
    /** The selection's shared assignment, or null when it is mixed. */
    selectedAssignment?: string[] | null;
    /** Give every selected issue this assignment. */
    onAssign?: (names: string[], people: Person[]) => void;
    /** Apply a field update to every selected issue. */
    onUpdate: (input: Record<string, unknown>) => void;
    /** Add one label (by name) to every selected issue. */
    onAddLabel: (name: string) => void;
    /** Delete every selected issue. */
    onDelete: () => void;
    /** Fork: the list is in Parked mode; the bar brings issues back. */
    parkedMode?: boolean;
    /** Fork: park the selection with a reason. Absent hides the action. */
    onPark?: (reason: string) => void;
    /** Fork: bring the selection back to work. */
    onUnpark?: () => void;
    /** Clear the selection. */
    onClear: () => void;
    onExport: () => void;
    error?: string;
  } = $props();

  let parkReason = $state("");
</script>

<!-- The outer div owns viewport centering; the inner div owns the entrance
     fly so the two transforms don't fight.
     LIF-276: below sm the bar pins to both edges (inset-x-3) instead of
     centering a min-content row — six inline controls exceed a 360px
     viewport. The row wraps if it still can't fit, and bottom offset
     respects the home-indicator safe area. -->
<div
  class="fixed z-40 inset-x-3 bottom-[max(1rem,env(safe-area-inset-bottom))]
         sm:inset-x-auto sm:bottom-6 sm:left-1/2 sm:-translate-x-1/2"
>
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div
    class="flex flex-wrap items-center justify-center gap-0.5 gap-y-1 pl-3 pr-1.5 py-1.5
           bg-[var(--surface)] border border-[var(--border)]
           rounded-xl shadow-[0_8px_28px_rgba(0,0,0,0.18)]"
    in:fly={{ y: 8, duration: 180 }}
    onclick={(e) => e.stopPropagation()}
  >
    <span class="text-body-sm font-medium text-[var(--text)] tabular-nums pr-1">
      {t("issues.bulk.selected", { count: selectedCount })}
    </span>
    {#if bulkBusy}
      <span class="text-caption text-[var(--text-faint)] animate-pulse pr-1">
        {t("issues.bulk.working")}
      </span>
    {/if}
    <div class="w-px h-4 bg-[var(--border)] mx-1"></div>

    <button class="text-body-sm px-2 py-1 rounded-md text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] disabled:opacity-50"
      disabled={bulkBusy} onclick={onExport}>{t("issues.bulk.export")}</button>

    <!-- Fork: parked issues. In Parked mode the main action brings the
         selection back; elsewhere it sets it aside, with an optional reason. -->
    {#if parkedMode && onUnpark}
      <button
        class="flex items-center gap-1 text-body-sm font-medium px-2 py-1 rounded-md
               text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]
               disabled:opacity-50"
        data-testid="bulk-unpark"
        disabled={bulkBusy}
        onclick={onUnpark}
      >
        <Undo2 size={13} />
        {t("parked.action.unparkSelected")}
      </button>
    {:else if !parkedMode && onPark}
      <div class="relative">
        <button
          class="flex items-center gap-1 text-body-sm px-2 py-1 rounded-md transition-colors disabled:opacity-50
                 {bulkMenu === 'park'
            ? 'text-[var(--text)] bg-[var(--bg-subtle)]'
            : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
          data-testid="bulk-park"
          disabled={bulkBusy}
          onclick={(e) => {
            e.stopPropagation();
            bulkMenu = bulkMenu === "park" ? null : "park";
          }}
        >
          <Hourglass size={13} />
          {t("parked.action.park")}
        </button>
        {#if bulkMenu === "park"}
          <form
            class="absolute bottom-full mb-1.5 left-0 w-[260px] max-w-[calc(100vw-2rem)]
                   bg-[var(--surface)] border border-[var(--border)]
                   rounded-lg shadow-lg p-3 flex flex-col gap-2"
            onsubmit={(e) => {
              e.preventDefault();
              onPark(parkReason);
              parkReason = "";
            }}
          >
            <p class="text-caption text-[var(--text-muted)]">{t("parked.bulk.hint")}</p>
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="w-full text-body-sm px-2 py-1.5 rounded-md bg-[var(--bg)] border border-[var(--border)]
                     text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:border-[var(--accent)]"
              placeholder={t("parked.reasonPlaceholder")}
              maxlength="500"
              autofocus
              bind:value={parkReason}
              data-testid="bulk-park-reason"
            />
            <button
              type="submit"
              class="self-end text-body-sm font-medium px-3 py-1 rounded-md
                     text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]"
              data-testid="bulk-park-confirm"
            >
              {t("parked.action.parkCount", { count: selectedCount })}
            </button>
          </form>
        {/if}
      </div>
    {/if}

    {#snippet bulkTrigger(menu: "status" | "priority" | "module" | "label" | "assign", label: string)}
      <button
        class="text-body-sm px-2 py-1 rounded-md transition-colors
               disabled:opacity-50 disabled:cursor-not-allowed
               {bulkMenu === menu
          ? 'text-[var(--text)] bg-[var(--bg-subtle)]'
          : 'text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
        disabled={bulkBusy}
        onclick={(e) => {
          e.stopPropagation();
          bulkMenu = bulkMenu === menu ? null : menu;
        }}
      >
        {label}
      </button>
    {/snippet}

    <!-- Status -->
    <div class="relative">
      {@render bulkTrigger("status", t("issues.field.status"))}
      {#if bulkMenu === "status"}
        <div
          class="absolute bottom-full mb-1.5 left-0 w-[160px]
                 bg-[var(--surface)] border border-[var(--border)]
                 rounded-lg shadow-lg py-1.5"
        >
          {#each STATUSES as s}
            <button
              class="w-full flex items-center gap-2 px-3 py-1.5 text-left
                     text-body-sm text-[var(--text)]
                     hover:bg-[var(--bg-subtle)] transition-colors"
              onclick={() => onUpdate({ status: s })}
            >
              <StatusIcon status={s} size={14} />
              {t(`common.status.${s}`)}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Priority -->
    <div class="relative">
      {@render bulkTrigger("priority", t("issues.field.priority"))}
      {#if bulkMenu === "priority"}
        <div
          class="absolute bottom-full mb-1.5 left-0 w-[160px]
                 bg-[var(--surface)] border border-[var(--border)]
                 rounded-lg shadow-lg py-1.5"
        >
          {#each PRIORITIES as p}
            <button
              class="w-full flex items-center gap-2 px-3 py-1.5 text-left
                     text-body-sm text-[var(--text)]
                     hover:bg-[var(--bg-subtle)] transition-colors"
              onclick={() => onUpdate({ priority: p })}
            >
              <PriorityIcon priority={p} size={14} />
              {t(`common.priority.${p}`)}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Module (only when the project has modules) -->
    {#if modules.length > 0}
      <div class="relative">
        {@render bulkTrigger("module", t("issues.field.module"))}
        {#if bulkMenu === "module"}
          <div
            class="absolute bottom-full mb-1.5 left-0 w-[180px]
                   bg-[var(--surface)] border border-[var(--border)]
                   rounded-lg shadow-lg py-1.5 max-h-[40vh] overflow-y-auto"
          >
            <button
              class="w-full px-3 py-1.5 text-left text-body-sm
                     text-[var(--text-faint)] hover:bg-[var(--bg-subtle)]
                     transition-colors"
              onclick={() => onUpdate({ module_id: null })}
            >
              {t("common.none")}
            </button>
            {#each modules as mod (mod.id)}
              <button
                class="w-full px-3 py-1.5 text-left text-body-sm
                       text-[var(--text)] hover:bg-[var(--bg-subtle)]
                       transition-colors truncate"
                onclick={() => onUpdate({ module_id: mod.id })}
              >
                {mod.name}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Add label (only when the project has labels) -->
    {#if labels.length > 0}
      <div class="relative">
        {@render bulkTrigger("label", t("issues.field.label"))}
        {#if bulkMenu === "label"}
          <div
            class="absolute bottom-full mb-1.5 left-0 w-[180px]
                   bg-[var(--surface)] border border-[var(--border)]
                   rounded-lg shadow-lg py-1.5 max-h-[40vh] overflow-y-auto"
          >
            {#each labels as lbl (lbl.id)}
              <button
                class="w-full flex items-center gap-2 px-3 py-1.5 text-left
                       text-body-sm text-[var(--text)]
                       hover:bg-[var(--bg-subtle)] transition-colors"
                onclick={() => onAddLabel(lbl.name)}
              >
                <span
                  class="size-2.5 rounded-full shrink-0"
                  style="background: {safeLabelColor(lbl.color)}"
                ></span>
                <span class="truncate">{lbl.name}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Assign (LIF-147). Opens the same picker as the issue view. -->
    {#if projectId !== null && onAssign}
      {@render bulkTrigger("assign", t("issues.assignee.assign"))}
    {/if}

    <div class="w-px h-4 bg-[var(--border)] mx-1"></div>

    <!-- Delete (confirm popover) -->
    <div class="relative">
      <button
        class="size-7 flex items-center justify-center rounded-md
               text-[var(--error)] hover:bg-[var(--error-bg)]
               transition-colors disabled:opacity-50"
        title={t("issues.bulk.deleteSelected")}
        disabled={bulkBusy}
        onclick={(e) => {
          e.stopPropagation();
          bulkMenu = bulkMenu === "delete" ? null : "delete";
        }}
      >
        <Trash2 size={14} />
      </button>
      {#if bulkMenu === "delete"}
        <div
          class="absolute bottom-full mb-1.5 right-0 w-[240px]
                 bg-[var(--surface)] border border-[var(--border)]
                 rounded-lg shadow-lg p-3"
        >
          <p class="text-body-sm font-medium text-[var(--text)] mb-1">
            {t("issues.bulk.deleteConfirm", { count: selectedCount })}
          </p>
          <p class="text-caption text-[var(--text-muted)] mb-3">
            {t("issues.bulk.cantUndo")}
          </p>
          <div class="flex items-center gap-2">
            <button
              class="text-body-sm font-medium text-[var(--error-text)]
                     bg-[var(--error)] px-3 py-1.5 rounded-md
                     hover:opacity-90 transition-opacity
                     disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={bulkBusy}
              onclick={onDelete}
            >
              {bulkBusy ? t("issues.bulk.deleting") : t("common.delete")}
            </button>
            <button
              class="text-body-sm text-[var(--text-muted)] px-3 py-1.5
                     rounded-md hover:bg-[var(--bg-subtle)] transition-colors"
              onclick={() => { bulkMenu = null; }}
            >
              {t("common.cancel")}
            </button>
          </div>
        </div>
      {/if}
    </div>

    <!-- Clear -->
    <button
      class="size-7 flex items-center justify-center rounded-md
             text-[var(--text-muted)] hover:text-[var(--text)]
             hover:bg-[var(--bg-subtle)] transition-colors"
      title={t("issues.bulk.clear")}
      disabled={bulkBusy}
      onclick={onClear}
    >
      <X size={14} />
    </button>
    {#if error}
      <p role="alert" class="w-full max-w-lg px-2 text-caption text-[var(--error)]">{error}</p>
    {/if}
  </div>
</div>

{#if projectId !== null && onAssign}
  <AssigneePicker
    bind:open={() => bulkMenu === "assign", (next) => (bulkMenu = next ? "assign" : null)}
    {projectId}
    subject={t("issues.count", { count: selectedCount })}
    current={selectedAssignment}
    onApply={onAssign}
  />
{/if}
