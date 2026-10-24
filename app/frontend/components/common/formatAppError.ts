import type { TFunction } from "i18next";
import type { AppError } from "../../api/bindings/AppError";

/** Maps AppError.kind to a localized sentence (Part 03 §1 Localization). SqlError.message is shown verbatim. */
export function formatAppError(err: AppError, t: TFunction): string {
  switch (err.kind) {
    case "busy":
      return t(`common:error.busy.${err.reason}`);
    case "sqlError":
      return err.message;
    case "queryTimeout":
      return t("common:error.queryTimeout", { seconds: Math.round(err.timeoutMs / 1000) });
    case "fileAccess":
      return t(err.reason === "insufficientSpace" ? "common:error.insufficientSpace" : "common:error.fileAccess", { path: err.path });
    case "notAPst":
      return t("common:error.notAPst", { path: err.path });
    case "invalidInput":
      if (err.field === "dataDir" && err.reason === "notEmpty") return t("common:error.dataDirNotEmpty");
      return t("common:error.invalidInput", { field: err.field });
    default:
      return t(`common:error.${err.kind}`);
  }
}
