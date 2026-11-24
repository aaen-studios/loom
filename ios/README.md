# ios

Swift and SwiftUI: the iPhone app, in one Xcode project generated from a spec with [XcodeGen](https://github.com/yonaskolb/XcodeGen) rather than a hand-edited `.xcodeproj`. It links the same [core](../core/) as the desktop app, and does less than the desktop where that suits a phone. Its widgets, Live Activities and push extension join the same project later.

The app holds no logic (PLAN.md, "The apps hold no logic"). It draws screens and reaches the system: the camera, photos, files, notifications, the clipboard. Everything else is the core's, which it talks to only in JSON, through `loom_core_start`, `loom_core_send` and `loom_core_stop` ([core/README.md](../core/README.md#between-the-app-and-the-core)).

## Building it

On a Mac with Xcode and XcodeGen (`brew install xcodegen`), from the repository's root:

```sh
calx scripts/ios.calx           # the core for a phone and the simulator, then ios/Loom.xcodeproj
calx scripts/ios.calx --sim     # the core for the simulator alone, which builds faster
calx scripts/ios.calx --sample  # no core: the project alone, with the app on sample frames
open ios/Loom.xcodeproj
```

[`scripts/ios.calx`](../scripts/ios.calx) builds `core/lib.calx` with `calx build --staticlib --target aarch64-ios` and `aarch64-ios-sim`, gives `loomcore.h` a module map so Swift imports it as `LoomCore`, puts both in `build/ios/LoomCore.xcframework`, and runs `xcodegen` with `LOOM_CORE=YES`, which includes [`core.yml`](core.yml) to link it. `Loom.xcodeproj` isn't kept in git: change [`project.yml`](project.yml) and run the script again.

The app icon is the one [`scripts/icons.calx`](../scripts/icons.calx) makes, [`design/logo/ios/AppIcon.appiconset`](../design/logo/ios/AppIcon.appiconset), light, dark and tinted. Xcode's asset compiler doesn't follow links, so the script copies it into `build/ios/Icons.xcassets` each time, `--sample` too, and the project takes it from there.

The core builds for iOS with Calx's iOS targets ([CLX-408](https://acta.aaenz.no/aaen-studios/clx#CLX-408), [CLX-410](https://acta.aaenz.no/aaen-studios/clx#CLX-410)): nightly 202611231257 or newer.

A build without the core (`--sample`, or `xcodegen` run by hand) runs on [`SampleCore`](Loom/Core/SampleCore.swift): sample agents and chats, a reply that streams, and a question to answer, in the frames the core sends. SwiftUI previews use it too. With the core linked, `-sample` at launch runs on it as well.

To run against an instance, set `LOOM_SERVER` in the scheme's environment, or type its address on the sign-in screen.

## Testing it against an instance

```sh
calx scripts/ios.calx --sim
calx tests/ios/live.calx
```

[`tests/ios/live.calx`](../tests/ios/live.calx) starts an instance with the fake agent ([tests/README.md](../tests/README.md)), builds the app, and runs it in the simulator, headless: it joins with the QR code's text, a reply streams in, it answers a question, and it follows the instance when it moves. Each step is checked on the instance, and each screen is a screenshot in `build/ios/live/`. With no one to tap, a debug build takes these from its launch arguments, each doing what its screen's button does: `-link <qr>` joins, `-say <text>` sends to the agent `-open` opened, `-allow` answers its questions yes, `-signIn` taps Sign in on its sign-in cards, and `-callback <url>` hands the app what a browser brought back to the `loom:` redirect (`fromLaunch` in [`Session.swift`](Loom/App/Session.swift)).

## Files

- [`Loom/Core/`](Loom/Core/): the C boundary. [`Core.swift`](Loom/Core/Core.swift) starts the core with its config (its cache in Application Support/Loom), sends it frames, and hands what it says to the main actor in batches. [`JSON.swift`](Loom/Core/JSON.swift) is the frames' JSON. [`SampleCore.swift`](Loom/Core/SampleCore.swift) stands in for the core without it.
- [`Loom/Model/Model.swift`](Loom/Model/Model.swift): what the screens show, as the core tells it, as [`desktop/src/model.rs`](../desktop/src/model.rs) does for the desktop app. Lists come from the core's cache with `core.get` and are fetched again when an event says they changed. Only a reply still being written lives here alone, from its deltas.
- [`Loom/App/`](Loom/App/): the app, and [`Session.swift`](Loom/App/Session.swift), which joins the core, the model and the system: `app.active` and `app.wake` as the app comes and goes, `app.network` when the network changes, `core.device_ask` for the clipboard, and `core.notify` as a notification. [`Notifications.swift`](Loom/App/Notifications.swift) asks for permission, and gives an approval Allow and Deny.
- [`Loom/Views/`](Loom/Views/): the screens. [`Cards.swift`](Loom/Views/Cards.swift) has the cards in a chat in the desktop's one frame: the service's badge or an icon at its start (`BadgeView`, from `shared/badge.calx`), the title with the agent's thread beside it, tugging while the card waits for the person, where it goes under it, and what it does in a pill; an agent's `show_card`, with choices as tiles, two to a row; loom's "Sign in to acta" for an MCP server, which opens its sign-in page over the app ([`SignIn.swift`](Loom/App/SignIn.swift)) and catches the browser coming back to `loom:/callback` (`mcp.sign_in`, `mcp.signed_in`), then says whom the person is signed in as once `mcp.list` says so, or why a sign-in didn't finish; an MCP server an agent proposed, with loom's knot (`Knot`) and the server's badge joined by the agent's thread (`SpanView`), slack while it waits and pulled straight once it's connected, its address or command, its settings, a masked field for each secret the person doesn't have, **Add** and **Cancel**, then **Sign in** and **Later** for one that needs it (`mcp.add_answer`), its tools once it's connected, and one quiet line once it's settled otherwise; and the questions an agent waits on, as the core put them in words: steps, the agent's reason, **Allow** with **Always allow** in the menu beside it, **Deny** and **Details**.
- [`Loom/Design/`](Loom/Design/): loom's colours and type from the design tokens ([`design/generated/Tokens.swift`](../design/generated/Tokens.swift), with the `phone` sizes), each agent's thread, and Markdown.

## The screens

- **Sign in**: the instance's address, a username and a password (`login`), or **Scan a code**, VisionKit's scanner reading the QR code the desktop app shows under Add a phone (`link`, with the code's text). The simulator has no camera, so there the text can be pasted.
- **Agents**: each agent as one ongoing chat, with its thread and what it's doing, as the desktop's sidebar shows them.
- **A chat**: as the desktop draws it ([PLAN.md, "One answer at the end"](../PLAN.md#one-answer-at-the-end)). The person's bubbles are the agent's colour itself with Vibrant bubbles, the default, or else a breath of it; what another agent sent stays grey. A reply's answer sits in a grey bubble, or is plain text if the person picked that, with Markdown and code blocks, one answer at the end, its steps (thoughts, tool calls in their places, and what it said on the way, smaller and quieter) folded under one line that shows the step it's on while it works. A routine's quiet run isn't drawn, and its old prompts aren't shown; what it posts is. Pictures are sized from their file's header before they load, to fit the column and 360 × 400. Two or more together are one stack of 240 × 180, as on the desktop, which opens them big over everything, a page each to swipe through, with each one's name and "1 / 3". Then questions with Allow and Deny, and the composer, with photos (sent as JPEG) and files.
- **Settings**: the account, the instance, the person's devices, the chat (Agent replies, Bubbles or Plain, and Vibrant bubbles, kept on the phone), notifications, and signing out.

When the instance moves, the core follows it and the app says so: "Your instance moved to …". A move the core won't follow (`moved_unsigned`, `moved_elsewhere`) shows the core's reason, and the app stays where it was.

For a screenshot of each screen, launch it with `-sample`, and `-signedOut`, `-open <agent id>` or `-settings`: `xcrun simctl launch booted no.aaenz.loom -sample -open a_1`.

## Push

Notifications show while the app runs, from the core's `core.notify`. Push to the lock screen goes through APNs, from the instance or through the push relay ([protocol.md, "Push"](../protocol/protocol.md#push)):

- **Permission**: Settings → Notifications asks, and on a yes the app registers with APNs.
- **The token**: `Session.registered(pushToken:)` keeps it, and sends the core `push.register` with `token` and `env` on each `welcome`, since the core sends a command only while it's connected. The core seals it for the push relay when `welcome` has a `relay`.
- **Allow and Deny**: an `approval.needed` notification has both, answered as `approve` without opening the app.
- **Still to come** ([LOO-253](https://acta.aaenz.no/aaen-studios/loom#LOO-253)): the `aps-environment` entitlement and signing with Elliot's Apple team, the notification service extension that opens the push relay's sealed notifications with `loom_push_open`, and TestFlight. Until then the app gets no APNs token, so notifications show only while it runs.
