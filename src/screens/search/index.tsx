import {
  Body1Strong, Button, Caption1, CounterBadge, MessageBar, MessageBarActions, MessageBarBody, SearchBox, Spinner, ToggleButton,
  makeStyles, tokens,
} from "@fluentui/react-components";
import { ChevronDownRegular, ChevronUpRegular, MailInboxRegular, SearchRegular } from "@fluentui/react-icons";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { EmptyState, formatAppError, formatNumber } from "../../components/common";
import { DslBuilder, MAX_LEAVES, countLeaves, newGroup, toConditionGroup, type GroupDraft } from "../../components/dsl";
import { MailList, ReadingPane, useMailPages } from "../../components/mail";
import { EMPTY_REFINE, LEAVES_PER_TERM, composeQuery, refineNodes, splitTerms, type RefineState } from "./compose";
import { RefinePane } from "./RefinePane";

const DEBOUNCE_MS = 300;
const COUNT_CAP = 10_000;
/** Non-fresh object: passes Fluent's slot type, carries the E2E selector to the native <input>. */
const SEARCH_INPUT = { "data-testid": "search-box", maxLength: 500 };

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "260px 1fr", height: "100%", minHeight: 0 },
  main: { display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0 },
  bar: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground1,
  },
  box: { flex: 1, maxWidth: "880px", height: "36px" },
  toggle: { flexShrink: 0 },
  summary: { color: tokens.colorNeutralForeground3, whiteSpace: "nowrap", flexShrink: 0 },
  dsl: {
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`, maxHeight: "40%", overflowY: "auto",
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground2,
  },
  hints: {
    display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL} 0`,
  },
  results: {
    flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "minmax(420px, 34%) 1fr",
    margin: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalS} 0`, overflow: "hidden", boxShadow: tokens.shadow2,
    borderRadius: `${tokens.borderRadiusLarge} ${tokens.borderRadiusLarge} 0 0`, backgroundColor: tokens.colorNeutralBackground1,
  },
  listPane: { display: "flex", flexDirection: "column", minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  count: { color: tokens.colorNeutralForeground3 },
  fill: { flex: 1, minHeight: 0 },
  center: { display: "grid", placeItems: "center", height: "100%" },
});

interface Inputs {
  text: string;
  refine: RefineState;
  draft: GroupDraft;
}

/** Prototype `#screen-search`. */
export function SearchScreen() {
  const { t, i18n } = useTranslation("search");
  const s = useStyles();
  const qc = useQueryClient();
  const status = useAppStatus();
  const { go } = useNavigation();
  const lng = i18n.language;

  const [inputs, setInputs] = useState<Inputs>(() => ({ text: "", refine: EMPTY_REFINE, draft: newGroup("and", []) }));
  const [debounced, setDebounced] = useState(inputs);
  useEffect(() => {
    const h = setTimeout(() => setDebounced(inputs), DEBOUNCE_MS);
    return () => clearTimeout(h);
  }, [inputs]);
  const [dslOpen, setDslOpen] = useState(false);

  const composed = useMemo(
    () => composeQuery(debounced.text, debounced.refine, toConditionGroup(debounced.draft)),
    [debounced],
  );
  const hasMail = status.counts.messages > 0;
  const query = hasMail ? composed.query : null;

  // Part 03 §5.2 stale requests: every new composed query gets the next sequence number.
  const key = JSON.stringify(query);
  const [seqState, setSeqState] = useState({ key, seq: 0 });
  // Library counts when the shown list was loaded: a different count later means new mail (Part 03 §9, no list reload).
  const [baseline, setBaseline] = useState(status.counts);
  if (seqState.key !== key) {
    setSeqState({ key, seq: seqState.seq + 1 });
    setBaseline(status.counts);
  }
  const seq = seqState.seq;
  const newMail = query !== null && (status.counts.messages !== baseline.messages || status.counts.psts !== baseline.psts);

  const pages = useMailPages(query, seq);
  const count = useQuery({
    queryKey: ["searchCount", seq, query],
    queryFn: () => api.searchCount(query!),
    enabled: query !== null,
    staleTime: Infinity, // changes only through seq or refresh()
    gcTime: Infinity, // a hidden screen has no observer, and the effect below keeps one entry
  });
  useEffect(() => {
    qc.removeQueries({ queryKey: ["searchCount"], predicate: (x) => x.queryKey[1] !== seq });
  }, [qc, seq]);
  const filters = useQuery({ queryKey: ["filterOptions", status.counts.messages, status.counts.psts], queryFn: () => api.filterOptions(), placeholderData: keepPreviousData });

  const [picked, setPicked] = useState<{ seq: number; id: number } | null>(null);
  const firstId = pages.rows[0]?.id ?? null;
  const selectedId = picked && picked.seq === seq ? picked.id : firstId;

  const terms = splitTerms(inputs.text).length;
  const builderLeaves = countLeaves(inputs.draft);
  const leafBudget = MAX_LEAVES - LEAVES_PER_TERM * terms - refineNodes(inputs.refine, new Date()).length;

  const clearAll = () => {
    const cleared = { text: "", refine: EMPTY_REFINE, draft: newGroup("and", []) };
    setInputs(cleared);
    setDebounced(cleared);
  };
  const refresh = () => {
    pages.reset();
    void qc.invalidateQueries({ queryKey: ["searchCount", seq] });
    setBaseline(status.counts);
  };

  let countText: string | null = null;
  if (query !== null && count.data) {
    countText = count.data.capped
      ? t("countCapped", { n: formatNumber(COUNT_CAP, lng) })
      : t("count", { n: formatNumber(count.data.count, lng) });
  }

  let list;
  if (!hasMail) {
    list = (
      <EmptyState icon={<MailInboxRegular />} title={t("empty.title")} body={t("empty.body")}
        action={<Button appearance="primary" onClick={() => go("import")}>{t("common:action.goImport")}</Button>} />
    );
  } else if (query === null) {
    list = <EmptyState icon={<SearchRegular />} title={t("idle.title")} body={t("idle.body")} />;
  } else if (pages.error) {
    list = (
      <MessageBar intent="error">
        <MessageBarBody>{formatAppError(pages.error, t)}</MessageBarBody>
      </MessageBar>
    );
  } else if (pages.isLoading) {
    list = (
      <div className={s.center}>
        <Spinner />
      </div>
    );
  } else if (pages.rows.length === 0) {
    list = (
      <EmptyState icon={<SearchRegular />} title={t("zero.title")} body={t("zero.body")}
        action={<Button onClick={clearAll}>{t("zero.clear")}</Button>} />
    );
  } else {
    list = (
      <MailList rows={pages.rows} selectedId={selectedId} onSelect={(id) => setPicked({ seq, id })}
        hasMore={pages.hasMore} capped={pages.capped} loadingMore={pages.isFetchingNextPage}
        onEndReached={() => void pages.fetchNextPage()} ariaLabel={t("list.label")} />
    );
  }

  const hints = [
    pages.shortQuery ? (
      <MessageBar key="short" intent="warning" data-testid="short-query-hint">
        <MessageBarBody>{t("shortQueryHint")}</MessageBarBody>
      </MessageBar>
    ) : null,
    composed.problem ? (
      <MessageBar key="problem" intent="error">
        <MessageBarBody>{t(`problem.${composed.problem}`)}</MessageBarBody>
      </MessageBar>
    ) : null,
    newMail ? (
      <MessageBar key="newMail" intent="info" data-testid="new-mail-bar">
        <MessageBarBody>{t("newMail")}</MessageBarBody>
        <MessageBarActions>
          <Button size="small" data-testid="new-mail-refresh" onClick={refresh}>{t("refresh")}</Button>
        </MessageBarActions>
      </MessageBar>
    ) : null,
  ].filter((h) => h !== null);

  return (
    <div className={s.root}>
      <RefinePane value={inputs.refine} onChange={(refine) => setInputs((p) => ({ ...p, refine }))} options={filters.data} />
      <div className={s.main}>
        <div className={s.bar}>
          <SearchBox className={s.box} appearance="filled-darker" input={SEARCH_INPUT} placeholder={t("box.placeholder")} aria-label={t("box.label")}
            value={inputs.text} onChange={(_, d) => setInputs((p) => ({ ...p, text: d.value }))} />
          <ToggleButton data-testid="dsl-toggle" className={s.toggle} checked={dslOpen} onClick={() => setDslOpen((o) => !o)}
            icon={dslOpen ? <ChevronUpRegular /> : <ChevronDownRegular />} iconPosition="after">
            {t("dsl.toggle")}
            <CounterBadge count={builderLeaves} size="small" />
          </ToggleButton>
          {builderLeaves > 0 ? <Caption1 className={s.summary}>{t("dsl.summary", { n: builderLeaves })}</Caption1> : null}
        </div>
        {dslOpen ? (
          <div className={s.dsl}>
            <DslBuilder value={inputs.draft} onChange={(draft) => setInputs((p) => ({ ...p, draft }))}
              options={filters.data ?? { psts: [], folders: [] }} leafBudget={leafBudget} />
          </div>
        ) : null}
        {hints.length > 0 ? <div className={s.hints}>{hints}</div> : null}
        {/* Prototype keeps the two-pane grid for every state; empty and zero states render inside the list pane. */}
        <div className={s.results}>
          <section className={s.listPane}>
            <div className={s.paneHead}>
              <Body1Strong>{t("results")}</Body1Strong>
              {countText ? <Caption1 className={s.count} data-testid="result-count">{countText}</Caption1> : null}
            </div>
            <div className={s.fill}>{list}</div>
          </section>
          {hasMail ? <ReadingPane mailId={selectedId} /> : <div />}
        </div>
      </div>
    </div>
  );
}
