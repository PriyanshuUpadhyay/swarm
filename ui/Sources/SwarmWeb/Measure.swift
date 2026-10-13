// The measure and smoke modes. The scenarios and metric keys are those of web/perf/run.mjs.
// Keys go through AppKit in this process (`NSWindow.sendEvent`), never through CGEvent.
import AppKit
import WebKit

extension Shell {
    /// Runs `body` as an async function body in the page and returns its value.
    @discardableResult
    func js(_ body: String, _ arguments: [String: Any] = [:]) async throws -> Any? {
        try await webView.callAsyncJavaScript(body, arguments: arguments, in: nil, contentWorld: .page)
    }

    func open(_ query: String = "") async throws {
        // The old page stays current until the new one commits, so its ready flag must not count.
        _ = try? await js("window.__swarmReady = false")
        // The page removes the token from its URL. A load that differs only in the fragment would
        // not reload it, so each load gets a query that the app does not read.
        let load = "\(query.isEmpty ? "?" : "\(query)&")load=\(Date().timeIntervalSince1970)"
        webView.load(URLRequest(url: pageURL(load)))
        for _ in 0..<300 {
            if (try? await js("return window.__swarmReady === true")) as? Bool == true { return }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw ShellError("the page was not ready in 15 s")
    }

    func keyEvent(_ type: NSEvent.EventType, _ characters: String, _ code: UInt16, _ mods: NSEvent.ModifierFlags = []) -> NSEvent {
        NSEvent.keyEvent(
            with: type, location: .zero, modifierFlags: mods, timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, characters: characters,
            charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code)!
    }

    /// One key press through the window, then one round trip so the page has handled it.
    func press(_ characters: String, _ code: UInt16) async throws {
        window.sendEvent(keyEvent(.keyDown, characters, code))
        window.sendEvent(keyEvent(.keyUp, characters, code))
        try await js("return 0")
    }

    func pressJ() async throws { try await press("j", 38) }

    /// Opens the chat at sidebar position `index` by keyboard: Home, then j `index` times, then Enter.
    func openChat(_ index: Int) async throws {
        try await js("document.querySelector('[data-testid=\"sidebar\"]').focus()")
        try await press(String(UnicodeScalar(NSHomeFunctionKey)!), 115)
        for _ in 0..<index { try await pressJ() }
        let before = try await js("return performance.now()") as? Double ?? 0
        try await press("\r", 36)
        // Done when the app records the paint of this open or switch.
        try await js(
            """
            const end = performance.now() + 30000;
            while (!(window.__swarmPerf ?? []).some((r) => r.kind !== "key" && r.start >= t)) {
              if (performance.now() > end) {
                const view = document.querySelector('[data-testid="transcript"]');
                throw new Error("openChat timeout " + JSON.stringify({
                  index, t, now: performance.now(), focus: document.activeElement?.outerHTML.slice(0, 120),
                  selected: document.querySelector('[data-testid="tree-row"][aria-selected="true"]')?.getAttribute("data-id"),
                  open: view?.getAttribute("data-chat"), state: view?.getAttribute("data-state"), text: view?.textContent.slice(0, 200),
                  palette: document.querySelector('[data-testid="palette"]') !== null, last: (window.__swarmPerf ?? []).slice(-4),
                }));
              }
              await new Promise((r) => requestAnimationFrame(r));
            }
            """, ["t": before, "index": index])
    }

    func perf(_ kind: String) async throws -> [Double] {
        try await js("return (window.__swarmPerf ?? []).filter((r) => r.kind === kind).map((r) => r.painted - r.start)", ["kind": kind]) as? [Double] ?? []
    }

    func clearPerf() async throws { try await js("window.__swarmPerf = []") }

    func maxIndex() async throws -> Int {
        try await js("return Math.max(-1, ...[...document.querySelectorAll('[data-testid=\"row\"]')].map((r) => Number(r.getAttribute('data-index'))))") as? Int ?? -1
    }

    /// WebKit throttles a hidden or occluded page. A phase counts only when the page is visible
    /// and requestAnimationFrame runs at 55 Hz or more over 1 s.
    func throttleCheck(_ phase: String) async throws -> [String: Any] {
        let got = try await js(
            """
            let n = 0;
            const end = performance.now() + 1000;
            // A timer ends the sample, because a throttled page may run no frame at all.
            await new Promise((done) => {
              const tick = () => { n++; if (performance.now() < end) requestAnimationFrame(tick); };
              requestAnimationFrame(tick);
              setTimeout(done, 1000);
            });
            return { visibility: document.visibilityState, raf_hz: n };
            """) as? [String: Any] ?? [:]
        let hz = got["raf_hz"] as? Double ?? 0
        let visible = got["visibility"] as? String == "visible"
        var load = 0.0
        getloadavg(&load, 1)
        // `occluded` is AppKit's own view of the window; `load1` is the 1-min load average of the Mac.
        return [
            "phase": phase, "visibility": got["visibility"] ?? "unknown", "raf_hz": hz, "valid": visible && hz >= 55,
            "occluded": !window.occlusionState.contains(.visible), "load1": (load * 100).rounded() / 100,
        ]
    }

    /// Physical footprint of the WebContent process that serves this web view, in MB.
    func webContentMB() -> Double? {
        // No public API names this process. This key is WebKit SPI, used by the measure mode only.
        guard let pid = webView.value(forKey: "_webProcessIdentifier") as? Int32, pid > 0 else { return nil }
        var info = rusage_info_v4()
        let status = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V4, $0) }
        }
        return status == 0 ? Double(info.ri_phys_footprint) / 1e6 : nil
    }

    func measure() async throws {
        let out = options["measure"]!
        let liveSeconds = Int(options["live-seconds"] ?? "60") ?? 60
        let soakMinutes = Double(options["soak-minutes"] ?? "10") ?? 10
        // App Nap and display sleep would both throttle the page in a long run.
        let activity = ProcessInfo.processInfo.beginActivity(
            options: [.userInitiated, .latencyCritical, .idleDisplaySleepDisabled], reason: "SwarmWeb measure")
        defer { ProcessInfo.processInfo.endActivity(activity) }
        // Above normal windows, so no other window occludes the page. This does not activate the app.
        window.level = .floating

        var result: [String: Any] = [
            "browser": "wkwebview", "app": options["app"] ?? "react", "heap_growth": NSNull(),
            "window": ["width": webView.bounds.width, "height": webView.bounds.height],
        ]
        var checks: [[String: Any]] = []
        func save() throws {
            result["checks"] = checks
            try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys]).write(to: URL(fileURLWithPath: out))
        }
        func phase(_ name: String) async throws {
            FileHandle.standardError.write(Data("\(Date().formatted(date: .omitted, time: .standard)) \(name)\n".utf8))
            checks.append(try await throttleCheck(name))
            try save()
        }
        func p(_ values: [Double], _ q: Double) -> Any {
            let sorted = values.sorted()
            return sorted.isEmpty ? NSNull() : sorted[min(sorted.count - 1, Int((q * Double(sorted.count)).rounded(.up)) - 1)]
        }
        // Frame gaps from requestAnimationFrame, as in run.mjs: a gap over 25 ms counts as dropped.
        func startFrames() async throws {
            try await js(
                """
                window.__frames = [];
                let last = performance.now();
                const tick = (t) => { window.__frames.push(t - last); last = t; if (window.__frameRun) requestAnimationFrame(tick); };
                window.__frameRun = true;
                requestAnimationFrame(tick);
                """)
        }
        func stopFrames() async throws -> [Double] {
            try await js("window.__frameRun = false; return window.__frames.slice(1)") as? [Double] ?? []
        }

        try await open()
        let chatRows = try await js(
            "return [...document.querySelectorAll('[data-testid=\"tree-row\"]')].map((r, i) => r.getAttribute('data-kind') === 'chat' ? i : -1).filter((i) => i >= 0)"
        ) as? [Int] ?? []
        guard chatRows.count >= 8 else { throw ShellError("the sidebar has \(chatRows.count) chats; the suite needs 8") }

        // Cold opens: cycle through every chat, so the 4-chat LRU always misses.
        try await phase("cold opens")
        try await clearPerf()
        for _ in 0..<3 { for i in chatRows.dropFirst(2) { try await openChat(i) } }
        result["open_p95"] = p(try await perf("open"), 0.95)

        try await phase("warm switches")
        try await openChat(chatRows[2])
        try await openChat(chatRows[3])
        try await clearPerf()
        for k in 0..<30 { try await openChat(chatRows[2 + k % 2]) }
        result["switch_p95"] = p(try await perf("switch"), 0.95)

        // Key-to-paint: j/k selection moves in the sidebar. `key_lag` is the time from the NSEvent
        // timestamp to the page's keydown handler; it shows that the two clocks agree.
        try await phase("key-to-paint")
        try await js(
            """
            document.querySelector('[data-testid="sidebar"]').focus();
            window.__keyLag = [];
            window.__keyLagTap = (e) => window.__keyLag.push(performance.now() - e.timeStamp);
            window.addEventListener("keydown", window.__keyLagTap, true);
            """)
        try await clearPerf()
        for k in 0..<120 {
            try await press(k % 20 < 10 ? "j" : "k", k % 20 < 10 ? 38 : 40)
            try await Task.sleep(for: .milliseconds(20))
        }
        let keys = try await perf("key")
        result["key_p95"] = p(keys, 0.95)
        result["key_p50"] = p(keys, 0.5)
        result["key_records"] = keys.count
        let lag = try await js("window.removeEventListener('keydown', window.__keyLagTap, true); return window.__keyLag") as? [Double] ?? []
        result["key_lag_p50"] = p(lag, 0.5)
        result["key_lag_p95"] = p(lag, 0.95)

        // Scroll the 100k-event chat upward by 600 px per frame until 50 older pages (10k lines)
        // are in. No time cap; it stops early only when no page arrives for 2 min.
        try await openChat(chatRows[0])
        try await phase("scroll")
        let scroll = try await js(
            """
            const el = document.querySelector('[data-testid="transcript"]');
            performance.clearResourceTimings();
            performance.setResourceTimingBufferSize(1000);
            const older = () => performance.getEntriesByType("resource").filter((e) => e.name.includes("/api/transcript") && e.name.includes("before=")).length;
            const gaps = [];
            const start = performance.now();
            let last = start, lastPage = start, pages = 0;
            while (pages * 200 < 10000) {
              const t = await new Promise((r) => requestAnimationFrame(r));
              gaps.push(t - last);
              last = t;
              el.scrollTop = Math.max(0, el.scrollTop - 600);
              const n = older();
              if (n !== pages) { pages = n; lastPage = t; }
              if (t - lastPage > 120000) break;
            }
            gaps.shift();
            return { gaps, pages, seconds: (performance.now() - start) / 1000, rows: document.querySelectorAll('[data-testid="row"]').length };
            """) as? [String: Any] ?? [:]
        let scrollGaps = scroll["gaps"] as? [Double] ?? []
        result["scroll_drop"] = scrollGaps.isEmpty ? NSNull() : Double(scrollGaps.filter { $0 > 25 }.count) / Double(scrollGaps.count)
        result["scroll_frames"] = scrollGaps.count
        result["scroll_worst_ms"] = scrollGaps.max() ?? NSNull()
        result["scroll_older_pages"] = scroll["pages"] ?? NSNull()
        result["scroll_seconds"] = scroll["seconds"] ?? NSNull()
        result["scroll_rows"] = scroll["rows"] ?? NSNull()

        // Giant tool output chat.
        try await phase("giant")
        try await clearPerf()
        for k in 0..<10 {
            try await openChat(chatRows[1])
            try await openChat(chatRows[4 + k % 4])
        }
        result["giant_open_p95"] = p(try await perf("open") + (try await perf("switch")), 0.95)

        // Live load: every chair log grows at 50 lines/s while the page follows all 12 chats.
        if liveSeconds > 0 {
            guard let home = ProcessInfo.processInfo.environment["SWARM_HOME"] else { throw ShellError("the live load needs SWARM_HOME") }
            try await open("?live=all")
            try await openChat(chatRows[2])
            try await phase("live")
            let liveStart = try await maxIndex()
            let gen = Process()
            gen.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            gen.arguments = ["node", options["gen"] ?? "web/perf/gen.mjs", "live", "--home", home, "--rate", "50", "--seconds", String(liveSeconds)]
            try gen.run()
            children.append(gen)
            try await startFrames()
            let end = Date().addingTimeInterval(Double(liveSeconds))
            while Date() < end {
                try await pressJ()
                try await Task.sleep(for: .milliseconds(250))
            }
            let live = try await stopFrames()
            while gen.isRunning { try await Task.sleep(for: .milliseconds(100)) }
            guard gen.terminationStatus == 0 else { throw ShellError("gen.mjs live exited \(gen.terminationStatus)") }
            try await Task.sleep(for: .milliseconds(500))
            result["live_rows_added"] = try await maxIndex() - liveStart
            result["live_drop"] = live.isEmpty ? NSNull() : Double(live.filter { $0 > 25 }.count) / Double(live.count)
            result["live_frames"] = live.count
            result["live_worst_ms"] = live.max() ?? NSNull()
            checks.append(try await throttleCheck("live end"))
        }

        // Memory soak: switch chats for `soakMinutes`. A sample is taken once a minute, after a full
        // cycle of chats, so the 4-chat LRU holds the same chats each time (see run.mjs).
        if soakMinutes > 0 {
            try await open()
            try await phase("soak")
            let start = Date()
            var samples: [[String: Any]] = []
            func sample(_ k: Int) async throws {
                let check = try await throttleCheck("soak sample")
                samples.append([
                    "k": k, "min": Date().timeIntervalSince(start) / 60, "mb": webContentMB() ?? NSNull(),
                    "raf_hz": check["raf_hz"]!, "visibility": check["visibility"]!, "valid": check["valid"]!,
                ])
                result["webcontent_mb"] = samples
                try save()
            }
            // A page that stops painting (hidden, occluded, or a stalled Mac) makes an open time out.
            // The soak records that as a stall and goes on, so one pause does not end a 1 h run.
            var stalls: [[String: Any]] = []
            var k = 0
            while Date().timeIntervalSince(start) < soakMinutes * 60 || k % chatRows.count != 0 {
                do {
                    try await openChat(chatRows[k % chatRows.count])
                } catch {
                    var stall = try await throttleCheck("stall")
                    stall["min"] = Date().timeIntervalSince(start) / 60
                    stall["frontmost"] = NSWorkspace.shared.frontmostApplication?.localizedName ?? "unknown"
                    stall["error"] = String("\(error)".prefix(600))
                    stalls.append(stall)
                    result["soak_stalls"] = stalls
                    try save()
                    guard stalls.count < 20 else { throw error }
                    continue
                }
                k += 1
                if k % chatRows.count == 0 && Date().timeIntervalSince(start) >= Double(samples.count) * 60 { try await sample(k) }
            }
            if samples.last?["k"] as? Int != k { try await sample(k) }
            // The early value is the 10-min sample, or the 20% point of a shorter run.
            let earlyMinute = min(10, soakMinutes * 0.2)
            if let early = samples.first(where: { ($0["min"] as? Double ?? 0) >= earlyMinute })?["mb"] as? Double,
               let late = samples.last?["mb"] as? Double {
                result["webcontent_growth"] = (late - early) / early
                result["webcontent_early_mb"] = early
                result["webcontent_late_mb"] = late
            }
            result["soak_opens"] = k
            result["soak_stalls"] = stalls
            checks.append(["phase": "soak samples", "visibility": samples.allSatisfy { $0["visibility"] as? String == "visible" } ? "visible" : "hidden",
                           "raf_hz": samples.compactMap { $0["raf_hz"] as? Double }.min() ?? 0, "valid": samples.allSatisfy { $0["valid"] as? Bool == true }])
        }
        try save()
    }

    /// Shell smoke: a plain key opens a chat, ⌘2 picks another, and ⌘K opens the palette. Prints
    /// one JSON line with the DOM state, then holds the window for a screenshot by window id.
    func smoke() async throws {
        try await open()
        // Counts the calls that arrive through the menu items.
        try await js("window.__menuCalls = []; const run = window.swarmCommand; window.swarmCommand = (id) => { const ok = run(id); window.__menuCalls.push([id, ok]); return ok; }")
        let firstChat = try await js("return [...document.querySelectorAll('[data-testid=\"tree-row\"]')].findIndex((r) => r.getAttribute('data-kind') === 'chat')") as? Int ?? 0
        try await openChat(firstChat)
        let chatBefore = try await js("return document.querySelector('[data-testid=\"transcript\"]').dataset.chat")
        // AppKit's order for a ⌘ key: the key window's views first, then the main menu.
        func commandKey(_ characters: String, _ code: UInt16) async throws -> String {
            let event = keyEvent(.keyDown, characters, code, .command)
            let by = window.performKeyEquivalent(with: event) ? "view" : NSApp.mainMenu!.performKeyEquivalent(with: event) ? "menu" : "nobody"
            try await Task.sleep(for: .milliseconds(500))
            return by
        }
        let pickedBy = try await commandKey("2", 19)
        let chatAfter = try await js("return document.querySelector('[data-testid=\"transcript\"]').dataset.chat")
        // The menu alone, with no view in the path.
        let menuTook = NSApp.mainMenu!.performKeyEquivalent(with: keyEvent(.keyDown, "k", 40, .command))
        try await Task.sleep(for: .milliseconds(500))
        let paletteOpen = try await js("return document.querySelector('[data-testid=\"palette\"]') !== null")
        let state: [String: Any] = [
            "window_id": window.windowNumber, "sidebar_loaded": try await js("return document.querySelector('[data-testid=\"sidebar\"]').dataset.loaded") ?? NSNull(),
            "tree_rows": try await js("return document.querySelectorAll('[data-testid=\"tree-row\"]').length") ?? NSNull(),
            "chat_before_cmd2": chatBefore ?? NSNull(), "chat_after_cmd2": chatAfter ?? NSNull(), "cmd2_taken_by": pickedBy,
            "cmdK_menu_took": menuTook, "palette_open_after_menu_cmdK": paletteOpen ?? NSNull(),
            "menu_calls": try await js("return window.__menuCalls") ?? NSNull(),
            "visibility": try await js("return document.visibilityState") ?? NSNull(),
            // ADR 0077: a message of the wrong shape gets an error and opens no panel.
            "bad_shell_message": try await js("try { await window.webkit.messageHandlers.swarm.postMessage({kind: 'pickFolder', path: '/etc'}); return 'accepted' } catch (e) { return e.message }") ?? NSNull(),
            "app_active": NSApp.isActive,
        ]
        print(String(decoding: try JSONSerialization.data(withJSONObject: state, options: [.sortedKeys]), as: UTF8.self))
        fflush(stdout)
        try await Task.sleep(for: .seconds(Double(options["hold"] ?? "6") ?? 6))
        let closedBy = try await commandKey("k", 40)
        let paletteAfter = try await js("return document.querySelector('[data-testid=\"palette\"]') !== null")
        print("{\"cmdK_again_taken_by\":\"\(closedBy)\",\"palette_open_after\":\(paletteAfter as? Bool ?? true)}")
    }
}
