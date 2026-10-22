/** A plain-language explanation of a failed contract read. */
export interface FriendlyError {
  title: string;
  /** What to tell the user; the raw message when the failure isn't recognised. */
  detail: string;
}

const RULES: { match: RegExp; error: FriendlyError }[] = [
  {
    match: /failed to fetch|network ?error|networkerror|load failed|err_network|econnrefused|enotfound|err_internet_disconnected/i,
    error: {
      title: "Can't reach the Stellar network.",
      detail: "Check your internet connection, then try again. The testnet RPC may also be temporarily down.",
    },
  },
  {
    match: /\b429\b|too many requests|rate.?limit/i,
    error: {
      title: "The Stellar RPC is rate limiting requests.",
      detail: "Too many reads in a short time. Wait a few seconds, then try again.",
    },
  },
  {
    match: /contract not found|missingvalue|non-?existing value for contract instance|wasm.*does not exist/i,
    error: {
      title: "The bounty contract wasn't found.",
      detail: "The configured contract ID doesn't exist on this network. It may have been redeployed — see DEPLOYMENT.md.",
    },
  },
];

/** Maps common RPC failures to messages a user can act on. */
export function friendlyError(error: Error): FriendlyError {
  const rule = RULES.find((r) => r.match.test(error.message));
  return rule ? rule.error : { title: "Couldn't read the contract.", detail: error.message };
}
