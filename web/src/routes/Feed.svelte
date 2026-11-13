<script lang="ts">
  // Feed (fork): live, cross-project activity across every project the
  // reader can see. Rows from /api/feed are grouped into cards by lib/feed
  // (one card per action, batches for bulk agent work); filters run on the
  // server and persist per browser. New activity arrives over the realtime
  // socket, with a 15s poll as the fallback for events the socket does not
  // carry (pages, plans). Any issue or page card can be commented on in place.

  import { getContext, tick, untrack } from "svelte";
  import { fly } from "svelte/transition";
  import {
    listFeed,
    listFeedActors,
    listProjects,
    listProjectGroups,
    createComment,
    createPageComment,
    type FeedItem,
    type FeedParams,
    type Project,
    type ProjectGroup,
    type ActorStat,
  } from "../lib/api";
  import {
    buildCards,
    verbFor,
    statusLabel,
    humanField,
    relationLabel,
    assigneeLabel,
    plainSnippet,
    dayLabel,
    initials,
    loadFilters,
    saveFilters,
    activeFilterCount,
    selectedProjectIds,
    toggle,
    verbOptions,
    EMPTY_FILTERS,
    type FeedCard,
    type FeedFilters,
    type FeedSubject,
    type AgentMode,
    loadDensity,
    saveDensity,
    type Density,
  } from "../lib/feed";
  import { t, formatNumber } from "../lib/i18n";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { loadScroll, clearScroll, scrollSaver } from "../lib/scrollMemory";
  import { openPeek, peekState, dockPeek } from "../lib/issues/peek.svelte";
  import { openPagePeek } from "../lib/pages/pagePeek.svelte";
  import PeekPanel from "../lib/issues/PeekPanel.svelte";
  import { toast } from "../lib/toast/toast.svelte";
  import StatusIcon from "../lib/StatusIcon.svelte";
  import ProjectIcon from "../lib/ProjectIcon.svelte";
  import Markdown from "../lib/Markdown.svelte";
  import TimeAgo from "../lib/TimeAgo.svelte";
  import Skeleton from "../lib/Skeleton.svelte";
  import ErrorState from "../lib/ErrorState.svelte";
  import {
    CirclePlus, ArrowRightLeft, MessageSquare, Pencil, UserRound, Tag, Link2,
    FileText, ListChecks, Box, SlidersHorizontal, X, SendHorizontal,
    ArrowUpRight, ArrowUp, Bot, Users, Search, Activity as ActivityIcon,
    Rows3, Rows2, AtSign,
  } from "lucide-svelte";

  const PAGE_SIZE = 60;
  const NEW_LIMIT = 100;
  const BATCH_PREVIEW = 5;

  let { navigate }: { navigate: (path: string) => void } = $props();

  const topbarCtx = getContext<{
    set: (s: import("svelte").Snippet | undefined) => void;
  } | undefined>("lific:topbar");
  $effect(() => {
    topbarCtx?.set(topbarContent);
    return () => topbarCtx?.set(undefined);
  });

  // ── State ──────────────────────────────────────────────

  let filters = $state<FeedFilters>(loadFilters());
  let projects = $state<Project[]>([]);
  let groups = $state<ProjectGroup[]>([]);
  let actors = $state<ActorStat[]>([]);

  let items = $state<FeedItem[]>([]);
  /** Rows that arrived while the reader was scrolled down. */
  let pending = $state<FeedItem[]>([]);
  let fresh = $state<Set<number>>(new Set());
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state("");
  let request = 0;

  let scroller = $state<HTMLDivElement | null>(null);
  let sentinel = $state<HTMLDivElement | null>(null);
  let atTop = $state(true);

  let filtersOpen = $state(false);
  let projectQuery = $state("");

  let composerFor = $state<string | null>(null);
  let draft = $state("");
  let sending = $state(false);
  let expanded = $state<Set<string>>(new Set());

  // ── Data ───────────────────────────────────────────────

  function params(): FeedParams {
    return {
      projects: selectedProjectIds(filters, groups),
      verbs: filters.verbs.length ? filters.verbs : undefined,
      actors: filters.actors.length ? filters.actors : undefined,
      agents: filters.agents,
      mentioned: filters.mentioned || undefined,
    };
  }

  async function loadMeta() {
    const [p, g, a] = await Promise.all([listProjects(), listProjectGroups(), listFeedActors()]);
    if (p.ok) projects = p.data;
    if (g.ok) groups = g.data;
    if (a.ok) actors = a.data;
  }

  // Coming back to the Feed restores the rows and the scroll position it
  // was left at (per tab); a filter change starts from the top.
  const SCROLL_KEY = "feed";
  const MAX_RESTORE_ROWS = 1000;
  let firstLoad = true;
  const rememberScroll = scrollSaver(SCROLL_KEY, () =>
    scroller ? { top: scroller.scrollTop, count: items.length } : null,
  );

  async function load() {
    const token = ++request;
    loading = true;
    error = "";
    pending = [];
    const restore = firstLoad ? loadScroll(SCROLL_KEY) : null;
    if (!firstLoad) clearScroll(SCROLL_KEY);
    firstLoad = false;
    const want = restore ? Math.min(Math.max(restore.count, PAGE_SIZE), MAX_RESTORE_ROWS) : PAGE_SIZE;
    const res = await listFeed({ ...params(), limit: Math.min(want, 200) });
    if (token !== request) return;
    if (!res.ok) {
      error = res.error;
      loading = false;
      return;
    }
    items = res.data.items;
    hasMore = res.data.has_more;
    while (restore && hasMore && items.length < want) {
      const more = await listFeed({
        ...params(),
        before: items[items.length - 1].id,
        limit: Math.min(200, want - items.length),
      });
      if (token !== request || !more.ok) break;
      items = [...items, ...more.data.items];
      hasMore = more.data.has_more;
    }
    loading = false;
    await tick();
    if (scroller) {
      // A restored visit goes back to its place; new filters start at the top.
      scroller.scrollTop = restore ? restore.top : 0;
      atTop = scroller.scrollTop < 120;
    }
  }

  async function loadMore() {
    if (loadingMore || !hasMore || items.length === 0) return;
    const token = request;
    loadingMore = true;
    const res = await listFeed({ ...params(), before: items[items.length - 1].id, limit: PAGE_SIZE });
    if (token === request && res.ok) {
      const known = new Set(items.map((i) => i.id));
      items = [...items, ...res.data.items.filter((i) => !known.has(i.id))];
      hasMore = res.data.has_more;
    }
    loadingMore = false;
  }

  /** Fetch what is newer than the newest row on screen. */
  async function refreshNew() {
    if (loading) return;
    const newest = pending[0]?.id ?? items[0]?.id;
    if (newest === undefined) return load();
    const token = request;
    const res = await listFeed({ ...params(), after: newest, limit: NEW_LIMIT });
    if (token !== request || !res.ok || res.data.items.length === 0) {
      if (res.ok) void refreshActors();
      return;
    }
    if (res.data.has_more) {
      // Too much happened to splice in; start over from the top.
      if (atTop) return load();
      pending = [...res.data.items, ...pending];
      return;
    }
    if (atTop) showNew(res.data.items);
    else pending = [...res.data.items, ...pending];
    void refreshActors();
  }

  async function refreshActors() {
    const a = await listFeedActors();
    if (a.ok) actors = a.data;
  }

  function showNew(rows: FeedItem[]) {
    const known = new Set(items.map((i) => i.id));
    const add = rows.filter((r) => !known.has(r.id));
    if (add.length === 0) return;
    items = [...add, ...items];
    const ids = new Set(fresh);
    for (const r of add) ids.add(r.id);
    fresh = ids;
    setTimeout(() => {
      const next = new Set(fresh);
      for (const r of add) next.delete(r.id);
      fresh = next;
    }, 4000);
  }

  async function revealPending() {
    const rows = pending;
    pending = [];
    showNew(rows);
    await tick();
    scroller?.scrollTo({ top: 0, behavior: "smooth" });
  }

  $effect(() => {
    void loadMeta();
  });

  // Reload whenever a filter changes (and once on mount).
  $effect(() => {
    void JSON.stringify(filters);
    // Group membership only matters once a group is selected.
    if (filters.groups.length) void groups;
    saveFilters($state.snapshot(filters) as FeedFilters);
    // Only the filters above trigger a reload: `load` itself reads groups
    // and the list, and tracking those would reload (and forget the saved
    // scroll position) whenever the group list arrives.
    untrack(() => void load());
  });

  $effect(() =>
    startAutoRefresh({
      refresh: refreshNew,
      intervalMs: 15_000,
      isBusy: () => loading || loadingMore,
      shouldRefresh: (event) =>
        event.type === "resync.required" || typeof event.project_id === "number",
      realtimeDebounceMs: 300,
      realtimeMaxWaitMs: 2000,
    }),
  );

  // On wide screens the issue preview takes the filter rail's place instead
  // of sliding over the feed; narrower screens keep the usual overlay.
  $effect(() => {
    const wide = window.matchMedia("(min-width: 1024px)");
    let undock: (() => void) | null = null;
    const sync = () => {
      if (wide.matches && !undock) undock = dockPeek();
      else if (!wide.matches && undock) {
        undock();
        undock = null;
      }
    };
    sync();
    wide.addEventListener("change", sync);
    return () => {
      wide.removeEventListener("change", sync);
      undock?.();
    };
  });

  let previewing = $derived(peekState.docked && peekState.open);

  function isPreviewed(card: FeedCard): boolean {
    return previewing && !card.batch && card.subjects[0].label === peekState.identifier;
  }

  // Infinite scroll.
  $effect(() => {
    if (!sentinel || !scroller) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) void loadMore();
      },
      { root: scroller, rootMargin: "600px" },
    );
    io.observe(sentinel);
    return () => io.disconnect();
  });

  function onScroll() {
    if (!scroller) return;
    rememberScroll();
    atTop = scroller.scrollTop < 120;
    if (atTop && pending.length) void revealPending();
  }

  // ── Derived view ───────────────────────────────────────

  let cards = $derived(buildCards(items));

  let days = $derived.by(() => {
    const out: { label: string; cards: FeedCard[] }[] = [];
    for (const c of cards) {
      const label = dayLabel(c.ts);
      if (out.length === 0 || out[out.length - 1].label !== label) out.push({ label, cards: [] });
      out[out.length - 1].cards.push(c);
    }
    return out;
  });

  let filterCount = $derived(activeFilterCount(filters));

  // Compact rows or full cards, per browser like the filters.
  let density = $state<Density>(loadDensity());
  function setDensity(d: Density) {
    density = d;
    saveDensity(d);
  }

  let shownProjects = $derived(
    projectQuery.trim()
      ? projects.filter((p) =>
          `${p.identifier} ${p.name}`.toLowerCase().includes(projectQuery.trim().toLowerCase()),
        )
      : projects,
  );

  let projectsById = $derived(new Map(projects.map((p) => [p.id, p])));

  // ── Actions ────────────────────────────────────────────

  function setFilters(patch: Partial<FeedFilters>) {
    filters = { ...filters, ...patch };
  }

  function resetFilters() {
    filters = { ...EMPTY_FILTERS };
  }

  function onlyProject(identifier: string | null) {
    const p = projects.find((x) => x.identifier === identifier);
    if (p) setFilters({ projects: [p.id], groups: [] });
  }

  function onlyActor(card: FeedCard) {
    setFilters({ actors: [card.actorId ?? "system"] });
  }

  function openSubject(s: FeedSubject) {
    if (s.type === "issue" && s.label) openPeek(s.label);
    else if (s.href) navigate(s.href);
  }

  // A click anywhere on a row or card previews its item (on wide screens in
  // the right column). Its own controls (author, project, links, buttons)
  // keep their meaning, and selecting text does not count as a click.
  function onRowClick(e: MouseEvent, card: FeedCard) {
    const target = e.target as HTMLElement;
    if (target.closest("button, a, textarea, input, [data-row-ignore]")) return;
    if (window.getSelection()?.toString()) return;
    previewCard(card);
  }

  function onRowKey(e: KeyboardEvent, card: FeedCard) {
    if (e.key === "Enter" && e.target === e.currentTarget) previewCard(card);
  }

  function previewCard(card: FeedCard) {
    const s = card.subjects[0];
    if (s.type === "issue" && s.label) openPeek(s.label);
    else if (s.type === "page" && s.pageId !== null && s.href) openPagePeek(s.pageId, s.href);
    else if (s.href) navigate(s.href);
  }

  function commentTarget(card: FeedCard): FeedSubject | null {
    if (card.batch) return null;
    const s = card.subjects[0];
    return s.issueId !== null || s.pageId !== null ? s : null;
  }

  async function openComposer(card: FeedCard) {
    if (composerFor !== card.key) draft = "";
    composerFor = composerFor === card.key ? null : card.key;
    await tick();
    document.querySelector<HTMLTextAreaElement>(`[data-composer="${card.key}"]`)?.focus();
  }

  async function sendComment(card: FeedCard) {
    const target = commentTarget(card);
    const body = draft.trim();
    if (!target || !body || sending) return;
    sending = true;
    const res = target.issueId !== null
      ? await createComment(target.issueId, body)
      : await createPageComment(target.pageId!, body);
    sending = false;
    if (!res.ok) {
      toast(t("feed.toast.postFailed", { error: res.error }));
      return;
    }
    draft = "";
    composerFor = null;
    toast(t("feed.toast.posted", { target: target.label ?? t("feed.toast.page") }));
    void refreshNew();
  }

  function composerKey(e: KeyboardEvent, card: FeedCard) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void sendComment(card);
    } else if (e.key === "Escape") {
      composerFor = null;
    }
  }

  function autoGrow(node: HTMLTextAreaElement) {
    const fit = () => {
      node.style.height = "auto";
      node.style.height = Math.min(node.scrollHeight, 320) + "px";
    };
    node.addEventListener("input", fit);
    fit();
    return { destroy: () => node.removeEventListener("input", fit) };
  }

  function toggleExpanded(key: string) {
    const next = new Set(expanded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expanded = next;
  }

</script>

<!-- The action, as a badge in its verb's color, then the words after it. -->
{#snippet phrase(card: FeedCard)}
  {@const ph = verbFor(card)}
  <span class="feed-verb verb-{ph.verb}">{ph.word}</span>{#if ph.rest}{" "}{ph.rest}{/if}
{/snippet}

<!-- What follows the phrase: the item, and for a move its new status. -->
{#snippet subjectTail(card: FeedCard, joiner: string)}
  {@const s = card.subjects[0]}
  {@const verb = card.lead.verb}
  {#if card.batch}
    {#if verb === "moved"}{@render statusChip(card.lead.new_value)}{/if}
  {:else if verb === "moved"}
    {@render subjectRef(s, false)} {joiner} {@render statusChip(card.lead.new_value)}
  {:else if verb === "assigned"}
    {@render subjectRef(s, false)} {t("feed.tail.assignedTo")} <span class="text-[var(--text)]">{assigneeLabel(card.lead.new_value ?? card.lead.old_value)}</span>
  {:else if card.kind === "structure"}
    <span class="font-medium text-[var(--text)]">{card.lead.entity_label ?? ""}</span>
  {:else}
    {@render subjectRef(s, false)}
  {/if}
{/snippet}

{#snippet kindIcon(kind: string, size: number)}
  {#if kind === "created"}<CirclePlus {size} />
  {:else if kind === "status"}<ArrowRightLeft {size} />
  {:else if kind === "comment"}<MessageSquare {size} />
  {:else if kind === "edit"}<Pencil {size} />
  {:else if kind === "assignee"}<UserRound {size} />
  {:else if kind === "label"}<Tag {size} />
  {:else if kind === "link"}<Link2 {size} />
  {:else if kind === "page"}<FileText {size} />
  {:else if kind === "plan"}<ListChecks {size} />
  {:else}<Box {size} />{/if}
{/snippet}

{#snippet statusChip(status: string | null)}
  <span class="font-medium text-[var(--text)] whitespace-nowrap">
    <span class="inline-block align-[-2px] ml-0.5 mr-1"><StatusIcon status={status ?? ""} size={12} /></span>{statusLabel(status)}
  </span>
{/snippet}

{#snippet subjectRef(s: FeedSubject, withTitle: boolean)}
  {#if s.label}
    <button
      class="font-mono text-caption font-medium text-[var(--accent)] hover:underline align-baseline"
      onclick={() => openSubject(s)}
    >{s.label}</button>
  {/if}
  {#if withTitle && s.title}
    <button
      class="text-left text-[var(--text)] font-medium hover:underline decoration-[var(--border)] underline-offset-2"
      onclick={() => openSubject(s)}
    >{s.title}</button>
  {/if}
{/snippet}

{#snippet inlineDetails(card: FeedCard)}
  <!-- Second line of a row: what the action touched, in one line. -->
  {@const labels = card.rows.filter((r) => r.kind === "label")}
  {@const assigns = card.rows.filter((r) => r.kind === "assignee")}
  {@const links = card.rows.filter((r) => r.kind === "link")}
  {@const edits = card.rows.filter((r) => r.kind === "edit")}
  <!-- Only added steps carry a title; a completed step's value is just "1". -->
  {@const steps = card.rows.filter((r) => r.entity_type === "plan_step" && r.action === "create")}
  {@const s = card.subjects[0]}
  {#if card.batch}
    {@const shown = card.subjects.slice(0, BATCH_PREVIEW)}
    {#each shown as b, i (b.key)}{#if i}<span class="text-[var(--text-faint)]">,&nbsp;</span>{/if}<button
        class="font-mono text-caption text-[var(--accent)] hover:underline"
        title={b.title ?? undefined}
        onclick={() => openSubject(b)}
      >{b.label}</button>{/each}
    {#if card.subjects.length > shown.length}
      <span class="text-[var(--text-faint)]"> +{card.subjects.length - shown.length}</span>
    {/if}
  {:else}
    {#if s.title && card.kind !== "structure"}
      <!-- Buttons always lay out as inline-block, so the title truncates
           itself; clipped by the line instead, a long one vanished whole. -->
      <button
        class="max-w-[calc(100%-1.75rem)] truncate align-bottom text-left text-[var(--text)] hover:underline decoration-[var(--border)] underline-offset-2"
        onclick={() => openSubject(s)}
      >{s.title}</button>
    {:else if card.kind === "structure"}
      {@const v = card.lead.new_value ?? card.lead.old_value}
      <span class="text-[var(--text-muted)]">{v && v !== card.lead.entity_label ? plainSnippet(v, 160) : ""}</span>
    {/if}
    {#each edits as e (e.id)}
      <span class="text-[var(--text-faint)]"> · {humanField(e.field)}</span>
      {#if e.field !== "description" && e.field !== "content"}
        <span class="text-[var(--text-faint)] line-through">{plainSnippet(e.old_value, 30) || t("feed.value.none")}</span>
        <span class="text-[var(--text-faint)]">→</span>
        <span class="text-[var(--text-muted)]">{plainSnippet(e.new_value, 40) || t("feed.value.none")}</span>
      {/if}
    {/each}
    {#each labels as l (l.id)}
      <span class="ml-1 inline-flex items-center text-micro px-1.5 rounded-full border border-[var(--border)] text-[var(--text-muted)] align-[1px] {l.action === 'detach' ? 'line-through' : ''}">{l.action === "detach" ? l.old_value : l.new_value}</span>
    {/each}
    {#each assigns as a (a.id)}
      <span class="ml-1.5 inline-flex items-center gap-0.5 text-micro text-[var(--text-muted)] align-[1px]"><UserRound size={10} />{a.action === "unassign" ? `− ${assigneeLabel(a.old_value)}` : assigneeLabel(a.new_value)}</span>
    {/each}
    {#each links as l (l.id)}
      {@const target = l.action === "unlink" ? l.old_value : l.new_value}
      <span class="ml-1.5 inline-flex items-center gap-0.5 text-micro text-[var(--text-muted)] align-[1px] {l.action === 'unlink' ? 'line-through' : ''}">
        <Link2 size={10} />{relationLabel(l.field)}
        {#if target}<button class="font-mono text-[var(--accent)] hover:underline" onclick={() => openPeek(target)}>{target}</button>{/if}
      </span>
    {/each}
    {#if card.kind === "plan" && steps.length}
      <span class="text-[var(--text-faint)]"> · {t("feed.details.steps", { count: steps.length })}{card.lead.entity_type === "plan_step" ? `: ${steps.map((x) => plainSnippet(x.new_value, 40)).filter(Boolean).slice(0, 2).join(", ")}` : ""}</span>
    {/if}
  {/if}
{/snippet}

{#snippet commentPreview(card: FeedCard)}
  {#each card.rows.filter((r) => r.kind === "comment" && r.action !== "delete") as c (c.id)}
    {@const key = `${card.key}:${c.id}`}
    {#if expanded.has(key)}
      <div class="feed-comment mt-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)] px-3 py-2 text-body-sm">
        <Markdown content={c.new_value ?? ""} />
        <button class="mt-1 text-micro text-[var(--text-faint)] hover:text-[var(--text)]" onclick={() => toggleExpanded(key)}>{t("feed.comment.collapse")}</button>
      </div>
    {:else}
      <button
        class="mt-1 w-full text-left border-l-2 border-[var(--border)] hover:border-[var(--accent)] pl-2
               text-caption leading-snug text-[var(--text-muted)] line-clamp-2 transition-colors"
        title={t("feed.comment.showFullTitle")}
        onclick={() => toggleExpanded(key)}
      >{plainSnippet(c.new_value, 400)}</button>
    {/if}
  {/each}
{/snippet}

{#snippet compactRow(card: FeedCard)}
  {@const s = card.subjects[0]}
  {@const target = commentTarget(card)}
  {@const isFresh = card.rows.some((r) => fresh.has(r.id))}
  <!-- A row opens its item, so it takes focus and Enter like a control;
       the controls inside it stay real buttons. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <article
    tabindex="0"
    onclick={(e) => onRowClick(e, card)}
    onkeydown={(e) => onRowKey(e, card)}
    class="feed-row group relative flex gap-2.5 px-3 py-2 transition-colors cursor-pointer outline-none
           focus-visible:bg-[var(--bg-subtle)]
           {isPreviewed(card) ? 'bg-[var(--accent-subtle)]' : 'hover:bg-[var(--bg-subtle)]'}
           {isFresh ? 'feed-fresh' : ''}"
    in:fly={{ y: -4, duration: 200 }}
  >
    <!-- Avatar with the event kind tucked into its corner. -->
    <button class="relative shrink-0 self-start mt-0.5" title={t("feed.onlyThis", { name: card.actorName })} onclick={() => onlyActor(card)}>
      <span
        class="size-6 rounded-full grid place-items-center text-[9px] font-bold select-none
               {card.isBot
          ? 'bg-[var(--accent-subtle)] text-[var(--accent)] border border-[var(--accent)]'
          : 'bg-[var(--accent)] text-[var(--accent-text)]'}"
      >{initials(card.actorName)}</span>
      <span
        class="verb-{card.lead.verb} absolute -bottom-0.5 -right-1 size-3.5 rounded-full grid place-items-center
               bg-[var(--surface)] ring-1 ring-[var(--border)]"
        style="color: var(--vc)"
      >{@render kindIcon(card.kind, 8)}</span>
    </button>

    <div class="flex-1 min-w-0">
      <!-- Line 1: who did what to which item, plus where and when. -->
      <div class="flex items-start gap-2 min-w-0 min-h-6 pt-0.5">
        <!-- Phones get up to two lines here so the item and status survive;
             wider screens keep one truncated line. -->
        <p class="flex-1 min-w-0 m-0 line-clamp-2 sm:line-clamp-none sm:truncate text-body-sm leading-5 text-[var(--text-muted)]">
          <button class="font-medium text-[var(--text)] hover:underline" onclick={() => onlyActor(card)}>{card.actorName}</button>
          {#if card.isBot}<span class="hidden sm:inline"><Bot size={11} class="inline -mt-0.5 text-[var(--accent)]" aria-label={t("feed.agent")} /></span>{/if}
          {@render phrase(card)}
          {@render subjectTail(card, "→")}
        </p>
        <div class="shrink-0 flex items-center gap-1.5 -mt-0.5">
          <span class="feed-actions flex items-center">
            {#if target}
              <button
                class="size-6 grid place-items-center rounded-md transition-colors
                       {composerFor === card.key ? 'text-[var(--accent)] bg-[var(--accent-subtle)]' : 'text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--surface)]'}"
                title={t("feed.comment.action")} aria-label={t("feed.comment.action")}
                onclick={() => openComposer(card)}
              ><MessageSquare size={13} /></button>
            {/if}
            {#if !card.batch && s.href}
              <button
                class="hidden sm:grid size-6 place-items-center rounded-md text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--surface)] transition-colors"
                title={t("common.open")} aria-label={t("common.open")}
                onclick={() => navigate(s.href!)}
              ><ArrowUpRight size={13} /></button>
            {/if}
          </span>
          {#if card.project}
            <button
              class="hidden sm:inline text-micro font-mono text-[var(--text-faint)] hover:text-[var(--text)]"
              title={t("feed.onlyThis", { name: card.projectName ?? card.project })}
              onclick={() => onlyProject(card.project)}
            >{card.project}</button>
          {/if}
          <TimeAgo class="text-micro text-[var(--text-faint)] tabular-nums" date={card.ts} />
        </div>
      </div>

      <!-- Line 2: the item itself and what changed on it. -->
      <p class="m-0 truncate text-body-sm leading-5">
        {#if s.type === "issue" && s.status && !card.batch}
          <span class="inline-block align-[-2px] mr-1"><StatusIcon status={s.status} size={12} /></span>
        {/if}
        {@render inlineDetails(card)}
      </p>

      {@render commentPreview(card)}

      {#if composerFor === card.key && target}
        <div class="mt-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg)] focus-within:border-[var(--accent)] transition-colors" transition:fly={{ y: -4, duration: 150 }}>
          <textarea
            data-composer={card.key}
            class="block w-full resize-none bg-transparent px-2.5 py-1.5 text-body-sm text-[var(--text)]
                   placeholder:text-[var(--text-faint)] outline-none min-h-[52px]"
            placeholder={t("feed.composer.placeholder", { target: target.label ?? t("feed.composer.thisPage") })}
            bind:value={draft}
            use:autoGrow
            onkeydown={(e) => composerKey(e, card)}
            disabled={sending}
          ></textarea>
          <div class="flex items-center justify-end gap-1.5 px-1.5 pb-1.5">
            <span class="hidden sm:inline mr-auto pl-1 text-micro text-[var(--text-faint)]">⌘/Ctrl + Enter · Esc</span>
            <button
              class="text-caption px-2 py-0.5 rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
              onclick={() => (composerFor = null)}
            >{t("common.cancel")}</button>
            <button
              class="inline-flex items-center gap-1 text-caption font-medium px-2 py-0.5 rounded-md
                     text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]
                     disabled:opacity-50 transition-colors"
              disabled={!draft.trim() || sending}
              onclick={() => sendComment(card)}
            ><SendHorizontal size={11} />{sending ? t("common.sending") : t("common.send")}</button>
          </div>
        </div>
      {/if}
    </div>
  </article>
{/snippet}

<!-- Full density: roomy cards with whole comments and labeled actions. -->
{#snippet chip(text: string, tone: "plain" | "add" | "remove" = "plain")}
  <span
    class="inline-flex items-center text-micro font-medium px-1.5 py-0.5 rounded-full border
           {tone === 'remove'
      ? 'border-[var(--border)] text-[var(--text-faint)] line-through'
      : 'border-[var(--border)] text-[var(--text-muted)]'}"
  >{tone === "add" ? "+ " : ""}{text}</span>
{/snippet}

{#snippet fullDetails(card: FeedCard)}
  {@const comments = card.rows.filter((r) => r.kind === "comment" && r.action !== "delete")}
  {@const labels = card.rows.filter((r) => r.kind === "label")}
  {@const assigns = card.rows.filter((r) => r.kind === "assignee")}
  {@const links = card.rows.filter((r) => r.kind === "link")}
  {@const edits = card.rows.filter((r) => r.kind === "edit")}
  {@const steps = card.rows.filter((r) => r.entity_type === "plan_step" && r.action === "create")}
  {@const isOpen = expanded.has(card.key)}

  {#if card.batch}
    {@const list = isOpen ? card.subjects : card.subjects.slice(0, BATCH_PREVIEW)}
    <ul class="mt-2 flex flex-col gap-1 m-0 p-0 list-none">
      {#each list as s (s.key)}
        <li class="flex items-center gap-2 min-w-0 text-body-sm">
          <StatusIcon status={s.status ?? ""} size={12} />
          {@render subjectRef(s, false)}
          <button
            class="truncate text-left text-[var(--text-muted)] hover:text-[var(--text)]"
            onclick={() => openSubject(s)}
          >{s.title ?? ""}</button>
        </li>
      {/each}
    </ul>
    {#if card.subjects.length > BATCH_PREVIEW}
      <button class="mt-1 text-caption text-[var(--text-faint)] hover:text-[var(--text)]" onclick={() => toggleExpanded(card.key)}>
        {isOpen ? t("common.showLess") : t("feed.batch.showAll", { count: card.subjects.length })}
      </button>
    {/if}
  {/if}

  {#if edits.length}
    <div class="mt-2 flex flex-col gap-1">
      {#each edits as e (e.id)}
        {#if e.field === "description" || e.field === "content"}
          <p class="m-0 text-caption text-[var(--text-muted)] line-clamp-2">
            <span class="text-[var(--text-faint)]">{humanField(e.field)}:</span> {plainSnippet(e.new_value, 220) || t("feed.value.cleared")}
          </p>
        {:else if e.field === "priority" || e.field === "title" || e.field === "module" || e.field?.endsWith("date")}
          <p class="m-0 text-caption text-[var(--text-muted)] break-words">
            <span class="text-[var(--text-faint)]">{humanField(e.field)}:</span>
            <span class="line-through decoration-[var(--text-faint)]">{plainSnippet(e.old_value, 80) || t("feed.value.none")}</span>
            <span class="text-[var(--text-faint)]">→</span>
            <span class="text-[var(--text)]">{plainSnippet(e.new_value, 80) || t("feed.value.none")}</span>
          </p>
        {/if}
      {/each}
    </div>
  {/if}

  {#if labels.length || assigns.length || links.length}
    <div class="mt-2 flex flex-wrap items-center gap-1.5">
      {#each labels as l (l.id)}
        {@render chip((l.action === "detach" ? l.old_value : l.new_value) ?? "", l.action === "detach" ? "remove" : card.kind === "created" ? "plain" : "add")}
      {/each}
      {#each assigns as a (a.id)}
        <span class="inline-flex items-center gap-1 text-micro text-[var(--text-muted)]">
          <UserRound size={11} />{a.action === "unassign" ? `− ${assigneeLabel(a.old_value)}` : assigneeLabel(a.new_value)}
        </span>
      {/each}
      {#each links as l (l.id)}
        {@const target = l.action === "unlink" ? l.old_value : l.new_value}
        <span class="inline-flex items-center gap-1 text-micro text-[var(--text-muted)] {l.action === 'unlink' ? 'line-through' : ''}">
          <Link2 size={11} />{relationLabel(l.field)}
          {#if target}
            <button class="font-mono text-[var(--accent)] hover:underline" onclick={() => openPeek(target)}>{target}</button>
          {/if}
        </span>
      {/each}
    </div>
  {/if}

  {#if steps.length && card.lead.entity_type === "plan"}
    <p class="mt-2 m-0 text-caption text-[var(--text-muted)]">
      {t("feed.details.steps", { count: steps.length })}
      {#if steps.length}· {steps.slice(-3).reverse().map((s) => plainSnippet(s.new_value, 40)).join(" · ")}{steps.length > 3 ? " …" : ""}{/if}
    </p>
  {:else if card.kind === "plan" && card.lead.entity_type === "plan_step"}
    <p class="mt-2 m-0 text-caption text-[var(--text-muted)] line-clamp-2">
      {card.rows.filter((r) => r.entity_type === "plan_step" && r.action === "create").map((s) => plainSnippet(s.new_value, 60)).filter(Boolean).join(" · ")}
    </p>
  {/if}

  {#if card.kind === "structure"}
    {@const v = card.lead.new_value ?? card.lead.old_value}
    {#if v && v !== card.lead.entity_label}
      <p class="mt-1.5 m-0 text-caption text-[var(--text-muted)] line-clamp-2">{plainSnippet(v, 200)}</p>
    {/if}
  {/if}

  {#each comments as c (c.id)}
    {@const long = (c.new_value ?? "").length > 420}
    {@const showAll = expanded.has(`${card.key}:${c.id}`)}
    <div
      class="feed-comment mt-2.5 relative rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)]
             px-3 py-2 text-body-sm {long && !showAll ? 'max-h-44 overflow-hidden' : ''}"
    >
      <Markdown content={c.new_value ?? ""} />
      {#if long && !showAll}
        <div class="absolute inset-x-0 bottom-0 h-12 bg-gradient-to-t from-[var(--bg-subtle)] to-transparent pointer-events-none"></div>
      {/if}
    </div>
    {#if long}
      <button class="mt-1 text-caption text-[var(--text-faint)] hover:text-[var(--text)]" onclick={() => toggleExpanded(`${card.key}:${c.id}`)}>
        {showAll ? t("common.showLess") : t("feed.comment.showFull")}
      </button>
    {/if}
  {/each}
{/snippet}

{#snippet fullCard(card: FeedCard)}
  {@const s = card.subjects[0]}
  {@const target = commentTarget(card)}
  {@const isFresh = card.rows.some((r) => fresh.has(r.id))}
  <!-- A row opens its item, so it takes focus and Enter like a control;
       the controls inside it stay real buttons. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <article
    tabindex="0"
    onclick={(e) => onRowClick(e, card)}
    onkeydown={(e) => onRowKey(e, card)}
    class="feed-row feed-card relative flex gap-3 px-3.5 sm:px-4 py-3.5 rounded-xl border bg-[var(--surface)]
           transition-colors cursor-pointer outline-none focus-visible:border-[var(--accent)]
           {isFresh || isPreviewed(card) ? 'feed-fresh border-[var(--accent)]' : 'border-[var(--border)] hover:border-[color-mix(in_oklab,var(--text-faint)_60%,var(--border))]'}"
    in:fly={{ y: -6, duration: 220 }}
  >
    <!-- Avatar with the event kind tucked into its corner. -->
    <button class="relative shrink-0 self-start" title={t("feed.onlyThis", { name: card.actorName })} onclick={() => onlyActor(card)}>
      <span
        class="size-8 rounded-full grid place-items-center text-micro font-bold select-none
               {card.isBot
          ? 'bg-[var(--accent-subtle)] text-[var(--accent)] border border-[var(--accent)]'
          : 'bg-[var(--accent)] text-[var(--accent-text)]'}"
      >{initials(card.actorName)}</span>
      <span
        class="verb-{card.lead.verb} absolute -bottom-1 -right-1 size-[18px] rounded-full grid place-items-center
               bg-[var(--surface)] border border-[var(--border)]"
        style="color: var(--vc)"
      >{@render kindIcon(card.kind, 10)}</span>
    </button>

    <div class="flex-1 min-w-0">
      <header class="flex items-center gap-1.5 min-w-0 text-body-sm">
        <button class="font-semibold text-[var(--text)] truncate hover:underline" onclick={() => onlyActor(card)}>
          {card.actorName}
        </button>
        {#if card.isBot}
          <span class="shrink-0 text-micro font-semibold uppercase tracking-wider px-1 py-px rounded bg-[var(--accent-subtle)] text-[var(--accent)]">{t("feed.agent")}</span>
        {/if}
        <span class="hidden sm:inline shrink-0 text-micro text-[var(--text-faint)]">{t("feed.via", { transport: card.transport })}</span>
        <span class="ml-auto shrink-0 flex items-center gap-2">
          {#if card.project}
            <button
              class="text-micro font-mono font-medium text-[var(--text-muted)] px-1.5 py-0.5 rounded
                     bg-[var(--bg-subtle)] hover:text-[var(--text)]"
              title={t("feed.onlyThis", { name: card.projectName ?? card.project })}
              onclick={() => onlyProject(card.project)}
            >{card.project}</button>
          {/if}
          <TimeAgo class="text-micro text-[var(--text-faint)] tabular-nums" date={card.ts} />
        </span>
      </header>

      <p class="mt-1 m-0 text-body-sm leading-relaxed text-[var(--text-muted)] break-words">
        {@render phrase(card)}
        {@render subjectTail(card, t("feed.tail.movedTo"))}
        {#if card.lead.field === "status" && !card.batch && card.lead.old_value}
          <span class="text-micro text-[var(--text-faint)]">{t("feed.fromStatus", { status: statusLabel(card.lead.old_value) })}</span>
        {/if}
      </p>

      {#if !card.batch && s.title && card.kind !== "structure"}
        <p class="mt-0.5 m-0 flex items-start gap-1.5 text-body leading-snug min-w-0">
          {#if s.type === "issue" && s.status}
            <span class="mt-[3px] shrink-0"><StatusIcon status={s.status} size={13} /></span>
          {/if}
          {@render subjectRef({ ...s, label: null }, true)}
        </p>
      {/if}

      {@render fullDetails(card)}

      <footer class="mt-2.5 flex items-center gap-1 -ml-1.5">
        {#if target}
          <button
            class="inline-flex items-center gap-1.5 text-caption px-1.5 py-1 rounded-md transition-colors
                   {composerFor === card.key ? 'text-[var(--text)] bg-[var(--bg-subtle)]' : 'text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
            onclick={() => openComposer(card)}
          ><MessageSquare size={13} />{t("feed.comment.action")}</button>
        {/if}
        {#if !card.batch && s.href}
          <button
            class="inline-flex items-center gap-1 text-caption px-1.5 py-1 rounded-md text-[var(--text-faint)]
                   hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
            onclick={() => navigate(s.href!)}
          >{t("common.open")}<ArrowUpRight size={12} /></button>
        {/if}
      </footer>

      {#if composerFor === card.key && target}
        <div class="mt-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg)] focus-within:border-[var(--accent)] transition-colors" transition:fly={{ y: -4, duration: 150 }}>
          <textarea
            data-composer={card.key}
            class="block w-full resize-none bg-transparent px-3 py-2 text-body-sm text-[var(--text)]
                   placeholder:text-[var(--text-faint)] outline-none min-h-[64px]"
            placeholder={t("feed.composer.placeholder", { target: target.label ?? t("feed.composer.thisPage") })}
            bind:value={draft}
            use:autoGrow
            onkeydown={(e) => composerKey(e, card)}
            disabled={sending}
          ></textarea>
          <div class="flex items-center justify-end gap-2 px-2 pb-2">
            <span class="hidden sm:inline mr-auto pl-1 text-micro text-[var(--text-faint)]">{t("feed.composer.hint")}</span>
            <button
              class="text-caption px-2.5 py-1 rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
              onclick={() => (composerFor = null)}
            >{t("common.cancel")}</button>
            <button
              class="inline-flex items-center gap-1.5 text-caption font-medium px-2.5 py-1 rounded-md
                     text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]
                     disabled:opacity-50 transition-colors"
              disabled={!draft.trim() || sending}
              onclick={() => sendComment(card)}
            ><SendHorizontal size={12} />{sending ? t("common.sending") : t("common.send")}</button>
          </div>
        </div>
      {/if}
    </div>
  </article>
{/snippet}


{#snippet sectionLabel(text: string, extra?: string)}
  <div class="flex items-center gap-2 mb-2">
    <span class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]">{text}</span>
    {#if extra}<span class="text-micro text-[var(--text-faint)] tabular-nums">{extra}</span>{/if}
  </div>
{/snippet}

{#snippet filterPanel()}
  <div class="flex flex-col gap-6">
    <!-- Who -->
    <section>
      {@render sectionLabel(t("feed.filter.who"))}
      <div class="grid grid-cols-3 p-0.5 rounded-lg bg-[var(--bg-subtle)] border border-[var(--border)] text-caption">
        {#each [["all", t("feed.filter.everyone")], ["only", t("feed.filter.agents")], ["exclude", t("feed.filter.people")]] as [mode, label] (mode)}
          <button
            class="py-1 rounded-md transition-colors inline-flex items-center justify-center gap-1
                   {filters.agents === mode ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm font-medium' : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
            onclick={() => setFilters({ agents: mode as AgentMode })}
          >
            {#if mode === "only"}<Bot size={12} />{:else if mode === "exclude"}<Users size={12} />{/if}{label}
          </button>
        {/each}
      </div>
      {#if actors.length}
        <div class="mt-2 flex flex-col gap-0.5">
          {#each actors as a (a.actor_user_id ?? "system")}
            {@const key = a.actor_user_id ?? "system"}
            {@const on = filters.actors.includes(key)}
            {@const name = a.display_name || a.username || t("feed.system")}
            <button
              class="flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors
                     {on ? 'bg-[var(--accent-subtle)]' : 'hover:bg-[var(--bg-subtle)]'}"
              onclick={() => setFilters({ actors: toggle(filters.actors, key) })}
            >
              <span
                class="size-5 rounded-full grid place-items-center text-[9px] font-bold shrink-0
                       {a.is_bot ? 'bg-[var(--accent-subtle)] text-[var(--accent)] border border-[var(--accent)]' : 'bg-[var(--accent)] text-[var(--accent-text)]'}"
              >{initials(name)}</span>
              <span class="flex-1 min-w-0 truncate text-body-sm {on ? 'text-[var(--accent)] font-medium' : 'text-[var(--text)]'}">{name}</span>
              <span class="text-micro text-[var(--text-faint)] tabular-nums">{formatNumber(a.actions)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Actions, in their badge colors. -->
    <section>
      {@render sectionLabel(t("feed.filter.actions"))}
      <div class="flex flex-wrap gap-1.5">
        {#each verbOptions() as o (o.verb)}
          {@const on = filters.verbs.includes(o.verb)}
          <button
            class="feed-verb-chip verb-{o.verb} inline-flex items-center gap-1.5 text-caption px-2 py-1 rounded-full border transition-colors"
            class:on
            aria-pressed={on}
            onclick={() => setFilters({ verbs: toggle(filters.verbs, o.verb) })}
          >
            <span class="feed-verb-dot"></span>{o.label}
          </button>
        {/each}
      </div>
    </section>

    {#if groups.length}
      <section>
        {@render sectionLabel(t("feed.filter.groups"))}
        <div class="flex flex-wrap gap-1.5">
          {#each groups as g (g.id)}
            {@const on = filters.groups.includes(g.id)}
            <button
              class="text-caption px-2 py-1 rounded-full border transition-colors
                     {on ? 'border-[var(--accent)] bg-[var(--accent-subtle)] text-[var(--accent)] font-medium' : 'border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)]'}"
              onclick={() => setFilters({ groups: toggle(filters.groups, g.id) })}
            >{g.name}<span class="ml-1 text-[var(--text-faint)] tabular-nums">{g.project_ids.length}</span></button>
          {/each}
        </div>
      </section>
    {/if}

    <section>
      {@render sectionLabel(t("feed.filter.projects"), filters.projects.length ? t("feed.filter.selected", { count: filters.projects.length }) : undefined)}
      {#if projects.length > 8}
        <label class="mb-1.5 flex items-center gap-2 px-2 py-1 rounded-md border border-[var(--border)] focus-within:border-[var(--accent)]">
          <Search size={12} class="text-[var(--text-faint)]" />
          <input class="flex-1 min-w-0 bg-transparent outline-none text-body-sm" placeholder={t("feed.filter.findProject")} bind:value={projectQuery} />
        </label>
      {/if}
      <div class="flex flex-col gap-0.5">
        {#each shownProjects as p (p.id)}
          {@const on = filters.projects.includes(p.id)}
          <button
            class="flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors
                   {on ? 'bg-[var(--accent-subtle)]' : 'hover:bg-[var(--bg-subtle)]'}"
            onclick={() => setFilters({ projects: toggle(filters.projects, p.id) })}
          >
            <ProjectIcon value={p.emoji} size={14} />
            <span class="flex-1 min-w-0 truncate text-body-sm {on ? 'text-[var(--accent)] font-medium' : 'text-[var(--text)]'}">{p.name}</span>
            <span class="text-micro font-mono text-[var(--text-faint)]">{p.identifier}</span>
          </button>
        {/each}
      </div>
    </section>

    {#if filterCount}
      <button class="self-start text-caption text-[var(--text-muted)] hover:text-[var(--text)] inline-flex items-center gap-1" onclick={resetFilters}>
        <X size={12} />{t("feed.filter.reset")}
      </button>
    {/if}
  </div>
{/snippet}

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-4 sm:px-6 py-2 w-full">
    <!-- The phone header already names the page. -->
    <span class="hidden sm:inline text-body-sm font-medium text-[var(--text)]">{t("feed.title")}</span>
    <span class="inline-flex items-center gap-1.5 text-micro text-[var(--text-faint)]" title={t("feed.live.title")}>
      <span class="relative flex size-2">
        <span class="absolute inline-flex size-full rounded-full bg-[var(--success)] opacity-60 animate-ping"></span>
        <span class="relative inline-flex size-2 rounded-full bg-[var(--success)]"></span>
      </span>
      {t("feed.live.label")}
    </span>
    <div class="ml-auto flex items-center gap-2">
      <!-- Fork: only comments that mention me. -->
      <button
        class="inline-flex items-center gap-1 text-caption px-1.5 sm:px-2 py-1 rounded-md border transition-colors
               {filters.mentioned
          ? 'border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-subtle)] font-medium'
          : 'border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)]'}"
        title={t("feed.mentioned.title")}
        aria-pressed={filters.mentioned}
        data-testid="feed-mentioned"
        onclick={() => setFilters({ mentioned: !filters.mentioned })}
      ><AtSign size={12} /><span class="hidden sm:inline">{t("feed.mentioned.label")}</span></button>
      <div class="flex p-0.5 rounded-md bg-[var(--bg-subtle)] border border-[var(--border)]" role="group" aria-label={t("feed.density.label")}>
        {#each [["compact", t("feed.density.compact"), t("feed.density.compactTitle")], ["full", t("feed.density.full"), t("feed.density.fullTitle")]] as [d, label, hint] (d)}
          <button
            class="inline-flex items-center gap-1 text-caption px-1.5 sm:px-2 py-0.5 rounded transition-colors
                   {density === d ? 'bg-[var(--surface)] text-[var(--text)] shadow-sm font-medium' : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
            title={hint}
            aria-pressed={density === d}
            onclick={() => setDensity(d as Density)}
          >{#if d === "compact"}<Rows3 size={12} />{:else}<Rows2 size={12} />{/if}<span class="hidden sm:inline">{label}</span></button>
        {/each}
      </div>
      {#if filterCount}
        <button class="hidden lg:inline-flex items-center gap-1 text-caption text-[var(--text-muted)] hover:text-[var(--text)]" onclick={resetFilters}>
          <X size={12} />{t("common.reset")}
        </button>
      {/if}
      <button
        class="lg:hidden inline-flex items-center gap-1.5 text-caption px-2 py-1 rounded-md border transition-colors
               {filtersOpen || filterCount ? 'border-[var(--accent)] text-[var(--accent)]' : 'border-[var(--border)] text-[var(--text-muted)]'}"
        onclick={() => (filtersOpen = !filtersOpen)}
      ><SlidersHorizontal size={12} />{t("feed.filter.button")}{filterCount ? ` · ${filterCount}` : ""}</button>
    </div>
  </div>
{/snippet}

<div class="h-full flex flex-col">
  <div class="flex-1 overflow-y-auto relative" bind:this={scroller} onscroll={onScroll}>
    {#if pending.length}
      <div class="sticky top-3 z-20 flex justify-center pointer-events-none">
        <button
          class="pointer-events-auto inline-flex items-center gap-1.5 text-caption font-medium px-3 py-1.5 rounded-full
                 bg-[var(--accent)] text-[var(--accent-text)] shadow-lg"
          onclick={revealPending}
          transition:fly={{ y: -8, duration: 180 }}
        ><ArrowUp size={12} />{t("feed.newRows", { count: pending.length })}</button>
      </div>
    {/if}

    <div class="flex flex-col lg:flex-row gap-8 px-3 sm:px-6 lg:px-8 py-5 max-w-[1180px] mx-auto items-start">
      {#if filtersOpen}
        <div class="lg:hidden w-full rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4" transition:fly={{ y: -6, duration: 150 }}>
          {@render filterPanel()}
        </div>
      {/if}

      <div class="flex-1 min-w-0 w-full max-w-[720px] mx-auto">
        {#if loading}
          <div class="flex flex-col rounded-xl border border-[var(--border)] divide-y divide-[var(--border)]">
            {#each Array(8) as _, i (i)}
              <div class="flex gap-2.5 px-3 py-2.5">
                <Skeleton variant="circle" class="size-6 shrink-0" />
                <div class="flex-1 flex flex-col gap-1.5 pt-1">
                  <Skeleton variant="bar" class="h-3 w-2/3" />
                  <Skeleton variant="bar" class="h-3 w-1/2" />
                </div>
              </div>
            {/each}
          </div>
        {:else if error}
          <ErrorState title={t("feed.error.title")} message={error}>
            <button
              class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
              onclick={load}
            >{t("common.retry")}</button>
          </ErrorState>
        {:else if cards.length === 0}
          <div class="flex flex-col items-center py-20 gap-3 px-6 max-w-[440px] mx-auto text-center">
            <ActivityIcon size={32} class="text-[var(--text-faint)]" />
            {#if filters.mentioned && filterCount === 1}
              <p class="text-body-lg text-[var(--text-muted)] m-0">{t("feed.mentioned.empty")}</p>
              <button class="text-caption text-[var(--accent)] hover:underline" onclick={() => setFilters({ mentioned: false })}>{t("feed.mentioned.off")}</button>
            {:else if filterCount}
              <p class="text-body-lg text-[var(--text-muted)] m-0">{t("feed.empty.filtered")}</p>
              <button class="text-caption text-[var(--accent)] hover:underline" onclick={resetFilters}>{t("feed.filter.reset")}</button>
            {:else}
              <p class="text-body-lg text-[var(--text-muted)] m-0">{t("feed.empty.title")}</p>
              <p class="text-body-sm text-[var(--text-faint)] m-0">{t("feed.empty.body")}</p>
            {/if}
          </div>
        {:else}
          {#each days as day (day.label)}
            <section class="mb-4 last:mb-0">
              <div class="sticky top-0 z-10 -mx-1 px-1 py-1.5 bg-[var(--bg)] flex items-center gap-2">
                <span class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]">{day.label}</span>
                <span class="text-micro text-[var(--text-faint)] tabular-nums">{day.cards.length}</span>
                <div class="flex-1 h-px bg-[var(--border)]"></div>
              </div>
              {#if density === "full"}
                <div class="flex flex-col gap-2.5">
                  {#each day.cards as card (card.key)}
                    {@render fullCard(card)}
                  {/each}
                </div>
              {:else}
              <div class="flex flex-col rounded-xl border border-[var(--border)] bg-[var(--surface)] divide-y divide-[var(--border)] overflow-hidden">
                {#each day.cards as card (card.key)}
                  {@render compactRow(card)}
                {/each}
              </div>
              {/if}
            </section>
          {/each}
          <div bind:this={sentinel} class="h-px"></div>
          {#if loadingMore}
            <p class="text-center text-caption text-[var(--text-faint)] py-4 m-0">{t("common.loading")}</p>
          {:else if !hasMore}
            <p class="text-center text-caption text-[var(--text-faint)] py-6 m-0">{t("feed.end")}</p>
          {/if}
        {/if}
      </div>

      {#if previewing}
        <!-- The issue preview, in place of the filters (closing it brings
             them back). -->
        <aside class="hidden lg:block w-[400px] xl:w-[440px] shrink-0 sticky top-5 h-[calc(100vh-7rem)]">
          <PeekPanel {navigate} embedded />
        </aside>
      {:else}
      <aside class="feed-rail hidden lg:block w-[272px] shrink-0 sticky top-5 max-h-[calc(100vh-7rem)] overflow-y-auto pb-6">
        {@render filterPanel()}
      </aside>
      {/if}
    </div>
  </div>
</div>

<style>
  /* New rows: a brief accent bar on the left edge. */
  .feed-fresh::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--accent);
    animation: feed-fresh 4s ease-out forwards;
  }
  @keyframes feed-fresh {
    0%, 60% { opacity: 1; }
    100% { opacity: 0; }
  }
  /* Row actions appear on hover with a mouse; touch screens always see them. */
  @media (hover: hover) and (pointer: fine) {
    .feed-actions { opacity: 0; transition: opacity 120ms; }
    :global(.feed-row:hover) .feed-actions,
    .feed-actions:focus-within { opacity: 1; }
  }
  /* The filter rail still scrolls when it outgrows the screen, but without
     a scrollbar beside it. */
  .feed-rail { scrollbar-width: none; }
  .feed-rail::-webkit-scrollbar { display: none; }
  /* Action colors (validated as a categorical set, light and dark, in the
     filter's order: every neighbouring pair stays apart for normal and
     color-blind vision). The verb is always spelled out, so color only
     helps scanning. `cancelled` is deliberately neutral. */
  .verb-created { --vc: #2171cc; }
  .verb-commented { --vc: #b74d00; }
  .verb-moved { --vc: #8156c0; }
  .verb-completed { --vc: #1c882d; }
  .verb-cancelled { --vc: #6f6f6f; }
  .verb-assigned { --vc: #af4387; }
  .verb-edited { --vc: #8f6d00; }
  .verb-labeled { --vc: #008d7e; }
  .verb-deleted { --vc: #bd413f; }
  .verb-linked { --vc: #0083b3; }
  :global(.dark) .verb-created { --vc: #5591dd; }
  :global(.dark) .verb-commented { --vc: #cd753f; }
  :global(.dark) .verb-moved { --vc: #9c7bd2; }
  :global(.dark) .verb-completed { --vc: #57a45b; }
  :global(.dark) .verb-cancelled { --vc: #9a9a9a; }
  :global(.dark) .verb-assigned { --vc: #c66ca1; }
  :global(.dark) .verb-edited { --vc: #a88d10; }
  :global(.dark) .verb-labeled { --vc: #00a89a; }
  :global(.dark) .verb-deleted { --vc: #d36c66; }
  :global(.dark) .verb-linked { --vc: #00a0c7; }

  /* The verb badge: tinted pill, text in the verb's color nudged toward the
     body ink so small type keeps its contrast. */
  .feed-verb {
    display: inline-block;
    padding: 0 0.375rem;
    border-radius: 0.375rem;
    font-weight: 600;
    font-size: var(--text-caption);
    line-height: 1.25rem;
    color: color-mix(in oklab, var(--vc) 82%, var(--text));
    background: color-mix(in oklab, var(--vc) 13%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--vc) 26%, transparent);
  }

  .feed-verb-chip {
    border-color: var(--border);
    color: var(--text-muted);
  }
  .feed-verb-chip:hover { color: var(--text); border-color: color-mix(in oklab, var(--vc) 45%, var(--border)); }
  .feed-verb-chip.on {
    font-weight: 500;
    color: color-mix(in oklab, var(--vc) 82%, var(--text));
    background: color-mix(in oklab, var(--vc) 13%, transparent);
    border-color: color-mix(in oklab, var(--vc) 45%, transparent);
  }
  .feed-verb-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 9999px;
    background: var(--vc);
  }

  .feed-comment :global(p:first-child) { margin-top: 0; }
  .feed-comment :global(p:last-child) { margin-bottom: 0; }
</style>
