import Foundation
import Testing
@testable import SwarmCore

@Suite("GitHub workspace inspection")
struct GitHubInspectionTests {
    @Test("Fork lookup honors the push remote and checks full head repository identity")
    func forkLookup() async throws {
        let fixture = try await makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let workspace = try await Git.inspect(in: fixture.root.path)
        let result = try await fixture.reader.lookup(in: workspace)
        #expect(result.repositories == ["fork/project", "upstream/project"])
        let match = try #require(result.match)
        #expect(match.repository == "upstream/project")
        #expect(match.pullRequest.number == 7)
        #expect(match.differsFromLocalHead)
        #expect(try await fixture.reader.patch(for: match, in: workspace.root).contains("+server change"))
    }

    @Test("A stable PR head with a changed base rejects the server diff")
    func baseDrift() async throws {
        let fixture = try await makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let workspace = try await Git.inspect(in: fixture.root.path)
        let match = try #require(try await fixture.reader.lookup(in: workspace).match)
        try Data().write(to: fixture.root.appendingPathComponent("change-base"))
        do {
            _ = try await fixture.reader.patch(for: match, in: workspace.root)
            Issue.record("A changed base was accepted")
        } catch {
            #expect(String(describing: error).contains("base or head changed"))
        }
    }

    @Test("No PR differs from network failure and ambiguous matching PRs")
    func noPRAndFailure() async throws {
        let fixture = try await makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let workspace = try await Git.inspect(in: fixture.root.path)
        let marker = fixture.root.appendingPathComponent("mode")
        try "empty".write(to: marker, atomically: true, encoding: .utf8)
        #expect(try await fixture.reader.lookup(in: workspace).match == nil)
        try "failure".write(to: marker, atomically: true, encoding: .utf8)
        await #expect(throws: WorkspaceReadError.self) { try await fixture.reader.lookup(in: workspace) }
        try "ambiguous".write(to: marker, atomically: true, encoding: .utf8)
        await #expect(throws: WorkspaceReadError.self) { try await fixture.reader.lookup(in: workspace) }
        try await Shell.check("git", ["checkout", "--detach", "HEAD"], cwd: fixture.root.path)
        let detached = try await Git.inspect(in: fixture.root.path)
        do {
            _ = try await fixture.reader.lookup(in: detached)
            Issue.record("Detached HEAD was accepted for PR lookup")
        } catch {
            #expect(String(describing: error).contains("detached"))
        }
    }

    @Test("Remote parsing accepts GitHub transports and rejects ambiguous destinations")
    func remoteIdentity() {
        #expect(GitHubRepository(remote: "git@github.com:Owner/repo.git")?.name == "owner/repo")
        #expect(GitHubRepository(remote: "ssh://git@github.com/Owner/repo.git")?.name == "owner/repo")
        #expect(GitHubRepository(remote: "https://github.com/Owner/repo.git")?.name == "owner/repo")
        #expect(GitHubRepository(remote: "https://github.com.attacker.test/Owner/repo.git") == nil)
        #expect(GitHubRepository(remote: "git@personal-alias:Owner/repo.git") == nil)
        #expect(GitHubRepository(name: "owner/../repo") == nil)
    }

    private func makeFixture() async throws -> (root: URL, reader: GitHubInspection) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("gh-inspect-\(UUID().uuidString)")
        try await Shell.check("git", ["init", "-b", "main", root.path])
        try await Shell.check("git", [
            "-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false",
            "commit", "--allow-empty", "-m", "base",
        ], cwd: root.path)
        for args in [
            ["remote", "add", "origin", "https://github.com/fork/project.git"],
            ["remote", "add", "upstream", "https://github.com/upstream/project.git"],
            ["config", "branch.main.remote", "upstream"],
            ["config", "branch.main.pushRemote", "origin"],
        ] { try await Shell.check("git", args, cwd: root.path) }
        let script = root.appendingPathComponent("gh-fixture.py")
        try #"""
        #!/usr/bin/env python3
        import json,sys
        from pathlib import Path
        root=Path(__file__).parent
        args=sys.argv[1:]
        mode=(root/'mode').read_text() if (root/'mode').exists() else ''
        if mode=='failure':
            print('fixture network failure',file=sys.stderr)
            sys.exit(1)
        request={'number':7,'title':'Fixture PR','state':'OPEN','isDraft':False,
          'baseRefName':'main','baseRefOid':'a'*40,'headRefName':'main','headRefOid':'b'*40,
          'headRepository':{'nameWithOwner':'fork/project'},'reviewDecision':'REVIEW_REQUIRED',
          'statusCheckRollup':[{'name':'Tests','status':'COMPLETED','conclusion':'SUCCESS'}],
          'url':'https://github.com/upstream/project/pull/7'}
        if args[:2]==['repo','view']:
            print(json.dumps({'nameWithOwner':'fork/project','parent':{'name':'project','owner':{'login':'upstream'}}}))
        elif args[:2]==['pr','list']:
            assert args[args.index('--head')+1]=='main'
            target=args[args.index('--repo')+1]
            if mode=='empty': print('[]')
            elif target=='github.com/upstream/project':
                if mode=='ambiguous':
                    second=dict(request,number=8,url='https://github.com/upstream/project/pull/8')
                    print(json.dumps([request,second]))
                else: print(json.dumps([request]))
            else:
                other=dict(request,headRepository={'nameWithOwner':'impostor/project'},url='https://github.com/fork/project/pull/7')
                print(json.dumps([other]))
        elif args[:2]==['pr','diff']:
            print('diff --git a/file b/file\n+server change')
        elif args[:2]==['pr','view']:
            counter=root/'views'
            count=int(counter.read_text())+1 if counter.exists() else 1
            counter.write_text(str(count))
            if (root/'change-base').exists() and count>1: request['baseRefOid']='c'*40
            print(json.dumps(request))
        else:
            sys.exit(2)
        """#.write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return (root, GitHubInspection(executable: script.path))
    }
}
