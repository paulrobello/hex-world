.PHONY: fetch-tools assets-p0 run shot run-shot run-diag shots web-shots fmt fmt-check lint typecheck test checkall checkall-ci ref-bevy build-web web-size web-smoke perf-gate perf-matrix shots-chunks shots-instanced

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

checkall-ci: checkall web-size

fetch-tools:
	bash scripts/fetch-ktx.sh

assets-p0: fetch-tools
	cd scripts/assets && npm ci --no-audit --no-fund
	node scripts/assets/build-p0-assets.mjs
	mkdir -p target
	. tools/ktx-env.sh; \
	ktx create --format R8G8B8A8_SRGB --encode uastc --uastc-quality 2 --zstd 18 --generate-mipmap assets/kit/p0/hex_test.png target/hex_test.uastc.ktx2 && \
	ktx transcode --target bc7 --zstd 18 target/hex_test.uastc.ktx2 assets/kit/p0/hex_test.bc7.ktx2 && \
	ktx transcode --target astc --zstd 18 target/hex_test.uastc.ktx2 assets/kit/p0/hex_test.astc.ktx2 && \
	ktx info assets/kit/p0/hex_test.bc7.ktx2 | head -20

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
