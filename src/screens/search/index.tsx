import {
  Button, Caption1, CounterBadge, MessageBar, MessageBarActions, MessageBarBody, SearchBox, Spinner, Subtitle2, ToggleButton,
  makeStyles, tokens,
} from "@fluentui/react-components";
import { ChevronDownRegular, ChevronUpRegular, MailInboxRegular, SearchRegular } from "@fluentui/react-icons";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import { onEvent } from "../../api/events";
import { useAppStatus, useNavigation } from "../../app/AppStatusContext";
import { EmptyState, formatAppError, formatNumber } from "../../components/common";
import { DslBuilder, countLeaves, newGroup, toConditionGroup, type GroupDraft } from "../../components/dsl";
import { MailList, ReadingPane, useMailPages } from "../../components/mail";
import { EMPTY_REFINE, composeQuery, refineNodes, splitTerms, type RefineState } from "./compose";
import { RefinePane } from "./RefinePane";

const DEBOUNCE_MS = 300;
const COUNT_CAP = 10_000;
/** Non-fresh object: passes Fluent's slot type, carries the E2E selector to the native <input>. */
const SEARCH_INPUT = { "data-testid": "search-box", maxLength: 500 };

const useStyles = makeStyles({
  root: { display: "grid", gridTemplateColumns: "260px 1fr", height: "100%", minHeight: 0 },
  main: { display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0 },
  bar: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`,
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground1,
  },
  box: { flex: 1, maxWidth: "880px" },
  summary: { color: tokens.colorNeutralForeground3 },
  dsl: {
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalL}`, maxHeight: "40%", overflowY: "auto",
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`, backgroundColor: tokens.colorNeutralBackground2,
  },
  hints: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, padding: `0 ${tokens.spacingHorizontalL}` },
  results: { flex: 1, minHeight: 0, display: "grid", gridTemplateColumns: "minmax(420px, 34%) 1fr" },
  listPane: { display: "flex", flexDirection: "column", minHeight: 0, borderRight: `1px solid ${tokens.colorNeutralStroke2}` },
  paneHead: {
    display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, minHeight: "44px", boxSizing: "border-box",
    padding: `0 ${tokens.spacingHorizontalM}`, borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
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
  const [newMail, setNewMail] = useState(false);

  const composed = useMemo(
    () => composeQuery(debounced.text, debounced.refine, toConditionGroup(debounced.draft)),
    [debounced],
  );
  const hasMail = status.counts.messages > 0;
  const query = hasMail ? composed.query : null;

  // Part 03 §5.2 stale requests: every new composed query gets the next sequence number.
  const key = JSON.stringify(query);
  const [seqState, setSeqState] = useState({ key, seq: 0 });
  if (seqState.key !== key) setSeqState({ key, seq: seqState.seq + 1 });
  const seq = seqState.seq;

  const pages = useMailPages(query, seq);
  const count = useQuery({
    queryKey: ["searchCount", seq, query],
    queryFn: () => api.searchCount(query!),
    enabled: query !== null,
  });
  const filters = useQuery({ queryKey: ["filterOptions"], queryFn: () => api.filterOptions() });

  const [picked, setPicked] = useState<{ seq: number; id: number } | null>(null);
  const firstId = pages.rows[0]?.id ?? null;
  const selectedId = picked && picked.seq === seq ? picked.id : firstId;

  const showingRef = useRef(false);
  useEffect(() => {
    showingRef.current = query !== null;
  }, [query]);
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let alive = true;
    void onEvent("library:changed", () => {
      void qc.invalidateQueries({ queryKey: ["filterOptions"] });
      if (showingRef.current) setNewMail(true); // never refetches searchMail (Part 03 §9)
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [qc]);

  const terms = splitTerms(inputs.text).length;
  const builderLeaves = countLeaves(inputs.draft);
  const leafBudget = 50 - 4 * terms - refineNodes(inputs.refine, new Date()).length;

  const clearAll = () => {
    const cleared = { text: "", refine: EMPTY_REFINE, draft: newGroup("and", []) };
    setInputs(cleared);
    setDebounced(cleared);
  };
  const refresh = () => {
    pages.reset();
    void qc.invalidateQueries({ queryKey: ["searchCount", seq] });
    setNewMail(false);
  };

  let countText: string | null = null;
  if (query !== null && count.data) {
    countText = count.data.capped
      ? t("countCapped", { n: formatNumber(COUNT_CAP, lng) })
      : t("count", { n: formatNumber(count.data.count, lng) });
  }

  let results;
  if (!hasMail) {
    results = (
      <EmptyState icon={<MailInboxRegular />} title={t("empty.title")} body={t("empty.body")}
        action={<Button appearance="primary" onClick={() => go("import")}>{t("common:action.goImport")}</Button>} />
    );
  } else if (query === null) {
    results = <EmptyState icon={<SearchRegular />} title={t("idle.title")} body={t("idle.body")} />;
  } else if (pages.error) {
    results = (
      <MessageBar intent="error">
        <MessageBarBody>{formatAppError(pages.error, t)}</MessageBarBody>
      </MessageBar>
    );
  } else if (pages.isLoading) {
    results = (
      <div className={s.center}>
        <Spinner />
      </div>
    );
  } else if (pages.rows.length === 0) {
    results = (
      <EmptyState icon={<SearchRegular />} title={t("zero.title")} body={t("zero.body")}
        action={<Button onClick={clearAll}>{t("zero.clear")}</Button>} />
    );
  } else {
    results = (
      <div className={s.results}>
        <section className={s.listPane}>
          <div className={s.paneHead}>
            <Subtitle2>{t("results")}</Subtitle2>
            {countText ? <Caption1 className={s.count} data-testid="result-count">{countText}</Caption1> : null}
          </div>
          <div className={s.fill}>
            <MailList rows={pages.rows} selectedId={selectedId} onSelect={(id) => setPicked({ seq, id })}
              hasMore={pages.hasMore} capped={pages.capped} loadingMore={pages.isFetchingNextPage}
              onEndReached={() => void pages.fetchNextPage()} ariaLabel={t("list.label")} />
          </div>
        </section>
        <ReadingPane mailId={selectedId} />
      </div>
    );
  }

  return (
    <div className={s.root}>
      <RefinePane value={inputs.refine} onChange={(refine) => setInputs((p) => ({ ...p, refine }))} options={filters.data} />
      <div className={s.main}>
        <div className={s.bar}>
          <SearchBox className={s.box} input={SEARCH_INPUT} placeholder={t("box.placeholder")} aria-label={t("box.label")}
            value={inputs.text} onChange={(_, d) => setInputs((p) => ({ ...p, text: d.value }))} />
          <ToggleButton data-testid="dsl-toggle" checked={dslOpen} onClick={() => setDslOpen((o) => !o)}
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
        <div className={s.hints}>
          {pages.shortQuery ? (
            <MessageBar intent="warning" data-testid="short-query-hint">
              <MessageBarBody>{t("shortQueryHint")}</MessageBarBody>
            </MessageBar>
          ) : null}
          {composed.problem ? (
            <MessageBar intent="error">
              <MessageBarBody>{t(`problem.${composed.problem}`)}</MessageBarBody>
            </MessageBar>
          ) : null}
          {newMail ? (
            <MessageBar intent="info" data-testid="new-mail-bar">
              <MessageBarBody>{t("newMail")}</MessageBarBody>
              <MessageBarActions>
                <Button size="small" data-testid="new-mail-refresh" onClick={refresh}>{t("refresh")}</Button>
              </MessageBarActions>
            </MessageBar>
          ) : null}
        </div>
        <div className={s.fill}>{results}</div>
      </div>
    </div>
  );
}
