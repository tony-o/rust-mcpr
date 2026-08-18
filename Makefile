_:
	cargo test

clean:
	rm -Rf target
	rm -Rf registry/target
	rm -Rf macros/target

verify-examples:
	find examples -name 'Cargo.toml' -exec bash -c 'cd $$(dirname {}); cargo build' ';'
	find examples -name 'verify.py' -exec bash -c 'DIR=$$(dirname {} | perl -pe '"'"'s/^.*\/([^\/]+)$$/$$1/'"'"'); echo "==============="; echo "TESTING $$DIR"; echo "==============="; cargo build && python3 "examples/$$DIR/verify.py" "target/debug/$$DIR"' ';'

