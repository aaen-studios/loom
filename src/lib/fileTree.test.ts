import { describe, expect, it } from "vitest";
import {
  ancestorsOf,
  diffPath,
  isDiffPath,
  isInside,
  languageFor,
  nameOf,
  parseDiffPath,
  parentOf,
  sortEntries,
  suggestedNameFor,
  tabLabel,
  unusedName,
} from "./fileTreeOperations";
import type { TreeEntry } from "./ipc";

function entry(name: string, path: string, isDir = false): TreeEntry {
  return { name, path, isDir };
}

describe("file tree paths", () => {
  it("finds a parent, and the root for a top-level entry", () => {
    expect(parentOf("src/components/Composer.tsx")).toBe("src/components");
    expect(parentOf("README.md")).toBe("");
    expect(parentOf("a/b")).toBe("a");
  });

  it("finds a name", () => {
    expect(nameOf("src/components/Composer.tsx")).toBe("Composer.tsx");
    expect(nameOf("README.md")).toBe("README.md");
    expect(nameOf("a/")).toBe("");
  });

  it("lists the folders on the way to a path, outermost first", () => {
    expect(ancestorsOf("src/dock/registry.tsx")).toEqual(["src", "src/dock"]);
    // A top-level file has no folders to expand.
    expect(ancestorsOf("README.md")).toEqual([]);
    // The path itself is not one of them: a file is not a folder.
    expect(ancestorsOf("a")).toEqual([]);
  });

  it("tells whether a path is inside a folder", () => {
    expect(isInside("", "anything/at/all.ts")).toBe(true);
    expect(isInside("src", "src/a.ts")).toBe(true);
    expect(isInside("src", "src")).toBe(true);
    expect(isInside("src", "srcs/a.ts")).toBe(false);
    expect(isInside("src", "test/a.ts")).toBe(false);
  });
});

describe("file tree ordering", () => {
  it("puts folders first, then sorts each group case-insensitively", () => {
    const sorted = sortEntries([
      entry("zebra.txt", "zebra.txt"),
      entry("src", "src", true),
      entry("Apple.md", "Apple.md"),
      entry("dock", "dock", true),
    ]);
    expect(sorted.map((e) => e.name)).toEqual(["dock", "src", "Apple.md", "zebra.txt"]);
  });

  it("does not mutate the input", () => {
    const input = [entry("b", "b"), entry("a", "a")];
    sortEntries(input);
    expect(input.map((e) => e.name)).toEqual(["b", "a"]);
  });
});

describe("new entry names", () => {
  it("uses the base when it is free", () => {
    expect(unusedName([])).toBe("untitled");
  });

  it("counts up from two when it is taken", () => {
    expect(unusedName([entry("untitled", "untitled")])).toBe("untitled-2");
    expect(
      unusedName([entry("untitled", "untitled"), entry("untitled-2", "untitled-2")]),
    ).toBe("untitled-3");
  });

  it("ignores case, so a name cannot collide on Windows", () => {
    // The filesystem is case-insensitive here, so a check that was
    // case-sensitive would hand back a name that then fails to create.
    expect(unusedName([entry("Untitled", "Untitled")])).toBe("untitled-2");
  });

  it("names a new folder differently from a new file", () => {
    expect(suggestedNameFor([], true)).toBe("new-folder");
    expect(suggestedNameFor([], false)).toBe("untitled");
  });
});

describe("languages", () => {
  it("maps the extensions this workspace actually contains", () => {
    expect(languageFor("src/App.tsx")).toBe("typescript");
    expect(languageFor("src-tauri/src/lib.rs")).toBe("rust");
    expect(languageFor("Cargo.toml")).toBe("ini");
    expect(languageFor("docs/spec.md")).toBe("markdown");
    expect(languageFor("styles.css")).toBe("css");
  });

  it("recognises a few extensionless filenames", () => {
    expect(languageFor("Dockerfile")).toBe("dockerfile");
    expect(languageFor(".editorconfig")).toBe("ini");
    expect(languageFor("Cargo.lock")).toBe("toml");
  });

  it("falls back to plaintext rather than guessing", () => {
    // A wrong grammar produces confidently wrong colours, which is worse than
    // none at all.
    expect(languageFor("data.weird")).toBe("plaintext");
    expect(languageFor("noextension")).toBe("plaintext");
    expect(languageFor("trailing.")).toBe("plaintext");
  });
});

describe("diff tabs", () => {
  it("round-trips a file and its side", () => {
    const staged = diffPath("src/App.tsx", true);
    const worktree = diffPath("src/App.tsx", false);

    expect(parseDiffPath(staged)).toEqual({ file: "src/App.tsx", staged: true });
    expect(parseDiffPath(worktree)).toEqual({ file: "src/App.tsx", staged: false });
  });

  it("keeps the two sides distinct", () => {
    // Sharing a tab between them would show the wrong diff the moment both are
    // open, which is exactly when it matters.
    expect(diffPath("a.ts", true)).not.toBe(diffPath("a.ts", false));
  });

  it("never mistakes a real path for a diff", () => {
    expect(isDiffPath("src/App.tsx")).toBe(false);
    expect(parseDiffPath("src/App.tsx")).toBeNull();
  });

  it("labels a diff tab so it cannot be confused with the file", () => {
    expect(tabLabel(diffPath("src/App.tsx", false))).toBe("App.tsx (diff)");
    expect(tabLabel("src/App.tsx")).toBe("App.tsx");
  });

  it("survives a diff path whose shape is wrong", () => {
    // A stored layout from a future build, or a hand-edited value.
    expect(parseDiffPath("diff:nonsense")).toBeNull();
    expect(tabLabel("diff:nonsense")).toBe("diff");
  });
});
