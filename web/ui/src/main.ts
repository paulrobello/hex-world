// Boots the overlay: the loading layer at once, then the play HUD once the engine is ready and the
// play scenario is up. Other scenarios (gates, look-dev) get no HUD and are polled once a second.
import { type AudioApi, GameAudio } from "./audio.ts";
import type { HexWorld, Volumes } from "./bridge.ts";
import { CameraKeeper, type RestoreStatus, localPoseStorage } from "./camera-pose.ts";
import { h } from "./dom.ts";
import { Hud } from "./hud.ts";
import { sprite } from "./icons.ts";
import { Loading } from "./loading.ts";
import { Persistence } from "./persist.ts";
import { OS_REDUCE, loadFrameCap, loadGrade, loadGrass, loadGrassDistance, loadMotion } from "./settings.ts";
import {
  gradeCommand,
  grassCommand,
  grassDistanceCommand,
  frameCapCommand,
  motionCommand,
  startupFrameCap,
  startupGrade,
  startupGrass,
  startupGrassDistance,
  tierBusy,
} from "./settings-model.ts";
import { loadWalkSpeed } from "./settings-world.ts";
import { idbFactory, openSlots } from "./slots.ts";
import { UpdateNote, watchVersion } from "./version-check.ts";
import { walkSpeedCommand } from "./walk-speed.ts";

type Bridge = HexWorld & {
  audio?: AudioApi;
  /** The viewer's camera restore: `ready` once the start-up resume has settled (scripts wait for `ready && !restoring`). */
  cameraRestore?: () => RestoreStatus & { ready: boolean };
};

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
  // A tier change before READY (detection, or the warm-up probe stepping down) is drawn first.
  const waitReady = (hw: HexWorld) => {
    if (!hw.ready() || tierBusy(hw.state().tier, null)) {
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
  // Engine motion (the staged rise, shake) follows the OS reduced-motion setting, at start and on change,
  // unless the viewer's Reduce motion choice (Settings, Controls) is On.
  const reduced = matchMedia(OS_REDUCE);
  const syncMotion = () => hw.cmd(motionCommand(loadMotion(), reduced.matches));
  syncMotion();
  reduced.addEventListener("change", syncMotion);
  // The viewer's colour grade; a `grade` key in the address wins and leaves the stored choice alone.
  const grade = startupGrade(location.search, loadGrade());
  if (grade) hw.cmd(gradeCommand(grade));
  // The viewer's grass density, the same way: a `grass` key in the address wins for the visit.
  const grass = startupGrass(location.search, loadGrass());
  if (grass != null) hw.cmd(grassCommand(grass));
  // And its grass distance: a `grass_distance` key in the address wins for the visit.
  const grassDistance = startupGrassDistance(location.search, loadGrassDistance());
  if (grassDistance) hw.cmd(grassDistanceCommand(grassDistance));
  // The viewer's frame rate cap: a `frame_cap` key in the address wins for the visit.
  const frameCap = startupFrameCap(location.search, loadFrameCap());
  if (frameCap) hw.cmd(frameCapCommand(frameCap));
  // The viewer's walk speed, sent only when one is stored (the engine starts at its default).
  const walkSpeed = loadWalkSpeed();
  if (walkSpeed != null) hw.cmd(walkSpeedCommand(walkSpeed));
  let hud: Hud | null = null;
  let persist: Persistence | null = null;
  // A newer deployed build offers a reload, which writes the world first (checked in play only).
  const update = new UpdateNote(root, () => persist);
  // This viewer's camera per saved world (localStorage), outside the world save and its history.
  const camera = new CameraKeeper(hw, { storage: localPoseStorage });
  hw.cameraRestore = () => ({ ready: persist?.ready ?? false, ...camera.status() });
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
        changed: () => {
          h.syncWorld();
          update.sync();
        },
        opened: (id) => camera.begin(id),
        switching: () => camera.flush(),
        removed: (id) => camera.forget(id),
      });
      h.attach(persist);
      const p = persist;
      void p.start().then(() => {
        if (p.storageOn) camera.keepOnly(p.slots.map((s) => s.id));
      });
    });
  };
  // Leaving or hiding the page writes a pending autosave at once (best effort in pagehide), and the camera.
  addEventListener("visibilitychange", () => {
    if (document.visibilityState !== "hidden") return;
    persist?.flushNow();
    camera.flush();
  });
  addEventListener("pagehide", () => {
    persist?.flushNow();
    camera.flush();
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
    if (!hud) {
      hud = new Hud(root, hw, hw.audio ?? null);
      startPersistence(hud);
      watchVersion(update);
    }
    persist?.observe(play.tiles);
    camera.tick(persist?.ready && persist.current.stored ? persist.current.id : null, state);
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
