<script lang="ts">
  // Fork: closure rate. Issues closed per day or week, stacked by who
  // closed them, with what was opened in the same stretch as a tick on the
  // same scale, and a per-actor table: throughput, rework, evidence and
  // what each one holds in progress now.
  import { Bot, CheckCheck, MessageSquareText, RotateCcw, TrendingDown, TrendingUp, User } from "lucide-svelte";
  import type { FlowReport } from "../api";
  import { t, formatNumber } from "../i18n";
  import {
    actorColor,
    actorName,
    actorOrder,
    buildBars,
    deltaPercent,
    formatDuration,
    isAgent,
    share,
    shortDate,
    type Grain,
  } from "./flow";

  let { report }: { report: FlowReport } = $props();

  let grain = $state<Grain>("day");
  let chosen = $state(false);
  $effect(() => {
    // Weeks read better once days get thin, until the reader picks.
    if (!chosen) grain = report.days > 30 ? "week" : "day";
  });
  let focus = $state<number | null | undefined>(undefined);
  let hover = $state<number | null>(null);

  const PLOT_H = 168;
  const SYSTEM = t("flow.system");

  let bars = $derived(buildBars(report, grain));
  let order = $derived(actorOrder(report));
  // The scale follows the closes. Created ticks share it, but a burst (an
  // import, a plan written out at once) is clipped to the top with its
  // number instead of flattening every bar.
  let closesMax = $derived(Math.max(0, ...bars.map((b) => b.done + b.cancelled)));
  let createdMax = $derived(Math.max(0, ...bars.map((b) => b.created)));
  let max = $derived(
    Math.max(1, closesMax === 0 ? createdMax : Math.max(closesMax, Math.min(createdMax, closesMax * 1.5))),
  );
  let gridTop = $derived(niceCeil(max));
  let labelEvery = $derived(Math.max(1, Math.ceil(bars.length / 7)));

  let cur = $derived(report.current);
  let prev = $derived(report.previous);
  let perDay = $derived(cur.done / report.days);
  let net = $derived(cur.created - cur.done - cur.cancelled);

  let rows = $derived(
    report.per_actor.filter((r) => r.done + r.cancelled + r.reopened + r.active_now > 0),
  );
  let topDone = $derived(Math.max(1, ...rows.map((r) => r.done)));

  function niceCeil(n: number): number {
    if (n <= 4) return Math.max(2, Math.ceil(n));
    const step = Math.pow(10, Math.floor(Math.log10(n)));
    const m = n / step;
    const nice = m <= 2 ? 2 : m <= 2.5 ? 2.5 : m <= 5 ? 5 : 10;
    return Math.ceil(nice * step);
  }

  function px(n: number): number {
    return n <= 0 ? 0 : Math.max(2, Math.round((n / gridTop) * PLOT_H));
  }

  function barLabel(date: string, until: string): string {
    return grain === "week" && until !== date ? `${shortDate(date)} – ${shortDate(until)}` : shortDate(date);
  }

  function dim(actor: number | null): boolean {
    return focus !== undefined && focus !== actor;
  }

  function name(id: number | null): string {
    return actorName(report.actors, id, SYSTEM);
  }

  function tipAlign(i: number, n: number): string {
    if (i < n / 3) return "left-0";
    if (i > (2 * n) / 3) return "right-0";
    return "left-1/2 -translate-x-1/2";
  }
</script>

{#snippet delta(value: number | null, upIsGood: boolean)}
  {#if value !== null && value !== 0}
    {@const good = value > 0 === upIsGood}
    <span
      class="inline-flex items-center gap-0.5 text-micro tabular-nums font-medium px-1.5 py-px rounded-full
             {good
        ? 'text-[var(--success)] bg-[color-mix(in_oklab,var(--success)_12%,transparent)]'
        : 'text-[var(--error)] bg-[color-mix(in_oklab,var(--error)_12%,transparent)]'}"
      title={t("flow.vsPrevious", { count: report.days })}
    >
      {#if value > 0}<TrendingUp size={11} />{:else}<TrendingDown size={11} />{/if}
      {value > 0 ? "+" : ""}{value}%
    </span>
  {/if}
{/snippet}

{#snippet kpi(Icon: typeof Bot, label: string, value: string, sub: string, change: number | null, upIsGood: boolean)}
  <div class="min-w-0 rounded-lg bg-[var(--bg-subtle)] px-3 py-2.5">
    <p class="flex items-center gap-1.5 text-micro uppercase tracking-wider text-[var(--text-faint)] font-medium">
      <Icon size={12} class="shrink-0" />
      <span class="truncate">{label}</span>
    </p>
    <p class="mt-1 flex items-baseline gap-2 flex-wrap">
      <span class="font-display text-[1.5rem] leading-none tabular-nums text-[var(--text)]">{value}</span>
      {@render delta(change, upIsGood)}
    </p>
    <p class="mt-1 text-caption text-[var(--text-muted)] line-clamp-2">{sub}</p>
  </div>
{/snippet}

<section class="rounded-xl bg-[var(--surface)] border border-[var(--border)] min-w-0" data-testid="closure-rate">
  <header class="flex flex-wrap items-start justify-between gap-3 px-5 pt-4">
    <div class="min-w-0">
      <h2 class="text-body-sm font-semibold text-[var(--text)]">{t("flow.closure.title")}</h2>
      <p class="text-caption text-[var(--text-faint)] mt-0.5">{t("flow.closure.subtitle", { count: report.days })}</p>
    </div>
    <div class="flex items-center p-0.5 rounded-lg bg-[var(--bg-subtle)]" role="radiogroup" aria-label={t("flow.closure.grain")}>
      {#each ["day", "week"] as const as g (g)}
        <button
          type="button"
          role="radio"
          aria-checked={grain === g}
          class="text-caption h-6 px-2.5 rounded-md transition-colors
                 {grain === g ? 'bg-[var(--surface)] text-[var(--text)] font-medium shadow-sm' : 'text-[var(--text-muted)] hover:text-[var(--text)]'}"
          onclick={() => {
            grain = g;
            chosen = true;
          }}
        >
          {t(`flow.closure.${g}`)}
        </button>
      {/each}
    </div>
  </header>

  <div class="grid grid-cols-2 lg:grid-cols-4 gap-2 px-5 pt-4">
    {@render kpi(
      CheckCheck,
      t("flow.kpi.closed"),
      formatNumber(cur.done),
      t("flow.kpi.perDay", { value: formatNumber(Math.round(perDay * 10) / 10) }),
      deltaPercent(cur.done, prev.done),
      true,
    )}
    {@render kpi(
      net > 0 ? TrendingUp : TrendingDown,
      t("flow.kpi.created"),
      formatNumber(cur.created),
      net > 0 ? t("flow.kpi.netUp", { count: net }) : net < 0 ? t("flow.kpi.netDown", { count: -net }) : t("flow.kpi.netFlat"),
      null,
      false,
    )}
    {@render kpi(
      RotateCcw,
      t("flow.kpi.rework"),
      share(cur.reopened, cur.done) === null ? "—" : `${share(cur.reopened, cur.done)}%`,
      t("flow.kpi.reopened", { count: cur.reopened }),
      null,
      false,
    )}
    {@render kpi(
      MessageSquareText,
      t("flow.kpi.evidence"),
      share(cur.with_evidence, cur.done) === null ? "—" : `${share(cur.with_evidence, cur.done)}%`,
      t("flow.kpi.evidenceSub"),
      null,
      true,
    )}
  </div>

  {#if cur.done + cur.cancelled + cur.created === 0}
    <p class="px-5 py-10 text-center text-body-sm text-[var(--text-muted)]">{t("flow.closure.empty")}</p>
  {:else}
    <!-- Chart -->
    <div class="px-5 pt-5">
      <div class="relative flex" style="height: {PLOT_H}px" role="img" aria-label={t("flow.closure.aria")}>
        <div class="absolute inset-0 flex flex-col justify-between pointer-events-none">
          {#each [gridTop, gridTop / 2, 0] as g (g)}
            <div class="flex items-center gap-2">
              <span class="w-6 text-right text-micro tabular-nums text-[var(--text-faint)] -translate-y-1/2">{g === 0 ? "" : formatNumber(g)}</span>
              <span class="flex-1 border-t {g === 0 ? 'border-[var(--border)]' : 'border-dashed border-[color-mix(in_oklab,var(--border)_70%,transparent)]'}"></span>
            </div>
          {/each}
        </div>
        <div class="relative flex-1 ml-8 flex items-end gap-[2px]">
          {#each bars as bar, i (bar.date)}
            <div
              class="relative flex-1 min-w-0 h-full flex flex-col justify-end items-center"
              role="presentation"
              onmouseenter={() => (hover = i)}
              onmouseleave={() => (hover = null)}
            >
              {#if hover === i}
                <span class="absolute inset-y-0 -inset-x-px rounded-md bg-[var(--bg-subtle)]"></span>
              {/if}
              <div class="relative w-full max-w-[30px] flex flex-col-reverse gap-[2px]">
                {#each bar.segments as seg (seg.actor)}
                  <span
                    class="block w-full rounded-[3px] transition-opacity {dim(seg.actor) ? 'opacity-20' : ''}"
                    style="height: {px(seg.count)}px; background: {actorColor(seg.actor)}"
                  ></span>
                {/each}
                {#if bar.cancelled > 0}
                  <span
                    class="block w-full rounded-[3px] border border-dashed border-[var(--text-faint)] transition-opacity {focus !== undefined ? 'opacity-20' : ''}"
                    style="height: {px(bar.cancelled)}px"
                  ></span>
                {/if}
              </div>
              {#if bar.created > 0}
                {@const clipped = bar.created > gridTop}
                <span
                  class="absolute left-1/2 -translate-x-1/2 w-[calc(100%+2px)] max-w-[34px] h-[2px] rounded-full bg-[var(--text)] opacity-60 pointer-events-none"
                  style="bottom: {clipped ? PLOT_H : px(bar.created)}px"
                ></span>
                {#if clipped}
                  <span
                    class="absolute left-1/2 -translate-x-1/2 text-micro leading-none tabular-nums text-[var(--text-muted)] pointer-events-none whitespace-nowrap"
                    style="bottom: {PLOT_H + 4}px"
                  >↑{formatNumber(bar.created)}</span>
                {/if}
              {/if}

              {#if hover === i}
                <div
                  class="absolute bottom-full mb-2 z-20 pointer-events-none px-3 py-2 rounded-lg whitespace-nowrap min-w-[160px]
                         bg-[var(--surface)] border border-[var(--border)] shadow-[0_6px_18px_rgba(0,0,0,0.18)] {tipAlign(i, bars.length)}"
                >
                  <p class="text-caption font-medium text-[var(--text)] mb-1">{barLabel(bar.date, bar.until)}</p>
                  {#each bar.segments as seg (seg.actor)}
                    <p class="flex items-center gap-1.5 text-micro text-[var(--text-muted)]">
                      <span class="size-2 rounded-sm shrink-0" style="background: {actorColor(seg.actor)}"></span>
                      {name(seg.actor)}
                      <span class="ml-auto pl-4 tabular-nums text-[var(--text)]">{seg.count}</span>
                    </p>
                  {/each}
                  {#if bar.segments.length === 0}
                    <p class="text-micro text-[var(--text-faint)]">{t("flow.closure.noneClosed")}</p>
                  {/if}
                  <div class="mt-1 pt-1 border-t border-[var(--border)] flex flex-col">
                    <p class="flex text-micro text-[var(--text-muted)]">{t("flow.closure.cancelled")}<span class="ml-auto pl-4 tabular-nums text-[var(--text)]">{bar.cancelled}</span></p>
                    <p class="flex text-micro text-[var(--text-muted)]">{t("flow.closure.reopened")}<span class="ml-auto pl-4 tabular-nums text-[var(--text)]">{bar.reopened}</span></p>
                    <p class="flex text-micro text-[var(--text-muted)]">{t("flow.closure.created")}<span class="ml-auto pl-4 tabular-nums text-[var(--text)]">{bar.created}</span></p>
                  </div>
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
      <div class="ml-8 flex gap-[2px] mt-1.5">
        {#each bars as bar, i (bar.date)}
          <span class="flex-1 min-w-0 text-center text-micro text-[var(--text-faint)] whitespace-nowrap overflow-visible">
            {#if i % labelEvery === 0}{shortDate(bar.date)}{/if}
          </span>
        {/each}
      </div>

      <!-- Legend: click an actor to bring out its share -->
      <div class="flex flex-wrap items-center gap-x-1 gap-y-1 mt-3">
        {#each order as id (id)}
          {@const total = report.per_actor.find((r) => r.actor_id === id)?.done ?? 0}
          <button
            type="button"
            class="inline-flex items-center gap-1.5 text-micro px-2 py-1 rounded-md transition-colors
                   {focus === id ? 'bg-[var(--bg-subtle)] text-[var(--text)]' : 'text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]'}
                   {dim(id) ? 'opacity-50' : ''}"
            aria-pressed={focus === id}
            onclick={() => (focus = focus === id ? undefined : id)}
          >
            <span class="size-2.5 rounded-sm shrink-0" style="background: {actorColor(id)}"></span>
            {#if isAgent(report.actors, id)}<Bot size={11} class="shrink-0" />{:else}<User size={11} class="shrink-0" />{/if}
            {name(id)}
            <span class="tabular-nums text-[var(--text-faint)]">{total}</span>
          </button>
        {/each}
        <span class="inline-flex items-center gap-1.5 text-micro text-[var(--text-muted)] px-2 py-1">
          <span class="size-2.5 rounded-sm border border-dashed border-[var(--text-faint)]"></span>{t("flow.closure.cancelled")}
        </span>
        <span class="inline-flex items-center gap-1.5 text-micro text-[var(--text-muted)] px-2 py-1">
          <span class="w-3 h-[2px] rounded-full bg-[var(--text)] opacity-60"></span>{t("flow.closure.created")}
        </span>
      </div>
    </div>

    <!-- Per actor -->
    {#if rows.length > 0}
      <div class="mt-4 border-t border-[var(--border)] overflow-x-auto">
        <table class="w-full text-body-sm table-fixed sm:table-auto" data-testid="closure-actors">
          <thead>
            <tr class="text-micro uppercase tracking-wider text-[var(--text-faint)] text-left whitespace-nowrap">
              <th class="font-medium px-5 py-2">{t("flow.table.actor")}</th>
              <th class="font-medium px-2 py-2 w-[64px] sm:w-[34%]">{t("flow.table.closed")}</th>
              <th class="font-medium px-2 py-2 text-right hidden md:table-cell">{t("flow.table.perDay")}</th>
              <th class="font-medium px-2 py-2 text-right w-[84px] sm:w-auto" title={t("flow.table.reworkHint")}>{t("flow.table.rework")}</th>
              <th class="font-medium px-2 py-2 text-right hidden sm:table-cell" title={t("flow.kpi.evidenceSub")}>{t("flow.table.evidence")}</th>
              <th class="font-medium px-2 py-2 text-right hidden md:table-cell" title={t("flow.table.cycleHint")}>{t("flow.table.cycle")}</th>
              <th class="font-medium pl-2 pr-4 sm:pr-5 py-2 text-right w-[84px] sm:w-auto whitespace-normal sm:whitespace-nowrap">{t("flow.table.active")}</th>
            </tr>
          </thead>
          <tbody>
            {#each rows as r (r.actor_id)}
              {@const ev = share(r.with_evidence, r.done)}
              {@const rw = share(r.reopened, r.done)}
              <tr
                class="border-t border-[var(--border)] transition-colors cursor-pointer
                       {focus === r.actor_id ? 'bg-[var(--bg-subtle)]' : 'hover:bg-[var(--bg-subtle)]'}"
                onclick={() => (focus = focus === r.actor_id ? undefined : r.actor_id)}
              >
                <td class="pl-4 sm:pl-5 pr-2 py-2">
                  <span class="flex items-center gap-2 min-w-0">
                    <span class="size-2.5 rounded-sm shrink-0" style="background: {actorColor(r.actor_id)}"></span>
                    {#if isAgent(report.actors, r.actor_id)}
                      <Bot size={13} class="shrink-0 text-[var(--text-faint)] hidden sm:block" />
                    {:else}
                      <User size={13} class="shrink-0 text-[var(--text-faint)] hidden sm:block" />
                    {/if}
                    <span class="truncate text-[var(--text)]">{name(r.actor_id)}</span>
                  </span>
                </td>
                <td class="px-2 py-2">
                  <span class="flex items-center gap-2">
                    <span class="sm:w-8 text-right tabular-nums text-[var(--text)] font-medium">{r.done}</span>
                    <span class="flex-1 h-1.5 rounded-full bg-[var(--bg-subtle)] overflow-hidden hidden sm:block">
                      <span class="block h-full rounded-full" style="width: {(r.done / topDone) * 100}%; background: {actorColor(r.actor_id)}"></span>
                    </span>
                  </span>
                </td>
                <td class="px-2 py-2 text-right tabular-nums text-[var(--text-muted)] hidden md:table-cell">
                  {formatNumber(Math.round((r.done / report.days) * 10) / 10)}
                </td>
                <td class="px-2 py-2 text-right tabular-nums {r.reopened > 0 ? 'text-[var(--warn)]' : 'text-[var(--text-muted)]'}">
                  {rw === null ? "—" : `${rw}%`}
                </td>
                <td class="px-2 py-2 text-right tabular-nums text-[var(--text-muted)] hidden sm:table-cell">
                  {ev === null ? "—" : `${ev}%`}
                </td>
                <td class="px-2 py-2 text-right tabular-nums text-[var(--text-muted)] hidden md:table-cell">
                  {formatDuration(r.cycle_p50)}
                </td>
                <td class="pl-2 pr-4 sm:pr-5 py-2 text-right tabular-nums {r.active_now > 2 ? 'text-[var(--warn)] font-medium' : 'text-[var(--text-muted)]'}">
                  {r.active_now}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="h-4"></div>
    {/if}
  {/if}
</section>
