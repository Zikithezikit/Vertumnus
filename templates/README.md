# Vertumnus Templates

Vertumnus currently uses **in-code string generation** for all output files
(see `crates/vertumnus-generator/src/codegen.rs` and `stubs.rs`), rather than
an external template engine.

This directory is reserved for future use with a template engine
(e.g., `askama` or `minijinja`) if the string-building approach becomes
unwieldy. See spec §7 for the originally proposed template layout.
