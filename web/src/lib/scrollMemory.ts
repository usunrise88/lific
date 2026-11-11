// Fork: remember where a long list was scrolled to (Feed, For me), per tab,
// so coming back from an issue lands where the reader left off. The list
// is reloaded with as many rows as were loaded before, then scrolled.

export interface SavedScroll {
  top: number;
  /** Rows loaded when the position was saved. */
  count: number;
  at: number;
}

const PREFIX = "lific:scroll:";
/** Older positions are stale: the list has moved on. */
const MAX_AGE_MS = 30 * 60 * 1000;

export function saveScroll(key: string, top: number, count: number): void {
  try {
    sessionStorage.setItem(PREFIX + key, JSON.stringify({ top, count, at: Date.now() }));
  } catch {
    // ignore
  }
}

export function loadScroll(key: string, now = Date.now()): SavedScroll | null {
  try {
    const raw = sessionStorage.getItem(PREFIX + key);
    if (!raw) return null;
    const v = JSON.parse(raw) as Partial<SavedScroll>;
    if (typeof v.top !== "number" || typeof v.count !== "number" || typeof v.at !== "number") return null;
    if (now - v.at > MAX_AGE_MS || v.top <= 0) return null;
    return { top: v.top, count: v.count, at: v.at };
  } catch {
    return null;
  }
}

export function clearScroll(key: string): void {
  try {
    sessionStorage.removeItem(PREFIX + key);
  } catch {
    // ignore
  }
}

/** A scroll listener that saves at most once per frame. */
export function scrollSaver(key: string, read: () => { top: number; count: number } | null) {
  let queued = false;
  return () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      const v = read();
      if (v) saveScroll(key, v.top, v.count);
    });
  };
}
