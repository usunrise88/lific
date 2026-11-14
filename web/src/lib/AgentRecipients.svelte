<script lang="ts">
  // Fork: which agents a comment being written will reach, and a picker to
  // address it to agents explicitly. Picks and @mentions replace the
  // automatic routing; the chips say which rule applies.
  import { AtSign, Bot, Plus, X } from "lucide-svelte";
  import type { IssueAgents } from "./api";
  import { agentLabel, predictRecipients, type RecipientKind } from "./agents";
  import { t } from "./i18n";

  let {
    info,
    draft,
    picks = $bindable<number[]>([]),
  }: { info: IssueAgents; draft: string; picks?: number[] } = $props();

  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();

  let recipients = $derived(predictRecipients(info, picks, draft));
  let explicit = $derived(recipients.some((r) => r.kind === "picked" || r.kind === "mention"));
  let choices = $derived(info.available.filter((a) => !picks.includes(a.id)));

  function hint(kind: RecipientKind): string {
    return t(`agents.kind.${kind}`);
  }

  function onWindowClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(e) => open && e.key === "Escape" && (open = false)} />

{#if info.available.length > 0}
  <div class="rcpt" bind:this={root} data-testid="agent-recipients">
    <Bot size={13} class="rcpt__icon" />
    <span class="rcpt__label">{t("agents.recipients")}</span>
    {#each recipients as r (r.agent.id)}
      <span
        class="rcpt__chip"
        class:rcpt__chip--auto={!explicit}
        title={hint(r.kind)}
      >
        {#if r.kind === "mention"}<AtSign size={11} />{/if}
        {agentLabel(r.agent)}
        {#if r.kind === "picked"}
          <button
            type="button"
            class="rcpt__x"
            aria-label={t("agents.unpick", { name: agentLabel(r.agent) })}
            onclick={() => (picks = picks.filter((id) => id !== r.agent.id))}
          ><X size={11} /></button>
        {/if}
      </span>
    {:else}
      <span class="rcpt__none">{t("agents.nobody")}</span>
    {/each}
    {#if !explicit && recipients.length > 0}
      <span class="rcpt__auto">{t("agents.auto")}</span>
    {/if}
    {#if choices.length > 0}
      <span class="rcpt__add-wrap">
        <button type="button" class="rcpt__add" aria-expanded={open} onclick={() => (open = !open)}>
          <Plus size={11} />{t("agents.pick")}
        </button>
        {#if open}
          <ul class="rcpt__menu" role="listbox" aria-label={t("agents.pick")}>
            {#each choices as a (a.id)}
              <li>
                <button
                  type="button"
                  role="option"
                  aria-selected="false"
                  onclick={() => {
                    picks = [...picks, a.id];
                    open = false;
                  }}
                >
                  <Bot size={13} />
                  <span class="rcpt__menu-name">{a.display_name || a.username}</span>
                  {#if a.owner_name}<span class="rcpt__menu-owner">{a.owner_name}</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </span>
    {/if}
  </div>
{/if}

<style>
  .rcpt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
    padding: 0.375rem 1rem;
    border-top: 1px solid var(--border);
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  .rcpt :global(.rcpt__icon) {
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .rcpt__label {
    color: var(--text-faint);
  }
  .rcpt__chip {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.0625rem 0.5rem;
    border-radius: 999px;
    border: 1px solid var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 500;
    white-space: nowrap;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rcpt__chip--auto {
    border-color: var(--border);
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-weight: 400;
  }
  .rcpt__x {
    display: inline-flex;
    margin-right: -0.25rem;
    opacity: 0.7;
  }
  .rcpt__x:hover {
    opacity: 1;
  }
  .rcpt__none,
  .rcpt__auto {
    color: var(--text-faint);
  }
  .rcpt__add-wrap {
    position: relative;
  }
  .rcpt__add {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.0625rem 0.5rem;
    border-radius: 999px;
    border: 1px dashed var(--border);
    color: var(--text-muted);
  }
  .rcpt__add:hover {
    color: var(--text);
    border-color: var(--text-faint);
  }
  .rcpt__menu {
    position: absolute;
    bottom: calc(100% + 0.375rem);
    left: 0;
    z-index: 30;
    min-width: 220px;
    max-height: 260px;
    overflow-y: auto;
    padding: 0.25rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background: var(--surface);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
  }
  .rcpt__menu button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.375rem 0.5rem;
    border-radius: 0.375rem;
    text-align: left;
    color: var(--text);
    font-size: 0.8125rem;
  }
  .rcpt__menu button:hover {
    background: var(--bg-subtle);
  }
  .rcpt__menu-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rcpt__menu-owner {
    color: var(--text-faint);
    font-size: 0.75rem;
  }
</style>
