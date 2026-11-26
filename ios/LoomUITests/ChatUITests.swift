import XCTest

/// The chat as a person uses it, on sample frames: the emoji picker in the keyboard's
/// place (LOO-361) and a long message cut with Show more (LOO-357). Each step keeps a
/// screenshot in the result bundle, and in the folder `LOOM_UI_SHOTS` names, when it
/// names one.
@MainActor
final class ChatUITests: XCTestCase {
    /// The app on sample frames, at the Builder's chat, whose reply with the release
    /// notes is long enough to be cut.
    private func builder() -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-sample", "-open", "a_1"]
        app.launch()
        XCTAssertTrue(smiley(app).waitForExistence(timeout: 20), "the chat's composer")
        return app
    }

    /// The composer's smiley, found by its identifier, as the system keyboard has an Emoji
    /// key too. Its label says what it brings up: `Emoji`, or `Keyboard` while the picker
    /// is up.
    private func smiley(_ app: XCUIApplication) -> XCUIElement {
        app.buttons["smiley"]
    }

    /// Waits for the smiley to say it brings up `label`.
    private func smiley(_ app: XCUIApplication, says label: String, timeout: TimeInterval = 5) -> Bool {
        let says = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", label), object: smiley(app))
        return XCTWaiter().wait(for: [says], timeout: timeout) == .completed
    }

    private func shot(_ app: XCUIApplication, _ name: String) {
        let picture = app.screenshot()
        let a = XCTAttachment(screenshot: picture)
        a.name = name
        a.lifetime = .keepAlways
        add(a)
        if let dir = ProcessInfo.processInfo.environment["LOOM_UI_SHOTS"], !dir.isEmpty {
            let file = URL(fileURLWithPath: dir).appendingPathComponent(name.replacingOccurrences(of: " ", with: "-") + ".png")
            try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
            try? picture.pngRepresentation.write(to: file)
        }
    }

    /// Opens the emoji picker with the composer's smiley.
    private func openPicker(_ app: XCUIApplication) -> XCUIElement {
        smiley(app).tap()
        let find = app.textFields["Find an emoji"]
        XCTAssertTrue(find.waitForExistence(timeout: 5), "the picker")
        XCTAssertTrue(smiley(app, says: "Keyboard"), "the smiley is a keyboard while the picker is up")
        return find
    }

    // MARK: The emoji picker (LOO-361)

    func testASwipeDownOnThePickerAtItsTopPutsItAway() {
        let app = builder()
        let find = openPicker(app)
        shot(app, "picker up")
        // From the grid's first row, under the search and the group tabs, down 150
        // points: the picker is about as tall as the keyboard, so a swipe from lower down
        // runs off the screen.
        let from = find.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).withOffset(CGVector(dx: 0, dy: 100))
        from.press(forDuration: 0.05, thenDragTo: from.withOffset(CGVector(dx: 0, dy: 150)))
        let went = find.waitForNonExistence(timeout: 5)
        shot(app, "picker swiped away")
        XCTAssertTrue(went, "the picker went")
        XCTAssertTrue(smiley(app, says: "Emoji"))
    }

    func testATapInTheChatPutsThePickerAway() {
        let app = builder()
        let find = openPicker(app)
        let words = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Did the build on main pass")).firstMatch
        XCTAssertTrue(words.waitForExistence(timeout: 5))
        words.tap()
        let went = find.waitForNonExistence(timeout: 5)
        shot(app, "picker tapped away")
        XCTAssertTrue(went, "the picker went")
        XCTAssertTrue(smiley(app, says: "Emoji"))
    }

    func testTheKeyboardGlyphSwapsTheKeyboardAndThePicker() {
        let app = builder()
        let find = openPicker(app)
        let field = app.textViews["Message"]
        smiley(app).tap()
        XCTAssertTrue(find.waitForNonExistence(timeout: 5), "the picker went for the keyboard")
        XCTAssertTrue(smiley(app, says: "Emoji"))
        shot(app, "keyboard back")
        XCTAssertTrue(field.value(forKey: "hasKeyboardFocus") as? Bool ?? false, "the composer has the cursor")
        smiley(app).tap()
        let back = find.waitForExistence(timeout: 5)
        shot(app, "picker back")
        XCTAssertTrue(back, "the picker came back")
        XCTAssertTrue(smiley(app, says: "Keyboard"))
    }

    // MARK: A long message (LOO-357)

    func testShowMoreOpensALongMessageAndShowLessFoldsIt() {
        let app = builder()
        let more = app.buttons["Show more"].firstMatch
        XCTAssertTrue(more.waitForExistence(timeout: 10), "the long reply is cut")
        shot(app, "cut")
        more.tap()
        let less = app.buttons["Show less"].firstMatch
        XCTAssertTrue(less.waitForExistence(timeout: 5), "it opened")
        XCTAssertFalse(app.buttons["Show more"].exists)
        shot(app, "opened")
        less.tap()
        XCTAssertTrue(app.buttons["Show more"].firstMatch.waitForExistence(timeout: 5), "it folded again")
        shot(app, "folded again")
    }

    func testATapOnTheCutWordsOpensThem() {
        let app = builder()
        XCTAssertTrue(app.buttons["Show more"].firstMatch.waitForExistence(timeout: 10), "the long reply is cut")
        let words = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Routines run on webhooks")).firstMatch
        XCTAssertTrue(words.waitForExistence(timeout: 5))
        words.tap()
        let opened = app.buttons["Show less"].firstMatch.waitForExistence(timeout: 5)
        shot(app, "opened by a tap on its words")
        XCTAssertTrue(opened, "it opened")
    }
}
