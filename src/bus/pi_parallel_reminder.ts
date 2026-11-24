// The owner keeps asking for parallel workers. Remind the
// model once per session, at the moment it commits to work: an approved plan or the first edit/write.
// The message is hidden from the TUI. Worker seats are leaves and skip it.
// A native Pi child has no seat id; `swarm` leaves this file out of its extension list.
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const REMINDER =
	"Split independent units into parallel workers, one concern each. Owner: ~/.swarm/kit/references/fan-out.md";

const isWorker = () => Boolean(process.env.SWARM_AGENT_ID) && process.env.SWARM_AGENT_ID !== "orchestrator";

export default function (pi: ExtensionAPI) {
	if (isWorker()) return;
	let sent = false;
	const remind = () => {
		if (sent) return;
		sent = true;
		pi.sendMessage({ customType: "parallel-reminder", content: REMINDER, display: false }, { deliverAs: "steer" });
	};

	pi.on("tool_call", (event) => {
		if (event.toolName === "edit" || event.toolName === "write") remind();
	});
	pi.on("tool_result", (event) => {
		if (event.toolName !== "approve_plan" || event.isError) return;
		if ((event.details as { approved?: boolean } | undefined)?.approved === true) remind();
	});
}
