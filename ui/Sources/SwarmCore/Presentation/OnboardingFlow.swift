import Foundation

/// The steps the welcome window walks through, and the rules for moving between them.
///
/// There are four, and two are not always there. The window used to be one screen that
/// opened straight onto four probes, which meant the first thing a new Swarm ever said to anybody
/// was a list of what their Mac might be missing. That reads as a form. A greeting first, then the
/// checks, is what turns the same two facts into a welcome, and it costs one press.
///
/// **The optional steps are offers rather than stages, and that is the whole reason
/// `OnboardingFlow` owns the sequence rather than this enum.** Each is shown only when there is
/// something to offer, and somebody who accepted an offer last week is not asked to do it again.
public enum OnboardingStep: String, Sendable, Hashable, CaseIterable, Identifiable, Codable {
    /// The mark, the name and one line saying what Swarm is. No information to act on.
    case greeting
    /// What this Mac already has, which is the window's other half.
    case checks
    /// Approving the helper that keeps a laptop awake with its lid shut, which is the one part of
    /// Keep Awake that Swarm cannot do on its own. Optional twice over: only a Mac with a lid is
    /// asked, and only one that has not already approved it. See `OnboardingFlow.steps`.
    ///
    /// **Here rather than in Settings alone, because of what it costs to be found later.** The
    /// approval is a trip to System Settings, and a switch that needs one is a switch people flip,
    /// see nothing happen, and give up on. Asked once, on the screen after the checks, it can be
    /// on by default for the rest of the app's life.
    case keepAwake
    /// The one command that couples the owner's own Claude Code to this Swarm. Optional, and
    /// omitted entirely when there is nothing to offer. See `OnboardingFlow.steps`.
    case commandLine

    public var id: String { rawValue }

    /// Reading order. Which of these a given window actually walks is `OnboardingFlow.steps`,
    /// which is the same list with the optional step taken out when it has nothing to say.
    public static let order: [OnboardingStep] = [
        .greeting, .checks, .keepAwake, .commandLine,
    ]

    /// True of a step the sequence may leave out. Nothing is lost by leaving it out: the offer is
    /// in Settings for the rest of the app's life, which is where somebody goes back for it.
    public var isOptional: Bool { self == .commandLine || self == .keepAwake }

    /// Whether any button moves the sequence onto this screen. The greeting is where it starts, so
    /// nothing does.
    public var isArrivedAt: Bool { self != .greeting }
}

/// Where the window opens, how it moves, and which steps it has at all.
///
/// Two interesting rules. `firstStep` is where it opens: a first run gets the greeting, because
/// that is the whole reason the greeting exists and a warm second is what somebody who has just
/// double clicked a fresh app is owed. The Help menu deliberately replays that first-run sequence.
/// A later broken launch opens on the checks instead. Back is still offered from there, so the
/// greeting is never a screen that has been taken away.
///
/// `steps` is which screens exist for this window at all, and it is a list rather than a constant
/// because each optional offer is only worth a screen when it has something to offer. Every move
/// walks that list, so a step that is not in it is not somewhere back can land either.
public struct OnboardingFlow: Sendable, Hashable {
    public private(set) var step: OnboardingStep
    /// Every step this window has shown, in the order it showed them. What makes back honest when
    /// the sequence did not start at the beginning.
    public private(set) var history: [OnboardingStep]
    /// Whether the optional command line step is part of this window's sequence. Answered by
    /// reading the owner's own configuration, which is a file on disk and therefore an answer that
    /// arrives after the window has opened. See `offerCommandLine`.
    public private(set) var offersCommandLine: Bool
    /// Whether the lid step is part of this window's sequence: a Mac with a lid, whose helper has
    /// not been approved already. Answered by the app, which is the only side that can ask.
    public private(set) var offersKeepAwake: Bool

    public init(
        step: OnboardingStep = .greeting,
        offersCommandLine: Bool = false,
        offersKeepAwake: Bool = false
    ) {
        self.step = step
        self.history = [step]
        self.offersCommandLine = offersCommandLine
        self.offersKeepAwake = offersKeepAwake
    }

    /// Where a window opened by this trigger starts.
    public static func firstStep(trigger: OnboardingTrigger) -> OnboardingStep {
        switch trigger {
        case .firstRun: .greeting
        case .blocked, .none: .checks
        }
    }

    public static func opening(
        trigger: OnboardingTrigger,
        offersCommandLine: Bool = false,
        offersKeepAwake: Bool = false
    ) -> OnboardingFlow {
        OnboardingFlow(
            step: firstStep(trigger: trigger),
            offersCommandLine: offersCommandLine,
            offersKeepAwake: offersKeepAwake
        )
    }

    /// The screens this window walks, in order.
    ///
    /// The step somebody is standing on is always in the list, whatever the offer says. Otherwise
    /// an answer arriving a moment late would take the screen out from under a reader, and the
    /// back control on it would point at a step the list no longer contains.
    public var steps: [OnboardingStep] {
        OnboardingStep.order.filter { !$0.isOptional || isOffered($0) || $0 == step }
    }

    /// Whether one optional step has something to offer on this Mac.
    public func isOffered(_ step: OnboardingStep) -> Bool {
        switch step {
        case .commandLine: offersCommandLine
        case .keepAwake: offersKeepAwake
        default: true
        }
    }

    /// Says whether the command line step is worth showing.
    public mutating func offerCommandLine(_ isOffered: Bool) {
        offersCommandLine = isOffered
    }

    public mutating func offerKeepAwake(_ isOffered: Bool) {
        offersKeepAwake = isOffered
    }

    private var position: Int { steps.firstIndex(of: step) ?? 0 }

    /// The step after this one, or nil at the end of the sequence. Nil is what says the primary
    /// button is the way out rather than the way on.
    public var next: OnboardingStep? {
        let walked = steps
        let after = (walked.firstIndex(of: step) ?? 0) + 1
        return after < walked.count ? walked[after] : nil
    }

    /// The step before this one, or nil at the start.
    ///
    /// Nil is what the view draws nothing for. A back control that is present and disabled on the
    /// first step is a promise of a screen that does not exist.
    public var back: OnboardingStep? {
        let before = position - 1
        return before >= 0 ? steps[before] : nil
    }

    public var canGoBack: Bool { back != nil }

    /// What the button that moves the sequence on says, or nil on a step that has no next.
    ///
    /// "Continue", on every screen. It was a table naming the screen each press opened, "Keep this
    /// Mac awake" and "Use Swarm from your terminal", on the argument that "Continue" tells nobody
    /// what they are about to see. The first person to walk through it on a fresh Mac read those
    /// as the decision rather than as the way on: "I don't want that, so I'll look for something
    /// else to click". A footer button that reads as an imperative, on a screen whose own button
    /// is the offer, is two answers to one question. The screen's content says what it is about,
    /// the dots beside the button say how far along the sequence is, and the button only moves.
    /// The greeting is the one exception, because it has no content for the button to follow on
    /// from: its button starts the sequence rather than continuing one.
    public var forwardButtonTitle: String? {
        guard next != nil else { return nil }
        return step == .greeting ? Self.startTitle : Self.forwardTitle
    }

    public static let forwardTitle = "Continue"
    public static let startTitle = "Get started"

    /// Where this screen sits in the sequence, counted from one, for the dots in the footer.
    ///
    /// Asked of `steps` rather than of `OnboardingStep.order`, so an offer that is not made on this
    /// Mac is not a dot somebody never reaches.
    public var progress: (position: Int, count: Int) { (position + 1, steps.count) }

    /// What the control back to the previous step says.
    ///
    /// One word wherever it appears. It was a table naming the screen it returns to, which never
    /// held more than one entry and which said "Back" in it, and a table of one is a decision
    /// nobody took. What a back control needs to say is where it goes, and on a window this short
    /// where it goes is the screen you were just on.
    public var backButtonTitle: String? { canGoBack ? "Back" : nil }

    /// Moves on, if there is anywhere to move on to. Returns whether anything changed, so a view
    /// only animates a transition that happened.
    @discardableResult
    public mutating func advance() -> Bool {
        guard let next else { return false }
        step = next
        history.append(next)
        return true
    }

    @discardableResult
    public mutating func goBack() -> Bool {
        guard let back else { return false }
        step = back
        history.append(back)
        return true
    }

    /// True the first time this window shows a step, which is the only time its entrance should
    /// play. Coming back to the greeting from the checks is a return, not an arrival, and
    /// replaying the whole opening sequence on a return is how a nice moment becomes a wait.
    public func isFirstVisit(to step: OnboardingStep) -> Bool {
        history.filter { $0 == step }.count <= 1
    }
}

// MARK: - The one button that is always on screen

/// What the welcome window's primary button says and does, on whichever step is up.
///
/// A decision rather than a drawing, and it is here because it is the one control the window
/// cannot get wrong twice: it is enabled on every step, it carries the return key, and what it
/// does changes with both the step and the machine. It was an `if` inside the footer, reading a
/// verdict, on a window that then grew a step the verdict says nothing about.
///
/// The rules, in the order they are asked:
///
/// - A blocked machine is offered another look rather than a closed door, and only on the screen
///   that is showing it what is wrong.
/// - A step with somewhere to go is a step whose button goes there.
/// - Otherwise the button leaves, which is what the last step's button always does. It is never
///   disabled and it never waits for a probe: a machine that has already answered can be left the
///   instant its owner wants to leave.
public struct OnboardingPrimary: Sendable, Hashable {
    public enum Action: Sendable, Hashable {
        /// Run the checks again, staying where we are.
        case checkAgain
        /// Show the next screen.
        case advance(OnboardingStep)
        /// Close the window and remember that this was finished.
        case finish
    }

    public let action: Action
    public let title: String

    /// The words on the button that ends the sequence.
    ///
    /// Named here rather than only in the verdict's table because the screen that ends the
    /// sequence is one no verdict has an opinion about.
    public static let finishTitle = "Start using Swarm"

    public init(step: OnboardingStep, verdict: SetupVerdict, next: OnboardingStep?) {
        // Blocked is asked about only on the screen carrying the column. A re-probe that turns
        // blocked while somebody is reading the offer would otherwise replace their way out with
        // a Check again for a list that is not in front of them.
        if step == .checks, verdict == .blocked {
            self.action = .checkAgain
            self.title = verdict.primaryButtonTitle
        } else if let next, next.isArrivedAt {
            self.action = .advance(next)
            self.title = step == .greeting ? OnboardingFlow.startTitle : OnboardingFlow.forwardTitle
        } else {
            self.action = .finish
            self.title = Self.finishTitle
        }
    }
}
