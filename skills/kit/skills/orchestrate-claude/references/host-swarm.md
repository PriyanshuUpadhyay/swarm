# Orchestrate from Claude — Swarm host reference

This reference is active under the `[agent-host: herdr]` host contract.

Follow `skills/swarm-orchestrator/SKILL.md` from the kit root for the session, launch, inbox,
rounds, liveness, and close commands. The kit root is three folders up from this reference's
folder. Apply `blocks/run-workers/SKILL.md` for result acceptance and cleanup requirements.

## Open items

Two open items exist today.
1. No Stop-hook gate covers swarm seats (the old completion gate was removed with herdr-ops).
2. The sandboxed-Codex drop box is not on the swarm path.
