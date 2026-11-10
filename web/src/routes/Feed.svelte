<script lang="ts">
  // Feed (fork): live, cross-project activity across every project the
  // reader can see. Rows from /api/feed are grouped into cards by lib/feed
  // (one card per action, batches for bulk agent work); filters run on the
  // server and persist per browser. New activity arrives over the realtime
  // socket, with a 15s poll as the fallback for events the socket does not
  // carry (pages, plans). Any issue or page card can be commented on in place.

  import { getContext, tick } from "svelte";
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
    KIND_OPTIONS,
    EMPTY_FILTERS,
    type FeedCard,
    type FeedFilters,
    type FeedSubject,
    type AgentMode,
  } from "../lib/feed";
  import { startAutoRefresh } from "../lib/autoRefresh.svelte";
  import { openPeek } from "../lib/issues/peek.svelte";
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
      kinds: filters.kinds.length ? filters.kinds : undefined,
      actors: filters.actors.length ? filters.actors : undefined,
      agents: filters.agents,
    };
  }

  async function loadMeta() {
    const [p, g, a] = await Promise.all([listProjects(), listProjectGroups(), listFeedActors()]);
    if (p.ok) projects = p.data;
    if (g.ok) groups = g.data;
    if (a.ok) actors = a.data;
  }

  async function load() {
    const token = ++request;
    loading = true;
    error = "";
    pending = [];
    const res = await listFeed({ ...params(), limit: PAGE_SIZE });
    if (token !== request) return;
    if (res.ok) {
      items = res.data.items;
      hasMore = res.data.has_more;
    } else {
      error = res.error;
    }
    loading = false;
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
    void load();
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
      toast(`Couldn't post comment: ${res.error}`);
      return;
    }
    draft = "";
    composerFor = null;
    toast(`Comment posted on ${target.label ?? "page"}`);
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

  const KIND_TONE: Record<string, string> = {
    created: "var(--success)",
    status: "var(--accent)",
    comment: "var(--color-indigo-500)",
    edit: "var(--text-muted)",
    assignee: "var(--color-orange-400)",
    label: "var(--text-muted)",
    link: "var(--text-muted)",
    page: "var(--text-muted)",
    plan: "var(--accent)",
    structure: "var(--text-faint)",
  };
</script>

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
  <span class="inline-flex items-center gap-1 align-middle whitespace-nowrap font-medium text-[var(--text)]">
    <StatusIcon status={status ?? ""} size={12} />{statusLabel(status)}
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

{#snippet chip(text: string, tone: "plain" | "add" | "remove" = "plain")}
  <span
    class="inline-flex items-center text-micro font-medium px-1.5 py-0.5 rounded-full border
           {tone === 'remove'
      ? 'border-[var(--border)] text-[var(--text-faint)] line-through'
      : 'border-[var(--border)] text-[var(--text-muted)]'}"
  >{tone === "add" ? "+ " : ""}{text}</span>
{/snippet}

{#snippet details(card: FeedCard)}
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
        {isOpen ? "Show less" : `Show all ${card.subjects.length}`}
      </button>
    {/if}
  {/if}

  {#if edits.length}
    <div class="mt-2 flex flex-col gap-1">
      {#each edits as e (e.id)}
        {#if e.field === "description" || e.field === "content"}
          <p class="m-0 text-caption text-[var(--text-muted)] line-clamp-2">
            <span class="text-[var(--text-faint)]">{humanField(e.field)}:</span> {plainSnippet(e.new_value, 220) || "(cleared)"}
          </p>
        {:else if e.field === "priority" || e.field === "title" || e.field === "module" || e.field?.endsWith("date")}
          <p class="m-0 text-caption text-[var(--text-muted)] break-words">
            <span class="text-[var(--text-faint)]">{humanField(e.field)}:</span>
            <span class="line-through decoration-[var(--text-faint)]">{plainSnippet(e.old_value, 80) || "none"}</span>
            <span class="text-[var(--text-faint)]">→</span>
            <span class="text-[var(--text)]">{plainSnippet(e.new_value, 80) || "none"}</span>
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
      {steps.length} step{steps.length === 1 ? "" : "s"}
      {#if steps.length}· {steps.slice(-3).reverse().map((s) => plainSnippet(s.new_value, 40)).join(" · ")}{steps.length > 3 ? " …" : ""}{/if}
    </p>
  {:else if card.kind === "plan" && card.lead.entity_type === "plan_step"}
    <p class="mt-2 m-0 text-caption text-[var(--text-muted)] line-clamp-2">
      {card.rows.filter((r) => r.entity_type === "plan_step").map((s) => plainSnippet(s.new_value, 60)).filter(Boolean).join(" · ")}
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
        {showAll ? "Show less" : "Show full comment"}
      </button>
    {/if}
  {/each}
{/snippet}

{#snippet feedCard(card: FeedCard)}
  {@const s = card.subjects[0]}
  {@const target = commentTarget(card)}
  {@const isFresh = card.rows.some((r) => fresh.has(r.id))}
  <article
    class="feed-card relative flex gap-3 px-3.5 sm:px-4 py-3.5 rounded-xl border bg-[var(--surface)]
           transition-colors {isFresh ? 'feed-fresh border-[var(--accent)]' : 'border-[var(--border)]'}"
    in:fly={{ y: -6, duration: 220 }}
  >
    <!-- Avatar with the event kind tucked into its corner. -->
    <button class="relative shrink-0 self-start" title="Only {card.actorName}" onclick={() => onlyActor(card)}>
      <span
        class="size-8 rounded-full grid place-items-center text-micro font-bold select-none
               {card.isBot
          ? 'bg-[var(--accent-subtle)] text-[var(--accent)] border border-[var(--accent)]'
          : 'bg-[var(--accent)] text-[var(--accent-text)]'}"
      >{initials(card.actorName)}</span>
      <span
        class="absolute -bottom-1 -right-1 size-[18px] rounded-full grid place-items-center
               bg-[var(--surface)] border border-[var(--border)]"
        style="color: {KIND_TONE[card.kind]}"
      >{@render kindIcon(card.kind, 10)}</span>
    </button>

    <div class="flex-1 min-w-0">
      <header class="flex items-center gap-1.5 min-w-0 text-body-sm">
        <button class="font-semibold text-[var(--text)] truncate hover:underline" onclick={() => onlyActor(card)}>
          {card.actorName}
        </button>
        {#if card.isBot}
          <span class="shrink-0 text-micro font-semibold uppercase tracking-wider px-1 py-px rounded bg-[var(--accent-subtle)] text-[var(--accent)]">agent</span>
        {/if}
        <span class="hidden sm:inline shrink-0 text-micro text-[var(--text-faint)]">via {card.transport}</span>
        <span class="ml-auto shrink-0 flex items-center gap-2">
          {#if card.project}
            <button
              class="text-micro font-mono font-medium text-[var(--text-muted)] px-1.5 py-0.5 rounded
                     bg-[var(--bg-subtle)] hover:text-[var(--text)]"
              title="Only {card.projectName ?? card.project}"
              onclick={() => onlyProject(card.project)}
            >{card.project}</button>
          {/if}
          <TimeAgo class="text-micro text-[var(--text-faint)] tabular-nums" date={card.ts} />
        </span>
      </header>

      <p class="mt-1 m-0 text-body-sm leading-relaxed text-[var(--text-muted)] break-words">
        {verbFor(card)}
        {#if card.kind === "status"}
          {#if !card.batch}{@render subjectRef(s, false)} to{/if}
          {@render statusChip(card.lead.new_value)}
          {#if !card.batch && card.lead.old_value}
            <span class="text-micro text-[var(--text-faint)]">from {statusLabel(card.lead.old_value)}</span>
          {/if}
        {:else if !card.batch}
          {#if card.kind === "assignee"}
            {@render subjectRef(s, false)} to <span class="text-[var(--text)]">{assigneeLabel(card.lead.new_value ?? card.lead.old_value)}</span>
          {:else if card.kind === "link"}
            {@render subjectRef(s, false)}
          {:else if card.kind === "structure"}
            <span class="font-medium text-[var(--text)]">{card.lead.entity_label ?? ""}</span>
          {:else}
            {@render subjectRef(s, false)}
          {/if}
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

      {@render details(card)}

      <footer class="mt-2.5 flex items-center gap-1 -ml-1.5">
        {#if target}
          <button
            class="inline-flex items-center gap-1.5 text-caption px-1.5 py-1 rounded-md transition-colors
                   {composerFor === card.key ? 'text-[var(--text)] bg-[var(--bg-subtle)]' : 'text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]'}"
            onclick={() => openComposer(card)}
          ><MessageSquare size={13} />Comment</button>
        {/if}
        {#if !card.batch && s.href}
          <button
            class="inline-flex items-center gap-1 text-caption px-1.5 py-1 rounded-md text-[var(--text-faint)]
                   hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors"
            onclick={() => navigate(s.href!)}
          >Open<ArrowUpRight size={12} /></button>
        {/if}
      </footer>

      {#if composerFor === card.key && target}
        <div class="mt-1.5 rounded-lg border border-[var(--border)] bg-[var(--bg)] focus-within:border-[var(--accent)] transition-colors" transition:fly={{ y: -4, duration: 150 }}>
          <textarea
            data-composer={card.key}
            class="block w-full resize-none bg-transparent px-3 py-2 text-body-sm text-[var(--text)]
                   placeholder:text-[var(--text-faint)] outline-none min-h-[64px]"
            placeholder="Comment on {target.label ?? 'this page'}… Markdown works"
            bind:value={draft}
            use:autoGrow
            onkeydown={(e) => composerKey(e, card)}
            disabled={sending}
          ></textarea>
          <div class="flex items-center justify-end gap-2 px-2 pb-2">
            <span class="hidden sm:inline mr-auto pl-1 text-micro text-[var(--text-faint)]">⌘/Ctrl + Enter to send · Esc to close</span>
            <button
              class="text-caption px-2.5 py-1 rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
              onclick={() => (composerFor = null)}
            >Cancel</button>
            <button
              class="inline-flex items-center gap-1.5 text-caption font-medium px-2.5 py-1 rounded-md
                     text-[var(--btn-success-text)] bg-[var(--btn-success)] hover:bg-[var(--btn-success-hover)]
                     disabled:opacity-50 transition-colors"
              disabled={!draft.trim() || sending}
              onclick={() => sendComment(card)}
            ><SendHorizontal size={12} />{sending ? "Sending…" : "Send"}</button>
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
      {@render sectionLabel("Who")}
      <div class="grid grid-cols-3 p-0.5 rounded-lg bg-[var(--bg-subtle)] border border-[var(--border)] text-caption">
        {#each [["all", "Everyone"], ["only", "Agents"], ["exclude", "People"]] as [mode, label] (mode)}
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
            {@const name = a.display_name || a.username || "System"}
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
              <span class="text-micro text-[var(--text-faint)] tabular-nums">{a.actions.toLocaleString()}</span>
            </button>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Event types -->
    <section>
      {@render sectionLabel("Events")}
      <div class="flex flex-wrap gap-1.5">
        {#each KIND_OPTIONS as o (o.kind)}
          {@const on = filters.kinds.includes(o.kind)}
          <button
            class="inline-flex items-center gap-1.5 text-caption px-2 py-1 rounded-full border transition-colors
                   {on
              ? 'border-[var(--accent)] bg-[var(--accent-subtle)] text-[var(--accent)] font-medium'
              : 'border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)] hover:border-[var(--text-faint)]'}"
            onclick={() => setFilters({ kinds: toggle(filters.kinds, o.kind) })}
          >
            <span style="color: {on ? 'inherit' : KIND_TONE[o.kind]}">{@render kindIcon(o.kind, 12)}</span>{o.label}
          </button>
        {/each}
      </div>
    </section>

    {#if groups.length}
      <section>
        {@render sectionLabel("Groups")}
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
      {@render sectionLabel("Projects", filters.projects.length ? `${filters.projects.length} selected` : undefined)}
      {#if projects.length > 8}
        <label class="mb-1.5 flex items-center gap-2 px-2 py-1 rounded-md border border-[var(--border)] focus-within:border-[var(--accent)]">
          <Search size={12} class="text-[var(--text-faint)]" />
          <input class="flex-1 min-w-0 bg-transparent outline-none text-body-sm" placeholder="Find project" bind:value={projectQuery} />
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
        <X size={12} />Reset filters
      </button>
    {/if}
  </div>
{/snippet}

{#snippet topbarContent()}
  <div class="flex items-center gap-3 px-4 sm:px-6 py-2 w-full">
    <span class="text-body-sm font-medium text-[var(--text)]">Feed</span>
    <span class="inline-flex items-center gap-1.5 text-micro text-[var(--text-faint)]" title="Updates arrive as they happen">
      <span class="relative flex size-2">
        <span class="absolute inline-flex size-full rounded-full bg-[var(--success)] opacity-60 animate-ping"></span>
        <span class="relative inline-flex size-2 rounded-full bg-[var(--success)]"></span>
      </span>
      Live
    </span>
    <div class="ml-auto flex items-center gap-2">
      {#if filterCount}
        <button class="hidden lg:inline-flex items-center gap-1 text-caption text-[var(--text-muted)] hover:text-[var(--text)]" onclick={resetFilters}>
          <X size={12} />Reset
        </button>
      {/if}
      <button
        class="lg:hidden inline-flex items-center gap-1.5 text-caption px-2 py-1 rounded-md border transition-colors
               {filtersOpen || filterCount ? 'border-[var(--accent)] text-[var(--accent)]' : 'border-[var(--border)] text-[var(--text-muted)]'}"
        onclick={() => (filtersOpen = !filtersOpen)}
      ><SlidersHorizontal size={12} />Filters{filterCount ? ` · ${filterCount}` : ""}</button>
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
        ><ArrowUp size={12} />{pending.length} new</button>
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
          <div class="flex flex-col gap-3">
            {#each Array(5) as _, i (i)}
              <div class="flex gap-3 px-4 py-3.5 rounded-xl border border-[var(--border)]">
                <Skeleton variant="circle" class="size-8 shrink-0" />
                <div class="flex-1 flex flex-col gap-2">
                  <Skeleton variant="bar" class="h-3 w-40" />
                  <Skeleton variant="bar" class="h-3 w-3/4" />
                  <Skeleton variant="bar" class="h-3 w-1/2" />
                </div>
              </div>
            {/each}
          </div>
        {:else if error}
          <ErrorState title="Couldn't load the feed" message={error}>
            <button
              class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)] transition-colors"
              onclick={load}
            >Try again</button>
          </ErrorState>
        {:else if cards.length === 0}
          <div class="flex flex-col items-center py-20 gap-3 px-6 max-w-[440px] mx-auto text-center">
            <ActivityIcon size={32} class="text-[var(--text-faint)]" />
            {#if filterCount}
              <p class="text-body-lg text-[var(--text-muted)] m-0">Nothing matches these filters</p>
              <button class="text-caption text-[var(--accent)] hover:underline" onclick={resetFilters}>Reset filters</button>
            {:else}
              <p class="text-body-lg text-[var(--text-muted)] m-0">No activity yet</p>
              <p class="text-body-sm text-[var(--text-faint)] m-0">New issues, status changes and comments from people and agents show up here as they happen.</p>
            {/if}
          </div>
        {:else}
          {#each days as day (day.label)}
            <section class="mb-6 last:mb-0">
              <div class="sticky top-0 z-10 -mx-1 px-1 py-2 bg-[var(--bg)] flex items-center gap-2">
                <span class="text-micro font-semibold uppercase tracking-widest text-[var(--text-muted)]">{day.label}</span>
                <div class="flex-1 h-px bg-[var(--border)]"></div>
              </div>
              <div class="flex flex-col gap-2.5">
                {#each day.cards as card (card.key)}
                  {@render feedCard(card)}
                {/each}
              </div>
            </section>
          {/each}
          <div bind:this={sentinel} class="h-px"></div>
          {#if loadingMore}
            <p class="text-center text-caption text-[var(--text-faint)] py-4 m-0">Loading…</p>
          {:else if !hasMore}
            <p class="text-center text-caption text-[var(--text-faint)] py-6 m-0">That's everything</p>
          {/if}
        {/if}
      </div>

      <aside class="hidden lg:block w-[272px] shrink-0 sticky top-5 max-h-[calc(100vh-7rem)] overflow-y-auto pb-6">
        {@render filterPanel()}
      </aside>
    </div>
  </div>
</div>

<style>
  .feed-fresh {
    animation: feed-fresh 4s ease-out forwards;
  }
  @keyframes feed-fresh {
    0% { box-shadow: 0 0 0 3px var(--accent-subtle); }
    100% { box-shadow: 0 0 0 0 transparent; border-color: var(--border); }
  }
  .feed-comment :global(p:first-child) { margin-top: 0; }
  .feed-comment :global(p:last-child) { margin-bottom: 0; }
</style>
