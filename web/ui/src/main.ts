// Boots the overlay: the loading layer at once, then the play HUD once the engine is ready and the
// play scenario is up. Other scenarios (gates, look-dev) get no HUD and are polled once a second.
import { type AudioApi, GameAudio } from "./audio.ts";
import type { HexWorld, Volumes } from "./bridge.ts";
import { h } from "./dom.ts";
import { Hud } from "./hud.ts";
import { sprite } from "./icons.ts";
import { Loading } from "./loading.ts";
import { Persistence } from "./persist.ts";
import { loadGrade } from "./settings.ts";
import { gradeCommand, startupGrade } from "./settings-model.ts";
import { idbFactory, openSlots } from "./slots.ts";

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
  // The viewer's colour grade; a `grade` key in the address wins and leaves the stored choice alone.
  const grade = startupGrade(location.search, loadGrade());
  if (grade) hw.cmd(gradeCommand(grade));
  let hud: Hud | null = null;
  let persist: Persistence | null = null;
  hw.watchEvents((batch) => {
    hud?.onEvents(batch);
    audio.onEvents(batch);
    persist?.onEvents(batch);
  });
  const startPersistence = (h: Hud) => {
    void openSlots(idbFactory()).then((store) => {
      persist = new Persistence(hw, store, {
        volumes: () => busVolumes(audio.api().state().volumes),
        download,
        notify: (n) => h.note(n),
        changed: () => h.syncWorld(),
      });
      h.attach(persist);
      void persist.start();
    });
  };
  // Leaving or hiding the page writes a pending autosave at once (best effort in pagehide).
  addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") persist?.flushNow();
  });
  addEventListener("pagehide", () => persist?.flushNow());
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
    if (!hud) {
      hud = new Hud(root, hw, hw.audio ?? null);
      startPersistence(hud);
    }
    persist?.observe(play.tiles);
    root.classList.remove("is-off");
    hud.render(play, state);
    requestAnimationFrame(tick);
  };
  tick();
}

/** The audio layer's buses as a save spells them. */
function busVolumes(v: {
  master: number;
  music: number;
  ambience: number;
  effects: number;
  interface: number;
}): Volumes {
  return { master: v.master, music: v.music, sfx: v.effects, ambience: v.ambience, ui: v.interface };
}

/** Saves `bytes` as a download named `name`. */
function download(name: string, bytes: Uint8Array): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: "application/octet-stream" }));
  const a = h("a", { href: url, download: name, class: "sr" });
  document.body.append(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

if (document.body) boot();
else addEventListener("DOMContentLoaded", boot, { once: true });
