import { makeStyles, tokens } from "@fluentui/react-components";
import markUrl from "./mark.svg";

const useStyles = makeStyles({
  mark: { display: "block", flexShrink: 0 },
  // Light tile behind the full-colour mark where it sits on a brand-coloured surface.
  chip: {
    display: "grid", placeItems: "center", flexShrink: 0,
    borderRadius: tokens.borderRadiusMedium, backgroundColor: tokens.colorNeutralBackground1,
  },
});

/** Decorative Outlooker mark. `<img>` keeps the SVG's mask/gradient ids out of the page DOM. */
export function BrandMark({ size, chip = false }: { size: number; chip?: boolean }) {
  const s = useStyles();
  if (!chip) return <img src={markUrl} alt="" width={size} height={size} className={s.mark} />;
  const inner = Math.round(size * 0.8);
  return (
    <span className={s.chip} style={{ width: size, height: size }} aria-hidden>
      <img src={markUrl} alt="" width={inner} height={inner} className={s.mark} />
    </span>
  );
}
