import SwiftUI

// Settings → Routines on the phone (PLAN.md, "The routine's page"): the person's
// routines, each with when it runs or how it stands, and New routine; and a routine's
// page, its choices as menu rows as the phone's Settings has them. The model
// (Routines.swift) says what each shows and what Save sends.

/// The person's routines, and New routine under them.
struct RoutinesView: View {
    @Environment(Session.self) private var session
    /// The routine whose page is pushed, by id, or "" for a new one's.
    @State private var open: String?

    var body: some View {
        let model = session.model
        List {
            if !model.routineList.isEmpty {
                Section {
                    ForEach(model.routineList) { r in
                        Button {
                            open = r.id
                        } label: {
                            RoutineRow(routine: r)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            Section {
                Button {
                    open = ""
                } label: {
                    HStack(spacing: Tokens.Space.m) {
                        Image(systemName: "plus").foregroundStyle(P.ink2).frame(width: 22)
                        Text("New routine").font(Fonts.body).foregroundStyle(P.ink)
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(P.side)
        .navigationTitle("Routines")
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(item: $open) { id in
            RoutinePage(routine: id)
        }
        .onAppear {
            model.loadRoutines()
            session.flush()
            // `-routine <id>` at launch opens that routine's page, `-routine new` a new
            // one's, for screenshots.
            let args = ProcessInfo.processInfo.arguments
            if open == nil, let i = args.firstIndex(of: "-routine"), i + 1 < args.count {
                open = args[i + 1] == "new" ? "" : args[i + 1]
            }
        }
    }
}

/// A routine's row: its clock in its agent's colour while it's on, its name, and its line,
/// with a dot for how it stands.
private struct RoutineRow: View {
    @Environment(Session.self) private var session
    var routine: Routine

    var body: some View {
        let model = session.model
        let (line, mark) = model.routineLine(routine)
        let tint = model.agent(routine.agent).map { P.thread($0.color) } ?? P.ink3
        HStack(spacing: Tokens.Space.m) {
            Image(systemName: "clock").foregroundStyle(mark == .off ? P.ink3 : tint).frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(routine.name).font(Fonts.body).foregroundStyle(P.ink)
                Text(line).font(Fonts.caption)
                    .foregroundStyle(mark == .wasted ? P.warn : mark == .failed ? P.bad : P.ink3)
            }
            .lineLimit(1)
            Spacer(minLength: 0)
            dot(mark)
            Image(systemName: "chevron.right").font(.system(size: 12, weight: .semibold)).foregroundStyle(P.ink3)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private func dot(_ mark: RoutineMark) -> some View {
        switch mark {
        case .on: Circle().fill(P.ok).frame(width: 7, height: 7)
        case .wasted: Circle().fill(P.warn).frame(width: 7, height: 7)
        case .failed: Circle().fill(P.bad).frame(width: 7, height: 7)
        case .off: Circle().strokeBorder(P.ink3, lineWidth: 1).frame(width: 7, height: 7)
        }
    }
}

/// A routine's page, or a new one's (`routine` ""): its name, agent, prompt, gate, model
/// and effort; When, with Repeat and what it picks; its options; and Run now and Delete.
struct RoutinePage: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var routine: String
    @State private var draft: RoutineDraft?
    @State private var confirmDelete = false
    /// The instance's address the person signed in with, for the webhook's.
    @AppStorage("loom.server") private var server = ""
    /// The row whose value was just copied, which says so for a moment.
    @State private var copied: String?

    var body: some View {
        let model = session.model
        let saved = model.routineList.first { $0.id == routine }
        Group {
            if let draft {
                form(saved, Binding { draft } set: { self.draft = $0 })
            } else {
                P.side
            }
        }
        .background(P.side)
        .navigationTitle(saved?.name ?? "New routine")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    guard let draft, let f = model.saveRoutine(draft) else { return }
                    session.send(f)
                    dismiss()
                }
                .font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                .disabled(draft == nil)
            }
        }
        .confirmationDialog("Delete \(saved?.name ?? "it")?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                session.send(model.deleteRoutine(routine))
                dismiss()
            }
        }
        .onAppear {
            fill()
            session.send(model.loadRoutineRuns(routine.isEmpty ? nil : routine))
        }
        .onDisappear { _ = model.loadRoutineRuns(nil) }
        // A routine's page opened before the routines came fills once its routine has.
        .onChange(of: saved == nil) { fill() }
        // Its Next, from the instance, for the schedule as it stands.
        .onChange(of: draft?.scheduleJSON, initial: true) { _, now in
            session.send(model.askScheduleNext(now ?? .null))
        }
    }

    /// A row that copies its value with a tap: its title, what it shows, and its copy
    /// glyph, or "Copied" for a moment once it's copied.
    private func copyRow(_ label: String, _ value: String, shown: String) -> some View {
        Button {
            UIPasteboard.general.string = value
            copied = label
            Task {
                try? await Task.sleep(for: .seconds(1.5))
                if copied == label { copied = nil }
            }
        } label: {
            HStack(spacing: Tokens.Space.s) {
                Text(label).font(Fonts.body).foregroundStyle(P.ink)
                Spacer(minLength: Tokens.Space.m)
                if copied == label {
                    Text("Copied").font(Fonts.body).foregroundStyle(P.ink3)
                } else {
                    Text(shown).font(Fonts.code).foregroundStyle(P.ink3).lineLimit(1).truncationMode(.middle)
                    Image(systemName: "doc.on.doc").font(.system(size: 13)).foregroundStyle(P.ink3)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Copy the \(label.lowercased())")
    }

    /// The page's draft as it opens: the routine's as it was saved, once it's here, or a
    /// new one's for the agent shown.
    private func fill() {
        guard draft == nil else { return }
        let model = session.model
        if routine.isEmpty {
            draft = RoutineDraft(agent: model.selected ?? listed(model.agents).first?.id ?? "")
        } else if let saved = model.routineList.first(where: { $0.id == routine }) {
            draft = RoutineDraft(saved)
        }
    }

    private func form(_ saved: Routine?, _ d: Binding<RoutineDraft>) -> some View {
        let model = session.model
        let agent = model.agent(d.wrappedValue.agent)
        return List {
            Section {
                HStack {
                    Text("Name").font(Fonts.body).foregroundStyle(P.ink)
                    TextField("Morning brief", text: d.name)
                        .multilineTextAlignment(.trailing)
                        .foregroundStyle(P.ink3)
                }
                Picker(selection: d.agent) {
                    ForEach(listed(model.agents)) { a in Text(a.name).tag(a.id) }
                } label: {
                    Text("Agent").font(Fonts.body).foregroundStyle(P.ink)
                }
                VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                    Text("Prompt").font(Fonts.caption).foregroundStyle(P.ink3)
                    TextField("What it's asked each time", text: d.prompt, axis: .vertical)
                        .lineLimit(2...10)
                }
                .padding(.vertical, Tokens.Space.xs)
                HStack {
                    Text("Gate").font(Fonts.body).foregroundStyle(P.ink)
                    TextField("Command", text: d.gate)
                        .font(Fonts.code)
                        .multilineTextAlignment(.trailing)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .foregroundStyle(P.ink2)
                }
                if !d.wrappedValue.gate.trimmingCharacters(in: .whitespaces).isEmpty {
                    HStack {
                        Text("Seconds").font(Fonts.body).foregroundStyle(P.ink)
                        TextField("60", text: d.gateSeconds)
                            .keyboardType(.numberPad)
                            .multilineTextAlignment(.trailing)
                            .foregroundStyle(P.ink3)
                    }
                }
                if let agent {
                    ForEach(agent.routinePicks, id: \.id) { option in
                        pick(option, agent, d)
                    }
                }
            } header: {
                if let words = saved?.cost.words, !words.isEmpty {
                    Text(words)
                        .font(Fonts.caption)
                        .foregroundStyle(saved?.cost.wasted == true ? P.warn : P.ink3)
                        .textCase(nil)
                        .frame(maxWidth: .infinity)
                }
            }
            Section("When") {
                when(d)
                // When it runs next, on a schedule.
                let next = model.nextRuns(saved, d.wrappedValue)
                if ["hours", "days"].contains(d.wrappedValue.repeatKind), !next.isEmpty {
                    HStack {
                        Text("Next").font(Fonts.body).foregroundStyle(P.ink)
                        Spacer()
                        Text(nextRunsWords(next)).font(Fonts.body).foregroundStyle(P.ink3).lineLimit(1).minimumScaleFactor(0.8)
                    }
                }
            }
            Section("Options") {
                toggle("On", d.on)
                toggle("Notify me", d.notify)
                toggle("Catch up", d.catchUp)
                toggle("A thread of its own", d.thread)
                if !d.wrappedValue.byHookAlone {
                    toggle("Webhook too", d.webhook)
                }
                // Where to post, once it has a webhook: to give GitHub or anything else.
                if let saved, !saved.hook.isEmpty, d.wrappedValue.webhook || d.wrappedValue.byHookAlone {
                    copyRow("Address", hookAddress(server: server, hook: saved.hook), shown: hookAddress(server: server, hook: saved.hook))
                    copyRow("Secret", saved.secret, shown: String(repeating: "•", count: 12))
                }
            }
            if let saved, !model.routineRuns.isEmpty, model.routineRunsFor == saved.id {
                Section("History") {
                    RunHistory(runs: model.routineRuns)
                }
            }
            if saved != nil {
                Section {
                    HStack(spacing: Tokens.Space.m) {
                        Button("Run now") {
                            session.send(model.runRoutine(routine))
                            dismiss()
                        }
                        .buttonStyle(.loomQuiet)
                        .frame(maxWidth: .infinity)
                        Button("Delete") { confirmDelete = true }
                            .buttonStyle(.loomQuiet)
                            .foregroundStyle(P.bad)
                            .frame(maxWidth: .infinity)
                    }
                }
                .listRowBackground(Color.clear)
                .listRowInsets(EdgeInsets())
            }
        }
        .font(Fonts.body)
        .scrollContentBackground(.hidden)
        .scrollDismissesKeyboard(.interactively)
    }

    /// The routine's model or effort: what its agent's program offers, the agent's own
    /// marked default, which it follows until another is picked.
    private func pick(_ option: ConfigOption, _ agent: Agent, _ d: Binding<RoutineDraft>) -> some View {
        let effort = option.category == "thought_level"
        let own = agent.chosen(option.id, defaults: session.model.defaults(for: agent.program))
        let value = effort ? d.effort : d.model
        let on = Binding<String> {
            option.values.contains { $0.value == value.wrappedValue } ? value.wrappedValue : own
        } set: {
            value.wrappedValue = $0 == own ? "" : $0
        }
        return Picker(selection: on) {
            ForEach(option.values, id: \.value) { v in
                Text(v.value == own ? "\(v.name) · default" : v.name).tag(v.value)
            }
        } label: {
            Text(effort ? "Effort" : "Model").font(Fonts.body).foregroundStyle(P.ink)
        }
    }

    private func toggle(_ label: String, _ on: Binding<Bool>) -> some View {
        Toggle(isOn: on) {
            Text(label).font(Fonts.body).foregroundStyle(P.ink)
        }
    }

    private func menu(_ label: String, _ on: Binding<String>, _ options: [(String, String)]) -> some View {
        Picker(selection: on) {
            ForEach(options, id: \.0) { Text($0.1).tag($0.0) }
        } label: {
            Text(label).font(Fonts.body).foregroundStyle(P.ink)
        }
    }

    /// When: Repeat, then every few hours within a window, the days and times, or what
    /// event it runs on.
    @ViewBuilder
    private func when(_ d: Binding<RoutineDraft>) -> some View {
        let model = session.model
        let kind = d.wrappedValue.repeatKind
        let kinds = [("hours", "Every few hours"), ("days", "On days"), ("webhook", "Webhook")]
            + (model.watchable || kind == "event" ? [("event", "On an event")] : [])
        menu("Repeat", Binding { kind } set: { d.wrappedValue.pick(repeat: $0, fresh: model.newEvent("acta")) }, kinds)
        switch kind {
        case "hours":
            menu("Every", Binding { String(d.wrappedValue.schedule.every) } set: { d.wrappedValue.schedule.every = Int($0) ?? 120 }, everyChoices(d.wrappedValue.schedule.every))
            clock("From", d.schedule.from)
            clock("To", d.schedule.to)
            DaysRow(days: d.schedule.days)
        case "days":
            DaysRow(days: d.schedule.days)
            TimesRow(draft: d)
        case "event":
            eventRows(d)
        default:
            EmptyView()
        }
    }

    /// Every few minutes or hours: the usual ones, and the routine's own when it's another.
    private func everyChoices(_ now: Int) -> [(String, String)] {
        var minutes = [15, 30, 60, 120, 180, 240, 360, 720]
        if !minutes.contains(now) {
            minutes.append(now)
            minutes.sort()
        }
        return minutes.map { m in
            let words = m % 60 != 0 ? "\(m) minutes" : m == 60 ? "1 hour" : "\(m / 60) hours"
            return (String(m), words)
        }
    }

    /// A time of day, "HH:MM", as the clock picks it.
    private func clock(_ label: String, _ text: Binding<String>) -> some View {
        DatePicker(selection: Binding { dateOf(text.wrappedValue) } set: { text.wrappedValue = hhmm($0) }, displayedComponents: .hourAndMinute) {
            Text(label).font(Fonts.body).foregroundStyle(P.ink)
        }
        .environment(\.locale, Locale(identifier: "en_GB"))
    }

    /// "On an event"'s rows: Acta or T3 when the person has both, which server when they
    /// have several, and on Acta the board and what happens on it, or on T3 the thread.
    @ViewBuilder
    private func eventRows(_ d: Binding<RoutineDraft>) -> some View {
        let model = session.model
        let e = d.wrappedValue.event
        let t3 = model.eventServers("t3")
        if !model.acta.boards.isEmpty && !t3.isEmpty {
            menu("On", Binding { e.integration } set: { k in
                if d.wrappedValue.event.integration != k { d.wrappedValue.event = model.newEvent(k) }
            }, [("acta", "Acta"), ("t3", "T3")])
        }
        let servers = model.eventServers(e.integration)
        if servers.count > 1 {
            menu("Server", Binding { e.server.isEmpty && e.integration == "acta" ? "acta" : e.server } set: { d.wrappedValue.event.server = $0 },
                 servers.map { ($0, $0) })
        }
        if e.integration == "t3" {
            HStack {
                Text("Thread").font(Fonts.body).foregroundStyle(P.ink)
                TextField("th_9", text: d.event.thread)
                    .font(Fonts.code)
                    .multilineTextAlignment(.trailing)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .foregroundStyle(P.ink2)
            }
        } else {
            menu("Board", Binding { e.board } set: { d.wrappedValue.pick(board: $0) },
                 model.acta.boards.map { ($0.board, boardLabel($0.board)) } + (model.acta.boards.contains { $0.board == e.board } || e.board.isEmpty ? [] : [(e.board, boardLabel(e.board))]))
            menu("Event", Binding { e.kind } set: { d.wrappedValue.pick(kind: $0) },
                 [("moved", "Moves to"), ("created", "New card"), ("commented", "Comment")] + (["moved", "created", "commented"].contains(e.kind) ? [] : [(e.kind, "Any change")]))
            if e.kind == "moved" {
                menu("Status", d.event.to, routineStatuses.map { ($0.key, $0.label) })
            }
        }
    }
}

/// The days a routine runs on, as seven round toggles, with "Weekdays" or "Every day"
/// beside them when that's what's picked.
private struct DaysRow: View {
    @Binding var days: UInt8

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
            HStack {
                Text("Days").font(Fonts.body).foregroundStyle(P.ink)
                Spacer()
                if days == everyDay || days == weekdays {
                    Text(days == everyDay ? "Every day" : "Weekdays").font(Fonts.body).foregroundStyle(P.ink3)
                }
            }
            HStack(spacing: 6) {
                ForEach(0..<7, id: \.self) { k in
                    let on = days & (1 << k) != 0
                    Button {
                        days ^= 1 << k
                    } label: {
                        Text(["M", "T", "W", "T", "F", "S", "S"][k])
                            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                            .foregroundStyle(on ? P.buttonInk : P.ink)
                            .frame(width: 36, height: 36)
                            .background(Circle().fill(on ? P.button : Color.clear))
                            .overlay(Circle().strokeBorder(on ? Color.clear : P.lineStrong, lineWidth: 1))
                            .contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(Calendar.current.weekdaySymbols[(k + 1) % 7])
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
        }
        .padding(.vertical, Tokens.Space.xs)
    }
}

/// A routine's times of day as chips, each taken off with a tap, and the clock that adds
/// one.
private struct TimesRow: View {
    @Binding var draft: RoutineDraft
    @State private var adding = dateOf("12:00")

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
            HStack {
                Text("Times").font(Fonts.body).foregroundStyle(P.ink)
                Spacer()
                DatePicker("", selection: $adding, displayedComponents: .hourAndMinute)
                    .labelsHidden()
                    .environment(\.locale, Locale(identifier: "en_GB"))
                Button {
                    draft.add(time: hhmm(adding))
                } label: {
                    Image(systemName: "plus").font(.system(size: 14, weight: .semibold)).foregroundStyle(P.ink2)
                        .frame(width: 32, height: 32)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add the time")
            }
            if !draft.schedule.times.isEmpty {
                HStack(spacing: 6) {
                    ForEach(draft.schedule.times, id: \.self) { t in
                        Button {
                            draft.schedule.times.removeAll { $0 == t }
                        } label: {
                            HStack(spacing: 4) {
                                Text(t)
                                Image(systemName: "xmark").font(.system(size: 9, weight: .semibold)).foregroundStyle(P.ink3)
                            }
                            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                            .foregroundStyle(P.ink)
                            .padding(.horizontal, Tokens.Space.m)
                            .frame(minHeight: Tokens.Control.chip)
                            .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("\(t), take it off")
                    }
                }
            }
        }
        .padding(.vertical, Tokens.Space.xs)
    }
}

/// A routine's runs (PLAN.md, "The routine's page"): a strip of its last 20, each coloured
/// by how it ended, over the runs, newest first: when, how it ended, its line and how long
/// it took. A tap opens one to what it posted, why it failed and what to do, or its answer.
private struct RunHistory: View {
    var runs: [RoutineRun]
    @State private var open: String?

    var body: some View {
        HStack(alignment: .bottom, spacing: 3) {
            ForEach(runs.prefix(20).reversed()) { r in
                RoundedRectangle(cornerRadius: 3).fill(tint(r.mark)).frame(width: 9, height: 22)
                    .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(open == r.run ? P.ink : Color.clear, lineWidth: 2))
                    .onTapGesture { open = open == r.run ? nil : r.run }
            }
        }
        .padding(.vertical, Tokens.Space.xs)
        .accessibilityHidden(true)
        ForEach(runs) { r in
            Button {
                open = open == r.run ? nil : r.run
            } label: {
                VStack(alignment: .leading, spacing: Tokens.Space.s) {
                    HStack(spacing: Tokens.Space.s) {
                        Text(When.time(r.at)).font(Fonts.caption).foregroundStyle(P.ink3).frame(width: 76, alignment: .leading)
                        Text(r.pill)
                            .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                            .foregroundStyle(tint(r.mark))
                            .padding(.horizontal, 6).padding(.vertical, 1)
                            .background(tint(r.mark).opacity(0.16), in: RoundedRectangle(cornerRadius: Tokens.Radius.row))
                        Text(r.line).font(Fonts.small).foregroundStyle(P.ink2).lineLimit(1)
                        Spacer(minLength: 0)
                        Text(r.took).font(Fonts.caption).foregroundStyle(P.ink3)
                    }
                    if open == r.run { detail(r).padding(.leading, 76 + Tokens.Space.s) }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    /// An open run: what it posted and what its agent answered, or why it failed or was
    /// skipped, and what to do.
    @ViewBuilder
    private func detail(_ r: RoutineRun) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            ForEach(Array(r.posted.enumerated()), id: \.offset) { _, text in part("Posted", text, P.ink) }
            if !r.error.isEmpty { part("Why", r.error, r.mark == .failed ? P.bad : P.ink) }
            if !r.hint.isEmpty { part("What to do", r.hint, P.ink) }
            let said = r.said.trimmingCharacters(in: .whitespacesAndNewlines)
            if !said.isEmpty && !r.posted.contains(where: { $0.trimmingCharacters(in: .whitespacesAndNewlines) == said }) {
                part("Its answer", said, P.ink2)
            }
        }
    }

    private func part(_ label: String, _ text: String, _ ink: Color) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(Fonts.ui(Tokens.PhoneSize.caption, .medium)).foregroundStyle(P.ink3)
            Text(text).font(Fonts.small).foregroundStyle(ink).textSelection(.enabled)
        }
    }

    private func tint(_ mark: RunMark) -> Color {
        switch mark {
        case .posted: P.ok
        case .quiet: P.ink3
        case .failed: P.bad
        case .skipped: P.warn
        case .going: P.button
        }
    }
}

/// "HH:MM" as today's date at that time, for a clock to pick from; noon for one empty.
private func dateOf(_ hhmm: String) -> Date {
    let parts = hhmm.split(separator: ":").compactMap { Int($0) }
    let (h, m) = parts.count == 2 ? (parts[0], parts[1]) : (12, 0)
    return Calendar.current.date(bySettingHour: h, minute: m, second: 0, of: .now) ?? .now
}

/// A clock's time as a schedule keeps it: "08:05".
private func hhmm(_ d: Date) -> String {
    let c = Calendar.current.dateComponents([.hour, .minute], from: d)
    return String(format: "%02d:%02d", c.hour ?? 0, c.minute ?? 0)
}

#Preview("Routines") {
    NavigationStack { RoutinesView() }.environment(Session(sample: true))
}
