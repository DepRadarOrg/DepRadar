import { render, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Detection } from "../lib/typosquat";
import { ScannerDetection } from "./ScannerDetection";

const detectPackage = vi.hoisted(() => vi.fn<(name: string) => Promise<Detection | null>>());
vi.mock("../lib/typosquat", () => ({ detectPackage }));

describe("ScannerDetection", () => {
  it("lists the signals that fire for a flagged package", async () => {
    detectPackage.mockResolvedValue({
      candidate: "expresss",
      flagged: true,
      confidence: 40,
      signals: [{ kind: "distance", detail: 'edit distance 1 from popular package "express" (rank 111)' }],
    });
    const { container } = render(<ScannerDetection name="expresss" />);
    const view = within(container);

    await waitFor(() => expect(view.getByText("Near-miss name")).toBeInTheDocument());
    expect(detectPackage).toHaveBeenCalledWith("expresss");
    expect(container).toHaveTextContent("confidence 40/100");
    expect(container).toHaveTextContent('edit distance 1 from popular package "express"');
  });

  it("says so when no signal fires", async () => {
    detectPackage.mockResolvedValue({ candidate: "express", flagged: false, confidence: 0, signals: [] });
    const { container } = render(<ScannerDetection name="express" />);
    await waitFor(() => expect(container).toHaveTextContent("No typosquat signals for express."));
  });

  it("degrades gracefully when the wasm module isn't built", async () => {
    detectPackage.mockResolvedValue(null);
    const { container } = render(<ScannerDetection name="expresss" />);
    await waitFor(() => expect(container).toHaveTextContent("scanner isn't available"));
  });
});
