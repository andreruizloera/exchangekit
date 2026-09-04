import { describe, expect, it } from "vitest";

import {
  formatCash,
  formatClock,
  formatPct,
  formatPrice,
  formatQty,
  formatSharpe,
  formatSignedCash,
  grade,
} from "./format";

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

describe("formatSignedCash", () => {
  it("prefixes a sign", () => {
    expect(formatSignedCash(6000)).toBe("+$60.00");
    expect(formatSignedCash(-1250)).toBe("-$12.50");
    expect(formatSignedCash(0)).toBe("+$0.00");
  });
});

describe("formatPct", () => {
  it("shows one decimal with a sign", () => {
    expect(formatPct(4.234)).toBe("+4.2%");
    expect(formatPct(-2)).toBe("-2.0%");
  });
});

describe("formatSharpe", () => {
  it("uses two decimals", () => {
    expect(formatSharpe(1.41421)).toBe("1.41");
    expect(formatSharpe(-0.5)).toBe("-0.50");
  });
});

describe("formatClock", () => {
  it("renders M:SS", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(9)).toBe("0:09");
    expect(formatClock(75)).toBe("1:15");
    expect(formatClock(-5)).toBe("0:00");
  });
});

describe("grade", () => {
  it("rewards a strong, steady round", () => {
    expect(grade(8, 2.0)).toBe("Sharp read. You worked the flow.");
  });
  it("credits any clear profit", () => {
    expect(grade(3, 0.5)).toBe("In the green. You beat the bots.");
    // Strong return but choppy equity misses the top grade.
    expect(grade(8, 0.4)).toBe("In the green. You beat the bots.");
  });
  it("calls a roughly flat round flat", () => {
    expect(grade(0.5, 0.1)).toBe("Flat. The spread ate your edge.");
    expect(grade(-0.5, 0)).toBe("Flat. The spread ate your edge.");
  });
  it("calls out a loss", () => {
    expect(grade(-4, -1)).toBe("Underwater. The bots picked you off.");
  });
});
