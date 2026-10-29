import { detectPackage } from "../lib/typosquat";
import { useAsync } from "../lib/useAsync";
import { ErrorState, Loading } from "./ui";

const SIGNAL_LABEL: Record<string, string> = {
  distance: "Near-miss name",
  keyboard: "Keyboard typo",
  homoglyph: "Lookalike characters",
  digit_swap: "Digit swap",
  scope_confusion: "Scope confusion",
  combosquat: "Combosquat",
};

/** Runs the typosquat scanner (wasm) on a bounty's package and shows which signals fire. */
export function ScannerDetection({ name }: { name: string }) {
  const state = useAsync(() => detectPackage(name), `scan:${name}`);

  return (
    <section className="scanner">
      <h2>Scanner detection</h2>
      {state.status === "loading" && <Loading label="Scanning package…" />}
      {state.status === "error" && <ErrorState error={state.error} onRetry={state.reload} />}
      {state.status === "success" && state.data === null && (
        <p className="muted">The typosquat scanner isn't available in this build.</p>
      )}
      {state.status === "success" && state.data && !state.data.flagged && (
        <p className="muted">
          No typosquat signals for <code>{state.data.candidate}</code>.
        </p>
      )}
      {state.status === "success" && state.data?.flagged && (
        <>
          <p>
            <code>{state.data.candidate}</code> looks like a typosquat{" "}
            <span className="scanner-confidence">confidence {state.data.confidence}/100</span>
          </p>
          <ul className="scanner-signals">
            {state.data.signals.map((signal) => (
              <li key={signal.kind}>
                <strong>{SIGNAL_LABEL[signal.kind] ?? signal.kind}</strong>
                <span className="muted">{signal.detail}</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
