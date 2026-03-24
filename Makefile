.PHONY: deps-check deps-update deps-install clean build test serve

deps-check:
	cargo run -- check-deps

deps-update:
	cargo run -- update-deps

deps-install:
	cargo run -- install-deps

clean:
	cargo run -- clean

build:
	cargo run --release -- build

test:
	test -f build/site/index.html
	test -f build/site/docs/index.html
	test -f build/site/404.html
	test -f build/site/CNAME
	test -f build/site/playground/index.html
	test -f build/site/playground/playground.js

serve:
	cargo run -- serve
