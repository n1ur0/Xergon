import { describe, it, expect } from "vitest";
import { cn } from "@/lib/utils";

describe("cn (classname merge utility)", () => {
  it("merges multiple class strings", () => {
    expect(cn("foo", "bar")).toBe("foo bar");
  });

  it("handles conditional classes (falsy values)", () => {
    expect(cn("base", false && "hidden", null, undefined, "visible")).toBe(
      "base visible"
    );
  });

  it("deduplicates conflicting Tailwind classes", () => {
    // tailwind-merge should resolve the later class
    expect(cn("px-4", "px-6")).toBe("px-6");
  });

  it("handles empty input", () => {
    expect(cn()).toBe("");
  });

  it("handles arrays and objects", () => {
    expect(cn(["a", "b"], { "c": true, "d": false })).toBe("a b c");
  });
});
