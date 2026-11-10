# Mirror Terrain simulator. Every target is deterministic for a given seed.
# Override sizes on the command line, e.g.:
#   make suite OUT=results/try GAMES=400

FUZZ_SECS ?= 60
FILE ?=
GAMES ?= 400
OUT   ?= results/v05
PROFILE_GAMES   ?= 40
PROFILE_THREADS ?= 1
SIM   := ./target/release/mt_sim

.PHONY: build test check quick suite viewer symcheck verify ci fuzz mutants profile demo clean help

help:
	@echo "make build        release build"
	@echo "make test         rules invariants (symmetry, determinism, path, scars) and bot tests"
	@echo "make check        test + verify + symcheck (run before trusting any number)"
	@echo "make quick        the scorecard as one JSON line          (GAMES)"
	@echo "make suite        the scorecard as report.md/json + replays (GAMES, OUT)"
	@echo "make viewer       embed OUT into viewer/index.html"
	@echo "make ci           everything CI should run (fast sizes)"
	@echo "make fuzz         run both fuzz targets for FUZZ_SECS each (needs nightly + cargo-fuzz)"
	@echo "make profile      samply profile of a quick run           (PROFILE_GAMES, PROFILE_THREADS)"
	@echo "make demo         play the game against a bot (Bevy window; first build takes a few minutes)"
	@echo "make mutants      mutation testing of mt_core + mt_bots (FILE=path for one file)"

build:
	cargo build --release

test:
	cargo test --release

verify: build
	$(SIM) verify 3

symcheck: build
	$(SIM) symcheck search 60
	$(SIM) symcheck shaper 15

check: test verify symcheck

quick: build
	$(SIM) quick $(GAMES)

suite: build
	$(SIM) suite $(OUT) $(GAMES)

viewer:
	python3 viewer/build_viewer.py $(OUT) viewer/index.html

ci: test verify
	$(SIM) symcheck search 30
	$(SIM) symcheck shaper 10
	$(SIM) match search greedy 100 > /dev/null

# Fuzzing needs a nightly toolchain: `rustup toolchain install nightly` and `cargo install cargo-fuzz`.
fuzz:
	cargo +nightly fuzz run game_invariants -- -max_total_time=$(FUZZ_SECS)
	cargo +nightly fuzz run mirrored_draw -- -max_total_time=$(FUZZ_SECS)

# `cargo install cargo-mutants`. Config is in .cargo/mutants.toml; the full run takes a while.
mutants:
	cargo mutants $(if $(FILE),-f $(FILE),)

# `cargo install --locked samply`. Records a `quick` run and opens the profile in the browser.
# One thread keeps the call tree readable; raise PROFILE_THREADS to see contention.
profile:
	cargo build --profile profiling
	MT_THREADS=$(PROFILE_THREADS) samply record ./target/profiling/mt_sim quick $(PROFILE_GAMES)

# The playable demo. BOT picks the opponent (any name from `mt_sim help`), SEED the map.
BOT  ?= shaper
SEED ?=
demo:
	cargo run -p mt_game --profile demo -- $(BOT) $(SEED)

clean:
	cargo clean
