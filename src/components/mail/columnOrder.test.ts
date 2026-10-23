import { describe, expect, it } from "vitest";
import { DEFAULT_ORDER, handleSide, moveColumn, parseOrder, readOrder, writeOrder } from "./columnOrder";

const KEY = "outlooker.splits.order-test";

describe("column order", () => {
  it("moves and clamps at the ends", () => {
    expect(moveColumn(DEFAULT_ORDER, "sender", 1)).toEqual(["subject", "sender", "date"]);
    expect(moveColumn(DEFAULT_ORDER, "sender", 99)).toEqual(["subject", "date", "sender"]);
    expect(moveColumn(DEFAULT_ORDER, "date", -5)).toEqual(["date", "sender", "subject"]);
    expect(moveColumn(DEFAULT_ORDER, "subject", 1)).toEqual([...DEFAULT_ORDER]);
  });

  it("round-trips through storage and keeps stored widths", () => {
    localStorage.setItem(KEY, '{"sender":120}');
    writeOrder(KEY, ["date", "sender", "subject"]);
    expect(readOrder(KEY)).toEqual(["date", "sender", "subject"]);
    expect(JSON.parse(localStorage.getItem(KEY)!).sender).toBe(120);
  });

  it("falls back to the default on malformed order", () => {
    for (const bad of [{} as Record<string, number>,{ "order.sender": 0, "order.subject": 0, "order.date": 1 }, { "order.sender": 0, "order.subject": 1, "order.date": 5 }, { "order.sender": 0.5, "order.subject": 1, "order.date": 2 }]) {
      expect(parseOrder(bad)).toEqual([...DEFAULT_ORDER]);
    }
    localStorage.setItem(KEY, "{");
    expect(readOrder(KEY)).toEqual([...DEFAULT_ORDER]);
  });

  it("puts each handle on the edge facing the subject", () => {
    expect(handleSide(DEFAULT_ORDER, "sender")).toBe("right");
    expect(handleSide(DEFAULT_ORDER, "date")).toBe("left");
    expect(handleSide(["subject", "sender", "date"], "sender")).toBe("left");
  });
});
