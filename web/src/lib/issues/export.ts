import { t } from "../i18n";
import { inPublicScope } from "../publicScope";

/** Use canonical server exports so comments and metadata match single-issue export. */
export async function selectedIssueExport(identifiers: string[]): Promise<Blob> {
  // LIF-471: there is no public export route, and a signed-in reader's token
  // must not leave the public view through this side door either.
  if (inPublicScope()) throw new Error(t("issues.export.publicView"));
  const token = localStorage.getItem("lific_token");
  const parts: BlobPart[] = [];
  const separator = new TextEncoder().encode("\n\n---\n\n");
  const limit = 16 * 1024 * 1024;
  let size = 0;
  for (const identifier of identifiers) {
    const response = await fetch(`/api/export/issues/${encodeURIComponent(identifier)}`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!response.ok) throw new Error(t("issues.export.failed", { identifier, status: response.status }));
    if (!response.body) throw new Error(t("issues.export.empty", { identifier }));
    if (parts.length) {
      parts.push(separator);
      size += separator.byteLength;
    }
    const reader = response.body.getReader();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        if (size > limit) {
          await reader.cancel();
          throw new Error(t("issues.export.tooLarge"));
        }
        parts.push(value);
      }
    } finally {
      reader.releaseLock();
    }
  }
  return new Blob(parts, { type: "text/markdown;charset=utf-8" });
}
