import { describe, expect, it } from "vitest";
import type { Session } from "../types";
import {
  findMention,
  insertSuggestion,
  matchChats,
  matchFiles,
  resolveChatMentions,
  shortId,
} from "./mentions";

function session(id: string, title: string, updatedAt = 0): Session {
  return {
    id,
    title,
    providerId: null,
    modelId: null,
    variant: null,
    personaId: null,
    systemPrompt: null,
    workdir: null,
    permissionMode: null,
    agentMode: null,
    computerAccess: false,
    browserAccess: false,
    position: null,
    createdAt: 0,
    updatedAt,
  };
}

describe("findMention", () => {
  it("finds a trigger at the start of the line", () => {
    expect(findMention("#", 1)).toEqual({
      trigger: "#",
      query: "",
      start: 0,
      end: 1,
    });
  });

  it("finds a trigger after whitespace and reports the query", () => {
    const span = findMention("ask #upd", 8);
    expect(span).toMatchObject({ trigger: "#", query: "upd", start: 4, end: 8 });
  });

  it("finds a file trigger anywhere in a line", () => {
    const span = findMention("look at @src/ma", 15);
    expect(span).toMatchObject({ trigger: "@", query: "src/ma", start: 8 });
  });

  it("ignores a trigger glued to a word", () => {
    // The whole reason for the rule: this is prose, not a mention.
    expect(findMention("mail me at foo@bar", 17)).toBeNull();
    expect(findMention("note#Fix this", 12)).toBeNull();
  });

  it("does offer a mention for a reference like #42", () => {
    // Deliberately not special-cased. The trigger did start a word, and the
    // thing that keeps prose quiet is that the popup only opens when a chat
    // actually matches — which is decided in the composer, not here.
    expect(findMention("issue #42", 9)).toMatchObject({ trigger: "#", query: "42" });
  });

  it("stops at whitespace", () => {
    expect(findMention("#done reading", 13)).toBeNull();
  });

  it("accepts a mention that starts a new line", () => {
    expect(findMention("first\n#chat", 11)).toMatchObject({
      trigger: "#",
      query: "chat",
      start: 6,
    });
  });

  it("will not carry a mention across a newline", () => {
    expect(findMention("#a\nb", 4)).toBeNull();
  });

  it("does not match across the other trigger", () => {
    expect(findMention("@#files", 7)).toBeNull();
  });

  it("returns null when there is no trigger at all", () => {
    expect(findMention("just prose", 10)).toBeNull();
    expect(findMention("", 0)).toBeNull();
  });

  it("refuses a caret outside the value", () => {
    expect(findMention("abcd", 99)).toBeNull();
    expect(findMention("abcd", -1)).toBeNull();
  });
});

describe("insertSuggestion", () => {
  it("replaces the typed span and leaves a trailing space", () => {
    const value = "ask #upd";
    const span = findMention(value, value.length)!;
    const result = insertSuggestion(value, span, "Updater fix");
    expect(result.value).toBe("ask #Updater fix ");
    expect(result.caret).toBe(result.value.length);
  });

  it("keeps the text after the caret", () => {
    const value = "ask @src then stop";
    // The caret sits at the end of "@src", not past the following space — past
    // it there is no mention to replace.
    const span = findMention(value, 8)!;
    const result = insertSuggestion(value, span, "src/main.rs");
    expect(result.value).toBe("ask @src/main.rs  then stop");
    expect(result.value.slice(result.caret)).toBe(" then stop");
  });

  it("clears the query when the trigger alone was typed", () => {
    const span = findMention("#", 1)!;
    expect(insertSuggestion("#", span, "Rust help").value).toBe("#Rust help ");
  });
});

describe("matchChats", () => {
  const sessions = [
    session("a", "Updater fix", 10),
    session("b", "Rust help", 30),
    session("c", "", 20),
  ];

  it("matches on a case-insensitive substring, newest first", () => {
    expect(matchChats("", sessions).map((entry) => entry.id)).toEqual([
      "b",
      "c",
      "a",
    ]);
  });

  it("narrows on the query", () => {
    expect(matchChats("rust", sessions).map((entry) => entry.id)).toEqual(["b"]);
  });

  it("treats an untitled chat as New chat", () => {
    expect(matchChats("new chat", sessions).map((entry) => entry.id)).toEqual(["c"]);
  });

  it("caps the list", () => {
    const many = Array.from({ length: 30 }, (_, index) =>
      session(`s${index}`, `chat ${index}`, index),
    );
    expect(matchChats("", many, 5)).toHaveLength(5);
  });
});

describe("matchFiles", () => {
  const files = [
    "src/main.rs",
    "src/lib/notes.md",
    "notes.md",
    "README.md",
  ];

  it("prefers a path that ends with the query", () => {
    // "@main" means src/main.rs, not src/maintenance/…
    expect(matchFiles("main.rs", files)[0]).toBe("src/main.rs");
  });

  it("prefers shallower files on a tie", () => {
    expect(matchFiles("notes.md", files)).toEqual([
      "notes.md",
      "src/lib/notes.md",
    ]);
  });

  it("returns everything for an empty query", () => {
    expect(matchFiles("", files)).toHaveLength(4);
  });

  it("caps the list", () => {
    const many = Array.from({ length: 40 }, (_, index) => `file${index}.ts`);
    expect(matchFiles("", many, 6)).toHaveLength(6);
  });
});

describe("resolveChatMentions", () => {
  const sessions = [session("1a2b3c4d-5e6f-7a8b", "Updater fix")];

  it("appends the chat id to a mention", () => {
    expect(resolveChatMentions("see #Updater fix for that", sessions)).toBe(
      "see #Updater fix (id: 1a2b3c4d) for that",
    );
  });

  it("leaves prose alone", () => {
    expect(resolveChatMentions("issue #42 is still open", sessions)).toBe(
      "issue #42 is still open",
    );
  });

  it("does not resolve twice", () => {
    const once = resolveChatMentions("#Updater fix", sessions);
    expect(resolveChatMentions(once, sessions)).toBe(once);
  });

  it("ignores an untitled chat", () => {
    expect(resolveChatMentions("#", [session("abcdefgh", "")])).toBe("#");
  });

  it("prefers the longest matching title", () => {
    const both = [
      session("aaaaaaaa", "Fix"),
      session("bbbbbbbb", "Fix the updater"),
    ];
    expect(resolveChatMentions("#Fix the updater", both)).toBe(
      "#Fix the updater (id: bbbbbbbb)",
    );
  });

  it("needs the mention to start a word", () => {
    expect(resolveChatMentions("note#Updater fix", sessions)).toBe(
      "note#Updater fix",
    );
  });
});

describe("shortId", () => {
  it("is eight hex characters, dashes removed", () => {
    expect(shortId("1a2b3c4d-5e6f")).toBe("1a2b3c4d");
    expect(shortId("abc")).toBe("abc");
  });
});
