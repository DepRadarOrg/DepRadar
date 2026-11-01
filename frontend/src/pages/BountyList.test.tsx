import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BountyList } from "./BountyList";
import type { Bounty } from "../lib/types";
import * as contract from "../lib/contract";

vi.mock("../lib/contract", () => ({
  listBounties: vi.fn(),
}));
vi.mock("../lib/typosquat", () => ({
  bountyPackage: (b: Pick<Bounty, "issueRef" | "package">) => b.package ?? null,
}));
// The Radar canvas isn't the subject here.
vi.mock("../components/Radar", () => ({ Radar: () => null }));

const bounty = (id: number, issueRef: string, pkg?: string): Bounty => ({
  id,
  creator: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
  issueRef,
  package: pkg,
  amount: 5_000_000n,
  token: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
  status: "Open",
  createdAt: 1_791_546_147,
  deadline: 1_794_138_138,
  claimCount: 0,
  pendingClaims: 0,
});

const bounties = [
  bounty(1, "expresss typosquat of express", "expresss"),
  bounty(2, "requetss typosquat of requests", "requetss"),
  bounty(3, "lodash prototype pollution", "lodash"),
];

function renderList() {
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <BountyList />
    </MemoryRouter>,
  );
}

const searchBox = () => screen.getByRole("searchbox", { name: /search bounties/i });

/** Types into the search box and lets the 150ms debounce elapse inside act(). */
function search(text: string) {
  act(() => {
    fireEvent.change(searchBox(), { target: { value: text } });
    vi.advanceTimersByTime(150);
  });
}

beforeEach(() => {
  // Fake timers with advancing real time: the debounce is testable via
  // advanceTimersByTime, while async contract resolves still settle.
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.mocked(contract.listBounties).mockResolvedValue(bounties);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("BountyList search", () => {
  it("shows every bounty when the query is empty", async () => {
    renderList();
    expect(await screen.findByText("expresss typosquat of express")).toBeInTheDocument();
    expect(screen.getByText("requetss typosquat of requests")).toBeInTheDocument();
    expect(screen.getByText("lodash prototype pollution")).toBeInTheDocument();
  });

  it("debounces input before filtering", async () => {
    renderList();
    await screen.findByText("expresss typosquat of express");
    act(() => {
      fireEvent.change(searchBox(), { target: { value: "lodash" } });
    });
    // Not yet applied before the debounce elapses.
    expect(screen.getByText("expresss typosquat of express")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(150);
    });
    expect(screen.getByText("lodash prototype pollution")).toBeInTheDocument();
    expect(screen.queryByText("expresss typosquat of express")).not.toBeInTheDocument();
  });

  it("filters case-insensitively by issue reference text", async () => {
    renderList();
    await screen.findByText("expresss typosquat of express");
    search("LODASH");
    expect(screen.getByText("lodash prototype pollution")).toBeInTheDocument();
    expect(screen.queryByText("expresss typosquat of express")).not.toBeInTheDocument();
  });

  it("filters by package name", async () => {
    renderList();
    await screen.findByText("expresss typosquat of express");
    search("requets");
    expect(screen.getByText("requetss typosquat of requests")).toBeInTheDocument();
    expect(screen.queryByText("lodash prototype pollution")).not.toBeInTheDocument();
  });

  it("shows a no-matches message", async () => {
    renderList();
    await screen.findByText("expresss typosquat of express");
    search("zzzznope");
    expect(screen.getByText(/no bounties match/i)).toBeInTheDocument();
  });

  it("clears the search with the clear button", async () => {
    renderList();
    await screen.findByText("expresss typosquat of express");
    search("lodash");
    expect(screen.queryByText("expresss typosquat of express")).not.toBeInTheDocument();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: /clear search/i }));
    });
    act(() => {
      vi.advanceTimersByTime(150);
    });
    expect(screen.getByText("expresss typosquat of express")).toBeInTheDocument();
    expect(screen.queryByText(/no bounties match/i)).not.toBeInTheDocument();
  });
});
