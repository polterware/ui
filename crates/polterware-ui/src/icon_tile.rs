//! `IconTile` (`.pw-icon-tile`): a square on the lit control material (a flat
//! `--material-face-top` face, the control edge and drop, `--radius-control`) holding a mark.
//!
//! The `plain` finish draws the mark in the secondary text color at stroke 2. The default
//! `chrome-mark` finish draws it at stroke 2.5, filled with a metal sheet tinted by the tile's
//! tone (`chromeMarkStopsFromTint`): light, the vivid tint, a light band, a deep band, then the
//! tint again. GPUI cannot mask a gradient with an SVG, so the mark is drawn once per band of
//! the sheet, each copy clipped to its band, in the band's color; the sheet's seeded warp and
//! 30° angle are not drawn.
//!
//! `OptionTile` (`.pw-option-tile`), the 20px tile leading the options of a popover list,
//! shows its label's initial when it has no icon: 10px semibold in the tone's accent.

use gpui::{App, FontWeight, IntoElement, Rgba, SharedString, Window, div, prelude::*, px};

use super::icons::{Icon, icon_stroked};
use super::material::{self, Edge};
use crate::theme::{Theme, mix, rgb_f32};

/// `IconTileTone`: the accent a `chrome-mark` tile tints its mark with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileTone {
    Neutral,
    Architecture,
    Infrastructure,
    Data,
    Execution,
    Knowledge,
    Identity,
    Security,
    Integration,
    Observability,
}

impl TileTone {
    fn accent(self, theme: &Theme) -> Rgba {
        match self {
            TileTone::Neutral => theme.icon_tile_accent_neutral,
            TileTone::Architecture | TileTone::Execution | TileTone::Integration => {
                theme.chart_blue
            }
            TileTone::Infrastructure | TileTone::Identity => theme.chart_cyan,
            TileTone::Data | TileTone::Observability => theme.chart_green,
            TileTone::Knowledge | TileTone::Security => theme.chart_amber,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileFinish {
    ChromeMark,
    Plain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TileMark {
    Icon(Icon),
    Svg(SharedString),
    Initial(SharedString),
}

#[derive(IntoElement)]
pub struct IconTile {
    mark_kind: TileMark,
    size: f32,
    mark: f32,
    tone: TileTone,
    finish: TileFinish,
    /// A circle in place of the control radius, for a tile inside a pill.
    round: bool,
    /// A tint in place of the tone's accent, as a text color class on the tile sets it.
    tint: Option<Rgba>,
}

impl IconTile {
    /// A 24px chrome-mark tile with a 16px mark in the neutral tone.
    pub fn new(icon: Icon) -> Self {
        Self {
            mark_kind: TileMark::Icon(icon),
            size: 24.0,
            mark: 16.0,
            tone: TileTone::Neutral,
            finish: TileFinish::ChromeMark,
            round: false,
            tint: None,
        }
    }

    /// A filled SVG mark (Electron `IconTile` accepts arbitrary SVG children), using
    /// the same chrome bands and material as Hugeicons. The asset source owns the path.
    pub fn svg_asset(mut self, path: impl Into<SharedString>) -> Self {
        self.mark_kind = TileMark::Svg(path.into());
        self
    }

    pub fn size(mut self, size: f32, mark: f32) -> Self {
        self.size = size;
        self.mark = mark;
        self
    }

    pub fn tone(mut self, tone: TileTone) -> Self {
        self.tone = tone;
        self
    }

    /// Tints the mark with `tint` in place of the tone's accent, as a text color class on the
    /// tile does (`class="!text-[var(--color-chart-violet)]"`): the chrome sheet reads it.
    pub fn tint(mut self, tint: Rgba) -> Self {
        self.tint = Some(tint);
        self
    }

    /// A round tile: a circle, as a pill's leading mark follows its round end.
    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }

    pub fn plain(mut self) -> Self {
        self.finish = TileFinish::Plain;
        self
    }

    /// `OptionTile`: 20px with a 12px mark, the icon when there is one, else the label's initial.
    pub fn option(icon: Option<Icon>, label: &str) -> Self {
        let tile = match icon {
            Some(icon) => Self::new(icon),
            None => Self {
                mark_kind: TileMark::Initial(crate::filter::initial(label).into()),
                ..Self::new(Icon::Bot)
            },
        };
        tile.size(20.0, 12.0)
    }
}

/// The bands of `chromeMarkStopsFromTint` over the sheet's height: (start, end, color).
fn chrome_bands(tint: Rgba) -> [(f32, f32, Rgba); 5] {
    const FLOOR: f32 = 0.55;
    let white = rgb_f32(1.0, 1.0, 1.0);
    let mean = (tint.r + tint.g + tint.b) / 3.0;
    let tint = if mean < FLOOR {
        mix(tint, white, (FLOOR - mean) / (1.0 - mean))
    } else {
        tint
    };
    let mean = (tint.r + tint.g + tint.b) / 3.0;
    let vivid_channel = |c: f32| (mean + (c - mean) * 1.8).clamp(0.0, 1.0);
    let vivid = rgb_f32(
        vivid_channel(tint.r),
        vivid_channel(tint.g),
        vivid_channel(tint.b),
    );
    let light = mix(vivid, white, 0.45);
    let deep = mix(vivid, rgb_f32(0.12, 0.12, 0.13), 0.4);
    [
        (0.0, 0.18, light),
        // The ramp from light to vivid, drawn at its middle.
        (0.18, 0.43, mix(light, vivid, 0.5)),
        (0.43, 0.55, light),
        (0.55, 0.70, deep),
        (0.70, 1.0, vivid),
    ]
}

impl RenderOnce for IconTile {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let accent = self
            .tint
            .unwrap_or_else(|| self.tone.accent(Theme::global(cx)));
        let theme = *Theme::global(cx);
        let radius = if self.round {
            self.size / 2.0
        } else {
            theme.radius_control.min(self.size / 2.0)
        };
        let inset = (self.size - self.mark) / 2.0;
        let draw: Box<dyn Fn(Rgba, f32) -> gpui::AnyElement> = match self.mark_kind {
            TileMark::Icon(icon) => {
                Box::new(move |color, stroke| icon_stroked(icon, self.mark, color, stroke))
            }
            TileMark::Svg(path) => Box::new(move |color, _| {
                gpui::svg()
                    .path(path.clone())
                    .size(px(self.mark))
                    .text_color(color)
                    .into_any_element()
            }),
            TileMark::Initial(initial) => {
                return div()
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(self.size))
                    .rounded(px(radius))
                    .bg(theme.material_face_top)
                    .shadow(material::control_drop(&theme))
                    .text_size(px(10.0))
                    .line_height(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(accent)
                    .child(initial)
                    .child(material::edge(Edge::Strong, radius, &theme));
            }
        };
        let mut tile = div()
            .relative()
            .flex_none()
            .size(px(self.size))
            .rounded(px(radius))
            .bg(theme.material_face_top)
            .shadow(material::control_drop(&theme));
        tile = match self.finish {
            TileFinish::Plain => tile.child(
                div()
                    .absolute()
                    .top(px(inset))
                    .left(px(inset))
                    .child(draw(theme.color_text_secondary, 2.0)),
            ),
            TileFinish::ChromeMark => {
                let mut tile = tile;
                for (start, end, color) in chrome_bands(accent) {
                    let top = start * self.size;
                    let height = (end - start) * self.size;
                    tile = tile.child(
                        div()
                            .absolute()
                            .top(px(top))
                            .left(px(0.0))
                            .w(px(self.size))
                            .h(px(height))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top(px(inset - top))
                                    .left(px(inset))
                                    .child(draw(color, 2.5)),
                            ),
                    );
                }
                tile
            }
        };
        tile.child(material::edge(Edge::Strong, radius, &theme))
    }
}

#[cfg(test)]
mod tests {
    use super::chrome_bands;
    use crate::theme::rgb_f32;

    #[test]
    fn the_bands_cover_the_sheet_in_order() {
        let bands = chrome_bands(rgb_f32(0.66, 0.66, 0.66));
        assert_eq!(bands[0].0, 0.0);
        assert_eq!(bands[4].1, 1.0);
        for pair in bands.windows(2) {
            assert_eq!(pair[0].1, pair[1].0);
        }
        // A grey tint stays grey; the deep band is darker than the light one.
        let (light, deep) = (bands[2].2, bands[3].2);
        assert!((light.r - light.b).abs() < 1e-4);
        assert!(deep.r < light.r);
    }
}
