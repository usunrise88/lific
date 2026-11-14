// Fork: parked issues ("Отложено") on the web.
//
// A parked issue keeps everything it has but is set aside: the issue list
// and the board leave it out, and the project's Parked mode
// (`/{PROJECT}/parked`) shows only those. Parking and bringing back go
// through `parkActions.ts`, so the list and the bulk bar word and undo them
// the same way. This module stays pure for tests.

import type { Issue } from "../api";

export function isParked(issue: Pick<Issue, "parked">): boolean {
  return !!issue.parked;
}

/** Only open issues can be parked. */
export function canPark(issue: Pick<Issue, "status" | "parked">): boolean {
  return !issue.parked && issue.status !== "done" && issue.status !== "cancelled";
}

/** Issues of the mode: parked ones in Parked mode, the rest otherwise. */
export function forMode<T extends Pick<Issue, "parked">>(issues: T[], parkedMode: boolean): T[] {
  return issues.filter((issue) => isParked(issue) === parkedMode);
}
