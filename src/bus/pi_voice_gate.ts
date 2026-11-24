// Blocks calls that post human-facing text (GitHub, Linear, Slack) until this session has read a
// voice block. The rules live in <swarm root>/kit/blocks/voice*; this file only enforces the read.
// `swarm launch` writes this file to <swarm root>/pi/, so the kit sits one level up.
// Worker seats are leaves that report to the chair and never post, so the gate skips them.
// A native Pi child has no seat id; `swarm` leaves this file out of its extension list.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isToolCallEventType, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

const GH_POST = [
	/\bgh\s+pr\s+(create|comment)\b/,
	/\bgh\s+issue\s+(create|comment)\b/,
	/\bgh\s+(pr|issue)\s+edit\b[\s\S]*(--body|--body-file|-b\b|-F\b|--title|-t\b)/,
	/\bgh\s+pr\s+review\b[\s\S]*(--body|--body-file|-b\b|-F\b)/,
	/\bgh\s+(pr|issue)\s+close\b[\s\S]*(--comment|-c\b)/,
];
const GH_API_GRAPHQL = /\bgh\s+api\s+graphql\b/;
const GH_API_TARGET = /\bgh\s+api\b[\s\S]*\b(comments|reviews|issues|pulls)\b/;
const GH_API_WRITE = /(-X|--method)[\s=]+(POST|PATCH)\b/i;
const GH_API_FIELDS = /\s(-f|-F|--field|--raw-field|--input)\b/;
const GH_API_GET = /(-X|--method)[\s=]+GET\b/i;
const COMPOSIO_WRITE =
	/\bcomposio\s+execute\b[\s\S]*\b(LINEAR_\w*(CREATE|UPDATE|COMMENT)\w*|SLACK_\w*(SEND|POST|UPDATE|SCHEDULE|REPLY)\w*)/;
const COMPOSIO_DRY = /--(get-schema|dry-run)\b/;

function postsHumanText(command: string): boolean {
	if (GH_POST.some((re) => re.test(command))) return true;
	if (GH_API_GRAPHQL.test(command)) return /\bmutation\b/.test(command);
	if (GH_API_TARGET.test(command) && !GH_API_GET.test(command)) {
		if (GH_API_WRITE.test(command) || GH_API_FIELDS.test(command)) return true;
	}
	return COMPOSIO_WRITE.test(command) && !COMPOSIO_DRY.test(command);
}

// The kit block voice-review owns the voice file path, in its first rule: Read `<path>`.
function voiceFilePath(kit: string): string | undefined {
	try {
		const block = readFileSync(join(kit, "blocks/voice-review/SKILL.md"), "utf8");
		const path = /Read `([^`]+)`/.exec(block)?.[1];
		return path?.startsWith("~/") ? join(homedir(), path.slice(2)) : path;
	} catch {
		return undefined;
	}
}

function pathsIn(text: string): string[] {
	const home = homedir();
	return text.split(/[\s'"`;|&<>()]+/).map((t) => resolve(t.replace(/^~(?=\/)/, home)));
}

function isVoiceBlock(path: string, blocks: string): boolean {
	return path.startsWith(blocks + "/") && /^voice[^/]*\/SKILL\.md$/.test(path.slice(blocks.length + 1));
}

export default function (pi: ExtensionAPI) {
	const id = process.env.SWARM_AGENT_ID;
	if (id && id !== "orchestrator") return;

	const kit = resolve(dirname(fileURLToPath(import.meta.url)), "../kit");
	const blocks = join(kit, "blocks");
	const voiceFile = voiceFilePath(kit);
	const reason =
		`Read the voice rules first: ${join(blocks, "voice-pr")} for PR text, voice-review for review comments, voice-reply for replies` +
		(voiceFile ? `, and the voice file ${voiceFile}.` : ".");
	const isVoice = (path: string) => isVoiceBlock(path, blocks) || path === voiceFile;
	let voiceRead = false;

	pi.on("tool_call", async (event) => {
		if (event.toolName === "read") {
			const [path] = pathsIn(String(event.input.path ?? ""));
			if (isVoice(path)) voiceRead = true;
			return;
		}
		if (!isToolCallEventType("bash", event)) return;

		const command = event.input.command;
		if (/\b(cat|bat|head|sed|less)\b/.test(command) && pathsIn(command).some(isVoice)) voiceRead = true;
		if (voiceRead) return;
		if (postsHumanText(command)) return { block: true, reason };
	});
}
