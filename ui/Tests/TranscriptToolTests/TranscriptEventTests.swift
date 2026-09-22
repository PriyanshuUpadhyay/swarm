import Foundation
import Testing
import TranscriptTool

@Suite("TranscriptEvent decoding suite")
struct TranscriptEventTests {
    @Test("Decodes page boundary event")
    func pageEvent() {
        let fixture = #"{"type":"page","start_offset":100,"end_offset":200}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .page(let start, let end) = event else {
            Issue.record("Expected .page event")
            return
        }
        #expect(start == 100)
        #expect(end == 200)
    }

    @Test("Decodes ignored event")
    func ignoredEvent() {
        let fixture = #"{"type":"ignored","kind":"mode","meta":{"session_id":"s1","uuid":"u1","timestamp":"2026-09-22T00:00:00Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .ignored(let kind, let meta) = event else {
            Issue.record("Expected .ignored event")
            return
        }
        #expect(kind == "mode")
        #expect(meta.sessionID == "s1")
        #expect(meta.uuid == "u1")
    }

    @Test("Decodes turn started event")
    func turnStartedEvent() {
        let fixture = #"{"type":"turn_started","meta":{"session_id":"s1","uuid":"u2","timestamp":"2026-09-22T00:00:01Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .turnStarted(let meta) = event else {
            Issue.record("Expected .turnStarted event")
            return
        }
        #expect(meta.uuid == "u2")
    }

    @Test("Decodes turn ended event")
    func turnEndedEvent() {
        let fixture = #"{"type":"turn_ended","duration_ms":42,"reason":"completed","meta":{"session_id":"s1","uuid":"u3","timestamp":"2026-09-22T00:00:02Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .turnEnded(let duration, let reason, _) = event else {
            Issue.record("Expected .turnEnded event")
            return
        }
        #expect(duration == 42)
        #expect(reason == .completed)
    }

    @Test("Decodes error event")
    func errorEvent() {
        let fixture = #"{"type":"error","message":"agent failed","meta":{"session_id":"s1","uuid":"u4","timestamp":"2026-09-22T00:00:03Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .error(let message, _) = event else {
            Issue.record("Expected .error event")
            return
        }
        #expect(message == "agent failed")
    }

    @Test("Decodes system message event")
    func systemMessageEvent() {
        let fixture = #"{"type":"system_message","kind":"compaction","text":"summary text","meta":{"session_id":"s1","uuid":"u5","timestamp":"2026-09-22T00:00:04Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .systemMessage(let kind, let text, _) = event else {
            Issue.record("Expected .systemMessage event")
            return
        }
        #expect(kind == "compaction")
        #expect(text == "summary text")
    }

    @Test("Decodes session info event")
    func sessionInfoEvent() {
        let fixture = #"{"type":"session_info","kind":"model","value":"claude-3-opus","meta":{"session_id":"s1","uuid":"u6","timestamp":"2026-09-22T00:00:05Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .sessionInfo(let kind, let value, _) = event else {
            Issue.record("Expected .sessionInfo event")
            return
        }
        #expect(kind == "model")
        #expect(value == "claude-3-opus")
    }

    @Test("Decodes image event")
    func imageEvent() {
        let fixture = #"{"type":"image","role":"user","media_type":"image/png","meta":{"session_id":"s1","uuid":"u7","timestamp":"2026-09-22T00:00:06Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .image(let role, let mediaType, _) = event else {
            Issue.record("Expected .image event")
            return
        }
        #expect(role == "user")
        #expect(mediaType == "image/png")
    }

    @Test("Decodes user message chunk")
    func userMessageChunkEvent() {
        let fixture = #"{"type":"user_message_chunk","text":"Build the project","meta":{"session_id":"s1","uuid":"u8","timestamp":"2026-09-22T00:00:07Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .userMessageChunk(let text, _) = event else {
            Issue.record("Expected .userMessageChunk event")
            return
        }
        #expect(text == "Build the project")
    }

    @Test("Decodes agent message chunk")
    func agentMessageChunkEvent() {
        let fixture = #"{"type":"agent_message_chunk","text":"Working on it now.","meta":{"session_id":"s1","uuid":"u9","timestamp":"2026-09-22T00:00:08Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .agentMessageChunk(let text, let meta) = event else {
            Issue.record("Expected .agentMessageChunk event")
            return
        }
        #expect(text == "Working on it now.")
        #expect(meta.uuid == "u9")
    }

    @Test("Decodes agent thought chunk")
    func agentThoughtChunkEvent() {
        let fixture = #"{"type":"agent_thought_chunk","text":"Inspecting compiler output","meta":{"session_id":"s1","uuid":"u10","timestamp":"2026-09-22T00:00:09Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .agentThoughtChunk(let text, _) = event else {
            Issue.record("Expected .agentThoughtChunk event")
            return
        }
        #expect(text == "Inspecting compiler output")
    }

    @Test("Decodes tool call event")
    func toolCallEvent() {
        let fixture = #"{"type":"tool_call","tool_call_id":"call-1","name":"Bash","input":{"command":"make build"},"status":"pending","meta":{"session_id":"s1","uuid":"u11","timestamp":"2026-09-22T00:00:10Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .toolCall(let toolCallID, let name, _, let status, _) = event else {
            Issue.record("Expected .toolCall event")
            return
        }
        #expect(toolCallID == "call-1")
        #expect(name == "Bash")
        #expect(status == .pending)
    }

    @Test("Decodes tool call update event")
    func toolCallUpdateEvent() {
        let fixture = #"{"type":"tool_call_update","tool_call_id":"call-1","status":"completed","content":"build succeeded","meta":{"session_id":"s1","uuid":"u12","timestamp":"2026-09-22T00:00:11Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .toolCallUpdate(let toolCallID, let status, let content, _) = event else {
            Issue.record("Expected .toolCallUpdate event")
            return
        }
        #expect(toolCallID == "call-1")
        #expect(status == .completed)
        #expect(content == "build succeeded")
    }

    @Test("Decodes elicitation event")
    func elicitationEvent() {
        let fixture = #"{"type":"elicitation","tool_call_id":"call-2","questions":[{"question":"Pick database","header":"Database","multi_select":false,"options":[{"label":"SQLite","description":"Local"}]}],"meta":{"session_id":"s1","uuid":"u13","timestamp":"2026-09-22T00:00:12Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .elicitation(let toolCallID, let questions, _) = event else {
            Issue.record("Expected .elicitation event")
            return
        }
        #expect(toolCallID == "call-2")
        #expect(questions.count == 1)
        #expect(questions.first?.question == "Pick database")
        #expect(questions.first?.options.first?.label == "SQLite")
    }

    @Test("Decodes elicitation result event")
    func elicitationResultEvent() {
        let fixture = #"{"type":"elicitation_result","tool_call_id":"call-2","answers":[{"question":"Pick database","answer":"SQLite"}],"meta":{"session_id":"s1","uuid":"u14","timestamp":"2026-09-22T00:00:13Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .elicitationResult(let toolCallID, let answers, _) = event else {
            Issue.record("Expected .elicitationResult event")
            return
        }
        #expect(toolCallID == "call-2")
        #expect(answers.first?.answer == "SQLite")
    }

    @Test("Decodes hook result event")
    func hookResultEvent() {
        let fixture = #"{"type":"hook_result","kind":"hook_success","hook_event":"Stop","hook_name":"cleanup","tool_call_id":"call-3","exit_code":0,"meta":{"session_id":"s1","uuid":"u15","timestamp":"2026-09-22T00:00:14Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .hookResult(let kind, let hookEvent, let hookName, let toolCallID, let exitCode, _) = event else {
            Issue.record("Expected .hookResult event")
            return
        }
        #expect(kind == "hook_success")
        #expect(hookEvent == "Stop")
        #expect(hookName == "cleanup")
        #expect(toolCallID == "call-3")
        #expect(exitCode == 0)
    }

    @Test("Decodes permission decision event")
    func permissionDecisionEvent() {
        let fixture = #"{"type":"permission_decision","hook_event":"can_use_tool","tool_call_id":"call-4","decision":"allow","meta":{"session_id":"s1","uuid":"u16","timestamp":"2026-09-22T00:00:15Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .permissionDecision(let hookEvent, let toolCallID, let decision, _) = event else {
            Issue.record("Expected .permissionDecision event")
            return
        }
        #expect(hookEvent == "can_use_tool")
        #expect(toolCallID == "call-4")
        #expect(decision == "allow")
    }

    @Test("Decodes unknown event")
    func unknownEvent() {
        let fixture = #"{"type":"unknown","raw":"{\"key\":\"val\"}","meta":{"session_id":"s1","uuid":"u17","timestamp":"2026-09-22T00:00:16Z"}}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .unknown(let raw, _) = event else {
            Issue.record("Expected .unknown event")
            return
        }
        #expect(raw.contains("val"))
    }

    @Test("Unknown event type decodes as unknown case without error")
    func unknownTypeDecodesAsUnknown() {
        let fixture = #"{"type":"future_unrecognised_type","details":"novel feature"}"#
        let event = TranscriptEvent.decode(line: fixture)
        guard case .unknown(let raw, _) = event else {
            Issue.record("Expected .unknown event for future unrecognized type")
            return
        }
        #expect(!raw.isEmpty)
    }

    @Test("Non-JSON line decodes as unknown case")
    func nonJsonLineDecodesAsUnknown() {
        let rawLine = "Plain non-JSON stderr notice"
        let event = TranscriptEvent.decode(line: rawLine)
        guard case .unknown(let raw, _) = event else {
            Issue.record("Expected .unknown event for non-JSON line")
            return
        }
        #expect(raw == rawLine)
    }
}
