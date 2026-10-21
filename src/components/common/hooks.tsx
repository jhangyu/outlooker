import { Toast, ToastTitle, useToastController } from "@fluentui/react-components";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../../api/commands";
import type { Language } from "../../api/bindings/Language";
import { formatAppError, toAppError } from "./formatAppError";

export function useAppToast() {
  const { dispatchToast } = useToastController("app");
  const { t } = useTranslation();
  return useMemo(() => {
    const notify = (text: string, intent: "success" | "error" | "info" = "success") =>
      dispatchToast(
        <Toast>
          <ToastTitle>{text}</ToastTitle>
        </Toast>,
        { intent },
      );
    return { notify, error: (e: unknown) => notify(formatAppError(toAppError(e), t), "error") };
  }, [dispatchToast, t]);
}

export function useLanguageSwitch(): (lng: Language) => Promise<void> {
  const { i18n } = useTranslation();
  const toast = useAppToast();
  return useCallback(
    async (lng: Language) => {
      await i18n.changeLanguage(lng);
      try {
        await api.settingsUpdate({ language: lng, webpQuality: null });
      } catch (e) {
        toast.error(e);
      }
    },
    [i18n, toast],
  );
}
