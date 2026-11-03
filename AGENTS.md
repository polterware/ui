# Polterware UI Agent Instructions

## Communication and editing

- Speak to Erick in Brazilian Portuguese.
- Write code, UI strings, technical documentation, commits, logs, and identifiers in English. No Portuguese text anywhere in the repository: code, comments, UI strings, error and log messages, tests, fixtures, docs, and skills.
- Preserve unrelated work and make the smallest safe change.
- Do not create branches or commits unless explicitly requested.
- Do not run browsers, previews, or development servers unless the current task asks for them. Automated tests and builds may run by default.

## What this is

The shared GPUI library of Polterware, Polterware Dispatch, and Vitals (see README.md for the crates). It knows no app: status words, routes, and data belong to the apps, which set them through `polterware_theme::set_root_rem` and `set_status_tones`.

- The Polterware design system (`DESIGN.md` and `@polterware/frontend-design-system` in [polterware/polterware](https://github.com/polterware/polterware)) is the visual source of truth. A control here ports its CSS: name the CSS it mirrors in the doc comment, and keep values as tokens in `polterware-theme`.
- Use no third-party GPUI kit (no gpui-component), and a control never defers its own panel.
- Hugeicons Free Stroke Rounded is the only icon set (`polterware-assets`, generated from `@hugeicons/core-free-icons` under the design system's names).
- Visual differences of about 1pt from the web design system are acceptable; do not vendor GPUI patches to chase them.
- Every control appears in `examples/gallery` in its variants and states. Add a gallery page with a new control, and compare captures (`GALLERY_SHOTS=<dir>`) before and after a visual change.
- The apps depend on this repository by git. A change to a public API is a change to every app: say which apps it breaks.

## Verification

Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` after relevant changes, and the gallery captures after visual ones. Maintain risk-proportional tests.
