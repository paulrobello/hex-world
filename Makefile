.PHONY: fetch-tools fetch-hdri assets-p0 run shot run-shot run-diag shots web-shots fmt fmt-check lint typecheck test checkall checkall-ci ref-bevy build-web web-size web-smoke perf-gate perf-matrix shots-chunks shots-instanced blender-test blender-test-selfcheck refs kit-build kit-determinism kit-compress kit-material kit-compress-test kit-fixture kit-renders kit-check kit-provenance kit-material-provenance shots-kit web-shots-kit lookdev-ibl shots-lookdev shots-tilt web-shots-tilt shots-water perf-water initial-download shots-play shots-cars brand audio-master audio-check audio-check-selftest

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

typecheck:
	cargo check --workspace --all-targets
	cargo check -p hex-app --target wasm32-unknown-unknown

test:
	cargo test --workspace

checkall: fmt-check lint typecheck test

# initial-download runs after web-size, on the dist/ it just built (not a sibling prerequisite: under
# make -j it would race web-size's rm -rf dist).
checkall-ci: checkall web-size
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

# The kit gate alone. checkall already runs it through `test` (cargo test --workspace).
kit-check:
	cargo test -p hex-render --test kit_gate

# Writes provenance with validation.pass only after every non-provenance gate passes.
# ROUTE=interactive defaults BLEND to the piece's .blend in the main checkout's store (owner O2,
# scripts/blender/lib/paths.py; HEXW_BLEND_STORE overrides). Scripted pieces get no blendRef.
BLEND_STORE = $(shell python3 scripts/blender/lib/paths.py store)
KIT_BLEND = $(or $(BLEND),$(if $(filter interactive,$(ROUTE)),$(shell python3 scripts/blender/lib/paths.py blend $(PIECE))))
KIT_PROV_ARGS = $(if $(REFS),--refs $(REFS)) $(if $(KIT_BLEND),--blend $(KIT_BLEND) --blend-store $(BLEND_STORE)) $(if $(ROUTE),--route $(ROUTE))
kit-provenance:
	@test -n "$(PIECE)" || { echo "usage: make kit-provenance PIECE=<id> [REFS=assets-source/refs/<ref>] [ROUTE=interactive [BLEND=path]]"; exit 2; }
	@if [ "$(ROUTE)" = interactive ] && [ ! -f "$(KIT_BLEND)" ]; then \
	  echo "kit-provenance: ROUTE=interactive needs the piece's .blend, got '$(KIT_BLEND)' (BLEND=, or python3 scripts/blender/lib/paths.py blend $(PIECE))"; exit 2; fi
	node scripts/assets/kit-provenance.mjs $(PIECE) $(KIT_PROV_ARGS)
	cargo test -p hex-render --test kit_gate -- --skip provenance
	$(BLENDER_RUN) -P scripts/blender/kit_render.py -- $(PIECE)
	node scripts/assets/kit-provenance.mjs $(PIECE) $(KIT_PROV_ARGS) \
	  --validated "kit_gate geometry,kit_gate textures,kit_gate bevy reimport,blender reimport sockets,six-view render"

# Provenance for a tileable set: assets-source/kit/materials/<set>/provenance.json.
# The set must be in assets/kit/manifest.json "materials" before validation.pass is stamped.
kit-material-provenance:
	@test -n "$(SET)" || { echo "usage: make kit-material-provenance SET=<set>"; exit 2; }
	node scripts/assets/kit-provenance.mjs $(SET) --material
	cargo test -p hex-render --test kit_gate ktx2_color_space_mips_and_budgets
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
	cargo run -p hex-app --release -- --shot single --exit-after 180
	@test -s target/shots/single.png && echo "wrote target/shots/single.png"

run-shot:
	cargo run -p hex-app --release -- --shot single --exit-after 180

run-diag:
	cargo run -p hex-app --release -- --scenario gate2000 --diag --dump-state target/state.json --exit-after-seconds 10
	@test -s target/state.json && cat target/state.json && echo

shots:
	mkdir -p target/shots
	rm -f target/shots/gallery-*.png target/shots/gallery-*.json
	cargo run -p hex-app --release -- --script scripts/shots/gallery.json --size 1280x800 --seed 1 --fps off
	@for n in single single-low gate-strategic gate-closeup gate-flock; do test -s target/shots/gallery-$$n.png || { echo "missing gallery-$$n.png"; exit 1; }; done
	@echo "wrote target/shots/gallery-*.png"

shots-chunks:
	mkdir -p target/shots
	rm -f target/shots/gallery-chunks-*.png target/shots/gallery-chunks-*.json
	sed 's/"gallery-/"gallery-chunks-/' scripts/shots/gallery.json > target/shots/gallery-chunks-script.json
	cargo run -p hex-app --release -- --script target/shots/gallery-chunks-script.json --size 1280x800 --seed 1 --fps off --layout chunks
	@for n in single single-low gate-strategic gate-closeup gate-flock; do test -s target/shots/gallery-chunks-$$n.png || { echo "missing gallery-chunks-$$n.png"; exit 1; }; done
	@echo "wrote target/shots/gallery-chunks-*.png (layout=chunks)"

shots-instanced:
	mkdir -p target/shots
	rm -f target/shots/gallery-instanced-*.png target/shots/gallery-instanced-*.json
	sed 's/"gallery-/"gallery-instanced-/' scripts/shots/gallery.json > target/shots/gallery-instanced-script.json
	cargo run -p hex-app --release -- --script target/shots/gallery-instanced-script.json --size 1280x800 --seed 1 --fps off --layout instanced
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
shots-kit:
	mkdir -p target/shots
	rm -f target/shots/kit-*.png
	cargo run -p hex-app --release -- --script scripts/shots/kit.json --size 1600x1000 --seed 1 --fps off $(ARGS)
	@for n in $(KIT_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; done
	@echo "wrote target/shots/kit-*.png"

# Tone-mapping comparison (D4) of the kit shots. ARGS passes kit keys and LOOKDEV_OUT moves the
# output, e.g. ARGS="--kit_src fixture" LOOKDEV_OUT=target/shots/lookdev-fixture.
LOOKDEV_OUT ?= target/shots
shots-lookdev:
	@mkdir -p $(LOOKDEV_OUT)
	@for t in agx tony aces; do \
	  rm -f target/shots/kit-*.png; \
	  cargo run -p hex-app --release -- --script scripts/shots/kit.json --size 1600x1000 --seed 1 --fps off --tonemap $$t $(ARGS) || exit 1; \
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
	  cargo run -p hex-app --release -- --script scripts/shots/tilt.json --size 1600x1000 --seed 1 --fps off --tilt $$m $(ARGS) || exit 1; \
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
	cargo run -p hex-app --release -- --script scripts/shots/water.json --size 1600x1000 --seed 1 --fps off --water_clock fixed $(ARGS)
	@for n in $(WATER_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; \
	  ffmpeg -loglevel error -y -i target/shots/$$n.png -q:v 2 $(WATER_DOCS)/$$n.jpg || exit 1; done
	node scripts/shots/water-motion.mjs target/shots/water-motion-script.json
	cargo run -p hex-app --release -- --script target/shots/water-motion-script.json --size 960x600 --seed 1 --fps off --water_clock fixed $(ARGS)
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

# Play scenario checkpoint (P2 placement loop): runs the scripted proof docs/checkpoints/play/play.json
# at 1280x800 with tilt-shift off, then copies JPEG stills and the state dumps next to it.
PLAY_SHOTS = play-01-start play-02-ghost-road play-03-placed play-04-illegal-ghost play-05-undo play-06-redo play-07-bulldoze-leaf play-08-bulldoze-bridge
PLAY_DOCS = docs/checkpoints/play
shots-play:
	mkdir -p target/shots $(PLAY_DOCS)
	rm -f target/shots/play-*.png target/shots/play-*.json
	cargo run -p hex-app --release -- --scenario play --script $(PLAY_DOCS)/play.json --size 1280x800 --seed 1 --fps off --tilt off $(ARGS)
	@for n in $(PLAY_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; \
	  ffmpeg -loglevel error -y -i target/shots/$$n.png -q:v 2 $(PLAY_DOCS)/$$n.jpg || exit 1; done
	cp target/shots/play-*.json $(PLAY_DOCS)/
	@echo "wrote $(PLAY_DOCS)/"

# Cars checkpoint (P2 W3a): runs the scripted proof docs/checkpoints/cars/cars.json at 1280x800 with
# tilt-shift off and `--clock fixed` (one 20 Hz sim tick every third frame, so the run replays
# identically), then copies JPEG stills and the state dumps next to it.
CARS_SHOTS = cars-02-overview cars-02-closeup cars-02-seam cars-04-disconnected cars-05-undo
CARS_DOCS = docs/checkpoints/cars
shots-cars:
	mkdir -p target/shots $(CARS_DOCS)
	rm -f target/shots/cars-*.png target/shots/cars-*.json
	cargo run -p hex-app --release -- --scenario play --script $(CARS_DOCS)/cars.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed $(ARGS)
	@for n in $(CARS_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; \
	  ffmpeg -loglevel error -y -i target/shots/$$n.png -q:v 2 $(CARS_DOCS)/$$n.jpg || exit 1; done
	cp target/shots/cars-*.json $(CARS_DOCS)/
	@echo "wrote $(CARS_DOCS)/"

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
