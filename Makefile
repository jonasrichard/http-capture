.PHONY: run-debug

run-debug:
	cargo build
	sudo setcap cap_net_raw,cap_net_admin=eip target/debug/http-capture
	target/debug/http-capture
