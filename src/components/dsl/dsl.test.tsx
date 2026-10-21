import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import { renderWithProviders } from "../../app/testing";
import {
  DslBuilder, buildFolderTree, countLeaves, depthOf, newCond, newGroup, toConditionGroup, toInstant,
  type CondDraft, type GroupDraft,
} from ".";

/** Part 03 §5.1 worked example, `req.query.root`, copied verbatim from the spec. */
const SPEC_ROOT: ConditionGroup = {
  op: "and",
  children: [
    { type: "group", op: "or", children: [
      { type: "sender", value: "contoso.com" },
      { type: "recipient", value: "alice", kinds: [] },
    ] },
    { type: "dateRange", from: "2022-12-31T16:00:00Z", to: "2023-12-31T16:00:00Z" },
    { type: "body", value: "報價" },
    { type: "hasAttachment", value: true },
    { type: "sourcePst", pstIds: [3, 7] },
  ],
};

const cond = (p: Partial<CondDraft>): CondDraft => ({ ...newCond(p.field), ...p });
/** The spec's user is in UTC+8; the test pins that offset instead of depending on the machine time zone. */
const utc8 = (day: string) => new Date(`${day}T00:00:00+08:00`);
const OPTIONS = { psts: [{ id: 3, displayName: "2023", path: "/p/2023.pst" }], folders: [] };

describe("DSL builder", () => {
  it("DSL builder output for the Part 03 §5.1 worked example deep-equals the spec JSON", () => {
    const draft = newGroup("and", [
      newGroup("or", [cond({ field: "sender", text: " contoso.com " }), cond({ field: "recipient", text: "alice" })]),
      cond({ field: "dateRange", fromDay: "2023-01-01", toDay: "2023-12-31" }),
      cond({ field: "body", text: "報價" }),
      cond({ field: "hasAttachment", flag: true }),
      cond({ field: "sourcePst", pstIds: [3, 7] }),
      cond({ field: "subject", text: "   " }),
      newGroup("or", [cond({ field: "folder" })]),
    ]);
    expect(toConditionGroup(draft, utc8)).toEqual(SPEC_ROOT);
    expect(countLeaves(draft)).toBe(6);
  });

  it("drops incomplete conditions and empty groups, and returns null for an empty root", () => {
    expect(toConditionGroup(newGroup("and", [cond({ field: "sender" }), newGroup("or", [])]))).toBeNull();
    expect(toConditionGroup(newGroup("or", [cond({ field: "dateRange", toDay: "2024-02-29" })]), utc8)).toEqual({
      op: "or",
      children: [{ type: "dateRange", from: null, to: "2024-02-29T16:00:00Z" }],
    });
    expect(toInstant(new Date(Date.UTC(2023, 0, 2, 3, 4, 5, 678)))).toBe("2023-01-02T03:04:05Z");
  });

  it("measures depth and builds the folder tree", () => {
    const deep = newGroup("and", [newGroup("or", [newGroup("and", [newGroup("or")])])]);
    expect(depthOf(deep)).toBe(4);
    const tree = buildFolderTree([
      { folderPath: "Inbox", pstId: 1, messageCount: 5 },
      { folderPath: "Inbox/Projects", pstId: 1, messageCount: 2 },
      { folderPath: "Inbox", pstId: 2, messageCount: 3 },
      { folderPath: "Sent Items", pstId: 2, messageCount: 4 },
    ]);
    expect(tree.map((n) => [n.path, n.count])).toEqual([["Inbox", 8], ["Sent Items", 4]]);
    expect(tree[0]!.children.map((n) => [n.path, n.name, n.count])).toEqual([["Inbox/Projects", "Projects", 2]]);
  });

  it("Add group adds a group with one empty condition", () => {
    const onChange = vi.fn();
    renderWithProviders(<DslBuilder value={newGroup("and", [])} onChange={onChange} options={OPTIONS} leafBudget={50} />);
    fireEvent.click(screen.getByRole("button", { name: "Add group" }));
    const next = onChange.mock.calls[0]![0] as GroupDraft;
    expect(next.children).toHaveLength(1);
    const group = next.children[0] as GroupDraft;
    expect(group.kind).toBe("group");
    expect(group.children).toEqual([expect.objectContaining({ kind: "cond", field: "sender", text: "" })]);
  });

  it("disables Add condition at the leaf budget and hides Add group at depth 4", () => {
    const value = newGroup("and", [
      newGroup("or", [cond({ field: "sender", text: "a@b" }), newGroup("and", [newGroup("or", [cond({ field: "body", text: "x" })])])]),
    ]);
    renderWithProviders(<DslBuilder value={value} onChange={vi.fn()} options={OPTIONS} leafBudget={2} />);
    for (const b of screen.getAllByRole("button", { name: "Add condition" })) expect((b as HTMLButtonElement).disabled).toBe(true);
    // root bar + groups at depth 2 and 3; the group at depth 4 has none.
    expect(screen.getAllByRole("button", { name: "Add group" })).toHaveLength(3);
  });
});
