import Foundation
import Testing
@testable import Loom

/// Acta's cards on the phone (PLAN.md, "Cards in messages" and "Acta beside the chat"),
/// as desktop/src/model.rs's `card_tests` and desktop/src/model/acta.rs's tests have them:
/// pills in a message's words, cards typed and pasted in the composer, the preview sheet,
/// Make a card, a board's screen, and Follow.
@MainActor
@Suite struct ActaTests {
    func card(_ v: JSON) -> ActaCard { ActaCard(v) }

    var loo206: JSON {
        ["id": "LOO-206", "board": "aaen-studios/loom", "board_name": "loom", "title": "Show agents' usage limits in the sidebar", "status": "in_progress",
         "url": "https://acta.aaenz.no/aaen-studios/loom#LOO-206", "assignee": "Elliot", "size": "m", "labels": ["desktop"], "text": "The plan's usage limits.",
         "subtasks": 5, "subtasks_done": 3, "at": 1000]
    }

    /// A model with two Acta boards, loom's named, and the person's chat with Builder shown.
    func actaModel() -> Model {
        let m = caughtUp()
        m.select("a_1")
        _ = m.fetches()
        m.acta.boards = [("LOO", "aaen-studios/loom"), ("CAL", "aaen-studios/calcite")]
        m.acta.names["aaen-studios/loom"] = "loom"
        return m
    }

    /// Answers `cmd` with `f`, as the core does.
    func answer(_ m: Model, _ cmd: JSON?, _ f: [String: JSON]) {
        var frame = f
        frame["t"] = "result"
        frame["id"] = cmd?["id"] ?? .null
        frame["ok"] = JSON(f["error"] == nil)
        m.take(.object(frame))
    }

    // MARK: Pills

    @Test func aCardSaysHowItIsDrawn() {
        let c = card(loo206)
        #expect(c.ink == .sky)
        #expect(!c.struck)
        #expect(c.statusWords == "In progress")
        #expect(c.boardLabel == "loom")
        #expect(c.chips == ["Elliot", "M", "desktop"])
        #expect(c.subtasksLine == "Subtasks 3 of 5")
        #expect(abs(c.subtasksPart - 0.6) < 1e-6)
        #expect(c.link == "https://acta.aaenz.no/aaen-studios/loom#LOO-206")
        #expect(!c.stale(1000 + cardFreshMs - 1) && c.stale(1000 + cardFreshMs))
        #expect(c.pillTitle == "Show agents' usage…")
        let ink = { (status: String) in self.card(["id": "X-1", "status": JSON(status)]).ink }
        #expect(ink("todo") == .quiet)
        #expect(ink("review") == .gold)
        #expect(ink("done") == .ok)
        #expect(ink("canceled") == .quiet)
        let canceled = card(["id": "X-1", "board": "w/b", "status": "canceled"])
        #expect(canceled.struck)
        #expect(canceled.statusWords == "Canceled")
        #expect(canceled.boardLabel == "b")
        #expect(canceled.subtasksLine == nil)
        #expect(canceled.link == "https://acta.aaenz.no/w/b#X-1")
        #expect(card(["id": "LOO-9", "board": "aaen-studios/loom", "missing": true]).missing)
    }

    @Test func readsCardIdsAndLinks() {
        #expect(isCardId("LOO-206") && isCardId("CLX-4"))
        #expect(!isCardId("LOO-0") && !isCardId("loo-2") && !isCardId("LOO-2a"))
        let link = { (u: String) in cardLinkParts(u).map { "\($0.board) \($0.id)" } }
        #expect(link("https://acta.aaenz.no/aaen-studios/loom#LOO-342") == "aaen-studios/loom LOO-342")
        #expect(link("https://acta.aaenz.no/aaen-studios/loom/?view=list&task=LOO-1") == "aaen-studios/loom LOO-1")
        #expect(link("https://acta.aaenz.no/aaen-studios/loom") == nil)
        #expect(link("https://example.com/aaen-studios/loom#LOO-1") == nil)
        #expect(pastedCard(" https://acta.aaenz.no/aaen-studios/loom#LOO-342 ") == "https://acta.aaenz.no/aaen-studios/loom#LOO-342")
        #expect(pastedCard("https://acta.aaenz.no/docs") == nil)
        #expect(pastedCard("see https://acta.aaenz.no/aaen-studios/loom#LOO-342") == nil)
    }

    @Test func aMessageShowsItsCardsAsPills() {
        var cal = card(loo206)
        cal.id = "CAL-338"
        cal.board = "aaen-studios/calcite"
        let cards = ["LOO-206": card(loo206), "CAL-338": cal]
        let shown = { (t: String) in withCardPills(t, cards) }
        #expect(shown("LOO-206 is nearly done, and so is https://acta.aaenz.no/aaen-studios/calcite#CAL-338.")
            == "[LOO-206](loom-card:LOO-206) is nearly done, and so is [CAL-338](loom-card:CAL-338).")
        #expect(shown("See [the card](https://acta.aaenz.no/aaen-studios/loom#LOO-206) (LOO-206)")
            == "See [LOO-206](loom-card:LOO-206) ([LOO-206](loom-card:LOO-206))")
        // Ids it hasn't looked up, code, an address sent as plain text, and ids inside
        // something longer stay as they are.
        let kept = "UTF-8, LOO-207, `LOO-206`, x-LOO-206, LOO-206-b and <https://acta.aaenz.no/aaen-studios/loom#LOO-206>"
        #expect(shown(kept) == kept)
        // A link to the same id on another board isn't that card.
        #expect(shown("https://acta.aaenz.no/aaen-studios/other#LOO-206") == "https://acta.aaenz.no/aaen-studios/other#LOO-206")
        #expect(shown("```\nLOO-206\n```\nLOO-206") == "```\nLOO-206\n```\n[LOO-206](loom-card:LOO-206)")
        #expect(withCardPills("LOO-206", [:]) == "LOO-206")
    }

    @Test func aTypedIdForAKnownBoardBecomesAPiece() {
        let prefixes = ["LOO", "CAL"]
        let typed = { (t: String) in typedCard(t, prefixes: prefixes).map { "\($0.start) \($0.id)" } }
        #expect(typed("see LOO-206 ") == "4 LOO-206")
        #expect(typed("CAL-338,") == "0 CAL-338")
        #expect(typed("see LOO-206") == nil)
        #expect(typed("UTF-8 ") == nil)
        #expect(typed("xLOO-206 ") == nil)
        #expect(typed("CLX-450 ") == nil)
        // Once it's a piece, U+FFFC in the words, there's no id before the caret.
        #expect(typed("see \u{FFFC} ") == nil)
        // A card's piece goes as its id or its link, and copies as its words.
        let parts: [Either] = [.words("see "), .piece(cardPiece("LOO-206")), .words(" now")]
        #expect(composerText(parts) == "see LOO-206 now")
        #expect(composerText([.piece(cardPiece("https://acta.aaenz.no/aaen-studios/loom#LOO-206"))]) == "https://acta.aaenz.no/aaen-studios/loom#LOO-206")
        #expect(keptParts(parts) == nil)
    }

    @Test func cardsAreAskedForAsTheyShow() {
        let m = Model()
        m.synced = true
        let now = 10_000_000
        // The person's watches come once caught up too (WatchTests).
        let asked = m.cardFetches(now: now)
        #expect(asked.map { $0["name"] } == ["watch.list", "acta.boards"])
        #expect(m.cardFetches(now: now).isEmpty, "the boards are asked for once at a time")
        answer(m, asked.last, ["value": ["boards": [["board": "aaen-studios/loom", "prefix": "LOO", "name": "loom"]]]])
        #expect(m.boardPrefixes == ["LOO"])
        #expect(m.hasActa)
        #expect(m.cardFetches(now: nowMs()).isEmpty)
        #expect(m.cardFetches(now: nowMs() + boardsFreshMs + 1000).first?["name"] == "acta.boards", "and again ten minutes on")
    }

    @Test func aMessagesCardsAreLookedAtOnceWhileItShows() {
        let m = actaModel()
        let now = nowMs()
        func with(_ id: String, at: Int) -> JSON {
            var card = loo206
            if case .object(var o) = card {
                o["at"] = JSON(at)
                card = .object(o)
            }
            var msg = message(id, in: "c_1")
            if case .object(var o) = msg {
                o["cards"] = ["LOO-206": card]
                msg = .object(o)
            }
            return msg
        }
        came(m, [with("m_old", at: now - cardFreshMs), with("m_new", at: now - 1000)])
        let refresh = m.cardFetches(now: now).filter { $0["name"] == "cards.refresh" }
        #expect(refresh.count == 1)
        #expect(refresh.first?["args"]["messages"] == ["m_old"])
        #expect(m.cardFetches(now: now).filter { $0["name"] == "cards.refresh" }.isEmpty, "each message is looked at once while it shows")
        m.cardsChatOpened()
        #expect(m.cardFetches(now: now).filter { $0["name"] == "cards.refresh" }.count == 1, "and again when its chat shows again")
        #expect(m.messages.first?.cards["LOO-206"]?.title == "Show agents' usage limits in the sidebar")
        #expect(m.pillCard("loom-card:LOO-206")?.board == "aaen-studios/loom")
    }

    @Test func aCardInTheComposerIsLookedUpOnce() throws {
        let m = Model()
        let f = try #require(m.previewCard("LOO-206"))
        #expect(f["name"] == "card.preview")
        #expect(f["args"]["ref"] == "LOO-206")
        #expect(m.previewCard("LOO-206") == nil)
        answer(m, f, ["value": ["card": ["id": "LOO-206", "title": "T", "status": "done"]]])
        guard case .ready(let c) = m.acta.previews["LOO-206"] else {
            Issue.record("no card")
            return
        }
        #expect(c.title == "T")
        let g = try #require(m.previewCard("UTF-8"))
        answer(m, g, ["error": ["code": "not_found", "message": "No."]])
        #expect(m.acta.previews["UTF-8"] == .failed)
    }

    // MARK: Making a card

    @Test func aTitleIsTheFirstLineWithoutItsMarks() {
        #expect(titleFrom("\n\n## **Fix** the `pill`\nMore.") == "Fix the pill")
        #expect(titleFrom("- [ ] Ship it") == "Ship it")
        #expect(titleFrom("> Quoted") == "Quoted")
        #expect(titleFrom("") == "")
        let t = titleFrom(String(repeating: "word ", count: 40))
        #expect(t.hasSuffix("word…") && t.count <= 121)
    }

    @Test func theFormIsFilledFromTheMessageOnTheLastBoardUsed() throws {
        let m = actaModel()
        let ask = try #require(m.startCard(message: "m_1", text: "The pill needs a preview\nWith the description."))
        #expect(ask["name"] == "acta.board")
        let f = try #require(m.acta.form)
        #expect(f.board == "aaen-studios/loom")
        #expect(f.title == "The pill needs a preview")
        #expect(f.description == "The pill needs a preview\nWith the description.")
        #expect(m.formBoard("aaen-studios/calcite") != nil)
        m.toggleLabel("engine")
        m.formSize("s")
        m.formSize("s")
        m.formPriority("high")
        #expect(!m.canMake("  "))
        let made = try #require(m.makeCard(title: "Smooth it", description: "Words"))
        #expect(made["name"] == "card.create")
        #expect(made["args"] == ["board": "aaen-studios/calcite", "title": "Smooth it", "description": "Words", "labels": ["engine"], "message": "m_1", "priority": "high"])
        // Make does nothing while it's on its way.
        #expect(!m.canMake("Smooth it") && m.makeCard(title: "Smooth it", description: "") == nil)
        answer(m, made, ["value": ["card": ["id": "LOO-400"]]])
        #expect(m.acta.form == nil)
        #expect(m.boardChoices.map(\.board) == ["aaen-studios/calcite", "aaen-studios/loom"])
        #expect(m.boardChoices.map(\.name) == ["calcite", "loom"])
    }

    @Test func aNewBoardDropsTheLabelsAndARefusalKeepsTheForm() throws {
        let m = actaModel()
        _ = m.startCard(message: "m_1", text: "Title")
        m.toggleLabel("desktop")
        m.toggleLabel("Bug")
        m.toggleLabel("desktop")
        #expect(m.acta.form?.labels == ["Bug"])
        _ = m.formBoard("aaen-studios/calcite")
        #expect(m.acta.form?.labels == [])
        let made = try #require(m.makeCard(title: "Title", description: ""))
        answer(m, made, ["error": ["code": "mcp_failed", "message": "nope isn't one of the board's labels."]])
        let f = try #require(m.acta.form, "the form went")
        #expect(!f.making)
        #expect(f.error == "nope isn't one of the board's labels.")
        // The board's labels, once acta.board answers, are the form's.
        let ask = try #require(m.formBoard("aaen-studios/loom"))
        answer(m, ask, ["value": ["board": ["board": "aaen-studios/loom", "name": "loom", "prefix": "LOO", "labels": ["Bug", "ios"], "people": ["Elliot"]]]])
        #expect(m.formLabels == ["Bug", "ios"])
        #expect(m.formBoard("aaen-studios/loom") == nil, "a board's labels are asked for once")
    }

    // MARK: A card's preview

    @Test func theSheetMovesItsCardAndGoesBackWhenRefused() throws {
        let m = actaModel()
        let get = m.openCard(board: "aaen-studios/loom", id: "LOO-206")
        #expect(get["args"] == ["board": "aaen-studios/loom", "id": "LOO-206"])
        #expect(m.acta.sheet?.loading == true)
        answer(m, get, ["value": ["card": ["id": "LOO-206", "board": "aaen-studios/loom", "title": "Usage", "status": "in_progress", "subtasks": 2, "subtasks_done": 1,
                                           "description": "## Why", "priority": "high",
                                           "checklist": [["id": "s1", "title": "a", "done": true], ["id": "s2", "title": "b", "done": false]],
                                           "comments": [["by": "Elliot", "at": "2026-11-25T10:00:00Z", "body": "Hi"]]]]])
        let s = try #require(m.acta.sheet)
        let d = try #require(s.detail)
        #expect(!s.loading)
        #expect(d.description == "## Why")
        #expect(d.checklist.count == 2)
        #expect(d.chips == ["High"])
        #expect(commentDay(d.comments[0].at) == "25 Nov")
        // Moved at once, and back as it was when Acta refuses.
        #expect(m.moveCard("in_progress") == nil)
        let moved = try #require(m.moveCard("review"))
        #expect(moved["args"]["status"] == "review")
        #expect(m.acta.sheet?.detail?.card.status == "review")
        answer(m, moved, ["error": ["code": "unavailable", "message": "Acta didn't answer."]])
        #expect(m.acta.sheet?.detail?.card.status == "in_progress")
        #expect(m.acta.sheet?.error == "Acta didn't answer.")
        // A tick counts at once.
        let tick = try #require(m.toggleSubtask("s2"))
        #expect(tick["args"] == ["board": "aaen-studios/loom", "id": "LOO-206", "subtask": "s2", "done": true])
        #expect(m.acta.sheet?.detail?.card.subtasksDone == 2)
    }

    @Test func aCardThatChangedChangesItsPills() throws {
        let m = actaModel()
        came(m, [["id": "m_1", "conversation": "c_1", "role": "person", "from": "d_4", "at": 1, "blocks": [["kind": "text", "text": "Make it"]],
                  "made_cards": [["id": "LOO-206", "board": "aaen-studios/loom"], ["id": "LOO-999", "board": "aaen-studios/loom"]],
                  "cards": ["LOO-206": ["id": "LOO-206", "board": "aaen-studios/loom", "title": "Usage", "status": "todo", "at": JSON(nowMs())]]]])
        // A card made from it that the instance hasn't looked up has no pill.
        #expect(m.messages.first?.made == ["LOO-206"])
        _ = m.cardFetches(now: nowMs())
        let get = m.openCard(board: "aaen-studios/loom", id: "LOO-206")
        // While it's looked up, the sheet has what the pill knew.
        #expect(m.acta.sheet?.detail?.card.status == "todo")
        answer(m, get, ["value": ["card": ["id": "LOO-206", "board": "aaen-studios/loom", "title": "Usage", "status": "done"]]])
        #expect(m.messages.first?.cards["LOO-206"]?.status == "done")
        // The instance is asked to keep the message so, once.
        let refresh = m.cardFetches(now: nowMs()).first { $0["name"] == "cards.refresh" }
        #expect(refresh?["args"]["messages"] == ["m_1"])
        #expect(m.cardFetches(now: nowMs()).allSatisfy { $0["name"] != "cards.refresh" })
    }

    @Test func anAnswerForAnotherCardChangesNothing() {
        let m = actaModel()
        let first = m.openCard(board: "aaen-studios/loom", id: "LOO-1")
        _ = m.openCard(board: "aaen-studios/loom", id: "LOO-2")
        answer(m, first, ["value": ["card": ["id": "LOO-1", "board": "aaen-studios/loom", "title": "One"]]])
        #expect(m.acta.sheet?.id == "LOO-2")
        #expect(m.acta.sheet?.loading == true && m.acta.sheet?.detail == nil)
        #expect(!m.actaResult(["t": "result", "id": "app.other_1"]))
    }

    // MARK: Following

    @Test func followMakesAWatchForTheChatAndGoesBackWhenRefused() throws {
        let m = actaModel()
        #expect(m.conversation == "c_1")
        // The watches were asked for once caught up, and are again when it catches up again.
        #expect(m.cardFetches(now: nowMs()).allSatisfy { $0["name"] != "watch.list" })
        m.take(["t": "synced", "seq": 2])
        _ = m.openCard(board: "aaen-studios/loom", id: "LOO-206")
        let list = try #require(m.cardFetches(now: nowMs()).first { $0["name"] == "watch.list" })
        #expect(m.cardFetches(now: nowMs()).allSatisfy { $0["name"] != "watch.list" })
        #expect(m.following(board: "aaen-studios/loom", card: "LOO-206") == nil)
        answer(m, list, ["value": ["watches": [
            ["id": "w_1", "integration": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "c_2"],
            ["id": "w_2", "integration": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "", "wake": ["routine": "r_1"]],
            ["id": "w_9", "integration": "t3", "target": ["thread": "th_1"], "conversation": "c_1"],
        ]]])
        // The board's watch posts in another chat, and a routine's isn't one to follow.
        #expect(m.following(board: "aaen-studios/loom") == false)
        #expect(m.acta.watches?.map(\.id) == ["w_1", "w_2", "w_9"])
        let put = try #require(m.toggleFollow(board: "aaen-studios/loom", card: "LOO-206"))
        #expect(put["name"] == "watch.put")
        #expect(put["args"] == ["target": ["board": "aaen-studios/loom", "card": "LOO-206"], "conversation": "c_1"])
        // Shown at once; pressed again while it's made, it stays.
        #expect(m.following(board: "aaen-studios/loom", card: "LOO-206") == true)
        #expect(m.toggleFollow(board: "aaen-studios/loom", card: "LOO-206") == nil)
        // Its watch.updated comes first, then the answer: one watch.
        let made: JSON = ["id": "w_3", "integration": "acta", "target": ["board": "aaen-studios/loom", "card": "LOO-206"], "conversation": "c_1"]
        m.take(["t": "event", "kind": "watch.updated", "seq": 2, "watch": made])
        answer(m, put, ["value": ["watch": made]])
        #expect(m.acta.watches?.map(\.id) == ["w_1", "w_2", "w_9", "w_3"])
        // Following stops with watch.remove, and is back when that's refused.
        let remove = try #require(m.toggleFollow(board: "aaen-studios/loom", card: "LOO-206"))
        #expect(remove["name"] == "watch.remove")
        #expect(remove["args"] == ["id": "w_3"])
        #expect(m.following(board: "aaen-studios/loom", card: "LOO-206") == false)
        answer(m, remove, ["error": ["code": "not_found", "message": "There's no watch w_3."]])
        #expect(m.following(board: "aaen-studios/loom", card: "LOO-206") == true)
        // Following the board, refused: back as it was, with why on the sheet.
        let board = try #require(m.toggleFollow(board: "aaen-studios/loom"))
        answer(m, board, ["error": ["code": "not_found", "message": "Acta isn't connected."]])
        #expect(m.following(board: "aaen-studios/loom") == false)
        #expect(m.acta.sheet?.error == "Acta isn't connected.")
        // Taken away on another device.
        m.take(["t": "event", "kind": "watch.removed", "seq": 3, "watch": "w_3"])
        #expect(m.following(board: "aaen-studios/loom", card: "LOO-206") == false)
    }

    // MARK: A board

    @Test func aBoardIsSortedInItsColumnsAndFiltered() {
        let tasks = [
            BoardTask(id: "LOO-1", title: "Card LOO-1", status: "todo", assignee: "Elliot", labels: ["desktop"]),
            BoardTask(id: "LOO-2", title: "Card LOO-2", status: "in_progress"),
            BoardTask(id: "LOO-3", title: "Card LOO-3", status: "todo", assignee: "Kim", labels: ["desktop", "Bug"]),
            BoardTask(id: "LOO-4", title: "Card LOO-4", status: "done", assignee: "Elliot"),
            BoardTask(id: "LOO-5", title: "Card LOO-5", status: "canceled", assignee: "Elliot"),
        ]
        let ids = { (t: [BoardTask]) in t.map(\.id) }
        #expect(ids(column(tasks, "todo")) == ["LOO-1", "LOO-3"])
        #expect(ids(column(tasks, "in_progress")) == ["LOO-2"])
        #expect(column(tasks, "review").isEmpty)
        #expect(ids(column(tasks, "done")) == ["LOO-4"])
        #expect(boardColumns.allSatisfy { !ids(column(tasks, $0)).contains("LOO-5") })
        #expect(ids(filtered(tasks, search: "loo-3", label: nil, assignee: nil)) == ["LOO-3"])
        #expect(ids(filtered(tasks, search: "card", label: "desktop", assignee: nil)) == ["LOO-1", "LOO-3"])
        #expect(ids(filtered(tasks, search: "", label: "desktop", assignee: "Kim")) == ["LOO-3"])
        #expect(tasks[0].initial == "E")
        #expect(tasks[1].initial == "")
    }

    @Test func theBoardAsksForItsCardsAndMovesOne() throws {
        let m = actaModel()
        let asks = m.openBoard()
        #expect(asks.count == 3)
        #expect(asks[0]["args"] == ["board": "aaen-studios/loom", "limit": 200])
        #expect(asks[1]["args"] == ["board": "aaen-studios/loom", "status": ["done"], "limit": 30])
        #expect(asks[2]["name"] == "acta.board")
        // The done may come first; the open go before them.
        answer(m, asks[1], ["value": ["tasks": [["id": "LOO-4", "title": "d", "status": "done"]]]])
        #expect(m.acta.screen?.loading == true)
        answer(m, asks[0], ["value": ["tasks": [["id": "LOO-1", "title": "a", "status": "todo", "assignee": "Elliot"]]]])
        answer(m, asks[2], ["value": ["board": ["board": "aaen-studios/loom", "labels": ["Bug"], "people": ["Elliot", "Kim"]]]])
        let b = try #require(m.acta.screen)
        #expect(!b.loading)
        #expect(b.tasks.map(\.id) == ["LOO-1", "LOO-4"])
        #expect(b.shown.map(\.id) == ["LOO-1"])
        m.boardStatus("done")
        #expect(m.acta.screen?.shown.map(\.id) == ["LOO-4"])
        // Moved where it is: nothing. Elsewhere: moved, and back when refused.
        #expect(m.moveTask("LOO-1", to: "todo") == nil)
        #expect(m.moveTask("LOO-1", to: "canceled") == nil)
        let moved = try #require(m.moveTask("LOO-1", to: "review"))
        #expect(moved["args"] == ["board": "aaen-studios/loom", "id": "LOO-1", "status": "review"])
        #expect(m.acta.screen?.tasks.first?.status == "review")
        answer(m, moved, ["error": ["code": "mcp_failed", "message": "No."]])
        #expect(m.acta.screen?.tasks.first?.status == "todo")
        #expect(m.acta.screen?.error == "No.")
        let again = try #require(m.moveTask("LOO-1", to: "done"))
        answer(m, again, ["value": ["card": ["id": "LOO-1", "board": "aaen-studios/loom", "title": "a", "status": "done"]]])
        #expect(m.acta.screen?.tasks.first?.status == "done")
        // Another board keeps the column picked.
        _ = m.openBoard("aaen-studios/calcite")
        #expect(m.acta.screen?.board == "aaen-studios/calcite")
        #expect(m.acta.screen?.status == "done")
        #expect(m.acta.screen?.tasks.isEmpty == true)
    }

    @Test func boardsIsInThePlusMenuWithActaConnected() {
        let none = caughtUp()
        #expect(!none.plusGroups("").joined().contains(.boards))
        let m = actaModel()
        #expect(m.plusGroups("").last?.last == .boards)
        #expect(m.plusGroups("boa").joined().contains(.boards))
        #expect(!m.plusGroups("emoji").joined().contains(.boards))
    }
}
