import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { explorer } from "../config";
import { AddressLink, ContractLink, ErrorState, StatusBadge } from "./ui";

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

describe("AddressLink", () => {
  const ADDRESS = "GCFFP6RU6C3JOZBIUNZN2ELJAITUZ3HWEOUUCFXIAHE6M2GHVHZB4D6A";

  it("renders no copy button by default", () => {
    render(<MemoryRouter><AddressLink address={ADDRESS} /></MemoryRouter>);
    expect(screen.queryByRole("button", { name: "Copy address" })).toBeNull();
  });

  it("copies the full address with the copy option", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<MemoryRouter><AddressLink address={ADDRESS} copy /></MemoryRouter>);
    fireEvent.click(screen.getByRole("button", { name: "Copy address" }));
    expect(writeText).toHaveBeenCalledWith(ADDRESS);
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  it("links account addresses to Stellar Expert", () => {
    render(<MemoryRouter><AddressLink address={ADDRESS} /></MemoryRouter>);
    const link = screen.getByRole("link", { name: "View on Stellar Expert" });
    expect(link).toHaveAttribute("href", explorer.account(ADDRESS));
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noreferrer");
  });

  it("links contract addresses directly to Stellar Expert", () => {
    const CONTRACT = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";
    render(<MemoryRouter><AddressLink address={CONTRACT} /></MemoryRouter>);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", explorer.contract(CONTRACT));
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noreferrer");
  });

  it("renders copy button for contract address when requested", async () => {
    const CONTRACT = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<MemoryRouter><AddressLink address={CONTRACT} copy /></MemoryRouter>);
    const button = screen.getByRole("button", { name: "Copy address" });
    expect(button).toBeInTheDocument();
    fireEvent.click(button);
    expect(writeText).toHaveBeenCalledWith(CONTRACT);
  });
});

describe("ContractLink", () => {
  const CONTRACT = "CCPRQANY467KKE4ROKCNW6TH3LY3EPVFRXIXJ5V4VCYHLJKWZPOZYQJQ";

  it("renders link to Stellar Expert contract view", () => {
    render(<ContractLink contractId={CONTRACT} />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", explorer.contract(CONTRACT));
    expect(link).toHaveAttribute("target", "_blank");
  });

  it("copies contract ID when copy is true", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<ContractLink contractId={CONTRACT} copy full />);
    fireEvent.click(screen.getByRole("button", { name: "Copy contract ID" }));
    expect(writeText).toHaveBeenCalledWith(CONTRACT);
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });
});
