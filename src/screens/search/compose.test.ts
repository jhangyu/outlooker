import { describe, expect, it } from "vitest";
import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import { toInstant } from "../../components/dsl";
import { EMPTY_REFINE, composeQuery, termGroup } from "./compose";

const words = (n: number) => Array.from({ length: n }, (_, i) => `word${i}`).join(" ");

describe("query composition", () => {
  it("composeQuery builds simple, dsl and problem results", () => {
    expect(composeQuery("  quarterly   report ", EMPTY_REFINE, null)).toEqual({
      query: { kind: "simple", text: "quarterly   report" },
      leaves: 8,
      problem: null,
    });
    const r = composeQuery("quarterly report", { ...EMPTY_REFINE, hasAttachment: true }, null);
    expect(r.query).toEqual({
      kind: "dsl",
      root: { op: "and", children: [termGroup("quarterly"), termGroup("report"), { type: "hasAttachment", value: true }] },
    });
    expect(r.leaves).toBe(9);
    expect(composeQuery(words(11), EMPTY_REFINE, null)).toEqual({ query: null, leaves: 44, problem: "tooManyTerms" });
    const builder: ConditionGroup = {
      op: "or",
      children: Array.from({ length: 11 }, (_, i) => ({ type: "subject" as const, value: `s${i}` })),
    };
    expect(composeQuery(words(10), EMPTY_REFINE, builder)).toEqual({ query: null, leaves: 51, problem: "tooManyConditions" });
    expect(composeQuery("   ", EMPTY_REFINE, null).query).toBeNull();
  });

  it("termGroup is the Part 03 §5.1 simple-query rule", () => {
    expect(termGroup("alice")).toEqual({
      type: "group",
      op: "or",
      children: [
        { type: "sender", value: "alice" },
        { type: "recipient", value: "alice", kinds: [] },
        { type: "subject", value: "alice" },
        { type: "body", value: "alice" },
      ],
    });
  });

  it("refine conditions and the builder become nodes of one and-root", () => {
    const now = new Date(2026, 9, 22, 15, 30);
    const builder: ConditionGroup = { op: "or", children: [{ type: "body", value: "報價" }] };
    const r = composeQuery("", { pstId: 4, folderPath: "Inbox", hasAttachment: false, last90Days: true }, builder, now);
    expect(r.query).toEqual({
      kind: "dsl",
      root: {
        op: "and",
        children: [
          { type: "sourcePst", pstIds: [4] },
          { type: "folder", folderPath: "Inbox", includeSubfolders: true },
          { type: "dateRange", from: toInstant(new Date(2026, 6, 24)), to: null },
          { type: "group", op: "or", children: [{ type: "body", value: "報價" }] },
        ],
      },
    });
    expect(r.leaves).toBe(4);
  });
});
