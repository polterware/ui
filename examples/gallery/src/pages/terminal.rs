//! Vitals' Terminal tab (`TerminalView.tsx` in `.vitals-terminal-pane`): the shell on the
//! surface, padded 20px 16px 12px, here fed a session that shows the 16 ANSI colors, the
//! 256-color cube and grays, 24-bit color, and the text styles. Typing echoes as a line
//! discipline would: Return gives a new prompt, Backspace erases, Control-C cancels.

use gpui::{AnyElement, App, Context, IntoElement, Window, div, prelude::*, px};
use polterware_ui::terminal::{TerminalEvent, TerminalView};
use polterware_ui::theme::Theme;

use super::super::Example;

const PROMPT: &str = "\x1b[1;32mops@demo\x1b[0m:\x1b[1;34m~\x1b[0m$ ";

/// The session the example opens on.
fn session() -> String {
    let mut out = String::new();
    out.push_str(PROMPT);
    out.push_str("ls --color\r\n");
    out.push_str(
        "\x1b[1;34mbin\x1b[0m  \x1b[1;34mdocs\x1b[0m  \x1b[1;32mdeploy.sh\x1b[0m  README.md  \
         \x1b[1;36mlogs\x1b[0m -> /var/log  \x1b[31mbackup.tar.gz\x1b[0m\r\n",
    );
    out.push_str(PROMPT);
    out.push_str("./colors\r\n");
    for (label, base) in [("normal", 40), ("bright", 100)] {
        out.push_str(&format!("{label:<8}"));
        for index in 0..8 {
            out.push_str(&format!("\x1b[{}m    \x1b[0m", base + index));
        }
        out.push_str("  ");
        for index in 0..8 {
            let fg = if base == 40 { 30 } else { 90 } + index;
            out.push_str(&format!("\x1b[{fg}mAa\x1b[0m "));
        }
        out.push_str("\r\n");
    }
    for row in 0..2 {
        out.push_str(if row == 0 { "256     " } else { "        " });
        for column in 0..54 {
            let index = 16 + row * 108 + column * 2;
            out.push_str(&format!("\x1b[48;5;{index}m \x1b[0m"));
        }
        out.push_str("\r\n");
    }
    out.push_str("grays   ");
    for index in 232..256 {
        out.push_str(&format!("\x1b[48;5;{index}m  \x1b[0m"));
    }
    out.push_str("\r\n24-bit  ");
    for step in 0..72 {
        let t = step as f32 / 71.0;
        let (r, g, b) = (
            (255.0 * (1.0 - t)) as u8,
            (255.0 * (1.0 - (2.0 * t - 1.0).abs())) as u8,
            (255.0 * t) as u8,
        );
        out.push_str(&format!("\x1b[48;2;{r};{g};{b}m \x1b[0m"));
    }
    out.push_str("\r\nstyles  \x1b[1mbold\x1b[0m  \x1b[2mdim\x1b[0m  \x1b[3mitalic\x1b[0m  ");
    out.push_str("\x1b[4munderline\x1b[0m  \x1b[7minverse\x1b[0m  \x1b[9mstrike\x1b[0m  ");
    out.push_str("\x1b[1;31mbold red\x1b[0m  \x1b[38;2;255;170;0mamber\x1b[0m\r\n");
    out.push_str(PROMPT);
    out
}

/// What a line discipline would echo for `input`.
fn echo(input: &[u8]) -> Vec<u8> {
    if input.first() == Some(&0x1b) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &byte in input {
        match byte {
            b'\r' => {
                out.extend_from_slice(b"\r\n");
                out.extend_from_slice(PROMPT.as_bytes());
            }
            0x7f => out.extend_from_slice(b"\x08 \x08"),
            0x03 => {
                out.extend_from_slice(b"^C\r\n");
                out.extend_from_slice(PROMPT.as_bytes());
            }
            byte if byte >= 0x20 => out.push(byte),
            _ => {}
        }
    }
    out
}

fn shell(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let terminal = window.use_keyed_state(
        "gallery-terminal",
        cx,
        |window, cx: &mut Context<TerminalView>| {
            let mut terminal = TerminalView::new(window, cx);
            terminal.feed(session().as_bytes(), cx);
            cx.subscribe_self(|terminal: &mut TerminalView, event, cx| {
                if let TerminalEvent::Input(bytes) = event {
                    terminal.feed(&echo(bytes), cx);
                }
            })
            .detach();
            terminal
        },
    );
    vec![
        div()
            .w_full()
            .h(px(372.0))
            .bg(theme.color_surface)
            .pt(px(20.0))
            .px(px(16.0))
            .pb(px(12.0))
            .child(terminal)
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "terminal",
        title: "Shell",
        height: 420.0,
        render: shell,
    }]
}
