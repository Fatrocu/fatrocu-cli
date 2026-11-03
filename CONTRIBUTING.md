# Contributing to fatrocu-cli 3.1

The CLI repository owns the shared Rust processing core and command-line automation for Windows and Linux. Follow the workspace agent instructions and notes.md for architecture and product decisions.

For each implementation task, work in a separate worktree and branch, commit there, run the required tests and the local extraction benchmark before opening a PR to main. The benchmark uses private local data; never commit source invoices, receipts, API credentials, or raw telemetry. PR gates are defined in ../benchmarks.md.

Use Rust stable unless the workspace pins another toolchain. Add focused tests for changed behavior, run formatting and lint checks, and describe actual validation in the PR. Exact release and publishing automation has not yet been selected.
