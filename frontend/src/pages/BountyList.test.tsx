import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { BountyList } from "./BountyList";
import type { Bounty } from "../lib/types";

vi.mock("../components/Radar", () => ({ Radar: () => null }));

const bounty = (id: number, issueRef: string): Bounty => ({
  id,
  creator: "GCREATOR",
  issueRef,
  amount: 50_000_000n,
  token: "NATIVE",
  status: "Open",
  createdAt: 1_700_000_000,
  deadline: 1_900_000_000,
  claimCount: 0,
  pendingClaims: 0,
});

const fixtures = [bounty(1, "npm:lodash typosquat"), bounty(2, "npm:express typosquat")];

vi.mock("../lib/contract", () => ({
  listBounties: vi.fn(async () => fixtures),
}));

function renderList() {
  return render(
    <MemoryRouter>
      <BountyList />
    </MemoryRouter>,
  );
}

describe("BountyList search", () => {
  it("keeps only bounties whose issueRef matches the query", async () => {
    renderList();
    await screen.findAllByText(/express/);

    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "LODASH" } });

    await waitFor(() => expect(screen.queryByText(/express/)).toBeNull());
    expect(screen.getAllByText(/lodash/).length).toBeGreaterThan(0);
  });

  it("shows 'No matches' when nothing matches", async () => {
    renderList();
    await screen.findAllByText(/express/);

    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "zzz-no-such-package" } });

    expect(await screen.findByText("No matches")).toBeInTheDocument();
  });

  it("clears the query and restores the full list", async () => {
    renderList();
    await screen.findAllByText(/express/);

    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "zzz-no-such-package" } });
    await screen.findByText("No matches");

    fireEvent.click(screen.getAllByRole("button", { name: "Clear search" })[0]);

    expect(screen.getByRole("searchbox")).toHaveValue("");
    await waitFor(() => expect(screen.getAllByText(/express/).length).toBeGreaterThan(0));
    expect(screen.queryByText("No matches")).toBeNull();
  });
});
