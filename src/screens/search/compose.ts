import type { ConditionGroup } from "../../api/bindings/ConditionGroup";
import type { ConditionNode } from "../../api/bindings/ConditionNode";
import type { SearchQuery } from "../../api/bindings/SearchQuery";
import { MAX_LEAVES, toInstant } from "../../components/dsl";

export interface RefineState {
  pstId: number | null;
  folderPath: string | null;
  hasAttachment: boolean;
  last90Days: boolean;
}

export const EMPTY_REFINE: RefineState = { pstId: null, folderPath: null, hasAttachment: false, last90Days: false };
const MAX_TERMS = 10;

export type ComposeResult = { query: SearchQuery | null; leaves: number; problem: null | "tooManyTerms" | "tooManyConditions" };

/** Part 03 §5.1 simple-query rule: a term matches any of sender, recipients, subject, body. */
export function termGroup(term: string): ConditionNode {
  return {
    type: "group",
    op: "or",
    children: [
      { type: "sender", value: term },
      { type: "recipient", value: term, kinds: [] },
      { type: "subject", value: term },
      { type: "body", value: term },
    ],
  };
}

function leafCount(nodes: ConditionNode[]): number {
  return nodes.reduce((n, c) => n + (c.type === "group" ? leafCount(c.children) : 1), 0);
}

export function refineNodes(r: RefineState, now: Date): ConditionNode[] {
  const out: ConditionNode[] = [];
  if (r.pstId !== null) out.push({ type: "sourcePst", pstIds: [r.pstId] });
  if (r.folderPath !== null) out.push({ type: "folder", folderPath: r.folderPath, includeSubfolders: true });
  if (r.hasAttachment) out.push({ type: "hasAttachment", value: true });
  if (r.last90Days) {
    // Local midnight 90 calendar days ago (the Date constructor normalizes the negative day).
    out.push({ type: "dateRange", from: toInstant(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 90)), to: null });
  }
  return out;
}

export function splitTerms(text: string): string[] {
  return text.trim().split(/\s+/).filter((t) => t.length > 0);
}

/** Search box + refine pane + builder → one SearchQuery (plan Decision "Search query composition in the frontend"). */
export function composeQuery(text: string, refine: RefineState, builder: ConditionGroup | null, now: Date = new Date()): ComposeResult {
  const terms = splitTerms(text);
  if (terms.length > MAX_TERMS) return { query: null, leaves: 4 * terms.length, problem: "tooManyTerms" };
  const refined = refineNodes(refine, now);
  const leaves = 4 * terms.length + refined.length + (builder ? leafCount(builder.children) : 0);
  if (leaves > MAX_LEAVES) return { query: null, leaves, problem: "tooManyConditions" };
  if (refined.length === 0 && builder === null) {
    return { query: terms.length > 0 ? { kind: "simple", text: text.trim() } : null, leaves, problem: null };
  }
  const children: ConditionNode[] = [
    ...terms.map(termGroup),
    ...refined,
    ...(builder ? [{ type: "group" as const, op: builder.op, children: builder.children }] : []),
  ];
  return { query: { kind: "dsl", root: { op: "and", children } }, leaves, problem: null };
}
