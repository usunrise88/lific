<script lang="ts">
  // Fork: Home's scope, by project group and project, and the period the
  // flow reports cover. Every widget below reads the same scope.
  import { Check, ChevronDown, Folder, Layers, Search, X } from "lucide-svelte";
  import type { Project, ProjectGroup } from "../api";
  import ProjectIcon from "../ProjectIcon.svelte";
  import { t } from "../i18n";
  import { PERIODS, type HomeFilters, type Period } from "./flow";
  import { toggle } from "../feed";

  let {
    filters,
    groups,
    projects,
    onchange,
  }: {
    filters: HomeFilters;
    groups: ProjectGroup[];
    projects: Project[];
    onchange: (next: HomeFilters) => void;
  } = $props();

  let open = $state(false);
  let query = $state("");
  let root: HTMLDivElement | undefined = $state();

  let none = $derived(filters.groups.length === 0 && filters.projects.length === 0);
  let pickedProjects = $derived(projects.filter((p) => filters.projects.includes(p.id)));
  let shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q
      ? projects.filter((p) => p.name.toLowerCase().includes(q) || p.identifier.toLowerCase().includes(q))
      : projects;
  });

  function set(patch: Partial<HomeFilters>) {
    onchange({ ...filters, ...patch });
  }

  function onWindowClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }

  function onKey(e: KeyboardEvent) {
    if (open && e.key === "Escape") open = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={onKey} />

{#snippet chip(on: boolean, onclick: () => void, label: string, Icon?: typeof Folder, removable = false)}
  <button
    type="button"
    class="shrink-0 inline-flex items-center gap-1.5 text-caption h-7 px-2.5 rounded-full border transition-colors whitespace-nowrap
           {on
      ? 'border-[var(--accent)] bg-[var(--accent-subtle)] text-[var(--accent)] font-medium'
      : 'border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)] hover:border-[var(--text-faint)]'}"
    aria-pressed={on}
    {onclick}
  >
    {#if Icon}<Icon size={12} class="shrink-0" />{/if}
    {label}
    {#if removable}<X size={11} class="shrink-0 -mr-0.5 opacity-70" />{/if}
  </button>
{/snippet}

<div
  bind:this={root}
  class="relative flex flex-wrap items-center gap-x-3 gap-y-2 rounded-xl bg-[var(--surface)] border border-[var(--border)] px-3 py-2"
  data-testid="home-filters"
>
  <div class="flex flex-wrap items-center gap-1.5 min-w-0 flex-1">
    <Layers size={14} class="shrink-0 text-[var(--text-faint)] mr-0.5" />
    {@render chip(none, () => set({ groups: [], projects: [] }), t("flow.filter.all"))}
    {#each groups as g (g.id)}
      {@render chip(filters.groups.includes(g.id), () => set({ groups: toggle(filters.groups, g.id) }), g.name, Folder)}
    {/each}
    {#each pickedProjects as p (p.id)}
      {@render chip(true, () => set({ projects: toggle(filters.projects, p.id) }), p.identifier, undefined, true)}
    {/each}
    <button
      type="button"
      class="shrink-0 inline-flex items-center gap-1 text-caption h-7 px-2.5 rounded-full border border-dashed transition-colors
             border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)] hover:border-[var(--text-faint)]"
      aria-expanded={open}
      onclick={() => {
        open = !open;
        query = "";
      }}
    >
      {t("flow.filter.projects")}
      <ChevronDown size={12} class="transition-transform {open ? 'rotate-180' : ''}" />
    </button>
  </div>

  <div class="flex items-center gap-2 shrink-0 ml-auto">
    <span class="text-micro text-[var(--text-faint)] hidden sm:inline">{t("flow.filter.period")}</span>
    <div class="flex items-center p-0.5 rounded-lg bg-[var(--bg-subtle)]" role="radiogroup" aria-label={t("flow.filter.period")}>
      {#each PERIODS as d (d)}
        {@const on = filters.days === d}
        <button
          type="button"
          role="radio"
          aria-checked={on}
          class="text-caption tabular-nums h-6 px-2.5 rounded-md transition-colors
                 {on ? 'bg-[var(--surface)] text-[var(--text)] font-medium shadow-sm' : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
          onclick={() => set({ days: d as Period })}
        >
          {t("flow.filter.days", { count: d })}
        </button>
      {/each}
    </div>
  </div>

  {#if open}
    <div
      class="absolute left-2 right-2 sm:right-auto sm:w-[320px] top-full mt-1.5 z-30 rounded-lg bg-[var(--surface)] border border-[var(--border)]
             shadow-[0_8px_24px_rgba(0,0,0,0.18)] overflow-hidden"
    >
      <label class="flex items-center gap-2 px-3 py-2 border-b border-[var(--border)]">
        <Search size={13} class="text-[var(--text-faint)] shrink-0" />
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="flex-1 min-w-0 bg-transparent text-body-sm text-[var(--text)] outline-none placeholder:text-[var(--text-faint)]"
          placeholder={t("flow.filter.search")}
          bind:value={query}
          autofocus
        />
      </label>
      <div class="max-h-[300px] overflow-y-auto py-1">
        {#each shown as p (p.id)}
          {@const on = filters.projects.includes(p.id)}
          <button
            type="button"
            class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-body-sm transition-colors hover:bg-[var(--bg-subtle)]
                   {on ? 'text-[var(--accent)]' : 'text-[var(--text)]'}"
            onclick={() => set({ projects: toggle(filters.projects, p.id) })}
          >
            <span class="size-3.5 grid place-items-center shrink-0">{#if on}<Check size={13} />{/if}</span>
            <ProjectIcon value={p.emoji} size={14} class="shrink-0 text-[var(--text-muted)]" />
            <span class="flex-1 min-w-0 truncate">{p.name}</span>
            <span class="text-micro font-mono text-[var(--text-faint)]">{p.identifier}</span>
          </button>
        {:else}
          <p class="px-3 py-2 text-caption text-[var(--text-faint)]">{t("flow.filter.noProjects")}</p>
        {/each}
      </div>
    </div>
  {/if}
</div>
