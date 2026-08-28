import type { CSSProperties, SVGProps } from "react";
const AMBER = "#f59e0b";
const VIOLET = "#a684ff";
const SPARK = "#fde68a";
const MOLTEN = "M50 60 C68 60 74 33 94 32";
const STREAMS: Array<[d: string, delay: string]> = [
  ["M50 60 C68 60 78 50 94 50", "1.18s"],
  ["M50 60 C68 60 78 70 94 70", "1.26s"],
  ["M50 60 C68 60 74 87 94 88", "1.34s"],
];
const DOTS: Array<[cy: number, fill: string, delay: string]> = [
  [32, AMBER, "1.14s"],
  [50, VIOLET, "1.46s"],
  [70, VIOLET, "1.54s"],
  [88, VIOLET, "1.62s"],
];
export function AnimatedForgeMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 120 120" fill="none" aria-hidden {...props}>
      <mask id="forgemark-f-animated">
        <rect x="8" y="38" width="44" height="44" rx="11" fill="#fff" />
        <rect x="20" y="47" width="7.5" height="26" rx="1.5" fill="#000" />
        <rect x="20" y="47" width="21" height="7.5" rx="1.5" fill="#000" />
        <rect x="20" y="58" width="15" height="7.5" rx="1.5" fill="#000" />
      </mask>
      { }
      <path
        d={MOLTEN}
        stroke={AMBER}
        strokeWidth="4.5"
        strokeLinecap="round"
        strokeDasharray="72"
        className="fm-draw"
        style={{ animationDelay: "0.7s", animationDuration: "0.5s" }}
      />
      {STREAMS.map(([d, delay]) => (
        <path
          key={d}
          d={d}
          stroke={VIOLET}
          strokeWidth="4.5"
          strokeLinecap="round"
          strokeDasharray="72"
          className="fm-draw fm-draw--quick"
          style={{ animationDelay: delay }}
        />
      ))}
      {DOTS.map(([cy, fill, delay]) => (
        <circle
          key={cy}
          cx="98"
          cy={cy}
          r="5.5"
          fill={fill}
          className={cy === 32 ? "fm-pop fm-pop--flare" : "fm-pop"}
          style={{ animationDelay: delay, color: fill }}
        />
      ))}
      { }
      <rect
        x="9"
        y="39"
        width="42"
        height="42"
        rx="10"
        fill="none"
        stroke={VIOLET}
        strokeWidth="2"
        strokeDasharray="168"
        className="fm-outline"
      />
      <rect
        x="8"
        y="38"
        width="44"
        height="44"
        rx="11"
        fill={VIOLET}
        mask="url(#forgemark-f-animated)"
        className="fm-flood"
      />
      { }
      <circle
        r="3"
        fill={SPARK}
        className="fm-spark"
        style={{ offsetPath: `path('${MOLTEN}')` } as CSSProperties}
      />
    </svg>
  );
}
