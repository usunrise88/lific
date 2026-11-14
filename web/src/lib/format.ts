// Shared timestamp formatting (LIF-123).
//
// Extracted from the detail routes + CommentThread so every surface
// renders timestamps identically. API timestamps are UTC without a
// trailing "Z", so we append it before parsing. Fork: formatted in the
// interface language (lib/i18n).

import { locale, t } from "./i18n";

export function formatDate(iso: string): string {
  const d = new Date(iso + "Z");
  return d.toLocaleDateString(locale(), {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

// Absolute date + time down to the minute, for tooltips / `title` on
// relative timestamps (LIF-285). formatDate already includes hh:mm today,
// so this is currently an alias — kept as a named export so TimeAgo has a
// stable "full timestamp" formatter even if formatDate's shape changes.
export function formatDateTime(iso: string): string {
  return formatDate(iso);
}

export function formatRelative(iso: string): string {
  const d = new Date(iso + "Z");
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHrs = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);
  if (diffMins < 1) return t("common.time.justNow");
  if (diffMins < 60) return t("common.time.minutesAgo", { count: diffMins });
  if (diffHrs < 24) return t("common.time.hoursAgo", { count: diffHrs });
  if (diffDays < 7) return t("common.time.daysAgo", { count: diffDays });
  return d.toLocaleDateString(locale(), { month: "short", day: "numeric" });
}
