import { describe, expect, it } from "vitest";
import {
  auxAmbiguity,
  auxRefLabel,
  auxRefMatches,
  instancePosition,
  isRecent,
  providersServing,
  referencesTo,
  totalModels,
  type ChatModelRefs,
  type ProviderIndex,
} from "./modelUsage";

/** Two OpenCode Go plans serving the same catalogue — the case this exists for. */
const providers: ProviderIndex = {
  "opencode-go": {
    name: "OpenCode Go",
    models: { "qwen3:8b": {}, "glm-4.7": {} },
  },
  "opencode-go-2": {
    name: "OpenCode Go 2",
    models: { "qwen3:8b": {} },
  },
  ollama: {
    name: "Ollama",
    models: { "qwen3:8b": {}, "llama3": {} },
  },
};

function chat(overrides: Partial<ChatModelRefs> = {}): ChatModelRefs {
  return {
    providerId: null,
    modelId: null,
    lite: null,
    imageModel: null,
    embeddingModel: null,
    computerModel: null,
    recentModels: [],
    ...overrides,
  };
}

describe("providersServing", () => {
  it("lists every provider serving an id, in id order", () => {
    expect(providersServing(providers, "qwen3:8b")).toEqual([
      "ollama",
      "opencode-go",
      "opencode-go-2",
    ]);
  });

  it("is empty for an id nobody serves, and for an empty id", () => {
    expect(providersServing(providers, "nope")).toEqual([]);
    expect(providersServing(providers, "")).toEqual([]);
  });
});

describe("referencesTo", () => {
  it("reports the app default", () => {
    expect(
      referencesTo(
        chat({ providerId: "ollama", modelId: "llama3" }),
        "ollama",
        "llama3",
      ),
    ).toEqual(["App default"]);
  });

  // The whole point of the chip: unselecting a model that the lite model uses
  // must not happen invisibly.
  it("reports every distinct setting, in a stable order", () => {
    expect(
      referencesTo(
        chat({
          providerId: "ollama",
          modelId: "qwen3:8b",
          lite: { providerId: "ollama", modelId: "qwen3:8b" },
          imageModel: { providerId: "", modelId: "qwen3:8b" },
          embeddingModel: { providerId: "ollama", modelId: "qwen3:8b" },
          computerModel: { providerId: "ollama", modelId: "qwen3:8b" },
        }),
        "ollama",
        "qwen3:8b",
      ),
    ).toEqual([
      "App default",
      "Lite model",
      "Image model",
      "Embedding model",
      "Computer model",
    ]);
  });

  it("says nothing about an unrelated model", () => {
    expect(
      referencesTo(
        chat({ providerId: "ollama", modelId: "llama3" }),
        "opencode-go",
        "glm-4.7",
      ),
    ).toEqual([]);
  });

  // A provider that merely *serves* the id is not a reference: the app default
  // names one provider, not every provider that could have served it.
  it("does not treat a same-id provider as a reference", () => {
    expect(
      referencesTo(
        chat({ providerId: "opencode-go", modelId: "qwen3:8b" }),
        "opencode-go-2",
        "qwen3:8b",
      ),
    ).toEqual([]);
  });
});

describe("auxRefMatches", () => {
  it("matches an unqualified ref in any provider", () => {
    // What a legacy bare string means: "this id, wherever it lives".
    expect(auxRefMatches({ providerId: "", modelId: "qwen3:8b" }, "ollama", "qwen3:8b")).toBe(
      true,
    );
    expect(
      auxRefMatches({ providerId: "", modelId: "qwen3:8b" }, "opencode-go-2", "qwen3:8b"),
    ).toBe(true);
  });

  it("matches a qualified ref only in the named provider", () => {
    const ref = { providerId: "opencode-go-2", modelId: "qwen3:8b" };
    expect(auxRefMatches(ref, "opencode-go-2", "qwen3:8b")).toBe(true);
    expect(auxRefMatches(ref, "opencode-go", "qwen3:8b")).toBe(false);
  });

  it("ignores a different model, a null ref, and whitespace", () => {
    expect(auxRefMatches({ providerId: "", modelId: "llama3" }, "ollama", "qwen3:8b")).toBe(
      false,
    );
    expect(auxRefMatches(null, "ollama", "qwen3:8b")).toBe(false);
    expect(auxRefMatches({ providerId: "  ", modelId: "qwen3:8b" }, "ollama", "qwen3:8b")).toBe(
      true,
    );
  });
});

describe("auxAmbiguity", () => {
  it("warns when several providers serve an unqualified id", () => {
    const warning = auxAmbiguity(
      providers,
      { providerId: "", modelId: "qwen3:8b" },
      "Embedding model",
    );
    expect(warning).toContain("Embedding model");
    expect(warning).toContain("3 providers");
    expect(warning).toContain("Ollama");
  });

  it("stays quiet when the ref is qualified or unambiguous", () => {
    expect(
      auxAmbiguity(
        providers,
        { providerId: "ollama", modelId: "qwen3:8b" },
        "Embedding model",
      ),
    ).toBeNull();
    expect(
      auxAmbiguity(
        providers,
        { providerId: "", modelId: "glm-4.7" },
        "Embedding model",
      ),
    ).toBeNull();
    expect(auxAmbiguity(providers, null, "Embedding model")).toBeNull();
  });
});

describe("isRecent", () => {
  it("matches on both ids", () => {
    const recents = [{ providerId: "ollama", modelId: "llama3" }];
    expect(isRecent(chat({ recentModels: recents }), "ollama", "llama3")).toBe(true);
    expect(isRecent(chat({ recentModels: recents }), "opencode-go", "llama3")).toBe(false);
  });
});

describe("instancePosition", () => {
  const presetOf = (id: string) => (id.startsWith("opencode-go") ? "opencode-go" : null);

  it("counts and orders instances of one preset", () => {
    expect(instancePosition(providers, presetOf, "opencode-go")).toEqual({
      count: 2,
      index: 1,
    });
    expect(instancePosition(providers, presetOf, "opencode-go-2")).toEqual({
      count: 2,
      index: 2,
    });
  });

  it("reports a lone provider as the only one", () => {
    expect(instancePosition(providers, presetOf, "ollama")).toEqual({
      count: 1,
      index: 1,
    });
  });
});

describe("auxRefLabel", () => {
  it("qualifies a pinned provider and leaves a bare ref alone", () => {
    expect(auxRefLabel(providers, { providerId: "ollama", modelId: "qwen3:8b" })).toBe(
      "Ollama · qwen3:8b",
    );
    expect(auxRefLabel(providers, { providerId: "", modelId: "qwen3:8b" })).toBe(
      "qwen3:8b",
    );
    expect(auxRefLabel(providers, null)).toBe("");
  });
});

describe("totalModels", () => {
  it("counts a provider's catalogue and treats an unknown one as empty", () => {
    expect(totalModels(providers, "opencode-go")).toBe(2);
    expect(totalModels(providers, "nope")).toBe(0);
  });
});
