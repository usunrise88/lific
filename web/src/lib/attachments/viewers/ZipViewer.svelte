<script lang="ts">
  // LIF-418: archive listing, from the server-side /preview endpoint.
  //
  // The browser never unzips anything here. The server already walked the
  // central directory and hands back the entry names and sizes; this just
  // tabulates them. When the endpoint 404s (older backend), errors, or answers
  // `kind: "none"`, the card degrades to the plain download chip.

  import { FileArchive } from "lucide-svelte";
  import ViewerFrame from "./ViewerFrame.svelte";
  import FileChip from "./FileChip.svelte";
  import { whenVisible } from "./visible";
  import { formatBytes, getAttachmentPreview, type ZipPreviewEntry } from "../../api";
  import { t } from "../../i18n";

  let {
    id,
    filename,
    sizeBytes = null,
  }: { id: number; filename: string; sizeBytes?: number | null } = $props();

  let entries = $state<ZipPreviewEntry[] | null>(null);
  let totalEntries = $state(0);
  let truncated = $state(false);
  let failed = $state(false);
  let requested = false;

  async function load() {
    if (requested) return;
    requested = true;
    const res = await getAttachmentPreview(id);
    if (!res.ok || res.data.kind !== "zip") {
      failed = true;
      return;
    }
    entries = res.data.entries;
    totalEntries = res.data.total_entries;
    truncated = res.data.truncated;
  }

  /** Directory entries are the ones ending in "/" and carry no useful size. */
  function isDirectory(entry: ZipPreviewEntry): boolean {
    return entry.name.endsWith("/");
  }
</script>

{#if failed}
  <FileChip {id} {filename} {sizeBytes} />
{:else}
  <div use:whenVisible={{ onVisible: () => void load(), enabled: entries === null }}>
    {#if entries === null}
      <FileChip {id} {filename} {sizeBytes} note={t("viewers.zip.reading")} />
    {:else}
      <ViewerFrame
        {id}
        {filename}
        {sizeBytes}
        meta={t("viewers.zip.entries", { count: totalEntries })}
        tone="flush"
      >
        {#snippet icon()}<FileArchive size={14} />{/snippet}

        <div class="zv__scroll">
          <table class="zv__table">
            <thead>
              <tr>
                <th scope="col">{t("viewers.zip.name")}</th>
                <th scope="col" class="zv__num-col">{t("viewers.zip.size")}</th>
                <th scope="col" class="zv__num-col">{t("viewers.zip.compressed")}</th>
              </tr>
            </thead>
            <tbody>
              {#each entries as entry (entry.name)}
                <tr class:zv__dir={isDirectory(entry)}>
                  <td class="zv__name">{entry.name}</td>
                  <td class="zv__num-col">{isDirectory(entry) ? "" : formatBytes(entry.size)}</td>
                  <td class="zv__num-col">
                    {isDirectory(entry) ? "" : formatBytes(entry.compressed)}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if truncated}
          <p class="zv__foot">
            {t("viewers.zip.truncated", { shown: entries.length, count: totalEntries })}
          </p>
        {/if}
      </ViewerFrame>
    {/if}
  </div>
{/if}

<style>
  .zv__scroll {
    max-height: 24rem;
    overflow: auto;
    background: var(--bg);
  }
  .zv__table {
    border-collapse: collapse;
    width: 100%;
    font-size: var(--text-caption);
  }
  .zv__table th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0.3125rem 0.625rem;
    background: var(--bg-subtle);
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
  }
  .zv__table td {
    padding: 0.25rem 0.625rem;
    border-bottom: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font-mono);
  }
  .zv__name {
    word-break: break-all;
  }
  .zv__num-col {
    text-align: right;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .zv__dir td {
    color: var(--text-faint);
  }
  .zv__foot {
    margin: 0;
    padding: 0.375rem 0.625rem;
    border-top: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--text-faint);
    font-size: var(--text-micro);
  }
</style>
