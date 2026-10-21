import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppStatus } from "./bindings/AppStatus";
import type { ImportProgress } from "./bindings/ImportProgress";
import type { ImportStatusChanged } from "./bindings/ImportStatusChanged";
import type { LibraryChanged } from "./bindings/LibraryChanged";
import type { DataDirMoveProgress } from "./bindings/DataDirMoveProgress";
import type { MaintenanceState } from "./bindings/MaintenanceState";

export const EVENTS = {
  appStatusChanged: "app:status-changed",
  importProgress: "import:progress",
  importStatusChanged: "import:status-changed",
  libraryChanged: "library:changed",
  dataDirMoveProgress: "data-dir:move-progress",
  maintenanceStatusChanged: "maintenance:status-changed",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

export interface EventPayloads {
  "app:status-changed": AppStatus;
  "import:progress": ImportProgress;
  "import:status-changed": ImportStatusChanged;
  "library:changed": LibraryChanged;
  "data-dir:move-progress": DataDirMoveProgress;
  "maintenance:status-changed": MaintenanceState;
}

export function onEvent<K extends EventName>(name: K, handler: (payload: EventPayloads[K]) => void): Promise<UnlistenFn> {
  return listen<EventPayloads[K]>(name, (event) => handler(event.payload));
}
