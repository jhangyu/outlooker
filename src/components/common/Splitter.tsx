import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { useLayoutEffect, useRef, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

const STEP = 16;

const useStyles = makeStyles({
  base: {
    position: "absolute", zIndex: 1, touchAction: "none", outlineStyle: "none",
    ":focus-visible": { backgroundColor: tokens.colorBrandStroke1 },
  },
  vertical: { width: "6px", cursor: "col-resize" },
  horizontal: { height: "6px", cursor: "row-resize" },
});

export type SplitterProps = {
  /** "vertical" is a vertical line between side-by-side panes (col-resize); "horizontal" separates stacked panes. */
  orientation: "vertical" | "horizontal";
  /** Size in px of the pane before the border; undefined while it follows its content. */
  value: number | undefined;
  min: number;
  /** Current maximum (already bounded by the container), undefined until the container is measured. */
  max?: number;
  /** Receives the container's content size along this axis, now and on every resize. */
  onExtent?: (px: number) => void;
  /** Element whose size bounds the split; default is the parent element. */
  container?: () => HTMLElement | null | undefined;
  onChange: (px: number) => void;
  onReset: () => void;
  /** The sized pane is AFTER the border (right/bottom pane): dragging toward the start grows it. */
  reverse?: boolean;
  /** Current px size of the sized pane, needed only while `value` is undefined (fr or auto tracks). */
  measure?: () => number;
  /** Position inside the (position: relative) parent, e.g. left/top and the grid area. */
  style?: CSSProperties;
  label?: string;
};

/**
 * Invisible 6 px hit area. Place it as a child of the layout container (`position: relative`), centred on the border.
 * Drag (pointer capture, `reverse` for a pane after the border), arrows move 16 px, Home/End jump to min/max, double-click resets.
 */
export function Splitter({ orientation, value, min, max, onExtent, container, onChange, onReset, reverse, measure, style, label }: SplitterProps) {
  const s = useStyles();
  const { t } = useTranslation("common");
  const vertical = orientation === "vertical";
  const self = useRef<HTMLDivElement>(null);
  const drag = useRef<{ from: number; start: number } | null>(null);
  const report = useRef(onExtent);
  report.current = onExtent;
  const pick = useRef(container);
  pick.current = container;

  useLayoutEffect(() => {
    const host = pick.current?.() ?? self.current?.parentElement;
    if (!host) return;
    const read = () => {
      const cs = getComputedStyle(host);
      const pad = vertical ? parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) : parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
      report.current?.((vertical ? host.clientWidth : host.clientHeight) - (pad || 0));
    };
    read();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(read);
    ro.observe(host);
    return () => ro.disconnect();
  }, [vertical]);

  const dir = reverse ? -1 : 1;
  const base = () => value ?? measure?.() ?? min;
  const apply = (px: number) => onChange(Math.min(Math.max(px, min), max ?? Infinity));

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { from: vertical ? e.clientX : e.clientY, start: base() };
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (drag.current) apply(drag.current.start + dir * ((vertical ? e.clientX : e.clientY) - drag.current.from));
  };
  const end = () => (drag.current = null);
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const [dec, inc] = vertical ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
    if (e.key === dec) apply(base() - dir * STEP);
    else if (e.key === inc) apply(base() + dir * STEP);
    else if (e.key === "Home") apply(min);
    else if (e.key === "End" && max !== undefined) apply(max);
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={self}
      role="separator"
      tabIndex={0}
      aria-label={label ?? t("action.resize")}
      aria-orientation={orientation}
      aria-valuenow={Math.round(value ?? (measure?.() || min))}
      aria-valuemin={min}
      aria-valuemax={max ?? min}
      className={mergeClasses(s.base, vertical ? s.vertical : s.horizontal)}
      style={style}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
    />
  );
}
