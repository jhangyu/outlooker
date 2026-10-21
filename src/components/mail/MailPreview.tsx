import { makeStyles, tokens } from "@fluentui/react-components";

const useStyles = makeStyles({
  frame: { display: "block", width: "100%", height: "100%", border: "0", backgroundColor: tokens.colorNeutralBackground1 },
});

/**
 * The only place that renders mail HTML (Part 03 §5.6): empty sandbox — no scripts, no same-origin, no forms,
 * no popups, no navigation. Never add an allow-* token. The element is not keyed by mail, so a new `html`
 * replaces srcdoc on the same iframe.
 */
export function MailPreview({ html, title }: { html: string; title: string }) {
  const s = useStyles();
  return <iframe className={s.frame} data-testid="mail-preview" sandbox="" srcDoc={html} referrerPolicy="no-referrer" title={title} />;
}
