import { describe, it, expect } from "vitest";

describe("toDisplayModel type narrowing", () => {
  // This test verifies the type narrowing logic used in app/models/page.tsx
  // We test the core concept: ChainModelInfo uses snake_case, ModelInfo uses camelCase

  it("correctly identifies ChainModelInfo by checking for snake_case key", () => {
    const chainModel = {
      id: "model-1",
      name: "Test Model",
      provider: "TestProvider",
      tier: "paid",
      price_per_input_token_nanoerg: 100,
      price_per_output_token_nanoerg: 200,
      effective_price_nanoerg: 150,
      provider_count: 3,
      available: true,
      context_window: 128000,
      free_tier: false,
    };

    const modelInfo = {
      id: "model-2",
      name: "Test Model 2",
      provider: "TestProvider",
      tier: "paid",
      pricePerInputTokenNanoerg: 100,
      pricePerOutputTokenNanoerg: 200,
      effectivePriceNanoerg: 150,
      providerCount: 3,
      available: true,
      contextWindow: 128000,
      freeTier: false,
    };

    // Simulate the narrowing logic from toDisplayModel
    function narrowModel(m: typeof chainModel | typeof modelInfo) {
      const isChain = "price_per_input_token_nanoerg" in m;
      if (isChain) {
        // Accessing snake_case properties should be safe
        expect(m.price_per_input_token_nanoerg).toBe(100);
        expect(m.context_window).toBe(128000);
        expect(m.free_tier).toBe(false);
      } else {
        // Accessing camelCase properties should be safe
        expect(m.pricePerInputTokenNanoerg).toBe(100);
        expect(m.contextWindow).toBe(128000);
        expect(m.freeTier).toBe(false);
      }
    }

    narrowModel(chainModel);
    narrowModel(modelInfo);
  });
});
