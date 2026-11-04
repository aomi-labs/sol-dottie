import { useEffect, useRef, useState, useSyncExternalStore } from "react";

/* Replays of Dottie's live tool reads (Nov 2, 2026): the RugCheck report and
   FluxBeam quote for FLUXB, Solomon's USDv docs and position read, and the
   NVIDIA xStock listing. Numbers come from docs/evidence; the wording is how
   Dottie answers. Markup in lines: **bold**, `mint…`. Each string in `answer`
   is one paragraph. */
export type Script = { id: string; chip: string; time: string; user: string; steps: string[]; answer: string[] };

export const scripts: Script[] = [
  {
    id: "check",
    chip: "is FLUXB safe?",
    time: "15:02",
    user: "is FLUXB safe? FLUXBmPhT3Fd1EDVFdg46YREqHBeNypn1h4EbnTzWERX",
    steps: [
      "pulling the RugCheck report for that mint",
      "it's a Token-2022 mint with a transfer fee. checking authorities and the biggest wallets before i call it",
    ],
    answer: [
      "**FluxBot (FLUXB)**, read off RugCheck:",
      "mint authority **none** · freeze authority **none** · creator holds **0**. nobody can print more or freeze you.",
      "there's a **3% transfer fee** baked into the token, so every move costs a haircut.",
      "top holder has **20.1%**, the top two together **38.8%**. that's concentrated, not a rug by itself.",
      "so: no kill switches, but a tax and heavy hands. keep it small if you go.",
    ],
  },
  {
    id: "quote",
    chip: "quote 0.01 SOL",
    time: "15:04",
    user: "ok quote 0.01 sol into fluxb on fluxbeam",
    steps: ["asking FluxBeam for a quote on the SOL/FLUXB pool"],
    answer: [
      "FluxBeam quote, pool `GeNc51…cW27`:",
      "**0.01 SOL → ≈ 285.58 FLUXB**, minimum out **282.73** after slippage.",
      "remember the 3% transfer fee, so about 277 should land in your wallet.",
      "say go and i'll stage the swap. you review it and sign in your wallet, nothing moves before that.",
    ],
  },
  {
    id: "earn",
    chip: "what's USDv?",
    time: "15:09",
    user: "what's solomon usdv? can my spare dollars earn there",
    steps: ["reading Solomon's docs and checking your wallet for a USDv position"],
    answer: [
      "**USDv** is Solomon's dollar. it tracks $1, backed by short-dated US Treasuries and selected stablecoins. eligible holders earn in its default state: no staking, wrapping or lockups.",
      "mint `USDvUSpnhCr9yBgj3UyVrD239HRUv4RsHwH2FxsWuMk`",
      "your wallet holds **no USDv yet**. want a Jupiter quote from USDC to start?",
    ],
  },
  {
    id: "stocks",
    chip: "buy NVIDIA",
    time: "15:13",
    user: "can i buy nvidia on solana? what does 0.01 sol get me",
    steps: ["looking up NVIDIA xStock in the verified list, then checking Jupiter for 0.01 SOL"],
    answer: [
      "**NVIDIA xStock (NVDAx)** is Backed's tokenized NVIDIA share on Solana. verified mint `Xsc9qv…9qEh`, last price about **$177.44**.",
      "0.01 SOL buys a small slice of one share. i'll stage the Jupiter swap and your wallet signs.",
      "one honest line: **tokenized exposure, not the underlying share.**",
    ],
  },
];

const TYPE_MS = 30;
const THINK_MS = 1350;
const ANSWER_THINK_MS = 1900;
const HOLD_MS = 6500;
const FADE_MS = 450;

type Phase = { typed: number; sent: boolean; shown: number; typing: boolean; leaving: boolean };
const empty: Phase = { typed: 0, sent: false, shown: 0, typing: false, leaving: false };
const finished = (s: Script): Phase => ({ typed: 0, sent: true, shown: s.steps.length + 1, typing: false, leaving: false });

const motionQuery = "(prefers-reduced-motion: reduce)";
export function useReducedMotion() {
  return useSyncExternalStore(
    (cb) => { const m = window.matchMedia(motionQuery); m.addEventListener("change", cb); return () => m.removeEventListener("change", cb); },
    () => window.matchMedia(motionQuery).matches,
    () => false,
  );
}

export function Rich({ text }: { text: string }) {
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`)/).map((part, i) => {
    if (part.startsWith("**")) return <b key={i}>{part.slice(2, -2)}</b>;
    if (part.startsWith("`")) return <code key={i}>{part.slice(1, -1)}</code>;
    return part;
  });
}

function Checks() {
  return (
    <svg width="16" height="10" viewBox="0 0 22 13" aria-hidden="true">
      <path d="M1 7l4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      <path d="M9 11l1 0 8-9" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function Bubble({ out, time, children }: { out?: boolean; time: string; children: React.ReactNode }) {
  return (
    <div className={`tg-row ${out ? "out" : "in"}`}>
      <div className="tg-rowin">
        <div className="tg-bub">
          {children}
          <span className="tg-meta">{time}{out ? <Checks /> : null}</span>
          <svg className="tg-tail" viewBox="0 0 16 22" aria-hidden="true">
            <path d={out ? "M0 0C0 12 6 19 16 22H0Z" : "M16 0C16 12 10 19 0 22H16Z"} />
          </svg>
        </div>
      </div>
    </div>
  );
}

/** The phone's iOS-style status bar and chat header. */
export function PhoneTop({ time, typing }: { time: string; typing?: boolean }) {
  return (
    <>
      <div className="tg-sb" aria-hidden="true">
        <span>{time}</span>
        <span className="tg-sbic">
          <svg width="17" height="11" viewBox="0 0 26 16"><rect x="0" y="11" width="4.5" height="5" rx="1" /><rect x="7" y="8" width="4.5" height="8" rx="1" /><rect x="14" y="4" width="4.5" height="12" rx="1" /><rect x="21" y="0" width="4.5" height="16" rx="1" /></svg>
          <svg width="16" height="11" viewBox="0 0 24 17"><path d="M12 16.5 8.4 12.9a5 5 0 0 1 7.2 0Z" /><path d="M4.6 9.2a10.6 10.6 0 0 1 14.8 0l-2 2a7.8 7.8 0 0 0-10.8 0Z" /><path d="M.8 5.4a16 16 0 0 1 22.4 0l-2 2a13.2 13.2 0 0 0-18.4 0Z" /></svg>
          <svg width="25" height="12" viewBox="0 0 38 18"><rect x=".5" y=".5" width="33" height="17" rx="5" fill="none" stroke="currentColor" opacity=".4" /><rect x="3" y="3" width="25" height="12" rx="3" /><rect x="35" y="6" width="2.5" height="6" rx="1" opacity=".4" /></svg>
        </span>
      </div>
      <div className="tg-head" aria-hidden="true">
        <span className="tg-back">
          <svg width="10" height="18" viewBox="0 0 16 28"><path d="M13 2 2 14l11 12" fill="none" stroke="currentColor" strokeWidth="3.4" strokeLinecap="round" strokeLinejoin="round" /></svg>
          Chats
        </span>
        <div className="tg-title">
          <b>Sol Dottie</b>
          <small>{typing ? <span className="tg-typing">typing<i>.</i><i>.</i><i>.</i></span> : "bot"}</small>
        </div>
        <img className="tg-avatar" src="/art/dottie.png" alt="" width={512} height={512} />
      </div>
    </>
  );
}

export function TelegramDemo() {
  const rootRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const reduced = useReducedMotion();
  const [index, setIndex] = useState(0);
  const [run, setRun] = useState(0);
  const [phase, setPhase] = useState<Phase>(empty);
  const script = scripts[index];

  useEffect(() => {
    const node = rootRef.current;
    if (!node) return;
    const observer = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting), { threshold: 0.3 });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (reduced || !visible) return;
    const timers: number[] = [];
    const at = (ms: number, fn: () => void) => timers.push(window.setTimeout(fn, ms));
    at(0, () => setPhase(empty));

    let t = 700;
    for (let i = 1; i <= script.user.length; i++) at(t + i * TYPE_MS, () => setPhase((p) => ({ ...p, typed: i })));
    t += script.user.length * TYPE_MS + 350;
    at(t, () => setPhase((p) => ({ ...p, typed: 0, sent: true })));
    const replies = script.steps.length + 1;
    for (let i = 1; i <= replies; i++) {
      t += 380;
      at(t, () => setPhase((p) => ({ ...p, typing: true })));
      t += i === replies ? ANSWER_THINK_MS : THINK_MS;
      at(t, () => setPhase((p) => ({ ...p, typing: false, shown: i })));
    }
    t += HOLD_MS;
    at(t, () => setPhase((p) => ({ ...p, leaving: true })));
    at(t + FADE_MS, () => { setIndex((i) => (i + 1) % scripts.length); setRun((r) => r + 1); });
    return () => timers.forEach(window.clearTimeout);
  }, [index, run, visible, reduced, script]);

  const pick = (i: number) => { setPhase(empty); setIndex(i); setRun((r) => r + 1); };
  const view = reduced ? finished(script) : phase;
  const typedText = script.user.slice(0, view.typed);
  const replies = [...script.steps.map((s) => [s]), script.answer];

  return (
    <div className="tg-demo" ref={rootRef}>
      <div className="tg-phone" role="img" aria-label={`Telegram chat with Sol Dottie. You: ${script.user}. Dottie: ${script.answer.join(" ").replace(/\*\*|`/g, "")}`}>
        <PhoneTop time={script.time} typing={view.typing} />
        <div className="tg-chat" aria-hidden="true">
          <div className={`tg-msgs${view.leaving ? " leaving" : ""}`} key={run}>
            {view.sent && <Bubble out time={script.time}>{script.user}</Bubble>}
            {replies.slice(0, view.shown).map((paras, i) => (
              <Bubble time={script.time} key={i}>
                {paras.map((para, j) => <span className="tg-p" key={j}><Rich text={para} /></span>)}
              </Bubble>
            ))}
          </div>
        </div>
        <div className="tg-input" aria-hidden="true">
          <span className="tg-menu">
            <svg width="13" height="11" viewBox="0 0 20 16"><path d="M2 2h16M2 8h16M2 14h16" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" /></svg>
            Menu
          </span>
          <div className="tg-field">
            {typedText ? <span>{typedText}</span> : null}
            {!view.sent && !reduced && <i className="tg-caret" />}
            {typedText ? null : <span className="tg-ph">Message</span>}
          </div>
          {typedText ? (
            <span className="tg-send"><svg width="13" height="14" viewBox="0 0 20 22"><path d="M10 20V3M3 10l7-7 7 7" fill="none" stroke="#fff" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" /></svg></span>
          ) : (
            <svg className="tg-mic" width="16" height="21" viewBox="0 0 24 32"><rect x="7" y="2" width="10" height="18" rx="5" fill="none" stroke="currentColor" strokeWidth="2.4" /><path d="M3 15a9 9 0 0 0 18 0M12 24v6" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" /></svg>
          )}
        </div>
        <div className="tg-home" aria-hidden="true"><i /></div>
      </div>

      <div className="tg-picks" role="group" aria-label="Pick a conversation">
        {scripts.map((s, i) => (
          <button key={s.id} type="button" className={i === index ? "on" : ""} aria-pressed={i === index} onClick={() => pick(i)}>
            {s.chip}
          </button>
        ))}
      </div>
    </div>
  );
}

/** A still of one conversation, laid on the page like Hoodit's clips. */
export function ChatStill({ script, art }: { script: Script; art?: string }) {
  const replies = [...script.steps.map((s) => [s]), script.answer];
  return (
    <div className={`chat-still${art ? " has-art" : ""}`} role="img" aria-label={`Sol Dottie answering: ${script.user}`}>
      {art ? <img className="chat-still-art" src={art} alt="" aria-hidden="true" /> : null}
      <div className="tg-phone tg-still">
        <PhoneTop time={script.time} />
        <div className="tg-chat" aria-hidden="true">
          <div className="tg-msgs">
            <Bubble out time={script.time}>{script.user}</Bubble>
            {replies.map((paras, i) => (
              <Bubble time={script.time} key={i}>
                {paras.map((para, j) => <span className="tg-p" key={j}><Rich text={para} /></span>)}
              </Bubble>
            ))}
          </div>
        </div>
        <div className="tg-input" aria-hidden="true">
          <span className="tg-menu"><svg width="13" height="11" viewBox="0 0 20 16"><path d="M2 2h16M2 8h16M2 14h16" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" /></svg>Menu</span>
          <div className="tg-field"><span className="tg-ph">Message</span></div>
          <svg className="tg-mic" width="16" height="21" viewBox="0 0 24 32"><rect x="7" y="2" width="10" height="18" rx="5" fill="none" stroke="currentColor" strokeWidth="2.4" /><path d="M3 15a9 9 0 0 0 18 0M12 24v6" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" /></svg>
        </div>
        <div className="tg-home" aria-hidden="true"><i /></div>
      </div>
    </div>
  );
}
