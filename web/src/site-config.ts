// Public, non-secret settings for the deployed site. A Vite env var of the same
// name (from .env.local or the deploy) overrides each one. The app template's
// names (VITE_AOMI_APPLICATION_ID, VITE_AOMI_BASE_URL) also work; Dottie's own
// names win when both are set.
const env = import.meta.env;

/** Hosted Sol Dottie App. Empty ships the sample-data concept desk. */
export const DOTTIE_APPLICATION_ID =
  env.VITE_DOTTIE_APPLICATION_ID?.trim() ||
  env.VITE_DOTTIE_APP_ID?.trim() ||
  env.VITE_AOMI_APPLICATION_ID?.trim() ||
  "";

/** Aomi backend. Undefined means the widget default, https://chat.aomi.dev. */
export const AOMI_API_URL =
  env.VITE_AOMI_API_URL?.trim() || env.VITE_AOMI_BASE_URL?.trim() || undefined;

/** Dottie's Telegram bot, e.g. https://t.me/<bot_username>. */
export const DOTTIE_TELEGRAM_URL = env.VITE_DOTTIE_TELEGRAM_URL?.trim() || "";

/** Local stacks may lack the default model's supplier, so they can show the picker. */
export const DOTTIE_SHOW_MODEL_PICKER = env.VITE_DOTTIE_SHOW_MODEL === "1";

/** Model preset for visitors who have not picked one. Empty keeps the widget default. */
export const DOTTIE_DEFAULT_MODEL = env.VITE_DOTTIE_DEFAULT_MODEL?.trim() || "";
