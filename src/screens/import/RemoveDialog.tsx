import {
  Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, MessageBar, MessageBarBody, makeStyles,
} from "@fluentui/react-components";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { AppError } from "../../api/bindings/AppError";
import type { PstSource } from "../../api/bindings/PstSource";
import { formatAppError, formatNumber, toAppError, useAppToast } from "../../components/common";

const useStyles = makeStyles({
  surface: { maxWidth: "520px" },
  // A long unbroken PST name wraps instead of overflowing the dialog.
  title: { overflowWrap: "anywhere" },
});

/** Only the PST's own message count is shown: the exact deleted / kept split is out of scope (user ruling). */
export function RemoveDialog({ pst, onClose }: { pst: PstSource; onClose(): void }) {
  const { t, i18n } = useTranslation("import");
  const s = useStyles();
  const qc = useQueryClient();
  const toast = useAppToast();
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.pstRemove(pst.id);
      await qc.invalidateQueries({ queryKey: ["pstList"] });
      toast.notify(t("remove.done", { name: pst.displayName }));
      onClose();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open modalType="modal" onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface className={s.surface}>
        <DialogBody>
          <DialogTitle className={s.title}>{t("remove.title", { name: pst.displayName })}</DialogTitle>
          <DialogContent>
            <p>{t("remove.body", { n: formatNumber(pst.messageCount, i18n.language) })}</p>
            {error ? (
              <MessageBar intent="error">
                <MessageBarBody>{formatAppError(error, t)}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common:action.cancel")}</Button>
            <Button appearance="primary" onClick={confirm} disabled={busy} data-testid="remove-confirm">
              {t("remove.confirm")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
