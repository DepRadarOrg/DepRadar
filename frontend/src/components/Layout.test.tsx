import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Layout, useTheme } from "./Layout";

function renderLayout() {
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <Layout />
    </MemoryRouter>,
  );
}

const toggle = () => screen.getByRole("button", { name: /theme/i });

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  vi.restoreAllMocks();
});

describe("Layout theme toggle", () => {
  it("defaults to dark (no data-theme attribute)", () => {
    renderLayout();
    expect(document.documentElement).not.toHaveAttribute("data-theme");
  });

  it("switches to light and sets the data-theme attribute", () => {
    renderLayout();
    fireEvent.click(toggle());
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    expect(localStorage.getItem("depradar-theme")).toBe("light");
  });

  it("persists the choice and restores it on the next mount", () => {
    renderLayout();
    fireEvent.click(toggle());
    cleanup();
    renderLayout();
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
  });

  it("toggles back to dark and clears the stored choice", () => {
    renderLayout();
    fireEvent.click(toggle());
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
    fireEvent.click(toggle());
    expect(document.documentElement).not.toHaveAttribute("data-theme");
    expect(localStorage.getItem("depradar-theme")).toBeNull();
  });

  it("survives a throwing localStorage (private mode)", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("denied");
    });
    renderLayout();
    fireEvent.click(toggle());
    expect(document.documentElement).toHaveAttribute("data-theme", "light");
  });
});

describe("useTheme", () => {
  it("reads a stored theme on mount", () => {
    localStorage.setItem("depradar-theme", "light");
    function Probe() {
      const [theme] = useTheme();
      return <span data-testid="t">{String(theme)}</span>;
    }
    render(<Probe />);
    expect(screen.getByTestId("t")).toHaveTextContent("light");
  });
});
