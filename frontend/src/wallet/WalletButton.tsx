import { useTranslation } from "react-i18next";
import { shortAddress } from "../lib/format";
import { expectedNetwork } from "./network";
import { useWallet } from "./WalletContext";

/** Top-bar control: install prompt, connect button, or the connected address and network. */
export function WalletButton() {
  const { t } = useTranslation();
  const wallet = useWallet();

  if (wallet.status === "unavailable") {
    return (
      <a href="https://www.freighter.app/" target="_blank" rel="noreferrer" className="btn btn-ghost">
        {t("wallet.install")}
      </a>
    );
  }

  if (wallet.status === "connected" && wallet.address) {
    return (
      <span className="wallet">
        {!wallet.networkMatches && (
          <span className="wallet-warn" role="alert">
            {t("wallet.networkMismatch", { wallet: wallet.network, expected: expectedNetwork() })}
          </span>
        )}
        <span className="mono" title={wallet.address}>
          {shortAddress(wallet.address)}
        </span>
        <span className="net">{wallet.network}</span>
        <button type="button" className="btn btn-ghost" onClick={wallet.disconnect}>
          {t("wallet.disconnect")}
        </button>
      </span>
    );
  }

  return (
    <span className="wallet">
      <button type="button" className="btn btn-ghost" onClick={wallet.connect} disabled={wallet.status === "connecting"}>
        {wallet.status === "connecting" ? t("wallet.connecting") : t("wallet.connect")}
      </button>
      {wallet.error && (
        <span className="muted" role="alert">
          {wallet.error}
        </span>
      )}
    </span>
  );
}
