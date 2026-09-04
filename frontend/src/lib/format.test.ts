import { describe, expect, it } from "vitest";

import { formatCash, formatPrice, formatQty } from "./format";

describe("formatPrice", () => {
  it("renders cents with a suffix", () => {
    expect(formatPrice(63)).toBe("63c");
    expect(formatPrice(8)).toBe("8c");
  });
  it("renders missing prices as a placeholder", () => {
    expect(formatPrice(null)).toBe("--");
  });
});

describe("formatCash", () => {
  it("renders cents as dollars", () => {
    expect(formatCash(995130)).toBe("$9,951.30");
    expect(formatCash(5)).toBe("$0.05");
    expect(formatCash(0)).toBe("$0.00");
  });
  it("handles negatives", () => {
    expect(formatCash(-1250)).toBe("-$12.50");
  });
});

describe("formatQty", () => {
  it("adds separators", () => {
    expect(formatQty(1000)).toBe("1,000");
    expect(formatQty(42)).toBe("42");
  });
});
