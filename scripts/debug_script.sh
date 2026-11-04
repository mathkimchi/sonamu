# cargo build --release
# WINIT_UNIX_BACKEND=wayland RUST_BACKTRACE=full cargo run --bin sonamu_sde
# cargo test --package sonamu_common --lib -- tests::dylib_test --exact --show-output
# cargo test --package sonamu_standard_tabs --lib -- demo::run_test --exact --show-output
# cargo test --package singularity_ui --lib -- test --show-output
# cargo run --package singularity_ui --bin demo
# RUST_BACKTRACE=full cargo run --package singularity_ui --example ab_glyph_demo
# cd sonamu_standard_tabs ; cargo expand --test demo
# cd sonamu_common ; cargo expand tab::packets # -Z macro-backtrace
# RUST_BACKTRACE=full cargo test --package sonamu_common --test sap_connection_test -- sap_connection_test --exact --show-output
# RUST_BACKTRACE=full cargo test --package sonamu_common --test query_response_sandbox -- test --exact --show-output --nocapture
# RUST_BACKTRACE=full cargo test --package sonamu_common --test all_packets_sandbox -- test_same_process --exact --show-output --nocapture
# RUST_BACKTRACE=full cargo test --package sonamu_common --test all_packets_sandbox -- test_multi_process --exact --show-output --nocapture
# RUST_BACKTRACE=1 cargo test --package singularity_sap --test macro_sandbox -- test_data_conversion --exact --show-output --nocapture
# cd singularity_sap ; cargo expand standard_packets # -Z macro-backtrace
# cd singularity_sap ; cargo expand --test dynamic_plugin_sandbox
RUST_BACKTRACE=1 cargo run --bin run_bare_nodular_demo

# RUST_BACKTRACE=1 cargo run --bin run_terminal_demo
