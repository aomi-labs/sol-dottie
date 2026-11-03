import { Aomi, type Event, type Session } from "@aomi-labs/client";

// The client has no default host; this is the same one the widget defaults to.
const DEFAULT_BASE_URL = "https://chat.aomi.dev";

export type Chat = {
  aomi: Aomi;
  applicationId: number;
  session?: Session;
  seen: number;
};

// One client for the whole process, so it signs in as a guest once and every
// session reuses that sign-in.
let client: Aomi | undefined;

export function newChat(): Chat {
  const applicationId = Number(process.env.AOMI_APPLICATION_ID?.trim());
  if (!Number.isSafeInteger(applicationId) || applicationId <= 0) {
    throw new Error("Set AOMI_APPLICATION_ID in client/.env (see .env.example).");
  }
  const baseUrl = process.env.AOMI_BASE_URL?.trim() || DEFAULT_BASE_URL;
  console.log(`host: ${new URL(baseUrl).host}  app: ${applicationId}`);
  client ??= new Aomi({ baseUrl });
  return { aomi: client, applicationId, seen: 0 };
}

/** Send one prompt in direct mode, so this app answers instead of the Auto agent. */
export async function ask(chat: Chat, prompt: string) {
  console.log(`\n> ${prompt}`);
  const run = chat.aomi.agent.run(prompt, {
    sessionId: chat.session?.sessionId,
    target: { mode: "direct", applicationId: chat.applicationId },
  });
  const result = await run;
  chat.session = run.session;

  // A tool call comes back as a `message` event that carries `tool_name` and
  // `tool_result`. Async tools finish later with a `tool_complete` event.
  const events: readonly Event[] = run.session.getSnapshot().events;
  for (const event of events.slice(chat.seen)) {
    if (event.type === "message" && event.tool_name) {
      const args = JSON.stringify(event.tool_arguments ?? {});
      console.log(`  tool ${event.tool_name} ${args} -> ${event.tool_result?.[1] ?? ""}`);
    } else if (event.type === "tool_complete") {
      console.log(`  tool ${event.tool_name} -> ${JSON.stringify(event.result)}`);
    }
  }
  chat.seen = events.length;

  const reply = result.messages.filter((m) => m.sender === "agent").at(-1);
  console.log(reply?.content ?? "(no reply)");
}

export function endChat(chat: Chat) {
  chat.session?.close();
}
