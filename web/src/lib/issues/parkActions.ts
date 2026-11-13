// Fork: park and bring back with an Undo toast (see `parked.ts`).

import { bulkParkIssues, parkIssue, type Issue } from "../api";
import { t } from "../i18n";
import { toast } from "../toast/toast.svelte";
import { isParked } from "./parked";

type Target = Pick<Issue, "id" | "parked">;

/** Park `targets` with one reason; the toast's Undo brings them back.
 *  Resolves to whether the server took it. */
export async function parkWithUndo(targets: Target[], reason: string, done?: () => void): Promise<boolean> {
  const ids = targets.map((issue) => issue.id);
  if (ids.length === 0) return true;
  const res = await bulkParkIssues(ids, true, reason);
  if (!res.ok) {
    toast(t("parked.toast.failed", { error: res.error }), { kind: "error" });
    return false;
  }
  done?.();
  toast(t(ids.length === 1 ? "parked.toast.parkedOne" : "parked.toast.parked", { count: ids.length }), {
    kind: "success",
    action: {
      label: t("common.undo"),
      fn: async () => {
        const undo = await bulkParkIssues(ids, false);
        if (!undo.ok) toast(t("issues.toast.undoFailed"), { kind: "error" });
        done?.();
      },
    },
  });
  return true;
}

/** Bring `targets` back to work; Undo parks each again with its own reason. */
export async function unparkWithUndo(targets: Target[], done?: () => void): Promise<boolean> {
  const parked = targets.filter(isParked);
  if (parked.length === 0) return true;
  const res = await bulkParkIssues(
    parked.map((issue) => issue.id),
    false,
  );
  if (!res.ok) {
    toast(t("parked.toast.failed", { error: res.error }), { kind: "error" });
    return false;
  }
  done?.();
  toast(
    t(parked.length === 1 ? "parked.toast.backOne" : "parked.toast.back", { count: parked.length }),
    {
      kind: "success",
      action: {
        label: t("common.undo"),
        fn: async () => {
          const results = await Promise.all(
            parked.map((issue) => parkIssue(issue.id, issue.parked?.reason ?? "")),
          );
          if (results.some((r) => !r.ok)) toast(t("issues.toast.undoFailed"), { kind: "error" });
          done?.();
        },
      },
    },
  );
  return true;
}
