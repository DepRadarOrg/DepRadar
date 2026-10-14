import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { EmptyCTA, ErrorState, StatusBadge } from "./ui";

describe("StatusBadge", () => {
  it("renders the status with a status-specific class", () => {
    render(<StatusBadge status="Rejected" />);
    const badge = screen.getByText("Rejected");
    expect(badge).toHaveClass("badge", "badge-rejected");
  });
});

describe("ErrorState", () => {
  it("shows the error and calls onRetry", () => {
    const onRetry = vi.fn();
    render(<ErrorState error={new Error("RPC timed out")} onRetry={onRetry} />);
    expect(screen.getByRole("alert")).toHaveTextContent("RPC timed out");
    screen.getByRole("button", { name: "Retry" }).click();
    expect(onRetry).toHaveBeenCalledOnce();
  });
});

describe("EmptyCTA", () => {
  it("renders title, description and links for researchers", () => {
    render(
      <EmptyCTA
        title="No open bounties right now"
        description="Earn bounties by finding typosquats."
      />
    );
    expect(screen.getByText("No open bounties right now")).toBeInTheDocument();
    expect(screen.getByText("Earn bounties by finding typosquats.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Read the Tutorial" })).toHaveAttribute(
      "href",
      "https://github.com/DepRadarOrg/DepRadar/blob/main/docs/TUTORIAL.md"
    );
    expect(screen.getByRole("link", { name: "Contributing Guide" })).toHaveAttribute(
      "href",
      "https://github.com/DepRadarOrg/DepRadar/blob/main/CONTRIBUTING.md"
    );
    expect(screen.getByRole("link", { name: "Browse GitHub Issues" })).toHaveAttribute(
      "href",
      "https://github.com/DepRadarOrg/DepRadar/issues"
    );
  });
});
