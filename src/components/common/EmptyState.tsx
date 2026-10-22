import { Body1, Subtitle1, makeStyles, tokens } from "@fluentui/react-components";
import type { JSX, ReactNode } from "react";

export interface EmptyStateProps {
  icon: JSX.Element;
  title: string;
  body?: string;
  action?: ReactNode;
  testId?: string;
}

const useStyles = makeStyles({
  root: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    gap: tokens.spacingVerticalS,
    padding: tokens.spacingVerticalXXL,
    textAlign: "center",
    height: "100%",
    boxSizing: "border-box",
  },
  art: {
    width: "72px",
    height: "72px",
    borderRadius: tokens.borderRadiusCircular,
    display: "grid",
    placeItems: "center",
    backgroundColor: tokens.colorBrandBackground2,
    color: tokens.colorBrandForeground1,
    fontSize: "36px",
    marginBottom: tokens.spacingVerticalS,
  },
  body: { color: tokens.colorNeutralForeground3, maxWidth: "420px" },
});

export function EmptyState({ icon, title, body, action, testId }: EmptyStateProps) {
  const s = useStyles();
  return (
    <div className={s.root} data-testid={testId ?? "empty-state"}>
      <div className={s.art} aria-hidden>
        {icon}
      </div>
      <Subtitle1>{title}</Subtitle1>
      {body ? <Body1 className={s.body}>{body}</Body1> : null}
      {action}
    </div>
  );
}
