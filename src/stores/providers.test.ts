import { describe, expect, it } from "vitest";
import type { ModelEntry } from "../types";
import { currentModel } from "./providers";

function entry(providerId: string, modelId: string): ModelEntry {
  return {
    providerId,
    providerName: providerId,
    kind: "openai-compatible",
    enabled: true,
    keyReady: true,
    keyRequired: true,
    modelId,
    spec: {
      name: null,
      context: null,
      output: null,
      inputModalities: ["text"],
      reasoning: null,
      favorite: false,
    },
  };
}

const models = [entry("a", "model-a"), entry("b", "model-b")];

describe("currentModel", () => {
  it("prefers the chat's own model", () => {
    expect(
      currentModel(
        models,
        { providerId: "a", modelId: "model-a" },
        { providerId: "b", modelId: "model-b" },
      )?.modelId,
    ).toBe("model-a");
  });

  // The bug this guards: with no chat open yet, picking a model only sets the
  // app-wide default, and the chip must reflect it.
  it("falls back to the app default when there is no session", () => {
    expect(
      currentModel(models, undefined, { providerId: "b", modelId: "model-b" })
        ?.modelId,
    ).toBe("model-b");
  });

  it("falls back when the session has no model yet", () => {
    expect(
      currentModel(
        models,
        { providerId: null, modelId: null },
        { providerId: "a", modelId: "model-a" },
      )?.modelId,
    ).toBe("model-a");
  });

  it("returns nothing when neither side has a usable model", () => {
    expect(currentModel(models, undefined, { providerId: null, modelId: null })).toBeUndefined();
    expect(
      currentModel(models, undefined, { providerId: "a", modelId: "gone" }),
    ).toBeUndefined();
    expect(currentModel([], undefined, { providerId: "a", modelId: "model-a" })).toBeUndefined();
  });
});
