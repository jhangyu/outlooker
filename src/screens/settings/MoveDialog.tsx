import {
  Body1, Button, Caption1, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, MessageBar,
  MessageBarBody, ProgressBar,
} from "@fluentui/react-components";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { DataDirMoveProgress } from "../../api/bindings/DataDirMoveProgress";
import { formatAppError, formatBytes, toAppError, useAppToast } from "../../components/common";
import { setMoveSession, useMoveSession } from "./moveSession";

/**
 * Progress arrives as `data-dir:move-progress`; boot writes it into the ["moveProgress"] query (Part 03 §8.2).
 * "Started" is the frame-level move session, so AppFrame keeps this dialog mounted while the phase leaves ready.
 */
export function MoveDialog({ target, onClose }: { target: string; onClose(): void }) {
  const { t, i18n } = useTranslation("settings");
  const lng = i18n.language;
  const qc = useQueryClient();
  const toast = useAppToast();
  const started = useMoveSession() !== null;
  const [error, setError] = useState<AppError | null>(null);
  const progress = useQuery<DataDirMoveProgress | null>({
    queryKey: ["moveProgress"],
    queryFn: () => null,
    enabled: false,
    initialData: null,
  });
  const p = started ? progress.data : null;

  const close = useCallback(() => {
    setMoveSession(qc, null);
    onClose();
  }, [qc, onClose]);

  useEffect(() => {
    if (p?.phase === "done") {
      toast.notify(t("move.done"));
      void qc.invalidateQueries({ queryKey: ["settings"] });
      close();
    }
  }, [p?.phase, toast, t, qc, close]);

  const confirm = async () => {
    qc.setQueryData(["moveProgress"], null);
    setError(null);
    // Set before the call: the phase may leave ready before data_dir_move returns.
    setMoveSession(qc, { target });
    try {
      await api.dataDirMove(target);
    } catch (e) {
      setMoveSession(qc, null);
      setError(toAppError(e));
    }
  };
  const blocking = started && p?.phase !== "failed";

  return (
    <Dialog open modalType={blocking ? "alert" : "modal"} onOpenChange={(_, d) => !d.open && !blocking && close()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{t("move.title")}</DialogTitle>
          <DialogContent>
            {!started ? <Body1>{t("move.body", { target })}</Body1> : null}
            {started ? (
              <>
                <Body1>{t(`move.phase.${p?.phase ?? "stoppingDb"}`)}</Body1>
                <ProgressBar value={p && p.bytesTotal > 0 ? p.bytesCopied / p.bytesTotal : undefined} />
                {p && p.bytesTotal > 0 ? (
                  <Caption1>{t("move.progress", { copied: formatBytes(p.bytesCopied, lng), total: formatBytes(p.bytesTotal, lng) })}</Caption1>
                ) : null}
              </>
            ) : null}
            {error ? (
              <MessageBar intent="error">
                <MessageBarBody>{formatAppError(error, t)}</MessageBarBody>
              </MessageBar>
            ) : null}
            {p?.phase === "failed" && p.error ? (
              <MessageBar intent="error">
                <MessageBarBody>{formatAppError(p.error, t)}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            {!started ? (
              <>
                <Button onClick={onClose}>{t("common:action.cancel")}</Button>
                <Button appearance="primary" onClick={confirm}>
                  {t("move.confirm")}
                </Button>
              </>
            ) : null}
            {started && p?.phase === "failed" ? <Button onClick={close}>{t("common:action.close")}</Button> : null}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
