import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function Boom(): never {
  throw new Error("render exploded");
}

describe("ErrorBoundary", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders its children when nothing throws", () => {
    render(
      <ErrorBoundary>
        <p>All good</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("All good")).toBeInTheDocument();
  });

  it("shows a reload button instead of a blank page when a child throws", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const reload = vi.fn();
    vi.spyOn(window, "location", "get").mockReturnValue({ ...window.location, reload });

    const { getByRole } = render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>,
    );

    expect(getByRole("alert")).toHaveTextContent("Something went wrong.");
    expect(getByRole("alert")).toHaveTextContent("render exploded");
    getByRole("button", { name: "Reload" }).click();
    expect(reload).toHaveBeenCalledOnce();
  });
});
