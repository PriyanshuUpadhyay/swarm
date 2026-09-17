import Testing
import Foundation
@testable import BloomCore

@Suite("Onboarding flow")
struct OnboardingFlowTests {
    @Test("A greeting, the checks, two offers that are usually not there, and a postcard")
    func order() {
        #expect(
            OnboardingStep.order
                == [.greeting, .checks, .keepAwake, .commandLine, .postcard]
        )
        #expect(!OnboardingStep.greeting.isOptional)
        #expect(!OnboardingStep.checks.isOptional)
        #expect(OnboardingStep.commandLine.isOptional)
        // A Mac with no lid has nothing to approve, so the step is not in its sequence at all.
        #expect(OnboardingStep.keepAwake.isOptional)
        // There is no configuration that could make an address empty and no build that cannot
        // show it.
        #expect(!OnboardingStep.postcard.isOptional)

        let plain = OnboardingFlow(step: .greeting)
        #expect(plain.steps == [.greeting, .checks, .postcard])
        #expect(plain.next == .checks)

        let lid = OnboardingFlow(step: .greeting, offersKeepAwake: true)
        #expect(lid.steps == [.greeting, .checks, .keepAwake, .postcard])

        let offered = OnboardingFlow(step: .greeting, offersCommandLine: true)
        #expect(
            offered.steps == [.greeting, .checks, .commandLine, .postcard]
        )
    }

    /// The owner's own argument for the position, kept as an assertion so a later step cannot be
    /// appended past it without somebody deciding to: this is the only screen in the sequence that
    /// asks for nothing, and a wizard that ends on a warm note ends better than one that ends on a
    /// form.
    @Test("The postcard is last, whether the command line offer is in the sequence or not")
    func thePostcardIsLast() {
        #expect(OnboardingStep.order.last == .postcard)
        #expect(OnboardingFlow(step: .greeting).steps.last == .postcard)
        #expect(OnboardingFlow(step: .greeting, offersCommandLine: true).steps.last == .postcard)
        #expect(OnboardingFlow(step: .postcard).next == nil)
    }

    @Test("Without the command line offer the checks lead straight to the postcard")
    func checksWithoutTheOffer() {
        var flow = OnboardingFlow(step: .checks)
        #expect(flow.next == .postcard)
        let moved = flow.advance()
        #expect(moved)
        #expect(flow.step == .postcard)
    }

    @Test("With the offer the checks lead to it, and the postcard is still the end")
    func endsAtThePostcard() {
        var flow = OnboardingFlow(step: .checks, offersCommandLine: true)
        #expect(flow.next == .commandLine)
        let moved = flow.advance()
        #expect(moved)
        #expect(flow.step == .commandLine)
        #expect(flow.next == .postcard)
        let movedAgain = flow.advance()
        #expect(movedAgain)
        #expect(flow.step == .postcard)
        #expect(flow.next == nil)
        let refused = flow.advance()
        #expect(!refused)
    }

    @Test("Back exists from the checks and nowhere else, and lands on whatever screen came before")
    func back() {
        let greeting = OnboardingFlow(step: .greeting)
        #expect(greeting.back == nil)
        #expect(!greeting.canGoBack)
        #expect(greeting.backButtonTitle == nil)

        let checks = OnboardingFlow(step: .checks)
        #expect(checks.back == .greeting)
        #expect(checks.canGoBack)
        #expect(checks.backButtonTitle != nil)

        let offer = OnboardingFlow(step: .commandLine, offersCommandLine: true)
        #expect(offer.back == .checks)

        // The last screen goes back to the one before it, whichever of them that is.
        #expect(OnboardingFlow(step: .postcard).back == .checks)
        #expect(OnboardingFlow(step: .postcard, offersCommandLine: true).back == .commandLine)
    }

    @Test("The step somebody is standing on stays in the sequence when the offer is withdrawn")
    func standingOnAWithdrawnStep() {
        var flow = OnboardingFlow(step: .checks, offersCommandLine: true)
        let moved = flow.advance()
        #expect(moved)
        flow.offerCommandLine(false)
        #expect(flow.step == .commandLine)
        #expect(flow.steps.contains(.commandLine))
        // And back still lands somewhere real rather than on the first step of a list this one
        // fell out of.
        #expect(flow.back == .checks)
        // Forward still lands somewhere real too, which is the half that only matters now the
        // withdrawn step is no longer the last one.
        #expect(flow.next == .postcard)
    }

    @Test("The forward button moves on rather than naming a decision")
    func titles() {
        #expect(OnboardingStep.greeting.isArrivedAt == false)
        #expect(OnboardingFlow(step: .greeting).forwardButtonTitle == "Get started")
        for step in [OnboardingStep.checks, .keepAwake, .commandLine] {
            #expect(
                OnboardingFlow(step: step, offersCommandLine: true, offersKeepAwake: true)
                    .forwardButtonTitle == "Continue"
            )
        }
        #expect(OnboardingFlow(step: .postcard).forwardButtonTitle == nil)
    }

    @Test("Progress counts only the screens this Mac is shown")
    func progress() {
        let everything = OnboardingFlow(step: .keepAwake, offersCommandLine: true, offersKeepAwake: true)
        #expect(everything.progress.position == 3)
        #expect(everything.progress.count == 5)

        let lean = OnboardingFlow(step: .postcard)
        #expect(lean.progress.position == 3)
        #expect(lean.progress.count == 3)
    }

    @Test("A first run opens on the greeting, and every other reason opens on the checks")
    func opening() {
        #expect(OnboardingFlow.firstStep(trigger: .firstRun) == .greeting)
        #expect(OnboardingFlow.firstStep(trigger: .blocked) == .checks)
        #expect(OnboardingFlow.firstStep(trigger: .none) == .checks)
    }

    @Test("Advancing walks the whole sequence and then refuses")
    func advancing() {
        var flow = OnboardingFlow(step: .greeting)
        let moved = flow.advance()
        #expect(moved)
        #expect(flow.step == .checks)
        let movedAgain = flow.advance()
        #expect(movedAgain)
        #expect(flow.step == .postcard)
        let refused = flow.advance()
        #expect(!refused)
        #expect(flow.step == .postcard)
    }

    @Test("Going back walks to the greeting and then refuses")
    func goingBack() {
        var flow = OnboardingFlow(step: .checks)
        #expect(flow.canGoBack)
        let wentBack = flow.goBack()
        #expect(wentBack)
        #expect(flow.step == .greeting)
        #expect(!flow.canGoBack)
        let wentBackAgain = flow.goBack()
        #expect(!wentBackAgain)
        #expect(flow.step == .greeting)
    }

    @Test("Back is offered even when the window opened straight onto the checks")
    func backFromAReturningOpen() {
        var flow = OnboardingFlow.opening(trigger: .none)
        #expect(flow.step == .checks)
        #expect(flow.canGoBack)
        let wentBack = flow.goBack()
        #expect(wentBack)
        #expect(flow.step == .greeting)
    }

    @Test("The entrance plays once, and a step returned to is a return rather than an arrival")
    func firstVisit() {
        var flow = OnboardingFlow(step: .greeting)
        #expect(flow.isFirstVisit(to: .greeting))
        #expect(flow.isFirstVisit(to: .checks))
        _ = flow.advance()
        #expect(flow.isFirstVisit(to: .checks))
        _ = flow.goBack()
        #expect(!flow.isFirstVisit(to: .greeting))
        _ = flow.advance()
        #expect(!flow.isFirstVisit(to: .checks))
    }
}

@Suite("The welcome window's primary button")
struct OnboardingPrimaryTests {
    @Test("A blocked machine is offered another look rather than a closed door")
    func blocked() {
        let primary = OnboardingPrimary(step: .checks, verdict: .blocked, next: .postcard)
        #expect(primary.action == .checkAgain)
        #expect(primary.title == "Check again")
    }

    @Test("A step with somewhere to go goes there")
    func advancing() {
        let toChecks = OnboardingPrimary(step: .greeting, verdict: .checking, next: .checks)
        #expect(toChecks.action == .advance(.checks))
        #expect(toChecks.title == "Get started")

        let toOffer = OnboardingPrimary(step: .checks, verdict: .ready, next: .commandLine)
        #expect(toOffer.action == .advance(.commandLine))
        #expect(toOffer.title == "Continue")

        // The command line screen leads somewhere now, so its own button moves rather than leaves.
        let toPostcard = OnboardingPrimary(
            step: .commandLine, verdict: .ready, next: .postcard
        )
        #expect(toPostcard.action == .advance(.postcard))
        #expect(toPostcard.title == "Continue")
    }

    @Test("The last step's button leaves, whatever the verdict is doing behind it")
    func finishing() {
        for verdict in [SetupVerdict.checking, .ready, .readyWithNotes, .blocked] {
            let primary = OnboardingPrimary(step: .postcard, verdict: verdict, next: nil)
            #expect(primary.action == .finish)
            #expect(primary.title == OnboardingPrimary.finishTitle)
        }
    }

    /// The screen that asks for nothing must not grow a button that does. Its footer moves the
    /// sequence out and nothing else, whatever the copy button on the screen itself is doing.
    @Test("The postcard step's own button only leaves")
    func thePostcardOnlyLeaves() {
        let primary = OnboardingPrimary(step: .postcard, verdict: .ready, next: nil)
        #expect(primary.action == .finish)
        #expect(primary.title == OnboardingPrimary.finishTitle)
    }

    @Test("Re-probing to blocked past the checks does not turn the way out into a re-check")
    func blockedPastTheChecks() {
        // The column is not on screen there, so Check again would be a button about a list nobody
        // can see, and it would close nothing.
        let onOffer = OnboardingPrimary(
            step: .commandLine, verdict: .blocked, next: .postcard
        )
        #expect(onOffer.action == .advance(.postcard))

        let onPostcard = OnboardingPrimary(step: .postcard, verdict: .blocked, next: nil)
        #expect(onPostcard.action == .finish)
        #expect(onPostcard.title == OnboardingPrimary.finishTitle)
    }
}
