import Foundation
import Testing
@testable import SwarmCore

/// What the login shell's PATH is read out of, and what happens to it afterwards.
///
/// The probe itself is not asserted on here. It starts the machine's own shell, which means the
/// answer is the author's `.zshrc` and the suite would be testing a dotfile. What can be pinned
/// down is everything either side of it: the parse of `env -0` output, including the shapes an
/// interactive shell actually produces, and the order the merged PATH comes out in.
@Suite struct LoginShellPathTests {
    private func dump(_ records: [String], noise: String = "") -> Data {
        Data((noise + records.joined(separator: "\0")).utf8)
    }

    @Test func readsThePathRecord() {
        let data = dump(["HOME=/Users/x", "PATH=/opt/homebrew/bin:/usr/bin", "SHELL=/bin/zsh"])
        #expect(LoginShellPath.directories(inEnvironmentDump: data) == ["/opt/homebrew/bin", "/usr/bin"])
    }

    @Test func answersNothingWhenThereIsNoPath() {
        #expect(LoginShellPath.directories(inEnvironmentDump: dump(["HOME=/Users/x"])).isEmpty)
        #expect(LoginShellPath.directories(inEnvironmentDump: Data()).isEmpty)
    }

    /// A shell that printed a banner with no trailing newline glues it to the front of the first
    /// record. powerlevel10k's instant prompt and every "a new release is available" notice do
    /// exactly this, and the record is still a PATH.
    @Test func survivesABannerGluedToTheFirstRecord() {
        let data = dump(["PATH=/opt/homebrew/bin"], noise: "nvm is not compatible with npm config\n")
        #expect(LoginShellPath.directories(inEnvironmentDump: data) == ["/opt/homebrew/bin"])
    }

    /// The reason for `-0` rather than plain `env`: a value with a newline in it would otherwise
    /// be read as the start of the next variable, and the one after it could be PATH.
    @Test func aNewlineInsideAnotherValueIsNotARecordBoundary() {
        let data = dump(["LS_COLORS=one\ntwo", "PATH=/usr/bin"])
        #expect(LoginShellPath.directories(inEnvironmentDump: data) == ["/usr/bin"])
    }

    /// A relative entry resolved against Swarm's working directory is a different directory from
    /// the one the user meant, which is the disagreement this whole type exists to end.
    @Test func keepsOnlyAbsoluteEntries() {
        let data = dump(["PATH=/usr/bin:node_modules/.bin::.:/bin"])
        #expect(LoginShellPath.directories(inEnvironmentDump: data) == ["/usr/bin", "/bin"])
    }

    @Test func dropsDuplicateEntries() {
        let data = dump(["PATH=/usr/bin:/bin:/usr/bin"])
        #expect(LoginShellPath.directories(inEnvironmentDump: data) == ["/usr/bin", "/bin"])
    }

    /// Most trusted first, and each directory once. A user who put a shim ahead of homebrew in
    /// their own shell meant it, so the guessed list must not be able to reorder them.
    @Test func mergePutsTheLoginShellFirstAndDedupes() {
        let merged = LoginShellPath.merge(
            discovered: ["/Users/x/.rbenv/shims", "/opt/homebrew/bin"],
            inherited: ["/usr/bin", "/bin"],
            guessed: ["/opt/homebrew/bin", "/usr/bin", "/sbin"]
        )
        #expect(merged == ["/Users/x/.rbenv/shims", "/opt/homebrew/bin", "/usr/bin", "/bin", "/sbin"])
    }

    @Test func mergeWithoutAnAnswerIsWhatSwarmAlreadyHad() {
        let merged = LoginShellPath.merge(
            discovered: [], inherited: ["/usr/bin"], guessed: ["/opt/homebrew/bin", "/usr/bin"]
        )
        #expect(merged == ["/usr/bin", "/opt/homebrew/bin"])
    }

    /// The escape hatch `Tools/test-core.sh` sets, and the answer for a machine whose startup
    /// files cannot be run headless. No process is started at all.
    @Test func theProbeCanBeTurnedOff() async {
        let found = await LoginShellPath.discover(
            shell: "/bin/zsh", environment: ["SWARM_UI_LOGIN_SHELL_PATH": "0"]
        )
        #expect(found.isEmpty)
    }

    /// A shell that is not there exits non-zero or fails to start, and either way the answer is
    /// empty rather than a crash or a wait.
    @Test func aShellThatIsNotThereAnswersNothing() async {
        let found = await LoginShellPath.discover(
            shell: "/nonexistent/shell", environment: [:]
        )
        #expect(found.isEmpty)
    }

    /// Nothing learned means nothing changes, which is what lets every failure path above be
    /// silent.
    @Test func adoptingNothingLeavesTheEnvironmentAlone() {
        let before = Shell.environment()["PATH"]
        Shell.adoptLoginShellPath([])
        #expect(Shell.environment()["PATH"] == before)
    }
}
