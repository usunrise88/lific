// LIF-418: pure helpers behind the project Files view.
//
// Everything here is a plain function over plain data so it can be unit
// tested (web/tests/files.test.ts) without mounting a component: the filter
// chip set, the sweep countdown wording, the delete-confirm sentence, the
// client-side permission mirror, and the route a linked-entity chip navigates
// to. Files.svelte holds only rendering and fetching.

import type { LinkedEntity, MimeClass, ProjectAttachment } from "../api";
import { t } from "../i18n";

/** The filter chips, in display order. `null` is the "All" chip. Built per
 *  call so the labels follow the active language. */
export function mimeFilters(): { value: MimeClass | null; label: string }[] {
  return [
    { value: null, label: t("files.filter.all") },
    { value: "image", label: t("files.filter.image") },
    { value: "video", label: t("files.filter.video") },
    { value: "audio", label: t("files.filter.audio") },
    { value: "text", label: t("files.filter.text") },
    { value: "pdf", label: t("files.filter.pdf") },
    { value: "archive", label: t("files.filter.archive") },
    { value: "other", label: t("files.filter.other") },
  ];
}

export function sortOptions(): {
  value: "created_at" | "size" | "filename";
  label: string;
}[] {
  return [
    { value: "created_at", label: t("files.sort.newest") },
    { value: "size", label: t("files.sort.largest") },
    { value: "filename", label: t("files.sort.name") },
  ];
}

/** Human label for a class, used by the row subtitle and the empty state. */
export function mimeClassLabel(cls: MimeClass): string {
  return mimeFilters().find((f) => f.value === cls)?.label ?? t("files.filter.other");
}

/**
 * The countdown shown next to a pending orphan.
 *
 * Deliberately coarse: the sweeper runs hourly, so minute-level precision
 * would be a lie. Zero (or less) means the file is already past the grace
 * window and goes on the next pass.
 */
export function formatSweepCountdown(seconds: number): string {
  if (seconds <= 0) return t("files.sweep.nextPass");
  const hours = Math.floor(seconds / 3600);
  if (hours >= 24) {
    const days = Math.floor(hours / 24);
    return t("files.sweep.days", { count: days });
  }
  if (hours >= 1) return t("files.sweep.hours", { count: hours });
  const minutes = Math.max(1, Math.floor(seconds / 60));
  return t("files.sweep.minutes", { count: minutes });
}

/**
 * The inline confirm sentence. States the blast radius, because deleting a
 * file that three issues embed breaks all three, and the row only shows the
 * chips for one project.
 */
export function deleteConfirmMessage(referenceCount: number): string {
  if (referenceCount === 0) {
    return t("files.deleteConfirm.noRefs");
  }
  return t("files.deleteConfirm.refs", { count: referenceCount });
}

/**
 * Client-side mirror of the server's delete gate (uploader, a project
 * maintainer, or an admin). This only hides a button; the server is still the
 * boundary, and it answers 403 either way.
 *
 * Fails open when the viewer is unknown (`viewerId === null`, i.e. the `me()`
 * probe hasn't resolved or failed): showing a button that might 403 beats
 * hiding one that would have worked, matching how projectRole.svelte.ts
 * treats an unresolved role.
 */
export function canDeleteAttachment(input: {
  uploaderId: number | null;
  viewerId: number | null;
  isAdmin: boolean;
  canEdit: boolean;
}): boolean {
  if (input.isAdmin || input.canEdit) return true;
  if (input.viewerId === null) return true;
  return input.uploaderId !== null && input.uploaderId === input.viewerId;
}

/**
 * The hash route a linked-entity chip navigates to, or null when the link
 * can't be resolved to a page in this UI (a workspace page has no project
 * route, and a comment on one inherits that).
 */
export function entityHref(
  projectIdentifier: string,
  entity: LinkedEntity,
): string | null {
  if (entity.page_id !== null) {
    return `/${projectIdentifier}/pages/${entity.page_id}`;
  }
  if (entity.identifier) {
    return `/${projectIdentifier}/issues/${entity.identifier}`;
  }
  return null;
}

/** Chip label: the identifier, with a marker when the reference is a comment
 *  rather than the entity body. */
export function entityChipLabel(entity: LinkedEntity): string {
  const base = entity.identifier ?? t("files.chip.unlinked");
  return entity.entity_type === "comment" ? t("files.chip.comment", { base }) : base;
}

/** The distinct uploader usernames present in a set of rows, sorted, for the
 *  uploader filter dropdown. */
export function uploaderOptions(items: ProjectAttachment[]): string[] {
  const names = new Set<string>();
  for (const item of items) {
    if (item.uploader) names.add(item.uploader);
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}
