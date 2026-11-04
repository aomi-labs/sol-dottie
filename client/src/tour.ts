import { ask, endChat, newChat } from "./turn.ts";

// Each inner list is one session; the prompts run in order inside it.
const sessions = [
  [
    "Which build are you?",
    "Check the token EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v with RugCheck",
  ],
  ["Quote a FluxBeam swap of 0.01 SOL into FLUXB."],
  ["What is Solomon USDv, and what does my Solomon position look like?"],
];

for (const prompts of sessions) {
  const chat = newChat();
  try {
    for (const prompt of prompts) await ask(chat, prompt);
  } finally {
    endChat(chat);
  }
}
