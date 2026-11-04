import { lazy, Suspense } from "react";
import { TelegramIcon } from "../telegram-icon";
import {
  AOMI_API_URL,
  DOTTIE_APPLICATION_ID,
  DOTTIE_DEFAULT_MODEL,
  DOTTIE_SHOW_MODEL_PICKER,
  DOTTIE_TELEGRAM_URL,
} from "../site-config";
import "./app.css";

// The widget and its wallet stack are heavy; the page shell paints first.
const DottieWidget = lazy(() => import("./dottie-widget"));

// `?application_id=` overrides the build's App for rehearsals.
const params = new URLSearchParams(window.location.search);
const applicationId = params.get("application_id")?.trim() || DOTTIE_APPLICATION_ID;

/** Stand-in shaped like the widget's empty chat, so nothing jumps when it arrives. */
function WidgetLoading({ text = "Waking up Dottie" }: { text?: string }) {
  return (
    <div className="widget-loading" role="status">
      <div className="widget-loading-side" aria-hidden="true"><i /><i /><i /></div>
      <div className="widget-loading-main">
        <span className="widget-loading-mark"><img src="/art/dottie.png" alt="" width={64} height={64} /></span>
        <p>{text}</p>
        <div className="widget-loading-composer" aria-hidden="true" />
      </div>
    </div>
  );
}

export function DottieApp() {
  return (
    <main className="dottie-app">
      <header className="app-bar">
        <a className="brand" href="/" aria-label="Sol Dottie home">
          <img className="brand-mark dottie-img" src="/art/dottie.png" alt="" width={64} height={64} />
          <span>Sol Dottie</span>
        </a>
        <span className="app-chain"><i /> SOLANA MAINNET</span>
        {DOTTIE_TELEGRAM_URL ? (
          <a className="app-tg" href={DOTTIE_TELEGRAM_URL} target="_blank" rel="noreferrer"><TelegramIcon /> Telegram</a>
        ) : null}
        <a className="app-back" href="/">← Back to site</a>
      </header>
      <section className="app-frame" aria-label="Sol Dottie chat">
        {applicationId ? (
          <Suspense fallback={<WidgetLoading />}>
            <DottieWidget applicationId={applicationId} baseUrl={AOMI_API_URL} showModelPicker={DOTTIE_SHOW_MODEL_PICKER} defaultModel={DOTTIE_DEFAULT_MODEL} />
          </Suspense>
        ) : (
          <WidgetLoading text="Dottie isn’t connected to an app yet. Set VITE_DOTTIE_APPLICATION_ID." />
        )}
      </section>
      <footer className="app-foot">
        <span>© 2026 <b>SOL DOTTIE</b> · SOLANA · BUILT ON AOMI</span>
        <span>Not financial advice. Your wallet signs every transaction.</span>
      </footer>
    </main>
  );
}
