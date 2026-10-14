// Boots the overlay: the loading layer at once, then the play HUD once the engine is ready and the
// play scenario is up. Other scenarios (gates, look-dev) get no HUD and are polled once a second.
import { type AudioApi, GameAudio } from "./audio.ts";
import type { HexWorld } from "./bridge.ts";
import { h } from "./dom.ts";
import { Hud } from "./hud.ts";
import { sprite } from "./icons.ts";
import { Loading } from "./loading.ts";

type Bridge = HexWorld & { audio?: AudioApi };

declare global {
  interface Window {
    hexWorld?: Bridge;
  }
}

const START_TIMEOUT_MS = 120_000;

function boot(): void {
  const root = h("div", { id: "hud", class: "hud is-off" });
  root.append(sprite());
  document.body.append(root);
  const loading = new Loading(root);
  const t0 = performance.now();

  const failEarly = (e: PromiseRejectionEvent) => {
    if (window.hexWorld) return;
    const msg = String((e.reason as Error | undefined)?.message ?? e.reason ?? "");
    if (/fetch|wasm|webassembly|network|compile/i.test(msg))
      loading.failed("The engine stopped downloading", "Retry picks up where it stopped.");
  };
  addEventListener("unhandledrejection", failEarly);

  const waitEngine = () => {
    const hw = window.hexWorld;
    if (!hw) {
      if (performance.now() - t0 > START_TIMEOUT_MS)
        return loading.failed("Hex World is taking too long to start", "Check the connection, then retry.");
      return void window.setTimeout(waitEngine, 50);
    }
    removeEventListener("unhandledrejection", failEarly);
    loading.show("renderer");
    waitReady(hw);
  };
  const waitReady = (hw: HexWorld) => {
    if (!hw.ready()) {
      if (performance.now() - t0 > START_TIMEOUT_MS)
        return loading.failed("The renderer did not start", "This graphics adapter may have stopped. Retry once.");
      return void requestAnimationFrame(() => waitReady(hw));
    }
    loading.ready();
    run(root, hw);
  };
  waitEngine();
}

function run(root: HTMLElement, hw: Bridge): void {
  // Audio unlocks on the first press in play. Its listeners go in before the HUD's, which stop game keys.
  const audio = new GameAudio();
  audio.listen();
  hw.audio = audio.api();
  // Engine motion (the staged rise, shake) follows the OS reduced-motion setting, at start and on change.
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  const syncMotion = () => hw.cmd({ cmd: "set_reduced_motion", on: reduced.matches });
  syncMotion();
  reduced.addEventListener("change", syncMotion);
  let hud: Hud | null = null;
  hw.watchEvents((batch) => {
    hud?.onEvents(batch);
    audio.onEvents(batch);
  });
  const tick = () => {
    // Pump before reading state, so the state is at least as new as the events it explains.
    hw.pumpEvents();
    const state = hw.state();
    const play = state.play ?? null;
    audio.update(play, state);
    if (!play) {
      hud?.sleep();
      root.classList.add("is-off");
      window.setTimeout(tick, 1000);
      return;
    }
    if (!hud) hud = new Hud(root, hw);
    root.classList.remove("is-off");
    hud.render(play);
    requestAnimationFrame(tick);
  };
  tick();
}

if (document.body) boot();
else addEventListener("DOMContentLoaded", boot, { once: true });
