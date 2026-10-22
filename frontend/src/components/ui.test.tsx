import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorState, StatusBadge } from "./ui";

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

  it("explains common RPC failures in plain language", () => {
    const { container } = render(<ErrorState error={new TypeError("Failed to fetch")} />);
    const alert = within(container).getByRole("alert");
    expect(alert).toHaveTextContent("Can't reach the Stellar network.");
    expect(alert).not.toHaveTextContent("Failed to fetch");
  });
});
