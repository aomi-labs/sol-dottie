import { useEffect, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { AomiWidget, type AomiRoutingConfig } from "@aomi-labs/widget";
import "@aomi-labs/widget/styles.css";
import { DottieSignIn, useDottieAuth } from "./sign-in";

const WELCOME_TITLE = "Hi, I’m Dottie. What should we check?";

// The suggestion chips put a starter prompt in the composer. They never send:
// the visitor reads it, finishes it and presses send. They stay within what the
// Rust app in apps/dottie can answer.
const SUGGESTIONS = [
  { label: "Check a token", prompt: "Check this token with RugCheck before I buy: " },
  { label: "Quote 0.01 SOL → FLUXB", prompt: "Quote a FluxBeam swap of 0.01 SOL into FLUXB." },
  { label: "Solomon USDv", prompt: "What is Solomon USDv, and what does my Solomon position look like?" },
  { label: "NVIDIA xStock", prompt: "Show me NVIDIA xStock (NVDAx) from your asset list and a Jupiter quote for 0.01 SOL into it." },
];

export type DottieWidgetProps = {
  applicationId: string;
  baseUrl?: string;
  showModelPicker: boolean;
  defaultModel: string;
};

/** Every turn goes straight to the Dottie app instead of Aomi's auto router. */
function routingFor(applicationId: string): AomiRoutingConfig {
  const numericId = Number(applicationId);
  return Number.isSafeInteger(numericId) && numericId > 0
    ? { targets: [{ mode: "direct", apps: [{ applicationId: numericId, app: "dottie" }] }], defaultMode: "direct" }
    : { targets: [{ mode: "auto" }] };
}

export default function DottieWidget({ applicationId, baseUrl, showModelPicker, defaultModel }: DottieWidgetProps) {
  const frameRef = useRef<HTMLDivElement>(null);
  const { auth, options } = useDottieAuth(baseUrl, applicationId);
  if (defaultModel) presetModel(baseUrl, applicationId, defaultModel);
  return (
    <div className="dottie-frame" ref={frameRef}>
      <DottieSignIn value={options}>
        <AomiWidget
          applicationId={applicationId}
          baseUrl={baseUrl}
          theme="light"
          routing={routingFor(applicationId)}
          auth={auth}
          walletFamilies={["solana"]}
          wallets={{
            // Widget 4's social providers need the EVM runtime internally.
            // Dottie's visible wallet family and app routing remain Solana.
            evm: { wallets: [], coinbase: false },
            solana: {
              preset: "popular",
              networks: [
                { id: "solana:mainnet", label: "Solana", cluster: "solana:mainnet", rpcHttpUrl: "https://api.mainnet-beta.solana.com", isDefault: true },
              ],
            },
          }}
          showHeader
          showSidebar
          walletPosition="footer"
          features={{ theme: false, library: false }}
          controlBarProps={{ hideModel: !showModelPicker, hideAppSecrets: true }}
          height="100%"
          width="100%"
          // Match the frame's inner radius so the corners read as one edge.
          style={{ borderRadius: 10, boxShadow: "none" }}
        />
      </DottieSignIn>
      <DottieBrand frameRef={frameRef} />
    </div>
  );
}

/** Static chip standing in for the network selector, which hides itself when only one chain is configured. */
function ChainPill() {
  return (
    <span className="dottie-chain" title="Dottie works on Solana mainnet">
      <i aria-hidden="true" />
      Solana
    </span>
  );
}

const sameList = (a: HTMLElement[], b: HTMLElement[]) => a.length === b.length && a.every((el, i) => el === b[i]);

/**
 * Dottie's welcome title, suggestion chips and chain pill inside the widget.
 * AomiWidget 4.0.2 has no props for these, so this patches its DOM the way
 * Hoodit does. Logos and placeholders are CSS in live.css.
 */
function DottieBrand({ frameRef }: { frameRef: RefObject<HTMLElement | null> }) {
  const [welcome, setWelcome] = useState<HTMLElement | null>(null);
  const [bars, setBars] = useState<HTMLElement[]>([]);

  useEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const sync = () => {
      const root = frame.querySelector<HTMLElement>(".aui-thread-welcome-root");
      const title = root?.querySelector(".aui-thread-welcome-title");
      if (title && title.textContent !== WELCOME_TITLE) title.textContent = WELCOME_TITLE;
      setWelcome(root);
      const found = [...frame.querySelectorAll<HTMLElement>(".aui-composer-action-scroll")];
      setBars((current) => (sameList(current, found) ? current : found));
    };
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(frame, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [frameRef]);

  const pills = bars.map((bar, i) => createPortal(<ChainPill />, bar, `chain-${i}`));
  const host = welcome?.querySelector(".aui-thread-welcome-suggestions");
  if (!welcome || !host) return <>{pills}</>;
  return (
    <>
      {pills}
      {createPortal(
        <div className="dottie-suggestions" role="group" aria-label="Things to ask Dottie">
          {SUGGESTIONS.map((item) => (
            <button key={item.label} type="button" className="dottie-suggestion" aria-label={item.prompt} onClick={() => fillComposer(welcome, item.prompt)}>
              <i aria-hidden="true" />
              {item.label}
            </button>
          ))}
        </div>,
        host,
      )}
    </>
  );
}

/** Put text in the widget composer (a contenteditable) without sending it. */
function fillComposer(root: HTMLElement, text: string) {
  const input = root.querySelector<HTMLElement>('[data-testid="aomi-composer-input"]');
  if (input && input.isContentEditable) {
    input.focus();
    window.getSelection()?.selectAllChildren(input);
    // execCommand keeps the editor's own input handling in the loop.
    if (document.execCommand("insertText", false, text)) return;
  }
  void navigator.clipboard?.writeText(text).catch(() => undefined);
  toast(`Copied. Paste this into the chat: “${text.trim()}”`);
}

function toast(message: string) {
  const el = document.createElement("div");
  el.className = "dottie-toast";
  el.setAttribute("role", "status");
  el.textContent = message;
  document.body.append(el);
  window.setTimeout(() => el.remove(), 4000);
}

/** Preset the widget's model for visitors who have not picked one. A local
 * stack sets this when the default model's supplier has no credit. The key
 * mirrors the widget's own scoped preference storage. */
function presetModel(baseUrl: string | undefined, applicationId: string, model: string) {
  const origin = (baseUrl ?? "https://chat.aomi.dev").replace(/\/+$/, "");
  const key = ["aomi", "v2", encodeURIComponent(origin), encodeURIComponent(applicationId), "modelSelection"].join(":");
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null");
    if (saved?.mode === "manual") return;
    localStorage.setItem(key, JSON.stringify({ mode: "manual", model }));
  } catch {
    // Storage can be denied; the widget then uses its own default.
  }
}
