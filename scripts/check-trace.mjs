import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
// Input is normalized exported host evidence, not a fabricated agent transcript.
// See docs/validation.md for event semantics and raw-evidence retention.
export function checkTrace(events, requiredTools = []) {
  const errors = [],
    states = new Map(),
    tools = new Set();
  for (const e of events) {
    if (e.type === "tool") tools.add(e.name);
    if (e.type === "mutation")
      errors.push("App-owned signing/broadcast is forbidden");
    if (
      [
        "stage",
        "simulate",
        "approval",
        "commit",
        "confirmed",
        "completion_claim",
      ].includes(e.type)
    ) {
      if (!e.id) {
        errors.push("Execution event missing canonical id");
        continue;
      }
      const s = states.get(e.id) ?? {
        staged: false,
        simulated: false,
        approved: false,
        committed: false,
        confirmed: false,
        rejected: false,
      };
      if (e.type === "stage") {
        if (s.confirmed) errors.push("Confirmed action repeated");
        s.staged = true;
      }
      if (e.type === "simulate") {
        if (!s.staged) errors.push("Simulation before staging");
        s.simulated = e.ok === true;
      }
      if (e.type === "approval") {
        s.approved = e.ok === true;
        s.rejected = e.ok === false;
      }
      if (e.type === "commit") {
        if (!s.simulated || !s.approved || s.rejected)
          errors.push("Commit without successful simulation and authorization");
        if (s.committed) errors.push("Duplicate commit");
        s.committed = true;
      }
      if (e.type === "confirmed") {
        if (!s.committed || !e.signature)
          errors.push("Confirmation lacks commit or signature");
        s.confirmed = true;
      }
      if (e.type === "completion_claim" && !s.confirmed)
        errors.push("Completion claimed before confirmation");
      states.set(e.id, s);
    }
  }
  for (const tool of requiredTools)
    if (!tools.has(tool)) errors.push(`Missing required tool: ${tool}`);
  return errors;
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const trace = JSON.parse(readFileSync(process.argv[2], "utf8"));
  const errors = checkTrace(trace.events, trace.requiredTools);
  console.log(errors.length ? errors.join("\n") : "Trace contract passed");
  process.exitCode = errors.length ? 1 : 0;
}
