.PHONY: clean ui-build ui-typecheck ui-lint ui-test ui-schema ui-check ui-e2e fetch-tools fetch-hdri assets-p0 run shot run-shot run-diag shots web-shots fmt fmt-check lint typecheck test checkall checkall-ci ref-bevy build-web web-size web-smoke perf-gate perf-matrix shots-chunks shots-instanced blender-test blender-test-selfcheck refs kit-build kit-determinism kit-compress kit-material kit-compress-test kit-fixture kit-renders kit-check kit-provenance kit-material-provenance shots-kit web-shots-kit lookdev-ibl shots-lookdev shots-tilt web-shots-tilt shots-water perf-water perf-passes initial-download shots-play shots-cars brand audio-master audio-check audio-check-selftest shots-water-play thumbs shots-rise shots-coast shots-wet-band audio-e2e shots-boids shots-photo shots-photo-fx shots-scatter web-scatter perf-water-world shots-ghost shots-bulldoze-dust shots-river-mouth shots-grass perf-grass shots-first-person shots-foliage-wind shots-windmill-play shots-farm-play

fmt:
	cargo fmt --all

clean:
	rm -rf dist target/kit-stage
	@for dir in web/ui scripts/assets target; do \
	  if test -d "$$dir"; then find "$$dir" -type f -name '*.stamp' -delete; fi; done

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

typecheck:
	cargo check --workspace --all-targets
	cargo check -p hex-app --target wasm32-unknown-unknown

test:
	cargo test --workspace

.PHONY: check-wgpu
# An unqualified inverse query fails if Cargo resolves more than one wgpu version.
check-wgpu:
	@tree=$$(cargo tree --locked -i wgpu --prefix none --depth 0) || exit 1; \
	count=$$(printf '%s\n' "$$tree" | grep -c '^wgpu v'); \
	test "$$count" = 1 || { echo "FAIL: expected one wgpu version, got $$count"; exit 1; }; \
	printf 'single wgpu OK: %s\n' "$$tree"

checkall: fmt-check lint typecheck test ui-check

# UI overlay (web/ui, PRD D18 and §14): TypeScript bundled by esbuild into web/ui/dist, which Trunk
# ships as web/ui/ (index.html). Tools are pinned in web/ui/package-lock.json; npm ci runs only when
# the lockfile is newer than the last install. Unit tests run .ts directly (Node 22.18+ type stripping).
UI = web/ui
UI_NPM = $(UI)/node_modules/.npm-ci.stamp
$(UI_NPM): $(UI)/package-lock.json
	cd $(UI) && npm ci --no-audit --no-fund
	touch $@

# Also Trunk's pre_build hook (Trunk.toml), so trunk build and trunk serve always carry the current HUD.
ui-build: $(UI_NPM)
	node $(UI)/build.mjs

ui-typecheck: $(UI_NPM)
	cd $(UI) && ./node_modules/.bin/tsc --noEmit -p .

ui-lint: $(UI_NPM)
	cd $(UI) && ./node_modules/.bin/biome check .

ui-test:
	node --test "$(UI)/test/*.test.ts" "scripts/web/*.test.mjs" scripts/assets/kit-provenance.test.mjs scripts/shots/run.test.mjs

ui-check: ui-typecheck ui-lint ui-test ui-build

# Rewrites the bridge contract (web/ui/src/bridge-schema.json) and the fit-rule parity fixture
# (web/ui/test/rules-parity.json) from the Rust enums and hex-core; cargo test fails while they are stale.
ui-schema:
	HEXW_UPDATE_SCHEMA=1 cargo test -p hex-app bridge::schema::schema_matches_web_ui

# Local only (GPU Chromium, never CI): drives the HUD with real keyboard and mouse on the built site,
# asserts through hexWorld.state() and writes e2e/test-results/ui/<mockup state>.jpg, then e2e/photo-fx.mjs
# (photo effects from the panel, measured in exported PNGs; e2e/test-results/photo-fx/), then e2e/persist.mjs
# (autosave, reload, export, import, storage blocked; e2e/test-results/persistence/, or PERSIST_OUT), then runs the
# audio checks (audio-e2e), then e2e/debug.mjs (the Settings Debug tab and every developer overlay;
# e2e/test-results/debug/, or DEBUG_OUT=docs/checkpoints/debug for the committed shots), then e2e/version.mjs
# (the new-version note: a swapped version.json, Reload saving first and loading the new build;
# e2e/test-results/version/, or VERSION_OUT=docs/checkpoints/version-check for the committed shots).
ui-e2e: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/ui.mjs dist
	node e2e/photo-fx.mjs dist
	node e2e/persist.mjs dist
	node e2e/audio.mjs dist
	node e2e/debug.mjs dist
	node e2e/version.mjs dist

# Local only: the web audio layer (PRD §8) read through hexWorld.audio.state(): nothing loads before a
# gesture, the context runs after one, events fire their cues, the focus mix, and mute. Writes
# e2e/test-results/audio/*.json.
audio-e2e: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/audio.mjs dist

# initial-download runs after web-size, on the dist/ it just built (not a sibling prerequisite: under
# make -j it would race web-size's rm -rf dist).
checkall-ci: checkall check-wgpu web-size
	@$(MAKE) --no-print-directory initial-download

fetch-tools:
	bash scripts/fetch-ktx.sh

BLENDER ?= /Applications/Blender.app/Contents/MacOS/Blender
BLENDER_RUN = "$(BLENDER)" -b --factory-startup --python-exit-code 1

blender-test:
	$(BLENDER_RUN) -P scripts/blender/run_tests.py -- $(TEST)

# Proves a failing Blender test fails make (brief §(e): Blender exits 0 on tracebacks without the flag).
blender-test-selfcheck:
	@mkdir -p target
	@if HEXW_FORCE_FAIL=1 $(BLENDER_RUN) -P scripts/blender/run_tests.py -- test_runner_selfcheck > target/selfcheck.log 2>&1; \
	then echo "FAIL: forced failure exited 0"; exit 1; \
	elif grep -q "forced failure" target/selfcheck.log; then echo "selfcheck OK: failure propagates"; \
	else echo "FAIL: Blender did not run the forced-failure test"; cat target/selfcheck.log; exit 1; fi

refs:
	bash scripts/refs/higgsfield-refs.sh $(PIECE)

assets-p0: fetch-tools
	cd scripts/assets && npm ci --no-audit --no-fund
	node scripts/assets/build-p0-assets.mjs
	mkdir -p target
	. tools/ktx-env.sh; \
	ktx create --format R8G8B8A8_SRGB --encode uastc --uastc-quality 2 --zstd 18 --generate-mipmap assets/kit/p0/hex_test.png target/hex_test.uastc.ktx2 && \
	ktx transcode --target bc7 --zstd 18 target/hex_test.uastc.ktx2 assets/kit/p0/hex_test.bc7.ktx2 && \
	ktx transcode --target astc --zstd 18 target/hex_test.uastc.ktx2 assets/kit/p0/hex_test.astc.ktx2 && \
	ktx info assets/kit/p0/hex_test.bc7.ktx2 | head -20

# P1 kit pipeline. Local only: needs Blender and tools/ktx-4.4.2. Per-channel color space lives in
# scripts/assets/kit-lib.mjs CHANNELS (albedo/emissive sRGB; normal/orm/masks linear), asserted on every encode.
kit-build:
	@test -n "$(PIECE)" || { echo "usage: make kit-build PIECE=<id>"; exit 2; }
	rm -rf target/kit-stage/$(PIECE) && mkdir -p target/kit-stage/$(PIECE) assets-source/kit/$(PIECE)
	$(BLENDER_RUN) -P scripts/blender/kit/$(PIECE).py -- target/kit-stage/$(PIECE)
	cp target/kit-stage/$(PIECE)/bake-report.json target/kit-stage/$(PIECE)/export-report.json assets-source/kit/$(PIECE)/

kit-determinism:
	@test -n "$(PIECE)" || { echo "usage: make kit-determinism PIECE=<id>"; exit 2; }
	rm -rf target/kit-det && mkdir -p target/kit-det/a target/kit-det/b
	HEXW_SKIP_BAKE=1 $(BLENDER_RUN) -P scripts/blender/kit/$(PIECE).py -- target/kit-det/a
	HEXW_SKIP_BAKE=1 $(BLENDER_RUN) -P scripts/blender/kit/$(PIECE).py -- target/kit-det/b
	@a=$$(shasum -a 256 target/kit-det/a/$(PIECE).glb | cut -d' ' -f1); b=$$(shasum -a 256 target/kit-det/b/$(PIECE).glb | cut -d' ' -f1); \
	test "$$a" = "$$b" && echo "deterministic $$a" || { echo "FAIL: GLB differs $$a $$b"; exit 1; }

# npm ci only when the lockfile is newer than the last install.
ASSETS_NPM = scripts/assets/node_modules/.npm-ci.stamp
$(ASSETS_NPM): scripts/assets/package-lock.json
	cd scripts/assets && npm ci --no-audit --no-fund
	touch $@

# CLASS=piece|hero|tileable overrides the budget class (default: export-report.json "budgetClass", else piece).
kit-compress: fetch-tools $(ASSETS_NPM)
	@test -n "$(PIECE)" || { echo "usage: make kit-compress PIECE=<id> [CLASS=hero]"; exit 2; }
	node scripts/assets/kit-compress.mjs piece $(PIECE) $(if $(CLASS),--class $(CLASS))

kit-material: fetch-tools $(ASSETS_NPM)
	@test -n "$(SET)" || { echo "usage: make kit-material SET=<set>"; exit 2; }
	rm -rf target/kit-stage/materials/$(SET) && mkdir -p target/kit-stage/materials/$(SET) assets-source/kit/materials/$(SET)
	$(BLENDER_RUN) -P scripts/blender/kit/mat_$(SET).py -- target/kit-stage/materials/$(SET)
	node scripts/assets/kit-compress.mjs material $(SET)
	cp target/kit-stage/materials/$(SET)/bake-report.json assets-source/kit/materials/$(SET)/

kit-compress-test: fetch-tools $(ASSETS_NPM)
	node --test scripts/assets/kit-compress.test.mjs scripts/assets/kit-provenance.test.mjs

# Rebuilds the committed gate fixture. Local only: needs Blender and tools/ktx-4.4.2.
# Then re-syncs assets/kit/p1/fixture, the byte copy kit_src=fixture serves (served_fixture_matches_the_gate_fixture).
KIT_FIXTURE = crates/hex-render/tests/fixtures/kit
kit-fixture: fetch-tools $(ASSETS_NPM)
	rm -rf target/kit-stage/fixture && mkdir -p target/kit-stage/fixture
	$(BLENDER_RUN) -P scripts/blender/kit/fixture.py -- target/kit-stage/fixture
	node scripts/assets/kit-compress.mjs piece fixture --out $(KIT_FIXTURE)/p1/fixture --report $(KIT_FIXTURE)/source/fixture/compress-report.json
	cp target/kit-stage/fixture/bake-report.json target/kit-stage/fixture/export-report.json $(KIT_FIXTURE)/source/fixture/
	node scripts/assets/kit-provenance.mjs fixture --source $(KIT_FIXTURE)/source/fixture --runtime $(KIT_FIXTURE) --validated "fixture gate"
	cp $(KIT_FIXTURE)/p1/fixture/* assets/kit/p1/fixture/

kit-renders:
	@test -n "$(PIECE)" || { echo "usage: make kit-renders PIECE=<id>"; exit 2; }
	$(BLENDER_RUN) -P scripts/blender/kit_render.py -- $(PIECE)

# The look-dev HDRI is gitignored (CC0, Poly Haven): download it into this checkout's
# assets-source/hdri/ and verify it against the URL and SHA-256 recorded in lookdev.json.
LOOKDEV = assets-source/lookdev/lookdev.json
fetch-hdri:
	@dst=$$(node -p 'require("./$(LOOKDEV)").hdri.file'); want=$$(node -p 'require("./$(LOOKDEV)").hdri.sha256'); \
	test -f "$$dst" || { mkdir -p "$$(dirname "$$dst")" && curl -fL --max-time 300 -o "$$dst.part" "$$(node -p 'require("./$(LOOKDEV)").hdri.url')" && mv "$$dst.part" "$$dst"; } || exit 1; \
	got=$$(shasum -a 256 "$$dst" | cut -d' ' -f1); \
	if [ "$$got" = "$$want" ]; then echo "hdri OK $$dst"; else echo "FAIL: $$dst sha256 $$got != $$want"; exit 1; fi

lookdev-ibl: fetch-hdri
	$(BLENDER_RUN) -P scripts/blender/lookdev_ibl.py -- assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr
	. tools/ktx-env.sh && ktx validate assets/kit/p1/ibl/noon.specular.ktx2 && ktx validate assets/kit/p1/ibl/noon.diffuse.ktx2

# The kit gate alone. checkall already runs it through `test` (cargo test --workspace). The second run
# prints the stale-generator warnings (edits since provenance warn, never fail: owner 2026-10-19).
kit-check:
	cargo test -p hex-play --test kit_gate
	@cargo test -q -p hex-play --test kit_gate -- provenance_complete --nocapture 2>&1 | grep '^warning: ' || true

# Writes provenance once, with validation.pass, after every non-provenance gate passes. The first call
# only checks (--check writes nothing) and refuses a missing build stage, which kit_render.py needs.
# ROUTE=interactive defaults BLEND to the piece's .blend in the main checkout's store (owner O2,
# scripts/blender/lib/paths.py; HEXW_BLEND_STORE overrides). Scripted pieces get no blendRef.
BLEND_STORE = $(shell python3 scripts/blender/lib/paths.py store)
KIT_BLEND = $(or $(BLEND),$(if $(filter interactive,$(ROUTE)),$(shell python3 scripts/blender/lib/paths.py blend $(PIECE))))
KIT_PROV_ARGS = $(if $(REFS),--refs $(REFS)) $(if $(KIT_BLEND),--blend $(KIT_BLEND) --blend-store $(BLEND_STORE)) $(if $(ROUTE),--route $(ROUTE))
kit-provenance:
	@test -n "$(PIECE)" || { echo "usage: make kit-provenance PIECE=<id> [REFS=assets-source/refs/<ref>] [ROUTE=interactive [BLEND=path]]"; exit 2; }
	@if [ "$(ROUTE)" = interactive ] && [ ! -f "$(KIT_BLEND)" ]; then \
	  echo "kit-provenance: ROUTE=interactive needs the piece's .blend, got '$(KIT_BLEND)' (BLEND=, or python3 scripts/blender/lib/paths.py blend $(PIECE))"; exit 2; fi
	node scripts/assets/kit-provenance.mjs $(PIECE) $(KIT_PROV_ARGS) --stage target/kit-stage/$(PIECE) --check
	cargo test -p hex-play --test kit_gate -- --skip provenance
	$(BLENDER_RUN) -P scripts/blender/kit_render.py -- $(PIECE)
	node scripts/assets/kit-provenance.mjs $(PIECE) $(KIT_PROV_ARGS) --stage target/kit-stage/$(PIECE) \
	  --validated "kit_gate geometry,kit_gate textures,kit_gate bevy reimport,blender reimport sockets,six-view render"

# Provenance for a tileable set: assets-source/kit/materials/<set>/provenance.json.
# The set must be in assets/kit/manifest.json "materials" before validation.pass is stamped.
kit-material-provenance:
	@test -n "$(SET)" || { echo "usage: make kit-material-provenance SET=<set>"; exit 2; }
	node scripts/assets/kit-provenance.mjs $(SET) --material --check
	cargo test -p hex-play --test kit_gate ktx2_color_space_mips_and_budgets
	node scripts/assets/kit-provenance.mjs $(SET) --material --validated "kit-compress tileable budget,kit_gate textures"

ref-bevy:
	@test -d target/ref/bevy || git clone --depth 1 --branch v0.20.0 https://github.com/bevyengine/bevy target/ref/bevy

build-web:
	trunk build --release

web-size:
	@rm -rf dist
	@$(MAKE) --no-print-directory build-web
	@wasm=$$(ls dist/*.wasm); raw=$$(wc -c < $$wasm | tr -d ' '); gz=$$(gzip -9 -c $$wasm | wc -c | tr -d ' '); \
	echo "wasm raw=$$raw gzip=$$gz"; \
	test $$gz -le 12582912 || { echo "FAIL: wasm gzip $$gz > 12 MiB budget"; exit 1; }

# PRD §10/§15 P2: wasm + JS + shell + shaders + IBL + core kit pieces + material sets, one texture
# family, as served by Pages, <= 30 MiB for the larger of BC7 and ASTC. Measures whatever dist/
# holds now and builds it only when missing; run web-size or build-web first to refresh it.
initial-download:
	@test -d dist || $(MAKE) --no-print-directory build-web
	node scripts/size/initial-download.mjs dist

# App icons, favicon.ico, manifest icons, social share image and the concept sheet, rebuilt
# byte-identically from design/brand/ sources. Needs resvg 0.48.1 (brew install resvg).
brand:
	node scripts/brand/build.mjs

web-smoke: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/smoke.mjs dist

# Local only (GPU Chromium): ghost evidence for owner feedback U1, written to docs/checkpoints/ghost/.
shots-ghost: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/ghost-shots.mjs dist

perf-gate: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/perf-gate.mjs dist

perf-matrix: build-web
	cargo build -p hex-app --release
	cd e2e && npm ci --no-audit --no-fund
	node e2e/perf-matrix.mjs dist target/release/hex-world

run:
	cargo run -p hex-app --release -- $(ARGS)

shot:
	mkdir -p target/shots
	rm -f target/shots/single.png
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --shot single --exit-after 180
	@test -s target/shots/single.png && echo "wrote target/shots/single.png"

run-shot:
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --shot single --exit-after 180

run-diag:
	cargo run -p hex-app --release -- --scenario gate2000 --diag --dump-state target/state.json --exit-after-seconds 10
	@test -s target/state.json && cat target/state.json && echo

shots:
	mkdir -p target/shots
	rm -f target/shots/gallery-*.png target/shots/gallery-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --script scripts/shots/gallery.json --size 1280x800 --seed 1 --fps off --tilt off
	@for n in single single-low gate-strategic gate-closeup gate-flock; do test -s target/shots/gallery-$$n.png || { echo "missing gallery-$$n.png"; exit 1; }; done
	@echo "wrote target/shots/gallery-*.png"

shots-chunks:
	mkdir -p target/shots
	rm -f target/shots/gallery-chunks-*.png target/shots/gallery-chunks-*.json
	sed 's/"gallery-/"gallery-chunks-/' scripts/shots/gallery.json > target/shots/gallery-chunks-script.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --script target/shots/gallery-chunks-script.json --size 1280x800 --seed 1 --fps off --tilt off --layout chunks
	@for n in single single-low gate-strategic gate-closeup gate-flock; do test -s target/shots/gallery-chunks-$$n.png || { echo "missing gallery-chunks-$$n.png"; exit 1; }; done
	@echo "wrote target/shots/gallery-chunks-*.png (layout=chunks)"

shots-instanced:
	mkdir -p target/shots
	rm -f target/shots/gallery-instanced-*.png target/shots/gallery-instanced-*.json
	sed 's/"gallery-/"gallery-instanced-/' scripts/shots/gallery.json > target/shots/gallery-instanced-script.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --script target/shots/gallery-instanced-script.json --size 1280x800 --seed 1 --fps off --tilt off --layout instanced
	@for n in single single-low gate-strategic gate-closeup gate-flock; do test -s target/shots/gallery-instanced-$$n.png || { echo "missing gallery-instanced-$$n.png"; exit 1; }; done
	@echo "wrote target/shots/gallery-instanced-*.png (layout=instanced)"

web-shots: build-web
	cd e2e && npm ci --no-audit --no-fund
	mkdir -p e2e/test-results
	rm -f e2e/test-results/web-*.png
	node e2e/shots.mjs dist scripts/shots/gallery.json

# The kit scenario's shots. ARGS / QUERY pass kit keys, e.g. ARGS="--kit_src fixture" and
# QUERY="kit_src=fixture" for the gate fixture.
KIT_SHOTS = kit-overview kit-seam-closeup kit-tree kit-tree-under kit-tree-game kit-lod1 kit-lod2
# Foliage wind evidence (PRD §9.1): fixed-clock frame pairs of a pine forest, the hero oak, its
# shadow, a river reed bed and wildflowers, with a difference image per pair, into
# docs/checkpoints/foliage-wind/ (shoot.sh).
shots-foliage-wind:
	cargo build -p hex-app --release
	bash scripts/shots/run.sh docs/checkpoints/foliage-wind/shoot.sh target/release/hex-world

shots-kit:
	mkdir -p target/shots
	rm -f target/shots/kit-*.png
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --script scripts/shots/kit.json --size 1600x1000 --seed 1 --fps off --tilt off $(ARGS)
	@for n in $(KIT_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; done
	@echo "wrote target/shots/kit-*.png"

# Tone-mapping comparison (D4) of the kit shots over the `tony` grade's exposure, grading and
# backdrop. ARGS passes kit keys and LOOKDEV_OUT moves the output, e.g. ARGS="--kit_src fixture"
# LOOKDEV_OUT=target/shots/lookdev-fixture.
LOOKDEV_OUT ?= target/shots
shots-lookdev:
	@mkdir -p $(LOOKDEV_OUT)
	@for t in agx tony aces; do \
	  rm -f target/shots/kit-*.png; \
	  bash scripts/shots/run.sh cargo run -p hex-app --release -- --script scripts/shots/kit.json --size 1600x1000 --seed 1 --fps off --tilt off --grade tony --tonemap $$t $(ARGS) || exit 1; \
	  for n in kit-overview kit-seam-closeup kit-tree; do cp target/shots/$$n.png $(LOOKDEV_OUT)/lookdev-$$t-$$n.png || exit 1; done; \
	done
	@echo "wrote $(LOOKDEV_OUT)/lookdev-{agx,tony,aces}-*.png"

# Tilt-shift off vs on (PRD §9.1): same cameras and seed, only `--tilt` differs. Writes
# docs/shots/tilt-shift/tilt-{off,on}-<view>.png.
TILT_VIEWS = overview mid wide
TILT_OUT = docs/shots/tilt-shift
shots-tilt:
	@mkdir -p target/shots $(TILT_OUT)
	@for m in off on; do \
	  rm -f target/shots/tilt-*.png; \
	  bash scripts/shots/run.sh cargo run -p hex-app --release -- --script scripts/shots/tilt.json --size 1600x1000 --seed 1 --fps off --tilt $$m $(ARGS) || exit 1; \
	  for n in $(TILT_VIEWS); do test -s target/shots/tilt-$$n.png || { echo "missing tilt-$$n.png"; exit 1; }; cp target/shots/tilt-$$n.png $(TILT_OUT)/tilt-$$m-$$n.png; done; \
	done
	@echo "wrote $(TILT_OUT)/tilt-{off,on}-*.png"

# The same pair in Chromium on WebGPU: e2e/test-results/web-tilt-{off,on}-<view>.png.
web-shots-tilt: build-web
	cd e2e && npm ci --no-audit --no-fund
	mkdir -p e2e/test-results
	rm -f e2e/test-results/web-tilt-*.png
	@for m in off on; do \
	  node e2e/shots.mjs dist scripts/shots/tilt.json "tilt=$$m" || exit 1; \
	  for n in $(TILT_VIEWS); do mv e2e/test-results/web-tilt-$$n.png e2e/test-results/web-tilt-$$m-$$n.png; done; \
	done

web-shots-kit: build-web
	cd e2e && npm ci --no-audit --no-fund
	mkdir -p e2e/test-results
	rm -f e2e/test-results/web-kit-*.png
	node e2e/shots.mjs dist scripts/shots/kit.json "$(QUERY)"

# Water look-dev owner checkpoint (PRD P2): stills at 1600x1000 (lossless PNGs stay in target/shots,
# JPEG copies go to docs/checkpoints/water/), then a 3 s MP4 per view (90 frames at a fixed 1/30 s
# water step, 960x600) in the same folder.
# ARGS passes app keys, e.g. ARGS="--tier medium".
WATER_SHOTS = water-overview water-ocean water-river water-beach water-seam-ocean water-seam-river water-edge water-edge-river
WATER_VIEWS = ocean river beach seam-ocean seam-river edge edge-river
WATER_DOCS = docs/checkpoints/water
shots-water:
	mkdir -p target/shots $(WATER_DOCS)
	rm -f target/shots/water-*.png
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --script scripts/shots/water.json --size 1600x1000 --seed 1 --fps off --tilt off --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(WATER_DOCS) $(WATER_SHOTS)
	node scripts/shots/water-motion.mjs target/shots/water-motion-script.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --script target/shots/water-motion-script.json --size 960x600 --seed 1 --fps off --tilt off --water_clock fixed $(ARGS)
	@for v in $(WATER_VIEWS); do \
	  test -s target/shots/water-motion-$$v-089.png || { echo "missing water-motion-$$v frames"; exit 1; }; \
	  ffmpeg -loglevel error -y -framerate 30 -i target/shots/water-motion-$$v-%03d.png -c:v libx264 -pix_fmt yuv420p -crf 22 $(WATER_DOCS)/water-motion-$$v.mp4 || exit 1; \
	done
	rm -f target/shots/water-motion-*.png
	@echo "wrote $(WATER_DOCS)/"

# Water frame time at 2560x1440, web (uncapped GPU Chromium) and native, under the host-load guard.
# WATER_TIERS=high,low (default) and WATER_ONLY=web|native narrow the run.
perf-water: build-web
	cargo build -p hex-app --release
	cd e2e && npm ci --no-audit --no-fund
	node e2e/perf-water.mjs dist target/release/hex-world

# P2 groundwork on the scripted 2,000-tile world (gate2000, instanced, mid600 shadows, High, 2560x1440),
# web and native, each run inside the host-load guard: the p95 budget run, the tilt-shift cost
# (tilt on vs off), per-pass GPU times (gpu_timing=on) and the >25 ms stall tail; writes
# docs/perf/p2-passes.json. PASSES_ONLY=web|native and PASSES_ROUNDS narrow the run.
perf-passes: build-web
	cargo build -p hex-app --release
	cd e2e && npm ci --no-audit --no-fund
	node e2e/perf-passes.mjs dist target/release/hex-world

# Water- and scatter-heavy play workload (P2 re-run): the deterministic 208-tile world of
# e2e/water-world.mjs (ocean, coast, rivers, forests, meadows) built by script in play on High at
# 2560x1440, measured after every rise settles, web (uncapped GPU Chromium) and native, tilt-shift on
# and off, each run inside the host-load guard. Writes docs/perf/p2-water-world.json (PERF_OUT
# overrides). WW_ONLY=web|native and WW_ROUNDS narrow the run.
perf-water-world: build-web
	cargo build -p hex-app --release
	cd e2e && npm ci --no-audit --no-fund
	caffeinate -d -u node e2e/perf-water-world.mjs dist target/release/hex-world

# Grass cost (crates/hex-play/src/play/grass.rs): e2e/perf-grass.mjs builds the ~2,000-tile play
# water world (radius 26) at 2560x1440 on High and reads uncapped GPU Chromium from a low camera over
# the start meadows with grass off, normal and lush, every run inside the host-load guard; writes
# docs/perf/p2-grass.json (PERF_OUT, GRASS_ROUNDS, GRASS_RADIUS, GRASS_ARMS).
perf-grass: build-web
	cd e2e && npm ci --no-audit --no-fund
	caffeinate -d -u node e2e/perf-grass.mjs dist

# Play scenario checkpoint (P2 placement loop): runs the scripted proof docs/checkpoints/play/play.json
# at 1280x800 with tilt-shift off, then copies JPEG stills and the state dumps next to it.
PLAY_SHOTS = play-01-start play-02-ghost-road play-03-placed play-04-illegal-ghost play-05-undo play-06-redo play-07-bulldoze-leaf play-08-bulldoze-bridge
PLAY_DOCS = docs/checkpoints/play
shots-play:
	mkdir -p target/shots $(PLAY_DOCS)
	rm -f target/shots/play-*.png target/shots/play-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(PLAY_DOCS)/play.json --size 1280x800 --seed 1 --fps off --tilt off $(ARGS)
	bash scripts/shots/run.sh --jpegs $(PLAY_DOCS) $(PLAY_SHOTS)
	cp target/shots/play-*.json $(PLAY_DOCS)/
	@echo "wrote $(PLAY_DOCS)/"

# Windmill tile in play: docs/checkpoints/windmill/windmill-play.json places the windmill tile among
# meadows and an oak, shoots it twice 60 frames (1 s, 24° of sail at 4 rpm) apart, then walks into the
# tower and dumps the block. 1280x800, tilt-shift off and `--clock fixed`, so the sails replay exactly.
# Native shots can come back black (idle display, high load): the target fails then and leaves the
# committed stills alone.
WINDMILL_SHOTS = windmill-play windmill-play-1s windmill-walk-blocked
WINDMILL_DOCS = docs/checkpoints/windmill
shots-windmill-play:
	mkdir -p target/shots $(WINDMILL_DOCS)
	rm -f target/shots/windmill-play*.png target/shots/windmill-play-state.json target/shots/windmill-walk-*
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(WINDMILL_DOCS)/windmill-play.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	@for n in $(WINDMILL_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; \
	  m=$$(magick target/shots/$$n.png -format '%[fx:mean]' info:); \
	  awk -v m="$$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$$n.png is black (mean $$m): idle display or high load, run again"; exit 1; }; \
	  true; done
	@for n in $(WINDMILL_SHOTS); do ffmpeg -loglevel error -y -i target/shots/$$n.png -q:v 2 $(WINDMILL_DOCS)/$$n.jpg || exit 1; done
	cp target/shots/windmill-play-state.json target/shots/windmill-walk-state.json $(WINDMILL_DOCS)/
	@echo "wrote $(WINDMILL_DOCS)/"

# Farm tile in play: docs/checkpoints/farm/farm-play.json places the farm tile among meadows, shoots
# it at play distance, then close-ups of the barn and hay and of the animals, an eye-level walk view
# across the pasture to the barn, and a walk into the barn's front wall with its block dumped.
# 1280x800, tilt-shift off and `--clock fixed`. A black still fails the target, as for the windmill.
FARM_SHOTS = farm-play farm-barn farm-animals farm-walk-eye farm-walk-blocked
FARM_DOCS = docs/checkpoints/farm
shots-farm-play:
	mkdir -p target/shots $(FARM_DOCS)
	rm -f target/shots/farm-*.png target/shots/farm-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(FARM_DOCS)/farm-play.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	@for n in $(FARM_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; \
	  m=$$(magick target/shots/$$n.png -format '%[fx:mean]' info:); \
	  awk -v m="$$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$$n.png is black (mean $$m): idle display or high load, run again"; exit 1; }; \
	  true; done
	@for n in $(FARM_SHOTS); do ffmpeg -loglevel error -y -i target/shots/$$n.png -q:v 2 $(FARM_DOCS)/$$n.jpg || exit 1; done
	cp target/shots/farm-play-state.json target/shots/farm-walk-state.json $(FARM_DOCS)/
	@echo "wrote $(FARM_DOCS)/"

# First-person walk checkpoint (PRD P8): builds a village street, a forest, a river, a beach and
# an ocean with docs/checkpoints/first-person/first-person.json, walks them at eye level at
# 1280x800 with tilt-shift off and `--clock fixed`, then copies JPEG stills and the state dumps.
FP_SHOTS = fp-00-orbit fp-02-village-street fp-03-forest fp-04-river-bank fp-05-beach fp-06-map-edge fp-07-orbit-back
FP_DOCS = docs/checkpoints/first-person
shots-first-person:
	mkdir -p target/shots $(FP_DOCS)
	rm -f target/shots/fp-*.png target/shots/fp-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(FP_DOCS)/first-person.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(FP_DOCS) $(FP_SHOTS)
	cp target/shots/fp-*.json $(FP_DOCS)/
	@echo "wrote $(FP_DOCS)/"

# Cars checkpoint (P2 W3a): runs the scripted proof docs/checkpoints/cars/cars.json at 1280x800 with
# tilt-shift off and `--clock fixed` (one 20 Hz sim tick every third frame, so the run replays
# identically), then copies JPEG stills and the state dumps next to it.
CARS_SHOTS = cars-02-overview cars-02-closeup cars-02-seam cars-04-disconnected cars-05-undo
CARS_DOCS = docs/checkpoints/cars
shots-cars:
	mkdir -p target/shots $(CARS_DOCS)
	rm -f target/shots/cars-*.png target/shots/cars-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(CARS_DOCS)/cars.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(CARS_DOCS) $(CARS_SHOTS)
	cp target/shots/cars-*.json $(CARS_DOCS)/
	@echo "wrote $(CARS_DOCS)/"

# Photo mode checkpoint (PRD §4.6): runs docs/checkpoints/photo/photo.json at 1280x800 with the
# game camera's tilt-shift off and `--clock fixed`: enter (tilt preset), a manual-DOF close-up and
# the same shot with DOF off, the tilt preset with roll and exposure, one framing click-focused near
# (`focus_at` low on the screen) then far (high on it), a 4x offscreen export (5120x3200), then exit
# back to the orbit camera. Copies JPEG stills, the state dumps, the export's PNG size and mean, and
# the DOF-vs-off RMSE (fails at or below 0.004, a lens that did nothing) to the folder, and fails
# unless the far click focuses farther, higher on the screen, and changes the image. Then, level at
# eye height 40 m south of a hero oak, focused at 150 m and after a click on the crown's middle
# (photo-08/09): fails unless the click hits the tree within 2% of 36.1 m, the view depth of the
# oak's LOD0 mesh along that ray (hex-play walk test `the_tree_click_shot_meets_the_oak_mesh_at_36_m`).
# The full-size PNG stays in target/shots.
PHOTO_SHOTS = photo-01-enter-tilt photo-02-dof-closeup photo-02-dof-off-closeup photo-03-tilt-roll-exposure photo-06-click-focus-near photo-07-click-focus-far photo-08-tree-before photo-09-tree-click photo-04-export-4x photo-05-exit-orbit
PHOTO_DOCS = docs/checkpoints/photo
shots-photo:
	mkdir -p target/shots $(PHOTO_DOCS)
	rm -f target/shots/photo-*.png target/shots/photo-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(PHOTO_DOCS)/photo.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(PHOTO_DOCS) $(PHOTO_SHOTS)
	magick identify -format '{"png": "%f", "width": %w, "height": %h, "mean": %[fx:mean]}\n' target/shots/photo-04-export-4x.png > $(PHOTO_DOCS)/photo-04-export-4x.size.json
	@grep -q '"width": 5120, "height": 3200' $(PHOTO_DOCS)/photo-04-export-4x.size.json || { echo "4x export is not 5120x3200"; cat $(PHOTO_DOCS)/photo-04-export-4x.size.json; exit 1; }
	@for n in $(PHOTO_SHOTS); do m=$$(magick target/shots/$$n.png -format '%[fx:mean]' info:); \
	  awk -v m="$$m" 'BEGIN { exit !(m > 0.02) }' || { echo "$$n.png is black (mean $$m)"; exit 1; }; done
	@r=$$(magick compare -metric RMSE target/shots/photo-02-dof-closeup.png target/shots/photo-02-dof-off-closeup.png null: 2>&1 | sed -E 's/.*\(([0-9.e-]+)\).*/\1/'); \
	  awk -v r="$$r" 'BEGIN { exit !(r > 0.004) }' || { echo "manual DOF changed nothing (RMSE $$r)"; exit 1; }; \
	  echo "{\"dof_vs_off_rmse\": $$r}" > $(PHOTO_DOCS)/photo-02-dof-rmse.json; echo "manual DOF vs off: RMSE $$r"
	@r=$$(magick compare -metric RMSE target/shots/photo-06-click-focus-near.png target/shots/photo-07-click-focus-far.png null: 2>&1 | sed -E 's/.*\(([0-9.e-]+)\).*/\1/'); \
	  awk -v r="$$r" 'BEGIN { exit !(r > 0.004) }' || { echo "click focus near vs far changed nothing (RMSE $$r)"; exit 1; }; \
	  echo "click focus near vs far: RMSE $$r"
	@python3 -c 'import json,sys; n,f=(json.load(open(f"target/shots/photo-0{i}-click-focus-{w}.json"))["photo"] for i,w in ((6,"near"),(7,"far"))); ok=n["focus_picks"]==1 and f["focus_picks"]==2 and n["focus_row"]==0.85 and f["focus_row"]==0.22 and n["focus"]<f["focus"]; print("click focus near", n["focus"], "m at row", n["focus_row"], "far", f["focus"], "m at row", f["focus_row"]); sys.exit(0 if ok else 1)' || { echo "click focus state is wrong"; exit 1; }
	@python3 -c 'import json,sys; t=json.load(open("target/shots/photo-09-tree-click.json"))["photo"]; ok=t["focus_hit"]=="tree" and abs(t["focus"]-36.1)<=0.02*36.1; print("tree click:", t["focus_hit"], t["focus"], "m (expected 36.1 m, the oak mesh)"); sys.exit(0 if ok else 1)' || { echo "the tree click missed the oak"; exit 1; }
	cp target/shots/photo-*.json $(PHOTO_DOCS)/
	@echo "wrote $(PHOTO_DOCS)/"

# Photo-mode effects checkpoint (owner, 2026-10-14): runs docs/checkpoints/photo/photo-fx.json at
# 1280x800 like shots-photo: one still per effect (none, black and white, sepia, vignette, grain,
# chromatic aberration), a combined look, its 2x export, the game camera after leaving photo mode,
# and state dumps. scripts/photo-fx-check.py asserts each effect and writes photo-fx-measure.json.
PHOTO_FX_SHOTS = photo-fx-00-none photo-fx-01-mono photo-fx-02-sepia photo-fx-03-vignette photo-fx-04-grain photo-fx-05-aberration photo-fx-06-combined photo-fx-07-export-2x photo-fx-08-exit-orbit
shots-photo-fx:
	mkdir -p target/shots $(PHOTO_DOCS)
	rm -f target/shots/photo-fx-*.png target/shots/photo-fx-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(PHOTO_DOCS)/photo-fx.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(PHOTO_DOCS) $(PHOTO_FX_SHOTS)
	python3 scripts/photo-fx-check.py
	cp target/shots/photo-fx-*.json $(PHOTO_DOCS)/
	@echo "wrote $(PHOTO_DOCS)/photo-fx-*"

# Audio pipeline (PRD §8, assets-source/audio/README.md). Needs python3 and ffmpeg with libopus.
# Masters every cue in assets-source/audio/cues.json that has a source; CUE=<id> masters one.
audio-master:
	python3 scripts/audio/master.py $(if $(CUE),--cue $(CUE))

# Loudness, true peak, loop seams, provenance hashes and the decoded-memory budget of assets/audio/.
audio-check:
	python3 scripts/audio/check.py

# Masters a synthetic tree in target/audio-selftest, checks it passes, then proves a hot file,
# a clicking loop seam and a phase-cancelling stereo cue each make audio-check exit non-zero.
audio-check-selftest:
	python3 scripts/audio/selftest.py

# Water tiles in play (P2 W3c): runs docs/checkpoints/water-play/water-play.json (a coast and a
# river chain, cut faces, bulldoze/undo/redo) at 1600x1000 with tilt-shift off and a fixed water
# clock, copies JPEG stills and state dumps next to it, then a 3 s motion clip per view in VIEWS of
# motion.mjs (90 frames, 960x600) encoded to MP4.
WATER_PLAY_SHOTS = water-play-01-overview water-play-02-coast water-play-03-river water-play-04-seam-river water-play-05-seam-bridge water-play-06-seam-coast water-play-07-seam-ocean water-play-08-edge-ocean water-play-09-edge-river water-play-10-before-bulldoze water-play-11-bulldozed water-play-12-undo water-play-13-redo
WATER_PLAY_VIEWS = coast seam-river seam-coast edge-ocean
WATER_PLAY_DOCS = docs/checkpoints/water-play
shots-water-play:
	mkdir -p target/shots $(WATER_PLAY_DOCS)
	rm -f target/shots/water-play-*.png target/shots/water-play-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(WATER_PLAY_DOCS)/water-play.json --size 1600x1000 --seed 1 --fps off --tilt off --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(WATER_PLAY_DOCS) $(WATER_PLAY_SHOTS)
	cp target/shots/water-play-state*.json $(WATER_PLAY_DOCS)/
	node $(WATER_PLAY_DOCS)/motion.mjs target/shots/water-play-motion-script.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script target/shots/water-play-motion-script.json --size 960x600 --seed 1 --fps off --tilt off --water_clock fixed $(ARGS)
	@for v in $(WATER_PLAY_VIEWS); do \
	  test -s target/shots/water-play-motion-$$v-089.png || { echo "missing water-play-motion-$$v frames"; exit 1; }; \
	  ffmpeg -loglevel error -y -framerate 30 -i target/shots/water-play-motion-$$v-%03d.png -c:v libx264 -pix_fmt yuv420p -crf 22 $(WATER_PLAY_DOCS)/water-play-motion-$$v.mp4 || exit 1; \
	done
	rm -f target/shots/water-play-motion-*.png
	@echo "wrote $(WATER_PLAY_DOCS)/"

# Coast checkpoint (owner O1, directional beach sockets): runs docs/checkpoints/coast/coast.json at
# 1600x1000 with tilt-shift off and fixed clocks (an island closed by straight, headland and bay
# tiles under ten rotations inside an ocean ring, seam close-ups, a headland filling mid-rise, and
# the refused beach-against-ocean and beach-against-grass placements), then copies JPEG stills and
# the state dumps next to it.
# caffeinate keeps the display awake: a sleeping display captures every frame black, and `test -s`
# passes on a black frame.
COAST_SHOTS = coast-01-overview coast-02-refused-beach coast-03-headland coast-04-bay coast-05-seam-straight-headland coast-06-seam-topdown coast-07-seam-straight-headland coast-08-rise-headland-fill
COAST_DOCS = docs/checkpoints/coast
shots-coast:
	mkdir -p target/shots $(COAST_DOCS)
	rm -f target/shots/coast-*.png target/shots/coast-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(COAST_DOCS)/coast.json --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(COAST_DOCS) $(COAST_SHOTS)
	cp target/shots/coast-*.json $(COAST_DOCS)/
	@echo "wrote $(COAST_DOCS)/"

# River mouth and spring (owner 2026-10-14, water contract §3a/§3b): runs
# docs/checkpoints/river-mouth/river-mouth.json (a spring-to-sea chain through each mouth
# handedness, the fan and foam, the spring pool, River and Beach edge seams, fish at the mouth) at
# 1600x1000 with tilt-shift off and fixed clocks, then copies JPEG stills and the state dump.
RIVER_MOUTH_SHOTS = river-mouth-01-chain-right river-mouth-02-chain-mirrored river-mouth-03-fan-right river-mouth-04-fan-foam-topdown river-mouth-05-fan-mirrored river-mouth-06-spring-pool river-mouth-07-seam-river-edge river-mouth-08-seam-river-edge-bend river-mouth-09-seam-beach-edge5 river-mouth-10-seam-beach-edge2 river-mouth-11-fish-at-mouth river-mouth-12-fan-sand-oblique river-mouth-13-grass-line-right-long river-mouth-14-grass-line-right-short river-mouth-15-grass-line-mirrored-long river-mouth-16-grass-line-mirrored-short
RIVER_MOUTH_DOCS = docs/checkpoints/river-mouth
shots-river-mouth:
	mkdir -p target/shots $(RIVER_MOUTH_DOCS)
	rm -f target/shots/river-mouth-*.png target/shots/river-mouth-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(RIVER_MOUTH_DOCS)/river-mouth.json --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(RIVER_MOUTH_DOCS) $(RIVER_MOUTH_SHOTS)
	cp target/shots/river-mouth-state.json $(RIVER_MOUTH_DOCS)/
	@echo "wrote $(RIVER_MOUTH_DOCS)/"

# Kit beach wet band (water contract §3): the coast island of the beach close-ups, four waterline
# close-ups as JPEG, and a 5 s MP4 of the straight beach (150 frames at the fixed 1/30 s water step).
WET_BAND_SHOTS = wet-band-01-straight wet-band-02-bay wet-band-03-seam-straight-headland wet-band-04-headland
WET_BAND_DOCS = docs/checkpoints/wet-band
shots-wet-band:
	mkdir -p target/shots $(WET_BAND_DOCS)
	rm -f target/shots/wet-band-*.png
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(WET_BAND_DOCS)/wet-band.json --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(WET_BAND_DOCS) $(WET_BAND_SHOTS)
	@test -s target/shots/wet-band-motion-149.png || { echo "missing wet-band-motion frames"; exit 1; }
	ffmpeg -loglevel error -y -framerate 30 -i target/shots/wet-band-motion-%03d.png -vf scale=960:-2 -c:v libx264 -pix_fmt yuv420p -crf 22 $(WET_BAND_DOCS)/wet-band-motion.mp4
	@echo "wrote $(WET_BAND_DOCS)/"

# Boids checkpoint (PRD §7.2, crates/hex-play/src/boids.rs): runs docs/checkpoints/boids/boids.json
# at 1600x1000 with tilt-shift off and fixed clocks (the single start tile with no birds; a river
# chain, a coast closing on ten ocean tiles, ten meadows and two oak tiles; the flock overhead, birds
# roosting on an oak with a 16-degree close-up of the folded starlings on the canopy, fish through the
# ocean surface and in the river channel with a close-up, then an oak tile bulldozed, undone (the birds
# flapping back in to it) and redone; then a spring, straight and river mouth joined to the coast, with
# fish on the mouth's fan and river fish near it), copies JPEG stills and the state dumps next to it,
# and asserts the dumps (check.mjs: birds per land tile, bird slots and fish, fish per region and river
# group sizes, each region's palette, the boid pieces loaded, roosts, roosting, river groups in the
# joined region and fish crossing the fan, which the fixed clock's fish track reads back from the GPU).
BOIDS_SHOTS = boids-01-flock boids-02-roost boids-02-roost-close boids-03-fish-ocean boids-04-fish-river boids-04-fish-river-close boids-04-fish-river-play boids-07-settle boids-07-flap-close boids-10-mouth-fan boids-11-mouth-river
BOIDS_DOCS = docs/checkpoints/boids
shots-boids:
	mkdir -p target/shots $(BOIDS_DOCS)
	rm -f target/shots/boids-*.png target/shots/boids-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(BOIDS_DOCS)/boids.json --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(BOIDS_DOCS) $(BOIDS_SHOTS)
	cp target/shots/boids-*.json $(BOIDS_DOCS)/
	node $(BOIDS_DOCS)/check.mjs $(BOIDS_DOCS)
	@echo "wrote $(BOIDS_DOCS)/"

# Grass checkpoint (crates/hex-play/src/play/grass.rs): docs/checkpoints/grass/grass.json on the
# scatter checkpoint's world at 1280x800, High, rich-3, fixed clocks and tilt-shift off: meadow,
# forest margin, road verge and river bank close-ups, straight down across two tile borders, an eye-level view, a mid-zoom showing the fade,
# a gust over the meadows in four frames 1.25 s apart, Low (no grass), then the place, bulldoze and
# Start over dumps; grass-off.json repeats the eye-level view with --grass off. A GPU capture:
# check the host load is at or below 15 first.
GRASS_SHOTS = grass-01-meadow grass-02-forest grass-03-road-verge grass-04-river-bank grass-05-eye grass-06-eye-off grass-07-midzoom grass-08-gust-1 grass-08-gust-2 grass-08-gust-3 grass-08-gust-4 grass-09-low grass-10-eye-sparse grass-10-eye-lush grass-11-border-meadow grass-12-border-forest
GRASS_DOCS = docs/checkpoints/grass
GRASS_RUN = bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed --tier high --grade rich-3

shots-grass:
	mkdir -p target/shots $(GRASS_DOCS)
	rm -f target/shots/grass-*.png target/shots/grass-*.json
	$(GRASS_RUN) --script $(GRASS_DOCS)/grass.json $(ARGS)
	$(GRASS_RUN) --script $(GRASS_DOCS)/grass-off.json --grass off $(ARGS)
	bash scripts/shots/run.sh --jpegs $(GRASS_DOCS) $(GRASS_SHOTS)
	cp target/shots/grass-state-*.json $(GRASS_DOCS)/
	@echo "wrote $(GRASS_DOCS)/"

# Scatter checkpoint (PRD §5.3, crates/hex-play/src/play/scatter.rs): runs
# docs/checkpoints/scatter/scatter.json at 1600x1000 with tilt-shift off and fixed clocks (a cluster
# of pine forests and oak tiles with their junctions, meadows with flower drifts, a forest between a
# road and a river, a house cluster; then a forest rising,
# bulldozed, undone, and again under reduced motion) after every pack has landed, then
# scatter-stream.json with the streamed packs held (a forest and the start meadow before and after
# their packs land). Copies JPEG stills and the state dumps next to it, and asserts the dumps
# (check.mjs: per-tile counts, roosts, undo, reduced motion, streaming, draw calls).
SCATTER_SHOTS = scatter-01-overview scatter-02-forest-cluster scatter-03-meadow scatter-04-road-river scatter-05-forest-closeup scatter-06-rise-props scatter-07-rise-settled scatter-12-oak scatter-13-junction scatter-14-meadow-drifts
SCATTER_DOCS = docs/checkpoints/scatter
shots-scatter:
	mkdir -p target/shots $(SCATTER_DOCS)
	rm -f target/shots/scatter-*.png target/shots/scatter-*.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(SCATTER_DOCS)/scatter.json --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	HEXW_PACK_HOLD_FRAMES=120 bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script $(SCATTER_DOCS)/scatter-stream.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed $(ARGS)
	bash scripts/shots/run.sh --jpegs $(SCATTER_DOCS) $(SCATTER_SHOTS)
	cp target/shots/scatter-*.json $(SCATTER_DOCS)/
	node $(SCATTER_DOCS)/check.mjs $(SCATTER_DOCS)
	@echo "wrote $(SCATTER_DOCS)/"

# Scatter on WebGPU (local only, GPU Chromium): the scatter checkpoint's 20-tile world on the built
# site with post=full and the device clamped to 16 sampled textures per stage. Asserts every kit
# forward pipeline binds exactly 16 next to refracting water, the per-tile scatter matches the native
# dump at the same seed, and the pass census stays within 500 draws (e2e/scatter.mjs).
web-scatter: build-web
	cd e2e && npm ci --no-audit --no-fund
	caffeinate -d -u node e2e/scatter.mjs dist $(SCATTER_DOCS)

# Legend inventory tile thumbnails (PRD §5.3): every tile's distinct rotations rendered natively
# through the play path at 4x (crates/hex-app/src/thumbs.rs), then keyed, downsampled to 96 px and
# encoded into assets/thumbs/ with the contact sheet in docs/checkpoints/thumbs/. Needs uv.
thumbs:
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --thumbs
	uv run --script scripts/thumbs/build.py

# Staged rise and Legend affordances (PRD §4.4, design/ui/legend/in-world.md): runs the script that
# docs/checkpoints/rise/rise.mjs writes at 1280x800 with tilt-shift off and both clocks fixed (every
# frame advances the rise by 1/60 s), then encode.sh writes the stage sheets, MP4 clips, stills and
# state dumps next to it.
RISE_DOCS = docs/checkpoints/rise
shots-rise:
	mkdir -p target/shots $(RISE_DOCS)
	rm -f target/shots/rise-*.png target/shots/rise-*.json target/shots/aff-*.png target/shots/aff-*.json
	node $(RISE_DOCS)/rise.mjs target/shots/rise-script.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script target/shots/rise-script.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash $(RISE_DOCS)/encode.sh
	rm -f target/shots/rise-*-f*.png

# Bulldoze dust cloud (owner request 2026-10-14, design/ui/legend/in-world.md Bulldoze): runs the
# script docs/checkpoints/bulldoze-dust/bulldoze-dust.mjs writes at 1280x800 with tilt-shift off and
# both clocks fixed (every frame advances 1/60 s), then encode.sh writes the bulldoze stills, a stage
# sheet, the bulldoze and undo clips and the state dumps next to it.
DUST_DOCS = docs/checkpoints/bulldoze-dust
shots-bulldoze-dust:
	mkdir -p target/shots $(DUST_DOCS)
	rm -f target/shots/dust-*.png target/shots/dust-*.json
	node $(DUST_DOCS)/bulldoze-dust.mjs target/shots/dust-script.json
	bash scripts/shots/run.sh cargo run -p hex-app --release -- --scenario play --script target/shots/dust-script.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed $(ARGS)
	bash $(DUST_DOCS)/encode.sh
	rm -f target/shots/dust-*-f*.png
