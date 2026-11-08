import { useMemo, useState, type ContextType } from "react";
import { paraAuth, privyAuth, type AomiWidgetAuth } from "@aomi-labs/widget";
import { WalletSignInOptionsContext } from "@aomi-labs/widget/host-composition";

export const DottieSignIn = WalletSignInOptionsContext.Provider;

type Provider = "privy" | "para";
const privyAppId = import.meta.env.VITE_PRIVY_APP_ID?.trim();
const paraApiKey = import.meta.env.VITE_PARA_API_KEY?.trim();
const paraEnvironment = import.meta.env.VITE_PARA_ENVIRONMENT;

/** Only the provider preference is saved here; the widget owns authentication. */
export function useDottieAuth(baseUrl: string | undefined, applicationId: string) {
  const storageKey = `dottie:${baseUrl ?? "https://chat.aomi.dev"}:${applicationId}:walletProvider`;
  const [provider, setProvider] = useState<Provider | null>(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved === "privy" && privyAppId) return saved;
      if (saved === "para" && paraApiKey) return saved;
    } catch {
      // Sign-in still works when storage is disabled.
    }
    // Solana-only visitors may have no browser wallet extension. Start the
    // default social provider so the widget can enable its sign-in button.
    return privyAppId ? "privy" : paraApiKey ? "para" : null;
  });
  const options = useMemo<ContextType<typeof WalletSignInOptionsContext>>(() => {
    const providers: Provider[] = [];
    if (privyAppId) providers.push("privy");
    if (paraApiKey) providers.push("para");
    return providers.map((id) => ({
      id,
      label: id === "privy" ? "Privy" : "Para",
      description: id === "privy" ? "Email, Google, X or Apple" : "Email or passkey",
      family: "multichain",
      kind: "social",
      status: "available",
      connect: async () => {
        try {
          localStorage.setItem(storageKey, id);
        } catch {
          // Remembering a provider is optional.
        }
        setProvider(id);
      },
    }));
  }, [storageKey]);
  const auth: AomiWidgetAuth = provider === "privy" && privyAppId
    ? privyAuth({ appId: privyAppId, appName: "Sol Dottie", methods: ["email", "google", "x", "apple"] })
    : provider === "para" && paraApiKey
      ? paraAuth({
          apiKey: paraApiKey,
          environment: paraEnvironment === "PROD" ? "PROD" : "BETA",
          methods: ["email", "google"],
          appName: "Sol Dottie",
        })
      : { type: "browser_wallet" };
  return { auth, options };
}
