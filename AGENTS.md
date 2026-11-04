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
- A control never defers its own panel.
- Hugeicons Free Stroke Rounded is the only icon set (`polterware-assets`, generated from `@hugeicons/core-free-icons` under the design system's names).
- Visual differences of about 1pt from the web design system are acceptable; do not vendor GPUI patches to chase them.
- Every control appears in `examples/gallery` in its variants and states. Add a gallery page with a new control, and compare captures (`GALLERY_SHOTS=<dir>`) before and after a visual change.
- The apps depend on this repository by git. A change to a public API is a change to every app: say which apps it breaks.

## Building interface

This library is how every Polterware app builds its interface, so interface work makes it stronger instead of going around it.

- Componentize first. When an app needs a control, a row, a list, a panel, or a pattern, build it here as a general component the app configures (items, slots, callbacks, events) and have the app use it. An app's own code keeps only what is truly its own: its data, its words, its routes, and what a confirmed command does. When an app already draws something by hand that another app could use, move it here. Name the apps a new or changed public API reaches.
- Two references, two jobs:
  - **Look**: the old Solid/Electron apps and the web design system (`@polterware/frontend-design-system`, `@polterware/feature-*` in [polterware/polterware](https://github.com/polterware/polterware); the pre-GPUI copies are in the workspace's `archive/` bundles). Match their layout, sizes, and spacing. Our tokens (`polterware-theme`) and materials (`material.rs`: faces, edges, drops, `IconTile`, `kbd`) are already right; use them and do not invent colors, radii, or shadows.
  - **Behavior and robustness**: [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit) (formerly gpui-component). Before building a component, read its counterpart there and match what it does: its state model and API shape (a state entity, builders, slots, events), keyboard handling, selection, scrolling, empty and disabled states, and the edge cases its tests cover. Port the behavior into this library; do not depend on the crate and do not take its look.
- Write the component's doc comment to say both: which CSS or Solid component it looks like, and which gpui-kit component it behaves like. `patterns/commander.rs` (`Commander`, after gpui-kit's `Command`) is the model to follow.
- Test the behavior here (`#[gpui::test]`), add the component to `examples/gallery`, and check it in a real app capture.
- Still by default. Nothing animates while nothing changes: a motion runs to its end and stops asking for frames. A bot (`bots.rs`) draws one shared idle frame; only `agent_avatar_live` moves, and only while it works, for the one bot a page is about. Measure CPU with the window in front before and after a change that adds motion (a 10-second average of the process's CPU time).
- Text that wraps inside a flex row is a `wrap_text` (`wrap.rs`): gpui 0.2.2's own text can paint a measurement taffy made at another width, one letter a line.
- GPUI clips to rectangles: content that scrolls inside a rounded frame must stop short of its corners, and an overlay (a fade, a fill) must stay inside the curve.

## Verification

Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` after relevant changes, and the gallery captures after visual ones. Maintain risk-proportional tests.
