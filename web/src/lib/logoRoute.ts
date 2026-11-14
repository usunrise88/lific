// Fork: where the sidebar logo leads. Pure, so it can be tested; the live
// value is held by logoTarget.svelte.ts.

import type { LogoTarget } from "./api";

export const REPO_URL = "https://github.com/VoidNullable/lific";

/** The in-app route for a target, or null for the external repository. */
export function logoRoute(target: LogoTarget): string | null {
  switch (target) {
    case "home":
      return "/";
    case "feed":
      return "/feed";
    case "for-me":
      return "/for-me";
    default:
      return null;
  }
}
