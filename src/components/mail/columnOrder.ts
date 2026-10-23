import { readNumbers, writeNumber } from "../common/splits";

export type ColId = "sender" | "subject" | "date";
export const DEFAULT_ORDER: readonly ColId[] = ["sender", "subject", "date"];

/** Order shares the `outlooker.splits.mail-list` map with the widths: one index per column under `order.<id>`. */
const PREFIX = "order.";

/** Stored index map → order. Anything but a permutation of 0..n-1 falls back to the default. */
export function parseOrder(stored: Record<string, number>): ColId[] {
  const pos = DEFAULT_ORDER.map((id) => stored[PREFIX + id]);
  const valid = pos.every((p) => p !== undefined && Number.isInteger(p) && p >= 0 && p < pos.length) && new Set(pos).size === pos.length;
  if (!valid) return [...DEFAULT_ORDER];
  const out: ColId[] = [];
  DEFAULT_ORDER.forEach((id, i) => (out[pos[i]!] = id));
  return out;
}

export const readOrder = (key: string): ColId[] => parseOrder(readNumbers(key));

export function writeOrder(key: string, order: readonly ColId[]): void {
  order.forEach((id, i) => writeNumber(key, PREFIX + id, i));
}

/** Move `id` to index `to`, clamped to the ends. */
export function moveColumn(order: readonly ColId[], id: ColId, to: number): ColId[] {
  const rest = order.filter((c) => c !== id);
  rest.splice(Math.min(Math.max(to, 0), rest.length), 0, id);
  return rest;
}

/** Side of a non-subject column that carries its resize handle: the edge facing the 1fr subject. Left means `reverse`. */
export const handleSide = (order: readonly ColId[], id: ColId): "left" | "right" =>
  order.indexOf(id) > order.indexOf("subject") ? "left" : "right";
