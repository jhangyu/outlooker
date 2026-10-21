import {
  Button, Caption1, Caption1Strong, Card, Checkbox, Dropdown, Input, Option, ToggleButton, makeStyles, tokens,
} from "@fluentui/react-components";
import { AddRegular, DismissRegular } from "@fluentui/react-icons";
import { Fragment, useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { FilterOptions } from "../../api/bindings/FilterOptions";
import type { GroupOp } from "../../api/bindings/GroupOp";
import type { RecipientKind } from "../../api/bindings/RecipientKind";
import {
  FIELDS, MAX_BUILDER_DEPTH, countLeaves, newCond, newGroup, updateNode,
  type CondDraft, type FieldKind, type GroupDraft,
} from "./model";

const OP: Record<FieldKind, "contains" | "between" | "is" | "in"> = {
  sender: "contains", recipient: "contains", subject: "contains", body: "contains",
  dateRange: "between", folder: "is", hasAttachment: "is", sourcePst: "in",
};
const KINDS: readonly RecipientKind[] = ["to", "cc", "bcc"];

const useStyles = makeStyles({
  root: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  bar: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  group: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: tokens.spacingHorizontalM,
    borderLeft: `3px solid ${tokens.colorBrandStroke1}`,
  },
  head: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS },
  spacer: { flex: 1 },
  join: { color: tokens.colorNeutralForeground3, paddingLeft: tokens.spacingHorizontalS },
  cond: { display: "grid", gridTemplateColumns: "150px 92px 1fr 28px", alignItems: "center", gap: tokens.spacingHorizontalS },
  value: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, minWidth: 0 },
  grow: { flex: 1, minWidth: 0 },
  foot: { display: "flex", gap: tokens.spacingHorizontalS },
});

interface Ctx {
  set(id: string, f: (n: CondDraft | GroupDraft) => CondDraft | GroupDraft | null): void;
  full: boolean;
  options: FilterOptions;
}

function OpToggle({ op, onChange }: { op: GroupOp; onChange(op: GroupOp): void }) {
  const { t } = useTranslation("search");
  return (
    <span>
      <ToggleButton size="small" checked={op === "and"} onClick={() => onChange("and")}>{t("dsl.and")}</ToggleButton>
      <ToggleButton size="small" checked={op === "or"} onClick={() => onChange("or")}>{t("dsl.or")}</ToggleButton>
    </span>
  );
}

function ValueControl({ cond, ctx }: { cond: CondDraft; ctx: Ctx }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const patch = (p: Partial<CondDraft>) => ctx.set(cond.id, (c) => ({ ...(c as CondDraft), ...p }));
  const folderPaths = useMemo(
    () => [...new Set(ctx.options.folders.map((f) => f.folderPath))].sort(),
    [ctx.options.folders],
  );
  switch (cond.field) {
    case "sender":
    case "subject":
    case "body":
      return (
        <Input className={s.grow} value={cond.text} maxLength={500} aria-label={t("dsl.value")}
          onChange={(_, d) => patch({ text: d.value })} />
      );
    case "recipient":
      return (
        <span className={s.value}>
          <Input className={s.grow} value={cond.text} maxLength={500} aria-label={t("dsl.value")}
            onChange={(_, d) => patch({ text: d.value })} />
          {KINDS.map((k) => (
            <ToggleButton key={k} size="small" checked={cond.kinds.includes(k)}
              onClick={() => patch({ kinds: cond.kinds.includes(k) ? cond.kinds.filter((x) => x !== k) : [...cond.kinds, k] })}>
              {t(`dsl.kinds.${k}`)}
            </ToggleButton>
          ))}
        </span>
      );
    case "dateRange":
      return (
        <span className={s.value}>
          <Input type="date" value={cond.fromDay} aria-label={t("dsl.fromDay")} onChange={(_, d) => patch({ fromDay: d.value })} />
          <Input type="date" value={cond.toDay} aria-label={t("dsl.toDay")} onChange={(_, d) => patch({ toDay: d.value })} />
        </span>
      );
    case "folder":
      return (
        <span className={s.value}>
          <Dropdown className={s.grow} placeholder={t("dsl.pickFolder")} aria-label={t("dsl.pickFolder")}
            value={cond.folderPath} selectedOptions={cond.folderPath ? [cond.folderPath] : []}
            onOptionSelect={(_, d) => patch({ folderPath: d.optionValue ?? "" })}>
            {folderPaths.map((p) => (
              <Option key={p} value={p} text={p}>
                <span style={{ paddingLeft: `${(p.split("/").length - 1) * 12}px` }}>{p.split("/").pop()}</span>
              </Option>
            ))}
          </Dropdown>
          <Checkbox checked={cond.includeSubfolders} label={t("dsl.includeSubfolders")}
            onChange={(_, d) => patch({ includeSubfolders: d.checked === true })} />
        </span>
      );
    case "hasAttachment":
      return (
        <Dropdown className={s.grow} aria-label={t("dsl.value")} value={t(cond.flag ? "dsl.yes" : "dsl.no")}
          selectedOptions={[cond.flag ? "yes" : "no"]} onOptionSelect={(_, d) => patch({ flag: d.optionValue === "yes" })}>
          <Option value="yes">{t("dsl.yes")}</Option>
          <Option value="no">{t("dsl.no")}</Option>
        </Dropdown>
      );
    case "sourcePst":
      return (
        <Dropdown className={s.grow} multiselect placeholder={t("dsl.pickPst")} aria-label={t("dsl.pickPst")}
          value={ctx.options.psts.filter((p) => cond.pstIds.includes(p.id)).map((p) => p.displayName).join(", ")}
          selectedOptions={cond.pstIds.map(String)}
          onOptionSelect={(_, d) => patch({ pstIds: d.selectedOptions.map(Number) })}>
          {ctx.options.psts.map((p) => (
            <Option key={p.id} value={String(p.id)}>{p.displayName}</Option>
          ))}
        </Dropdown>
      );
  }
}

function CondRow({ cond, ctx }: { cond: CondDraft; ctx: Ctx }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  return (
    <div className={s.cond} data-testid="dsl-cond">
      <Dropdown aria-label={t("dsl.fieldLabel")} value={t(`dsl.field.${cond.field}`)} selectedOptions={[cond.field]}
        onOptionSelect={(_, d) => d.optionValue && ctx.set(cond.id, () => ({ ...newCond(d.optionValue as FieldKind), id: cond.id }))}>
        {FIELDS.map((f) => (
          <Option key={f} value={f}>{t(`dsl.field.${f}`)}</Option>
        ))}
      </Dropdown>
      <Caption1>{t(`dsl.op.${OP[cond.field]}`)}</Caption1>
      <ValueControl cond={cond} ctx={ctx} />
      <Button appearance="subtle" size="small" icon={<DismissRegular />} aria-label={t("dsl.remove")}
        onClick={() => ctx.set(cond.id, () => null)} />
    </div>
  );
}

function GroupCard({ group, index, depth, ctx }: { group: GroupDraft; index: number; depth: number; ctx: Ctx }) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const append = (node: CondDraft | GroupDraft) =>
    ctx.set(group.id, (g) => ({ ...(g as GroupDraft), children: [...(g as GroupDraft).children, node] }));
  return (
    <Card appearance="outline" className={s.group}>
      <div className={s.head}>
        <Caption1Strong>{t("dsl.group", { n: index })}</Caption1Strong>
        <Caption1>{t("dsl.match")}</Caption1>
        <OpToggle op={group.op} onChange={(op) => ctx.set(group.id, (g) => ({ ...(g as GroupDraft), op }))} />
        <span className={s.spacer} />
        <Button appearance="subtle" size="small" icon={<DismissRegular />} aria-label={t("dsl.remove")}
          onClick={() => ctx.set(group.id, () => null)} />
      </div>
      {group.children.map((c, i) => (
        <Fragment key={c.id}>
          {i > 0 ? <Caption1 className={s.join}>{t(`dsl.${group.op}`)}</Caption1> : null}
          {c.kind === "group" ? <GroupCard group={c} index={i + 1} depth={depth + 1} ctx={ctx} /> : <CondRow cond={c} ctx={ctx} />}
        </Fragment>
      ))}
      <div className={s.foot}>
        <Button size="small" icon={<AddRegular />} disabled={ctx.full} onClick={() => append(newCond())}>{t("dsl.addCond")}</Button>
        {depth < MAX_BUILDER_DEPTH ? (
          <Button size="small" icon={<AddRegular />} disabled={ctx.full} onClick={() => append(newGroup())}>{t("dsl.addGroup")}</Button>
        ) : null}
      </div>
    </Card>
  );
}

/** Prototype `#dsl` / `#dslGroups`: root bar, then one outline Card per group (brand left border). */
export function DslBuilder({ value, onChange, options, leafBudget }: {
  value: GroupDraft;
  onChange(next: GroupDraft): void;
  options: FilterOptions;
  leafBudget: number;
}) {
  const { t } = useTranslation("search");
  const s = useStyles();
  const ctx: Ctx = {
    set: (id, f) => onChange(updateNode(value, id, f)),
    full: countLeaves(value) >= leafBudget,
    options,
  };
  return (
    <div className={s.root} data-testid="dsl-builder">
      <div className={s.bar}>
        <Caption1>{t("dsl.between")}</Caption1>
        <OpToggle op={value.op} onChange={(op) => onChange({ ...value, op })} />
        <Button size="small" icon={<AddRegular />} disabled={ctx.full}
          onClick={() => onChange({ ...value, children: [...value.children, newGroup()] })}>
          {t("dsl.addGroup")}
        </Button>
        <span className={s.spacer} />
        <Button size="small" appearance="subtle" onClick={() => onChange({ ...value, children: [] })}>{t("dsl.clear")}</Button>
      </div>
      {value.children.map((c, i) =>
        c.kind === "group" ? (
          <GroupCard key={c.id} group={c} index={i + 1} depth={2} ctx={ctx} />
        ) : (
          <CondRow key={c.id} cond={c} ctx={ctx} />
        ),
      )}
    </div>
  );
}
