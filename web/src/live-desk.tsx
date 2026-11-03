import { StrictMode, useEffect, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { createRoot } from "react-dom/client";
import { AomiWidget, type AomiRoutingConfig } from "@aomi-labs/widget";
import "@aomi-labs/widget/styles.css";
import { showModal } from "./mock-desk";

const WELCOME_TITLE = "Hi, I’m Dottie. What should we check?";

// The suggestion chips put a starter prompt in the composer. They never send:
// the visitor reads it, finishes it and presses send. They stay within what the
// Rust app in apps/dottie can answer.
const SUGGESTIONS = [
  { label: "Check a token", prompt: "Check this token with RugCheck before I buy: " },
  {
    label: "Quote 0.01 SOL → FLUXB",
    prompt: "Quote a FluxBeam swap of 0.01 SOL into FLUXB.",
  },
  {
    label: "Solomon USDv",
    prompt: "What is Solomon USDv, and what does my Solomon position look like?",
  },
  {
    label: "NVIDIA xStock",
    prompt:
      "Show me NVIDIA xStock (NVDAx) from your asset list and a Jupiter quote for 0.01 SOL into it.",
  },
];

export type LiveDeskOptions = {
  applicationId: string;
  baseUrl?: string;
  telegramUrl: string;
  showModelPicker: boolean;
  defaultModel: string;
};

const q = <T extends Element = HTMLElement>(selector: string) =>
  document.querySelector<T>(selector);

/** Replace the concept desk with the live Aomi widget, branded for Dottie. */
export function mountLiveDesk({
  applicationId,
  baseUrl,
  telegramUrl,
  showModelPicker,
  defaultModel,
}: LiveDeskOptions) {
  const shell = q("#desk-shell");
  if (!shell) return;

  const label = q(".demo-label");
  if (label)
    label.innerHTML =
      "<i></i> Live on Solana mainnet · Real quotes · Your wallet signs every transaction";

  // The widget owns the whole desk: its own sidebar, chat and sign-in. The
  // element keeps its id so the Web app / Telegram toggle still finds it.
  const frame = document.createElement("section");
  frame.id = "desk-shell";
  frame.className = "dottie-app";
  frame.setAttribute("aria-label", "Sol Dottie chat");
  shell.replaceWith(frame);

  // Telegram section: point at the real bot when there is one.
  const channelNote = q(".channel-copy small");
  if (channelNote)
    channelNote.textContent = telegramUrl
      ? "Same Dottie, same wallet handoff, in your pocket."
      : "The Telegram bot is on its way. The web desk above is live.";
  if (telegramUrl) {
    const tgBtn = q(".channel-copy .btn");
    if (tgBtn)
      tgBtn.innerHTML =
        '<svg><use href="#tg"/></svg> Open in Telegram <span>↗</span>';
  }

  const numericId = Number(applicationId);
  const routing: AomiRoutingConfig =
    Number.isSafeInteger(numericId) && numericId > 0
      ? {
          targets: [
            {
              mode: "direct",
              apps: [{ applicationId: numericId, app: "dottie" }],
            },
          ],
          defaultMode: "direct",
        }
      : { targets: [{ mode: "auto" }] };

  if (defaultModel) presetModel(baseUrl, applicationId, defaultModel);

  createRoot(frame).render(
    <StrictMode>
      <DottieDesk
        applicationId={applicationId}
        baseUrl={baseUrl}
        routing={routing}
        showModelPicker={showModelPicker}
      />
    </StrictMode>,
  );
}

function DottieDesk({
  applicationId,
  baseUrl,
  routing,
  showModelPicker,
}: {
  applicationId: string;
  baseUrl?: string;
  routing: AomiRoutingConfig;
  showModelPicker: boolean;
}) {
  const frameRef = useRef<HTMLDivElement>(null);
  return (
    <div className="dottie-frame" ref={frameRef}>
      <AomiWidget
        applicationId={applicationId}
        baseUrl={baseUrl}
        theme="light"
        routing={routing}
        auth={{ type: "browser_wallet" }}
        walletFamilies={["solana"]}
        wallets={{
          evm: false,
          solana: {
            preset: "popular",
            networks: [
              {
                id: "solana:mainnet",
                label: "Solana",
                cluster: "solana:mainnet",
                rpcHttpUrl: "https://api.mainnet-beta.solana.com",
                isDefault: true,
              },
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

const sameList = (a: HTMLElement[], b: HTMLElement[]) =>
  a.length === b.length && a.every((el, i) => el === b[i]);

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
      if (title && title.textContent !== WELCOME_TITLE)
        title.textContent = WELCOME_TITLE;
      setWelcome(root);
      const found = [
        ...frame.querySelectorAll<HTMLElement>(".aui-composer-action-scroll"),
      ];
      setBars((current) => (sameList(current, found) ? current : found));
    };
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(frame, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [frameRef]);

  const pills = bars.map((bar, i) =>
    createPortal(<ChainPill />, bar, `chain-${i}`),
  );
  const host = welcome?.querySelector(".aui-thread-welcome-suggestions");
  if (!welcome || !host) return <>{pills}</>;
  return (
    <>
      {pills}
      {createPortal(
        <div
          className="dottie-suggestions"
          role="group"
          aria-label="Things to ask Dottie"
        >
          {SUGGESTIONS.map((item) => (
            <button
              key={item.label}
              type="button"
              className="dottie-suggestion"
              aria-label={item.prompt}
              onClick={() => fillComposer(welcome, item.prompt)}
            >
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
  const input = root.querySelector<HTMLElement>(
    '[data-testid="aomi-composer-input"]',
  );
  if (input && input.isContentEditable) {
    input.focus();
    window.getSelection()?.selectAllChildren(input);
    // execCommand keeps the editor's own input handling in the loop.
    if (document.execCommand("insertText", false, text)) return;
  }
  void navigator.clipboard?.writeText(text).catch(() => undefined);
  showModal("Here’s a starter.", `Paste this into the chat: “${text.trim()}”`);
}

/** Preset the widget's model for visitors who have not picked one. A local
 * stack sets this when the default model's supplier has no credit. The key
 * mirrors the widget's own scoped preference storage. */
function presetModel(
  baseUrl: string | undefined,
  applicationId: string,
  model: string,
) {
  const origin = (baseUrl ?? "https://chat.aomi.dev").replace(/\/+$/, "");
  const key = [
    "aomi",
    "v2",
    encodeURIComponent(origin),
    encodeURIComponent(applicationId),
    "modelSelection",
  ].join(":");
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null");
    if (saved?.mode === "manual") return;
    localStorage.setItem(key, JSON.stringify({ mode: "manual", model }));
  } catch {
    // Storage can be denied; the widget then uses its own default.
  }
}
