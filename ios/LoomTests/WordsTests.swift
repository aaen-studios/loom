import Testing
@testable import Loom

/// What a message's words become before they're drawn, and what the composer sends
/// (PLAN.md, "Mentions", "Links and sources", "Links in the composer", "Emoji"), as
/// desktop/src/model.rs has them.
@Suite struct WordsTests {
    // MARK: Links

    @Test func aBareAddressShowsAsItsPagesTitle() {
        let links = ["https://example.com/a": PageLink(title: "Example", site: "example.com")]
        #expect(titledLinks("See https://example.com/a.", links) == "See [Example](https://example.com/a).")
        #expect(titledLinks("See https://example.com/b.", links) == "See https://example.com/b.")
        #expect(titledLinks("```\nhttps://example.com/a\n```", links) == "```\nhttps://example.com/a\n```")
        #expect(titledLinks("`https://example.com/a`", links) == "`https://example.com/a`")
        #expect(titledLinks("[here](https://example.com/a)", links) == "[here](https://example.com/a)")
        // Sent as plain text: itself, linking to it.
        #expect(titledLinks("<https://example.com/a>", links) == "[https://example.com/a](https://example.com/a)")
    }

    @Test func aLongTitleIsCutAtAWord() {
        #expect(shortTitle("Watering schedule") == "Watering schedule")
        let long = String(repeating: "word ", count: 20)
        #expect(shortTitle(long) == Array(repeating: "word", count: 12).joined(separator: " ") + "…")
        #expect(shortTitle(String(repeating: "x", count: 70)) == String(repeating: "x", count: 60) + "…")
    }

    @Test func aFileMarkLinksToTheFileSentWithIt() {
        let file = FileRef(file: "abc", name: "balcony.png", mime: "image/png", size: 1)
        #expect(linkFiles("Is [balcony.png] ready?", [file]) == "Is [balcony.png](loom-file:abc) ready?")
        #expect(linkFiles("[balcony.png](https://x.com) and [balcony.png]", [file]) == "[balcony.png](https://x.com) and [balcony.png](loom-file:abc)")
        #expect(linkFiles("Is [balcony.png] ready?", [FileRef(file: "abc", name: "bal]cony.png", mime: "image/png", size: 1)]) == "Is [balcony.png] ready?")
    }

    @Test func picturesInAFanLoseTheirMarks() {
        let stacked = ["balcony 2.png", "leaves.png", "balcony.png"].map { FileRef(file: $0, name: $0, mime: "image/png", size: 1) }
        #expect(unmarkStacked("[balcony 2.png] [leaves.png] [balcony.png] And these, from this evening?", stacked) == "And these, from this evening?")
        // Among other words a mark is what was said.
        #expect(unmarkStacked("Is [balcony.png] ready to pick?", stacked) == "Is [balcony.png] ready to pick?")
        // Alone on its line, it goes with its line.
        #expect(unmarkStacked("[leaves.png]\nNice?", stacked) == "Nice?")
    }

    @Test func whereALinkGoes() {
        #expect(LinkTo("https://example.com") == .page)
        #expect(LinkTo("loom:agent/a_2") == .agent("a_2"))
        #expect(LinkTo("loom:file/a_1/notes/harvest%202026.md") == .file(agent: "a_1", path: "notes/harvest 2026.md"))
        #expect(LinkTo("loom:file/a_1") == .other)
        #expect(LinkTo("loom-file:abc") == .attached("abc"))
        #expect(LinkTo("loom:skill/review") == .skill("review"))
        #expect(LinkTo("mailto:ola@example.com") == .other)
        #expect(isPicture("Balcony.PNG") && !isPicture("notes.md"))
    }

    // MARK: Mentions

    @Test func aMentionGoesAsItsLink() {
        #expect(agentMention(id: "a_2", name: "Casper") == Mention(words: "@Casper", link: "loom:agent/a_2"))
        #expect(fileMention(agent: "a_1", path: "notes/harvest 2026.md", name: "harvest 2026.md") == Mention(words: "harvest 2026.md", link: "loom:file/a_1/notes/harvest%202026.md"))
        let casper = agentMention(id: "a_2", name: "Casper")
        #expect(withMentionLinks("Ask @Casper, not @Casper", [casper]) == "Ask [@Casper](loom:agent/a_2), not @Casper")
        #expect(withMentionLinks("Ask him", [casper]) == "Ask him")
    }

    @Test func aRepliesMentionsAreLinksToo() {
        let text = withReplyMentions("Noted in `notes/watering.md`, and @Scribe knows. Mail me@Scribe.no.", agents: [(id: "a_2", name: "Scribe")], files: [NamedFile(path: "notes/watering.md", name: "watering.md")], agent: "a_4")
        #expect(text == "Noted in [watering.md](loom:file/a_4/notes/watering.md), and [@Scribe](loom:agent/a_2) knows. Mail me@Scribe.no.")
        #expect(withReplyMentions("```\n@Scribe\n```", agents: [(id: "a_2", name: "Scribe")], files: [], agent: "a_4") == "```\n@Scribe\n```")
    }

    @Test func thePickerOpensForAnAtAfterASpace() throws {
        let q = try #require(mentionQuery("Hi @Cas"))
        #expect(q.at == 3 && q.query == "Cas")
        let start = try #require(mentionQuery("@"))
        #expect(start.at == 0 && start.query == "")
        #expect(mentionQuery("(@Ca")?.query == "Ca")
        #expect(mentionQuery("Hi @Cas ") == nil)
        #expect(mentionQuery("mail@example") == nil)
        #expect(mentionQuery("No at") == nil)
    }

    // MARK: Quotes

    @Test func aReplyQuotesTheAnswerOrThePartPicked() {
        let mine = Message(id: "m_1", role: "person", at: 0, blocks: [.text("text", "Is it ready?"), .text("text", " And the notes? ")])
        #expect(ReplyTo.to(mine, agent: "a_1", quote: nil, ask: true) == ReplyTo(message: "m_1", role: "person", text: "Is it ready?\nAnd the notes?"))
        let theirs = Message(id: "m_2", role: "agent", at: 0, blocks: [.text("text", "Checking."), .text("tool", ""), .text("text", "It passed.")])
        #expect(ReplyTo.to(theirs, agent: "a_1", quote: nil, ask: true) == ReplyTo(message: "m_2", role: "agent", agent: "a_1", text: "It passed."))
        #expect(ReplyTo.to(theirs, agent: "a_1", quote: " the run ", ask: true) == ReplyTo(message: "m_2", role: "agent", agent: "a_1", text: "the run", part: true, ask: true))
    }

    @Test func aQuoteIsOnePlainLine() {
        let r = ReplyTo(message: "m_1", role: "agent", text: "## Notes\n\n- **one**\n- two\n\n---\n\n| a | b |")
        #expect(r.line == "Notes")
        #expect(r.oneLine == "Notes one two a b")
        #expect(ReplyTo(message: "m_1", role: "agent", text: String(repeating: "a", count: 300)).oneLine == String(repeating: "a", count: 240) + "…")
    }

    // MARK: The composer

    @Test func theComposerSendsEachPieceAsTheChatShowsIt() {
        let text = composerText([
            .words("Look at https://x.com/a "), .piece(linkPiece("https://y.com")), .words(" and "),
            .piece(Piece(id: "loom:agent/a_2", text: "@Cas[p]er")), .words(" "), .piece(emojiPiece("🎉")),
            .piece(Piece(id: PieceKind.attach + "at_1", text: "notes.md")),
        ])
        #expect(text == "Look at <https://x.com/a> https://y.com and [@Casper](loom:agent/a_2) 🎉[notes.md]")
    }

    @Test func anAddressTypedGoesAsPlainText() {
        #expect(plainLinks("`https://x.com` and [a](https://y.com) and https://z.com.") == "`https://x.com` and [a](https://y.com) and <https://z.com>.")
        #expect(plainLinks("```\nhttps://x.com\n```") == "```\nhttps://x.com\n```")
    }

    @Test func aPasteIsALinkWhenItsOnlyOne() {
        #expect(pastedLink("  https://x.com/a \n") == "https://x.com/a")
        #expect(pastedLink("see https://x.com") == nil)
        #expect(pastedLink("https://x.com https://y.com") == nil)
        #expect(siteOf("https://www.Example.com/a") == "example.com")
        #expect(siteOf("not a link") == "")
    }

    @Test func aLongFileNameIsCutInTheMiddle() {
        #expect(middleCut("notes.md") == "notes.md")
        #expect(middleCut("a very long file name for the holidays.png") == "a very long file n…idays.png")
        #expect(middleCut("a very long file name for the holidays.png").count == 28)
        #expect(cutFileNames("[a very long file name for the holidays.png](loom-file:abc) and [a very long file name for the holidays.png](https://x.com)")
            == "[a very long file n…idays.png](loom-file:abc) and [a very long file name for the holidays.png](https://x.com)")
    }

    // MARK: Emoji

    @Test func anEmojiIsTheSameWhateverItsSkinTone() {
        #expect(sameEmoji(EmojiItem(["emoji": "👍🏽"]), EmojiItem(["emoji": "👍"])))
        #expect(sameEmoji(EmojiItem(["emoji": "❤️"]), EmojiItem(["emoji": "❤"])))
        #expect(!sameEmoji(EmojiItem(["emoji": "👍"]), EmojiItem(["emoji": "👋"])))
        let knot = EmojiItem(["emoji": "<:knot:e_1>", "custom": "e_1"])
        #expect(sameEmoji(knot, EmojiItem(["emoji": "<:knot:e_1>", "custom": "e_1"])))
        #expect(!sameEmoji(knot, EmojiItem(["emoji": "👍"])))
    }

    @Test func anOwnEmojisNameIsAsTheInstanceTakesIt() {
        #expect(emojiName("My Cat-2!") == "my_cat_2")
        #expect(emojiName(":knot:") == "knot")
        #expect(emojiName(String(repeating: "a", count: 40)) == String(repeating: "a", count: 32))
        #expect(CustomEmoji(id: "e_1", name: "knot", file: "f", animated: true).token == "<a:knot:e_1>")
    }
}
