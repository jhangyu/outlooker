import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import type { ConditionNode } from "../../api/bindings/ConditionNode";
import type { FolderRef } from "../../api/bindings/FolderRef";
import type { GroupOp } from "../../api/bindings/GroupOp";
import type { RecipientKind } from "../../api/bindings/RecipientKind";

export type FieldKind = "sender" | "recipient" | "subject" | "body" | "dateRange" | "folder" | "hasAttachment" | "sourcePst";
export const FIELDS: readonly FieldKind[] = ["sender", "recipient", "subject", "body", "dateRange", "folder", "hasAttachment", "sourcePst"];

export interface CondDraft {
  kind: "cond";
  id: string;
  field: FieldKind;
  text: string;
  kinds: RecipientKind[];
  fromDay: string; // "YYYY-MM-DD" from <input type="date">, "" when unset
  toDay: string; // inclusive in the UI
  folderPath: string;
  includeSubfolders: boolean;
  flag: boolean;
  pstIds: number[];
}

export interface GroupDraft {
  kind: "group";
  id: string;
  op: GroupOp;
  children: Array<CondDraft | GroupDraft>;
}

/** Builder root = depth 1; composeQuery wraps the builder in one more group, so the sent tree stays within depth 5 (Part 03 §5.1). */
export const MAX_BUILDER_DEPTH = 4;
export const MAX_LEAVES = 50;

let lastId = 0;
const nextId = () => `n${++lastId}`;

export function newCond(field: FieldKind = "sender"): CondDraft {
  return {
    kind: "cond", id: nextId(), field, text: "", kinds: [], fromDay: "", toDay: "",
    folderPath: "", includeSubfolders: true, flag: true, pstIds: [],
  };
}

export function newGroup(op: GroupOp = "and", children: Array<CondDraft | GroupDraft> = [newCond()]): GroupDraft {
  return { kind: "group", id: nextId(), op, children };
}

export function isComplete(c: CondDraft): boolean {
  switch (c.field) {
    case "sender":
    case "recipient":
    case "subject":
    case "body":
      return c.text.trim().length > 0;
    case "dateRange":
      return c.fromDay !== "" || c.toDay !== "";
    case "folder":
      return c.folderPath !== "";
    case "hasAttachment":
      return true;
    case "sourcePst":
      return c.pstIds.length > 0;
  }
}

export function countLeaves(g: GroupDraft): number {
  return g.children.reduce((n, c) => n + (c.kind === "group" ? countLeaves(c) : isComplete(c) ? 1 : 0), 0);
}

export function depthOf(g: GroupDraft): number {
  return 1 + Math.max(0, ...g.children.map((c) => (c.kind === "group" ? depthOf(c) : 0)));
}

function ymd(day: string): [number, number, number] {
  const [y, m, d] = day.split("-").map(Number);
  return [y!, m!, d!];
}

/** Local midnight of a calendar day (Part 03 §1 Timestamps: the UI converts local days to instants). */
export function localDayStart(day: string): Date {
  const [y, m, d] = ymd(day);
  return new Date(y, m - 1, d);
}

function nextDay(day: string): string {
  const [y, m, d] = ymd(day);
  return new Date(Date.UTC(y, m - 1, d + 1)).toISOString().slice(0, 10);
}

/** RFC 3339 UTC instant without milliseconds, e.g. 2022-12-31T16:00:00Z. */
export function toInstant(d: Date): string {
  return d.toISOString().replace(/\.\d{3}Z$/, "Z");
}

function toNode(c: CondDraft, dayStart: (day: string) => Date): ConditionNode {
  const value = c.text.trim();
  switch (c.field) {
    case "sender":
      return { type: "sender", value };
    case "subject":
      return { type: "subject", value };
    case "body":
      return { type: "body", value };
    case "recipient":
      return { type: "recipient", value, kinds: c.kinds };
    case "dateRange":
      // The UI "to" day is inclusive; the contract's `to` is exclusive, so send the start of the next day.
      return {
        type: "dateRange",
        from: c.fromDay ? toInstant(dayStart(c.fromDay)) : null,
        to: c.toDay ? toInstant(dayStart(nextDay(c.toDay))) : null,
      };
    case "folder":
      return { type: "folder", folderPath: c.folderPath, includeSubfolders: c.includeSubfolders };
    case "hasAttachment":
      return { type: "hasAttachment", value: c.flag };
    case "sourcePst":
      return { type: "sourcePst", pstIds: c.pstIds };
  }
}

/** Incomplete conditions and groups left empty are dropped; an empty root gives null. */
export function toConditionGroup(g: GroupDraft, dayStart: (day: string) => Date = localDayStart): ConditionGroup | null {
  const children = g.children.flatMap((c): ConditionNode[] => {
    if (c.kind === "cond") return isComplete(c) ? [toNode(c, dayStart)] : [];
    const sub = toConditionGroup(c, dayStart);
    return sub ? [{ type: "group", op: sub.op, children: sub.children }] : [];
  });
  return children.length > 0 ? { op: g.op, children } : null;
}

/** Replaces (or, when f returns null, removes) the node with `id` anywhere below `g`. */
export function updateNode(
  g: GroupDraft,
  id: string,
  f: (n: CondDraft | GroupDraft) => CondDraft | GroupDraft | null,
): GroupDraft {
  const children = g.children.flatMap((c) => {
    if (c.id === id) {
      const next = f(c);
      return next ? [next] : [];
    }
    return [c.kind === "group" ? updateNode(c, id, f) : c];
  });
  return { ...g, children };
}

export interface FolderTreeNode {
  path: string;
  name: string;
  count: number; // messages directly in this folder path, summed over PSTs
  children: FolderTreeNode[];
}

/** Part 03 §5.4: folders of all PSTs merged by their "/"-separated path. */
export function buildFolderTree(folders: FolderRef[]): FolderTreeNode[] {
  const root: FolderTreeNode = { path: "", name: "", count: 0, children: [] };
  for (const f of folders) {
    const parts = f.folderPath.split("/");
    let node = root;
    parts.forEach((name, i) => {
      const path = parts.slice(0, i + 1).join("/");
      let child = node.children.find((c) => c.path === path);
      if (!child) {
        child = { path, name, count: 0, children: [] };
        node.children.push(child);
      }
      if (i === parts.length - 1) child.count += f.messageCount;
      node = child;
    });
  }
  return root.children;
}
