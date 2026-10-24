import type { MailDetail } from "../../api/bindings/MailDetail";
import type { MailRow } from "../../api/bindings/MailRow";

/**
 * jsdom has no layout, so the virtualizer sees a 0 px viewport and renders no rows. virtual-core reads
 * element.clientHeight / clientWidth (and skips ResizeObserver when jsdom lacks it); this defines both on
 * HTMLElement.prototype and returns the restore function.
 */
export function stubViewport(height: number): () => void {
  const proto = HTMLElement.prototype;
  const saved = (["clientHeight", "clientWidth"] as const).map((k) => [k, Object.getOwnPropertyDescriptor(proto, k)] as const);
  Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => height });
  Object.defineProperty(proto, "clientWidth", { configurable: true, get: () => 1000 });
  return () => {
    for (const [k, d] of saved) {
      if (d) Object.defineProperty(proto, k, d);
      else Reflect.deleteProperty(proto, k);
    }
  };
}

export function makeMailRow(id: number, p: Partial<MailRow> = {}): MailRow {
  return {
    id,
    sentAt: "2026-10-01T08:00:00Z",
    from: { name: "Ann Lee", address: "ann@contoso.example" },
    toPreview: "Bob",
    recipientCount: 1,
    subject: `Subject ${id}`,
    snippet: "snippet",
    hasAttachments: false,
    occurrenceCount: 1,
    ...p,
  };
}

export function makeMailDetail(p: Partial<MailDetail> = {}): MailDetail {
  return {
    id: 5,
    messageId: "<q1@contoso.example>",
    subject: "Quote Q1",
    sentAt: "2026-10-01T08:00:00Z",
    receivedAt: "2026-10-01T08:01:00Z",
    from: { name: "Ann Lee", address: "ann@contoso.example" },
    to: [{ name: "Bob", address: "bob@x.example" }],
    cc: [],
    bcc: [],
    bodyHtml: "<p>Hello</p>",
    bodyText: "Hello",
    attachments: [{ id: 1, fileName: "quote.pdf", sizeBytes: 2048, mimeType: "application/pdf" }],
    occurrences: [{ pstId: 1, pstDisplayName: "archive", pstPath: "/p/archive.pst", folderPath: "Inbox/Projects", pstItemId: 77 }],
    ...p,
  };
}
