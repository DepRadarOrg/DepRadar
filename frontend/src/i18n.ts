import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const resources = {
  en: {
    translation: {
      nav: {
        about: "About",
        leaderboard: "Leaderboard",
        github: "GitHub",
      },
      hero: {
        eyebrow: "Live on Stellar Testnet",
        titlePrefix: "Autonomous",
        titleAccent: "Dependency Defense",
        lede: "Soroban bounties for open source package security. Claim bounties with proof of vulnerability.",
      },
      board: {
        title: "Bounties",
        filterAria: "Filter bounties by status",
        searchLabel: "Search bounties by package name",
        searchPlaceholder: "Search by package…",
        searchClear: "Clear search",
        noMatches: "No matches",
        sortLabel: "Sort bounties",
      },
      sort: {
        newest: "Newest",
        reward: "Highest reward",
        closing: "Closing soon",
        claims: "Most claims",
      },
      filters: {
        all: "All",
        open: "Open",
        paid: "Paid",
        refunded: "Refunded",
      },
      leaderboard: {
        title: "Researcher Leaderboard",
        subtitle: "Top security researchers by verified payouts and accepted claims.",
        rank: "Rank",
        researcher: "Researcher",
        bountiesSolved: "Bounties Won",
        totalEarned: "Total Earned",
        empty: "No bounties have been paid out to researchers yet.",
      },
      bounty: {
        id: "#{{id}}",
        claim: "{{count}} claim",
        claim_plural: "{{count}} claims",
        closesIn: "closes {{when}}",
        closed: "closed",
        fundedBy: "Funded by ",
        reward: "Reward",
        token: "Token",
        timeline: "Timeline",
        status: "Status",
        details: "Details",
        findingRef: "Target Finding",
        native: "Native XLM",
        sentTo: "Sent to",
        claimsClose: "Claims close",
        claimsSummary: "Claims ({{count}})",
        claimCount: "{{count}} claim",
        claimCount_plural: "{{count}} claims",
      },
      empty: {
        title: "No bounties yet",
        body: "Get started by creating a bounty or learning how the protocol works.",
        noneForFilter: "No {{filter}} bounties found",
        links: {
          tutorial: "Read the tutorial",
          contributing: "Contributing guide",
          issues: "Browse issues",
        },
      },
      about: {
        title: "About DepRadar",
        intro: "DepRadar is a transparent claims and refunds protocol running on Stellar Soroban.",
        contract: "Contract",
        contractId: "Contract ID",
        network: "Network",
        unknown: "Unknown",
        administration: "Administration",
        loadingAdmin: "Loading admin…",
        failedAdmin: "Failed to load admin: {{message}}",
        currentAdmin: "Current Admin",
        pendingAdmin: "Pending Admin",
        notSet: "Not set",
        loading: "Loading…",
        unavailable: "Unavailable",
        none: "None",
        rules: "Rules",
        claims: "Claims",
        claimsRule1: "Claims are reviewed by the admin.",
        claimsRule2: "Accepted claims are paid out instantly from the contract.",
        claimsRule3: "Rejected claims receive no payout.",
        refunds: "Refunds",
        refundsRule1: "The creator can cancel after the deadline if no claims are pending.",
        refundsRule2: "The creator can also cancel if the review window has expired.",
      },
      footer: {
        readOnly: "Read-only testnet client. Contracts execute on <0>Stellar</0>.",
        built: "Open-source under MIT license",
      },
      app: {
        notFound: "Page not found",
        backToBoard: "← Back to all bounties",
        bountyMissing: "Bounty #{{id}} was not found on-chain.",
      },
    },
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: "en",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
});

export default i18n;
