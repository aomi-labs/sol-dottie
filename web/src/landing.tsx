import { useEffect, useRef, useState } from "react";
import { TelegramDemo } from "./telegram-demo";
import { TelegramIcon } from "./telegram-icon";
import { DOTTIE_TELEGRAM_URL } from "./site-config";

const APP_URL = "/app";
/** The QR points at the Telegram bot when there is one, else at this site. */
const telegramUrl = DOTTIE_TELEGRAM_URL;
const hasBot = telegramUrl !== "";
const botHandle = hasBot ? "@" + telegramUrl.replace(/\/+$/, "").split("/").pop()!.toUpperCase() : "";

const sessions = [
  { day: "Wednesday · 4 Nov", time: "15:00 – 17:00", where: "Room 6 · Workshop + integration clinic" },
  { day: "Sunday · 8 Nov", time: "14:00 – 15:00", where: "Room 9 · Agents day · Show ’n’ Tell" },
];

/** What Dottie does, one card each. The asks are the ones the Telegram demo replays. */
const features = [
  { id: "check", index: "01 / CHECK IT", title: "Is it a rug?", ask: "is FLUXB safe?",
    text: "Mint authority, freeze authority, the creator’s bag, hidden transfer fees and the top ten, read off RugCheck one by one. No kill switches still doesn’t get called safe." },
  { id: "trade", index: "02 / TRADE IT", title: "Your move. Your venue.", ask: "quote 0.01 sol into fluxb on fluxbeam",
    text: "A real FluxBeam quote at your size before you’re in, with the minimum out and the token’s own fee spelled out. Jupiter for the big names. Dottie stages it, your wallet signs it." },
  { id: "earn", index: "03 / EARN IT", title: "Give idle dollars a job.", ask: "can my spare dollars earn in solomon usdv?",
    text: "Solomon’s USDv is a dollar that earns in its default state, with no staking or wrapping. Dottie explains it in a paragraph and reads your position before you move a cent." },
  { id: "stocks", index: "04 / STOCK TOKENS", title: "Wall Street, in your pocket.", ask: "can i buy nvidia on solana?",
    text: "NVIDIA xStock and friends from the verified list only, never a lookalike. Dottie names the mint, quotes it through Jupiter and says the honest line: tokenized exposure, not the share." },
];

const icons: Record<string, React.ReactNode> = {
  check: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m20 20-4.8-4.8M8 10.5h5M10.5 8v5" /></svg>,
  trade: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M4 8h13l-3-3M20 16H7l3 3" /></svg>,
  earn: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5v9M14.6 9.6c-.5-.9-1.5-1.3-2.6-1.3-1.5 0-2.6.8-2.6 1.9 0 2.6 5.2 1.3 5.2 3.9 0 1.1-1.1 1.9-2.6 1.9-1.1 0-2.1-.4-2.6-1.3" /></svg>,
  stocks: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M4 19h16M5 15l4.5-5 3.5 3 6-7" /><path d="M15 6h4v4" /></svg>,
};

function Dottie({ className = "dottie-img" }: { className?: string }) {
  return <img className={className} src="/art/dottie.png" alt="" aria-hidden="true" width={512} height={512} />;
}

/** Primary call to action: Telegram when the bot exists, else the web app. */
function MainCta({ className = "" }: { className?: string }) {
  if (hasBot)
    return (
      <a className={`primary-button tg-button ${className}`} href={telegramUrl} target="_blank" rel="noreferrer">
        <TelegramIcon /> Open in Telegram
      </a>
    );
  return (
    <a className={`primary-button ${className}`} href={APP_URL}>
      Talk to Dottie <span>→</span>
    </a>
  );
}

function SecondaryCta({ className = "text-link" }: { className?: string }) {
  if (hasBot) return <a className={className} href={APP_URL}>Or use the web app <span>→</span></a>;
  return <span className={`${className} soon`}>Telegram bot on its way</span>;
}

/** QR sticker drawn on the client for whatever the site points at. */
function QrCode({ url }: { url: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    void import("qrcode").then(({ toCanvas }) =>
      toCanvas(canvas, url, { width: 300, margin: 0, errorCorrectionLevel: "H", color: { dark: "#1d0e2e", light: "#ffffff" } }),
    );
  }, [url]);
  return (
    <div className="qr-code" role="img" aria-label={`QR code for ${url}`}>
      <canvas ref={ref} width={300} height={300} />
      <span><Dottie className="qr-mark" /></span>
    </div>
  );
}

export function DottieLanding() {
  const [qrFront, setQrFront] = useState(false);
  const [siteUrl, setSiteUrl] = useState("https://sol-dottie");
  useEffect(() => setSiteUrl(`${window.location.origin}${window.location.pathname}`), []);
  const qrUrl = hasBot ? telegramUrl : siteUrl;
  const qrLabel = hasBot ? botHandle : siteUrl.replace(/^https?:\/\//, "").replace(/\/$/, "").toUpperCase();

  const toggleStage = () => setQrFront((value) => !value);
  const stageKey = (event: React.KeyboardEvent) => {
    if (event.key === "Enter" || event.key === " ") { event.preventDefault(); toggleStage(); }
  };
  const stageLabel = qrFront ? "Shrink the QR code" : "Enlarge the QR code";

  return (
    <main className="site-shell">
      <div className="hero-zone">
        <header className="nav-shell">
          <a className="brand" href="#top" aria-label="Sol Dottie home">
            <Dottie className="brand-mark" />
            <span>Sol Dottie</span>
          </a>
          <nav className="nav-links" aria-label="Sections">
            <a href="#demo">Demo</a>
            <a href="#edge">The edge</a>
            <a href="#features">Features</a>
            <a href="#how">How it works</a>
            <a href="#join">Join</a>
          </nav>
          <div className="nav-cta">
            <a className="nav-web" href={APP_URL}>Web app</a>
            {hasBot ? (
              <a className="telegram-link" href={telegramUrl} target="_blank" rel="noreferrer"><TelegramIcon /> Telegram</a>
            ) : (
              <a className="telegram-link" href={APP_URL}><Dottie className="tg-icon" /> Talk to Dottie</a>
            )}
          </div>
        </header>

        <div className="hero-band">
          <section className="hero" id="top">
            <div className="hero-copy">
              <div className="eyebrow"><span className="eyebrow-live"><i /> LIVE ON</span><strong>SOLANA</strong></div>
              <h1>Less tabs.<em>More <b>“done.”</b></em></h1>
              <p className="hero-dek">
                Meet Dottie, your little Solana sidekick. Check that coin, make your move, and give your spare
                dollars a job. Ask in plain English. Dottie reads the chain, stages the trade, and you sign.
              </p>
              <div className="hero-actions">
                <MainCta />
                <SecondaryCta />
              </div>
              <div className="hero-proof">
                <div><strong>CHECK IT</strong><span>RugCheck · authorities · holders</span></div>
                <div><strong>TRADE IT</strong><span>FluxBeam · Jupiter · stock tokens</span></div>
                <div><strong>EARN IT</strong><span>Solomon USDv · spare dollars</span></div>
              </div>
              <div className={`hero-card${qrFront ? " big" : ""}`} role="button" tabIndex={0} aria-label={stageLabel} aria-pressed={qrFront} onClick={toggleStage} onKeyDown={stageKey}>
                <b>Hi, I’m Dottie!</b>
                <QrCode url={qrUrl} />
                <div className="qr-caption"><span>SCAN ME</span><a href={qrUrl} target="_blank" rel="noreferrer" onClick={(event) => event.stopPropagation()}>{qrLabel}</a></div>
              </div>
            </div>
            <div className="hero-gang" aria-hidden="true">
              <img src="/art/gang-cut.webp" alt="" width={1461} height={941} />
            </div>
          </section>
          <i className="cloud cloud-a" aria-hidden="true" />
          <i className="cloud cloud-b" aria-hidden="true" />
          <i className="cloud cloud-c" aria-hidden="true" />
          <i className="hero-hill" aria-hidden="true" />
        </div>


        <section className="widget-demo-section section-shell" id="demo">
          <div className="widget-demo-copy">
            <p className="overline">Live demo</p>
            <h2>Try Dottie<em>before you scan.</em></h2>
            <p>This is how Dottie talks in Telegram: a short line while she checks, then a straight answer. Built from her live reads on Nov 2, 2026.</p>
          </div>
          <div className="demo-witch" aria-hidden="true"><img src="/art/witch-cut.webp" alt="" width={460} height={894} /></div>
          <div className="widget-wrap">
            <TelegramDemo />
            <a className="join-link demo-web" href={APP_URL}>Or ask her on the web <span>→</span></a>
          </div>
        </section>
      </div>

      <div className="tape" aria-hidden="true">
        <div>WHO MADE IT <span>✦</span> WHO HOLDS IT <span>✦</span> WHAT’S THE FEE <span>✦</span> WHAT’S THE QUOTE <span>✦</span> WHERE DO DOLLARS EARN <span>✦</span> WHO MADE IT <span>✦</span> WHO HOLDS IT <span>✦</span> WHAT’S THE FEE <span>✦</span> WHAT’S THE QUOTE <span>✦</span> WHERE DO DOLLARS EARN <span>✦</span></div>
      </div>

      <section className="edge" id="edge">
       <div className="thesis section-shell">
        <div className="section-number">01 / THE EDGE</div>
        <div className="thesis-copy">
          <p className="overline">YOUR LITTLE DESK</p>
          <h2>Fewer tabs.<em>Clearer calls.</em></h2>
          <p>Dottie does the homework a careful friend would: the RugCheck report, the real quote, the fee hiding in the token. Then she gives you her honest read, and “keep it small” counts as an answer.</p>
        </div>
        <section className="workshop plate-cream" id="workshop">
          <div className="workshop-copy">
            <p className="overline">BREAKPOINT · LONDON · SOLANA HACKER HOUSE</p>
            <h2>Ship a financial agent in an hour.</h2>
            <p>Bring a laptop. You leave with your own Dottie: a Solana agent with a custom tool, deployed and chatting.</p>
            <a className="text-link" href="https://app.notion.com/p/40c36be0954d81139c4dc39d55d062ed" target="_blank" rel="noreferrer">The full build plan <span>→</span></a>
          </div>
          <div className="sessions">
            {sessions.map((s) => (
              <article className="session" key={s.day}><span>{s.day}</span><b>{s.time}</b><small>{s.where}</small></article>
            ))}
          </div>
        </section>
       </div>
      </section>

      <div className="features-wrap section-shell" id="features">
        <div className="desk-readout plate">
          <div className="desk-status"><span><i /> FROM A LIVE READ · NOV 2, 2026</span><b>NOT ADVICE</b></div>
          <div className="desk-intent">
            <small>YOUR INTENT</small>
            <p>“is FLUXB safe? and what does 0.01 SOL get me?”</p>
            <span>ONE TELEGRAM MESSAGE</span>
          </div>
          <div className="desk-resolve">
            <div><small>01 / CHECK</small><strong>RugCheck report</strong><span>Mint and freeze authority, creator bag, transfer fee, top holders.</span></div>
            <div><small>02 / QUOTE</small><strong>FluxBeam pool</strong><span>A real quote at your size, with the minimum out after slippage.</span></div>
            <div><small>03 / CALL</small><strong>A straight take</strong><span>No kill switches, but a 3% tax and heavy hands. Keep it small.</span></div>
          </div>
          <div className="desk-ticker" aria-hidden="true">
            <span>TRANSFER FEE <b>3%</b></span><i />
            <span>TOP HOLDER <b>20.1%</b></span><i />
            <span>0.01 SOL <b>≈ 285.58 FLUXB</b></span><i />
            <span>MIN OUT <b>282.73</b></span>
          </div>
        </div>
        <section className="features plate-cream">
          <div className="features-head">
            <p className="overline">IN TELEGRAM · ON THE WEB</p>
            <h2>One chat.<em>A whole little desk.</em></h2>
            <p>Ask the way you’d ask a friend who trades. Memecoins, stock tokens like NVDA, and a dollar that earns. Every answer comes from Dottie’s live reads on Nov 2, 2026.</p>
          </div>
          <div className="feature-grid">
            {features.map((f) => (
              <article className={`feature ${f.id}`} key={f.id}>
                <div className="feature-top"><span className="feature-icon">{icons[f.id]}</span><span className="feature-index">{f.index}</span></div>
                <h3>{f.title}</h3>
                <p>{f.text}</p>
                <div className="show-ask"><span>{f.ask}</span></div>
              </article>
            ))}
          </div>
          <div className="features-dottie" aria-hidden="true">
            <span className="tag">Ask me anything!</span>
            <Dottie />
            <i className="hill" />
          </div>
          <i className="cloud cloud-d" aria-hidden="true" />
        </section>
      </div>

      <div className="flow-wrap section-shell" id="how">
        <section className="flow">
          <div className="flow-header">
            <p className="overline">ZERO COMMANDS TO MEMORIZE</p>
            <h2>From question to<em>reviewable trade.</em></h2>
          </div>
          <div className="flow-steps">
            <article>
              <b className="flow-number">1</b>
              <div className="flow-copy"><h3>Scan</h3><p>{hasBot ? "Open Dottie from the QR code and link your Telegram." : "Open Dottie from the QR code, on the web or in Telegram."}</p><small className="flow-note">SESSION CREATED IN SECONDS</small></div>
              <div className="flow-visual scan-visual" aria-hidden="true"><i className="scan-ring scan-ring-outer" /><i className="scan-ring scan-ring-inner" /><Dottie className="scan-mark" /><small className="scan-status">LINKED</small></div>
            </article>
            <article>
              <b className="flow-number">2</b>
              <div className="flow-copy"><h3>Fund</h3><p>Bring SOL to your wallet. Dottie never holds your keys.</p><small className="flow-note">YOUR BALANCE · YOUR CONTROL</small></div>
              <div className="flow-visual balance-visual"><i className="balance-coin" aria-hidden="true">◎</i><div><small className="balance-label">AVAILABLE TO TRADE</small><strong className="balance-amount">0.80 SOL</strong><span className="balance-chain"><i /> SOLANA MAINNET</span></div></div>
            </article>
            <article>
              <b className="flow-number">3</b>
              <div className="flow-copy"><h3>Talk</h3><p>Ask about a coin or name a trade. Dottie checks it, quotes it, stages the swap, and you sign when it looks right.</p><small className="flow-note">NO COMMAND SYNTAX</small></div>
              <div className="flow-visual talk-visual"><span className="talk-command">“check FLUXB, then buy 0.01 SOL.”</span><i className="talk-arrow">→</i><b className="talk-review">REVIEW TX ↗</b></div>
            </article>
          </div>
        </section>

      </div>

      <section className="closing" id="join">
        <div className="closing-inner section-shell">
          <div className="closing-copy">
            <p className="overline">EARLY ACCESS</p>
            <h2>Your next move?<em>Just ask Dottie.</em></h2>
            <p>{hasBot ? "Open Dottie in Telegram, or try her on the web today. Same Dottie, same wallet, your final say." : "Try Dottie on the web today. The Telegram bot is on its way: same Dottie, same wallet, your final say."}</p>
            <div className="join-actions">
              <MainCta />
              {hasBot ? <a className="join-link" href={APP_URL}>Or use the web app <span>→</span></a> : null}
            </div>
            <div className="closer-details">
              <div className="closer-row closer-account"><span><i /> {hasBot ? botHandle : "DOTTIE WEB APP"}</span><b>ONLINE</b></div>
              <div className="closer-row"><span>IDENTITY</span><b>{hasBot ? "TELEGRAM · WALLET" : "YOUR WALLET"}</b></div>
              <div className="closer-row"><span>SETTLEMENT</span><b>SOLANA</b></div>
              <div className="closer-row"><span>FINAL SAY</span><b>ALWAYS YOURS</b></div>
            </div>
          </div>
          <div className="closing-dottie" aria-hidden="true"><Dottie /></div>
        </div>
      </section>

      <footer className="footer section-shell">
        <div className="brand footer-brand"><Dottie className="brand-mark" /><span>Sol Dottie</span></div>
        <p>Check it. Trade it. Keep the final say.</p>
        <p className="risk">Sol Dottie does not provide financial advice. Numbers on this page come from Dottie’s live reads on Nov 2, 2026 and the balance is illustrative. Nothing executes until you sign. Built on Aomi.</p>
        <span>© 2026 AOMI LABS</span>
      </footer>
    </main>
  );
}
