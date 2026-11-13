import { describe, expect, test } from "bun:test";
import { canPark, forMode, isParked } from "../src/lib/issues/parked";

const parked = { reason: "later", at: "2026-11-13 10:00:00" };

describe("parked issues (fork)", () => {
  test("each mode shows its own issues", () => {
    const rows = [
      { id: 1, parked: undefined },
      { id: 2, parked },
      { id: 3 },
    ];
    expect(forMode(rows, false).map((r) => r.id)).toEqual([1, 3]);
    expect(forMode(rows, true).map((r) => r.id)).toEqual([2]);
    expect(isParked(rows[1])).toBe(true);
  });

  test("only open, unparked issues can be parked", () => {
    expect(canPark({ status: "backlog" })).toBe(true);
    expect(canPark({ status: "active" })).toBe(true);
    expect(canPark({ status: "done" })).toBe(false);
    expect(canPark({ status: "cancelled" })).toBe(false);
    expect(canPark({ status: "todo", parked })).toBe(false);
  });
});
